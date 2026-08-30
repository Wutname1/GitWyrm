//! Code-host integration: sign-in, pull requests, and issues.
//!
//! Host-agnostic. Every command here resolves the repository's provider through
//! `hosting::registry` and dispatches to it, so this layer never names a
//! particular site. The commands keep their `github_*` names because they are
//! part of the frontend contract and renaming them all would be churn with no
//! user-visible gain -- the *behaviour* is whichever host the repository is on.
//!
//! Two things remain genuinely GitHub-only and say so: the device-code sign-in
//! (the other hosts use tokens) and the repository picker and SSH key pairing,
//! which have no equivalent elsewhere.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::time::Duration;
use tauri::State;

use crate::ai::{auth, copilot};
use crate::error::AppError;
use crate::hosting::{
    self, github::GitHub, http as host_http, Credential, HostProvider, ProviderId, RepoSlug,
};
use crate::state::RepoManager;

pub use crate::hosting::{
    HostComment as GithubComment, IssueDetail, IssueSummary, MergeMethod, PrCommit, PrDetail,
    PrFile, PrSummary,
};

const PROVIDER_ID: &str = "github";
const API_BASE: &str = "https://api.github.com";
const TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Provider resolution

/// The provider that owns a repository, plus the slug to address it with.
///
/// Both come from `origin`. A repository whose remote is missing, or on a host
/// GitWyrm has no integration for, has no provider -- callers treat that as
/// "nothing to show" rather than an error, because it is the normal state for a
/// local-only or self-hosted repository.
async fn resolve(
    manager: &State<'_, RepoManager>,
    repo_id: &str,
) -> Result<Option<(&'static dyn HostProvider, RepoSlug)>, AppError> {
    let open = manager.get(repo_id)?;
    let url = tauri::async_runtime::spawn_blocking(move || {
        let repo = open.repo.lock().unwrap();
        repo.find_remote("origin")
            .ok()
            .and_then(|r| r.url().ok().map(String::from))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?;

    let Some(url) = url else { return Ok(None) };
    let Some(provider) = hosting::provider_for(&url) else {
        return Ok(None);
    };
    Ok(provider.slug_from_remote(&url).map(|slug| (provider, slug)))
}

/// Looks up a provider by id for the commands that act on a named host rather
/// than on a repository (connect, disconnect, status).
fn by_id(id: ProviderId) -> Result<&'static dyn HostProvider, AppError> {
    hosting::provider_for_id(id).ok_or_else(|| AppError::Other("unknown host".into()))
}

// ---------------------------------------------------------------------------
// Sign-in

#[tauri::command]
#[specta::specta]
pub async fn github_device_start() -> Result<copilot::DeviceCodeInfo, AppError> {
    copilot::device_start(GitHub::SCOPE).await
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

/// Stores a token for a host that signs in with one, then verifies it works.
///
/// Verifying before reporting success is the point: a token with a missing
/// scope is accepted by the store but fails on the first real request, and an
/// error at that moment does not name the cause. Checking here lets the connect
/// screen say "that token did not work" while the user still has the token page
/// open.
#[tauri::command]
#[specta::specta]
pub async fn host_connect_token(
    app: tauri::AppHandle,
    provider: ProviderId,
    credential: Credential,
) -> Result<String, AppError> {
    let token = credential.token.trim();
    if token.is_empty() {
        return Err(AppError::Other("paste the token first".into()));
    }
    let host = by_id(provider)?;
    if host.auth_kind() == crate::hosting::AuthKind::EmailAndToken
        && credential
            .email
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(AppError::Other(
            "Bitbucket needs the email address of your Atlassian account as well as the token"
                .into(),
        ));
    }

    let packed = host_http::pack_credential(
        token,
        credential.email.as_deref(),
        credential.base_url.as_deref(),
    );
    auth::set(&app, provider.as_str(), auth::AuthInfo::Api { key: packed })?;
    // Before the verify below, not after: that call goes through `send`, so a
    // cooldown left from the old credential would refuse the new one without
    // asking the host, and the user could not reconnect until it aged out.
    host_http::clear_cooldown(host.display_name());

    match host.auth_status(&app).await {
        Ok(Some(login)) => Ok(login),
        // Stored fine but the host does not recognise it: remove it again rather
        // than leaving a credential behind that will fail every later request.
        Ok(None) => {
            let _ = auth::remove(&app, provider.as_str());
            Err(AppError::Other(format!(
        "{} did not accept that token. Check it was copied whole and has the permissions listed above.",
        host.display_name()
      )))
        }
        Err(e) => {
            let _ = auth::remove(&app, provider.as_str());
            Err(e)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn github_sign_out(app: tauri::AppHandle) -> Result<(), AppError> {
    host_sign_out(app, ProviderId::Github).await
}

#[tauri::command]
#[specta::specta]
pub async fn host_sign_out(app: tauri::AppHandle, provider: ProviderId) -> Result<(), AppError> {
    // Signing out clears the bench too, so connecting a different account is not
    // met with the previous one's refusal.
    if let Ok(host) = by_id(provider) {
        host_http::clear_cooldown(host.display_name());
    }
    tauri::async_runtime::spawn_blocking(move || auth::remove(&app, provider.as_str()))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
}

/// The signed-in GitHub login, or None when not connected.
#[tauri::command]
#[specta::specta]
pub async fn github_auth_status(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
    GitHub.auth_status(&app).await
}

// ---------------------------------------------------------------------------
// Provider list

/// One host as the Integrations screen shows it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct HostProviderInfo {
    pub id: ProviderId,
    pub display_name: String,
    /// "device_code", "personal_access_token" or "email_and_token": which connect
    /// control to show.
    pub auth_kind: String,
    /// Where the user creates a token, for the connect screen's help link.
    pub token_url: Option<String>,
    /// The permissions the token needs, listed verbatim so a user can tick them.
    pub required_scopes: Vec<String>,
    /// The signed-in account name, or None when not connected.
    pub connected_as: Option<String>,
    pub capabilities: crate::hosting::HostCapabilities,
}

/// Every host GitWyrm knows about, with the connection state of each.
///
/// Status is best-effort per host: one unreachable site must not blank the
/// whole screen, so a failed check reads as "not connected" and the user can
/// retry by reopening.
#[tauri::command]
#[specta::specta]
pub async fn hosting_providers(app: tauri::AppHandle) -> Result<Vec<HostProviderInfo>, AppError> {
    let mut out = Vec::with_capacity(hosting::ALL_PROVIDERS.len());
    for provider in hosting::ALL_PROVIDERS {
        out.push(HostProviderInfo {
            id: provider.id(),
            display_name: provider.display_name().to_string(),
            auth_kind: provider.auth_kind().as_str().to_string(),
            token_url: provider.token_url().map(String::from),
            required_scopes: provider
                .required_scopes()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            connected_as: provider.auth_status(&app).await.unwrap_or(None),
            capabilities: provider.capabilities(),
        });
    }
    Ok(out)
}

/// What the GitHub CLI fallback can currently do, for the settings row.
#[derive(Debug, Clone, Serialize, Type)]
pub struct GhCliStatus {
    /// `gh` was found on this machine.
    pub installed: bool,
    /// `gh` is installed and has a working login, so the fallback can be used.
    pub signed_in: bool,
}

/// Whether the GitHub CLI is installed and signed in.
///
/// Reported rather than inferred so the settings row can say which of the two
/// is missing: "install it" and "sign into it" are different next steps, and a
/// single "unavailable" would send half the users to the wrong one.
#[tauri::command]
#[specta::specta]
pub async fn gh_cli_status() -> Result<GhCliStatus, AppError> {
    // A PATH walk plus `gh auth status`, both of which touch the filesystem and
    // spawn a process; neither belongs on the IPC thread.
    tauri::async_runtime::spawn_blocking(|| {
        // Signing into or out of `gh` happens outside this process, so the
        // settings row must probe for real rather than read a cached answer.
        crate::hosting::gh_cli::invalidate_availability();
        match crate::hosting::gh_cli::availability() {
        Ok(_) => GhCliStatus {
            installed: true,
            signed_in: true,
        },
        Err(crate::hosting::gh_cli::Unavailable::NotSignedIn) => GhCliStatus {
            installed: true,
            signed_in: false,
        },
        Err(crate::hosting::gh_cli::Unavailable::NotInstalled) => GhCliStatus {
            installed: false,
            signed_in: false,
        },
        }
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))
}

/// Which host a repository's origin belongs to, or None when origin is missing
/// or points at a host GitWyrm does not know.
#[tauri::command]
#[specta::specta]
pub async fn repo_host_provider(
    manager: State<'_, RepoManager>,
    repo_id: String,
) -> Result<Option<ProviderId>, AppError> {
    Ok(resolve(&manager, &repo_id).await?.map(|(p, _)| p.id()))
}

// ---------------------------------------------------------------------------
// Repo detection

#[derive(Debug, Clone, Serialize, Type)]
pub struct GithubRepoRef {
    pub owner: String,
    pub repo: String,
}

/// The owner/repo behind origin, or None when the repository is not on a host
/// GitWyrm integrates with.
///
/// Named for GitHub because it predates the other hosts, but it resolves for
/// any of them -- the frontend uses it purely as "an addressable project", and
/// passes it straight back to the commands below.
#[tauri::command]
#[specta::specta]
pub async fn github_repo_slug(
    manager: State<'_, RepoManager>,
    repo_id: String,
) -> Result<Option<GithubRepoRef>, AppError> {
    Ok(resolve(&manager, &repo_id).await?.map(|(_, slug)| {
        GithubRepoRef {
            // GitLab subgroups and Azure's organization ride in `extra`; packing them
            // into `owner` keeps the wire shape two-part, and `slug_for` unpacks it.
            owner: slug.extra.clone().unwrap_or_else(|| slug.owner.clone()),
            repo: slug.repo,
        }
    }))
}

/// Rebuilds the internal slug from the two-part wire shape.
///
/// `owner` carries whatever `github_repo_slug` packed: a plain owner for GitHub
/// and Bitbucket, a full namespace path for GitLab, an organization for Azure.
/// Each provider reads the half it needs.
fn slug_for(provider: ProviderId, owner: &str, repo: &str) -> RepoSlug {
    match provider {
        // The namespace path is the whole address; the last segment is the project.
        ProviderId::Gitlab => {
            let short = owner.rsplit('/').next().unwrap_or(owner);
            RepoSlug::new(short, repo).with_extra(owner)
        }
        // `owner` is the organization; the project is carried alongside it.
        ProviderId::AzureDevops => {
            let (org, project) = owner.split_once('/').unwrap_or((owner, repo));
            RepoSlug::new(project, repo).with_extra(org)
        }
        _ => RepoSlug::new(owner, repo),
    }
}

/// Resolves the provider for a slug the frontend handed back.
///
/// The frontend does not know which host a slug belongs to, so this asks the
/// repository. Falls back to GitHub when nothing resolves, which keeps
/// already-open GitHub repos working if origin is briefly unreadable.
async fn provider_and_slug(
    manager: &State<'_, RepoManager>,
    repo_id: Option<&str>,
    owner: &str,
    repo: &str,
) -> Result<(&'static dyn HostProvider, RepoSlug), AppError> {
    if let Some(repo_id) = repo_id {
        if let Some((provider, _)) = resolve(manager, repo_id).await? {
            return Ok((provider, slug_for(provider.id(), owner, repo)));
        }
    }
    Ok((&GitHub, RepoSlug::new(owner, repo)))
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct GithubRepository {
    pub full_name: String,
    pub clone_url: String,
    pub html_url: String,
    pub description: Option<String>,
    pub private: bool,
    pub pushed_at: String,
    pub starred: bool,
}

#[derive(Deserialize)]
struct ApiRepository {
    full_name: String,
    clone_url: String,
    html_url: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    pushed_at: Option<String>,
    updated_at: String,
}

impl ApiRepository {
    fn into_repository(self, starred: bool) -> GithubRepository {
        GithubRepository {
            full_name: self.full_name,
            clone_url: self.clone_url,
            html_url: self.html_url,
            description: self.description,
            private: self.private,
            pushed_at: self.pushed_at.unwrap_or(self.updated_at),
            starred,
        }
    }
}

/// A signed-in GitHub request, for the two features that are GitHub-only.
fn gh_api(
    app: &tauri::AppHandle,
    method: reqwest::Method,
    path: &str,
) -> Result<reqwest::RequestBuilder, AppError> {
    let cred = host_http::credential(app, ProviderId::Github)?
        .ok_or_else(|| crate::hosting::not_connected("GitHub"))?;
    Ok(reqwest::Client::new()
        .request(method, format!("{API_BASE}{path}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "GitWyrm")
        .timeout(TIMEOUT)
        .bearer_auth(cred.token))
}

async fn gh_json<T: serde::de::DeserializeOwned>(
    builder: reqwest::RequestBuilder,
) -> Result<T, AppError> {
    host_http::send_json(builder, "GitHub", &["message", "errors"]).await
}

/// Repositories available to the signed-in account, with starred repositories
/// marked so the add screen can keep those shortcuts first.
///
/// GitHub-only: the repository picker has no equivalent on the other hosts yet.
#[tauri::command]
#[specta::specta]
pub async fn github_list_repositories(
    app: tauri::AppHandle,
) -> Result<Vec<GithubRepository>, AppError> {
    let repositories: Vec<ApiRepository> = gh_json(gh_api(
    &app,
    reqwest::Method::GET,
    "/user/repos?sort=pushed&direction=desc&per_page=50&affiliation=owner,collaborator,organization_member",
  )?)
  .await?;

    let starred: Vec<ApiRepository> = gh_json(gh_api(
        &app,
        reqwest::Method::GET,
        "/user/starred?sort=updated&direction=desc&per_page=50",
    )?)
    .await?;

    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(repositories.len() + starred.len());
    for repo in starred {
        seen.insert(repo.full_name.to_lowercase());
        result.push(repo.into_repository(true));
    }
    for repo in repositories {
        if seen.insert(repo.full_name.to_lowercase()) {
            result.push(repo.into_repository(false));
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Lists, detail and actions -- dispatched to whichever host owns the repository

#[tauri::command]
#[specta::specta]
pub async fn github_list_prs(
    app: tauri::AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
    owner: String,
    repo: String,
) -> Result<Vec<PrSummary>, AppError> {
    let (provider, slug) = provider_and_slug(&manager, repo_id.as_deref(), &owner, &repo).await?;
    provider.list_prs(&app, &slug).await
}

#[tauri::command]
#[specta::specta]
pub async fn github_list_issues(
    app: tauri::AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
    owner: String,
    repo: String,
) -> Result<Vec<IssueSummary>, AppError> {
    let (provider, slug) = provider_and_slug(&manager, repo_id.as_deref(), &owner, &repo).await?;
    // A host with no issue tracker answers empty rather than erroring: the panel
    // shows nothing, which is the truth, instead of a failure the user cannot fix.
    if !provider.capabilities().issues {
        return Ok(Vec::new());
    }
    provider.list_issues(&app, &slug).await
}

#[tauri::command]
#[specta::specta]
pub async fn github_pr_detail(
    app: tauri::AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
    owner: String,
    repo: String,
    number: u32,
) -> Result<PrDetail, AppError> {
    let (provider, slug) = provider_and_slug(&manager, repo_id.as_deref(), &owner, &repo).await?;
    provider.pr_detail(&app, &slug, number).await
}

/// The files a pull request changes, diffs included where the host sends them.
///
/// Separate from `github_pr_detail` on purpose: this is the expensive half of a
/// pull request -- one response can run to megabytes of patch text -- while the
/// conversation is small and read constantly. Splitting them lets the view show
/// the title and discussion immediately and stream the diffs in behind, and lets
/// the two be cached on different clocks.
///
/// Empty on hosts that do not serve this; see `HostCapabilities::pr_contents`.
#[tauri::command]
#[specta::specta]
pub async fn github_pr_files(
    app: tauri::AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
    owner: String,
    repo: String,
    number: u32,
) -> Result<Vec<PrFile>, AppError> {
    let (provider, slug) = provider_and_slug(&manager, repo_id.as_deref(), &owner, &repo).await?;
    provider.pr_files(&app, &slug, number).await
}

/// The commits on a pull request's branch. Empty on hosts without support.
#[tauri::command]
#[specta::specta]
pub async fn github_pr_commits(
    app: tauri::AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
    owner: String,
    repo: String,
    number: u32,
) -> Result<Vec<PrCommit>, AppError> {
    let (provider, slug) = provider_and_slug(&manager, repo_id.as_deref(), &owner, &repo).await?;
    provider.pr_commits(&app, &slug, number).await
}

#[tauri::command]
#[specta::specta]
pub async fn github_issue_detail(
    app: tauri::AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
    owner: String,
    repo: String,
    number: u32,
) -> Result<IssueDetail, AppError> {
    let (provider, slug) = provider_and_slug(&manager, repo_id.as_deref(), &owner, &repo).await?;
    provider.issue_detail(&app, &slug, number).await
}

/// Posts a reply on a pull request or an issue.
#[tauri::command]
#[specta::specta]
pub async fn github_comment(
    app: tauri::AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
    owner: String,
    repo: String,
    number: u32,
    body: String,
    is_pr: Option<bool>,
) -> Result<GithubComment, AppError> {
    let body = body.trim();
    if body.is_empty() {
        return Err(AppError::Other("write a reply first".into()));
    }
    let (provider, slug) = provider_and_slug(&manager, repo_id.as_deref(), &owner, &repo).await?;
    provider
        .comment(&app, &slug, number, is_pr.unwrap_or(true), body)
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn github_approve_pr(
    app: tauri::AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
    owner: String,
    repo: String,
    number: u32,
) -> Result<(), AppError> {
    let (provider, slug) = provider_and_slug(&manager, repo_id.as_deref(), &owner, &repo).await?;
    provider.approve_pr(&app, &slug, number).await
}

#[tauri::command]
#[specta::specta]
pub async fn github_merge_pr(
    app: tauri::AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
    owner: String,
    repo: String,
    number: u32,
    method: MergeMethod,
) -> Result<(), AppError> {
    let (provider, slug) = provider_and_slug(&manager, repo_id.as_deref(), &owner, &repo).await?;
    provider.merge_pr(&app, &slug, number, method).await
}

#[tauri::command]
#[specta::specta]
pub async fn github_close_pr(
    app: tauri::AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
    owner: String,
    repo: String,
    number: u32,
) -> Result<(), AppError> {
    let (provider, slug) = provider_and_slug(&manager, repo_id.as_deref(), &owner, &repo).await?;
    provider.close_pr(&app, &slug, number).await
}

#[tauri::command]
#[specta::specta]
pub async fn github_close_issue(
    app: tauri::AppHandle,
    manager: State<'_, RepoManager>,
    repo_id: Option<String>,
    owner: String,
    repo: String,
    number: u32,
) -> Result<(), AppError> {
    let (provider, slug) = provider_and_slug(&manager, repo_id.as_deref(), &owner, &repo).await?;
    provider.close_issue(&app, &slug, number).await
}

// ---------------------------------------------------------------------------
// SSH keys (GitHub only)

/// One local key paired with what GitHub knows about it, or a GitHub key with
/// no local counterpart.
///
/// Both halves are optional so the three real states are representable: a key
/// on both sides (matched), a local key GitHub has never seen (needs adding),
/// and a key registered from another computer (nothing to do, but worth
/// showing so "my key is missing" is never a mystery).
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyPairing {
    /// Fingerprint, always `SHA256:...`. The join key between the two sides.
    pub fingerprint: String,
    /// Absolute path to the local public key, or None when only GitHub has it.
    pub local_path: Option<String>,
    /// The title shown on GitHub, which is often the only real label a key has.
    pub github_title: Option<String>,
    /// GitHub's last-used date (`YYYY-MM-DD`), when it reports one. None means
    /// never used, which is a strong hint the key is dead weight.
    pub last_used: Option<String>,
}

#[derive(Deserialize)]
struct GithubKey {
    key_id: String,
    title: String,
    last_used: Option<String>,
}

/// GitHub reports fingerprints with the `SHA256:` prefix, same as ssh-keygen,
/// but normalise both sides so a change on either never silently stops
/// matching -- a wrong answer here reads as "your key is not on GitHub", which
/// would send someone off to re-upload a key that is already there.
fn fingerprint_key(raw: &str) -> String {
    raw.trim().trim_start_matches("SHA256:").to_owned()
}

/// Dates arrive as RFC 3339; only the day is worth showing.
fn day_only(stamp: &str) -> String {
    stamp.split('T').next().unwrap_or(stamp).to_owned()
}

/// Pairs local keys against GitHub's list, matched keys first.
///
/// Kept separate from the request so the three-way join is testable without a
/// network round trip.
fn pair_keys(local: Vec<crate::git::ssh::SshKey>, remote: Vec<GithubKey>) -> Vec<SshKeyPairing> {
    let mut paired: Vec<SshKeyPairing> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for key in local {
        let fp = fingerprint_key(&key.fingerprint);
        let found = remote.iter().find(|r| fingerprint_key(&r.key_id) == fp);
        seen.insert(fp);
        paired.push(SshKeyPairing {
            fingerprint: key.fingerprint.clone(),
            local_path: Some(key.path.clone()),
            github_title: found.map(|r| r.title.clone()),
            last_used: found.and_then(|r| r.last_used.as_deref()).map(day_only),
        });
    }

    for key in remote {
        if seen.contains(&fingerprint_key(&key.key_id)) {
            continue;
        }
        paired.push(SshKeyPairing {
            fingerprint: key.key_id.clone(),
            local_path: None,
            github_title: Some(key.title),
            last_used: key.last_used.as_deref().map(day_only),
        });
    }

    paired
}

/// Cross-references the SSH keys on this computer against the ones registered
/// on the signed-in GitHub account.
///
/// Needs no extra consent: `read:user` (already in GitHub's scope) implicitly
/// grants `read:public_key`, so anyone who connected GitHub for pull requests
/// can use this without signing in again.
#[tauri::command]
#[specta::specta]
pub async fn github_ssh_key_pairings(
    app: tauri::AppHandle,
) -> Result<Vec<SshKeyPairing>, AppError> {
    let local = tauri::async_runtime::spawn_blocking(crate::git::ssh::list_keys)
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    let remote: Vec<GithubKey> = gh_json(gh_api(&app, reqwest::Method::GET, "/user/keys")?).await?;
    Ok(pair_keys(local, remote))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(path: &str, fingerprint: &str) -> crate::git::ssh::SshKey {
        crate::git::ssh::SshKey {
            path: path.into(),
            name: path.into(),
            fingerprint: fingerprint.into(),
            comment: String::new(),
            algorithm: "ED25519".into(),
        }
    }

    fn remote(fingerprint: &str, title: &str, last_used: Option<&str>) -> GithubKey {
        GithubKey {
            key_id: fingerprint.into(),
            title: title.into(),
            last_used: last_used.map(String::from),
        }
    }

    #[test]
    fn a_local_key_on_github_is_matched_and_titled() {
        let out = pair_keys(
            vec![local("id_ed25519.pub", "SHA256:abc")],
            vec![remote(
                "SHA256:abc",
                "Work laptop",
                Some("2026-08-01T10:00:00Z"),
            )],
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].github_title.as_deref(), Some("Work laptop"));
        // The title is the whole point: it is often the only real label a key has.
        assert_eq!(out[0].last_used.as_deref(), Some("2026-08-01"));
    }

    #[test]
    fn a_local_key_absent_from_github_has_no_title() {
        let out = pair_keys(vec![local("id_ed25519.pub", "SHA256:abc")], vec![]);
        assert_eq!(out.len(), 1);
        assert!(out[0].github_title.is_none());
        assert!(out[0].local_path.is_some());
    }

    #[test]
    fn a_github_key_with_no_local_file_is_still_listed() {
        // Registered from another computer. Nothing to fix, but it has to show up
        // or "where did my key go" has no answer.
        let out = pair_keys(vec![], vec![remote("SHA256:xyz", "Old desktop", None)]);
        assert_eq!(out.len(), 1);
        assert!(out[0].local_path.is_none());
        assert_eq!(out[0].github_title.as_deref(), Some("Old desktop"));
        assert!(out[0].last_used.is_none());
    }

    #[test]
    fn matching_ignores_the_sha256_prefix_on_either_side() {
        // A prefix mismatch would read as "not on GitHub" and send someone off to
        // re-upload a key that is already there.
        let out = pair_keys(
            vec![local("id_ed25519.pub", "SHA256:abc")],
            vec![remote("abc", "Bare", None)],
        );
        assert_eq!(out.len(), 1, "should pair, not produce two rows");
        assert_eq!(out[0].github_title.as_deref(), Some("Bare"));
    }

    #[test]
    fn local_keys_come_before_github_only_ones() {
        let out = pair_keys(
            vec![local("a.pub", "SHA256:a")],
            vec![remote("SHA256:z", "Elsewhere", None)],
        );
        assert_eq!(out.len(), 2);
        assert!(out[0].local_path.is_some());
        assert!(out[1].local_path.is_none());
    }

    /// The wire slug is two-part, but GitLab subgroups and Azure organizations
    /// need more than that. Packing into `owner` and unpacking here is what keeps
    /// the frontend contract unchanged; a break shows up as requests to the wrong
    /// project.
    #[test]
    fn unpacks_gitlab_subgroup_slugs() {
        let slug = slug_for(ProviderId::Gitlab, "group/sub/proj", "proj");
        assert_eq!(slug.extra.as_deref(), Some("group/sub/proj"));
        assert_eq!(slug.repo, "proj");
    }

    #[test]
    fn unpacks_azure_org_slugs() {
        let slug = slug_for(ProviderId::AzureDevops, "myorg/MyProject", "myrepo");
        assert_eq!(slug.extra.as_deref(), Some("myorg"));
        assert_eq!(slug.owner, "MyProject");
        assert_eq!(slug.repo, "myrepo");
    }

    #[test]
    fn passes_simple_slugs_through() {
        let slug = slug_for(ProviderId::Github, "owner", "repo");
        assert_eq!(slug.owner, "owner");
        assert_eq!(slug.repo, "repo");
        assert!(slug.extra.is_none());
    }
}

// ---------------------------------------------------------------------------
// Library scan (repository picker)

/// Open pull request and issue counts for one repository on disk.
///
/// `path` is echoed back exactly as it came in so the picker can match the row
/// it asked about without re-normalising anything.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RepoActivityCount {
    pub path: String,
    pub prs: u32,
    pub issues: u32,
    /// None when the repository has no remote on a host GitWyrm integrates
    /// with, or the host could not be reached. Distinct from a count of zero,
    /// which means the host answered and there is genuinely nothing open.
    pub checked: bool,
}

/// Counts open pull requests and issues for a list of repositories on disk.
///
/// Reads `origin` straight off each path rather than going through RepoManager:
/// the picker's repositories are closed, so there is no open handle to ask.
/// A repository that is not on a known host, or whose host errors, comes back
/// with `checked: false` instead of failing the whole scan -- one unreachable
/// remote must not cost the user every other count.
#[tauri::command]
#[specta::specta]
pub async fn github_scan_repos(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<Vec<RepoActivityCount>, AppError> {
    // Six at a time: enough that a folder of twenty repositories finishes in a
    // few seconds, few enough that the host does not see a burst it rate-limits.
    const AT_ONCE: usize = 6;
    let mut out = Vec::with_capacity(paths.len());
    for chunk in paths.chunks(AT_ONCE) {
        let mut set = tokio::task::JoinSet::new();
        for path in chunk {
            let app = app.clone();
            let path = path.clone();
            set.spawn(async move { count_one(&app, path).await });
        }
        while let Some(joined) = set.join_next().await {
            out.push(joined.map_err(|e| AppError::Other(e.to_string()))?);
        }
    }
    // JoinSet finishes out of order; the picker matches on path, but a stable
    // order keeps the result readable in logs and tests.
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// One repository's counts. Never fails: an unreadable repository, an unknown
/// host, or a host that errors all come back unchecked.
async fn count_one(app: &tauri::AppHandle, path: String) -> RepoActivityCount {
    let unchecked = |path: String| RepoActivityCount {
        path,
        prs: 0,
        issues: 0,
        checked: false,
    };

    let disk = path.clone();
    let url = tauri::async_runtime::spawn_blocking(move || {
        git2::Repository::open(&disk)
            .ok()
            .and_then(|repo| {
                repo.find_remote("origin")
                    .ok()?
                    .url()
                    .ok()
                    .map(String::from)
            })
    })
    .await
    .ok()
    .flatten();

    let Some(url) = url else { return unchecked(path) };
    let Some(provider) = hosting::provider_for(&url) else {
        return unchecked(path);
    };
    let Some(slug) = provider.slug_from_remote(&url) else {
        return unchecked(path);
    };

    let prs = match provider.list_prs(app, &slug).await {
        Ok(prs) => prs.len() as u32,
        Err(_) => return unchecked(path),
    };
    let issues = if provider.capabilities().issues {
        match provider.list_issues(app, &slug).await {
            Ok(issues) => issues.len() as u32,
            // Issues need a connected account on most hosts; a signed-out user
            // still gets their pull request counts rather than nothing.
            Err(_) => 0,
        }
    } else {
        0
    };
    RepoActivityCount {
        path,
        prs,
        issues,
        checked: true,
    }
}
