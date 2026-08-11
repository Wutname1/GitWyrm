//! Bitbucket Cloud as a [`HostProvider`], over REST 2.0.
//!
//! Four things differ from every other host here:
//!
//! 1. **Auth is email + API token, sent as Basic.** App passwords were removed
//!    on 2026-07-28. The username half is the *Atlassian account email*, not the
//!    Bitbucket username, and it cannot be inferred -- hence
//!    [`AuthKind::EmailAndToken`].
//! 2. **A pull request has no `description`.** The body lives in `summary.raw`.
//!    Reading `description` yields nothing and looks like an empty PR.
//! 3. **Authors are `account` objects, not `user` objects.** Only `/user`
//!    returns `nickname`; everywhere else GDPR shapes guarantee only
//!    `display_name`, so that is what is shown.
//! 4. **The issue tracker is opt-in per repository.** A repo with it disabled
//!    answers 404, which means "no tracker" rather than "no such repo".

use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;

use super::http::{self, TIMEOUT};
use super::registry::ProviderId;
use super::{
  not_connected, AuthKind, HostCapabilities, HostComment, HostProvider, IssueDetail, IssueSummary,
  MergeMethod, PrDetail, PrSummary, RepoSlug,
};
use crate::error::AppError;
use crate::git::remote_url::{self, RemoteProvider};

pub struct Bitbucket;

const HOST: &str = "Bitbucket";
const API_BASE: &str = "https://api.bitbucket.org/2.0";
/// Bitbucket nests its message under `error`.
const ERROR_KEYS: &[&str] = &["error", "message"];

impl Bitbucket {
  fn request(
    &self,
    app: &tauri::AppHandle,
    method: reqwest::Method,
    path: &str,
  ) -> Result<reqwest::RequestBuilder, AppError> {
    let cred = http::credential(app, ProviderId::Bitbucket)?.ok_or_else(|| not_connected(HOST))?;
    let builder = http::client()
      .request(method, format!("{API_BASE}{path}"))
      .header("User-Agent", "GitWyrm")
      .timeout(TIMEOUT);
    Ok(match cred.email.as_deref() {
      // Atlassian API token: Basic with the account email as the username.
      Some(email) => {
        let encoded =
          base64::engine::general_purpose::STANDARD.encode(format!("{email}:{}", cred.token));
        builder.header("Authorization", format!("Basic {encoded}"))
      }
      // Repository/workspace access tokens are true bearer tokens.
      None => builder.bearer_auth(cred.token),
    })
  }

  fn repo_path(&self, slug: &RepoSlug) -> String {
    format!("/repositories/{}/{}", slug.owner, slug.repo)
  }

  async fn comments(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    kind: &str,
    number: u32,
  ) -> Result<Vec<HostComment>, AppError> {
    let path = format!("{}/{kind}/{number}/comments?pagelen=100", self.repo_path(slug));
    let page: Page<ApiComment> =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    Ok(
      page
        .values
        .into_iter()
        // Deleted comments still come back, with null content.
        .filter(|c| !c.deleted)
        .map(|c| HostComment {
          author_is_bot: false,
          author: c.user.map(|u| u.display_name).unwrap_or_default(),
          body: c.content.map(|c| c.raw).unwrap_or_default(),
          created_at: c.created_on,
        })
        .collect(),
    )
  }
}

#[async_trait]
impl HostProvider for Bitbucket {
  fn id(&self) -> ProviderId {
    ProviderId::Bitbucket
  }

  fn display_name(&self) -> &'static str {
    HOST
  }

  fn matches(&self, provider: RemoteProvider) -> bool {
    provider == RemoteProvider::Bitbucket
  }

  fn auth_kind(&self) -> AuthKind {
    AuthKind::EmailAndToken
  }

  fn token_url(&self) -> Option<&'static str> {
    Some("https://id.atlassian.com/manage-profile/security/api-tokens")
  }

  /// Atlassian tokens require scopes to be ticked explicitly; an unscoped token
  /// fails against Bitbucket without saying why. `read:pullrequest` notably does
  /// not imply `read:repository`.
  fn required_scopes(&self) -> &'static [&'static str] {
    &[
      "read:repository:bitbucket",
      "read:pullrequest:bitbucket",
      "write:pullrequest:bitbucket",
      "read:issue:bitbucket",
      "write:issue:bitbucket",
      "read:account",
    ]
  }

  fn capabilities(&self) -> HostCapabilities {
    HostCapabilities {
      issues: true,
      choose_merge_method: true,
      pr_updated_at: true,
      pr_line_counts: false,
    }
  }

  fn slug_from_remote(&self, url: &str) -> Option<RepoSlug> {
    let parsed = remote_url::parse(url)?;
    if parsed.provider != RemoteProvider::Bitbucket {
      return None;
    }
    let (workspace, repo) = parsed.owner_repo()?;
    Some(RepoSlug::new(workspace, repo))
  }

  async fn auth_status(&self, app: &tauri::AppHandle) -> Result<Option<String>, AppError> {
    if http::credential(app, ProviderId::Bitbucket)?.is_none() {
      return Ok(None);
    }
    #[derive(Deserialize)]
    struct User {
      display_name: String,
      #[serde(default)]
      nickname: Option<String>,
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
    // `/user` is the one endpoint that carries a nickname; prefer it since it
    // is the handle people recognise, and fall back to the display name.
    Ok(Some(user.nickname.unwrap_or(user.display_name)))
  }

  async fn list_prs(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
  ) -> Result<Vec<PrSummary>, AppError> {
    // pagelen caps at 50 on this endpoint specifically.
    let path = format!(
      "{}/pullrequests?state=OPEN&sort=-updated_on&pagelen=50",
      self.repo_path(slug)
    );
    let page: Page<ApiPr> =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    Ok(page.values.into_iter().map(Into::into).collect())
  }

  async fn list_issues(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
  ) -> Result<Vec<IssueSummary>, AppError> {
    let path = format!(
      "{}/issues?q=state%3D%22new%22+OR+state%3D%22open%22&sort=-updated_on&pagelen=50",
      self.repo_path(slug)
    );
    let res = self
      .request(app, reqwest::Method::GET, &path)?
      .send()
      .await
      .map_err(|e| AppError::Other(format!("could not reach {HOST}: {e}")))?;
    // The tracker is opt-in per repository. 404 here means "not enabled", which
    // is an empty list rather than an error the user can do anything about.
    if res.status().as_u16() == 404 {
      return Ok(Vec::new());
    }
    let page: Page<ApiIssue> = http::check(res, HOST, ERROR_KEYS)
      .await?
      .json()
      .await
      .map_err(|e| AppError::Other(format!("bad response from {HOST}: {e}")))?;
    Ok(page.values.into_iter().map(Into::into).collect())
  }

  async fn pr_detail(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<PrDetail, AppError> {
    let path = format!("{}/pullrequests/{number}", self.repo_path(slug));
    let pr: ApiPr =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    let comments = self.comments(app, slug, "pullrequests", number).await?;
    let merged = pr.state == "MERGED";
    Ok(PrDetail {
      number: pr.id,
      title: pr.title,
      // The body is `summary.raw`; there is no `description` field.
      body: pr.summary.map(|s| s.raw).unwrap_or_default(),
      author_is_bot: false,
      author: pr.author.map(|a| a.display_name).unwrap_or_default(),
      state: pr.state,
      draft: pr.draft,
      merged,
      // Bitbucket exposes no mergeability check on the PR resource.
      mergeable: None,
      head_ref: pr.source.and_then(|s| s.branch).map(|b| b.name).unwrap_or_default(),
      base_ref: pr
        .destination
        .and_then(|d| d.branch)
        .map(|b| b.name)
        .unwrap_or_default(),
      additions: None,
      deletions: None,
      changed_files: None,
      comments,
      html_url: pr.links.html_href(),
      created_at: pr.created_on,
      updated_at: Some(pr.updated_on),
    })
  }

  async fn issue_detail(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<IssueDetail, AppError> {
    let path = format!("{}/issues/{number}", self.repo_path(slug));
    let issue: ApiIssue =
      http::send_json(self.request(app, reqwest::Method::GET, &path)?, HOST, ERROR_KEYS).await?;
    let comments = self.comments(app, slug, "issues", number).await?;
    Ok(IssueDetail {
      number: issue.id,
      title: issue.title,
      body: issue.content.map(|c| c.raw).unwrap_or_default(),
      author: issue.reporter.map(|r| r.display_name).unwrap_or_default(),
      state: issue.state,
      // Bitbucket has `kind` and `priority` rather than free-form labels.
      labels: [issue.kind, issue.priority]
        .into_iter()
        .flatten()
        .collect(),
      assignee: issue.assignee.map(|a| a.display_name),
      comments,
      html_url: issue.links.html_href(),
      created_at: issue.created_on,
      updated_at: Some(issue.updated_on),
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
    let kind = if is_pr { "pullrequests" } else { "issues" };
    let path = format!("{}/{kind}/{number}/comments", self.repo_path(slug));
    let created: ApiComment = http::send_json(
      self
        .request(app, reqwest::Method::POST, &path)?
        // Only `raw` is accepted; sending html or markup is rejected.
        .json(&serde_json::json!({ "content": { "raw": body } })),
      HOST,
      ERROR_KEYS,
    )
    .await?;
    Ok(HostComment {
      author_is_bot: false,
      author: created.user.map(|u| u.display_name).unwrap_or_default(),
      body: created.content.map(|c| c.raw).unwrap_or_default(),
      created_at: created.created_on,
    })
  }

  async fn approve_pr(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<(), AppError> {
    let path = format!("{}/pullrequests/{number}/approve", self.repo_path(slug));
    // Deliberately no body: Bitbucket rejects one here.
    http::send(self.request(app, reqwest::Method::POST, &path)?, HOST, ERROR_KEYS).await?;
    Ok(())
  }

  async fn merge_pr(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
    method: MergeMethod,
  ) -> Result<(), AppError> {
    let strategy = match method {
      MergeMethod::Merge => "merge_commit",
      MergeMethod::Squash => "squash",
      MergeMethod::Rebase => "rebase_merge",
    };
    let path = format!("{}/pullrequests/{number}/merge", self.repo_path(slug));
    http::send(
      self.request(app, reqwest::Method::POST, &path)?.json(&serde_json::json!({
        "type": "pullrequest",
        "merge_strategy": strategy,
      })),
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
    let path = format!("{}/issues/{number}", self.repo_path(slug));
    http::send(
      self
        .request(app, reqwest::Method::PUT, &path)?
        .json(&serde_json::json!({ "state": "resolved" })),
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
struct Page<T> {
  #[serde(default = "Vec::new")]
  values: Vec<T>,
}

/// GDPR-era account shape: only `display_name` is guaranteed outside `/user`.
#[derive(Deserialize)]
struct ApiAccount {
  #[serde(default)]
  display_name: String,
}

#[derive(Deserialize)]
struct ApiContent {
  #[serde(default)]
  raw: String,
}

#[derive(Deserialize)]
struct ApiBranch {
  #[serde(default)]
  name: String,
}

#[derive(Deserialize)]
struct ApiEndpoint {
  #[serde(default)]
  branch: Option<ApiBranch>,
}

#[derive(Deserialize, Default)]
struct ApiLinks {
  #[serde(default)]
  html: Option<ApiLink>,
}

#[derive(Deserialize)]
struct ApiLink {
  #[serde(default)]
  href: String,
}

impl ApiLinks {
  fn html_href(self) -> String {
    self.html.map(|l| l.href).unwrap_or_default()
  }
}

#[derive(Deserialize)]
struct ApiPr {
  id: u32,
  title: String,
  state: String,
  #[serde(default)]
  draft: bool,
  /// The PR body. Named `summary`, not `description`.
  #[serde(default)]
  summary: Option<ApiContent>,
  #[serde(default)]
  author: Option<ApiAccount>,
  #[serde(default)]
  source: Option<ApiEndpoint>,
  #[serde(default)]
  destination: Option<ApiEndpoint>,
  #[serde(default)]
  links: ApiLinks,
  created_on: String,
  updated_on: String,
}

#[derive(Deserialize)]
struct ApiIssue {
  id: u32,
  title: String,
  state: String,
  #[serde(default)]
  content: Option<ApiContent>,
  #[serde(default)]
  reporter: Option<ApiAccount>,
  #[serde(default)]
  assignee: Option<ApiAccount>,
  #[serde(default)]
  kind: Option<String>,
  #[serde(default)]
  priority: Option<String>,
  #[serde(default)]
  links: ApiLinks,
  created_on: String,
  updated_on: String,
}

#[derive(Deserialize)]
struct ApiComment {
  #[serde(default)]
  user: Option<ApiAccount>,
  #[serde(default)]
  content: Option<ApiContent>,
  #[serde(default)]
  created_on: String,
  #[serde(default)]
  deleted: bool,
}

impl From<ApiPr> for PrSummary {
  fn from(p: ApiPr) -> Self {
    PrSummary {
      number: p.id,
      title: p.title,
      author_is_bot: false,
      author: p.author.map(|a| a.display_name).unwrap_or_default(),
      draft: p.draft,
      head_ref: p.source.and_then(|s| s.branch).map(|b| b.name).unwrap_or_default(),
      base_ref: p
        .destination
        .and_then(|d| d.branch)
        .map(|b| b.name)
        .unwrap_or_default(),
      updated_at: Some(p.updated_on),
      created_at: p.created_on,
      html_url: p.links.html_href(),
    }
  }
}

impl From<ApiIssue> for IssueSummary {
  fn from(i: ApiIssue) -> Self {
    IssueSummary {
      number: i.id,
      title: i.title,
      author: i.reporter.map(|r| r.display_name).unwrap_or_default(),
      labels: [i.kind, i.priority].into_iter().flatten().collect(),
      assignee: i.assignee.map(|a| a.display_name),
      comments: 0,
      updated_at: Some(i.updated_on),
      html_url: i.links.html_href(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_workspace_and_slug() {
    for url in [
      "git@bitbucket.org:team/app.git",
      "https://bitbucket.org/team/app.git",
      "ssh://git@bitbucket.org/team/app",
    ] {
      let slug = Bitbucket.slug_from_remote(url).expect(url);
      assert_eq!(slug.owner, "team", "{url}");
      assert_eq!(slug.repo, "app", "{url}");
    }
  }

  #[test]
  fn other_hosts_do_not_resolve() {
    assert!(Bitbucket
      .slug_from_remote("https://github.com/o/r.git")
      .is_none());
  }

  /// The body lives in `summary.raw`; a struct reading `description` silently
  /// produces empty PRs, which is the bug this guards.
  #[test]
  fn reads_pr_body_from_summary() {
    let json = serde_json::json!({
      "id": 7,
      "title": "Add thing",
      "state": "OPEN",
      "summary": { "raw": "the body" },
      "created_on": "2026-01-01T00:00:00Z",
      "updated_on": "2026-01-02T00:00:00Z",
    });
    let pr: ApiPr = serde_json::from_value(json).expect("parse");
    assert_eq!(pr.summary.map(|s| s.raw).as_deref(), Some("the body"));
  }

  /// Author objects outside `/user` carry only `display_name`; a missing
  /// nickname must not fail the parse.
  #[test]
  fn tolerates_gdpr_account_shape() {
    let json = serde_json::json!({
      "id": 1,
      "title": "t",
      "state": "OPEN",
      "author": { "display_name": "Ada L" },
      "created_on": "x",
      "updated_on": "y",
    });
    let pr: ApiPr = serde_json::from_value(json).expect("parse");
    let summary: PrSummary = pr.into();
    assert_eq!(summary.author, "Ada L");
  }
}
