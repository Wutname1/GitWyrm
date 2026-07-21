//! GitHub integration: sign-in, pull requests, and issues over the REST API.
//! Reuses the device-code flow and auth.json store from the AI subsystem;
//! the token lives under the "github" provider id and never reaches the
//! webview.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::time::Duration;
use tauri::State;

use crate::ai::{auth, copilot};
use crate::error::AppError;
use crate::state::RepoManager;

const PROVIDER_ID: &str = "github";
const API_BASE: &str = "https://api.github.com";
const TIMEOUT: Duration = Duration::from_secs(30);
/// Scopes: `repo` covers reading and acting on PRs/issues in private and
/// public repos; `read:user` lets us show who is signed in.
const SCOPE: &str = "repo read:user";

fn token(app: &tauri::AppHandle) -> Result<String, AppError> {
  match auth::get(app, PROVIDER_ID)? {
    Some(auth::AuthInfo::Oauth { access, .. }) => Ok(access),
    Some(auth::AuthInfo::Api { key }) => Ok(key),
    None => Err(AppError::Other(
      "not signed in to GitHub; connect GitHub first".into(),
    )),
  }
}

fn api(app: &tauri::AppHandle, method: reqwest::Method, path: &str) -> Result<reqwest::RequestBuilder, AppError> {
  let token = token(app)?;
  Ok(
    reqwest::Client::new()
      .request(method, format!("{API_BASE}{path}"))
      .bearer_auth(token)
      .header("Accept", "application/vnd.github+json")
      .header("X-GitHub-Api-Version", "2022-11-28")
      .header("User-Agent", "GitWyrm")
      .timeout(TIMEOUT),
  )
}

/// Turns a non-2xx response into a readable error, preferring GitHub's own
/// `message` field over raw status codes.
async fn check(res: reqwest::Response) -> Result<reqwest::Response, AppError> {
  let status = res.status();
  if status.is_success() {
    return Ok(res);
  }
  let body = res.text().await.unwrap_or_default();
  let message = serde_json::from_str::<serde_json::Value>(&body)
    .ok()
    .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
    .unwrap_or_else(|| body.chars().take(200).collect());
  Err(AppError::Other(match status.as_u16() {
    401 => "GitHub sign-in is no longer valid; connect GitHub again".into(),
    403 if message.contains("rate limit") => "GitHub rate limit reached; try again in a few minutes".into(),
    _ => format!("GitHub said: {message}"),
  }))
}

async fn send(builder: reqwest::RequestBuilder) -> Result<reqwest::Response, AppError> {
  let res = builder
    .send()
    .await
    .map_err(|e| AppError::Other(format!("could not reach GitHub: {e}")))?;
  check(res).await
}

// ---------------------------------------------------------------------------
// Sign-in

#[tauri::command]
#[specta::specta]
pub async fn github_device_start() -> Result<copilot::DeviceCodeInfo, AppError> {
  copilot::device_start(SCOPE).await
}

/// One poll pass; the frontend loops on Pending so sign-in stays cancellable.
#[tauri::command]
#[specta::specta]
pub async fn github_device_poll(
  app: tauri::AppHandle,
  device_code: String,
  interval: u32,
) -> Result<copilot::PollResult, AppError> {
  match copilot::device_poll(&device_code, interval).await? {
    copilot::PollOutcome::Token(token) => {
      auth::set(
        &app,
        PROVIDER_ID,
        auth::AuthInfo::Oauth {
          refresh: token.clone(),
          access: token,
          expires: 0,
          enterprise_url: None,
        },
      )?;
      Ok(copilot::PollResult::Complete)
    }
    copilot::PollOutcome::Pending { interval } => Ok(copilot::PollResult::Pending { interval }),
  }
}

#[tauri::command]
#[specta::specta]
pub fn github_sign_out(app: tauri::AppHandle) -> Result<(), AppError> {
  auth::remove(&app, PROVIDER_ID)
}

/// The signed-in login, or None when no token is stored or the token no
/// longer works (so the UI falls back to the connect prompt).
#[tauri::command]
#[specta::specta]
pub async fn github_auth_status(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
  if auth::get(&app, PROVIDER_ID)?.is_none() {
    return Ok(None);
  }
  #[derive(Deserialize)]
  struct User {
    login: String,
  }
  let res = api(&app, reqwest::Method::GET, "/user")?
    .send()
    .await
    .map_err(|e| AppError::Other(format!("could not reach GitHub: {e}")))?;
  if res.status().as_u16() == 401 {
    return Ok(None);
  }
  let user: User = check(res)
    .await?
    .json()
    .await
    .map_err(|e| AppError::Other(format!("bad response from GitHub: {e}")))?;
  Ok(Some(user.login))
}

// ---------------------------------------------------------------------------
// Repo detection

#[derive(Debug, Clone, Serialize, Type)]
pub struct GithubRepoRef {
  pub owner: String,
  pub repo: String,
}

/// The GitHub owner/repo behind the origin remote, or None when origin is
/// missing or not hosted on github.com.
#[tauri::command]
#[specta::specta]
pub async fn github_repo_slug(
  manager: State<'_, RepoManager>,
  repo_id: String,
) -> Result<Option<GithubRepoRef>, AppError> {
  let open = manager.get(&repo_id)?;
  tauri::async_runtime::spawn_blocking(move || {
    let repo = open.repo.lock().unwrap();
    let Ok(remote) = repo.find_remote("origin") else {
      return Ok(None);
    };
    let Some(url) = remote.url() else { return Ok(None) };
    Ok(parse_github_slug(url))
  })
  .await
  .map_err(|e| AppError::Other(e.to_string()))?
}

/// `git@github.com:o/r.git`, `ssh://git@github.com/o/r`, or
/// `https://github.com/o/r.git` -> owner/repo. Non-GitHub hosts return None.
fn parse_github_slug(url: &str) -> Option<GithubRepoRef> {
  let (host, path) = if let Some(rest) = url.strip_prefix("git@") {
    let (host, path) = rest.split_once(':')?;
    (host, path)
  } else if let Some(rest) = url.strip_prefix("ssh://git@") {
    rest.split_once('/')?
  } else if let Some(rest) = url.strip_prefix("https://") {
    let rest = rest.strip_prefix("git@").unwrap_or(rest);
    rest.split_once('/')?
  } else {
    return None;
  };
  if host != "github.com" {
    return None;
  }
  let path = path.trim_end_matches('/').trim_end_matches(".git");
  let (owner, repo) = path.split_once('/')?;
  if owner.is_empty() || repo.is_empty() || repo.contains('/') {
    return None;
  }
  Some(GithubRepoRef {
    owner: owner.to_string(),
    repo: repo.to_string(),
  })
}

// ---------------------------------------------------------------------------
// Response shapes (private, straight off the REST API)

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
struct ApiComment {
  user: ApiUser,
  #[serde(default)]
  body: Option<String>,
  created_at: String,
}

// ---------------------------------------------------------------------------
// Shapes sent to the webview

#[derive(Debug, Clone, Serialize, Type)]
pub struct PrSummary {
  pub number: u32,
  pub title: String,
  pub author: String,
  pub author_is_bot: bool,
  pub draft: bool,
  pub head_ref: String,
  pub base_ref: String,
  pub updated_at: String,
  pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct IssueSummary {
  pub number: u32,
  pub title: String,
  pub author: String,
  pub labels: Vec<String>,
  pub assignee: Option<String>,
  pub comments: u32,
  pub updated_at: String,
  pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct GithubComment {
  pub author: String,
  pub author_is_bot: bool,
  pub body: String,
  pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct PrDetail {
  pub number: u32,
  pub title: String,
  pub body: String,
  pub author: String,
  pub author_is_bot: bool,
  pub state: String,
  pub draft: bool,
  pub merged: bool,
  pub mergeable: Option<bool>,
  pub head_ref: String,
  pub base_ref: String,
  pub additions: u32,
  pub deletions: u32,
  pub changed_files: u32,
  pub comments: Vec<GithubComment>,
  pub html_url: String,
  pub created_at: String,
  pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct IssueDetail {
  pub number: u32,
  pub title: String,
  pub body: String,
  pub author: String,
  pub state: String,
  pub labels: Vec<String>,
  pub assignee: Option<String>,
  pub comments: Vec<GithubComment>,
  pub html_url: String,
  pub created_at: String,
  pub updated_at: String,
}

fn is_bot(user: &ApiUser) -> bool {
  user.kind == "Bot"
}

fn to_comment(c: ApiComment) -> GithubComment {
  GithubComment {
    author_is_bot: is_bot(&c.user),
    author: c.user.login,
    body: c.body.unwrap_or_default(),
    created_at: c.created_at,
  }
}

// ---------------------------------------------------------------------------
// Lists

#[tauri::command]
#[specta::specta]
pub async fn github_list_prs(
  app: tauri::AppHandle,
  owner: String,
  repo: String,
) -> Result<Vec<PrSummary>, AppError> {
  let path = format!("/repos/{owner}/{repo}/pulls?state=open&per_page=50&sort=updated&direction=desc");
  let prs: Vec<ApiPr> = send(api(&app, reqwest::Method::GET, &path)?)
    .await?
    .json()
    .await
    .map_err(|e| AppError::Other(format!("bad response from GitHub: {e}")))?;
  Ok(
    prs
      .into_iter()
      .map(|p| PrSummary {
        number: p.number,
        title: p.title,
        author_is_bot: is_bot(&p.user),
        author: p.user.login,
        draft: p.draft,
        head_ref: p.head.name,
        base_ref: p.base.name,
        updated_at: p.updated_at,
        html_url: p.html_url,
      })
      .collect(),
  )
}

#[tauri::command]
#[specta::specta]
pub async fn github_list_issues(
  app: tauri::AppHandle,
  owner: String,
  repo: String,
) -> Result<Vec<IssueSummary>, AppError> {
  let path = format!("/repos/{owner}/{repo}/issues?state=open&per_page=50&sort=updated&direction=desc");
  let issues: Vec<ApiIssue> = send(api(&app, reqwest::Method::GET, &path)?)
    .await?
    .json()
    .await
    .map_err(|e| AppError::Other(format!("bad response from GitHub: {e}")))?;
  Ok(
    issues
      .into_iter()
      // The issues endpoint also returns pull requests; keep real issues only.
      .filter(|i| i.pull_request.is_none())
      .map(|i| IssueSummary {
        number: i.number,
        title: i.title,
        author: i.user.login,
        labels: i.labels.into_iter().map(|l| l.name).collect(),
        assignee: i.assignee.map(|a| a.login),
        comments: i.comments,
        updated_at: i.updated_at,
        html_url: i.html_url,
      })
      .collect(),
  )
}

// ---------------------------------------------------------------------------
// Detail

async fn fetch_comments(
  app: &tauri::AppHandle,
  owner: &str,
  repo: &str,
  number: u32,
) -> Result<Vec<GithubComment>, AppError> {
  let path = format!("/repos/{owner}/{repo}/issues/{number}/comments?per_page=100");
  let comments: Vec<ApiComment> = send(api(app, reqwest::Method::GET, &path)?)
    .await?
    .json()
    .await
    .map_err(|e| AppError::Other(format!("bad response from GitHub: {e}")))?;
  Ok(comments.into_iter().map(to_comment).collect())
}

#[tauri::command]
#[specta::specta]
pub async fn github_pr_detail(
  app: tauri::AppHandle,
  owner: String,
  repo: String,
  number: u32,
) -> Result<PrDetail, AppError> {
  let path = format!("/repos/{owner}/{repo}/pulls/{number}");
  let pr: ApiPr = send(api(&app, reqwest::Method::GET, &path)?)
    .await?
    .json()
    .await
    .map_err(|e| AppError::Other(format!("bad response from GitHub: {e}")))?;
  let comments = fetch_comments(&app, &owner, &repo, number).await?;
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
    additions: pr.additions,
    deletions: pr.deletions,
    changed_files: pr.changed_files,
    comments,
    html_url: pr.html_url,
    created_at: pr.created_at,
    updated_at: pr.updated_at,
  })
}

#[tauri::command]
#[specta::specta]
pub async fn github_issue_detail(
  app: tauri::AppHandle,
  owner: String,
  repo: String,
  number: u32,
) -> Result<IssueDetail, AppError> {
  let path = format!("/repos/{owner}/{repo}/issues/{number}");
  let issue: ApiIssue = send(api(&app, reqwest::Method::GET, &path)?)
    .await?
    .json()
    .await
    .map_err(|e| AppError::Other(format!("bad response from GitHub: {e}")))?;
  let comments = fetch_comments(&app, &owner, &repo, number).await?;
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
    updated_at: issue.updated_at,
  })
}

// ---------------------------------------------------------------------------
// Actions

/// Posts a comment on an issue or pull request (GitHub uses the issues
/// endpoint for both) and returns it for optimistic display.
#[tauri::command]
#[specta::specta]
pub async fn github_comment(
  app: tauri::AppHandle,
  owner: String,
  repo: String,
  number: u32,
  body: String,
) -> Result<GithubComment, AppError> {
  let body = body.trim();
  if body.is_empty() {
    return Err(AppError::Other("write a reply first".into()));
  }
  let path = format!("/repos/{owner}/{repo}/issues/{number}/comments");
  let created: ApiComment = send(
    api(&app, reqwest::Method::POST, &path)?.json(&serde_json::json!({ "body": body })),
  )
  .await?
  .json()
  .await
  .map_err(|e| AppError::Other(format!("bad response from GitHub: {e}")))?;
  Ok(to_comment(created))
}

#[tauri::command]
#[specta::specta]
pub async fn github_approve_pr(
  app: tauri::AppHandle,
  owner: String,
  repo: String,
  number: u32,
) -> Result<(), AppError> {
  let path = format!("/repos/{owner}/{repo}/pulls/{number}/reviews");
  send(api(&app, reqwest::Method::POST, &path)?.json(&serde_json::json!({ "event": "APPROVE" })))
    .await?;
  Ok(())
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MergeMethod {
  Merge,
  Squash,
  Rebase,
}

#[tauri::command]
#[specta::specta]
pub async fn github_merge_pr(
  app: tauri::AppHandle,
  owner: String,
  repo: String,
  number: u32,
  method: MergeMethod,
) -> Result<(), AppError> {
  let method = match method {
    MergeMethod::Merge => "merge",
    MergeMethod::Squash => "squash",
    MergeMethod::Rebase => "rebase",
  };
  let path = format!("/repos/{owner}/{repo}/pulls/{number}/merge");
  send(
    api(&app, reqwest::Method::PUT, &path)?.json(&serde_json::json!({ "merge_method": method })),
  )
  .await?;
  Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn github_close_issue(
  app: tauri::AppHandle,
  owner: String,
  repo: String,
  number: u32,
) -> Result<(), AppError> {
  let path = format!("/repos/{owner}/{repo}/issues/{number}");
  send(api(&app, reqwest::Method::PATCH, &path)?.json(&serde_json::json!({ "state": "closed" })))
    .await?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::parse_github_slug;

  #[test]
  fn parses_common_remote_urls() {
    for url in [
      "git@github.com:owner/repo.git",
      "ssh://git@github.com/owner/repo",
      "https://github.com/owner/repo.git",
      "https://github.com/owner/repo/",
    ] {
      let slug = parse_github_slug(url).expect(url);
      assert_eq!(slug.owner, "owner");
      assert_eq!(slug.repo, "repo");
    }
  }

  #[test]
  fn rejects_non_github_hosts() {
    assert!(parse_github_slug("git@gitlab.com:owner/repo.git").is_none());
    assert!(parse_github_slug("https://example.com/owner/repo").is_none());
    assert!(parse_github_slug("/local/path").is_none());
  }
}
