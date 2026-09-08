//! Which agent clients GitWyrm knows about, as data.
//!
//! Every fact that used to be a `match client { ... }` arm lives here as a
//! field on one [`ClientSpec`] row: where the client keeps its personal
//! configuration, whether it has a repository-scoped file at all, which item
//! kinds can be read out of it, whether a merge writer exists, and the JSON
//! key its MCP servers sit under.
//!
//! The point of the table is the same one `ai::agent::registry` makes for
//! command-line tools: the surrounding machinery (locating files, hashing,
//! previewing, backing up, writing, undoing) is not client-specific at all.
//! The lock-in was entirely in the dispatch. So **adding a sixth client is
//! adding a row here**, not editing five match statements that the compiler
//! will happily let drift apart.
//!
//! ## Why the paths are components and not literals
//!
//! A row cannot store a finished path, because the personal path depends on
//! the user's home directory and the repository path depends on which repo is
//! open. Instead each row stores *relative* path components, and
//! [`super::locations`] joins them onto the right root. That keeps the row
//! declarative (a list of names) while leaving the one thing that genuinely
//! varies at runtime outside the table.
//!
//! ## Why writing stays deliberately narrow
//!
//! [`ClientSpec::writer`] is the single gate on whether a client can ever be
//! an apply destination. A row with `writer: None` is read-only, no matter
//! what else it declares -- see [`super::writers::is_supported`], which is
//! now a lookup rather than a hand-maintained `matches!`. Read-only is the
//! default for a reason: writing into a client's settings file when its safe
//! surface has only been guessed at, rather than proven with fixtures, risks
//! corrupting configuration that has nothing to do with GitWyrm.

use super::model::{ClientId, ItemKind};

/// How a client's merge writer builds new file content, for the clients that
/// have one. Modelled as a capability rather than a bare boolean so the row
/// carries the *shape* of the write, not just permission for it: today every
/// supported client keeps its MCP servers under one top-level JSON key, and
/// naming that key is the whole difference between the Claude writer and the
/// OpenCode writer.
///
/// A future client that needs something structurally different (a TOML
/// editor, a nested pointer, a separate file per item) gets a new variant
/// here rather than another arm inside `build_new_content`, so the writer
/// dispatch stays a match over *kinds of writing* instead of a match over
/// clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterKind {
    /// MCP servers live in a map under one top-level key of a JSON document,
    /// merged in place so every byte outside that key survives untouched.
    JsonMcpMap {
        /// The top-level key holding the server map, e.g. `mcpServers`.
        key: &'static str,
    },
    /// The client accepts more than one place for its server map, and the
    /// file decides which. VS Code reads three shapes depending on how it was
    /// configured, so writing a fixed key would create a second map beside
    /// the one already in use and quietly do nothing.
    ///
    /// `candidates` are checked in order against the document; the first one
    /// already present wins, and `default_path` is used only when the file
    /// has none of them.
    JsonMcpMapFirstPresent {
        candidates: &'static [&'static [&'static str]],
        default_path: &'static [&'static str],
    },
    /// MCP servers live in a TOML table, one sub-table per server. Edited
    /// through a real TOML document model so the comments and layout of a
    /// hand-written config survive the write.
    TomlTableMap {
        /// The parent table holding the servers, e.g. `mcp_servers`.
        key: &'static str,
    },
}

/// Everything GitWyrm knows about one agent client, as data.
///
/// Ordering note: [`CLIENTS`] is declared in [`ClientId::ALL`] order, and the
/// inventory's source-selection tie-break sorts by `ClientId as u8`, so the
/// declaration order below is load-bearing for which client is picked as an
/// item's source when the same item exists in several. Reordering rows
/// Which directory a client's personal paths hang off.
///
/// Almost every client here keeps its settings straight under the user's home
/// folder, and did so on every platform -- so the root was simply assumed to
/// be home, and the components in a row were joined onto it.
///
/// VS Code does not. It keeps `Code/User/settings.json` under the per-user
/// application-data directory, which is a different place on each platform:
/// `%APPDATA%` on Windows, `~/Library/Application Support` on macOS, and
/// `~/.config` elsewhere. The row declared the last of those three and it was
/// used everywhere, so on Windows GitWyrm looked for the settings file in a
/// folder VS Code has never written to.
///
/// The effect was not a missing feature but a wrong answer: the client read
/// as *not installed*, its connectors never appeared, and -- since a
/// destination that does not exist yet is created -- copying one to it would
/// have written a brand new settings file somewhere the editor never reads,
/// and reported that as a success.
///
/// Declared per row rather than decided in code, for the reason this whole
/// module exists: a client is a row here, not an arm in a match somewhere
/// else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRoot {
    /// The user's home directory.
    Home,
    /// The per-user application-data directory, which differs by platform.
    AppData,
}

impl PathRoot {
    /// This root on the machine GitWyrm is running on, or `None` when the
    /// environment does not say where it is.
    pub fn resolve(self, home: &std::path::Path) -> Option<std::path::PathBuf> {
        match self {
            PathRoot::Home => Some(home.to_path_buf()),
            PathRoot::AppData => app_data_dir(home),
        }
    }
}

/// The per-user application-data directory.
///
/// Kept beside `PathRoot` rather than in the location code, so the one place
/// that knows this differs by platform is the one place that names it.
fn app_data_dir(home: &std::path::Path) -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        // `%APPDATA%` is the roaming profile, which is where VS Code writes.
        // Falling back to the home-relative shape is deliberate: it keeps a
        // test harness that only overrides the home directory working.
        return std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .or_else(|| Some(home.join("AppData").join("Roaming")));
    }
    #[cfg(target_os = "macos")]
    {
        return Some(home.join("Library").join("Application Support"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| Some(home.join(".config")));
    }
}

/// changes what people see; adding a row at the end does not.
#[derive(Debug, Clone)]
pub struct ClientSpec {
    /// Wire/IPC identity. Kept as the existing enum so the TypeScript
    /// bindings and every frontend column keep working unchanged.
    pub id: ClientId,
    /// Stable string identity, matching the kebab-case wire form of
    /// [`ClientSpec::id`]. This is what gets written into a receipt, so a
    /// client that later outgrows the enum can still be recorded and read
    /// back.
    pub key: &'static str,
    /// Short name shown to people. Plain product names, nothing technical.
    pub display_name: &'static str,
    /// Which directory `personal_paths` hang off. Almost always the user's
    /// home folder; see [`PathRoot`] for the one that is not, and why getting
    /// it wrong reads as "this app is not installed".
    pub personal_root: PathRoot,
    /// Personal configuration files, relative to [`Self::personal_root`].
    /// More than one entry means the client reads several files; they are
    /// searched in the order given.
    pub personal_paths: &'static [&'static [&'static str]],
    /// Repository-scoped configuration files, relative to a repo's working
    /// directory. Empty for a client that has no project-local settings.
    pub repo_paths: &'static [&'static [&'static str]],
    /// Which kinds of item can be read out of this client today. A kind
    /// missing here is simply not discovered, which is different from being
    /// discovered and unwritable. Read through [`ClientSpec::can_read_kind`].
    pub readable_kinds: &'static [ItemKind],
    /// How this client's files are written, or `None` when it is read-only.
    pub writer: Option<WriterKind>,
    /// The text a writer starts from when the destination file does not
    /// exist yet. An empty string for formats where an empty document has no
    /// syntax of its own.
    pub empty_document: &'static str,
    /// Other apps that read this same configuration rather than keeping one
    /// of their own.
    ///
    /// They are deliberately NOT separate rows. A row is a place settings
    /// live, and giving one file two rows makes a single connector look like
    /// two copies that need syncing -- GitWyrm would offer to copy it to a
    /// client that already has it, then write the same file twice. Naming
    /// them here keeps someone who looks for their own app by name from
    /// concluding GitWyrm does not know about it.
    pub also_used_by: &'static [&'static str],
}

impl ClientSpec {
    /// Whether an apply can ever target this client. The single source of
    /// truth for read-only status.
    pub fn can_write(&self) -> bool {
        self.writer.is_some()
    }

    /// Whether this client has any project-local configuration file.
    ///
    /// Not consulted by the location joins (an empty `repo_paths` already
    /// yields no locations); this is for callers that need to ask the
    /// question without building paths, such as explaining to someone why a
    /// client shows no repository row.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn has_repo_scope(&self) -> bool {
        !self.repo_paths.is_empty()
    }

    /// Whether this client's files are parsed for `kind` today.
    ///
    /// Read by the item readers, which decide per client whether to look for
    /// a kind at all rather than parsing every file for everything.
    pub fn can_read_kind(&self, kind: ItemKind) -> bool {
        self.readable_kinds.contains(&kind)
    }
}

/// Every known client, in [`ClientId::ALL`] order.
///
/// Paths and MCP keys below match what each client actually reads on disk;
/// they were taken from the readers that were already parsing these files
/// rather than from documentation, so the table and the parsers cannot
/// disagree.
pub const CLIENTS: &[ClientSpec] = &[
    ClientSpec {
        id: ClientId::Codex,
        key: "codex",
        display_name: "Codex",
        personal_root: PathRoot::Home,
        personal_paths: &[&[".codex", "config.toml"]],
        // Codex has no project-local config file GitWyrm reads today.
        repo_paths: &[],
        // Skills live in `~/.codex/skills`, one folder per skill with a
        // `SKILL.md` inside -- the same shape `skills::read_skills_at`
        // already reads. Confirmed against a real install rather than
        // documentation.
        readable_kinds: &[ItemKind::McpConnector, ItemKind::Skill],
        // TOML, written through `toml_patch` rather than the narrow
        // display-only parser the reader uses, so comments and layout in a
        // hand-written config survive the merge.
        writer: Some(WriterKind::TomlTableMap { key: "mcp_servers" }),
        empty_document: "",
        also_used_by: &[],
    },
    ClientSpec {
        id: ClientId::ClaudeCode,
        key: "claude-code",
        display_name: "Claude",
        personal_root: PathRoot::Home,
        personal_paths: &[&[".claude", "settings.json"], &[".claude.json"]],
        repo_paths: &[&[".claude", "settings.json"]],
        // The only client whose skills folder has been checked against a real
        // install. See `skills::skill_dirs`, which reads this.
        readable_kinds: &[ItemKind::McpConnector, ItemKind::Skill],
        writer: Some(WriterKind::JsonMcpMap { key: "mcpServers" }),
        empty_document: "{}\n",
        also_used_by: &[],
    },
    ClientSpec {
        id: ClientId::OpenCode,
        key: "open-code",
        display_name: "OpenCode",
        // OpenCode reads its configuration in layers, taking the first file
        // that exists at each level rather than one fixed name. Declaring
        // only `opencode.json` meant a person whose config lives in
        // `config.json` -- the default the tool falls back to when none of
        // them exist -- had their connectors read as absent, and a copy
        // aimed at a file their OpenCode does not read.
        //
        // Taken from the resolution order in OpenCode's own config loader
        // rather than from documentation: user paths are tried
        // config.json -> opencode.json -> opencode.jsonc, and project paths
        // opencode.json -> opencode.jsonc -> .opencode/opencode.json ->
        // .opencode/opencode.jsonc.
        personal_root: PathRoot::Home,
        personal_paths: &[
            &[".config", "opencode", "config.json"],
            &[".config", "opencode", "opencode.json"],
            &[".config", "opencode", "opencode.jsonc"],
        ],
        repo_paths: &[
            &["opencode.json"],
            &["opencode.jsonc"],
            &[".opencode", "opencode.json"],
            &[".opencode", "opencode.jsonc"],
        ],
        // Skills sit beside the connectors: OpenCode keeps them in
        // `~/.config/opencode/skills` and `<repo>/.opencode/skills`, one
        // folder per skill with a `SKILL.md` inside -- the same shape
        // `skills::read_skills_at` already reads for Claude. Taken from
        // OpenCode's own skill loader rather than guessed at.
        readable_kinds: &[ItemKind::McpConnector, ItemKind::Skill],
        writer: Some(WriterKind::JsonMcpMap { key: "mcp" }),
        empty_document: "{}\n",
        // OpenChamber is a web interface over OpenCode: it reads these same
        // files and keeps no MCP configuration of its own, so it is named
        // here rather than given a row that would double-count one file.
        also_used_by: &["OpenChamber"],
    },
    ClientSpec {
        id: ClientId::VsCodeCopilot,
        key: "vs-code-copilot",
        display_name: "Copilot",
        // VS Code keeps this under the per-user application-data folder,
        // not the home folder -- `%APPDATA%\Code\User` on Windows. The row
        // used to spell the Linux shape and have it joined onto home
        // everywhere, so on Windows GitWyrm looked somewhere VS Code has
        // never written and reported Copilot as not installed.
        personal_root: PathRoot::AppData,
        personal_paths: &[&["Code", "User", "settings.json"]],
        repo_paths: &[&[".vscode", "settings.json"]],
        readable_kinds: &[ItemKind::McpConnector],
        // The editor's settings file holds far more than agent configuration,
        // so the write is deliberately narrow: one member of whichever server
        // map the file already uses, merged byte-preserving, and nothing else
        // in the document is touched. VS Code reads three shapes, so the
        // writer follows the file rather than picking one.
        writer: Some(WriterKind::JsonMcpMapFirstPresent {
            candidates: &[
                &["servers"],
                &["mcp", "servers"],
                &["github.copilot.chat.mcp.servers"],
                &["mcpServers"],
            ],
            // What a fresh VS Code settings file gets: the modern key the
            // editor documents for MCP servers.
            default_path: &["mcp", "servers"],
        }),
        empty_document: "{}\n",
        also_used_by: &[],
    },
];

/// The row for one client. Every client in [`ClientId::ALL`] has a row (proved
/// by `every_client_id_has_exactly_one_registry_row`), so callers that already
/// hold a `ClientId` can treat this as infallible.
pub fn spec(client: ClientId) -> &'static ClientSpec {
    lookup(client).unwrap_or_else(|| {
        // Unreachable while the test above passes. Falling back to the first
        // row rather than panicking keeps a table edit from taking down a
        // scan, since discovery is meant to degrade rather than fail.
        &CLIENTS[0]
    })
}

fn lookup(client: ClientId) -> Option<&'static ClientSpec> {
    CLIENTS.iter().find(|spec| spec.id == client)
}

/// The row whose [`ClientSpec::key`] matches, for reading back a persisted
/// receipt whose client was recorded as a string. Returns `None` for a key
/// this build does not know, which is the case a string receipt exists to
/// survive: the undo itself still works, only the display name is missing.
#[cfg_attr(not(test), allow(dead_code))]
pub fn spec_by_key(key: &str) -> Option<&'static ClientSpec> {
    CLIENTS.iter().find(|spec| spec.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_client_id_has_exactly_one_registry_row() {
        for &client in &ClientId::ALL {
            let matches = CLIENTS.iter().filter(|spec| spec.id == client).count();
            assert_eq!(matches, 1, "{client:?} should have exactly one row");
        }
        assert_eq!(CLIENTS.len(), ClientId::ALL.len());
    }

    #[test]
    fn registry_rows_are_declared_in_client_id_order() {
        // The inventory picks an item's source by `ClientId as u8`, so a row
        // order that drifts from the enum would quietly change which client
        // wins that tie-break.
        let ids: Vec<ClientId> = CLIENTS.iter().map(|spec| spec.id).collect();
        assert_eq!(ids, ClientId::ALL.to_vec());
    }

    #[test]
    fn every_key_matches_the_wire_form_of_its_client_id() {
        // Receipts persist the key, and the frontend already speaks the
        // kebab-case wire form, so the two must be the same string.
        for spec in CLIENTS {
            let wire = serde_json::to_string(&spec.id).expect("client id serializes");
            let wire = wire.trim_matches('"');
            assert_eq!(spec.key, wire, "{:?} key should match its wire form", spec.id);
        }
    }

    #[test]
    fn keys_are_unique_so_a_receipt_never_resolves_to_two_clients() {
        let mut keys: Vec<&str> = CLIENTS.iter().map(|spec| spec.key).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }

    #[test]
    fn a_key_round_trips_back_to_the_same_row() {
        for spec in CLIENTS {
            assert_eq!(spec_by_key(spec.key).map(|s| s.id), Some(spec.id));
        }
        assert!(spec_by_key("not-a-client").is_none());
    }

    #[test]
    fn exactly_the_json_clients_declare_a_writer() {
        // Guards the write surface: a new row must not silently become an
        // apply destination just by being added to the table. Every client
        // listed here has a writer proven against fixtures of its real file
        // format.
        let writable: Vec<ClientId> = CLIENTS.iter().filter(|s| s.can_write()).map(|s| s.id).collect();
        assert_eq!(
            writable,
            vec![
                ClientId::Codex,
                ClientId::ClaudeCode,
                ClientId::OpenCode,
                ClientId::VsCodeCopilot
            ]
        );
    }

    /// OpenChamber is a web interface over OpenCode and keeps no MCP
    /// configuration of its own. It used to have a row pointing at
    /// `.config/openchamber/config.json`, a file its source never reads: a
    /// copy there wrote a file nothing would load, and the row reported the
    /// client "not detected" on machines that had it, because detection is
    /// driven by these same config paths.
    ///
    /// It is named on OpenCode's row instead of getting one of its own --
    /// two rows over one file would show a single connector as two copies
    /// needing sync, then write the same file twice.
    #[test]
    fn an_app_sharing_another_app_s_config_is_named_not_given_a_row() {
        let opencode = spec(ClientId::OpenCode);
        assert_eq!(opencode.also_used_by, &["OpenChamber"]);

        // And no row claims a config path under an app that has none.
        for spec in CLIENTS {
            for path in spec.personal_paths {
                assert!(
                    !path.contains(&"openchamber"),
                    "{:?} points at a config file OpenChamber does not read",
                    spec.id
                );
            }
        }
    }

    /// Every name in `also_used_by` must be an app with no row of its own.
    /// A name that also has a row would be double-counted rather than
    /// deduplicated, which is the whole thing this field exists to prevent.
    #[test]
    fn a_shared_config_name_never_also_has_its_own_row() {
        for spec in CLIENTS {
            for name in spec.also_used_by {
                assert!(
                    !CLIENTS.iter().any(|other| other.display_name == *name),
                    "{name} is named as sharing config AND has its own row"
                );
            }
        }
    }

    #[test]
    fn every_client_declares_at_least_one_personal_location() {
        for spec in CLIENTS {
            assert!(!spec.personal_paths.is_empty(), "{:?} needs a personal path", spec.id);
        }
    }

    #[test]
    fn only_the_clients_with_a_project_file_declare_repo_paths() {
        let scoped: Vec<ClientId> = CLIENTS.iter().filter(|s| s.has_repo_scope()).map(|s| s.id).collect();
        assert_eq!(
            scoped,
            vec![ClientId::ClaudeCode, ClientId::OpenCode, ClientId::VsCodeCopilot]
        );
    }
}
