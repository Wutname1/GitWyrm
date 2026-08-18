//! Outgoing diagnostics that are not crash reports.
//!
//! Today that is one thing: the install ping, which counts deployed builds.
//! Sentry's own setup lives in `lib::init_sentry` because it has to run before
//! anything else in the process.

pub mod install;
