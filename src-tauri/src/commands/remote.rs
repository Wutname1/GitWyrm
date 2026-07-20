//! Network operations via system git.exe (Git Credential Manager handles auth).
//! Progress lines from stderr stream to the frontend as `git-progress` events.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::git::refs;
use crate::git::types::{PullResult, PushResult, RebaseResult, RemoteBranchInfo, RemoteInfo};
use crate::state::RepoManager;

/// The current branch, its upstream, and how far apart they are. Returns `None`
/// for the pair when HEAD is detached or the branch has no upstream, so callers
/// can still report something sensible. Mirrors the ahead/behind calculation in
/// `commands::branch::list_branches`.
struct TrackingState {
  branch: Option<String>,
  upstream: Option<String>,
  ahead: u32,
  behind: u32,
  /// Set when an upstream is configured but its ref could not be resolved --
  /// usually a remote branch that was deleted and pruned. The counts are
  /// meaningless in that case and must not be read as "in sync".
  upstream_gone: bool,
}

fn tracking_state(repo: &git2::Repository) -> TrackingState {
  branch_tracking_state(repo, None)
}

/// Tracking state for a named local branch, or for HEAD when `branch_name` is
/// `None`. Used by push to report on a branch that is not checked out.
fn branch_tracking_state(repo: &git2::Repository, branch_name: Option<&str>) -> TrackingState {
  let none =
    TrackingState { branch: None, upstream: None, ahead: 0, behind: 0, upstream_gone: false };

  let name = match branch_name {
    Some(n) => n.to_string(),
    None => {
      let Ok(head) = repo.head() else { return none };
      if !head.is_branch() {
        return none;
      }
      let Some(name) = head.shorthand().map(str::to_string) else { return none };
      name
    }
  };
  let Ok(branch) = repo.find_branch(&name, git2::BranchType::Local) else { return none };

  let upstream = branch.upstream().ok().and_then(|u| u.name().ok().flatten().map(str::to_string));

  // An upstream whose ref will not resolve is reported separately: its counts
  // are (0, 0), which would otherwise be indistinguishable from a branch that
  // genuinely matches its upstream -- and push would report "sent 0 commits"
  // after successfully sending them.
  let (ahead, behind, upstream_gone) = match (&upstream, branch.get().target()) {
    (Some(up), Some(local_oid)) => {
      let up_oid =
        repo.find_branch(up, git2::BranchType::Remote).ok().and_then(|b| b.get().target());
      match up_oid {
        Some(up_oid) => repo
          .graph_ahead_behind(local_oid, up_oid)
          .map(|(a, b)| (a as u32, b as u32, false))
          .unwrap_or((0, 0, true)),
        None => (0, 0, true),
      }
    }
    (Some(_), None) => (0, 0, true),
    _ => (0, 0, false),
  };

  TrackingState { branch: Some(name), upstream, ahead, behind, upstream_gone }
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, Type)]
pub struct GitProgressPayload {
  pub repo_id: String,
  pub operation: String,
  pub line: String,
}

/// git writes progress, informational notes, and real errors all to stderr, so
/// the last line is often counting objects or a credential-helper note rather
/// than the cause of the failure. Prefer lines git itself marks as errors, then
/// fall back to the last line that isn't obvious progress noise.
fn failure_detail(stderr_lines: &[String], stdout: &str) -> String {
  let is_noise = |l: &str| {
    let low = l.to_lowercase();
    low.starts_with("remote:")
      || low.contains('%')
      || low.starts_with("counting objects")
      || low.starts_with("compressing objects")
      || low.starts_with("writing objects")
      || low.starts_with("receiving objects")
      || low.starts_with("resolving deltas")
      || low.starts_with("enumerating objects")
      || low.starts_with("everything up-to-date")
      || low.starts_with("already up to date")
  };

  // Lines git explicitly tags are the real cause when present.
  let tagged = stderr_lines.iter().rev().find(|l| {
    let low = l.to_lowercase();
    low.starts_with("error:") || low.starts_with("fatal:") || low.starts_with("hint:")
  });
  if let Some(line) = tagged {
    return line.clone();
  }

  stderr_lines
    .iter()
    .rev()
    .find(|l| !is_noise(l))
    .cloned()
    .unwrap_or_else(|| stdout.trim().to_string())
}

fn run_streaming(
  app: &AppHandle,
  repo_id: &str,
  repo_path: Option<&str>,
  operation: &str,
  args: &[&str],
) -> Result<String, AppError> {
  let mut cmd = Command::new("git");
  if let Some(path) = repo_path {
    cmd.arg("-C").arg(path);
  }
  cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW);
  }

  let mut child = cmd.spawn().map_err(|e| {
    if e.kind() == std::io::ErrorKind::NotFound {
      AppError::Other("git executable not found on PATH".into())
    } else {
      AppError::Io(e)
    }
  })?;

  // git writes progress to stderr; stream each line to the frontend.
  let stderr = child.stderr.take();
  let mut stderr_lines: Vec<String> = Vec::new();
  if let Some(stderr) = stderr {
    let reader = BufReader::new(stderr);
    for line in reader.split(b'\r') {
      // Progress uses \r updates; split on both \r and \n chunks.
      let Ok(chunk) = line else { break };
      for part in String::from_utf8_lossy(&chunk).split('\n') {
        let part = part.trim();
        if part.is_empty() {
          continue;
        }
        stderr_lines.push(part.to_string());
        let _ = app.emit(
          "git-progress",
          GitProgressPayload {
            repo_id: repo_id.to_string(),
            operation: operation.to_string(),
            line: part.to_string(),
          },
        );
      }
    }
  }

  let output = child.wait_with_output().map_err(AppError::Io)?;
  let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

  if !output.status.success() {
    return Err(AppError::Other(format!("git {operation} failed: {}", failure_detail(&stderr_lines, &stdout))));
  }
  Ok(stdout)
}

#[tauri::command]
#[specta::specta]
pub async fn git_fetch(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    run_streaming(&app, &repo_id, Some(&path), "fetch", &["fetch", "--all", "--prune", "--progress"])?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn git_pull(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<PullResult, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let before = { tracking_state(&open.repo.lock().unwrap()) };
    run_streaming(&app, &repo_id, Some(&path), "pull", &["pull", "--progress"])?;
    let after = { tracking_state(&open.repo.lock().unwrap()) };

    // Commits we were behind by and no longer are is what the pull brought in.
    let received = before.behind.saturating_sub(after.behind);

    Ok(PullResult {
      branch: after.branch.or(before.branch),
      upstream: after.upstream.or(before.upstream),
      received,
      ahead_after: after.ahead,
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn git_push(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<PushResult, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let before = { tracking_state(&open.repo.lock().unwrap()) };
    run_streaming(&app, &repo_id, Some(&path), "push", &["push", "--progress"])?;
    let after = { tracking_state(&open.repo.lock().unwrap()) };

    // Commits we were ahead by and no longer are is what the remote took.
    let pushed = before.ahead.saturating_sub(after.ahead);

    Ok(PushResult {
      branch: after.branch.or(before.branch),
      upstream: after.upstream.or(before.upstream),
      pushed,
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Push a named local branch, which need not be the one checked out. A branch
/// with no upstream is published to the default remote and tracked from then
/// on, so the next push needs no extra decision.
#[tauri::command]
#[specta::specta]
pub async fn git_push_branch(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  branch: String,
) -> Result<PushResult, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let (before, had_upstream, remote) = {
      let repo = open.repo.lock().unwrap();
      let state = branch_tracking_state(&repo, Some(&branch));
      let had_upstream = state.upstream.is_some();
      // The remote is needed either way: naming it is what lets the push
      // target a branch other than the checked-out one.
      let remote = match &state.upstream {
        // `origin/main` -> `origin`; the remote owns everything before the
        // first slash, and branch names may contain further slashes.
        Some(up) => up.split_once('/').map(|(r, _)| r.to_string()).unwrap_or(default_remote(&repo)?),
        None => default_remote(&repo)?,
      };
      (state, had_upstream, remote)
    };

    // Name the branch explicitly, so the push does not depend on which branch
    // happens to be checked out.
    let refspec = format!("refs/heads/{branch}");
    let mut args: Vec<&str> = vec!["push", "--progress"];
    // Re-link on a first publish, and also when the upstream ref went missing:
    // the config still names it, but the tracking ref needs recreating.
    if !had_upstream || before.upstream_gone {
      args.push("--set-upstream");
    }
    args.push(&remote);
    args.push(&refspec);

    run_streaming(&app, &repo_id, Some(&path), "push", &args)?;
    let after = { branch_tracking_state(&open.repo.lock().unwrap(), Some(&branch)) };

    // A branch whose upstream ref was pruned has no usable "before" count --
    // it reads (0, 0) exactly like a branch that matches -- so subtracting
    // would report zero after a successful push. It is recreating the remote
    // branch, so count it the same way as a first publish.
    let pushed = if had_upstream && !before.upstream_gone {
      // Commits we were ahead by and no longer are is what the remote took.
      before.ahead.saturating_sub(after.ahead)
    } else {
      // A freshly published branch: everything it holds over the new upstream's
      // merge base went across, which `published_count` reads back.
      published_count(&open.repo.lock().unwrap(), &branch)
    };

    Ok(PushResult {
      branch: after.branch.or(before.branch).or(Some(branch)),
      upstream: after.upstream.or(before.upstream),
      pushed,
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// How many commits a freshly published branch handed to the remote. Its new
/// upstream now matches it exactly, so counting against that yields zero;
/// count against the other remote-tracking branches instead, which is what the
/// remote did not already have.
fn published_count(repo: &git2::Repository, branch: &str) -> u32 {
  let Ok(local) = repo.find_branch(branch, git2::BranchType::Local) else { return 0 };
  let Some(tip) = local.get().target() else { return 0 };
  let Some(upstream) = local.upstream().ok().and_then(|u| u.get().target()) else { return 0 };

  let mut walk = match repo.revwalk() {
    Ok(w) => w,
    Err(_) => return 0,
  };
  if walk.push(tip).is_err() {
    return 0;
  }
  // Hide every other remote-tracking branch: what remains is unique to this one.
  if let Ok(branches) = repo.branches(Some(git2::BranchType::Remote)) {
    for (remote_branch, _) in branches.flatten() {
      if let Some(oid) = remote_branch.get().target() {
        if oid != upstream {
          let _ = walk.hide(oid);
        }
      }
    }
  }
  walk.count() as u32
}

/// Link a local branch to a remote branch of the same name, so push and pull
/// know where it belongs. Used to repair a branch whose remote branch was
/// deleted; publishing a brand-new branch happens through `git_push_branch`.
#[tauri::command]
#[specta::specta]
pub async fn set_branch_upstream(
  manager: State<'_, RepoManager>,
  repo_id: String,
  branch: String,
  remote: Option<String>,
) -> Result<String, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let remote = match remote {
      Some(r) => r,
      None => default_remote(&repo)?,
    };
    let upstream = format!("{remote}/{branch}");
    // The remote-tracking ref must exist, else the link would point nowhere
    // and push/pull would fail later with a much worse message.
    if repo.find_branch(&upstream, git2::BranchType::Remote).is_err() {
      return Err(AppError::Other(format!(
        "{upstream} doesn't exist. Send this branch to the remote first."
      )));
    }
    let mut local = repo.find_branch(&branch, git2::BranchType::Local)?;
    local.set_upstream(Some(&upstream))?;
    Ok(upstream)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Bring a branch up to date with its upstream without checking it out.
///
/// A branch that is only behind fast-forwards cleanly. One that has also moved
/// locally cannot: combining the two histories is a merge, which needs a
/// working tree, so this reports that rather than guessing. Pulling the branch
/// you are on goes through `git_pull` instead.
#[tauri::command]
#[specta::specta]
pub async fn git_pull_branch(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  branch: String,
) -> Result<PullResult, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let (before, remote) = {
      let repo = open.repo.lock().unwrap();
      let state = branch_tracking_state(&repo, Some(&branch));
      let Some(upstream) = state.upstream.clone() else {
        return Err(AppError::Other(format!(
          "{branch} isn't linked to a remote branch yet, so there's nothing to get."
        )));
      };
      let remote =
        upstream.split_once('/').map(|(r, _)| r.to_string()).unwrap_or(default_remote(&repo)?);
      (state, remote)
    };

    if before.ahead > 0 {
      return Err(AppError::Other(format!(
        "{branch} has its own commits as well as new ones on the remote. Switch to it to combine them."
      )));
    }
    if before.behind == 0 {
      return Ok(PullResult {
        branch: Some(branch),
        upstream: before.upstream,
        received: 0,
        ahead_after: before.ahead,
      });
    }

    // `<branch>:<branch>` updates the local ref directly. git refuses this
    // when it would not be a fast-forward, which is the guard we want.
    let refspec = format!("{branch}:{branch}");
    run_streaming(&app, &repo_id, Some(&path), "fetch", &["fetch", "--progress", &remote, &refspec])?;

    let after = { branch_tracking_state(&open.repo.lock().unwrap(), Some(&branch)) };
    Ok(PullResult {
      branch: after.branch.or(Some(branch)),
      upstream: after.upstream.or(before.upstream),
      received: before.behind.saturating_sub(after.behind),
      ahead_after: after.ahead,
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// The remote to publish a new branch to: the only one configured, or
/// `origin` when there are several.
fn default_remote(repo: &git2::Repository) -> Result<String, AppError> {
  let remotes = repo.remotes().map_err(AppError::Git)?;
  let names: Vec<String> = remotes.iter().flatten().map(str::to_string).collect();
  match names.len() {
    0 => Err(AppError::Other("This repository has no remote to push to.".into())),
    1 => Ok(names[0].clone()),
    _ => names
      .iter()
      .find(|n| *n == "origin")
      .cloned()
      .ok_or_else(|| AppError::Other("Several remotes are set up. Pick one in Remotes first.".into())),
  }
}

/// List configured remotes with their URLs and remote-tracking branches.
/// Local config read only; no network.
#[tauri::command]
#[specta::specta]
pub async fn list_remotes(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Vec<RemoteInfo>, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let records = refs::walk_branches(&repo)?;
    let locals = refs::local_tips(&records);
    let head_oid = repo.head().ok().and_then(|h| h.target());
    let mut remotes = Vec::new();

    for name in repo.remotes()?.iter().flatten() {
      let remote = repo.find_remote(name)?;
      let url = remote.url().unwrap_or("").to_string();
      let push_url = remote.pushurl().map(str::to_string).filter(|p| *p != url);

      let mut branches: Vec<RemoteBranchInfo> = records
        .iter()
        .filter(|r| r.remote_name() == Some(name))
        .map(|rec| {
          let short = rec.short_name().to_string();

          // Compare against the same-named local branch when there is one. When
          // there isn't, compare against HEAD so the row can still answer "is
          // there work here I don't have?" - which is the whole point.
          let counterpart = locals.get(short.as_str()).map(|&oid| (short.clone(), oid));
          let local_only_missing = counterpart.is_none();
          let baseline = counterpart.as_ref().map(|(_, oid)| *oid).or(head_oid);

          let (ahead_of_local, behind_local) = match (rec.tip, baseline) {
            (Some(remote_oid), Some(base_oid)) => {
              refs::ahead_behind(&repo, remote_oid, base_oid).unwrap_or((0, 0))
            }
            _ => (0, 0),
          };

          let commit = rec.tip.and_then(|oid| repo.find_commit(oid).ok());
          RemoteBranchInfo {
            name: short,
            tip: rec.tip.map(|oid| format!("{:.7}", oid)),
            time: rec.time.map(|t| t as f64),
            summary: commit.as_ref().and_then(|c| c.summary()).map(str::to_string),
            local_counterpart: counterpart.map(|(n, _)| n),
            ahead_of_local,
            behind_local,
            local_only_missing,
          }
        })
        .collect();
      branches.sort_by(|a, b| a.name.cmp(&b.name));

      let missing_locally = branches.iter().filter(|b| b.local_only_missing).count() as u32;
      remotes.push(RemoteInfo { name: name.to_string(), url, push_url, branches, missing_locally });
    }

    remotes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(remotes)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Add a new remote. Fails if the name is already in use.
#[tauri::command]
#[specta::specta]
pub async fn add_remote(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  url: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    repo.remote(name.trim(), url.trim())?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Rename a remote. Also rewrites its remote-tracking refs and any branch
/// upstreams that referenced the old name.
#[tauri::command]
#[specta::specta]
pub async fn rename_remote(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  new_name: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    // Returns any non-default refspecs that couldn't be auto-updated; a standard
    // remote has none, so we don't surface them.
    repo.remote_rename(name.trim(), new_name.trim())?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Change a remote's fetch URL.
#[tauri::command]
#[specta::specta]
pub async fn set_remote_url(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  url: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    repo.remote_set_url(name.trim(), url.trim())?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Delete a remote and its remote-tracking branches.
#[tauri::command]
#[specta::specta]
pub async fn remove_remote(
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    repo.remote_delete(name.trim())?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Set a remote-tracking branch as the upstream ("set target") of the current
/// local branch. `remote_branch` is the full remote-tracking name, e.g.
/// `origin/main`.
#[tauri::command]
#[specta::specta]
pub async fn set_upstream(
  manager: State<'_, RepoManager>,
  repo_id: String,
  remote_branch: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let head = repo.head()?;
    if !head.is_branch() {
      return Err(AppError::Other("HEAD is detached; check out a branch first".into()));
    }
    let shorthand = head
      .shorthand()
      .ok_or_else(|| AppError::Other("could not read current branch name".into()))?
      .to_string();

    let mut local = repo.find_branch(&shorthand, git2::BranchType::Local)?;
    // Confirm the remote-tracking branch exists before wiring it up.
    repo.find_branch(remote_branch.trim(), git2::BranchType::Remote)?;
    local.set_upstream(Some(remote_branch.trim()))?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Push the current branch, overwriting the remote with `--force-with-lease`.
/// Lease-based so it refuses to clobber remote commits the user hasn't fetched;
/// used after a local rewind/rebase leaves the branch diverged from its upstream.
#[tauri::command]
#[specta::specta]
pub async fn git_push_force(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<PushResult, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let before = { tracking_state(&open.repo.lock().unwrap()) };
    run_streaming(
      &app,
      &repo_id,
      Some(&path),
      "push",
      &["push", "--force-with-lease", "--progress"],
    )?;
    let after = { tracking_state(&open.repo.lock().unwrap()) };

    Ok(PushResult {
      branch: after.branch.or(before.branch),
      upstream: after.upstream.or(before.upstream),
      pushed: before.ahead.saturating_sub(after.ahead),
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Rebase a branch onto `onto` (e.g. `origin/main`), replaying its commits on
/// top. Rebases the current branch when `branch` is None; otherwise git checks
/// out `branch` first and leaves HEAD there. A clean rebase returns no
/// conflicts. A rebase that hits conflicts leaves the repo paused
/// (rebase-in-progress) and returns the conflicted paths instead of erroring,
/// so the frontend can guide the user. Refuses to start over a dirty tree.
#[tauri::command]
#[specta::specta]
pub async fn git_rebase(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  onto: String,
  branch: Option<String>,
) -> Result<RebaseResult, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    {
      let repo = open.repo.lock().unwrap();
      if refs::tracked_changes_present(&repo)? {
        return Err(AppError::Other(
          "working tree has changes; commit or stash before rebasing".into(),
        ));
      }
    }

    let mut args = vec!["rebase", "--progress", onto.as_str()];
    if let Some(b) = branch.as_deref() {
      args.push(b);
    }

    match run_streaming(&app, &repo_id, Some(&path), "rebase", &args) {
      Ok(_) => Ok(RebaseResult { conflicts: Vec::new() }),
      Err(e) => {
        // A conflicting rebase exits non-zero but leaves a rebase-in-progress
        // state under .git. If that's what happened, report the conflicts
        // rather than the raw error; a real failure (no rebase state) errors.
        let repo = open.repo.lock().unwrap();
        let git_dir = repo.path();
        let in_progress =
          git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists();
        if in_progress {
          let conflicts = refs::conflicted_paths(&repo)?;
          Ok(RebaseResult { conflicts })
        } else {
          Err(e)
        }
      }
    }
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn git_clone(
  app: AppHandle,
  url: String,
  destination: String,
) -> Result<String, AppError> {
  tauri::async_runtime::spawn_blocking(move || {
    run_streaming(&app, "clone", None, "clone", &["clone", "--progress", &url, &destination])?;
    Ok(destination)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}
