//! Code hosts, behind one interface.
//!
//! GitWyrm talks to whatever site a repository's `origin` points at. Those sites
//! differ in almost every detail -- GitHub calls them pull requests and GitLab
//! calls them merge requests, GitHub numbers issues per repository and Azure
//! tracks work items per project, each has its own auth dance -- but the
//! questions GitWyrm asks are the same handful every time: who am I, which
//! project is this, what is open, and let me act on one.
//!
//! [`HostProvider`] is those questions. Everything host-specific lives behind
//! it, so the command layer, the query keys, and the UI never learn which site
//! they are talking to. Adding a host means writing one implementation and
//! listing it in the registry; no caller changes.
//!
//! The types crossing this boundary are deliberately the *union* of what hosts
//! offer, not the intersection. `PrSummary::draft` is meaningless on a host
//! without draft PRs and reports `false` there rather than the shape changing
//! per provider; `updated_at` is Optional because Azure DevOps genuinely has no
//! such field. A shape that changed per host would push the matching back into
//! every caller, which is the thing this module exists to prevent.
//!
//! **What is deliberately not uniform:** line counts (`additions`/`deletions`)
//! come back as None on every host but GitHub. Only GitHub reports them in the
//! PR payload; the others need per-file diff parsing (GitLab, Bitbucket) or
//! cannot produce them at all at reasonable cost (Azure). Showing nothing is
//! honest and free -- see `docs/hosting-providers.md`.

pub mod azure;
pub mod bitbucket;
pub mod github;
pub mod gitlab;
pub mod http;
pub mod registry;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;
use crate::git::remote_url::RemoteProvider;

pub use registry::{provider_for, provider_for_id, ProviderId, ALL_PROVIDERS};

/// Which repository on a host, resolved from a git remote.
///
/// Two segments covers GitHub (`owner/repo`), Bitbucket (`workspace/slug`) and
/// the common GitLab case. The awkward shapes -- GitLab subgroups and Azure's
/// org/project/repo triple -- are carried in `extra`, which each provider fills
/// and reads on its own terms. Callers pass the whole slug around opaquely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RepoSlug {
  pub owner: String,
  pub repo: String,
  /// Host-specific remainder. GitLab puts the full namespace path here (so
  /// subgroups survive), Azure puts the organization. Empty for the simple
  /// two-segment hosts.
  #[serde(default)]
  pub extra: Option<String>,
}

impl RepoSlug {
  pub fn new(owner: impl Into<String>, repo: impl Into<String>) -> Self {
    Self {
      owner: owner.into(),
      repo: repo.into(),
      extra: None,
    }
  }

  pub fn with_extra(mut self, extra: impl Into<String>) -> Self {
    self.extra = Some(extra.into());
    self
  }
}

/// How a host wants the user to sign in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
  /// User enters a short code on the host's site; we poll until they finish.
  DeviceCode,
  /// User pastes a token they created on the host.
  PersonalAccessToken,
  /// Bitbucket: an Atlassian account email plus an API token, sent together as
  /// Basic auth. App passwords were removed on 2026-07-28, so the email half is
  /// now required and cannot be inferred.
  EmailAndToken,
}

impl AuthKind {
  pub fn as_str(self) -> &'static str {
    match self {
      AuthKind::DeviceCode => "device_code",
      AuthKind::PersonalAccessToken => "personal_access_token",
      AuthKind::EmailAndToken => "email_and_token",
    }
  }
}

// ---------------------------------------------------------------------------
// Wire shapes shared by every provider

#[derive(Debug, Clone, Serialize, Type)]
pub struct PrSummary {
  pub number: u32,
  pub title: String,
  pub author: String,
  pub author_is_bot: bool,
  pub draft: bool,
  pub head_ref: String,
  pub base_ref: String,
  /// When the pull request last changed. None on Azure DevOps, whose API has no
  /// such field; the UI falls back to `created_at` there.
  pub updated_at: Option<String>,
  pub created_at: String,
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
  pub updated_at: Option<String>,
  pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct HostComment {
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
  /// None on every host but GitHub; see the module docs.
  pub additions: Option<u32>,
  pub deletions: Option<u32>,
  pub changed_files: Option<u32>,
  pub comments: Vec<HostComment>,
  pub html_url: String,
  pub created_at: String,
  pub updated_at: Option<String>,
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
  pub comments: Vec<HostComment>,
  pub html_url: String,
  pub created_at: String,
  pub updated_at: Option<String>,
}

/// How a merge should be performed. Hosts support different subsets; each
/// implementation maps these onto its own vocabulary and errors clearly when
/// the host cannot honour the choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MergeMethod {
  Merge,
  Squash,
  Rebase,
}

/// What a host can and cannot do, so the UI hides controls that would fail
/// rather than showing buttons that error on click.
#[derive(Debug, Clone, Copy, Serialize, Type)]
pub struct HostCapabilities {
  /// Whether the host has an issue tracker GitWyrm reads. False for Azure
  /// DevOps, whose work items are defined per-project by the process template.
  pub issues: bool,
  /// Whether a merge method can be chosen per merge. False on GitLab, where the
  /// strategy is a project setting and honouring a per-merge choice would mean
  /// silently rewriting project config.
  pub choose_merge_method: bool,
  /// Whether the host reports a last-updated time for pull requests.
  pub pr_updated_at: bool,
  /// Whether PR line counts are available.
  pub pr_line_counts: bool,
}

/// The credential a provider needs, as entered by the user.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct Credential {
  pub token: String,
  /// Only used by [`AuthKind::EmailAndToken`]; ignored elsewhere.
  #[serde(default)]
  pub email: Option<String>,
  /// Base URL for self-hosted installs, e.g. `https://gitlab.mycorp.com`.
  /// None means the provider's public host.
  #[serde(default)]
  pub base_url: Option<String>,
}

/// What one host can do.
///
/// Implementations are stateless: the credential is read from the shared
/// `auth.json` store on each call, keyed by [`HostProvider::id`]. That keeps the
/// trait object-safe and means a token change takes effect immediately rather
/// than needing a cached client to be rebuilt.
#[async_trait]
pub trait HostProvider: Send + Sync {
  /// Stable key for `auth.json` and for the frontend's provider list. Never
  /// changes once shipped: it is what a stored token is filed under, so
  /// renaming one silently signs everybody out.
  fn id(&self) -> ProviderId;

  /// Name shown to the user, e.g. "GitHub".
  fn display_name(&self) -> &'static str;

  /// The remote hosts this provider claims.
  fn matches(&self, provider: RemoteProvider) -> bool;

  /// How connecting works, so the UI knows which control to show.
  fn auth_kind(&self) -> AuthKind;

  /// Where the user creates a token, opened by the connect screen's help link.
  /// None for hosts that do not use tokens.
  fn token_url(&self) -> Option<&'static str>;

  /// Which permissions the user must tick when creating that token. Shown
  /// verbatim on the connect screen, because a token missing one scope fails
  /// later with an error that does not name the cause.
  fn required_scopes(&self) -> &'static [&'static str];

  fn capabilities(&self) -> HostCapabilities;

  /// Owner/repo for a remote this provider owns, or None when the URL belongs
  /// to another host or does not fit a shape this provider can address.
  fn slug_from_remote(&self, url: &str) -> Option<RepoSlug>;

  /// The signed-in account name, or None when the stored credential is missing
  /// or no longer valid.
  async fn auth_status(&self, app: &tauri::AppHandle) -> Result<Option<String>, AppError>;

  async fn list_prs(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
  ) -> Result<Vec<PrSummary>, AppError>;

  async fn list_issues(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
  ) -> Result<Vec<IssueSummary>, AppError>;

  async fn pr_detail(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<PrDetail, AppError>;

  async fn issue_detail(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<IssueDetail, AppError>;

  async fn comment(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
    is_pr: bool,
    body: &str,
  ) -> Result<HostComment, AppError>;

  async fn approve_pr(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<(), AppError>;

  async fn merge_pr(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
    method: MergeMethod,
  ) -> Result<(), AppError>;

  async fn close_issue(
    &self,
    app: &tauri::AppHandle,
    slug: &RepoSlug,
    number: u32,
  ) -> Result<(), AppError>;
}

/// "Not signed in" phrased the same way for every host, since the fix is always
/// the same: connect it in Settings > Integrations.
pub fn not_connected(host: &str) -> AppError {
  AppError::Other(format!("not signed in to {host}; connect {host} first"))
}
