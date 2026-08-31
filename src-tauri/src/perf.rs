//! Backend timing for IPC commands, reported to Sentry as transactions.
//!
//! The frontend already wraps every `invoke` in a span, but that span measures
//! the whole round-trip: dispatch queueing, the work itself, and serialization
//! back. When a burst of commands is issued at once, every one of those spans
//! grows by however long the others took, so a slow *queue* is
//! indistinguishable from slow *work* -- which is exactly the confusion that
//! made a stalled IPC thread look like every git operation being slow at once.
//!
//! A transaction started inside the command body measures only the work, so the
//! gap between the frontend span and this one is the queueing cost, and it
//! becomes possible to say which of the two is actually the problem.

use std::time::Instant;

/// Times a command body for as long as it is held.
///
/// A guard rather than a wrapper function so instrumenting a command is one
/// line at the top of it, with no re-indenting and no change to how the body
/// returns -- including the `?` early returns most commands are built from,
/// which a closure would have to thread a return type through.
///
/// The Sentry transaction is a no-op when Sentry is disabled (dev builds) and
/// is left unsampled when the user has usage telemetry off -- `init_sentry`
/// sets `traces_sample_rate` to 0.0, and an unsampled transaction is never
/// sent. So this is safe to construct unconditionally.
///
/// The slow-command warning below is separate: it goes to the local log, which
/// is on the user's disk and is only ever uploaded if they file a report.
pub struct CommandTiming {
    name: &'static str,
    started: Instant,
    tx: sentry::TransactionOrSpan,
    /// How much work the command turned out to be (files touched), when it
    /// knows. Duration alone cannot separate "slow per file" from "a lot of
    /// files", which is the first question asked of every hang report.
    scale: std::sync::atomic::AtomicU64,
}

impl CommandTiming {
    pub fn start(name: &'static str, op: &'static str) -> Self {
        let tx = sentry::start_transaction(sentry::TransactionContext::new(name, op));
        Self {
            name,
            started: Instant::now(),
            tx: tx.into(),
            scale: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Record how many files this operation covered.
    ///
    /// Cheap enough to call from a progress callback: it is a relaxed store, and
    /// only the final value is read. Callers pass the running total, so the last
    /// call wins.
    pub fn set_scale(&self, files: u64) {
        self.scale
            .store(files, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Drop for CommandTiming {
    fn drop(&mut self) {
        let ms = self.started.elapsed().as_millis();
        let files = self.scale.load(std::sync::atomic::Ordering::Relaxed);
        // 250ms is roughly where an interaction stops feeling immediate. Below
        // that the log would be pure noise, since these run on every screen.
        if ms >= 250 {
            if files > 0 {
                // Per-file cost is the number that makes two reports
                // comparable across repositories of different sizes.
                let per_file = ms as f64 / files as f64;
                log::warn!(
                    "{}: slow, took {ms}ms for {files} files ({per_file:.1}ms/file)",
                    self.name
                );
            } else {
                log::warn!("{}: slow, took {ms}ms", self.name);
            }
        }
        if files > 0 {
            self.tx
                .set_data("files", sentry::protocol::Value::from(files));
        }
        self.tx.clone().finish();
    }
}
