//! The runtime registry of live executions: task 2.1's "runtime registry
//! keyed by durable session and execution ID, not repository ID."
//!
//! `commands::airun::DriverRegistry` (the scripted-demo driver) and
//! `commands::airun::gate_answers()` are both keyed by `repo_id`. That is
//! wrong for two reasons the reset audit names directly: a session can
//! outlive the repository window that started it, and a session can carry
//! more than one concurrent execution (a lead plus helpers,
//! `ExecutionRecord::parent_execution_id`) that a single repo-keyed slot
//! cannot address independently. This registry is keyed by
//! `(session_id, execution_id)` instead, so `Stop one` can target exactly one
//! execution and `Stop all` can target every execution that belongs to one
//! session, without touching another session that happens to share the same
//! repository.
//!
//! What it stores is deliberately narrow: only what `stop_execution_at`
//! needs to reach a *live* execution --
//! [`airun::cli_run::CancelHandle`](crate::airun::cli_run::CancelHandle) (the
//! cooperative stop signal `run_task`'s select loop watches) and a
//! completion signal so a caller can wait for the acknowledgement task 2.5
//! requires before persisting `Stopped`. It does not store the join handle
//! or anything about the process itself -- `run_task`/`AcpConnection::shutdown`
//! already own the process's lifetime; this registry only needs a way to
//! reach in and a way to know when it is done.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::airun::cli_run::CancelHandle;

use super::model::{ExecutionId, SessionId};

/// One live execution's cancellation surface, plus a one-shot completion
/// signal.
struct LiveExecution {
    cancel: CancelHandle,
    /// Notified exactly once, when [`ExecutionRegistry::complete`] removes
    /// this entry. A stop caller that wants to know cancellation was
    /// actually acknowledged (task 2.5) awaits this instead of guessing from
    /// a fixed sleep.
    completed: Arc<tokio::sync::Notify>,
}

/// Registry of every execution this process currently believes is live,
/// keyed by `(session_id, execution_id)`.
///
/// Managed as Tauri state (`app.manage(ExecutionRegistry::default())`),
/// alongside (not instead of) `RunSessionLinks`/`SessionLocks` -- this
/// registry has no persistence of its own and answers a different question
/// than either of those: `RunSessionLinks` answers "which durable session is
/// this repository's live run routed into," `SessionLocks` serializes writes
/// to one session's file, and this registry answers "is there something in
/// THIS process I can signal to stop *that specific* execution."
///
/// Lock-ordering: this registry's own mutex is never held while acquiring a
/// `SessionLocks` guard or a `RunSessionLinks` lock, and neither of those is
/// ever acquired while holding this one -- every method here takes its lock,
/// does a pure map operation, and releases it before returning, so there is
/// no scope in which two of these three locks are held at once from this
/// module's own code.
#[derive(Default, Clone)]
pub struct ExecutionRegistry {
    inner: Arc<Mutex<HashMap<(SessionId, ExecutionId), LiveExecution>>>,
}

/// Why [`ExecutionRegistry::stop`] could not reach a live execution to
/// cancel it -- distinct from "it was told to stop" so a caller can tell a
/// genuine no-op (nothing was running) from an acknowledged cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopOutcome {
    /// The execution was live in this process and has been asked to stop.
    /// Does not by itself mean it already stopped -- see
    /// [`ExecutionRegistry::wait_for_stop`].
    Requested,
    /// Nothing in this process is registered for that (session, execution)
    /// pair. Not an error: the execution may have already finished a moment
    /// earlier (bridge.rs's own idempotent handling of a late `Ended`
    /// covers that race), or this process never started it (a session
    /// reopened after a restart -- see `session_recovery`).
    NotLive,
}

impl ExecutionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a freshly-started execution. Must be called BEFORE the
    /// spawned task's first `Working` event is emitted (task 2.2: "Store
    /// cancellation token/process shutdown ownership before emitting
    /// Working") -- a Stop that arrives in the window between "the record
    /// says Preparing/Working" and "this registry knows about it" would
    /// otherwise find [`StopOutcome::NotLive`] and silently do nothing.
    pub fn register(&self, session_id: SessionId, execution_id: ExecutionId, cancel: CancelHandle) {
        self.inner.lock().unwrap().insert(
            (session_id, execution_id),
            LiveExecution {
                cancel,
                completed: Arc::new(tokio::sync::Notify::new()),
            },
        );
    }

    /// Removes an execution's registration and wakes anyone waiting on
    /// [`Self::wait_for_stop`], regardless of whether it ended by finishing,
    /// failing, or being stopped -- an execution that is no longer running
    /// must not stay reachable for a future Stop to (harmlessly, but
    /// confusingly) "succeed" against.
    ///
    /// **Idempotent, not called-once.** This used to claim it ran exactly
    /// once, from the spawned task's own completion path. There are two call
    /// sites: that one, and the watchdog that fires when the task panics
    /// (`commands::agent_desk`). They are mutually exclusive today -- a
    /// panic skips the first -- but nothing enforces that, and a task that is
    /// cancelled rather than panicking would reach both. Calling it twice is
    /// safe: the second call removes nothing and wakes nobody, which is
    /// pinned by a test rather than left as a claim.
    pub fn complete(&self, session_id: &SessionId, execution_id: &ExecutionId) {
        let removed = self
            .inner
            .lock()
            .unwrap()
            .remove(&(session_id.clone(), execution_id.clone()));
        if let Some(live) = removed {
            live.completed.notify_waiters();
        }
    }

    /// Signals one execution to stop. Returns immediately -- this does not
    /// wait for acknowledgement, see [`Self::wait_for_stop`] for that.
    pub fn stop(&self, session_id: &SessionId, execution_id: &ExecutionId) -> StopOutcome {
        let guard = self.inner.lock().unwrap();
        match guard.get(&(session_id.clone(), execution_id.clone())) {
            Some(live) => {
                live.cancel.cancel();
                StopOutcome::Requested
            }
            None => StopOutcome::NotLive,
        }
    }

    /// Every execution ID currently live for `session_id`. Used by `Stop
    /// all` (task 2.4: "Route Stop all to lead and every helper in that
    /// session only") to enumerate exactly the executions this process
    /// actually has something registered for, rather than trusting the
    /// persisted record's state -- the persisted state and this registry can
    /// disagree for a moment around a crash or a race, and this registry is
    /// the one that answers "can I actually signal this."
    pub fn live_executions_for_session(&self, session_id: &SessionId) -> Vec<ExecutionId> {
        self.inner
            .lock()
            .unwrap()
            .keys()
            .filter(|(sid, _)| sid == session_id)
            .map(|(_, eid)| eid.clone())
            .collect()
    }

    /// Whether this process currently has something registered for
    /// `(session_id, execution_id)`. Read-only check for the `started` state
    /// `execution_registry`'s own callers use to decide whether a session
    /// reads as genuinely live right now (see `session_recovery`'s
    /// `execution_is_live` parameter, which this registry is the intended
    /// source of once wired at the reconciliation call site).
    pub fn is_live(&self, session_id: &SessionId, execution_id: &ExecutionId) -> bool {
        self.inner
            .lock()
            .unwrap()
            .contains_key(&(session_id.clone(), execution_id.clone()))
    }

    /// Waits for `(session_id, execution_id)` to be removed via
    /// [`Self::complete`], or for `timeout` to elapse first. Task 2.5: "Do
    /// not persist Stopped until cancellation is acknowledged, or a typed
    /// timeout is shown" -- the caller of `stop_execution_at` uses this
    /// return value to choose which of those two it reports.
    ///
    /// If the execution is already gone by the time this is called (a race
    /// between `stop()` and completion), this returns immediately as
    /// acknowledged -- there is nothing left to wait for, and treating an
    /// already-finished execution as a timeout would be actively misleading.
    pub async fn wait_for_stop(
        &self,
        session_id: &SessionId,
        execution_id: &ExecutionId,
        timeout: std::time::Duration,
    ) -> bool {
        let completed: Arc<tokio::sync::Notify> = {
            let guard = self.inner.lock().unwrap();
            match guard.get(&(session_id.clone(), execution_id.clone())) {
                Some(live) => live.completed.clone(),
                // Already gone -- nothing to wait for, report acknowledged.
                None => return true,
            }
        };
        // `notified()` is created (and immediately pinned) from the CLONED
        // `Arc<Notify>` right after the registry's own lock is released, but
        // BEFORE re-checking liveness -- `Notify`'s guarantee is that a
        // `Notified` future created before `notify_waiters()` runs will
        // still fire even if this task starts polling it afterward. Arming
        // the wait before the liveness re-check (rather than after) is what
        // closes the race: if `complete()` runs anywhere from right after
        // the clone above through the `select!` below, this `notified` was
        // already registered and will still be woken.
        let notified = completed.notified();
        tokio::pin!(notified);
        if !self.is_live(session_id, execution_id) {
            return true;
        }
        tokio::select! {
            _ = &mut notified => true,
            _ = tokio::time::sleep(timeout) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: &str) -> (SessionId, ExecutionId) {
        (format!("sess-{n}"), format!("exec-{n}"))
    }

    #[test]
    fn a_registered_execution_is_live_and_can_be_stopped() {
        let registry = ExecutionRegistry::new();
        let (session_id, execution_id) = ids("1");
        registry.register(session_id.clone(), execution_id.clone(), CancelHandle::new());

        assert!(registry.is_live(&session_id, &execution_id));
        assert_eq!(
            registry.stop(&session_id, &execution_id),
            StopOutcome::Requested
        );
    }

    #[test]
    fn stopping_an_unregistered_execution_is_not_live_not_an_error() {
        let registry = ExecutionRegistry::new();
        let (session_id, execution_id) = ids("ghost");
        assert!(!registry.is_live(&session_id, &execution_id));
        assert_eq!(registry.stop(&session_id, &execution_id), StopOutcome::NotLive);
    }

    #[test]
    fn completing_removes_the_execution() {
        let registry = ExecutionRegistry::new();
        let (session_id, execution_id) = ids("1");
        registry.register(session_id.clone(), execution_id.clone(), CancelHandle::new());
        assert!(registry.is_live(&session_id, &execution_id));

        registry.complete(&session_id, &execution_id);
        assert!(!registry.is_live(&session_id, &execution_id));
        assert_eq!(registry.stop(&session_id, &execution_id), StopOutcome::NotLive);
    }

    #[test]
    fn stop_scope_one_never_touches_a_different_execution_in_the_same_session() {
        let registry = ExecutionRegistry::new();
        let session_id: SessionId = "sess-shared".into();
        let lead: ExecutionId = "exec-lead".into();
        let helper: ExecutionId = "exec-helper".into();
        registry.register(session_id.clone(), lead.clone(), CancelHandle::new());
        registry.register(session_id.clone(), helper.clone(), CancelHandle::new());

        assert_eq!(registry.stop(&session_id, &lead), StopOutcome::Requested);
        // The helper must still be live -- Stop one must never touch a
        // sibling execution in the same session (task 2.3).
        assert!(registry.is_live(&session_id, &helper));
    }

    #[test]
    fn stop_scope_one_never_touches_the_same_execution_id_in_a_different_session() {
        let registry = ExecutionRegistry::new();
        let execution_id: ExecutionId = "exec-1".into();
        let session_a: SessionId = "sess-a".into();
        let session_b: SessionId = "sess-b".into();
        registry.register(session_a.clone(), execution_id.clone(), CancelHandle::new());
        registry.register(session_b.clone(), execution_id.clone(), CancelHandle::new());

        registry.stop(&session_a, &execution_id);
        assert!(
            registry.is_live(&session_b, &execution_id),
            "an execution ID reused across sessions must not cross-cancel"
        );
    }

    #[test]
    fn live_executions_for_session_lists_only_that_sessions_executions() {
        let registry = ExecutionRegistry::new();
        let session_id: SessionId = "sess-1".into();
        let other_session: SessionId = "sess-2".into();
        registry.register(session_id.clone(), "lead".into(), CancelHandle::new());
        registry.register(session_id.clone(), "helper-1".into(), CancelHandle::new());
        registry.register(other_session.clone(), "lead".into(), CancelHandle::new());

        let mut live = registry.live_executions_for_session(&session_id);
        live.sort();
        assert_eq!(live, vec!["helper-1".to_string(), "lead".to_string()]);
    }

    #[tokio::test]
    async fn wait_for_stop_returns_true_once_completed() {
        let registry = ExecutionRegistry::new();
        let (session_id, execution_id) = ids("1");
        registry.register(session_id.clone(), execution_id.clone(), CancelHandle::new());

        let registry_for_complete = registry.clone();
        let session_for_complete = session_id.clone();
        let execution_for_complete = execution_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            registry_for_complete.complete(&session_for_complete, &execution_for_complete);
        });

        let acknowledged = registry
            .wait_for_stop(&session_id, &execution_id, std::time::Duration::from_secs(5))
            .await;
        assert!(acknowledged, "must observe the completion, not time out");
    }

    #[tokio::test]
    async fn wait_for_stop_reports_a_typed_timeout_when_never_completed() {
        let registry = ExecutionRegistry::new();
        let (session_id, execution_id) = ids("1");
        registry.register(session_id.clone(), execution_id.clone(), CancelHandle::new());

        let acknowledged = registry
            .wait_for_stop(&session_id, &execution_id, std::time::Duration::from_millis(30))
            .await;
        assert!(!acknowledged, "an execution that never completes must time out, not hang forever");
    }

    #[tokio::test]
    async fn wait_for_stop_on_an_already_gone_execution_returns_true_immediately() {
        let registry = ExecutionRegistry::new();
        let (session_id, execution_id) = ids("gone");
        // Never registered at all -- nothing to wait for.
        let acknowledged = registry
            .wait_for_stop(&session_id, &execution_id, std::time::Duration::from_secs(5))
            .await;
        assert!(acknowledged);
    }

    // -- Task 2.8: stop during provider silence, tool execution, approval
    // wait, and output streaming. --
    //
    // These four "phases" are indistinguishable from the registry's own
    // point of view -- it only ever sees "signal this execution to stop" and
    // "this execution completed." What actually differs between the four
    // scenarios is what `cli_run::run_task`'s select loop happens to be
    // doing when the `CancelHandle::cancel()` call lands (mid-`prompt`
    // await, inside `handle()`'s `answers.recv()` block, etc.) -- exercised
    // structurally in `cli_run.rs`'s own tests via `handle()` directly, since
    // driving those exact phases end-to-end requires a live CLI subprocess
    // this crate cannot spin up in a unit test. What IS testable here,
    // and is the actual contract every one of the four phases depends on,
    // is that `stop()`/`wait_for_stop()` behave identically regardless of
    // how long the registered execution takes to notice and call
    // `complete()` -- fast (silence/streaming, nothing blocking), slow
    // (a long tool call), or never (a hang) all reduce to the same three
    // cases below.

    /// Simulates "stop during provider silence" (or output streaming) --
    /// the execution notices the cancel signal almost immediately (nothing
    /// was blocking it) and completes right away.
    #[tokio::test]
    async fn stop_signaled_while_idle_or_streaming_acknowledges_immediately() {
        let registry = ExecutionRegistry::new();
        let (session_id, execution_id) = ids("streaming");
        registry.register(session_id.clone(), execution_id.clone(), CancelHandle::new());

        let registry_for_complete = registry.clone();
        let s = session_id.clone();
        let e = execution_id.clone();
        tokio::spawn(async move {
            // No delay: nothing was blocking this execution, so it observes
            // the cancel and completes essentially instantly.
            registry_for_complete.complete(&s, &e);
        });

        assert_eq!(registry.stop(&session_id, &execution_id), StopOutcome::Requested);
        let acknowledged = registry
            .wait_for_stop(&session_id, &execution_id, std::time::Duration::from_secs(5))
            .await;
        assert!(acknowledged);
    }

    /// Simulates "stop during tool execution" -- the execution is in the
    /// middle of something (a tool call) that takes a noticeable but
    /// bounded amount of time to unwind before it can honor the cancel and
    /// complete. Must still be reported as acknowledged, not a timeout, as
    /// long as it finishes within the caller's budget.
    #[tokio::test]
    async fn stop_signaled_mid_tool_execution_still_acknowledges_within_budget() {
        let registry = ExecutionRegistry::new();
        let (session_id, execution_id) = ids("tool-call");
        registry.register(session_id.clone(), execution_id.clone(), CancelHandle::new());

        let registry_for_complete = registry.clone();
        let s = session_id.clone();
        let e = execution_id.clone();
        tokio::spawn(async move {
            // A bounded delay standing in for "finish the in-flight tool
            // call, then honor the cancel."
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            registry_for_complete.complete(&s, &e);
        });

        registry.stop(&session_id, &execution_id);
        let acknowledged = registry
            .wait_for_stop(&session_id, &execution_id, std::time::Duration::from_secs(5))
            .await;
        assert!(acknowledged, "a bounded delay must still count as acknowledged, not a timeout");
    }

    /// Simulates "stop during an approval wait" (`handle`'s
    /// `answers.recv()` block in `cli_run.rs`) -- the execution is blocked
    /// on something that a real user interaction would normally resolve,
    /// but never does in this scenario (the equivalent of the gate being
    /// abandoned). `wait_for_stop` must time out rather than hang forever,
    /// and the execution must still read as live afterward -- the caller
    /// (`stop_execution_at`) is the one that decides to force-stop the
    /// process at that point, this layer only reports the honest state.
    #[tokio::test]
    async fn stop_signaled_while_stuck_at_an_approval_wait_times_out_without_hanging() {
        let registry = ExecutionRegistry::new();
        let (session_id, execution_id) = ids("approval-wait");
        registry.register(session_id.clone(), execution_id.clone(), CancelHandle::new());

        // Nothing ever completes this execution -- the stand-in for a run
        // stuck forever at an abandoned approval gate.
        registry.stop(&session_id, &execution_id);
        let acknowledged = registry
            .wait_for_stop(&session_id, &execution_id, std::time::Duration::from_millis(30))
            .await;
        assert!(!acknowledged, "a run stuck at an unanswered gate must time out, not hang the caller");
        assert!(
            registry.is_live(&session_id, &execution_id),
            "a timed-out wait must not itself remove the registration -- only complete() does that"
        );
    }

    /// Calling `stop()` more than once for the same execution (a duplicate
    /// Stop click landing while the first is still being processed) must
    /// stay a harmless no-op-ish repeat, never a panic or a double-complete.
    #[tokio::test]
    async fn finishing_the_same_execution_twice_is_harmless() {
        // Two call sites reach `complete`: the spawned task's own end, and
        // the watchdog that fires when that task panics. They are mutually
        // exclusive today and nothing enforces it, so the safe behaviour is
        // pinned here rather than asserted in a comment.
        let registry = ExecutionRegistry::new();
        let (session_id, execution_id) = ids("double-complete");
        registry.register(session_id.clone(), execution_id.clone(), CancelHandle::new());

        registry.complete(&session_id, &execution_id);
        assert!(!registry.is_live(&session_id, &execution_id));

        // The second call must not panic, resurrect the entry, or report
        // anything different.
        registry.complete(&session_id, &execution_id);
        assert!(!registry.is_live(&session_id, &execution_id));

        // And a Stop afterwards says plainly that there is nothing running,
        // rather than succeeding against a ghost.
        assert!(matches!(
            registry.stop(&session_id, &execution_id),
            StopOutcome::NotLive
        ));
    }

    #[tokio::test]
    async fn stopping_the_same_execution_twice_is_harmless() {
        let registry = ExecutionRegistry::new();
        let (session_id, execution_id) = ids("double-stop");
        registry.register(session_id.clone(), execution_id.clone(), CancelHandle::new());

        assert_eq!(registry.stop(&session_id, &execution_id), StopOutcome::Requested);
        assert_eq!(registry.stop(&session_id, &execution_id), StopOutcome::Requested);

        registry.complete(&session_id, &execution_id);
        assert_eq!(registry.stop(&session_id, &execution_id), StopOutcome::NotLive);
    }
}
