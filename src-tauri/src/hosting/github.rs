//! GitHub as a [`HostProvider`].
//!
//! These are the same REST calls the app has always made; what changed is that
//! they now sit behind the trait, so the command layer no longer names GitHub.

use async_trait::async_trait;
use serde::Deserialize;

use super::http::{self, TIMEOUT};
use super::patch::parse_file_patch;
use super::registry::ProviderId;
use super::{
  not_connected, AuthKind, HostCapabilities, HostComment, HostProvider, IssueDetail, IssueSummary,
  MergeMethod, PrCommit, PrDetail, PrFile, PrSummary, RepoSlug,
};
use crate::error::AppError;
use crate::git::remote_url::{self, RemoteProvider};

pub struct GitHub;

const HOST: &str = "GitHub";
const API_BASE: &str = "https://api.github.com";
/// GitHub answers errors in `message`; `errors` carries validation detail.
const ERROR_KEYS: &[&str] = &["message", "errors"];

impl GitHub {
  /// Scopes: `repo` covers reading and acting on PRs/issues in private and
  /// public repos; `read:user` lets us show who is signed in.
  pub const SCOPE: &'static str = "repo read:user";

  fn request(
    &self,
    app: &tauri::AppHandle,
    method: reqwest::Method,
    path: &str,
  ) -> Result<reqwest::RequestBuilder, AppError> {
    let cred = http::credential(app, ProviderId::Github)?.ok_or_else(|| not_connected(HOST))?;
    Ok(self.anonymous(method, path).bearer_auth(cred.token))
  }

  fn anonymous(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
    http::client()
      .request(method, format!("{API_BASE}{path}"))
      .header("Accept", "application/vnd.github+json")
      .header("X-GitHub-Api-Version", "2022-11-28")
      .header("User-Agent", "GitWyrm")
      .timeout(TIMEOUT)
  }

  /// Signed-in request when a token is stored, anonymous otherwise.
  ///
  /// GitHub serves public repositories anonymously, so read-only lookups still
  /// work with nobody connected. The anonymous rate limit is much lower
  /// (60/hour per IP rather than 5000) and private repos return 404, which is
  /// why only small, cached reads use this.
  fn api_or_public(
    &self,
    app: &tauri::AppHandle,
    method: reqwest::Method,
    path: &str,
  ) -> reqwest::RequestBuilder {
    match self.request(app, method.clone(), path) {
      Ok(builder) => builder,
      Err(_) => self.anonymous(method, path),
    }
  }

  async fn fetch_comments(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<Vec<HostComment>, AppError> {
    let path = format!(
      "/repos/{}/{}/issues/{number}/comments?per_page=100",
      slug.owner, slug.repo
    );
    let comments: Vec<ApiComment> =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    Ok(comments.into_iter().map(to_comment).collect())
  }
}

#[async_trait]
impl HostProvider for GitHub {
  fn id(&self) -> ProviderId {
    ProviderId::Github
  }

  fn display_name(&self) -> &'static str {
    HOST
  }

  fn matches(&self, provider: RemoteProvider) -> bool {
    provider == RemoteProvider::GitHub
  }

  fn auth_kind(&self) -> AuthKind {
    AuthKind::DeviceCode
  }

  fn token_url(&self) -> Option<&'static str> {
    None
  }

  fn required_scopes(&self) -> &'static [&'static str] {
    &["repo", "read:user"]
  }

  fn capabilities(&self) -> HostCapabilities {
    HostCapabilities {
      issues: true,
      choose_merge_method: true,
      pr_updated_at: true,
      pr_line_counts: true,
      pr_contents: true,
    }
  }

  /// Only github.com proper. An Enterprise host parses as `RemoteProvider::GitHub`
  /// but answers on a different API base, so claiming it here would produce
  /// requests to api.github.com for a repository that does not live there.
  fn slug_from_remote(&self, url: &str) -> Option<RepoSlug> {
    let parsed = remote_url::parse(url)?;
    if parsed.host != "github.com" {
      return None;
    }
    let (owner, repo) = parsed.owner_repo()?;
    Some(RepoSlug::new(owner, repo))
  }

  async fn auth_status(&self, app: &tauri::AppHandle) -> Result<Option<String>, AppError> {
    if http::credential(app, ProviderId::Github)?.is_none() {
      return Ok(None);
    }
    #[derive(Deserialize)]
    struct User {
      login: String,
    }
    let res = self
      .request(app, reqwest::Method::GET, "/user")?
      .send()
      .await
      .map_err(|e| AppError::Other(format!("could not reach {HOST}: {e}")))?;
    if res.status().as_u16() == 401 {
      return Ok(None);
    }
    let user: User = http::check(res, HOST, ERROR_KEYS)
      .await?
      .json()
      .await
      .map_err(|e| AppError::Other(format!("bad response from {HOST}: {e}")))?;
    Ok(Some(user.login))
  }

  async fn list_prs(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
  ) -> Result<Vec<PrSummary>, AppError> {
    let path = format!(
      "/repos/{}/{}/pulls?state=open&per_page=50&sort=updated&direction=desc",
      slug.owner, slug.repo
    );
    // Falls back to an anonymous read so PR links work in public repos without
    // connecting GitHub. A private repo is invisible that way and answers 404,
    // so treat that as "no open pull requests": the branch menu omits the item
    // instead of the UI raising an error for a repo the user never connected.
    let res = self
      .api_or_public(app, reqwest::Method::GET, &path)
      .send()
      .await
      .map_err(|e| AppError::Other(format!("could not reach {HOST}: {e}")))?;
    if res.status().as_u16() == 404 {
      return Ok(Vec::new());
    }
    let prs: Vec<ApiPr> = http::check(res, HOST, ERROR_KEYS)
      .await?
      .json()
      .await
      .map_err(|e| AppError::Other(format!("bad response from {HOST}: {e}")))?;
    Ok(prs.into_iter().map(Into::into).collect())
  }

  async fn list_issues(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
  ) -> Result<Vec<IssueSummary>, AppError> {
    let path = format!(
      "/repos/{}/{}/issues?state=open&per_page=50&sort=updated&direction=desc",
      slug.owner, slug.repo
    );
    let issues: Vec<ApiIssue> =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    Ok(
      issues
        .into_iter()
        // The issues endpoint also returns pull requests; keep real issues only.
        .filter(|i| i.pull_request.is_none())
        .map(Into::into)
        .collect(),
    )
  }

  async fn pr_detail(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<PrDetail, AppError> {
    let path = format!("/repos/{}/{}/pulls/{number}", slug.owner, slug.repo);
    let pr: ApiPr =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    let comments = self.fetch_comments(app, slug, number).await?;
    Ok(PrDetail {
      number: pr.number,
      title: pr.title,
      body: pr.body.unwrap_or_default(),
      author_is_bot: is_bot(&pr.user),
      author: pr.user.login,
      state: pr.state,
      draft: pr.draft,
      merged: pr.merged,
      mergeable: pr.mergeable,
      head_ref: pr.head.name,
      base_ref: pr.base.name,
      additions: Some(pr.additions),
      deletions: Some(pr.deletions),
      changed_files: Some(pr.changed_files),
      comments,
      html_url: pr.html_url,
      created_at: pr.created_at,
      updated_at: Some(pr.updated_at),
    })
  }

  async fn issue_detail(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<IssueDetail, AppError> {
    let path = format!("/repos/{}/{}/issues/{number}", slug.owner, slug.repo);
    let issue: ApiIssue =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    let comments = self.fetch_comments(app, slug, number).await?;
    Ok(IssueDetail {
      number: issue.number,
      title: issue.title,
      body: issue.body.unwrap_or_default(),
      author: issue.user.login,
      state: issue.state,
      labels: issue.labels.into_iter().map(|l| l.name).collect(),
      assignee: issue.assignee.map(|a| a.login),
      comments,
      html_url: issue.html_url,
      created_at: issue.created_at,
      updated_at: Some(issue.updated_at),
    })
  }

  /// The pull request's changed files, diffs included.
  ///
  /// `per_page=100` is GitHub's maximum and the endpoint caps out at 300 files
  /// regardless of paging, so a very large pull request is necessarily partial.
  /// One page is fetched rather than three: it covers essentially every real
  /// review, and the file list says how many it is showing, so a truncated list
  /// reads as truncated instead of as the whole change.
  async fn pr_files(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<Vec<PrFile>, AppError> {
    let path = format!(
      "/repos/{}/{}/pulls/{number}/files?per_page=100",
      slug.owner, slug.repo
    );
    let files: Vec<ApiPrFile> =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    Ok(
      files
        .into_iter()
        .map(|f| PrFile {
          // GitHub omits `patch` for binary files and for anything it considers
          // too large to inline. The row still lists the path and counts.
          diff: f.patch.as_deref().map(|p| {
            parse_file_patch(
              &f.filename,
              f.previous_filename.clone(),
              p,
              f.additions,
              f.deletions,
            )
          }),
          path: f.filename,
          old_path: f.previous_filename,
          status: f.status,
          additions: f.additions,
          deletions: f.deletions,
        })
        .collect(),
    )
  }

  async fn pr_commits(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<Vec<PrCommit>, AppError> {
    let path = format!(
      "/repos/{}/{}/pulls/{number}/commits?per_page=100",
      slug.owner, slug.repo
    );
    let commits: Vec<ApiPrCommit> =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    Ok(
      commits
        .into_iter()
        .map(|c| PrCommit {
          summary: c
            .commit
            .message
            .lines()
            .next()
            .unwrap_or_default()
            .to_string(),
          // `author` is the GitHub account and is null for a commit whose email
          // matches no user; the commit's own author name is always present, so
          // it is the fallback rather than showing nobody.
          author: c
            .author
            .map(|a| a.login)
            .unwrap_or(c.commit.author.name),
          authored_at: Some(c.commit.author.date),
          sha: c.sha,
          html_url: c.html_url,
        })
        .collect(),
    )
  }

  async fn comment(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
    _is_pr: bool,
    body: &str,
  ) -> Result<HostComment, AppError> {
    // GitHub posts PR and issue comments through the same issues endpoint.
    let path = format!(
      "/repos/{}/{}/issues/{number}/comments",
      slug.owner, slug.repo
    );
    let created: ApiComment = http::send_json(
      self
        .request(app, reqwest::Method::POST, &path)?
        .json(&serde_json::json!({ "body": body })),
      HOST,
      ERROR_KEYS,
    )
    .await?;
    Ok(to_comment(created))
  }

  async fn approve_pr(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<(), AppError> {
    let path = format!(
      "/repos/{}/{}/pulls/{number}/reviews",
      slug.owner, slug.repo
    );
    http::send(
      self
        .request(app, reqwest::Method::POST, &path)?
        .json(&serde_json::json!({ "event": "APPROVE" })),
      HOST,
      ERROR_KEYS,
    )
    .await?;
    Ok(())
  }

  async fn merge_pr(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
    method: MergeMethod,
  ) -> Result<(), AppError> {
    let method = match method {
      MergeMethod::Merge => "merge",
      MergeMethod::Squash => "squash",
      MergeMethod::Rebase => "rebase",
    };
    let path = format!("/repos/{}/{}/pulls/{number}/merge", slug.owner, slug.repo);
    http::send(
      self
        .request(app, reqwest::Method::PUT, &path)?
        .json(&serde_json::json!({ "merge_method": method })),
      HOST,
      ERROR_KEYS,
    )
    .await?;
    Ok(())
  }

  async fn close_pr(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<(), AppError> {
    // GitHub models a pull request as an issue for state changes, but this is
    // still a separate host operation so the other providers close their PRs
    // through their own endpoints.
    let path = format!("/repos/{}/{}/issues/{number}", slug.owner, slug.repo);
    http::send(
      self
        .request(app, reqwest::Method::PATCH, &path)?
        .json(&serde_json::json!({ "state": "closed" })),
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
    let path = format!("/repos/{}/{}/issues/{number}", slug.owner, slug.repo);
    http::send(
      self
        .request(app, reqwest::Method::PATCH, &path)?
        .json(&serde_json::json!({ "state": "closed" })),
      HOST,
      ERROR_KEYS,
    )
    .await?;
    Ok(())
  }
}

// ---------------------------------------------------------------------------
// Response shapes, straight off the REST API

#[derive(Deserialize)]
struct ApiUser {
  login: String,
  #[serde(default, rename = "type")]
  kind: String,
}

#[derive(Deserialize)]
struct ApiLabel {
  name: String,
}

#[derive(Deserialize)]
struct ApiBranchRef {
  #[serde(rename = "ref")]
  name: String,
}

#[derive(Deserialize)]
struct ApiPr {
  number: u32,
  title: String,
  state: String,
  #[serde(default)]
  body: Option<String>,
  user: ApiUser,
  #[serde(default)]
  draft: bool,
  #[serde(default)]
  merged: bool,
  #[serde(default)]
  mergeable: Option<bool>,
  head: ApiBranchRef,
  base: ApiBranchRef,
  #[serde(default)]
  additions: u32,
  #[serde(default)]
  deletions: u32,
  #[serde(default)]
  changed_files: u32,
  html_url: String,
  created_at: String,
  updated_at: String,
}

#[derive(Deserialize)]
struct ApiIssue {
  number: u32,
  title: String,
  state: String,
  #[serde(default)]
  body: Option<String>,
  user: ApiUser,
  #[serde(default)]
  labels: Vec<ApiLabel>,
  #[serde(default)]
  assignee: Option<ApiUser>,
  #[serde(default)]
  comments: u32,
  html_url: String,
  created_at: String,
  updated_at: String,
  /// Present when an "issue" is really a pull request; used to filter.
  #[serde(default)]
  pull_request: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ApiPrFile {
  filename: String,
  status: String,
  #[serde(default)]
  additions: u32,
  #[serde(default)]
  deletions: u32,
  /// Absent for binary and over-large files; see `pr_files`.
  #[serde(default)]
  patch: Option<String>,
  /// Only present when the file was renamed or copied.
  #[serde(default)]
  previous_filename: Option<String>,
}

#[derive(Deserialize)]
struct ApiCommitAuthor {
  name: String,
  date: String,
}

#[derive(Deserialize)]
struct ApiCommitBody {
  message: String,
  author: ApiCommitAuthor,
}

#[derive(Deserialize)]
struct ApiPrCommit {
  sha: String,
  commit: ApiCommitBody,
  /// The GitHub account behind the commit, when one matches the email.
  #[serde(default)]
  author: Option<ApiUser>,
  html_url: String,
}

#[derive(Deserialize)]
struct ApiComment {
  user: ApiUser,
  #[serde(default)]
  body: Option<String>,
  created_at: String,
}

impl From<ApiPr> for PrSummary {
  fn from(p: ApiPr) -> Self {
    PrSummary {
      number: p.number,
      title: p.title,
      author_is_bot: is_bot(&p.user),
      author: p.user.login,
      draft: p.draft,
      head_ref: p.head.name,
      base_ref: p.base.name,
      updated_at: Some(p.updated_at),
      created_at: p.created_at,
      html_url: p.html_url,
    }
  }
}

impl From<ApiIssue> for IssueSummary {
  fn from(i: ApiIssue) -> Self {
    IssueSummary {
      number: i.number,
      title: i.title,
      author: i.user.login,
      labels: i.labels.into_iter().map(|l| l.name).collect(),
      assignee: i.assignee.map(|a| a.login),
      comments: i.comments,
      updated_at: Some(i.updated_at),
      html_url: i.html_url,
    }
  }
}

fn is_bot(user: &ApiUser) -> bool {
  user.kind == "Bot"
}

fn to_comment(c: ApiComment) -> HostComment {
  HostComment {
    author_is_bot: is_bot(&c.user),
    author: c.user.login,
    body: c.body.unwrap_or_default(),
    created_at: c.created_at,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_common_remote_urls() {
    for url in [
      "git@github.com:owner/repo.git",
      "ssh://git@github.com/owner/repo",
      "https://github.com/owner/repo.git",
      "https://github.com/owner/repo/",
    ] {
      let slug = GitHub.slug_from_remote(url).expect(url);
      assert_eq!(slug.owner, "owner", "{url}");
      assert_eq!(slug.repo, "repo", "{url}");
    }
  }

  /// Enterprise parses as GitHub but is served from another API base, so the
  /// slug must not resolve -- see `slug_from_remote`.
  #[test]
  fn enterprise_hosts_do_not_resolve() {
    assert!(GitHub
      .slug_from_remote("https://github.mycorp.com/o/r.git")
      .is_none());
  }

  #[test]
  fn other_hosts_do_not_resolve() {
    assert!(GitHub.slug_from_remote("https://gitlab.com/o/r.git").is_none());
  }
}
