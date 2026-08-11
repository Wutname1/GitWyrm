//! Code hosts, behind one interface.
//!
//! GitWyrm talks to whatever site a repository's `origin` points at. Those sites
//! differ in almost every detail -- GitHub calls them pull requests and GitLab
//! calls them merge requests, GitHub numbers issues per repository and Azure
//! numbers work items per project, each has its own auth dance -- but the
//! questions GitWyrm asks are the same four every time: who am I, which project
//! is this, what is open, and let me act on one.
//!
//! [`HostProvider`] is those four questions. Everything host-specific lives
//! behind it, so the command layer, the query keys, and the UI never learn which
//! site they are talking to. Adding a host means writing one implementation and
//! listing it in [`provider_for`]; no caller changes.
//!
//! The types crossing this boundary are deliberately the *union* of what hosts
//! offer, not the intersection: `PrSummary::draft` is meaningless on a host
//! without draft PRs, and such a host reports `false` rather than the shape
//! changing per provider. A shape that changed per host would push the matching
//! back into every caller, which is the thing this module exists to prevent.

pub mod github;
pub mod registry;

use crate::error::AppError;
use crate::git::remote_url::RemoteProvider;

pub use registry::{provider_for, ProviderId, ALL_PROVIDERS};

/// Which repository on a host, resolved from a git remote.
///
/// Two segments, because that is what the current hosts' review APIs key on.
/// GitLab subgroups and Azure's org/project/repo triple do not fit this and are
/// exactly why [`HostProvider::slug_from_remote`] returns `Option` rather than
/// this type directly -- a host that cannot express its own project in two
/// segments must say so instead of guessing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSlug {
  pub owner: String,
  pub repo: String,
}

/// How a host wants the user to sign in.
///
/// Only the shapes GitWyrm can actually drive today. A host needing something
/// else -- a web redirect with a local callback listener, say -- adds a variant
/// here rather than inventing its own path, so the UI keeps one place to learn
/// what a connect button must render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
  /// User enters a short code on the host's site; we poll until they finish.
  DeviceCode,
  /// User pastes a personal access token they created on the host.
  PersonalAccessToken,
}

/// What one host can do. Implementations are stateless; the token comes from
/// the shared `auth.json` store keyed by [`HostProvider::id`].
///
/// Async methods are not used here because the trait is only a description --
/// the async work lives in free functions per provider, which keeps the trait
/// object-safe and dodges the async-trait dependency for what is currently a
/// four-method interface.
pub trait HostProvider: Send + Sync {
  /// Stable key for `auth.json` and for the frontend's provider list. Never
  /// changes once shipped: it is what a stored token is filed under, so
  /// renaming one silently signs everybody out.
  fn id(&self) -> ProviderId;

  /// Name shown to the user, e.g. "GitHub".
  fn display_name(&self) -> &'static str;

  /// The remote hosts this provider claims. Used to route a repository to its
  /// provider without the caller knowing the mapping.
  fn matches(&self, provider: RemoteProvider) -> bool;

  /// How connecting works, so the UI knows which control to show.
  fn auth_kind(&self) -> AuthKind;

  /// Whether this provider is wired up end to end. False means the UI lists it
  /// and explains it is not available yet, rather than offering a button that
  /// cannot work.
  ///
  /// This exists so the provider list is honest while hosts are added one at a
  /// time: a greyed row that says "not supported yet" is a true statement, and
  /// a connect button that errors is not.
  fn implemented(&self) -> bool {
    true
  }

  /// Owner/repo for a remote this provider owns, or None when the URL belongs
  /// to another host or does not fit the two-segment shape.
  fn slug_from_remote(&self, url: &str) -> Option<RepoSlug>;
}

/// Every host-specific failure funnels through here so messages read the same
/// whichever site produced them, and always name the site.
pub fn host_error(host: &str, message: impl std::fmt::Display) -> AppError {
  AppError::Other(format!("{host} said: {message}"))
}
