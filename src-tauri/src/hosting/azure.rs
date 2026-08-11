//! Azure DevOps as a [`HostProvider`], over REST 7.1.
//!
//! The most distant of the four from GitHub's shape:
//!
//! 1. **No issues.** Azure has Work Items, whose types and states are defined
//!    per project by the process template -- `Issue` is the primary backlog item
//!    under Basic, a blocker tracker under Agile/CMMI, and does not exist at all
//!    under Scrum. There is no honest fixed mapping, so this reports
//!    `issues: false` and the issue list stays empty rather than showing
//!    something that means a different thing in every project.
//! 2. **No last-updated field on pull requests, and no sort parameter.** The PR
//!    object carries only `creationDate`, `closedDate` and
//!    `completionQueueTime`. Producing an "updated" time means a request per PR,
//!    so this reports `pr_updated_at: false` and orders newest-first by id,
//!    which is the API's own observed order.
//! 3. **A three-part coordinate** (organization / project / repository) against
//!    everyone else's two, spread across five remote URL shapes.
//! 4. **`api-version` is required on every single call.** Omitting it is an
//!    error, not a default.

use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;

use super::http::{self, encode_segment, TIMEOUT};
use super::registry::ProviderId;
use super::{
  not_connected, AuthKind, HostCapabilities, HostComment, HostProvider, IssueDetail, IssueSummary,
  MergeMethod, PrDetail, PrSummary, RepoSlug,
};
use crate::error::AppError;
use crate::git::remote_url::{self, RemoteProvider};

pub struct AzureDevOps;

const HOST: &str = "Azure DevOps";
const API_VERSION: &str = "7.1";
const ERROR_KEYS: &[&str] = &["message", "value"];

/// Azure's own name for a repository coordinate, unpacked from [`RepoSlug`]:
/// `extra` holds the organization, `owner` the project, `repo` the repository.
struct Coordinate {
  org: String,
  project: String,
  repo: String,
}

impl AzureDevOps {
  fn coordinate(&self, slug: &RepoSlug) -> Result<Coordinate, AppError> {
    let org = slug.extra.clone().ok_or_else(|| {
      AppError::Other("this Azure DevOps remote has no organization in its URL".into())
    })?;
    Ok(Coordinate {
      org,
      project: slug.owner.clone(),
      repo: slug.repo.clone(),
    })
  }

  fn request(
    &self,
    app: &tauri::AppHandle,
    method: reqwest::Method,
    url: String,
  ) -> Result<reqwest::RequestBuilder, AppError> {
    let cred = http::credential(app, ProviderId::AzureDevops)?.ok_or_else(|| not_connected(HOST))?;
    // Basic auth with an EMPTY username and the PAT as the password.
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!(":{}", cred.token));
    Ok(
      http::client()
        .request(method, url)
        .header("Authorization", format!("Basic {encoded}"))
        .header("User-Agent", "GitWyrm")
        .timeout(TIMEOUT),
    )
  }

  /// Builds a project-scoped Git API URL, appending the mandatory api-version.
  fn git_url(&self, app: &tauri::AppHandle, coord: &Coordinate, tail: &str) -> String {
    let base = self.base_url(app, &coord.org);
    let joiner = if tail.contains('?') { '&' } else { '?' };
    format!(
      "{base}/{}/_apis/git/repositories/{}{tail}{joiner}api-version={API_VERSION}",
      encode_segment(&coord.project),
      encode_segment(&coord.repo),
    )
  }

  /// The organization's API root. A stored base URL wins so on-prem Server
  /// installs (which sit behind a collection path, sometimes under `/tfs`)
  /// work; otherwise the cloud host.
  fn base_url(&self, app: &tauri::AppHandle, org: &str) -> String {
    http::credential(app, ProviderId::AzureDevops)
      .ok()
      .flatten()
      .and_then(|c| c.base_url)
      .map(|b| b.trim_end_matches('/').to_string())
      .unwrap_or_else(|| format!("https://dev.azure.com/{}", encode_segment(org)))
  }

  /// The browser URL for a pull request. Azure's `url` field is an API address,
  /// not a web one, so this is built rather than read.
  fn pr_web_url(&self, app: &tauri::AppHandle, coord: &Coordinate, id: u32) -> String {
    format!(
      "{}/{}/_git/{}/pullrequest/{id}",
      self.base_url(app, &coord.org),
      encode_segment(&coord.project),
      encode_segment(&coord.repo),
    )
  }

  /// Comment threads for a PR, flattened into the shared comment shape.
  ///
  /// Every vote, push and policy evaluation creates a system thread, and on a
  /// real pull request those outnumber human comments, so they are dropped.
  async fn threads(
    &self,
    app: &tauri::AppHandle,
    coord: &Coordinate,
    number: u32,
  ) -> Result<Vec<HostComment>, AppError> {
    let url = self.git_url(app, coord, &format!("/pullRequests/{number}/threads"));
    let page: Page<ApiThread> =
      http::send_json(self.request(app, reqwest::Method::GET, url)?, HOST, ERROR_KEYS).await?;
    let mut out = Vec::new();
    for thread in page.value {
      for comment in thread.comments {
        if comment.is_deleted || comment.comment_type.as_deref() == Some("system") {
          continue;
        }
        out.push(HostComment {
          author_is_bot: false,
          author: comment
            .author
            .map(|a| a.display_name)
            .unwrap_or_default(),
          body: comment.content.unwrap_or_default(),
          created_at: comment.published_date.unwrap_or_default(),
        });
      }
    }
    Ok(out)
  }
}

#[async_trait]
impl HostProvider for AzureDevOps {
  fn id(&self) -> ProviderId {
    ProviderId::AzureDevops
  }

  fn display_name(&self) -> &'static str {
    HOST
  }

  fn matches(&self, provider: RemoteProvider) -> bool {
    provider == RemoteProvider::AzureDevOps
  }

  fn auth_kind(&self) -> AuthKind {
    AuthKind::PersonalAccessToken
  }

  fn token_url(&self) -> Option<&'static str> {
    Some("https://dev.azure.com")
  }

  fn required_scopes(&self) -> &'static [&'static str] {
    &["Code (read and write)", "Pull Request Threads (read and write)"]
  }

  fn capabilities(&self) -> HostCapabilities {
    HostCapabilities {
      // Work items are process-template-defined; see the module docs.
      issues: false,
      choose_merge_method: true,
      pr_updated_at: false,
      pr_line_counts: false,
    }
  }

  /// Handles Azure's five remote shapes. The organization goes in `extra`, the
  /// project in `owner`, the repository in `repo`.
  fn slug_from_remote(&self, url: &str) -> Option<RepoSlug> {
    let parsed = remote_url::parse(url)?;
    if parsed.provider != RemoteProvider::AzureDevOps {
      return None;
    }
    let parts: Vec<&str> = parsed.path.split('/').filter(|s| !s.is_empty()).collect();

    // SSH: v3/{org}/{project}/{repo} -- no `_git` segment to anchor on.
    if parts.first().map(|p| p.eq_ignore_ascii_case("v3")) == Some(true) {
      if parts.len() >= 4 {
        return Some(RepoSlug::new(parts[2], parts[3..].join("/")).with_extra(parts[1]));
      }
      return None;
    }

    // HTTPS: the `_git` segment separates the project from the repository.
    let git_at = parts.iter().position(|p| *p == "_git")?;
    let repo = parts.get(git_at + 1)?;
    let before: Vec<&str> = parts[..git_at].to_vec();

    // `{org}.visualstudio.com/{project}/_git/{repo}` puts the org in the host.
    if let Some(org) = parsed.host.strip_suffix(".visualstudio.com") {
      // An optional DefaultCollection segment may precede the project.
      let project = before.last().copied().unwrap_or(repo);
      return Some(RepoSlug::new(project, *repo).with_extra(org));
    }

    // `dev.azure.com/{org}/{project}/_git/{repo}`, or `{org}/_git/{repo}` when
    // the project name matches the repository name.
    match before.as_slice() {
      [org, project, ..] => Some(RepoSlug::new(*project, *repo).with_extra(*org)),
      [org] => Some(RepoSlug::new(*repo, *repo).with_extra(*org)),
      [] => None,
    }
  }

  async fn auth_status(&self, app: &tauri::AppHandle) -> Result<Option<String>, AppError> {
    let Some(cred) = http::credential(app, ProviderId::AzureDevops)? else {
      return Ok(None);
    };
    // The profile API lives on a different host and is documented as not
    // accepting PATs, though it does in practice. `connectionData` is on the
    // org's own host and is what the CLI uses, so it is the reliable check.
    let Some(org) = cred.base_url.clone().or_else(|| {
      // Without a stored base URL there is no organization to ask about until a
      // repository names one, so a stored token is reported as connected.
      None
    }) else {
      return Ok(Some("connected".into()));
    };
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ConnectionData {
      authenticated_user: Option<AuthUser>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AuthUser {
      #[serde(default)]
      provider_display_name: String,
    }
    let url = format!(
      "{}/_apis/connectionData?api-version={API_VERSION}",
      org.trim_end_matches('/')
    );
    let res = self
      .request(app, reqwest::Method::GET, url)?
      .send()
      .await
      .map_err(|e| AppError::Other(format!("could not reach {HOST}: {e}")))?;
    if matches!(res.status().as_u16(), 401 | 403) {
      return Ok(None);
    }
    let data: ConnectionData = http::check(res, HOST, ERROR_KEYS)
      .await?
      .json()
      .await
      .map_err(|e| AppError::Other(format!("bad response from {HOST}: {e}")))?;
    Ok(Some(
      data
        .authenticated_user
        .map(|u| u.provider_display_name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "connected".into()),
    ))
  }

  /// Newest-first by id. Azure offers no sort parameter and no updated-at
  /// field, so this is the API's own order rather than a choice.
  async fn list_prs(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
  ) -> Result<Vec<PrSummary>, AppError> {
    let coord = self.coordinate(slug)?;
    let url = self.git_url(
      app,
      &coord,
      "/pullrequests?searchCriteria.status=active&$top=50",
    );
    let page: Page<ApiPr> =
      http::send_json(self.request(app, reqwest::Method::GET, url)?, HOST, ERROR_KEYS).await?;
    Ok(
      page
        .value
        .into_iter()
        .map(|pr| {
          let html_url = self.pr_web_url(app, &coord, pr.pull_request_id);
          pr.into_summary(html_url)
        })
        .collect(),
    )
  }

  /// Always empty: Azure has no issue tracker in GitHub's sense. See the module
  /// docs and `capabilities().issues == false`, which keeps the UI from asking.
  async fn list_issues(
    &self,
    _app: &tauri::AppHandle,
    _slug: &RepoSlug,
  ) -> Result<Vec<IssueSummary>, AppError> {
    Ok(Vec::new())
  }

  async fn pr_detail(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<PrDetail, AppError> {
    let coord = self.coordinate(slug)?;
    let url = self.git_url(app, &coord, &format!("/pullRequests/{number}"));
    let pr: ApiPr =
      http::send_json(self.request(app, reqwest::Method::GET, url)?, HOST, ERROR_KEYS).await?;
    let comments = self.threads(app, &coord, number).await?;
    let html_url = self.pr_web_url(app, &coord, pr.pull_request_id);
    Ok(PrDetail {
      number: pr.pull_request_id,
      title: pr.title.unwrap_or_default(),
      body: pr.description.unwrap_or_default(),
      author_is_bot: false,
      author: pr
        .created_by
        .as_ref()
        .map(|c| c.display_name.clone())
        .unwrap_or_default(),
      merged: pr.status.as_deref() == Some("completed"),
      state: pr.status.clone().unwrap_or_default(),
      draft: pr.is_draft,
      mergeable: pr.merge_status.as_deref().map(|s| s == "succeeded"),
      head_ref: strip_ref(pr.source_ref_name.as_deref()),
      base_ref: strip_ref(pr.target_ref_name.as_deref()),
      additions: None,
      deletions: None,
      changed_files: None,
      comments,
      html_url,
      created_at: pr.creation_date.unwrap_or_default(),
      updated_at: None,
    })
  }

  async fn issue_detail(
    &self,
    _app: &tauri::AppHandle,
    _slug: &RepoSlug,
    _number: u32,
  ) -> Result<IssueDetail, AppError> {
    Err(AppError::Other(
      "Azure DevOps work items are not shown in GitWyrm".into(),
    ))
  }

  async fn comment(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
    is_pr: bool,
    body: &str,
  ) -> Result<HostComment, AppError> {
    if !is_pr {
      return Err(AppError::Other(
        "Azure DevOps work items are not shown in GitWyrm".into(),
      ));
    }
    let coord = self.coordinate(slug)?;
    let url = self.git_url(app, &coord, &format!("/pullRequests/{number}/threads"));
    // Numeric enums on write, strings on read: commentType 1 is "text",
    // status 1 is "active".
    let created: ApiThread = http::send_json(
      self
        .request(app, reqwest::Method::POST, url)?
        .json(&serde_json::json!({
          "comments": [{ "parentCommentId": 0, "content": body, "commentType": 1 }],
          "status": 1,
        })),
      HOST,
      ERROR_KEYS,
    )
    .await?;
    let first = created.comments.into_iter().next();
    Ok(HostComment {
      author_is_bot: false,
      author: first
        .as_ref()
        .and_then(|c| c.author.as_ref())
        .map(|a| a.display_name.clone())
        .unwrap_or_default(),
      body: first
        .as_ref()
        .and_then(|c| c.content.clone())
        .unwrap_or_else(|| body.to_string()),
      created_at: first
        .and_then(|c| c.published_date)
        .unwrap_or_default(),
    })
  }

  /// Approving means casting a vote of 10 as yourself, which needs your own
  /// identity id in both the path and the body.
  async fn approve_pr(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<(), AppError> {
    let coord = self.coordinate(slug)?;
    let me = self.profile_id(app, &coord).await?;
    let url = self.git_url(
      app,
      &coord,
      &format!("/pullRequests/{number}/reviewers/{me}"),
    );
    http::send(
      self
        .request(app, reqwest::Method::PUT, url)?
        .json(&serde_json::json!({ "vote": 10, "id": me })),
      HOST,
      ERROR_KEYS,
    )
    .await?;
    Ok(())
  }

  /// Completion is queued rather than immediate, so a success here means
  /// "accepted", not "merged". The UI refreshes the PR afterwards, which is
  /// where the real outcome shows up.
  async fn merge_pr(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
    method: MergeMethod,
  ) -> Result<(), AppError> {
    let coord = self.coordinate(slug)?;
    // `lastMergeSourceCommit` is the optimistic-concurrency token and is
    // required, so the PR must be read immediately before completing it.
    let detail_url = self.git_url(app, &coord, &format!("/pullRequests/{number}"));
    let pr: ApiPr = http::send_json(
      self.request(app, reqwest::Method::GET, detail_url)?,
      HOST,
      ERROR_KEYS,
    )
    .await?;
    let commit = pr
      .last_merge_source_commit
      .map(|c| c.commit_id)
      .ok_or_else(|| {
        AppError::Other("Azure DevOps did not report the branch tip; try refreshing".into())
      })?;
    let strategy = match method {
      MergeMethod::Merge => "noFastForward",
      MergeMethod::Squash => "squash",
      MergeMethod::Rebase => "rebase",
    };
    let url = self.git_url(app, &coord, &format!("/pullrequests/{number}"));
    http::send(
      self
        .request(app, reqwest::Method::PATCH, url)?
        .json(&serde_json::json!({
          "status": "completed",
          "lastMergeSourceCommit": { "commitId": commit },
          "completionOptions": { "mergeStrategy": strategy },
        })),
      HOST,
      ERROR_KEYS,
    )
    .await?;
    Ok(())
  }

  async fn close_issue(
    &self,
    _app: &tauri::AppHandle,
    _slug: &RepoSlug,
    _number: u32,
  ) -> Result<(), AppError> {
    Err(AppError::Other(
      "Azure DevOps work items are not shown in GitWyrm".into(),
    ))
  }
}

impl AzureDevOps {
  /// The signed-in user's identity id, needed to vote on a pull request.
  async fn profile_id(
    &self,
    app: &tauri::AppHandle,
    coord: &Coordinate,
  ) -> Result<String, AppError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ConnectionData {
      authenticated_user: Option<IdOnly>,
    }
    #[derive(Deserialize)]
    struct IdOnly {
      id: String,
    }
    let url = format!(
      "{}/_apis/connectionData?api-version={API_VERSION}",
      self.base_url(app, &coord.org)
    );
    let data: ConnectionData =
      http::send_json(self.request(app, reqwest::Method::GET, url)?, HOST, ERROR_KEYS).await?;
    data
      .authenticated_user
      .map(|u| u.id)
      .ok_or_else(|| AppError::Other(format!("{HOST} did not say who you are signed in as")))
  }
}

/// `refs/heads/main` -> `main`.
fn strip_ref(value: Option<&str>) -> String {
  value
    .unwrap_or_default()
    .strip_prefix("refs/heads/")
    .unwrap_or(value.unwrap_or_default())
    .to_string()
}

// ---------------------------------------------------------------------------
// Response shapes

#[derive(Deserialize)]
struct Page<T> {
  #[serde(default = "Vec::new")]
  value: Vec<T>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ApiIdentity {
  #[serde(default)]
  display_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiCommitRef {
  #[serde(default)]
  commit_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiPr {
  pull_request_id: u32,
  #[serde(default)]
  title: Option<String>,
  #[serde(default)]
  description: Option<String>,
  #[serde(default)]
  created_by: Option<ApiIdentity>,
  /// In the schema but absent from sample responses, so it must default.
  #[serde(default)]
  is_draft: bool,
  #[serde(default)]
  status: Option<String>,
  #[serde(default)]
  merge_status: Option<String>,
  #[serde(default)]
  source_ref_name: Option<String>,
  #[serde(default)]
  target_ref_name: Option<String>,
  #[serde(default)]
  creation_date: Option<String>,
  #[serde(default)]
  last_merge_source_commit: Option<ApiCommitRef>,
}

impl ApiPr {
  fn into_summary(self, html_url: String) -> PrSummary {
    PrSummary {
      number: self.pull_request_id,
      title: self.title.unwrap_or_default(),
      author_is_bot: false,
      author: self.created_by.map(|c| c.display_name).unwrap_or_default(),
      draft: self.is_draft,
      head_ref: strip_ref(self.source_ref_name.as_deref()),
      base_ref: strip_ref(self.target_ref_name.as_deref()),
      // Azure has no updated timestamp; the UI shows created instead.
      updated_at: None,
      created_at: self.creation_date.unwrap_or_default(),
      html_url,
    }
  }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiThread {
  #[serde(default = "Vec::new")]
  comments: Vec<ApiThreadComment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiThreadComment {
  #[serde(default)]
  author: Option<ApiIdentity>,
  #[serde(default)]
  content: Option<String>,
  #[serde(default)]
  published_date: Option<String>,
  #[serde(default)]
  comment_type: Option<String>,
  #[serde(default)]
  is_deleted: bool,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_https_dev_azure_remote() {
    let slug = AzureDevOps
      .slug_from_remote("https://dev.azure.com/myorg/MyProject/_git/myrepo")
      .expect("https");
    assert_eq!(slug.extra.as_deref(), Some("myorg"));
    assert_eq!(slug.owner, "MyProject");
    assert_eq!(slug.repo, "myrepo");
  }

  #[test]
  fn parses_ssh_v3_remote() {
    let slug = AzureDevOps
      .slug_from_remote("git@ssh.dev.azure.com:v3/myorg/MyProject/myrepo")
      .expect("ssh");
    assert_eq!(slug.extra.as_deref(), Some("myorg"));
    assert_eq!(slug.owner, "MyProject");
    assert_eq!(slug.repo, "myrepo");
  }

  /// The legacy form puts the organization in the hostname.
  #[test]
  fn parses_legacy_visualstudio_remote() {
    let slug = AzureDevOps
      .slug_from_remote("https://myorg.visualstudio.com/MyProject/_git/myrepo")
      .expect("legacy");
    assert_eq!(slug.extra.as_deref(), Some("myorg"));
    assert_eq!(slug.owner, "MyProject");
    assert_eq!(slug.repo, "myrepo");
  }

  /// The project segment can be absent when it matches the repository name.
  #[test]
  fn parses_remote_without_project_segment() {
    let slug = AzureDevOps
      .slug_from_remote("https://dev.azure.com/myorg/_git/myrepo")
      .expect("no project");
    assert_eq!(slug.extra.as_deref(), Some("myorg"));
    assert_eq!(slug.owner, "myrepo");
    assert_eq!(slug.repo, "myrepo");
  }

  #[test]
  fn strips_ref_prefixes() {
    assert_eq!(strip_ref(Some("refs/heads/main")), "main");
    assert_eq!(strip_ref(Some("refs/heads/feature/a")), "feature/a");
    assert_eq!(strip_ref(None), "");
  }

  #[test]
  fn other_hosts_do_not_resolve() {
    assert!(AzureDevOps
      .slug_from_remote("https://github.com/o/r.git")
      .is_none());
  }

  /// isDraft is missing from real responses despite being in the schema.
  #[test]
  fn tolerates_missing_is_draft() {
    let json = serde_json::json!({ "pullRequestId": 3, "title": "t" });
    let pr: ApiPr = serde_json::from_value(json).expect("parse");
    assert!(!pr.is_draft);
  }
}
