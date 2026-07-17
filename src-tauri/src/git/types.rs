use serde::Serialize;
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

#[derive(Debug, Clone, Serialize, Type)]
pub struct StashInfo {
  pub index: u32,
  pub message: String,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TagInfo {
  pub name: String,
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

/// Current merge-in-progress state of the repo.
#[derive(Debug, Clone, Serialize, Type)]
pub struct MergeState {
  /// True when a merge is underway (MERGE_HEAD exists).
  pub merging: bool,
  /// Branch/ref being merged in, best-effort from MERGE_MSG.
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
