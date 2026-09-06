//! Finding the skills an agent client has installed.
//!
//! # Why this is not in `readers.rs`
//!
//! Every other item GitWyrm reads is a member of a JSON object inside one
//! configuration file. A skill is not: it is a *directory* containing a
//! `SKILL.md`, and the client finds it by scanning a folder rather than by
//! reading a key. That difference is why `ItemKind::Skill` existed for a long
//! time with nothing ever producing one, and why the Skills tab rendered
//! empty -- the JSON readers had nowhere to look.
//!
//! # What a skill is, concretely
//!
//! A folder whose name is the skill's identity, containing `SKILL.md`, which
//! opens with YAML front matter:
//!
//! ```text
//! ---
//! name: cloudflare
//! description: Comprehensive Cloudflare platform skill covering Workers...
//! ---
//! ```
//!
//! Only `name` and `description` are read. Everything else in the front
//! matter -- and the whole body below it -- belongs to the client and is not
//! GitWyrm's to interpret.
//!
//! # Why the front matter is parsed by hand
//!
//! Two fields do not justify a YAML dependency, and a real YAML parser would
//! be *worse* here: it would reject a file over a mistake somewhere else in
//! the front matter, when the two fields this needs were sitting there
//! readable. `readers.rs` made the same call for Codex's TOML. A file this
//! cannot understand contributes nothing rather than failing the scan.
//!
//! The scan is one level deep and does not recurse. A skill's own folder holds
//! references, scripts and assets, and walking into those would report a
//! skill's internals as more skills.

use std::fs;
use std::path::{Path, PathBuf};

use super::model::{ConfigLocation, ExtraFields, ItemKind, JsonValue, RawItem};

/// The file that makes a directory a skill.
const SKILL_FILE: &str = "SKILL.md";

/// Every skill under `dir`, or an empty list when there is no such folder.
///
/// An absent skills directory is the ordinary case for a client that has
/// never installed one, so it is not an error. Neither is a single unreadable
/// skill: it is skipped and the rest are still reported, because one bad
/// folder must not make a person's whole skill list disappear.
pub fn read_skills_at(dir: &Path, location_for: impl Fn(&Path) -> ConfigLocation) -> Vec<RawItem> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut items = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = path.join(SKILL_FILE);
        let Ok(raw) = fs::read(&manifest) else {
            // A directory with no SKILL.md is not a skill. Common: a `.git`
            // folder, an editor's dotfolder, a half-finished draft.
            continue;
        };
        if let Some(item) = build_skill(&path, &manifest, &raw, &location_for) {
            items.push(item);
        }
    }

    // Stable order regardless of how the filesystem hands them back, so the
    // same machine does not reshuffle the list between scans.
    items.sort_by(|a, b| a.identity.cmp(&b.identity));
    items
}

/// One skill, or `None` when the folder does not describe one.
fn build_skill(
    dir: &Path,
    manifest: &Path,
    raw: &[u8],
    location_for: &impl Fn(&Path) -> ConfigLocation,
) -> Option<RawItem> {
    let text = String::from_utf8_lossy(raw);
    let front = front_matter(&text);

    // The folder name is the identity, not the `name:` field. The client
    // finds a skill by its folder, so that is what makes two installs "the
    // same skill" -- a front matter that disagrees with its folder would
    // otherwise split one skill into two rows that never match.
    let identity = dir.file_name()?.to_string_lossy().into_owned();
    if identity.is_empty() {
        return None;
    }

    let mut extra = ExtraFields::new();
    if let Some(name) = front.get("name") {
        extra.insert("name".into(), JsonValue(serde_json::Value::String(name.clone())));
    }
    if let Some(desc) = front.get("description") {
        extra.insert("description".into(), JsonValue(serde_json::Value::String(desc.clone())));
    }

    Some(RawItem {
        // A skill's location is its SKILL.md, never the folder: every other
        // part of this subsystem assumes a location is one file (see
        // `ConfigLocation::path`), and the manifest is what a writer would
        // ever target.
        location: location_for(manifest),
        kind: ItemKind::Skill,
        display_name: front.get("name").cloned().unwrap_or_else(|| identity.clone()),
        description: front.get("description").cloned(),
        // A skill's front matter holds no credentials -- it is a description
        // of what the skill does, not how to reach a service. Anything that
        // did look secret is still caught by the shared scanner below.
        secret_fields: super::redact::find_secret_fields(&extra),
        identity,
        extra,
        content_hash: content_hash(raw),
    })
}

/// The `key: value` pairs in a `---` fenced block at the very top of the file.
///
/// Deliberately shallow: it reads flat scalars and stops at the closing
/// fence. Nested structures (a `references:` list, for instance) are skipped
/// rather than misread, because a wrong value here would be shown to someone
/// as if it were the skill's real description.
fn front_matter(text: &str) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();

    // Front matter is only front matter at the very start. A `---` further
    // down is an ordinary Markdown rule.
    let mut lines = text.lines();
    match lines.next() {
        Some(first) if first.trim() == "---" => {}
        _ => return out,
    }

    for line in lines {
        let trimmed = line.trim_end();
        if trimmed.trim() == "---" {
            break;
        }
        // An indented line continues the value above it, which this does not
        // model. Skipping is right: reporting "- workers" as a field would be
        // worse than reporting nothing.
        if line.starts_with(' ') || line.starts_with('\t') || line.starts_with('-') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        out.insert(key.to_string(), unquote(value).to_string());
    }
    out
}

/// Drops one matching pair of surrounding quotes, if present.
fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &value[1..value.len() - 1];
        }
    }
    value
}

/// Same hash the JSON readers use, so a skill and a connector compare the
/// same way.
///
/// The hex encoding is hand-rolled to match `readers::hex` and `plan::hex`,
/// which are both private to their modules. Three copies of six lines is
/// worse than one shared helper; worth folding together the next time this
/// module is touched.
fn content_hash(raw: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let mut hasher = Sha256::new();
    hasher.update(raw);
    hasher.finalize().iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Where each client keeps its skills, when it has such a folder.
///
/// Returns the directories to scan, not files. A client with no skills
/// concept returns nothing, which is how the scan stays quiet about clients
/// this does not apply to.
pub fn skill_dirs(home: &Path, repo_root: Option<&Path>, client: super::model::ClientId) -> Vec<(PathBuf, super::model::ConfigScope)> {
    use super::model::{ClientId, ConfigScope};

    // Whether a client has skills at all is the registry's answer, not a
    // second list kept here. Two places stating the same fact is exactly what
    // the client table exists to stop.
    if !super::registry::spec(client).can_read_kind(super::model::ItemKind::Skill) {
        return Vec::new();
    }

    let mut out = Vec::new();
    match client {
        // Claude Code reads `~/.claude/skills` and, for work scoped to one
        // project, `<repo>/.claude/skills`.
        ClientId::ClaudeCode => {
            out.push((home.join(".claude").join("skills"), ConfigScope::Personal));
            if let Some(root) = repo_root {
                out.push((root.join(".claude").join("skills"), ConfigScope::Repo));
            }
        }
        // OpenCode reads `~/.config/opencode/skills` and, for work scoped to
        // one project, `<repo>/.opencode/skills`. Note the personal path sits
        // under `.config/opencode` while the project one is a bare
        // `.opencode` -- that asymmetry is OpenCode's own, not a mistake here.
        ClientId::OpenCode => {
            out.push((
                home.join(".config").join("opencode").join("skills"),
                ConfigScope::Personal,
            ));
            if let Some(root) = repo_root {
                out.push((root.join(".opencode").join("skills"), ConfigScope::Repo));
            }
        }
        // Unreachable while these rows do not declare `Skill`. Kept
        // exhaustive so adding that kind to another row fails to compile here
        // rather than silently returning nothing.
        ClientId::Codex | ClientId::VsCodeCopilot => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OpenCode's skill folders, taken from its own loader rather than
    /// guessed: personal under `.config/opencode`, project under a bare
    /// `.opencode`. The asymmetry is OpenCode's own.
    #[test]
    fn opencode_skills_live_where_opencode_looks_for_them() {
        use super::super::model::{ClientId, ConfigScope};
        let home = Path::new("C:/Users/me");
        let repo = Path::new("C:/code/widgets");
        let dirs = skill_dirs(home, Some(repo), ClientId::OpenCode);

        assert_eq!(
            dirs,
            vec![
                (home.join(".config").join("opencode").join("skills"), ConfigScope::Personal),
                (repo.join(".opencode").join("skills"), ConfigScope::Repo),
            ]
        );
    }

    /// Every client the registry says has skills must say where they are.
    /// A row declaring `Skill` with no folders here would scan nothing and
    /// report the person has none, which is the "absent looks like empty"
    /// failure this project keeps finding.
    #[test]
    fn every_client_that_has_skills_says_where_they_are() {
        use super::super::model::{ClientId, ItemKind};
        for client in ClientId::ALL {
            let declares = super::super::registry::spec(client).can_read_kind(ItemKind::Skill);
            let dirs = skill_dirs(Path::new("C:/Users/me"), None, client);
            assert_eq!(
                declares,
                !dirs.is_empty(),
                "{client:?} declares skills={declares} but returned {} folders",
                dirs.len()
            );
        }
    }

    use crate::agent_config::model::{ClientId, ConfigScope};

    fn location_for(path: &Path) -> ConfigLocation {
        ConfigLocation {
            client: ClientId::ClaudeCode,
            scope: ConfigScope::Personal,
            path: path.to_string_lossy().into_owned(),
        }
    }

    fn write_skill(root: &Path, name: &str, body: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).expect("create skill dir");
        fs::write(dir.join(SKILL_FILE), body).expect("write SKILL.md");
    }

    #[test]
    fn a_skill_folder_is_read_as_one_item() {
        let tmp = tempfile::TempDir::new().expect("temp");
        write_skill(
            tmp.path(),
            "cloudflare",
            "---\nname: cloudflare\ndescription: Workers and Pages.\n---\n\nBody.",
        );

        let items = read_skills_at(tmp.path(), location_for);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].identity, "cloudflare");
        assert_eq!(items[0].display_name, "cloudflare");
        assert_eq!(items[0].description.as_deref(), Some("Workers and Pages."));
        assert_eq!(items[0].kind, ItemKind::Skill);
    }

    #[test]
    fn the_folder_name_is_the_identity_even_when_the_front_matter_disagrees() {
        // The client finds a skill by its folder, so that is what makes two
        // installs the same skill. Trusting `name:` would split one skill
        // into two rows that never match across clients.
        let tmp = tempfile::TempDir::new().expect("temp");
        write_skill(tmp.path(), "my-folder", "---\nname: Something Else\n---\n");

        let items = read_skills_at(tmp.path(), location_for);
        assert_eq!(items[0].identity, "my-folder");
        assert_eq!(items[0].display_name, "Something Else");
    }

    #[test]
    fn a_skills_location_is_the_manifest_file_never_the_folder() {
        // Every other part of this subsystem assumes a location is one file.
        let tmp = tempfile::TempDir::new().expect("temp");
        write_skill(tmp.path(), "x", "---\nname: x\n---\n");

        let items = read_skills_at(tmp.path(), location_for);
        assert!(
            items[0].location.path.ends_with("SKILL.md"),
            "got {}",
            items[0].location.path
        );
    }

    #[test]
    fn a_folder_without_a_manifest_is_not_a_skill() {
        // A `.git` folder, an editor dotfolder, a half-finished draft.
        let tmp = tempfile::TempDir::new().expect("temp");
        fs::create_dir_all(tmp.path().join("not-a-skill")).expect("mkdir");
        assert!(read_skills_at(tmp.path(), location_for).is_empty());
    }

    #[test]
    fn one_unreadable_skill_does_not_hide_the_others() {
        let tmp = tempfile::TempDir::new().expect("temp");
        write_skill(tmp.path(), "good-one", "---\nname: good-one\n---\n");
        fs::create_dir_all(tmp.path().join("bad-one")).expect("mkdir");

        let items = read_skills_at(tmp.path(), location_for);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].identity, "good-one");
    }

    #[test]
    fn a_missing_skills_folder_is_empty_rather_than_an_error() {
        let tmp = tempfile::TempDir::new().expect("temp");
        let items = read_skills_at(&tmp.path().join("nope"), location_for);
        assert!(items.is_empty());
    }

    #[test]
    fn skills_come_back_in_a_stable_order() {
        // The filesystem does not promise an order; two scans of one machine
        // reshuffling the list would look like something changed.
        let tmp = tempfile::TempDir::new().expect("temp");
        for name in ["zebra", "apple", "mango"] {
            write_skill(tmp.path(), name, &format!("---\nname: {name}\n---\n"));
        }
        let ids: Vec<String> = read_skills_at(tmp.path(), location_for)
            .into_iter()
            .map(|i| i.identity)
            .collect();
        assert_eq!(ids, vec!["apple", "mango", "zebra"]);
    }

    #[test]
    fn a_nested_list_in_the_front_matter_is_skipped_not_misread() {
        // Real skills carry `references:` lists. Reporting "- workers" as a
        // field value would be worse than reporting nothing.
        let tmp = tempfile::TempDir::new().expect("temp");
        write_skill(
            tmp.path(),
            "cf",
            "---\nname: cf\nreferences:\n  - workers\n  - pages\ndescription: Real one.\n---\n",
        );
        let items = read_skills_at(tmp.path(), location_for);
        assert_eq!(items[0].description.as_deref(), Some("Real one."));
        assert_eq!(items[0].display_name, "cf");
    }

    #[test]
    fn a_file_with_no_front_matter_still_reports_the_skill() {
        // The folder is the skill. A missing description is missing detail,
        // not a missing skill.
        let tmp = tempfile::TempDir::new().expect("temp");
        write_skill(tmp.path(), "bare", "Just a body, no fence.\n");
        let items = read_skills_at(tmp.path(), location_for);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].display_name, "bare");
        assert_eq!(items[0].description, None);
    }

    #[test]
    fn a_dashed_rule_further_down_is_not_front_matter() {
        let tmp = tempfile::TempDir::new().expect("temp");
        write_skill(tmp.path(), "md", "# Title\n\n---\nname: sneaky\n---\n");
        let items = read_skills_at(tmp.path(), location_for);
        assert_eq!(items[0].display_name, "md", "a rule mid-document was read as front matter");
    }

    #[test]
    fn quoted_values_lose_their_quotes() {
        let tmp = tempfile::TempDir::new().expect("temp");
        write_skill(tmp.path(), "q", "---\nname: \"Quoted Name\"\n---\n");
        assert_eq!(read_skills_at(tmp.path(), location_for)[0].display_name, "Quoted Name");
    }

    /// A skill can be seen but never written, on any client.
    ///
    /// Skills are folders of files; the JSON writers can only edit a member
    /// of an object. Offering to copy one would either do nothing or write a
    /// SKILL.md-shaped thing into a settings file. Reading them is the whole
    /// feature for now, and the writer must keep refusing.
    #[test]
    fn no_client_can_be_asked_to_write_a_skill() {
        use crate::agent_config::writers::{build_new_content, WriteContentError};
        for client in ClientId::ALL {
            let err = build_new_content(client, ItemKind::Skill, "x", &ExtraFields::new(), "{}");
            assert!(
                matches!(err, Err(WriteContentError::Unsupported)),
                "{client:?} offered to write a skill"
            );
        }
    }

    #[test]
    fn a_client_gets_a_skills_folder_only_when_the_table_says_it_reads_skills() {
        // The registry is the single source of truth for this. A client whose
        // row does not declare `Skill` gets no folder scanned, so the fact
        // lives in one place rather than being restated here.
        let home = Path::new("/home/someone");
        for client in ClientId::ALL {
            let declared = crate::agent_config::registry::spec(client)
                .can_read_kind(ItemKind::Skill);
            let dirs = skill_dirs(home, None, client);
            assert_eq!(
                !dirs.is_empty(),
                declared,
                "{client:?} disagrees with its registry row about skills"
            );
        }
    }

    #[test]
    fn a_repo_adds_a_second_skills_folder_for_claude_code() {
        let home = Path::new("/home/someone");
        let repo = Path::new("/code/project");
        let dirs = skill_dirs(home, Some(repo), ClientId::ClaudeCode);
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[1].1, ConfigScope::Repo);
    }
}

/// Reads the skills actually installed on this machine and prints them.
///
/// Ignored because it depends on the box. Run deliberately:
/// `cargo test --lib real_skills_on_this_machine -- --ignored --nocapture`
#[cfg(test)]
#[test]
#[ignore]
fn real_skills_on_this_machine() {
    let Some(home) = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
    else {
        println!("no home directory to scan");
        return;
    };
    let dir = home.join(".claude").join("skills");
    let items = read_skills_at(&dir, |p| ConfigLocation {
        client: crate::agent_config::model::ClientId::ClaudeCode,
        scope: crate::agent_config::model::ConfigScope::Personal,
        path: p.to_string_lossy().into_owned(),
    });
    println!("{} skills under {}", items.len(), dir.display());
    for item in &items {
        let desc = item.description.as_deref().unwrap_or("(no description)");
        let short: String = desc.chars().take(60).collect();
        println!("  {:<28} {}", item.identity, short);
    }
}
