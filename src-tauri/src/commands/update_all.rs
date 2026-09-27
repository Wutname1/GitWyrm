//! "Get latest" for many branches and many repositories at once.
//!
//! Two entry points share one engine:
//!
//! - [`pull_branches`] updates several branches of one open repository with a
//!   single fetch (the Branch Manager).
//! - [`update_all_start`] runs the same thing over every repository in the code
//!   folders plus any extra paths, as a background job that reports progress
//!   with events and keeps its last report for the results view.
//!
//! The rules are the same everywhere: fetch every remote once, then move each
//! local branch that is only behind its upstream. Nothing is ever merged or
//! rebased here. A branch that has its own commits too, one checked out in
//! another worktree, or a checked-out branch with uncommitted changes (unless
//! the caller allowed setting them aside) is left alone and reported.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use git2::{BranchType, Repository, RepositoryState};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::branch::fast_forward_branch_to;
use crate::commands::remote::fetch_all_at;
use crate::error::AppError;
use crate::git::refs;
use crate::git::shell::Attended;
use crate::state::{repo_id_for, RepoManager};

/// How many repositories update at the same time. Fetches are network-bound,
/// so a few in flight hides latency; more than this mostly contends for disk
/// and bandwidth and makes the progress toast jump around.
const PARALLEL_REPOS: usize = 4;

/// What happened to one branch.
#[derive(Debug, Clone, Copy, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BranchUpdateKind {
    /// Moved forward to match the server.
    Updated,
    /// Checked out with uncommitted changes, and setting them aside was not
    /// allowed for this repository. Left where it was.
    ChangesInTheWay,
    /// Moved forward, but the changes set aside for it did not go back cleanly.
    /// They are kept in a stash and the working tree needs attention.
    ChangesClashed,
    /// Has its own commits as well as new ones on the server. Combining them is
    /// a merge or rebase, which is never done in bulk.
    BothChanged,
    /// Checked out in another worktree, which would have to move with it.
    OpenElsewhere,
    /// Its copy on the server was deleted.
    ServerCopyGone,
    /// Something else went wrong; see the message.
    Failed,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct BranchUpdate {
    pub name: String,
    pub kind: BranchUpdateKind,
    /// Commits it received, or for a branch left alone, how many were waiting.
    pub commits: u32,
    pub message: Option<String>,
}

/// The worst thing that happened in one repository, used to order results.
#[derive(Debug, Clone, Copy, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepoUpdateLevel {
    Error,
    Warning,
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct RepoUpdate {
    pub name: String,
    pub path: String,
    pub level: RepoUpdateLevel,
    /// A problem with the repository as a whole (could not fetch, mid-merge).
    pub message: Option<String>,
    /// The fetch failed for want of a sign-in. A retry that may prompt can fix it.
    pub needs_sign_in: bool,
    /// The checked-out branch was skipped because of uncommitted changes. A
    /// retry that is allowed to set them aside can fix it.
    pub changes_in_the_way: bool,
    /// Total commits received across every branch that moved.
    pub commits_received: u32,
    /// Branches worth mentioning: moved, or left alone for a reason.
    pub branches: Vec<BranchUpdate>,
    /// Branches that already matched the server.
    pub up_to_date: u32,
    /// Never started because the job was stopped first.
    pub skipped: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct UpdateAllReport {
    pub job: u32,
    /// Seconds since the epoch.
    pub started_at: f64,
    pub finished_at: f64,
    pub cancelled: bool,
    pub repos: Vec<RepoUpdate>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct UpdateAllProgress {
    pub job: u32,
    pub total: u32,
    pub done: u32,
    /// Paths of the repositories being worked on right now.
    pub running: Vec<String>,
    pub branches_updated: u32,
    pub commits_received: u32,
    pub errors: u32,
    pub warnings: u32,
    pub stopping: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct UpdateAllState {
    pub running: Option<UpdateAllProgress>,
    pub last: Option<UpdateAllReport>,
}

#[derive(Debug, Clone, Deserialize, Type)]
pub struct UpdateAllRequest {
    /// Code folders; every repository directly inside each one is included.
    pub folders: Vec<String>,
    /// Individual repositories to include as well (open tabs, or a retry list).
    pub paths: Vec<String>,
    /// Repositories whose checked-out branch may have its uncommitted changes
    /// set aside and put back while it moves.
    pub allow_set_aside: Vec<String>,
    /// Let git show a sign-in window. Runs one repository at a time so windows
    /// never stack up.
    pub sign_in: bool,
}

/// The single update job, if one is running, and the last finished report.
#[derive(Default)]
pub struct UpdateAllJobs {
    inner: Mutex<JobsInner>,
}

#[derive(Default)]
struct JobsInner {
    next_id: u32,
    running: Option<UpdateAllProgress>,
    cancel: Option<Arc<AtomicBool>>,
    last: Option<UpdateAllReport>,
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Separator-agnostic form of a path, for de-duplication. Case-insensitive on
/// Windows only: elsewhere two folders can differ by case alone.
fn path_key(path: &str) -> String {
    let key = path.replace('\\', "/").trim_end_matches('/').to_string();
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        key
    }
}

fn folder_name(path: &str) -> String {
    Path::new(path.trim_end_matches(['/', '\\']))
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Branches checked out anywhere other than this checkout: in a linked
/// worktree, or in the main checkout when this one is itself a worktree.
/// Moving one of those would leave that folder's files behind its branch.
fn checked_out_elsewhere(repo: &Repository) -> HashSet<String> {
    let mut names = HashSet::new();
    let head_of = |r: &Repository| {
        r.head()
            .ok()
            .filter(|h| h.is_branch())
            .and_then(|h| h.shorthand().ok().map(str::to_string))
    };

    if let Ok(list) = repo.worktrees() {
        for name in list.iter().flatten().flatten() {
            let Ok(wt) = repo.find_worktree(name) else {
                continue;
            };
            let Ok(other) = Repository::open_from_worktree(&wt) else {
                continue;
            };
            if other.workdir() == repo.workdir() {
                continue;
            }
            if let Some(branch) = head_of(&other) {
                names.insert(branch);
            }
        }
    }
    if repo.is_worktree() {
        if let Ok(main) = Repository::open(repo.commondir()) {
            if let Some(branch) = head_of(&main) {
                names.insert(branch);
            }
        }
    }
    names
}

/// The result of moving a repository's branches, before it is folded into a
/// [`RepoUpdate`].
#[derive(Debug, Default)]
pub(crate) struct BranchesOutcome {
    pub branches: Vec<BranchUpdate>,
    pub up_to_date: u32,
}

/// Move every local branch that is only behind its upstream, using the
/// remote-tracking refs already on disk. Does not fetch.
///
/// `only` limits the pass to the named branches. `allow_set_aside` lets the
/// checked-out branch move over uncommitted changes by stashing them and
/// putting them back.
pub(crate) fn update_local_branches(
    repo: &mut Repository,
    repo_path: &str,
    only: Option<&[String]>,
    allow_set_aside: bool,
) -> BranchesOutcome {
    let mut out = BranchesOutcome::default();

    let head_name = repo
        .head()
        .ok()
        .filter(|h| h.is_branch())
        .and_then(|h| h.shorthand().ok().map(str::to_string));
    let elsewhere = checked_out_elsewhere(repo);

    let mut names: Vec<String> = match repo.branches(Some(BranchType::Local)) {
        Ok(branches) => branches
            .flatten()
            .filter_map(|(b, _)| b.name().ok().flatten().map(str::to_string))
            .collect(),
        Err(_) => Vec::new(),
    };
    if let Some(only) = only {
        names.retain(|n| only.iter().any(|o| o == n));
    }
    names.sort_by_key(|n| n.to_lowercase());

    for name in names {
        let Some((local_oid, upstream_name, upstream_oid)) = (|| {
            let branch = repo.find_branch(&name, BranchType::Local).ok()?;
            let local = branch.get().target()?;
            match branch.upstream() {
                Ok(up) => {
                    let up_name = up.name().ok().flatten()?.to_string();
                    let up_oid = up.get().target()?;
                    Some((local, up_name, Some(up_oid)))
                }
                Err(_) => {
                    // A configured upstream whose ref was pruned: the branch
                    // was published once and its server copy is gone.
                    let configured = repo
                        .config()
                        .ok()
                        .and_then(|c| c.get_string(&format!("branch.{name}.merge")).ok())
                        .is_some();
                    configured.then(|| (local, String::new(), None))
                }
            }
        })() else {
            // Never published: nothing to get.
            continue;
        };

        let Some(upstream_oid) = upstream_oid else {
            out.branches.push(BranchUpdate {
                name,
                kind: BranchUpdateKind::ServerCopyGone,
                commits: 0,
                message: None,
            });
            continue;
        };

        let (ahead, behind) = match repo.graph_ahead_behind(local_oid, upstream_oid) {
            Ok((a, b)) => (a as u32, b as u32),
            Err(e) => {
                out.branches.push(BranchUpdate {
                    name,
                    kind: BranchUpdateKind::Failed,
                    commits: 0,
                    message: Some(e.message().to_string()),
                });
                continue;
            }
        };

        if behind == 0 {
            out.up_to_date += 1;
            continue;
        }
        if ahead > 0 {
            out.branches.push(BranchUpdate {
                name,
                kind: BranchUpdateKind::BothChanged,
                commits: behind,
                message: None,
            });
            continue;
        }
        if elsewhere.contains(&name) {
            out.branches.push(BranchUpdate {
                name,
                kind: BranchUpdateKind::OpenElsewhere,
                commits: behind,
                message: None,
            });
            continue;
        }

        let is_head = head_name.as_deref() == Some(name.as_str());
        if is_head && !allow_set_aside {
            match refs::tracked_changes_present(repo) {
                Ok(false) => {}
                Ok(true) => {
                    out.branches.push(BranchUpdate {
                        name,
                        kind: BranchUpdateKind::ChangesInTheWay,
                        commits: behind,
                        message: None,
                    });
                    continue;
                }
                Err(e) => {
                    out.branches.push(BranchUpdate {
                        name,
                        kind: BranchUpdateKind::Failed,
                        commits: 0,
                        message: Some(e.to_string()),
                    });
                    continue;
                }
            }
        }

        let update = match fast_forward_branch_to(repo, repo_path, &name, &upstream_name) {
            Ok(moved) if moved.stashed => BranchUpdate {
                name,
                kind: BranchUpdateKind::ChangesClashed,
                commits: behind,
                message: None,
            },
            Ok(_) => BranchUpdate {
                name,
                kind: BranchUpdateKind::Updated,
                commits: behind,
                message: None,
            },
            Err(e) => {
                let raw = e.to_string();
                let message = if raw.contains("prevents checkout") {
                    "Some files on this computer are in the way of the new version. Open the project to update this branch."
                        .to_string()
                } else {
                    raw
                };
                BranchUpdate {
                    name,
                    kind: BranchUpdateKind::Failed,
                    commits: 0,
                    message: Some(message),
                }
            }
        };
        out.branches.push(update);
    }
    out
}

fn level_for(message: bool, branches: &[BranchUpdate]) -> RepoUpdateLevel {
    use BranchUpdateKind as K;
    if message
        || branches
            .iter()
            .any(|b| matches!(b.kind, K::ChangesClashed | K::Failed))
    {
        RepoUpdateLevel::Error
    } else if branches.iter().any(|b| {
        matches!(
            b.kind,
            K::ChangesInTheWay | K::BothChanged | K::OpenElsewhere
        )
    }) {
        RepoUpdateLevel::Warning
    } else if branches.iter().any(|b| b.kind == K::Updated) {
        RepoUpdateLevel::Updated
    } else {
        RepoUpdateLevel::Unchanged
    }
}

fn blank_report(name: String, path: String) -> RepoUpdate {
    RepoUpdate {
        name,
        path,
        level: RepoUpdateLevel::Unchanged,
        message: None,
        needs_sign_in: false,
        changes_in_the_way: false,
        commits_received: 0,
        branches: Vec::new(),
        up_to_date: 0,
        skipped: false,
    }
}

fn finish_report(mut report: RepoUpdate, outcome: BranchesOutcome) -> RepoUpdate {
    report.commits_received = outcome
        .branches
        .iter()
        .filter(|b| {
            matches!(
                b.kind,
                BranchUpdateKind::Updated | BranchUpdateKind::ChangesClashed
            )
        })
        .map(|b| b.commits)
        .sum();
    report.changes_in_the_way = outcome
        .branches
        .iter()
        .any(|b| b.kind == BranchUpdateKind::ChangesInTheWay);
    report.up_to_date = outcome.up_to_date;
    report.branches = outcome.branches;
    report.level = level_for(report.message.is_some(), &report.branches);
    report
}

/// Fetch and update one repository. Uses the open tab's handle when the
/// repository is open, so the work queues behind (and never races) whatever
/// the user is doing in that tab.
fn update_one(
    manager: &RepoManager,
    path: &str,
    only: Option<&[String]>,
    allow_set_aside: bool,
    attended: Attended,
    cancel: Option<&AtomicBool>,
) -> RepoUpdate {
    let mut report = blank_report(folder_name(path), path.to_string());

    let fresh = match Repository::open(path) {
        Ok(repo) => repo,
        Err(_) => {
            report.message = Some("This folder is not a git project anymore.".into());
            report.level = RepoUpdateLevel::Error;
            return report;
        }
    };
    let Some(workdir) = fresh.workdir().map(Path::to_path_buf) else {
        report.message =
            Some("This project has no working folder, so there is nothing to update.".into());
        report.level = RepoUpdateLevel::Error;
        return report;
    };
    if fresh.remotes().map(|r| r.len() == 0).unwrap_or(true) {
        // Only on this computer: nothing to get, and not a problem.
        return report;
    }
    if fresh.state() != RepositoryState::Clean {
        report.message =
            Some("Is in the middle of combining changes. Open it to finish that first.".into());
        report.level = RepoUpdateLevel::Warning;
        return report;
    }

    if let Err(failure) = fetch_all_at(path, attended, cancel) {
        if failure.stopped {
            report.skipped = true;
            return report;
        }
        report.needs_sign_in = failure.needs_sign_in;
        report.message = Some(failure.message);
        report.level = RepoUpdateLevel::Error;
        return report;
    }

    let outcome = match manager.get(&repo_id_for(&workdir)) {
        Ok(open) => {
            drop(fresh);
            let mut repo = open.repo.lock().unwrap();
            update_local_branches(&mut repo, path, only, allow_set_aside)
        }
        Err(_) => {
            let mut repo = fresh;
            update_local_branches(&mut repo, path, only, allow_set_aside)
        }
    };
    finish_report(report, outcome)
}

/// Update several branches of one open repository with a single fetch.
///
/// `branches` of `None` means every branch that tracks a remote. The user is
/// looking at this repository, so the checked-out branch may have its changes
/// set aside and put back, the same as a plain pull.
#[tauri::command]
#[specta::specta]
pub async fn pull_branches(
    manager: State<'_, RepoManager>,
    repo_id: String,
    branches: Option<Vec<String>>,
) -> Result<RepoUpdate, AppError> {
    let open = manager.get(&repo_id)?;
    let path = open.path.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let _timing = crate::perf::CommandTiming::start("pull_branches", "git.pull_branches");
        let mut report = blank_report(folder_name(&path), path.clone());
        if let Err(failure) = fetch_all_at(&path, Attended::User, None) {
            report.needs_sign_in = failure.needs_sign_in;
            report.message = Some(failure.message);
            report.level = RepoUpdateLevel::Error;
            return Ok(report);
        }
        let mut repo = open.repo.lock().unwrap();
        let outcome = update_local_branches(&mut repo, &path, branches.as_deref(), true);
        Ok(finish_report(report, outcome))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Every repository the request names, found and de-duplicated.
fn gather_targets(request: &UpdateAllRequest) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut targets = Vec::new();
    let mut push = |path: String| {
        if seen.insert(path_key(&path)) {
            targets.push(path);
        }
    };
    for folder in &request.folders {
        match crate::commands::scan::scan_folder(Path::new(folder)) {
            Ok(repos) => repos.into_iter().for_each(|r| push(r.path)),
            Err(e) => log::warn!("update-all: could not scan {folder}: {e}"),
        }
    }
    for path in &request.paths {
        push(path.clone());
    }
    targets
}

fn sort_results(repos: &mut [RepoUpdate]) {
    let rank = |l: RepoUpdateLevel| match l {
        RepoUpdateLevel::Error => 0,
        RepoUpdateLevel::Warning => 1,
        RepoUpdateLevel::Updated => 2,
        RepoUpdateLevel::Unchanged => 3,
    };
    repos.sort_by(|a, b| {
        rank(a.level)
            .cmp(&rank(b.level))
            .then_with(|| a.skipped.cmp(&b.skipped))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// Start updating every repository the request names, in the background.
///
/// Returns at once with the job number. Progress arrives as
/// `update-all-progress` events and the final report as `update-all-finished`;
/// [`update_all_state`] answers the same questions for a window that was not
/// listening. Only one job runs at a time.
#[tauri::command]
#[specta::specta]
pub async fn update_all_start(
    app: AppHandle,
    jobs: State<'_, UpdateAllJobs>,
    request: UpdateAllRequest,
) -> Result<u32, AppError> {
    let cancel = Arc::new(AtomicBool::new(false));
    let job = {
        let mut inner = jobs.inner.lock().unwrap();
        if inner.running.is_some() {
            return Err(AppError::Other(
                "Already getting the latest. Wait for it to finish, or stop it first.".into(),
            ));
        }
        inner.next_id += 1;
        let job = inner.next_id;
        inner.running = Some(UpdateAllProgress {
            job,
            total: 0,
            done: 0,
            running: Vec::new(),
            branches_updated: 0,
            commits_received: 0,
            errors: 0,
            warnings: 0,
            stopping: false,
        });
        inner.cancel = Some(cancel.clone());
        job
    };

    // A plain thread, not the async runtime's blocking pool: this can run for
    // minutes, and the pool is shared with every other command.
    std::thread::Builder::new()
        .name("update-all".into())
        .spawn(move || run_job(app, job, request, cancel))
        .map_err(AppError::Io)?;
    Ok(job)
}

fn emit_progress(app: &AppHandle, progress: &UpdateAllProgress) {
    let _ = app.emit("update-all-progress", progress.clone());
}

fn run_job(app: AppHandle, job: u32, request: UpdateAllRequest, cancel: Arc<AtomicBool>) {
    let started_at = now_secs();
    let targets = gather_targets(&request);
    let allow: HashSet<String> = request
        .allow_set_aside
        .iter()
        .map(|p| path_key(p))
        .collect();
    let attended = if request.sign_in {
        Attended::User
    } else {
        Attended::Background
    };
    let workers = if request.sign_in { 1 } else { PARALLEL_REPOS };
    log::info!(
        "update-all job {job}: {} repositories, sign_in={}",
        targets.len(),
        request.sign_in
    );

    let jobs = app.state::<UpdateAllJobs>();
    let manager = app.state::<RepoManager>();
    let update_progress = |f: &dyn Fn(&mut UpdateAllProgress)| {
        let snapshot = {
            let mut inner = jobs.inner.lock().unwrap();
            let Some(progress) = inner.running.as_mut() else {
                return;
            };
            f(progress);
            progress.clone()
        };
        emit_progress(&app, &snapshot);
    };
    update_progress(&|p| p.total = targets.len() as u32);

    let results: Mutex<Vec<Option<RepoUpdate>>> = Mutex::new(vec![None; targets.len()]);
    let next = AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for _ in 0..workers.min(targets.len().max(1)) {
            scope.spawn(|| loop {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(path) = targets.get(index) else {
                    break;
                };
                update_progress(&|p| p.running.push(path.clone()));

                let report = update_one(
                    &manager,
                    path,
                    None,
                    allow.contains(&path_key(path)),
                    attended,
                    Some(&cancel),
                );

                let updated = report
                    .branches
                    .iter()
                    .filter(|b| b.kind == BranchUpdateKind::Updated)
                    .count() as u32;
                let commits = report.commits_received;
                let level = report.level;
                results.lock().unwrap()[index] = Some(report);
                update_progress(&|p| {
                    p.running.retain(|r| r != path);
                    p.done += 1;
                    p.branches_updated += updated;
                    p.commits_received += commits;
                    match level {
                        RepoUpdateLevel::Error => p.errors += 1,
                        RepoUpdateLevel::Warning => p.warnings += 1,
                        _ => {}
                    }
                });
            });
        }
    });

    let cancelled = cancel.load(Ordering::SeqCst);
    let mut repos: Vec<RepoUpdate> = results
        .into_inner()
        .unwrap()
        .into_iter()
        .zip(targets.iter())
        .map(|(result, path)| {
            result.unwrap_or_else(|| {
                let mut skipped = blank_report(folder_name(path), path.clone());
                skipped.skipped = true;
                skipped
            })
        })
        .collect();
    sort_results(&mut repos);

    let report = UpdateAllReport {
        job,
        started_at,
        finished_at: now_secs(),
        cancelled,
        repos,
    };
    {
        let mut inner = jobs.inner.lock().unwrap();
        inner.running = None;
        inner.cancel = None;
        inner.last = Some(report.clone());
    }
    log::info!(
        "update-all job {job} finished in {:.1}s (cancelled={cancelled})",
        report.finished_at - report.started_at
    );
    let _ = app.emit("update-all-finished", report);
}

/// Ask the running job to stop. Fetches in progress are ended at once, and
/// every repository not finished is reported as not checked.
#[tauri::command]
#[specta::specta]
pub async fn update_all_cancel(
    app: AppHandle,
    jobs: State<'_, UpdateAllJobs>,
) -> Result<(), AppError> {
    let snapshot = {
        let mut inner = jobs.inner.lock().unwrap();
        if let Some(cancel) = &inner.cancel {
            cancel.store(true, Ordering::SeqCst);
        }
        inner.running.as_mut().map(|p| {
            p.stopping = true;
            p.clone()
        })
    };
    if let Some(progress) = snapshot {
        emit_progress(&app, &progress);
    }
    Ok(())
}

/// The running job's progress and the last finished report, for a view that
/// mounts after the events it would have heard.
#[tauri::command]
#[specta::specta]
pub async fn update_all_state(jobs: State<'_, UpdateAllJobs>) -> Result<UpdateAllState, AppError> {
    let inner = jobs.inner.lock().unwrap();
    Ok(UpdateAllState {
        running: inner.running.clone(),
        last: inner.last.clone(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use git2::Signature;

    use super::*;

    fn test_repo() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::tempdir().expect("temp repo");
        let repo = Repository::init(dir.path()).expect("repo");
        let mut config = repo.config().expect("config");
        config.set_str("user.name", "Update Test").expect("name");
        config
            .set_str("user.email", "update@example.com")
            .expect("email");
        (dir, repo)
    }

    fn commit_file(repo: &Repository, name: &str, contents: &str) -> git2::Oid {
        let workdir = repo.workdir().expect("workdir");
        fs::write(workdir.join(name), contents).expect("write");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new(name)).expect("add");
        index.write().expect("write index");
        let tree = repo
            .find_tree(index.write_tree().expect("tree"))
            .expect("tree");
        let sig = Signature::now("Update Test", "update@example.com").expect("sig");
        let parents: Vec<_> = repo
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok())
            .into_iter()
            .collect();
        let parent_refs: Vec<_> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, name, &tree, &parent_refs)
            .expect("commit")
    }

    /// Point `refs/remotes/origin/<branch>` at `oid` and make `branch` track it,
    /// as a fetch would have left things.
    fn track(repo: &Repository, branch: &str, oid: git2::Oid) {
        repo.remote("origin", "https://example.invalid/repo.git")
            .ok();
        repo.reference(
            &format!("refs/remotes/origin/{branch}"),
            oid,
            true,
            "fake fetch",
        )
        .expect("remote ref");
        let mut config = repo.config().expect("config");
        config
            .set_str(&format!("branch.{branch}.remote"), "origin")
            .expect("remote");
        config
            .set_str(
                &format!("branch.{branch}.merge"),
                &format!("refs/heads/{branch}"),
            )
            .expect("merge");
    }

    fn head_branch(repo: &Repository) -> String {
        repo.head().unwrap().shorthand().unwrap().to_string()
    }

    fn branch_tip(repo: &Repository, name: &str) -> git2::Oid {
        repo.find_branch(name, BranchType::Local)
            .unwrap()
            .get()
            .target()
            .unwrap()
    }

    fn kinds(outcome: &BranchesOutcome) -> Vec<(String, BranchUpdateKind)> {
        outcome
            .branches
            .iter()
            .map(|b| (b.name.clone(), b.kind))
            .collect()
    }

    #[test]
    fn moves_a_branch_that_is_not_checked_out_without_touching_the_tree() {
        let (dir, mut repo) = test_repo();
        let base = commit_file(&repo, "a.txt", "a");
        let main = head_branch(&repo);
        // `develop` sits at base; the server has one more commit on it.
        repo.branch("develop", &repo.find_commit(base).unwrap(), false)
            .unwrap();
        let ahead = commit_file(&repo, "b.txt", "b");
        repo.reference(&format!("refs/heads/{main}"), base, true, "rewind")
            .unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        track(&repo, "develop", ahead);
        track(&repo, &main, base);

        let path = dir.path().to_string_lossy().into_owned();
        let outcome = update_local_branches(&mut repo, &path, None, false);

        assert_eq!(
            kinds(&outcome),
            vec![("develop".into(), BranchUpdateKind::Updated)]
        );
        assert_eq!(outcome.branches[0].commits, 1);
        assert_eq!(outcome.up_to_date, 1, "{main} already matched");
        assert_eq!(branch_tip(&repo, "develop"), ahead);
        assert_eq!(head_branch(&repo), main, "must not switch branches");
        assert!(
            !dir.path().join("b.txt").exists(),
            "the working tree is untouched"
        );
    }

    #[test]
    fn leaves_a_branch_that_has_its_own_commits_alone() {
        let (dir, mut repo) = test_repo();
        let base = commit_file(&repo, "a.txt", "a");
        let main = head_branch(&repo);
        let theirs = commit_file(&repo, "theirs.txt", "t");
        repo.reference(&format!("refs/heads/{main}"), base, true, "rewind")
            .unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        let ours = commit_file(&repo, "ours.txt", "o");
        track(&repo, &main, theirs);

        let path = dir.path().to_string_lossy().into_owned();
        let outcome = update_local_branches(&mut repo, &path, None, true);

        assert_eq!(
            kinds(&outcome),
            vec![(main.clone(), BranchUpdateKind::BothChanged)]
        );
        assert_eq!(branch_tip(&repo, &main), ours, "never merged or reset");
    }

    #[test]
    fn skips_the_checked_out_branch_over_changes_unless_allowed() {
        let (dir, mut repo) = test_repo();
        let base = commit_file(&repo, "a.txt", "a");
        let main = head_branch(&repo);
        let ahead = commit_file(&repo, "b.txt", "b");
        repo.reference(&format!("refs/heads/{main}"), base, true, "rewind")
            .unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        track(&repo, &main, ahead);
        fs::write(dir.path().join("a.txt"), "work in progress").unwrap();
        let path = dir.path().to_string_lossy().into_owned();

        let refused = update_local_branches(&mut repo, &path, None, false);
        assert_eq!(
            kinds(&refused),
            vec![(main.clone(), BranchUpdateKind::ChangesInTheWay)]
        );
        assert_eq!(branch_tip(&repo, &main), base);

        let allowed = update_local_branches(&mut repo, &path, None, true);
        assert_eq!(
            kinds(&allowed),
            vec![(main.clone(), BranchUpdateKind::Updated)]
        );
        assert_eq!(branch_tip(&repo, &main), ahead);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "work in progress",
            "the set-aside work must come back"
        );
    }

    #[test]
    fn reports_a_branch_whose_server_copy_was_deleted() {
        let (dir, mut repo) = test_repo();
        let base = commit_file(&repo, "a.txt", "a");
        repo.branch("old-feature", &repo.find_commit(base).unwrap(), false)
            .unwrap();
        track(&repo, "old-feature", base);
        repo.find_reference("refs/remotes/origin/old-feature")
            .unwrap()
            .delete()
            .unwrap();

        let path = dir.path().to_string_lossy().into_owned();
        let outcome = update_local_branches(&mut repo, &path, None, false);

        assert_eq!(
            kinds(&outcome),
            vec![("old-feature".into(), BranchUpdateKind::ServerCopyGone)]
        );
        assert_eq!(
            level_for(false, &outcome.branches),
            RepoUpdateLevel::Unchanged
        );
    }

    #[test]
    fn only_touches_the_named_branches() {
        let (dir, mut repo) = test_repo();
        let base = commit_file(&repo, "a.txt", "a");
        let main = head_branch(&repo);
        for name in ["one", "two"] {
            repo.branch(name, &repo.find_commit(base).unwrap(), false)
                .unwrap();
        }
        let ahead = commit_file(&repo, "b.txt", "b");
        repo.reference(&format!("refs/heads/{main}"), base, true, "rewind")
            .unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        track(&repo, "one", ahead);
        track(&repo, "two", ahead);

        let path = dir.path().to_string_lossy().into_owned();
        let outcome = update_local_branches(&mut repo, &path, Some(&["two".to_string()]), false);

        assert_eq!(
            kinds(&outcome),
            vec![("two".into(), BranchUpdateKind::Updated)]
        );
        assert_eq!(branch_tip(&repo, "one"), base);
    }

    #[test]
    fn leaves_a_branch_open_in_another_worktree_alone() {
        use crate::git::shell::run_git;

        let (dir, mut repo) = test_repo();
        let base = commit_file(&repo, "a.txt", "a");
        let main = head_branch(&repo);
        repo.branch("develop", &repo.find_commit(base).unwrap(), false)
            .unwrap();
        let ahead = commit_file(&repo, "b.txt", "b");
        repo.reference(&format!("refs/heads/{main}"), base, true, "rewind")
            .unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        track(&repo, "develop", ahead);

        let path = dir.path().to_string_lossy().into_owned();
        let other = tempfile::tempdir().unwrap();
        let other_path = other.path().join("wt").to_string_lossy().into_owned();
        run_git(
            Some(&path),
            &["worktree", "add", "-q", &other_path, "develop"],
        )
        .unwrap();

        let outcome = update_local_branches(&mut repo, &path, None, false);

        assert_eq!(
            kinds(&outcome),
            vec![("develop".into(), BranchUpdateKind::OpenElsewhere)]
        );
        assert_eq!(branch_tip(&repo, "develop"), base);
    }

    #[test]
    fn set_aside_changes_that_clash_are_kept_and_reported() {
        let (dir, mut repo) = test_repo();
        let base = commit_file(&repo, "a.txt", "a");
        let main = head_branch(&repo);
        let ahead = commit_file(&repo, "a.txt", "server version");
        repo.reference(&format!("refs/heads/{main}"), base, true, "rewind")
            .unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        track(&repo, &main, ahead);
        fs::write(dir.path().join("a.txt"), "my version").unwrap();

        let path = dir.path().to_string_lossy().into_owned();
        let outcome = update_local_branches(&mut repo, &path, None, true);

        assert_eq!(
            kinds(&outcome),
            vec![(main.clone(), BranchUpdateKind::ChangesClashed)]
        );
        assert_eq!(level_for(false, &outcome.branches), RepoUpdateLevel::Error);
        let mut stashes = 0;
        repo.stash_foreach(|_, _, _| {
            stashes += 1;
            true
        })
        .unwrap();
        assert_eq!(stashes, 1, "the user's work must still be recoverable");
    }

    /// The whole per-repository path with a real fetch: a teammate pushes to
    /// `main` and `develop`, and one run brings both local branches forward
    /// while the user stays on `main` with its files updated.
    #[test]
    fn fetches_and_updates_every_branch_from_a_real_remote() {
        use crate::git::shell::run_git;

        let root = tempfile::tempdir().unwrap();
        let at = |p: &str| root.path().join(p).to_string_lossy().into_owned();
        let git = |dir: &str, args: &[&str]| {
            run_git(Some(dir), args).unwrap_or_else(|e| panic!("git {args:?}: {e}"));
        };
        let commit = |dir: &str, file: &str| {
            fs::write(Path::new(dir).join(file), file).unwrap();
            git(dir, &["add", "."]);
            git(
                dir,
                &[
                    "-c",
                    "user.name=T",
                    "-c",
                    "user.email=t@e.com",
                    "commit",
                    "-q",
                    "-m",
                    file,
                ],
            );
        };

        let server = at("server.git");
        git(&at(""), &["init", "-q", "--bare", "-b", "main", &server]);
        let seed = at("seed");
        git(&at(""), &["clone", "-q", &server, &seed]);
        git(&seed, &["checkout", "-q", "-b", "main"]);
        commit(&seed, "one.txt");
        git(&seed, &["push", "-q", "origin", "main"]);
        git(&seed, &["push", "-q", "origin", "main:develop"]);

        let mine = at("mine");
        git(&at(""), &["clone", "-q", &server, &mine]);
        git(
            &mine,
            &["branch", "-q", "--track", "develop", "origin/develop"],
        );

        commit(&seed, "two.txt");
        git(&seed, &["push", "-q", "origin", "main"]);
        git(&seed, &["push", "-q", "origin", "main:develop"]);

        let manager = RepoManager::default();
        let report = update_one(&manager, &mine, None, false, Attended::Background, None);

        assert_eq!(report.level, RepoUpdateLevel::Updated, "{report:?}");
        assert_eq!(report.commits_received, 2);
        let names: Vec<_> = report.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["develop", "main"]);
        assert!(
            Path::new(&mine).join("two.txt").exists(),
            "the checked-out branch's files moved too"
        );
    }

    #[test]
    fn errors_sort_before_warnings_before_updates() {
        let mk = |name: &str, level| RepoUpdate {
            level,
            ..blank_report(name.into(), name.into())
        };
        let mut repos = vec![
            mk("a-ok", RepoUpdateLevel::Updated),
            mk("b-same", RepoUpdateLevel::Unchanged),
            mk("c-warn", RepoUpdateLevel::Warning),
            mk("d-err", RepoUpdateLevel::Error),
        ];
        sort_results(&mut repos);
        let names: Vec<_> = repos.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["d-err", "c-warn", "a-ok", "b-same"]);
    }

    #[test]
    fn duplicate_paths_are_updated_once() {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("proj");
        Repository::init(&repo_dir).unwrap();
        let spelled = repo_dir.to_string_lossy().into_owned();
        let request = UpdateAllRequest {
            folders: vec![dir.path().to_string_lossy().into_owned()],
            paths: vec![format!(
                "{}/",
                if cfg!(windows) {
                    spelled.replace('\\', "/").to_uppercase()
                } else {
                    spelled
                }
            )],
            allow_set_aside: Vec::new(),
            sign_in: false,
        };
        assert_eq!(gather_targets(&request).len(), 1);
    }
}
