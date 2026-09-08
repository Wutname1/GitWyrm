//! Group raw items across clients by identity and compute per-destination
//! sync state (task 1.4).
//!
//! Grouping is exact-match on `(kind, identity)` -- see [`RawItem::identity`]
//! -- deliberately not fuzzy, so two genuinely different items are never
//! merged into one inventory row.

use std::collections::BTreeMap;

use super::model::{
    ClientDetection, ClientId, ClientSyncStatus, ConfigScope, InventoryEntry, InventorySummary,
    ItemSource, RawItem, ClientSyncState,
};

/// One logical item's occurrences across every client/location it was found
/// at, keyed by `(kind, identity)`.
struct Group<'a> {
    occurrences: Vec<&'a RawItem>,
}

fn group_key(item: &RawItem) -> (super::model::ItemKind, &str) {
    (item.kind, item.identity.as_str())
}

/// Build the inventory: one row per distinct `(kind, identity)`, with the
/// first-seen occurrence treated as the source (matching the mockup's
/// "source: Codex" / "source: this repository" labels) and every detected
/// client's comparison state against that source.
pub fn build_inventory(items: &[RawItem], detections: &[ClientDetection]) -> Vec<InventoryEntry> {
    let mut groups: BTreeMap<(super::model::ItemKind, &str), Group> = BTreeMap::new();
    for item in items {
        groups.entry(group_key(item)).or_insert_with(|| Group { occurrences: Vec::new() }).occurrences.push(item);
    }

    let mut entries = Vec::new();
    for ((kind, identity), group) in groups {
        // Every group was built by pushing the item currently being
        // iterated (see the loop above), so it always has at least one
        // occurrence; `continue` rather than `expect` keeps this branch
        // provably safe without asserting an invariant at runtime.
        let Some(source_item) = group
            .occurrences
            .iter()
            .min_by_key(|i| (i.location.scope != ConfigScope::Repo, i.location.client as u8))
            .copied()
        else {
            continue;
        };

        let source = if source_item.location.scope == ConfigScope::Repo {
            ItemSource::Repository
        } else {
            ItemSource::Client { client: source_item.location.client }
        };

        let per_client = ClientId::ALL
            .iter()
            .map(|&client| {
                let detection = detections.iter().find(|d| d.client == client);
                let state = client_state(client, source_item, &group.occurrences, detection);
                ClientSyncStatus { client, state }
            })
            .collect();

        let has_secrets = group.occurrences.iter().any(|i| !i.secret_fields.is_empty());

        entries.push(InventoryEntry {
            item_id: format!("{:?}:{}", kind, identity),
            kind,
            display_name: source_item.display_name.clone(),
            scope: source_item.location.scope,
            source,
            per_client,
            has_secrets,
        });
    }

    entries.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    entries
}

fn client_state(
    client: ClientId,
    source_item: &RawItem,
    occurrences: &[&RawItem],
    detection: Option<&ClientDetection>,
) -> ClientSyncState {
    if source_item.location.client == client {
        return ClientSyncState::IsSource;
    }

    let detected = detection.map(|d| d.present).unwrap_or(false);
    let write_supported = detection.map(|d| d.write_supported).unwrap_or(false);

    let existing = occurrences.iter().find(|i| i.location.client == client);

    match existing {
        // This client HAS the item, and that outranks `detected`.
        //
        // `detected` asks whether one of the client's declared config files
        // exists. Holding one of its items means a file belonging to it was
        // read off disk -- direct evidence, and stronger than the question
        // `detected` answers. Saying "not detected on this machine at all"
        // while listing something found on this machine would be the panel
        // contradicting itself.
        //
        // Not a hypothetical gap between the two. Connectors are read from the
        // very files `detected` tests, so for them the two always agree. Skills
        // are not: they are scanned out of `<client>/skills` folders, which are
        // nowhere in the declared config paths. Someone with
        // `~/.claude/skills/foo/SKILL.md` and no `~/.claude/settings.json` has
        // skills to show for a client whose config file is genuinely absent.
        //
        // Deliberately not fixed by widening `detected` to count skill
        // folders. A folder can be left behind by an uninstall, or copied in
        // by hand, and `detected` gates whether this client is offered as a
        // destination for CONNECTORS -- whose config file really would be
        // missing. Treating a stray folder as an installation would trade a
        // visible contradiction for a quiet one.
        Some(item) => {
            if item.extra == source_item.extra {
                ClientSyncState::Same
            } else if !write_supported {
                ClientSyncState::Unsupported
            } else {
                ClientSyncState::Different
            }
        }
        None => {
            if !detected {
                ClientSyncState::ClientNotDetected
            } else if !write_supported {
                ClientSyncState::Unsupported
            } else {
                ClientSyncState::Missing
            }
        }
    }
}

/// Aggregate the summary line counts (mockup: "18 skills found - 11 match -
/// 3 differ - 4 exist in one app") for one item kind.
pub fn summarize(entries: &[InventoryEntry], kind: super::model::ItemKind) -> InventorySummary {
    let relevant: Vec<&InventoryEntry> = entries.iter().filter(|e| e.kind == kind).collect();
    let total = relevant.len() as u32;

    let mut matching = 0u32;
    let mut differing = 0u32;
    let mut exists_in_one = 0u32;

    for entry in &relevant {
        let others: Vec<&ClientSyncStatus> = entry
            .per_client
            .iter()
            .filter(|s| s.state != ClientSyncState::ClientNotDetected)
            .collect();

        let present_elsewhere = others.iter().any(|s| {
            matches!(
                s.state,
                ClientSyncState::Same | ClientSyncState::Different | ClientSyncState::Outdated | ClientSyncState::NeedsSetup
            )
        });

        if !present_elsewhere {
            exists_in_one += 1;
        } else if others.iter().all(|s| matches!(s.state, ClientSyncState::Same | ClientSyncState::IsSource | ClientSyncState::KeptSeparate)) {
            matching += 1;
        } else if others.iter().any(|s| matches!(s.state, ClientSyncState::Different | ClientSyncState::Outdated)) {
            differing += 1;
        }
    }

    InventorySummary {
        total,
        matching,
        differing,
        exists_in_one,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_config::model::{ConfigLocation, ItemKind, JsonValue, RawItem};
    use std::collections::BTreeMap;

    fn item(client: ClientId, scope: ConfigScope, identity: &str, extra_val: &str) -> RawItem {
        let mut extra = BTreeMap::new();
        extra.insert("command".to_string(), JsonValue(serde_json::json!(extra_val)));
        RawItem {
            location: ConfigLocation {
                client,
                scope,
                path: format!("/fake/{identity}"),
            },
            kind: ItemKind::McpConnector,
            identity: identity.to_string(),
            display_name: identity.to_string(),
            description: None,
            extra,
            secret_fields: Vec::new(),
            content_hash: "h".to_string(),
        }
    }

    fn all_detected() -> Vec<ClientDetection> {
        ClientId::ALL
            .iter()
            .map(|&client| ClientDetection {
                client,
                present: true,
                write_supported: matches!(client, ClientId::ClaudeCode | ClientId::OpenCode),
                also_used_by: Vec::new(),
            })
            .collect()
    }

    /// A client GitWyrm has read something from is never reported absent.
    ///
    /// This held by accident and nothing said so. Connectors come out of the
    /// very files detection tests, so for them the two answers always agree
    /// and no test could tell the difference. Skills do not: they are scanned
    /// out of `<client>/skills`, which is nowhere in the declared config
    /// paths, so someone with `~/.claude/skills/foo/SKILL.md` and no
    /// `~/.claude/settings.json` reaches exactly this state -- an item in hand
    /// for a client whose config file is genuinely missing.
    ///
    /// Reporting "not detected on this machine at all" while listing something
    /// found on this machine is the panel contradicting itself.
    #[test]
    fn a_client_with_an_item_is_not_called_absent_even_when_undetected() {
        // Codex sorts first, so it becomes the group's source and Claude
        // Code stays an ordinary peer -- which is the row under test.
        let source = item(ClientId::Codex, ConfigScope::Personal, "fetch", "npx");
        let theirs = item(ClientId::ClaudeCode, ConfigScope::Personal, "fetch", "npx");

        let mut detections = all_detected();
        // No config file for Claude Code -- only a skills folder, which
        // detection does not look at.
        detections
            .iter_mut()
            .find(|d| d.client == ClientId::ClaudeCode)
            .unwrap()
            .present = false;

        let entries = build_inventory(&[source, theirs], &detections);
        let claude = entries[0]
            .per_client
            .iter()
            .find(|s| s.client == ClientId::ClaudeCode)
            .expect("a row for every client");

        assert_ne!(
            claude.state,
            ClientSyncState::ClientNotDetected,
            "GitWyrm read this client's item, so it cannot also say the client is not here"
        );
        assert_eq!(claude.state, ClientSyncState::Same);
    }

    /// The other half of the same rule: with nothing found and no config file,
    /// absent is the honest answer and must stay reachable.
    ///
    /// It gates whether this client is offered as a place to copy a connector
    /// to, so widening it -- counting a stray skills folder as an install, say
    /// -- would offer a destination whose config file really is missing.
    #[test]
    fn a_client_with_nothing_found_and_no_config_file_is_still_absent() {
        let source = item(ClientId::Codex, ConfigScope::Personal, "fetch", "npx");

        let mut detections = all_detected();
        detections
            .iter_mut()
            .find(|d| d.client == ClientId::ClaudeCode)
            .unwrap()
            .present = false;

        let entries = build_inventory(&[source], &detections);
        let claude = entries[0]
            .per_client
            .iter()
            .find(|s| s.client == ClientId::ClaudeCode)
            .expect("a row for every client");

        assert_eq!(claude.state, ClientSyncState::ClientNotDetected);
    }

    #[test]
    fn identical_items_across_two_clients_are_marked_same() {
        let items = vec![
            item(ClientId::ClaudeCode, ConfigScope::Personal, "github", "npx"),
            item(ClientId::OpenCode, ConfigScope::Personal, "github", "npx"),
        ];
        let entries = build_inventory(&items, &all_detected());
        assert_eq!(entries.len(), 1);
        let opencode_state = entries[0]
            .per_client
            .iter()
            .find(|s| s.client == ClientId::OpenCode)
            .unwrap();
        assert_eq!(opencode_state.state, ClientSyncState::Same);
    }

    #[test]
    fn differing_items_are_marked_different_when_writer_is_supported() {
        let items = vec![
            item(ClientId::ClaudeCode, ConfigScope::Personal, "github", "npx"),
            item(ClientId::OpenCode, ConfigScope::Personal, "github", "node"),
        ];
        let entries = build_inventory(&items, &all_detected());
        let opencode_state = entries[0]
            .per_client
            .iter()
            .find(|s| s.client == ClientId::OpenCode)
            .unwrap();
        assert_eq!(opencode_state.state, ClientSyncState::Different);
    }

    #[test]
    fn missing_item_on_a_detected_writable_client_is_missing() {
        let items = vec![item(ClientId::ClaudeCode, ConfigScope::Personal, "github", "npx")];
        let entries = build_inventory(&items, &all_detected());
        let opencode_state = entries[0]
            .per_client
            .iter()
            .find(|s| s.client == ClientId::OpenCode)
            .unwrap();
        assert_eq!(opencode_state.state, ClientSyncState::Missing);
    }

    #[test]
    fn missing_item_on_an_undetected_client_is_client_not_detected() {
        let items = vec![item(ClientId::ClaudeCode, ConfigScope::Personal, "github", "npx")];
        let mut detections = all_detected();
        detections.iter_mut().find(|d| d.client == ClientId::OpenCode).unwrap().present = false;
        let entries = build_inventory(&items, &detections);
        let opencode_state = entries[0]
            .per_client
            .iter()
            .find(|s| s.client == ClientId::OpenCode)
            .unwrap();
        assert_eq!(opencode_state.state, ClientSyncState::ClientNotDetected);
    }

    #[test]
    fn missing_item_on_a_client_with_no_writer_is_unsupported() {
        let items = vec![item(ClientId::ClaudeCode, ConfigScope::Personal, "github", "npx")];
        let entries = build_inventory(&items, &all_detected());
        let codex_state = entries[0]
            .per_client
            .iter()
            .find(|s| s.client == ClientId::Codex)
            .unwrap();
        assert_eq!(codex_state.state, ClientSyncState::Unsupported);
    }

    #[test]
    fn the_source_client_itself_is_marked_is_source() {
        let items = vec![item(ClientId::ClaudeCode, ConfigScope::Personal, "github", "npx")];
        let entries = build_inventory(&items, &all_detected());
        let source_state = entries[0]
            .per_client
            .iter()
            .find(|s| s.client == ClientId::ClaudeCode)
            .unwrap();
        assert_eq!(source_state.state, ClientSyncState::IsSource);
    }

    #[test]
    fn repo_scoped_source_beats_personal_scope() {
        let items = vec![
            item(ClientId::ClaudeCode, ConfigScope::Personal, "gitwyrm-release", "a"),
            item(ClientId::OpenCode, ConfigScope::Repo, "gitwyrm-release", "a"),
        ];
        let entries = build_inventory(&items, &all_detected());
        assert_eq!(entries[0].source, ItemSource::Repository);
    }
}
