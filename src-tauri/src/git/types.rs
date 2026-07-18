use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Type)]
pub struct RepoInfo {
  pub id: String,
  pub name: String,
  pub path: String,
  pub head_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct RefInfo {
  pub name: String,
  #[serde(rename = "type")]
  pub ref_type: RefKind,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
  Head,
  Branch,
  Remote,
  Tag,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct CommitEntry {
  pub sha: String,
  pub short_sha: String,
  pub summary: String,
  pub author_name: String,
  pub author_email: String,
  pub author_initials: String,
  /// Unix epoch seconds.
  pub time: f64,
  pub lane: u32,
  /// Lane of each parent edge, aligned with `parent_shas`.
  pub parent_lanes: Vec<u32>,
  pub parent_shas: Vec<String>,
  pub is_merge: bool,
  pub refs: Vec<RefInfo>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct LogPage {
  pub commits: Vec<CommitEntry>,
  pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
pub enum StatusCode {
  #[serde(rename = "A")]
  Added,
  #[serde(rename = "M")]
  Modified,
  #[serde(rename = "D")]
  Deleted,
  #[serde(rename = "R")]
  Renamed,
  #[serde(rename = "!")]
  Conflicted,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct FileChange {
  pub path: String,
  pub status: StatusCode,
  pub additions: u32,
  pub deletions: u32,
  pub conflicted: bool,
  /// Set when this path is a submodule whose pinned commit moved. Ordinary file
  /// actions (stash, discard-by-checkout) can't touch it; the UI must offer
  /// submodule-specific handling instead.
  pub submodule: Option<SubmoduleMove>,
}

/// How a submodule's checked-out commit differs from the commit its parent repo
/// pins. `ahead`/`behind` are the workdir commit's position relative to the
/// recorded one.
#[derive(Debug, Clone, Serialize, Type)]
pub struct SubmoduleMove {
  pub path: String,
  /// Commit the parent repo records for this submodule.
  pub recorded_sha: String,
  /// Commit the submodule is actually checked out at, if initialized.
  pub workdir_sha: Option<String>,
  /// Commits the workdir is ahead of the recorded commit.
  pub ahead: u32,
  /// Commits the workdir is behind the recorded commit.
  pub behind: u32,
  /// False when the submodule has not been checked out (needs init).
  pub initialized: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct WorkingStatus {
  pub staged: Vec<FileChange>,
  pub unstaged: Vec<FileChange>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct BranchInfo {
  pub name: String,
  pub is_head: bool,
  pub upstream: Option<String>,
  pub ahead: u32,
  pub behind: u32,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct BranchList {
  pub local: Vec<BranchInfo>,
  pub remote: Vec<String>,
}

/// How two arbitrary refs relate: commits `ours` has that `theirs` doesn't
/// (ahead) and commits `theirs` has that `ours` doesn't (behind).
#[derive(Debug, Clone, Serialize, Type)]
pub struct BranchRelation {
  pub ahead: u32,
  pub behind: u32,
}

/// What happened to uncommitted changes during a branch switch.
#[derive(Debug, Clone, Copy, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutOutcome {
  /// Tree was clean, or changes carried over cleanly. Nothing to report.
  Clean,
  /// Changes were stashed and popped back successfully.
  Stashed,
  /// Changes were stashed, the switch happened, but the pop conflicted. The
  /// stash was KEPT as a backup; the working tree has conflict markers.
  StashPopConflict,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct StashInfo {
  pub index: u32,
  pub message: String,
}

/// Result of a stash-save attempt. A clean working tree is a no-op, not an error.
#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StashOutcome {
  Stashed,
  NothingToStash,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TagInfo {
  pub name: String,
}

/// A configured remote and the remote-tracking branches under it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RemoteInfo {
  pub name: String,
  pub url: String,
  /// Push URL when it differs from the fetch URL, else None.
  pub push_url: Option<String>,
  /// Short branch names under this remote (without the `<remote>/` prefix),
  /// e.g. `main`, `dependabot/npm/foo`. Excludes the symbolic HEAD ref.
  pub branches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct CommitDetail {
  pub sha: String,
  pub summary: String,
  pub body: String,
  pub author_name: String,
  pub author_email: String,
  pub time: f64,
  pub parent_shas: Vec<String>,
  pub files: Vec<FileChange>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct DiffLineEntry {
  /// "+" added, "-" removed, "" context, "@" hunk header.
  pub sign: String,
  pub old_no: Option<u32>,
  pub new_no: Option<u32>,
  pub text: String,
  /// Index into `FileDiff.hunks` this line belongs to. Hunk-header lines
  /// (`sign == "@"`) also carry the index of the hunk they introduce.
  pub hunk_index: u32,
}

/// A `@@ -old_start,old_lines +new_start,new_lines @@` hunk boundary.
#[derive(Debug, Clone, Serialize, Type)]
pub struct HunkHeader {
  pub old_start: u32,
  pub old_lines: u32,
  pub new_start: u32,
  pub new_lines: u32,
  /// Raw header text including any trailing section context.
  pub header: String,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct FileDiff {
  pub path: String,
  /// Rename source path, when the delta is a rename.
  pub old_path: Option<String>,
  pub additions: u32,
  pub deletions: u32,
  pub hunks: Vec<HunkHeader>,
  pub lines: Vec<DiffLineEntry>,
  pub binary: bool,
}

/// What a merge of a given ref into HEAD would do, without performing it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct MergeAnalysis {
  /// HEAD already contains the target; merging is a no-op.
  pub up_to_date: bool,
  /// Merge can fast-forward (no merge commit needed).
  pub can_fast_forward: bool,
  /// A real merge commit would be created.
  pub normal: bool,
  /// Short sha the target ref resolves to.
  pub target_sha: String,
}

/// Outcome of starting a merge.
#[derive(Debug, Clone, Serialize, Type)]
pub struct MergeResult {
  /// HEAD already contained the target; nothing changed.
  pub up_to_date: bool,
  /// HEAD was advanced without a merge commit.
  pub fast_forwarded: bool,
  /// Paths left in a conflicted state, needing resolution.
  pub conflicts: Vec<String>,
}

/// Outcome of a push. Measured from the branch's ahead/behind against its
/// upstream before and after, so the report reflects what actually moved rather
/// than what git printed.
#[derive(Debug, Clone, Serialize, Type)]
pub struct PushResult {
  /// The branch that was pushed, if HEAD was on one.
  pub branch: Option<String>,
  /// Its upstream, e.g. `origin/main`.
  pub upstream: Option<String>,
  /// Commits handed to the remote. Zero means the remote already matched.
  pub pushed: u32,
}

/// Outcome of a pull, measured the same way as `PushResult`.
#[derive(Debug, Clone, Serialize, Type)]
pub struct PullResult {
  pub branch: Option<String>,
  pub upstream: Option<String>,
  /// Commits brought in from the remote. Zero means there was nothing new.
  pub received: u32,
  /// The pull left the branch with commits still to push.
  pub ahead_after: u32,
}

/// Outcome of a rebase. A clean rebase returns no conflicts; a paused rebase
/// (conflicts to resolve) lists the conflicted paths and leaves the repo in its
/// rebase-in-progress state.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RebaseResult {
  /// Paths left in a conflicted state; the rebase is paused until resolved.
  pub conflicts: Vec<String>,
}

/// A pending index-level operation that can leave conflicts to resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
pub enum OperationKind {
  Merge,
  CherryPick,
  Revert,
}

/// How far a reset rewinds: ref only, ref+index, or ref+index+working tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum ResetMode {
  /// Move the branch ref only; index and working tree keep the changes.
  Soft,
  /// Move the ref and reset the index; working tree keeps the changes.
  Mixed,
  /// Move the ref, index, and working tree. Discards uncommitted work.
  Hard,
}

/// Where a branch ref pointed before an operation, so it can be undone.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RefMove {
  /// The branch that moved.
  pub branch: String,
  /// Full sha the branch pointed at before the move.
  pub previous_sha: String,
}

/// Current in-progress operation state of the repo (merge or cherry-pick).
#[derive(Debug, Clone, Serialize, Type)]
pub struct MergeState {
  /// True when an operation is underway (a merge or cherry-pick is in progress).
  pub merging: bool,
  /// Which operation is in progress, if any.
  pub operation: Option<OperationKind>,
  /// Branch/ref/commit being merged or picked, best-effort from the state files.
  pub incoming_label: Option<String>,
  /// Paths still conflicted.
  pub conflicts: Vec<String>,
}

/// The three sides of a conflicted file, as full text.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ConflictContent {
  pub path: String,
  /// Common ancestor version (stage 1); empty if added on both sides.
  pub base: String,
  /// Our version (stage 2, current branch).
  pub ours: String,
  /// Their version (stage 3, incoming branch).
  pub theirs: String,
  /// Working-tree text with conflict markers, for manual editing.
  pub merged: String,
  /// Any side is binary/undecodable; only ours/theirs whole-file choice is safe.
  pub binary: bool,
}
