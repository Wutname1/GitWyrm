//! AI-assisted splitting of one busy working tree into logical commits.
//!
//! The model only plans: it assigns opaque, independently-stageable change
//! units to commit messages. GitWyrm owns every patch and tree it writes. All
//! commit objects are prepared off-ref, then HEAD moves once after the final
//! tree is verified, so an AI or patch failure leaves the user's branch alone.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use git2::{ErrorCode, Oid, ResetType};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::ai::{auth, catalog, client, prompt};
use crate::commands::patch::{self, SelectedLine};
use crate::error::AppError;
use crate::settings;
use crate::state::RepoManager;

const MAX_COMMITS: usize = 8;
const MAX_SPECIAL_INSTRUCTION_CHARS: usize = 4_000;
const MAX_PLAN_PROMPT_CHARS: usize = 120_000;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, Type)]
pub struct AiCreatedCommit {
  pub sha: String,
  pub summary: String,
  pub description: String,
  pub files: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PlannedCommit {
  summary: String,
  #[serde(default)]
  description: String,
  units: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CommitPlan {
  commits: Vec<PlannedCommit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeadState {
  oid: Option<Oid>,
  reference: Option<String>,
  detached: bool,
}

#[derive(Debug, Clone)]
struct ChangeUnit {
  id: String,
  path: String,
  block_index: usize,
  selection: Option<Vec<SelectedLine>>,
  preview: String,
}

#[derive(Debug, Clone)]
struct DiffBlock {
  raw: String,
  has_hunks: bool,
}

struct Snapshot {
  head: HeadState,
  tree: Oid,
  blocks: Vec<DiffBlock>,
  units: Vec<ChangeUnit>,
}

struct TempIndex(PathBuf);

impl TempIndex {
  fn new() -> Self {
    let stamp = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|duration| duration.as_nanos())
      .unwrap_or_default();
    Self(std::env::temp_dir().join(format!(
      "gitwyrm-ai-{}-{stamp}.index",
      std::process::id()
    )))
  }
}

impl Drop for TempIndex {
  fn drop(&mut self) {
    let _ = std::fs::remove_file(&self.0);
    let lock = PathBuf::from(format!("{}.lock", self.0.to_string_lossy()));
    let _ = std::fs::remove_file(lock);
  }
}

fn run_with_index(
  repo_path: &str,
  index: &Path,
  args: &[&str],
  stdin: Option<&[u8]>,
) -> Result<String, AppError> {
  let mut command = Command::new("git");
  command
    .arg("-C")
    .arg(repo_path)
    .args(args)
    .env("GIT_INDEX_FILE", index);

  if stdin.is_some() {
    command.stdin(Stdio::piped());
  }
  command.stdout(Stdio::piped()).stderr(Stdio::piped());

  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
  }

  let mut child = command.spawn().map_err(|error| {
    if error.kind() == std::io::ErrorKind::NotFound {
      AppError::Other("git executable not found on PATH".into())
    } else {
      AppError::Io(error)
    }
  })?;

  if let Some(bytes) = stdin {
    child
      .stdin
      .take()
      .ok_or_else(|| AppError::Other("failed to open git input".into()))?
      .write_all(bytes)?;
  }

  let output = child.wait_with_output()?;
  let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
  let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
  if !output.status.success() {
    let detail = if stderr.trim().is_empty() { stdout } else { stderr };
    return Err(AppError::Other(format!(
      "git {} failed: {}",
      args.first().copied().unwrap_or("command"),
      detail.trim()
    )));
  }
  Ok(stdout)
}

fn head_state(repo: &git2::Repository) -> Result<HeadState, AppError> {
  match repo.head() {
    Ok(head) => Ok(HeadState {
      oid: head.target(),
      reference: if head.is_branch() {
        head.name().map(str::to_string)
      } else {
        None
      },
      detached: repo.head_detached()?,
    }),
    Err(error) if matches!(error.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
      let reference = repo
        .find_reference("HEAD")
        .ok()
        .and_then(|head| head.symbolic_target().map(str::to_string));
      Ok(HeadState { oid: None, reference, detached: false })
    }
    Err(error) => Err(error.into()),
  }
}

fn init_index(repo_path: &str, index: &Path, head: Option<Oid>) -> Result<(), AppError> {
  match head {
    Some(oid) => {
      let oid = oid.to_string();
      run_with_index(repo_path, index, &["read-tree", &oid], None)?;
    }
    None => {
      run_with_index(repo_path, index, &["read-tree", "--empty"], None)?;
    }
  }
  Ok(())
}

fn diff_args(head: Option<Oid>, names_only: bool) -> Vec<String> {
  let mut args = vec![
    "-c".into(),
    "diff.indentHeuristic=false".into(),
    "-c".into(),
    "diff.algorithm=myers".into(),
    "diff".into(),
    "--cached".into(),
    "--no-color".into(),
    "--no-ext-diff".into(),
    "--diff-algorithm=myers".into(),
    "--find-renames".into(),
  ];
  if names_only {
    args.extend(["--name-only".into(), "-z".into()]);
  } else {
    args.extend(["--full-index".into(), "--binary".into(), "-U3".into()]);
  }
  if let Some(oid) = head {
    args.push(oid.to_string());
  }
  args.push("--".into());
  args
}

fn split_diff_blocks(raw: &str) -> Result<Vec<String>, AppError> {
  let starts: Vec<usize> = raw
    .match_indices("diff --git ")
    .filter_map(|(index, _)| {
      (index == 0 || raw.as_bytes().get(index.wrapping_sub(1)) == Some(&b'\n')).then_some(index)
    })
    .collect();
  if starts.is_empty() {
    return Err(AppError::Other("no changes found to organize".into()));
  }

  Ok(
    starts
      .iter()
      .enumerate()
      .map(|(index, start)| {
        let end = starts.get(index + 1).copied().unwrap_or(raw.len());
        raw[*start..end].to_string()
      })
      .collect(),
  )
}

fn fallback_path(block: &str, index: usize) -> String {
  block
    .lines()
    .find_map(|line| line.strip_prefix("+++ b/"))
    .or_else(|| block.lines().find_map(|line| line.strip_prefix("--- a/")))
    .map(|path| path.trim_matches('"').to_string())
    .unwrap_or_else(|| format!("changed file {}", index + 1))
}

fn parse_snapshot(head: HeadState, tree: Oid, diff: String, paths: Vec<String>) -> Result<Snapshot, AppError> {
  let raw_blocks = split_diff_blocks(&diff)?;
  let mut blocks = Vec::with_capacity(raw_blocks.len());
  let mut units = Vec::new();

  for (block_index, raw) in raw_blocks.into_iter().enumerate() {
    let path = paths
      .get(block_index)
      .filter(|path| !path.is_empty())
      .cloned()
      .unwrap_or_else(|| fallback_path(&raw, block_index));
    let logical = patch::logical_patch_units(&raw)?;
    let has_hunks = !logical.is_empty();

    if has_hunks {
      for unit in logical {
        units.push(ChangeUnit {
          id: format!("u{}", units.len() + 1),
          path: path.clone(),
          block_index,
          selection: Some(unit.selection),
          preview: unit.preview,
        });
      }
    } else {
      units.push(ChangeUnit {
        id: format!("u{}", units.len() + 1),
        path,
        block_index,
        selection: None,
        preview: raw.chars().take(4_000).collect(),
      });
    }
    blocks.push(DiffBlock { raw, has_hunks });
  }

  Ok(Snapshot { head, tree, blocks, units })
}

fn capture_snapshot(repo_path: &str, repo: &git2::Repository) -> Result<Snapshot, AppError> {
  let unmerged = crate::git::shell::run_git(Some(repo_path), &["ls-files", "-u"])?.stdout;
  if !unmerged.trim().is_empty() {
    return Err(AppError::Other(
      "Finish resolving conflicted files before generating commits".into(),
    ));
  }

  let head = head_state(repo)?;
  let temp = TempIndex::new();
  init_index(repo_path, &temp.0, head.oid)?;
  run_with_index(repo_path, &temp.0, &["add", "-A", "--", "."], None)?;

  let tree_text = run_with_index(repo_path, &temp.0, &["write-tree"], None)?;
  let tree = Oid::from_str(tree_text.trim())?;

  let raw_args = diff_args(head.oid, false);
  let raw_refs: Vec<&str> = raw_args.iter().map(String::as_str).collect();
  let diff = run_with_index(repo_path, &temp.0, &raw_refs, None)?;

  let name_args = diff_args(head.oid, true);
  let name_refs: Vec<&str> = name_args.iter().map(String::as_str).collect();
  let names = run_with_index(repo_path, &temp.0, &name_refs, None)?;
  let paths = names
    .split('\0')
    .filter(|path| !path.is_empty())
    .map(str::to_string)
    .collect();

  parse_snapshot(head, tree, diff, paths)
}

fn present_units(units: &[ChangeUnit]) -> String {
  let fixed_overhead: usize = units.iter().map(|unit| unit.id.len() + unit.path.len() + 32).sum();
  let preview_budget = MAX_PLAN_PROMPT_CHARS.saturating_sub(fixed_overhead);
  let per_unit = (preview_budget / units.len().max(1)).max(300);
  let mut output = String::new();

  for unit in units {
    output.push_str(&format!("\n--- {} | {} ---\n", unit.id, unit.path));
    let mut preview: String = unit.preview.chars().take(per_unit).collect();
    if preview.len() < unit.preview.len() {
      preview.push_str("\n[change preview shortened]\n");
    }
    output.push_str(&preview);
    if !output.ends_with('\n') {
      output.push('\n');
    }
  }
  output
}

fn parse_plan(text: &str) -> Result<CommitPlan, AppError> {
  let trimmed = text.trim();
  let json = match (trimmed.find('{'), trimmed.rfind('}')) {
    (Some(start), Some(end)) if start <= end => &trimmed[start..=end],
    _ => trimmed,
  };
  serde_json::from_str(json)
    .map_err(|error| AppError::Other(format!("AI returned a plan GitWyrm could not read: {error}")))
}

fn validate_plan(plan: &CommitPlan, requested: usize, units: &[ChangeUnit]) -> Result<(), AppError> {
  if plan.commits.len() != requested {
    return Err(AppError::Other(format!(
      "AI planned {} commits instead of the {requested} requested. Try again or adjust the instructions.",
      plan.commits.len()
    )));
  }

  let known: HashSet<&str> = units.iter().map(|unit| unit.id.as_str()).collect();
  let mut assigned = HashSet::new();
  for (index, commit) in plan.commits.iter().enumerate() {
    if commit.summary.trim().is_empty() {
      return Err(AppError::Other(format!("Commit {} has no message", index + 1)));
    }
    if commit.summary.contains('\n') || commit.summary.trim().chars().count() > 72 {
      return Err(AppError::Other(format!(
        "Commit {} needs a one-line message under 72 characters",
        index + 1
      )));
    }
    if commit.units.is_empty() {
      return Err(AppError::Other(format!("Commit {} has no changes", index + 1)));
    }
    for unit in &commit.units {
      if !known.contains(unit.as_str()) {
        return Err(AppError::Other(format!("AI used an unknown change label: {unit}")));
      }
      if !assigned.insert(unit.as_str()) {
        return Err(AppError::Other(format!("AI placed {unit} in more than one commit")));
      }
    }
  }

  if assigned.len() != units.len() {
    return Err(AppError::Other(
      "AI left some changes out of the plan. No commits were created; try again.".into(),
    ));
  }
  Ok(())
}

fn patch_for_units(snapshot: &Snapshot, selected: &HashSet<String>) -> Result<String, AppError> {
  let mut by_block: HashMap<usize, Vec<SelectedLine>> = HashMap::new();
  let mut whole_blocks = HashSet::new();

  for unit in &snapshot.units {
    if !selected.contains(unit.id.as_str()) {
      continue;
    }
    match &unit.selection {
      Some(lines) => by_block.entry(unit.block_index).or_default().extend(lines.clone()),
      None => {
        whole_blocks.insert(unit.block_index);
      }
    }
  }

  let mut output = String::new();
  for (index, block) in snapshot.blocks.iter().enumerate() {
    if whole_blocks.contains(&index) {
      output.push_str(&block.raw);
    } else if block.has_hunks {
      if let Some(selection) = by_block.get(&index) {
        output.push_str(&patch::build_patch_from_raw(&block.raw, selection, false)?);
      }
    }
  }
  Ok(output)
}

fn tree_for_units(
  repo_path: &str,
  snapshot: &Snapshot,
  selected: &HashSet<String>,
) -> Result<Oid, AppError> {
  let temp = TempIndex::new();
  init_index(repo_path, &temp.0, snapshot.head.oid)?;
  let patch = patch_for_units(snapshot, selected)?;
  if patch.trim().is_empty() {
    return Err(AppError::Other("a planned commit had no applicable changes".into()));
  }
  run_with_index(
    repo_path,
    &temp.0,
    &["apply", "--cached", "--binary", "--whitespace=nowarn", "-"],
    Some(patch.as_bytes()),
  )?;
  let tree = run_with_index(repo_path, &temp.0, &["write-tree"], None)?;
  Ok(Oid::from_str(tree.trim())?)
}

fn create_commit_chain(
  repo_path: &str,
  repo: &git2::Repository,
  snapshot: &Snapshot,
  plan: CommitPlan,
) -> Result<Vec<AiCreatedCommit>, AppError> {
  if head_state(repo)? != snapshot.head {
    return Err(AppError::Other(
      "The current branch changed while AI was working. No commits were created.".into(),
    ));
  }
  let latest = capture_snapshot(repo_path, repo)?;
  if latest.tree != snapshot.tree {
    return Err(AppError::Other(
      "Your files changed while AI was working. No commits were created; review the new changes and try again.".into(),
    ));
  }

  let signature = repo.signature().map_err(|_| {
    AppError::Other("Set your name and email in Git before creating commits".into())
  })?;
  let unit_by_id: HashMap<&str, &ChangeUnit> =
    snapshot.units.iter().map(|unit| (unit.id.as_str(), unit)).collect();
  let mut selected = HashSet::new();
  let mut parent_oid = snapshot.head.oid;
  let mut created = Vec::with_capacity(plan.commits.len());

  for planned in plan.commits {
    for unit in &planned.units {
      selected.insert(unit.clone());
    }
    let tree_oid = tree_for_units(repo_path, snapshot, &selected)?;
    let tree = repo.find_tree(tree_oid)?;
    let parent = parent_oid.map(|oid| repo.find_commit(oid)).transpose()?;
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
    let summary = planned.summary.trim().to_string();
    let description = planned.description.trim().to_string();
    let message = if description.is_empty() {
      summary.clone()
    } else {
      format!("{summary}\n\n{description}")
    };
    let oid = repo.commit(None, &signature, &signature, &message, &tree, &parents)?;

    let mut files: Vec<String> = planned
      .units
      .iter()
      .filter_map(|id| unit_by_id.get(id.as_str()).map(|unit| unit.path.clone()))
      .collect();
    files.sort();
    files.dedup();
    created.push(AiCreatedCommit {
      sha: oid.to_string(),
      summary,
      description,
      files,
    });
    parent_oid = Some(oid);
  }

  let final_oid = parent_oid.ok_or_else(|| AppError::Other("AI made no commits".into()))?;
  let final_commit = repo.find_commit(final_oid)?;
  if final_commit.tree_id() != snapshot.tree {
    return Err(AppError::Other(
      "The generated commits did not include every change. No branch was changed.".into(),
    ));
  }

  if snapshot.head.oid.is_some() {
    repo.reset(final_commit.as_object(), ResetType::Mixed, None)?;
  } else {
    let reference = snapshot
      .head
      .reference
      .as_deref()
      .ok_or_else(|| AppError::Other("GitWyrm could not find the current branch".into()))?;
    repo.reference(reference, final_oid, true, "commit: AI generated commits")?;
    let tree = final_commit.tree()?;
    let mut index = repo.index()?;
    index.read_tree(&tree)?;
    index.write()?;
  }

  Ok(created)
}

fn bearer_for(info: &auth::AuthInfo) -> &str {
  match info {
    auth::AuthInfo::Api { key } => key,
    auth::AuthInfo::Oauth { refresh, .. } => refresh,
  }
}

#[tauri::command]
#[specta::specta]
pub async fn generate_commits(
  app: tauri::AppHandle,
  manager: State<'_, RepoManager>,
  repo_id: String,
  provider: String,
  model: String,
  commit_count: u8,
  special_instructions: String,
) -> Result<Vec<AiCreatedCommit>, AppError> {
  let requested = commit_count as usize;
  if !(2..=MAX_COMMITS).contains(&requested) {
    return Err(AppError::Other(format!("Choose between 2 and {MAX_COMMITS} commits")));
  }
  if special_instructions.chars().count() > MAX_SPECIAL_INSTRUCTION_CHARS {
    return Err(AppError::Other("Special instructions are too long".into()));
  }

  let open = manager.get(&repo_id)?;
  let repo_path = open.path.to_string_lossy().into_owned();
  let snapshot = tauri::async_runtime::spawn_blocking({
    let open = open.clone();
    let repo_path = repo_path.clone();
    move || {
      let repo = open.repo.lock().unwrap();
      capture_snapshot(&repo_path, &repo)
    }
  })
  .await
  .map_err(|error| AppError::Other(error.to_string()))??;

  if snapshot.units.len() < requested {
    return Err(AppError::Other(format!(
      "These changes have only {} safe groups. Choose {} or fewer commits.",
      snapshot.units.len(),
      snapshot.units.len()
    )));
  }

  let info = auth::get(&app, &provider)?
    .ok_or_else(|| AppError::Other("Connect the selected AI provider first".into()))?;
  let provider_config = catalog::find(&app, &provider).await?;
  let saved_instruction = settings::get_settings(app.clone())?
    .ai_instruction
    .unwrap_or_default();
  let message_guidance = if saved_instruction.trim().is_empty() {
    prompt::DEFAULT_INSTRUCTION
  } else {
    saved_instruction.trim()
  };
  let special = if special_instructions.trim().is_empty() {
    "(none)"
  } else {
    special_instructions.trim()
  };
  let system = format!(
    "You organize a working tree into exactly {requested} small, logical git commits. \
Each labeled change unit is indivisible, but units from the same file may go into different commits. \
Every unit must appear exactly once. Order commits so foundations come before the work that uses them. \
Keep tests with the behavior they verify. Never invent or edit code.\n\n\
Commit message guidance:\n{message_guidance}\n\n\
Return JSON only, with this exact shape:\n\
{{\"commits\":[{{\"summary\":\"under 72 characters\",\"description\":\"1-3 plain sentences\",\"units\":[\"u1\"]}}]}}"
  );
  let user = format!(
    "Create exactly {requested} commits.\n\nSpecial instructions from the user:\n{special}\n\n\
Recent commit subjects:\n{}\n\nChange units:{}",
    crate::git::shell::run_git(Some(&repo_path), &["log", "--oneline", "--no-decorate", "-10"])
      .map(|output| output.stdout)
      .unwrap_or_default()
      .trim(),
    present_units(&snapshot.units)
  );

  let response = client::chat(client::ChatRequest {
    provider: &provider_config,
    bearer: bearer_for(&info),
    model: &model,
    system: &system,
    user: &user,
    max_tokens: 4_096,
  })
  .await?;
  let plan = parse_plan(&response)?;
  validate_plan(&plan, requested, &snapshot.units)?;

  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    create_commit_chain(&repo_path, &repo, &snapshot, plan)
  })
  .await
  .map_err(|error| AppError::Other(error.to_string()))?
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  fn units() -> Vec<ChangeUnit> {
    ["u1", "u2", "u3"]
      .into_iter()
      .enumerate()
      .map(|(index, id)| ChangeUnit {
        id: id.into(),
        path: format!("file-{index}"),
        block_index: index,
        selection: None,
        preview: String::new(),
      })
      .collect()
  }

  #[test]
  fn reads_json_even_when_wrapped_in_a_code_fence() {
    let plan = parse_plan(
      "```json\n{\"commits\":[{\"summary\":\"First\",\"units\":[\"u1\"]}]}\n```",
    )
    .unwrap();
    assert_eq!(plan.commits[0].summary, "First");
  }

  #[test]
  fn rejects_duplicate_or_missing_units() {
    let plan = CommitPlan {
      commits: vec![
        PlannedCommit { summary: "One".into(), description: String::new(), units: vec!["u1".into(), "u2".into()] },
        PlannedCommit { summary: "Two".into(), description: String::new(), units: vec!["u2".into()] },
      ],
    };
    assert!(validate_plan(&plan, 2, &units()).is_err());
  }

  #[test]
  fn accepts_a_complete_exact_plan() {
    let plan = CommitPlan {
      commits: vec![
        PlannedCommit { summary: "One".into(), description: String::new(), units: vec!["u1".into()] },
        PlannedCommit { summary: "Two".into(), description: String::new(), units: vec!["u2".into(), "u3".into()] },
      ],
    };
    assert!(validate_plan(&plan, 2, &units()).is_ok());
  }

  #[test]
  fn creates_a_verified_commit_chain_without_rewriting_files() {
    let temp = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(temp.path()).unwrap();
    {
      let mut config = repo.config().unwrap();
      config.set_str("user.name", "Test Wyrm").unwrap();
      config.set_str("user.email", "test@gitwyrm.dev").unwrap();
    }
    let original = (1..=8).map(|line| format!("line {line}\n")).collect::<String>();
    fs::write(temp.path().join("story.txt"), &original).unwrap();
    let base = {
      let mut index = repo.index().unwrap();
      index.add_path(Path::new("story.txt")).unwrap();
      index.write().unwrap();
      let tree_oid = index.write_tree().unwrap();
      let tree = repo.find_tree(tree_oid).unwrap();
      let signature = repo.signature().unwrap();
      repo.commit(Some("HEAD"), &signature, &signature, "base", &tree, &[]).unwrap()
    };

    let changed = original
      .replace("line 2\n", "line two changed\n")
      .replace("line 6\n", "line six changed\n");
    fs::write(temp.path().join("story.txt"), &changed).unwrap();
    let repo_path = temp.path().to_string_lossy().into_owned();
    let snapshot = capture_snapshot(&repo_path, &repo).unwrap();
    assert_eq!(
      snapshot.units.len(),
      2,
      "separate change blocks in one hunk should be separate AI units"
    );

    let plan = CommitPlan {
      commits: vec![
        PlannedCommit {
          summary: "Change the opening".into(),
          description: "Updates the first part.".into(),
          units: vec![snapshot.units[0].id.clone()],
        },
        PlannedCommit {
          summary: "Change the ending".into(),
          description: "Updates the last part.".into(),
          units: vec![snapshot.units[1].id.clone()],
        },
      ],
    };
    let made = create_commit_chain(&repo_path, &repo, &snapshot, plan).unwrap();

    assert_eq!(made.len(), 2);
    assert_eq!(fs::read_to_string(temp.path().join("story.txt")).unwrap(), changed);
    assert_eq!(repo.statuses(None).unwrap().len(), 0, "the final tree should be clean");
    let mut walk = repo.revwalk().unwrap();
    walk.push_head().unwrap();
    walk.hide(base).unwrap();
    assert_eq!(walk.count(), 2);
  }
}
