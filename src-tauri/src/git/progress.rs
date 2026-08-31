//! Progress reporting for local (non-network) git operations.
//!
//! Network commands stream git's own stderr as `git-progress` lines. The
//! operations that rewrite the working tree -- merge, discard, checkout -- print
//! nothing, but they are the ones that take the longest: a merge touching 302
//! files ran for ~780ms, and 10,000 files for ~26s. The user saw a frozen window
//! for that whole time with no indication anything was happening.
//!
//! libgit2 calls back per file during a checkout, so real counts are available
//! rather than a spinner. Same event name and payload as the network path, so
//! the frontend has one thing to listen to.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Minimum gap between emitted updates.
///
/// The callback fires once per file, so a large checkout would otherwise post
/// thousands of events and flood the webview -- making the UI *less* responsive,
/// which is the opposite of the point. At this interval a 26s operation sends
/// about 260 updates, which is plenty for a smooth bar.
const EMIT_INTERVAL: Duration = Duration::from_millis(100);

/// One progress update: a human-readable line plus the counts behind it.
pub struct ProgressUpdate<'a> {
    pub repo_id: &'a str,
    pub operation: &'a str,
    pub line: String,
    pub completed: u32,
    pub total: u32,
}

/// Where a [`LocalProgress`] sends its updates.
///
/// A plain callback rather than an `AppHandle` on purpose: this module is under
/// `git/`, which is otherwise free of Tauri types, and linking `tauri` in here
/// broke the lib test harness (STATUS_ENTRYPOINT_NOT_FOUND -- the test binary
/// resolves against the cdylib, which exports no Rust symbols). The command
/// layer owns the emit; this module owns the throttling and the wording.
type Sink = Arc<dyn Fn(ProgressUpdate<'_>) + Send + Sync>;

/// Emits throttled progress for one long-running local operation.
#[derive(Clone)]
pub struct LocalProgress {
    sink: Option<Sink>,
    /// Shared with the command's timing guard so the operation's file count
    /// reaches the perf log and Sentry without a second callback.
    timing: Option<Arc<crate::perf::CommandTiming>>,
    repo_id: String,
    operation: String,
    /// Millis since `started` at the last emit; shared so clones throttle
    /// against each other rather than each keeping its own clock.
    last_emit_ms: Arc<AtomicU64>,
    started: Instant,
}

impl LocalProgress {
    pub fn new(sink: Option<Sink>, repo_id: &str, operation: &str) -> Self {
        Self::with_timing(sink, None, repo_id, operation)
    }

    /// As [`LocalProgress::new`], but also reports the operation's file count to
    /// a timing guard.
    pub fn with_timing(
        sink: Option<Sink>,
        timing: Option<Arc<crate::perf::CommandTiming>>,
        repo_id: &str,
        operation: &str,
    ) -> Self {
        Self {
            sink,
            timing,
            repo_id: repo_id.to_string(),
            operation: operation.to_string(),
            last_emit_ms: Arc::new(AtomicU64::new(0)),
            started: Instant::now(),
        }
    }

    /// Report progress through an operation of `total` steps.
    ///
    /// `total` is 0 before libgit2 knows the size of the work; those calls are
    /// dropped rather than reported as "0 of 0".
    pub fn report(&self, completed: usize, total: usize) {
        if total == 0 {
            return;
        }
        // Record the size of the work even when the update itself is throttled,
        // so the timing report knows how many files this operation covered.
        if let Some(timing) = &self.timing {
            timing.set_scale(total as u64);
        }
        let done = completed >= total;
        if !done && !self.should_emit() {
            return;
        }
        self.emit(format!("{completed} of {total} files"), completed, total);
    }

    /// Announce the start of an operation whose size is not known yet, so the
    /// UI can show something during the scan that precedes the first callback.
    pub fn begin(&self, message: &str) {
        self.emit(message.to_string(), 0, 0);
    }

    /// Whether enough time has passed since the last emit. Always true for the
    /// first call, so an operation reports immediately rather than after the
    /// first interval.
    fn should_emit(&self) -> bool {
        let now = self.started.elapsed().as_millis() as u64;
        let last = self.last_emit_ms.load(Ordering::Relaxed);
        if last != 0 && now.saturating_sub(last) < EMIT_INTERVAL.as_millis() as u64 {
            return false;
        }
        self.last_emit_ms.store(now.max(1), Ordering::Relaxed);
        true
    }

    fn emit(&self, line: String, completed: usize, total: usize) {
        let Some(sink) = &self.sink else { return };
        sink(ProgressUpdate {
            repo_id: &self.repo_id,
            operation: &self.operation,
            line,
            completed: completed as u32,
            total: total as u32,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without a sink the reporter is inert -- the operation still runs. This is
    /// the path every existing test takes.
    #[test]
    fn reporting_without_a_sink_is_a_no_op() {
        let p = LocalProgress::new(None, "repo", "merge");
        p.begin("Merging");
        p.report(1, 10);
        p.report(10, 10);
    }

    /// A total of zero means libgit2 has not sized the work yet; reporting
    /// "0 of 0" would render an empty or NaN progress bar.
    #[test]
    fn an_unknown_total_is_not_reported() {
        let p = LocalProgress::new(None, "repo", "merge");
        p.report(0, 0);
    }

    /// The first update is reported immediately rather than after one interval,
    /// so a slow operation says something straight away.
    #[test]
    fn the_first_update_is_reported_immediately() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = {
            let seen = seen.clone();
            Arc::new(move |u: ProgressUpdate<'_>| {
                seen.lock().unwrap().push((u.completed, u.total, u.line));
            })
        };
        let p = LocalProgress::new(Some(sink), "repo", "merge");
        p.report(3, 300);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "the first update is not throttled");
        assert_eq!(seen[0].0, 3);
        assert_eq!(seen[0].1, 300);
        assert_eq!(seen[0].2, "3 of 300 files");
    }

    /// The throttle must never swallow the final update, or the bar sticks
    /// just short of complete after the work has finished.
    #[test]
    fn the_final_update_is_never_throttled() {
        let p = LocalProgress::new(None, "repo", "discard");
        p.report(1, 100);
        // Immediately after, so the interval has certainly not elapsed.
        assert!(!p.should_emit(), "an intermediate update is throttled");
        p.report(100, 100);
    }
}
