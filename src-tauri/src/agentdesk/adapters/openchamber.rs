//! OpenChamber adapter.
//!
//! # Status: detection only, and file-based import is not possible (task 3.5 / Gate 6)
//!
//! This was long recorded as "blocked on a fixture": OpenChamber had not been
//! installed on the machine the change was built on, so its on-disk session
//! schema could not be proved against a real sample, and design.md's rule to
//! "reuse OpenCode parsing only where fixtures prove schema compatibility"
//! could not be honored. That framing was wrong, and reading OpenChamber's
//! own source settled it.
//!
//! **OpenChamber does not keep session history on disk at all.** It is a
//! frontend over the OpenCode CLI, and its data directory holds configuration
//! only -- `settings.json`, `tunnel-profiles.json`, `tunnel-cli-state.json`
//! and `cloudflare-managed-remote-tunnels.json` are the only files its own
//! `cli-paths.js` names. Every session read goes through an HTTP control API
//! against a *running* server (`requestControlAction(port, 'session.list' |
//! 'session.messages' | 'session.status', ...)` in its
//! `packages/web/bin/lib/commands-session.js`), and the transcripts
//! themselves belong to OpenCode, which already has its own adapter here.
//!
//! So there is no fixture to find and no schema to verify. A future import
//! would have to talk to a live OpenChamber server on a port -- a different
//! shape of adapter from every file-reading one in this module, and a
//! different trust question (a network service rather than a file the user
//! already has). Until that is designed, this adapter stays detection-only,
//! and it stays honest everywhere else: `list_sessions`/`read_session`
//! return [`AdapterError::MissingPath`] rather than fabricated data, and
//! `continuation_capability` returns [`ContinuationCapability::Unsupported`].
//!
//! Per build-order.md Gate 6: "An adapter that fails its gate stays hidden
//! behind its capability flag without blocking the rest of Agent Desk."
//! `commands::agent_import::ADAPTER_CAPABILITY_FLAGS` marks `"openchamber"`
//! disabled by default for exactly this reason -- see that module. This
//! adapter is still registered (so detection telemetry/UI can show "not
//! verified yet" rather than pretending the client does not exist), but no
//! import UI surfaces it while its flag is off.

use std::path::PathBuf;

use super::{
    AdapterError, AgentClientAdapter, AdapterId, ContinuationCapability, DetectedClient,
    ExternalSessionDetail, ExternalSessionSummary,
};

const SUPPORTED_RANGE: &str =
    "no on-disk session history to read; import would need a live server connection, not a file parser";

pub struct OpenChamberAdapter {
    home_override: Option<PathBuf>,
}

impl Default for OpenChamberAdapter {
    fn default() -> Self {
        Self { home_override: None }
    }
}

impl OpenChamberAdapter {
    pub fn at(home: PathBuf) -> Self {
        Self {
            home_override: Some(home),
        }
    }

    /// OpenChamber's data directory, read from its own source rather than
    /// guessed: `packages/web/bin/lib/cli-paths.js` resolves
    /// `OPENCHAMBER_DATA_DIR` first and otherwise uses
    /// `<home>/.config/openchamber` on EVERY platform, Windows included.
    ///
    /// This used to follow the usual `%APPDATA%` convention on Windows,
    /// which is what every other client here does and what OpenChamber
    /// happens not to do, so the probe looked in a folder that never
    /// exists and the adapter reported "not installed" on a machine that
    /// had it.
    fn data_dir(&self) -> Option<PathBuf> {
        if let Some(h) = &self.home_override {
            return Some(h.clone());
        }
        if let Some(explicit) = std::env::var_os("OPENCHAMBER_DATA_DIR") {
            let trimmed = explicit.to_string_lossy().trim().to_string();
            if !trimmed.is_empty() {
                return Some(PathBuf::from(trimmed));
            }
        }
        home_dir().map(|h| h.join(".config").join("openchamber"))
    }
}


/// The user's home directory, the way OpenChamber's own `os.homedir()`
/// resolves it: `USERPROFILE` on Windows, `HOME` elsewhere.
fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

impl AgentClientAdapter for OpenChamberAdapter {
    fn id(&self) -> AdapterId {
        "openchamber"
    }

    fn display_name(&self) -> &'static str {
        "OpenChamber"
    }

    fn supported_version_range(&self) -> &'static str {
        SUPPORTED_RANGE
    }

    fn detect(&self) -> Result<Option<DetectedClient>, AdapterError> {
        let Some(dir) = self.data_dir() else {
            return Ok(None);
        };
        if !dir.is_dir() {
            return Ok(None);
        }
        // Detected, but never marked supported. The directory existing means
        // OpenChamber is installed; it does not mean there is anything here to
        // import, because what this folder holds is settings and tunnel state,
        // not chats. This still lets the detected-clients UI say "found, not
        // yet supported" instead of "not found," which is the honest
        // distinction to draw.
        Ok(Some(DetectedClient {
            home_dir: dir,
            version: None,
            supported: false,
        }))
    }

    fn list_sessions(
        &self,
        client: &DetectedClient,
    ) -> Result<Vec<ExternalSessionSummary>, AdapterError> {
        // There is no session file anywhere to scan: OpenChamber serves its
        // sessions from a running server, and the transcripts belong to
        // OpenCode. Returning an empty list would read as "zero sessions
        // found", a measurement this adapter never took. MissingPath is the
        // honest typed outcome: the expected data is not at any path.
        Err(AdapterError::MissingPath {
            path: client.home_dir.display().to_string(),
        })
    }

    fn read_session(
        &self,
        client: &DetectedClient,
        _external_session_id: &str,
    ) -> Result<ExternalSessionDetail, AdapterError> {
        // Same reasoning as `list_sessions` above, which this did not follow.
        // `SessionNotFound` says "that chat is no longer in the other tool" --
        // an assertion about a folder this adapter has never opened. The module
        // doc has always promised both return `MissingPath` "rather than
        // fabricated data"; only one of them did.
        Err(AdapterError::MissingPath {
            path: client.home_dir.display().to_string(),
        })
    }

    fn continuation_capability(
        &self,
        _client: &DetectedClient,
        _external_session_id: &str,
    ) -> ContinuationCapability {
        ContinuationCapability::Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read from OpenChamber's own `cli-paths.js`: the env override wins,
    /// and otherwise it is `<home>/.config/openchamber` on every platform.
    /// A `%APPDATA%` guess pointed at a folder OpenChamber never creates,
    /// so the adapter reported "not installed" on machines that had it.
    #[test]
    fn the_data_directory_follows_openchambers_own_rule() {
        let adapter = OpenChamberAdapter::default();
        // The override is absolute, and trimmed: a stray space in an
        // environment variable should not produce a relative path.
        // Set directly: this crate has no env-scoping test helper, and the
        // variable is restored before the assertions that must not see it.
        let previous = std::env::var_os("OPENCHAMBER_DATA_DIR");
        std::env::set_var("OPENCHAMBER_DATA_DIR", "  C:/oc-data  ");
        let overridden = adapter.data_dir();
        match &previous {
            Some(v) => std::env::set_var("OPENCHAMBER_DATA_DIR", v),
            None => std::env::remove_var("OPENCHAMBER_DATA_DIR"),
        }
        assert_eq!(
            overridden,
            Some(PathBuf::from("C:/oc-data")),
            "the documented override must win, trimmed"
        );

        if previous.is_none() {
            let dir = adapter.data_dir().expect("a home directory");
            let tail: Vec<_> = dir
                .components()
                .rev()
                .take(2)
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            assert_eq!(tail, vec!["openchamber".to_string(), ".config".to_string()]);
        }
    }

    #[test]
    fn detects_a_present_directory_but_never_marks_it_supported() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path()).unwrap();
        let adapter = OpenChamberAdapter::at(dir.path().to_path_buf());
        let detected = adapter.detect().unwrap().expect("directory exists");
        assert!(
            !detected.supported,
            "must never claim support without a verified schema"
        );
    }

    #[test]
    fn missing_directory_is_not_detected() {
        let dir = tempfile::TempDir::new().unwrap();
        let adapter = OpenChamberAdapter::at(dir.path().join("does-not-exist"));
        assert!(adapter.detect().unwrap().is_none());
    }

    #[test]
    fn list_sessions_is_honest_about_having_no_source_rather_than_claiming_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let adapter = OpenChamberAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        assert!(matches!(
            adapter.list_sessions(&client),
            Err(AdapterError::MissingPath { .. })
        ));
    }

    #[test]
    fn continuation_is_unsupported() {
        let dir = tempfile::TempDir::new().unwrap();
        let adapter = OpenChamberAdapter::at(dir.path().to_path_buf());
        let client = adapter.detect().unwrap().unwrap();
        assert_eq!(
            adapter.continuation_capability(&client, "any"),
            ContinuationCapability::Unsupported
        );
    }
}
