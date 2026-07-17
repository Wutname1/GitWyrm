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
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct FileDiff {
  pub path: String,
  pub additions: u32,
  pub deletions: u32,
  pub lines: Vec<DiffLineEntry>,
  pub binary: bool,
}
