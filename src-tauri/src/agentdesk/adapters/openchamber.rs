//! OpenChamber adapter.
//!
//! # Status: detection only, not shipped enabled (task 3.5 / Gate 6)
//!
//! OpenChamber was not present on the machine this change was built and
//! verified on (checked `~/.config`, `~/.local/share`, and every path this
//! adapter's `data_dir()` probes below -- nothing exists). Every other
//! adapter in this change was built against a *real* installation's on-disk
//! layout; this one could not be, and design.md's instruction to "reuse
//! OpenCode parsing only where fixtures prove schema compatibility" cannot
//! be honored without a real (or credibly documented) sample to prove
//! compatibility against.
//!
//! Rather than guess a schema and risk either silently importing garbage or
//! claiming a capability that does not work, this adapter implements
//! `detect()` honestly (it looks for a plausible data directory and reports
//! whether one exists) and returns [`AdapterError::ClientNotDetected`]-style
//! honesty everywhere else: `list_sessions`/`read_session` return
//! [`AdapterError::MissingPath`] rather than fabricated data, and
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

const SUPPORTED_RANGE: &str = "not verified against a real installation; adapter stays behind its capability flag";

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

    /// The most plausible data directory given OpenChamber's own naming
    /// (`openchamber`), following the same XDG-on-Unix /
    /// `%APPDATA%`-on-Windows convention every other CLI-style client in
    /// this change uses. Unverified -- see module doc.
    fn data_dir(&self) -> Option<PathBuf> {
        if let Some(h) = &self.home_override {
            return Some(h.clone());
        }
        #[cfg(windows)]
        {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .map(|a| a.join("openchamber"))
        }
        #[cfg(not(windows))]
        {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|h| h.join(".config").join("openchamber"))
        }
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
        // Detected, but never marked supported -- the schema has not been
        // verified, so import must not be offered even when the directory
        // exists. This still lets the detected-clients UI say "found, not
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
        // No verified session location to scan -- returning an empty list
        // would look like "zero sessions found," which is a claim this
        // adapter cannot back up. MissingPath is the honest typed outcome:
        // the expected data is not known to exist at any specific path.
        Err(AdapterError::MissingPath {
            path: client.home_dir.display().to_string(),
        })
    }

    fn read_session(
        &self,
        _client: &DetectedClient,
        external_session_id: &str,
    ) -> Result<ExternalSessionDetail, AdapterError> {
        Err(AdapterError::SessionNotFound {
            external_session_id: external_session_id.into(),
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
    fn list_sessions_is_honest_about_having_no_verified_source_rather_than_claiming_empty() {
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
