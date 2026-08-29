//! Finding the `snip` CLI, and reading the savings it has recorded.
//!
//! `snip` is not a git tool. It wraps a shell command and shrinks that
//! command's output before an AI model ever reads it, so a long build log or a
//! noisy test run costs a fraction of the context it otherwise would. GitWyrm
//! cares about it for one reason: users who run AI agents alongside this app
//! want to know whether the tool is installed and what it has saved them.
//!
//! Discovery and reporting only. GitWyrm never puts `snip` in front of a
//! command it runs. That is deliberate and worth stating plainly, because the
//! obvious "just prefix everything with snip" idea is a trap: on the filtered
//! path `snip` does not connect stdin to the wrapped command, so anything that
//! reads input -- a prompt, a pager, a credential helper -- hangs or fails.
//!
//! Verified against the v0.25.0 sources: `snip --version` prints `snip v0.25.0`
//! (note the literal `v`), there is no `doctor` or `health` subcommand, and
//! `snip gain --json` is the only machine-readable report.

pub mod detect;
pub mod gain;

pub use detect::{detect, SnipState};
pub use gain::{gain, SnipGainOutcome};
