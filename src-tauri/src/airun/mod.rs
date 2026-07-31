//! Task runs: the session the console shows, and the driver behind it.
//!
//! Separate from [`crate::ai::agent`], which is the engine that does the work.
//! This layer owns what the *user* sees -- one run per repository, typed events
//! carrying their own plain-language sentence, and the gate answers that come
//! back. The engine is one driver behind the seam; a scripted driver is
//! another, so every console state can be verified before a provider is wired.

pub mod cli_run;
pub mod complete;
pub mod driver;
pub mod engine;
pub mod scripted;
pub mod session;

// Re-exported so callers use `airun::X` rather than reaching through the
// submodules. Unused until the commands land in the same change.
#[allow(unused_imports)]
pub use driver::{GateAnswer, GateRequest, RunDriver, RunEventKind, RunState, RunStep};
#[allow(unused_imports)]
pub use session::{RunSession, SessionRegistry};
