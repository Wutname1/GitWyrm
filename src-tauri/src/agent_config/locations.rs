//! Minimal, read-only discovery of where each agent client keeps its skill
//! and MCP connector configuration.
//!
//! NOTE (follow-up): the architecture doc calls for reusing adapter detection
//! shared with the external-chat-import feature
//! (`src-tauri/src/commands/agent_import.rs` and its adapter module), which
//! was being built concurrently with this change in a separate worktree/
//! session. To avoid touching those files while both changes were in flight,
//! this module defines its own minimal location discovery instead of
//! depending on that work. The two should be unified in a follow-up once
//! both have landed, so client detection has one implementation instead of
//! two that can drift apart.
//!
//! Every function here only *locates* files; nothing in this module opens a
//! file for writing (task 1.2: "no write capability in scan commands").

use std::path::PathBuf;

use super::model::{ClientDetection, ClientId, ConfigLocation, ConfigScope};

/// The user's home directory, resolved once per call so tests can override it
/// without touching process-global state.
fn home_dir() -> Option<PathBuf> {
    dirs_home()
}

#[cfg(not(test))]
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

#[cfg(test)]
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("GITWYRM_TEST_HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Personal (user-home) configuration locations for each client, plus a
/// repository-scoped location when the client supports one and `repo_root`
/// is given. A location is returned even when nothing exists there yet --
/// presence is reported separately via [`detect_clients`] -- so a preview can
/// still target the conventional path for a client that simply has not
/// written anything there.
pub fn personal_locations(client: ClientId) -> Vec<ConfigLocation> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    match client {
        ClientId::Codex => vec![ConfigLocation {
            client,
            scope: ConfigScope::Personal,
            path: home.join(".codex").join("config.toml").to_string_lossy().into_owned(),
        }],
        ClientId::ClaudeCode => vec![
            ConfigLocation {
                client,
                scope: ConfigScope::Personal,
                path: home.join(".claude").join("settings.json").to_string_lossy().into_owned(),
            },
            ConfigLocation {
                client,
                scope: ConfigScope::Personal,
                path: home
                    .join(".claude.json")
                    .to_string_lossy()
                    .into_owned(),
            },
        ],
        ClientId::OpenCode => vec![ConfigLocation {
            client,
            scope: ConfigScope::Personal,
            path: home
                .join(".config")
                .join("opencode")
                .join("opencode.json")
                .to_string_lossy()
                .into_owned(),
        }],
        ClientId::VsCodeCopilot => vec![ConfigLocation {
            client,
            scope: ConfigScope::Personal,
            path: home
                .join(".config")
                .join("Code")
                .join("User")
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
        }],
        ClientId::OpenChamber => vec![ConfigLocation {
            client,
            scope: ConfigScope::Personal,
            path: home
                .join(".config")
                .join("openchamber")
                .join("config.json")
                .to_string_lossy()
                .into_owned(),
        }],
    }
}

/// Repository-scoped locations, when the client supports project-local
/// configuration. `repo_root` is the working directory of the open repo.
pub fn repo_locations(client: ClientId, repo_root: &str) -> Vec<ConfigLocation> {
    let root = PathBuf::from(repo_root);
    match client {
        ClientId::ClaudeCode => vec![ConfigLocation {
            client,
            scope: ConfigScope::Repo,
            path: root.join(".claude").join("settings.json").to_string_lossy().into_owned(),
        }],
        ClientId::OpenCode => vec![ConfigLocation {
            client,
            scope: ConfigScope::Repo,
            path: root.join("opencode.json").to_string_lossy().into_owned(),
        }],
        ClientId::VsCodeCopilot => vec![ConfigLocation {
            client,
            scope: ConfigScope::Repo,
            path: root
                .join(".vscode")
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
        }],
        ClientId::Codex | ClientId::OpenChamber => Vec::new(),
    }
}

/// Which clients are actually present on this machine: does any known
/// location for them exist? A client with zero existing files is reported as
/// not present even if a writer supports it, so the UI's "Detected apps" tab
/// only lists what is genuinely installed/configured, not every client
/// GitWyrm merely knows how to write to.
pub fn detect_clients(repo_root: Option<&str>) -> Vec<ClientDetection> {
    ClientId::ALL
        .iter()
        .map(|&client| {
            let mut locations = personal_locations(client);
            if let Some(root) = repo_root {
                locations.extend(repo_locations(client, root));
            }
            let present = locations.iter().any(|loc| PathBuf::from(&loc.path).is_file());
            ClientDetection {
                client,
                present,
                write_supported: super::writers::is_supported(client),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    struct HomeGuard {
        _dir: TempDir,
        prev: Option<std::ffi::OsString>,
        /// Held until the guard drops, so the next home-swapping test cannot
        /// start until this one has restored the env var.
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var("GITWYRM_TEST_HOME", v),
                None => std::env::remove_var("GITWYRM_TEST_HOME"),
            }
        }
    }
    /// Serializes the tests that repoint `GITWYRM_TEST_HOME`.
    ///
    /// The env var is process-wide and Rust runs tests as threads in ONE
    /// process, so two of these running at once would each see the other's
    /// home directory -- which is exactly why
    /// `detect_clients_reports_present_only_when_a_file_actually_exists`
    /// failed intermittently under `--test-threads=4` while passing alone.
    /// The lock is held for the guard's whole lifetime, so only one home-
    /// swapping test runs at a time; every other test is unaffected.
    static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_home() -> HomeGuard {
        // A poisoned lock here just means an earlier home test panicked; the
        // env var is still restored by that test's guard, so recovering is
        // correct rather than cascading the failure.
        let lock = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = TempDir::new().expect("temp home");
        let prev = std::env::var_os("GITWYRM_TEST_HOME");
        std::env::set_var("GITWYRM_TEST_HOME", dir.path());
        HomeGuard { _dir: dir, prev, _lock: lock }
    }

    #[test]
    fn personal_locations_are_returned_even_when_nothing_exists_yet() {
        let _guard = with_home();
        let locs = personal_locations(ClientId::ClaudeCode);
        assert!(!locs.is_empty());
        assert!(locs.iter().all(|l| l.scope == ConfigScope::Personal));
    }

    #[test]
    fn detect_clients_reports_present_only_when_a_file_actually_exists() {
        let guard = with_home();
        let before = detect_clients(None);
        let claude = before.iter().find(|d| d.client == ClientId::ClaudeCode).unwrap();
        assert!(!claude.present);

        let locs = personal_locations(ClientId::ClaudeCode);
        let target = PathBuf::from(&locs[0].path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "{}").unwrap();

        let after = detect_clients(None);
        let claude_after = after.iter().find(|d| d.client == ClientId::ClaudeCode).unwrap();
        assert!(claude_after.present);
        drop(guard);
    }

    #[test]
    fn repo_locations_are_only_added_for_clients_that_support_them() {
        let repo = TempDir::new().unwrap();
        let root = repo.path().to_string_lossy().into_owned();
        assert!(repo_locations(ClientId::Codex, &root).is_empty());
        assert!(!repo_locations(ClientId::ClaudeCode, &root).is_empty());
    }
}
