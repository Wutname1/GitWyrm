//! GitLab as a [`HostProvider`], over REST v4.
//!
//! Close to GitHub in shape, with four differences that bite if you assume
//! otherwise:
//!
//! 1. **`iid`, not `id`.** Every merge request and issue has a per-project
//!    `iid` (what the UI shows, what sub-resource paths take) and an
//!    instance-global `id`. Using `id` in a URL yields a 404 or, worse, someone
//!    else's project's resource.
//! 2. **Sorting is split.** `order_by` picks the field and `sort` is only the
//!    direction -- the reverse of GitHub, where `sort` picks the field.
//! 3. **`state=opened`,** not `open`. A wrong value is not an error; it returns
//!    everything, which looks like a filter that silently stopped working.
//! 4. **The merge strategy is a project setting,** not a per-merge choice.
//!    Only squash can be requested per merge. Honouring a caller's Merge or
//!    Rebase choice would mean rewriting project config behind their back, so
//!    this reports `choose_merge_method: false` and uses the project's own.

use async_trait::async_trait;
use serde::Deserialize;

use super::http::{self, encode_segment, TIMEOUT};
use super::registry::ProviderId;
use super::{
  not_connected, AuthKind, HostCapabilities, HostComment, HostProvider, IssueDetail, IssueSummary,
  MergeMethod, PrDetail, PrSummary, RepoSlug,
};
use crate::error::AppError;
use crate::git::remote_url::{self, RemoteProvider};

pub struct GitLab;

const HOST: &str = "GitLab";
const DEFAULT_BASE: &str = "https://gitlab.com";
/// GitLab uses `message` for validation errors (often an array) and `error` for
/// auth failures.
const ERROR_KEYS: &[&str] = &["message", "error"];

impl GitLab {
  /// The project's URL-encoded full path, which is how GitLab addresses a
  /// project without a numeric id. `extra` holds the full namespace path so
  /// subgroups (`group/sub/proj`) survive; `owner/repo` is the fallback.
  fn project(&self, slug: &RepoSlug) -> String {
    let path = slug
      .extra
      .clone()
      .unwrap_or_else(|| format!("{}/{}", slug.owner, slug.repo));
    encode_segment(&path)
  }

  fn request(
    &self,
    app: &tauri::AppHandle,
    method: reqwest::Method,
    path: &str,
  ) -> Result<reqwest::RequestBuilder, AppError> {
    let cred = http::credential(app, ProviderId::Gitlab)?.ok_or_else(|| not_connected(HOST))?;
    let base = cred.base_url.as_deref().unwrap_or(DEFAULT_BASE);
    Ok(
      http::client()
        .request(method, format!("{}/api/v4{path}", base.trim_end_matches('/')))
        // PRIVATE-TOKEN is GitLab's own header and accepts both personal and
        // project access tokens; Bearer only works for OAuth tokens.
        .header("PRIVATE-TOKEN", cred.token)
        .header("User-Agent", "GitWyrm")
        .timeout(TIMEOUT),
    )
  }

  async fn notes(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    kind: &str,
    iid: u32,
  ) -> Result<Vec<HostComment>, AppError> {
    let path = format!(
      "/projects/{}/{kind}/{iid}/notes?per_page=100&sort=asc&order_by=created_at",
      self.project(slug)
    );
    let notes: Vec<ApiNote> =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    Ok(
      notes
        .into_iter()
        // System notes are GitLab's activity log ("changed title", "assigned
        // to"), not conversation. They arrive mixed in with no server-side
        // filter and typically outnumber real comments.
        .filter(|n| !n.system)
        .map(|n| HostComment {
          author_is_bot: n.author.bot,
          author: n.author.username,
          body: n.body,
          created_at: n.created_at,
        })
        .collect(),
    )
  }
}

#[async_trait]
impl HostProvider for GitLab {
  fn id(&self) -> ProviderId {
    ProviderId::Gitlab
  }

  fn display_name(&self) -> &'static str {
    HOST
  }

  fn matches(&self, provider: RemoteProvider) -> bool {
    provider == RemoteProvider::GitLab
  }

  fn auth_kind(&self) -> AuthKind {
    AuthKind::PersonalAccessToken
  }

  fn token_url(&self) -> Option<&'static str> {
    Some("https://gitlab.com/-/user_settings/personal_access_tokens")
  }

  /// `api` is the only scope that covers reading and writing merge requests,
  /// issues and notes. `read_api` is read-only, so a token with it fails on the
  /// first reply the user tries to post.
  fn required_scopes(&self) -> &'static [&'static str] {
    &["api"]
  }

  fn capabilities(&self) -> HostCapabilities {
    HostCapabilities {
      issues: true,
      // The strategy is a project setting; see the module docs.
      choose_merge_method: false,
      pr_updated_at: true,
      pr_line_counts: false,
    }
  }

  /// Keeps the full namespace path in `extra` so subgroups address correctly.
  fn slug_from_remote(&self, url: &str) -> Option<RepoSlug> {
    let parsed = remote_url::parse(url)?;
    if parsed.provider != RemoteProvider::GitLab {
      return None;
    }
    // GitLab nests arbitrarily deep, so take the last segment as the project
    // and everything before it as the namespace.
    let (namespace, repo) = parsed.path.rsplit_once('/')?;
    if namespace.is_empty() || repo.is_empty() {
      return None;
    }
    Some(RepoSlug::new(namespace, repo).with_extra(&parsed.path))
  }

  async fn auth_status(&self, app: &tauri::AppHandle) -> Result<Option<String>, AppError> {
    if http::credential(app, ProviderId::Gitlab)?.is_none() {
      return Ok(None);
    }
    #[derive(Deserialize)]
    struct User {
      username: String,
    }
    let res = self
      .request(app, reqwest::Method::GET, "/user")?
      .send()
      .await
      .map_err(|e| AppError::Other(format!("could not reach {HOST}: {e}")))?;
    if matches!(res.status().as_u16(), 401 | 403) {
      return Ok(None);
    }
    let user: User = http::check(res, HOST, ERROR_KEYS)
      .await?
      .json()
      .await
      .map_err(|e| AppError::Other(format!("bad response from {HOST}: {e}")))?;
    Ok(Some(user.username))
  }

  async fn list_prs(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
  ) -> Result<Vec<PrSummary>, AppError> {
    let path = format!(
      "/projects/{}/merge_requests?state=opened&order_by=updated_at&sort=desc&per_page=50",
      self.project(slug)
    );
    let mrs: Vec<ApiMr> =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    Ok(mrs.into_iter().map(Into::into).collect())
  }

  async fn list_issues(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
  ) -> Result<Vec<IssueSummary>, AppError> {
    let path = format!(
      "/projects/{}/issues?state=opened&order_by=updated_at&sort=desc&per_page=50",
      self.project(slug)
    );
    let issues: Vec<ApiIssue> =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    Ok(issues.into_iter().map(Into::into).collect())
  }

  async fn pr_detail(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<PrDetail, AppError> {
    let path = format!("/projects/{}/merge_requests/{number}", self.project(slug));
    let mr: ApiMr =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    let comments = self.notes(app, slug, "merge_requests", number).await?;
    Ok(PrDetail {
      number: mr.iid,
      title: mr.title,
      body: mr.description.unwrap_or_default(),
      author_is_bot: mr.author.bot,
      author: mr.author.username,
      // GitLab has no boolean `merged`; the state carries it.
      merged: mr.state == "merged",
      state: mr.state,
      draft: mr.draft,
      // `detailed_merge_status` replaced the deprecated `merge_status` in 15.6.
      // Anything other than "mergeable" is a reason it cannot merge right now.
      mergeable: mr
        .detailed_merge_status
        .as_deref()
        .map(|s| s == "mergeable"),
      head_ref: mr.source_branch,
      base_ref: mr.target_branch,
      additions: None,
      deletions: None,
      changed_files: None,
      comments,
      html_url: mr.web_url,
      created_at: mr.created_at,
      updated_at: Some(mr.updated_at),
    })
  }

  async fn issue_detail(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<IssueDetail, AppError> {
    let path = format!("/projects/{}/issues/{number}", self.project(slug));
    let issue: ApiIssue =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    let comments = self.notes(app, slug, "issues", number).await?;
    Ok(IssueDetail {
      number: issue.iid,
      title: issue.title,
      body: issue.description.unwrap_or_default(),
      author: issue.author.username,
      state: issue.state,
      labels: issue.labels,
      assignee: issue.assignee.map(|a| a.username),
      comments,
      html_url: issue.web_url,
      created_at: issue.created_at,
      updated_at: Some(issue.updated_at),
    })
  }

  async fn comment(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
    is_pr: bool,
    body: &str,
  ) -> Result<HostComment, AppError> {
    let kind = if is_pr { "merge_requests" } else { "issues" };
    let path = format!("/projects/{}/{kind}/{number}/notes", self.project(slug));
    let note: ApiNote = http::send_json(
      self
        .request(app, reqwest::Method::POST, &path)?
        .json(&serde_json::json!({ "body": body })),
      HOST,
      ERROR_KEYS,
    )
    .await?;
    Ok(HostComment {
      author_is_bot: note.author.bot,
      author: note.author.username,
      body: note.body,
      created_at: note.created_at,
    })
  }

  async fn approve_pr(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<(), AppError> {
    let path = format!(
      "/projects/{}/merge_requests/{number}/approve",
      self.project(slug)
    );
    http::send(self.request(app, reqwest::Method::POST, &path)?, HOST, ERROR_KEYS).await?;
    Ok(())
  }

  /// GitLab decides merge vs rebase-merge vs fast-forward at the project level,
  /// so only squash is passed through. Merge and Rebase use whatever the
  /// project is configured for rather than silently changing that setting.
  async fn merge_pr(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
    method: MergeMethod,
  ) -> Result<(), AppError> {
    let path = format!(
      "/projects/{}/merge_requests/{number}/merge",
      self.project(slug)
    );
    let squash = method == MergeMethod::Squash;
    http::send(
      self
        .request(app, reqwest::Method::PUT, &path)?
        .json(&serde_json::json!({ "squash": squash })),
      HOST,
      ERROR_KEYS,
    )
    .await?;
    Ok(())
  }

  async fn close_issue(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<(), AppError> {
    let path = format!("/projects/{}/issues/{number}", self.project(slug));
    http::send(
      self
        .request(app, reqwest::Method::PUT, &path)?
        // A bare verb, not a state name: "closed" here is silently ignored.
        .json(&serde_json::json!({ "state_event": "close" })),
      HOST,
      ERROR_KEYS,
    )
    .await?;
    Ok(())
  }
}

// ---------------------------------------------------------------------------
// Response shapes

#[derive(Deserialize)]
struct ApiAuthor {
  username: String,
  #[serde(default)]
  bot: bool,
}

#[derive(Deserialize)]
struct ApiMr {
  iid: u32,
  title: String,
  state: String,
  #[serde(default)]
  description: Option<String>,
  author: ApiAuthor,
  #[serde(default)]
  draft: bool,
  source_branch: String,
  target_branch: String,
  #[serde(default)]
  detailed_merge_status: Option<String>,
  web_url: String,
  created_at: String,
  updated_at: String,
}

#[derive(Deserialize)]
struct ApiIssue {
  iid: u32,
  title: String,
  state: String,
  #[serde(default)]
  description: Option<String>,
  author: ApiAuthor,
  /// Plain strings on GitLab, unlike GitHub's label objects.
  #[serde(default)]
  labels: Vec<String>,
  #[serde(default)]
  assignee: Option<ApiAuthor>,
  #[serde(default)]
  user_notes_count: u32,
  web_url: String,
  created_at: String,
  updated_at: String,
}

#[derive(Deserialize)]
struct ApiNote {
  body: String,
  author: ApiAuthor,
  created_at: String,
  #[serde(default)]
  system: bool,
}

impl From<ApiMr> for PrSummary {
  fn from(m: ApiMr) -> Self {
    PrSummary {
      number: m.iid,
      title: m.title,
      author_is_bot: m.author.bot,
      author: m.author.username,
      draft: m.draft,
      head_ref: m.source_branch,
      base_ref: m.target_branch,
      updated_at: Some(m.updated_at),
      created_at: m.created_at,
      html_url: m.web_url,
    }
  }
}

impl From<ApiIssue> for IssueSummary {
  fn from(i: ApiIssue) -> Self {
    IssueSummary {
      number: i.iid,
      title: i.title,
      author: i.author.username,
      labels: i.labels,
      assignee: i.assignee.map(|a| a.username),
      comments: i.user_notes_count,
      updated_at: Some(i.updated_at),
      html_url: i.web_url,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_simple_projects() {
    for url in [
      "git@gitlab.com:owner/repo.git",
      "https://gitlab.com/owner/repo.git",
      "ssh://git@gitlab.com/owner/repo",
    ] {
      let slug = GitLab.slug_from_remote(url).expect(url);
      assert_eq!(slug.owner, "owner", "{url}");
      assert_eq!(slug.repo, "repo", "{url}");
      assert_eq!(slug.extra.as_deref(), Some("owner/repo"), "{url}");
    }
  }

  /// Subgroups are the case a two-segment slug cannot express; `extra` carries
  /// the full path so the API call still addresses the right project.
  #[test]
  fn keeps_subgroup_paths() {
    let slug = GitLab
      .slug_from_remote("https://gitlab.com/group/sub/deep/proj.git")
      .expect("subgroup");
    assert_eq!(slug.extra.as_deref(), Some("group/sub/deep/proj"));
    assert_eq!(slug.repo, "proj");
    assert_eq!(GitLab.project(&slug), "group%2Fsub%2Fdeep%2Fproj");
  }

  #[test]
  fn self_hosted_gitlab_resolves() {
    let slug = GitLab
      .slug_from_remote("https://gitlab.mycorp.com/team/app.git")
      .expect("self hosted");
    assert_eq!(slug.repo, "app");
  }

  #[test]
  fn other_hosts_do_not_resolve() {
    assert!(GitLab.slug_from_remote("https://github.com/o/r.git").is_none());
  }

  /// A single-segment path has no namespace and cannot be addressed.
  #[test]
  fn rejects_pathless_remotes() {
    assert!(GitLab.slug_from_remote("https://gitlab.com/lonely.git").is_none());
  }
}
