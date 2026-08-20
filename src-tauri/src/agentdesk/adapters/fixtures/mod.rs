//! Fixture harness (task 1.3): every fixture is generated fresh into a
//! [`tempfile::TempDir`] and never points at a real, live client directory.
//! "Copied anonymized trees" means the *shape* on disk is copied from a real
//! installation (verified against a live one during this change), with every
//! path, name, and message body replaced by synthetic content -- nothing
//! here reads or references an actual user's files.

pub mod claude_code;
pub mod codex;
pub mod opencode;
// No `openchamber` fixture module: OpenChamber's adapter (see
// `adapters::openchamber`) is detection-only pending a real installation to
// verify a schema against, so it has no session data to fixture yet.
pub mod vscode_copilot;
