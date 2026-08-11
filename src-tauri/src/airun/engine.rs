//! How a run reports what it is doing.
//!
//! The bridge that used to live here mapped our own engine's vocabulary onto
//! the console's. The provider CLI runs the loop now, so `cli_run` speaks the
//! console's vocabulary directly and only the sink remains.

use std::sync::Arc;

use super::driver::{RunState, RunStep};

/// What the host does with each event it produces.
///
/// A callback rather than a channel so the caller decides whether to emit,
/// record, or both -- the Tauri layer needs all three and the tests need none.
pub type Sink = Arc<dyn Fn(RunState, RunStep) + Send + Sync>;
