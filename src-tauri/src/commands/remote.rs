//! Network operations via system git.exe (Git Credential Manager handles auth).
//! Progress lines from stderr stream to the frontend as `git-progress` events.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use git2::BranchType;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::git::refs;
use crate::git::submodule::follow_and_report;
use crate::git::types::{
  PullResult, PushResult, RebaseResult, RemoteBranchInfo, RemoteInfo, RemoteTagInfo, UnpushedTag,
};
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
      let Ok(name) = head.shorthand().map(str::to_string) else { return none };
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
use crate::git::shell::CREATE_NO_WINDOW;

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
  // A `remote:` prefix is progress noise for most lines, but the server also
  // reports the real reason a push was refused this way -- e.g.
  // `remote: error: GH006: Protected branch update failed`. Strip the prefix so
  // that reason can be recognized and surfaced instead of discarded as noise.
  fn strip_remote(l: &str) -> &str {
    l.strip_prefix("remote:").map(str::trim).unwrap_or(l)
  }

  let is_noise = |l: &str| {
    let low = strip_remote(l).to_lowercase();
    low.is_empty()
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

  // Lines git or the server explicitly tag are the real cause when present. The
  // `remote:` prefix is stripped first so server-side errors count too, and the
  // rejection markers git prints for a refused ref are treated the same way.
  let tagged = stderr_lines.iter().rev().find(|l| {
    let low = strip_remote(l).to_lowercase();
    low.starts_with("error:")
      || low.starts_with("fatal:")
      || low.starts_with("hint:")
      || low.contains("[rejected]")
      || low.contains("[remote rejected]")
  });
  if let Some(line) = tagged {
    return strip_remote(line).to_string();
  }

  stderr_lines
    .iter()
    .rev()
    .find(|l| !is_noise(l))
    .map(|l| strip_remote(l).to_string())
    .unwrap_or_else(|| stdout.trim().to_string())
}

fn run_streaming(
  app: &AppHandle,
  repo_id: &str,
  repo_path: Option<&str>,
  operation: &str,
  args: &[&str],
) -> Result<String, AppError> {
  // Honor the user's configured git.exe, same as git::shell::run_git. Without
  // this, network operations ignore the Settings override that local ops respect.
  let mut cmd = Command::new(crate::git::shell::git_program_name());
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

    // `--autostash` is what keeps a pull from ever failing just because the
    // working tree is dirty. Without it git refuses the whole operation with
    // "your local changes would be overwritten by merge" and leaves the user to
    // stash by hand -- a dead end the app should never hand back, since the
    // stash/pull/reapply it asks for is exactly what git can do itself.
    //
    // Git performs it atomically: on a conflicting reapply it keeps the stash
    // entry rather than dropping it, so the changes are always recoverable. It
    // also applies to both the merge and rebase forms, so it holds regardless of
    // the user's `pull.rebase` setting.
    run_streaming(&app, &repo_id, Some(&path), "pull", &["pull", "--progress", "--autostash"])?;

    // A pulled commit can change which version of a submodule the project
    // pins, and git leaves the nested checkout on the old one -- surfacing it
    // as a pending change the user never made. Move it to what was just
    // pulled, setting aside any edits inside it first.
    let submodules = {
      let repo = open.repo.lock().unwrap();
      follow_and_report(&repo, &path, "pull")
    };

    let after = { tracking_state(&open.repo.lock().unwrap()) };

    // Commits we were behind by and no longer are is what the pull brought in.
    let received = before.behind.saturating_sub(after.behind);

    Ok(PullResult {
      branch: after.branch.or(before.branch),
      upstream: after.upstream.or(before.upstream),
      received,
      ahead_after: after.ahead,
      submodules,
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Where a push should send a branch when the branch itself doesn't say.
///
/// A bare `git push` only works once a branch already tracks something. On a
/// branch that has never been published -- or one whose upstream ref was
/// deleted -- git refuses with "no upstream branch" and tells the user to type
/// a command, which is exactly the moment the app should just handle it. So
/// name the remote and the branch explicitly and pass `--set-upstream`, the
/// same way `git_push_branch` publishes a branch by name.
///
/// Returns `None` when the branch already tracks a live upstream: the plain
/// push is correct there and needs no extra arguments.
#[derive(Debug)]
struct PublishArgs {
  remote: String,
  refspec: String,
}

fn publish_args(state: &TrackingState, repo: &git2::Repository) -> Result<Option<PublishArgs>, AppError> {
  // A live upstream already answers where this goes.
  if state.upstream.is_some() && !state.upstream_gone {
    return Ok(None);
  }
  // Detached HEAD has no branch to publish; let git report that itself rather
  // than inventing a target.
  let Some(branch) = state.branch.as_deref() else { return Ok(None) };

  // Re-publishing a branch whose upstream ref went missing should go back to
  // the remote it named, not the default one. `state.upstream` can't answer
  // that: it is resolved *through* the remote-tracking ref, so a pruned ref
  // reads as no upstream at all. The config entry outlives the ref, so ask it.
  let remote = configured_remote(repo, branch)
    .map(Ok)
    .unwrap_or_else(|| default_remote(repo))?;

  Ok(Some(PublishArgs { remote, refspec: format!("refs/heads/{branch}") }))
}

/// The remote a branch is configured to push to (`branch.<name>.remote`),
/// regardless of whether its remote-tracking ref still exists. Returns `None`
/// for a branch that was never published, or one naming a remote that has since
/// been removed -- in both cases the caller falls back to the default.
fn configured_remote(repo: &git2::Repository, branch: &str) -> Option<String> {
  let config = repo.config().ok()?;
  let remote = config.get_string(&format!("branch.{branch}.remote")).ok()?;
  // A remote named in config but no longer set up would fail the push with a
  // worse message than "pick a remote".
  repo.find_remote(&remote).ok()?;
  Some(remote)
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
    let (before, publish) = {
      let repo = open.repo.lock().unwrap();
      let state = tracking_state(&repo);
      let publish = publish_args(&state, &repo)?;
      (state, publish)
    };

    let mut args: Vec<&str> = vec!["push", "--progress"];
    if let Some(p) = &publish {
      args.push("--set-upstream");
      args.push(&p.remote);
      args.push(&p.refspec);
    }
    run_streaming(&app, &repo_id, Some(&path), "push", &args)?;
    let after = { tracking_state(&open.repo.lock().unwrap()) };

    // A branch that was just published has no usable "before" count -- with no
    // upstream, or a pruned one, it reads (0, 0) exactly like a branch that
    // matches -- so subtracting would report zero after sending everything.
    let pushed = match (&publish, after.branch.as_deref()) {
      (Some(_), Some(branch)) => published_count(&open.repo.lock().unwrap(), branch),
      // Commits we were ahead by and no longer are is what the remote took.
      _ => before.ahead.saturating_sub(after.ahead),
    };

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
    let (before, publish, remote) = {
      let repo = open.repo.lock().unwrap();
      let state = branch_tracking_state(&repo, Some(&branch));
      let publish = publish_args(&state, &repo)?;
      // The remote is needed even when the branch already tracks one: naming it
      // is what lets the push target a branch other than the checked-out one.
      let remote = match &publish {
        Some(p) => p.remote.clone(),
        // `origin/main` -> `origin`; the remote owns everything before the
        // first slash, and branch names may contain further slashes.
        None => state
          .upstream
          .as_ref()
          .and_then(|up| up.split_once('/').map(|(r, _)| r.to_string()))
          .unwrap_or(default_remote(&repo)?),
      };
      (state, publish, remote)
    };

    // Name the branch explicitly, so the push does not depend on which branch
    // happens to be checked out.
    let refspec = format!("refs/heads/{branch}");
    let mut args: Vec<&str> = vec!["push", "--progress"];
    // Link on a first publish, and also when the upstream ref went missing:
    // the config still names it, but the tracking ref needs recreating.
    if publish.is_some() {
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
    let pushed = if publish.is_none() {
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

/// The local branch configured to track `remote_full` (`origin/develop`), if any.
///
/// Reads the upstream recorded in git config. A local branch that merely shares
/// the remote branch's name is NOT tracking it -- that is the ordinary state of
/// a branch nobody has linked yet, and reporting it as tracked is what left
/// users seeing "already set" on a link that was never written, with no way to
/// set it.
fn tracking_local(records: &[refs::RefRecord], remote_full: &str) -> Option<String> {
  records
    .iter()
    .find(|r| !r.is_remote && r.upstream.as_deref() == Some(remote_full))
    .map(|r| r.name.clone())
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
        // This updates a branch that is not checked out, so the working tree --
        // and every submodule checkout in it -- is untouched by design.
        submodules: Vec::new(),
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
      submodules: Vec::new(),
    })
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// The remote to publish a new branch to: the only one configured, or
/// `origin` when there are several.
fn default_remote(repo: &git2::Repository) -> Result<String, AppError> {
  let remotes = repo.remotes().map_err(AppError::Git)?;
  let names: Vec<String> = remotes.iter().flatten().flatten().map(str::to_string).collect();
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

/// The remote a tag operation should target: the caller's choice when given,
/// otherwise the repository's default remote.
fn resolve_remote(repo: &git2::Repository, remote: &str) -> Result<String, AppError> {
  let remote = remote.trim();
  if remote.is_empty() {
    default_remote(repo)
  } else {
    Ok(remote.to_string())
  }
}

/// Parse `git ls-remote --tags` output into tag names and the objects they
/// point at. Annotated tags also produce a `^{}` line naming the commit they
/// peel to; those carry no new name, so they are dropped.
fn parse_ls_remote_tags(stdout: &str) -> Vec<RemoteTagInfo> {
  let mut tags = Vec::new();
  for line in stdout.lines() {
    let Some((sha, refname)) = line.split_once('\t') else { continue };
    let Some(name) = refname.strip_prefix("refs/tags/") else { continue };
    if name.ends_with("^{}") {
      continue;
    }
    tags.push(RemoteTagInfo { name: name.to_string(), sha: sha.trim().to_string() });
  }
  tags
}

/// Tags the named remote already has. Hits the network via `git ls-remote`, so
/// callers should cache the result rather than polling it.
#[tauri::command]
#[specta::specta]
pub async fn list_remote_tags(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  remote: String,
) -> Result<Vec<RemoteTagInfo>, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let remote = { resolve_remote(&open.repo.lock().unwrap(), &remote)? };
    let stdout = run_streaming(
      &app,
      &repo_id,
      Some(&path),
      "ls-remote",
      &["ls-remote", "--tags", &remote],
    )?;
    let mut tags = parse_ls_remote_tags(&stdout);
    tags.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(tags)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Local tags the remote is missing. `commit_on_remote` distinguishes tags that
/// can be pushed on their own from those whose commit hasn't been sent yet.
#[tauri::command]
#[specta::specta]
pub async fn unpushed_tags(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  remote: String,
) -> Result<Vec<UnpushedTag>, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let remote = { resolve_remote(&open.repo.lock().unwrap(), &remote)? };
    let stdout = run_streaming(
      &app,
      &repo_id,
      Some(&path),
      "ls-remote",
      &["ls-remote", "--tags", &remote],
    )?;
    let on_remote: std::collections::HashSet<String> =
      parse_ls_remote_tags(&stdout).into_iter().map(|t| t.name).collect();

    let repo = open.repo.lock().unwrap();

    // Tips of this remote's tracking branches. A tagged commit reachable from
    // any of them is already on the remote, so its tag can be pushed alone.
    let prefix = format!("{remote}/");
    let mut remote_tips = Vec::new();
    if let Ok(branches) = repo.branches(Some(git2::BranchType::Remote)) {
      for (branch, _) in branches.flatten() {
        let Ok(Some(name)) = branch.name() else { continue };
        if name.starts_with(&prefix) {
          if let Some(oid) = branch.get().target() {
            remote_tips.push(oid);
          }
        }
      }
    }

    let mut missing = Vec::new();
    for name in repo.tag_names(None)?.iter().flatten().flatten() {
      if on_remote.contains(name) {
        continue;
      }
      let Ok(reference) = repo.find_reference(&format!("refs/tags/{name}")) else { continue };
      let Ok(commit) = reference.peel_to_commit() else { continue };
      let oid = commit.id();
      let commit_on_remote = remote_tips
        .iter()
        .any(|tip| *tip == oid || repo.graph_descendant_of(*tip, oid).unwrap_or(false));
      missing.push(UnpushedTag {
        name: name.to_string(),
        target_sha: oid.to_string(),
        commit_on_remote,
      });
    }
    missing.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(missing)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Push one tag to a remote. Never force-updates: a tag the remote already has
/// under a different object fails rather than silently moving it.
#[tauri::command]
#[specta::specta]
pub async fn push_tag(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  remote: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let remote = { resolve_remote(&open.repo.lock().unwrap(), &remote)? };
    let refspec = format!("refs/tags/{}", name.trim());
    run_streaming(
      &app,
      &repo_id,
      Some(&path),
      "push",
      &["push", "--progress", &remote, &refspec],
    )?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Delete a tag from a remote. The local tag is left alone, so a tag can be
/// un-published without losing the local copy.
#[tauri::command]
#[specta::specta]
pub async fn delete_remote_tag(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  remote: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let remote = { resolve_remote(&open.repo.lock().unwrap(), &remote)? };
    let refspec = format!("refs/tags/{}", name.trim());
    run_streaming(
      &app,
      &repo_id,
      Some(&path),
      "push",
      &["push", "--progress", "--delete", &remote, &refspec],
    )?;
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Delete a branch from a remote. `name` is the branch as it exists on the
/// remote (`feature/x`), without the remote prefix. The local branch of the
/// same name, if any, is left alone -- callers that want both gone delete the
/// local copy separately.
///
/// The stale remote-tracking ref is pruned so the branch stops showing in the
/// sidebar and graph immediately; without this the ref lingers until the next
/// fetch and the row looks undeleted.
#[tauri::command]
#[specta::specta]
pub async fn delete_remote_branch(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  name: String,
  remote: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    let remote = { resolve_remote(&open.repo.lock().unwrap(), &remote)? };
    let name = name.trim().to_string();
    if name.is_empty() {
      return Err(AppError::Other("no branch name given".into()));
    }
    let refspec = format!("refs/heads/{name}");
    run_streaming(
      &app,
      &repo_id,
      Some(&path),
      "push",
      &["push", "--progress", "--delete", &remote, &refspec],
    )?;

    // The push succeeded, so the branch is gone from the server. Dropping the
    // tracking ref is bookkeeping: report success even if it is already absent.
    let repo = open.repo.lock().unwrap();
    if let Ok(mut branch) = repo.find_branch(&format!("{remote}/{name}"), BranchType::Remote) {
      let _ = branch.delete();
    }
    Ok(())
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
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
    let mut remotes = Vec::new();

    for name in repo.remotes()?.iter().flatten().flatten() {
      let remote = repo.find_remote(name)?;
      let url = remote.url().unwrap_or("").to_string();
      let push_url = remote.pushurl().ok().flatten().map(str::to_string).filter(|p| *p != url);

      let mut branches: Vec<RemoteBranchInfo> = records
        .iter()
        .filter(|r| r.remote_name() == Some(name))
        .map(|rec| {
          let short = rec.short_name().to_string();

          // A remote branch is only meaningfully ahead of or behind its
          // same-named local branch. Comparing a remote-only branch to HEAD
          // mixes unrelated histories and produces misleading counts.
          let counterpart = locals.get(short.as_str()).map(|&oid| (short.clone(), oid));
          let local_only_missing = counterpart.is_none();
          let baseline = counterpart.as_ref().map(|(_, oid)| *oid);

          // Who actually tracks this branch, from config. Deliberately not the
          // name match above: sharing a name is not tracking, and conflating
          // the two is what made the menu claim a branch was already connected
          // when no upstream had ever been set.
          let tracked_by = tracking_local(&records, &rec.name);

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
            summary: commit.as_ref().and_then(|c| c.summary().ok().flatten()).map(str::to_string),
            local_counterpart: counterpart.map(|(n, _)| n),
            tracked_by,
            ahead_of_local,
            behind_local,
            local_only_missing,
          }
        })
        .collect();
      branches.sort_by(|a, b| a.name.cmp(&b.name));

      let missing_locally = branches.iter().filter(|b| b.local_only_missing).count() as u32;
      let parsed = crate::git::remote_url::parse(&url);
      let provider = parsed
        .as_ref()
        .map(|p| p.provider)
        .unwrap_or(crate::git::remote_url::RemoteProvider::SelfHosted);
      let web_base = parsed.as_ref().map(|p| p.web_base());
      remotes.push(RemoteInfo {
        name: name.to_string(),
        url,
        push_url,
        branches,
        missing_locally,
        provider,
        web_base,
      });
    }

    remotes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(remotes)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// The host's page for one branch of a remote, or None when the host has no
/// known web route. Provider routes live in `git::remote_url` so the frontend
/// does not keep its own copy of them.
#[tauri::command]
#[specta::specta]
pub fn remote_branch_web_url(remote_url_value: String, branch: String) -> Option<String> {
  crate::git::remote_url::parse(&remote_url_value)?.branch_url(branch.trim())
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
      .ok()
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
    let (before, publish) = {
      let repo = open.repo.lock().unwrap();
      let state = tracking_state(&repo);
      let publish = publish_args(&state, &repo)?;
      (state, publish)
    };

    let mut args: Vec<&str> = vec!["push", "--force-with-lease", "--progress"];
    if let Some(p) = &publish {
      args.push("--set-upstream");
      args.push(&p.remote);
      args.push(&p.refspec);
    }
    run_streaming(&app, &repo_id, Some(&path), "push", &args)?;
    let after = { tracking_state(&open.repo.lock().unwrap()) };

    let pushed = match (&publish, after.branch.as_deref()) {
      (Some(_), Some(branch)) => published_count(&open.repo.lock().unwrap(), branch),
      _ => before.ahead.saturating_sub(after.ahead),
    };

    Ok(PushResult {
      branch: after.branch.or(before.branch),
      upstream: after.upstream.or(before.upstream),
      pushed,
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
/// so the frontend can guide the user. Uncommitted changes are auto-stashed and
/// put back afterwards, so a dirty tree is not a reason to refuse.
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
    // A dirty tree is not a reason to refuse: `--autostash` has git set the
    // changes aside and put them back when the rebase lands, the same way pull
    // does. Telling the user to go stash by hand would be asking them to do what
    // git will do for them.
    // No `--progress` here: unlike fetch/pull/push, `git rebase` has no such
    // flag and exits with "unknown option `progress'" if given one.
    let mut args = vec!["rebase", "--autostash", onto.as_str()];
    if let Some(b) = branch.as_deref() {
      args.push(b);
    }

    rebase_outcome(run_streaming(&app, &repo_id, Some(&path), "rebase", &args), &open)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Interpret a rebase command's exit. A conflicting step exits non-zero but
/// leaves a rebase-in-progress state under .git; report the conflicts rather
/// than the raw error. A real failure (no rebase state left) errors.
fn rebase_outcome(
  run: Result<String, AppError>,
  open: &crate::state::OpenRepo,
) -> Result<RebaseResult, AppError> {
  match run {
    Ok(_) => {
      // A replayed commit can move a submodule pointer just like a pulled one
      // can, leaving the nested checkout behind. Same follow-up, best effort:
      // the rebase itself already succeeded.
      let submodules = {
        let repo = open.repo.lock().unwrap();
        let path = open.path.to_string_lossy().into_owned();
        follow_and_report(&repo, &path, "rebase")
      };
      Ok(RebaseResult { conflicts: Vec::new(), submodules })
    }
    Err(e) => {
      let repo = open.repo.lock().unwrap();
      let git_dir = repo.path();
      let in_progress =
        git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists();
      if in_progress {
        let conflicts = refs::conflicted_paths(&repo)?;
        // Paused mid-rebase: nothing has landed, so no pin has moved yet.
        Ok(RebaseResult { conflicts, submodules: Vec::new() })
      } else {
        Err(e)
      }
    }
  }
}

/// Resume a paused rebase after its conflicts were resolved and staged. The
/// next step may conflict again, in which case the returned paths are the new
/// round to resolve. `core.editor=true` keeps git from opening an editor for
/// the replayed commit messages.
#[tauri::command]
#[specta::specta]
pub async fn rebase_continue(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<RebaseResult, AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    {
      let repo = open.repo.lock().unwrap();
      if repo.index()?.has_conflicts() {
        return Err(AppError::Other(
          "resolve all conflicts before continuing the rebase".into(),
        ));
      }
    }
    let args = ["-c", "core.editor=true", "rebase", "--continue"];
    rebase_outcome(run_streaming(&app, &repo_id, Some(&path), "rebase", &args), &open)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// Abandon a paused rebase, restoring the branch to where it was before the
/// rebase started.
#[tauri::command]
#[specta::specta]
pub async fn rebase_abort(
  app: AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<(), AppError> {
  let open = manager.get(&repo_id)?;
  let path = open.path.to_string_lossy().into_owned();
  tauri::async_runtime::spawn_blocking(move || {
    run_streaming(&app, &repo_id, Some(&path), "rebase", &["rebase", "--abort"])?;
    Ok(())
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
    // `--` keeps a URL that starts with `-` from being read as a git option.
    // Without it, a pasted `--upload-pack=...` clone URL runs an arbitrary command.
    if url.starts_with('-') {
      return Err(AppError::Other("that clone address isn't valid".into()));
    }
    run_streaming(&app, "clone", None, "clone", &["clone", "--progress", "--", &url, &destination])?;
    Ok(destination)
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

#[cfg(test)]
mod tests {
  use super::*;
  use git2::{Repository, Signature};
  use std::fs;

  fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().expect("index");
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).expect("add");
    index.write().expect("write index");
    let tree = repo.find_tree(index.write_tree().expect("tree id")).expect("tree");
    let sig = Signature::now("Push Test", "push@example.com").expect("signature");
    let parents = repo.head().ok().and_then(|h| h.peel_to_commit().ok()).into_iter().collect::<Vec<_>>();
    let parent_refs = parents.iter().collect::<Vec<_>>();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs).expect("commit");
  }

  fn repo_with_commit() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().expect("temp repo");
    let repo = Repository::init(dir.path()).expect("repo");
    fs::write(dir.path().join("base.txt"), "base\n").expect("write");
    commit_all(&repo, "base");
    (dir, repo)
  }

  /// A local branch that shares a remote branch's name but tracks nothing --
  /// the state the upstream bug was reported in.
  fn untracked_namesake(repo: &Repository, remote: &str, branch: &str) {
    let head = repo.head().expect("head").peel_to_commit().expect("commit");
    if repo.find_remote(remote).is_err() {
      repo.remote(remote, "https://example.invalid/repo.git").expect("remote");
    }
    repo
      .reference(&format!("refs/remotes/{remote}/{branch}"), head.id(), true, "test remote")
      .expect("remote ref");
    // Local branch of the same name, with no upstream set.
    repo.branch(branch, &head, true).expect("local branch");
  }

  #[test]
  fn a_shared_name_is_not_tracking() {
    // The reported bug: local `develop` beside `origin/develop`, never linked.
    // The menu read the shared name as a tracking link, so it claimed the
    // branch was already set up and hid the action that would set it.
    let (dir, repo) = repo_with_commit();
    untracked_namesake(&repo, "origin", "develop");

    let records = refs::walk_branches(&repo).expect("walk");
    assert_eq!(
      tracking_local(&records, "origin/develop"),
      None,
      "a same-named local branch with no upstream must not count as tracking"
    );
    drop(dir);
  }

  #[test]
  fn a_configured_upstream_is_tracking() {
    let (dir, repo) = repo_with_commit();
    track(&repo, "origin");

    let records = refs::walk_branches(&repo).expect("walk");
    let head = repo.head().expect("head").shorthand().expect("name").to_string();
    assert_eq!(tracking_local(&records, &format!("origin/{head}")), Some(head));
    drop(dir);
  }

  #[test]
  fn tracking_follows_config_not_the_name() {
    // A local branch may track a remote branch with a different name. Reading
    // the name would miss this link entirely and report the wrong branch.
    let (dir, repo) = repo_with_commit();
    let head_commit = repo.head().expect("head").peel_to_commit().expect("commit");
    repo.remote("origin", "https://example.invalid/repo.git").expect("remote");
    repo
      .reference("refs/remotes/origin/develop", head_commit.id(), true, "test remote")
      .expect("remote ref");
    let mut local = repo.branch("my-work", &head_commit, true).expect("local branch");
    local.set_upstream(Some("origin/develop")).expect("upstream");

    let records = refs::walk_branches(&repo).expect("walk");
    assert_eq!(
      tracking_local(&records, "origin/develop"),
      Some("my-work".to_string()),
      "tracking must be read from config, including a differently-named branch"
    );
    drop(dir);
  }

  /// Point the current branch at a remote-tracking ref, the way a branch that
  /// has been published once looks.
  fn track(repo: &Repository, remote: &str) {
    let head = repo.head().expect("head").peel_to_commit().expect("commit");
    let branch_name = repo.head().expect("head").shorthand().expect("name").to_string();
    repo.remote(remote, "https://example.invalid/repo.git").expect("remote");
    repo
      .reference(&format!("refs/remotes/{remote}/{branch_name}"), head.id(), true, "test remote")
      .expect("remote ref");
    let mut branch = repo.find_branch(&branch_name, git2::BranchType::Local).expect("branch");
    branch.set_upstream(Some(&format!("{remote}/{branch_name}"))).expect("upstream");
  }

  /// The reported bug: a branch that has never been pushed must be published
  /// rather than left to a bare `git push`, which fails with "no upstream
  /// branch" and asks the user to type a command.
  #[test]
  fn a_never_pushed_branch_gets_published() {
    let (_dir, repo) = repo_with_commit();
    repo.remote("origin", "https://example.invalid/repo.git").expect("remote");

    let state = tracking_state(&repo);
    let publish = publish_args(&state, &repo).expect("args").expect("must publish");
    assert_eq!(publish.remote, "origin");
    let branch = state.branch.expect("branch");
    assert_eq!(publish.refspec, format!("refs/heads/{branch}"));
  }

  /// A branch already tracking a live upstream needs no extra arguments; the
  /// plain push is correct and must stay that way.
  #[test]
  fn a_tracking_branch_pushes_plainly() {
    let (_dir, repo) = repo_with_commit();
    track(&repo, "origin");

    let state = tracking_state(&repo);
    assert!(publish_args(&state, &repo).expect("args").is_none(), "no republish needed");
  }

  /// An upstream named in config whose ref was pruned still needs re-linking,
  /// and must go back to the remote it named rather than the default.
  #[test]
  fn a_pruned_upstream_is_republished_to_its_own_remote() {
    let (_dir, repo) = repo_with_commit();
    // A second remote exists, so falling back to the default would be visible.
    repo.remote("origin", "https://example.invalid/other.git").expect("origin");
    track(&repo, "upstream");
    let branch_name = repo.head().expect("head").shorthand().expect("name").to_string();
    // Prune the remote-tracking ref, leaving the config pointing at nothing.
    repo
      .find_reference(&format!("refs/remotes/upstream/{branch_name}"))
      .expect("ref")
      .delete()
      .expect("prune");

    let state = tracking_state(&repo);
    let publish = publish_args(&state, &repo).expect("args").expect("must republish");
    assert_eq!(publish.remote, "upstream", "goes back to the remote it named");
  }

  /// A detached HEAD has no branch to publish. Inventing a target would push
  /// the wrong thing, so let git report the situation itself.
  #[test]
  fn a_detached_head_is_left_to_git() {
    let (_dir, repo) = repo_with_commit();
    repo.remote("origin", "https://example.invalid/repo.git").expect("remote");
    let head = repo.head().expect("head").peel_to_commit().expect("commit");
    repo.set_head_detached(head.id()).expect("detach");

    let state = tracking_state(&repo);
    assert!(publish_args(&state, &repo).expect("args").is_none(), "nothing to publish");
  }

  /// A branch naming a remote that has since been deleted must not push to a
  /// remote that isn't there; fall back to the default instead.
  #[test]
  fn a_config_naming_a_deleted_remote_falls_back() {
    let (_dir, repo) = repo_with_commit();
    track(&repo, "gone");
    repo.remote("origin", "https://example.invalid/repo.git").expect("origin");
    repo.remote_delete("gone").expect("delete remote");

    let state = tracking_state(&repo);
    let publish = publish_args(&state, &repo).expect("args").expect("must publish");
    assert_eq!(publish.remote, "origin", "falls back when the named remote is gone");
  }

  /// With no remote configured there is nowhere to publish to, and the message
  /// has to say that plainly instead of surfacing git's raw complaint.
  #[test]
  fn no_remote_explains_itself() {
    let (_dir, repo) = repo_with_commit();
    let state = tracking_state(&repo);
    let err = publish_args(&state, &repo).expect_err("no remote");
    assert!(err.to_string().contains("no remote"), "got: {err}");
  }
}
