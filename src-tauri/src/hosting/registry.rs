//! The list of known hosts, and how a repository finds its own.
//!
//! One table. Adding a host means adding a struct that implements
//! [`HostProvider`] and one line in [`ALL_PROVIDERS`]; routing, the settings
//! list, and the "which provider owns this repo" lookup all read from here.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::github::GitHub;
use super::{AuthKind, HostProvider};
use crate::git::remote_url::{self, RemoteProvider};

/// Stable identifier for a host, used as the `auth.json` key and in the UI.
///
/// Serialized in snake_case so the frontend's provider ids match these names
/// exactly; the GitHub value must stay `"github"` because tokens are already
/// stored under that key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
  Github,
  Gitlab,
  Bitbucket,
  AzureDevops,
}

impl ProviderId {
  /// The `auth.json` key. Spelled out rather than derived from the enum so a
  /// future rename of the variant cannot silently orphan stored tokens.
  pub fn as_str(self) -> &'static str {
    match self {
      ProviderId::Github => "github",
      ProviderId::Gitlab => "gitlab",
      ProviderId::Bitbucket => "bitbucket",
      ProviderId::AzureDevops => "azure_devops",
    }
  }
}

/// A host that GitWyrm knows the shape of but cannot talk to yet.
///
/// Listing these is deliberate. Someone whose work lives on GitLab should be
/// able to open Integrations and get a straight answer -- "known, not built
/// yet" -- instead of finding GitHub alone and having to guess whether their
/// host is unsupported or merely undiscovered.
struct Planned {
  id: ProviderId,
  display_name: &'static str,
  remote: RemoteProvider,
  auth_kind: AuthKind,
}

impl HostProvider for Planned {
  fn id(&self) -> ProviderId {
    self.id
  }
  fn display_name(&self) -> &'static str {
    self.display_name
  }
  fn matches(&self, provider: RemoteProvider) -> bool {
    provider == self.remote
  }
  fn auth_kind(&self) -> AuthKind {
    self.auth_kind
  }
  fn implemented(&self) -> bool {
    false
  }
  fn slug_from_remote(&self, _url: &str) -> Option<super::RepoSlug> {
    None
  }
}

const GITLAB: Planned = Planned {
  id: ProviderId::Gitlab,
  display_name: "GitLab",
  remote: RemoteProvider::GitLab,
  // GitLab's device flow is not enabled for arbitrary apps, so a token the user
  // creates themselves is the realistic path when this is built.
  auth_kind: AuthKind::PersonalAccessToken,
};

const BITBUCKET: Planned = Planned {
  id: ProviderId::Bitbucket,
  display_name: "Bitbucket",
  remote: RemoteProvider::Bitbucket,
  auth_kind: AuthKind::PersonalAccessToken,
};

const AZURE: Planned = Planned {
  id: ProviderId::AzureDevops,
  display_name: "Azure DevOps",
  remote: RemoteProvider::AzureDevOps,
  auth_kind: AuthKind::PersonalAccessToken,
};

/// Every known host, in the order the settings list shows them.
pub const ALL_PROVIDERS: &[&dyn HostProvider] = &[&GitHub, &GITLAB, &BITBUCKET, &AZURE];

/// The provider that owns a remote URL, or None when no known host claims it.
///
/// Returns planned-but-unbuilt providers too. Callers that need a working host
/// check [`HostProvider::implemented`]; callers that only need to name the host
/// (to explain why something is unavailable) do not.
pub fn provider_for(remote_url: &str) -> Option<&'static dyn HostProvider> {
  let parsed = remote_url::parse(remote_url)?;
  ALL_PROVIDERS
    .iter()
    .copied()
    .find(|p| p.matches(parsed.provider))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn routes_remotes_to_their_host() {
    let cases = [
      ("git@github.com:o/r.git", ProviderId::Github),
      ("https://github.com/o/r.git", ProviderId::Github),
      ("https://gitlab.com/o/r.git", ProviderId::Gitlab),
      ("git@bitbucket.org:o/r.git", ProviderId::Bitbucket),
      ("https://dev.azure.com/org/proj/_git/r", ProviderId::AzureDevops),
    ];
    for (url, expected) in cases {
      let provider = provider_for(url).unwrap_or_else(|| panic!("no provider for {url}"));
      assert_eq!(provider.id(), expected, "{url}");
    }
  }

  /// A host we have no integration for must not be claimed by one we do, or the
  /// UI would offer GitHub actions on someone's self-hosted Gitea.
  #[test]
  fn unknown_hosts_match_nothing() {
    assert!(provider_for("https://git.example.com/o/r.git").is_none());
    assert!(provider_for("not a url at all").is_none());
  }

  #[test]
  fn only_github_is_implemented_today() {
    let built: Vec<_> = ALL_PROVIDERS
      .iter()
      .filter(|p| p.implemented())
      .map(|p| p.id())
      .collect();
    assert_eq!(built, vec![ProviderId::Github]);
  }

  /// Auth keys are what stored tokens are filed under; changing one signs the
  /// affected users out silently.
  #[test]
  fn auth_keys_are_stable() {
    assert_eq!(ProviderId::Github.as_str(), "github");
    assert_eq!(ProviderId::Gitlab.as_str(), "gitlab");
    assert_eq!(ProviderId::Bitbucket.as_str(), "bitbucket");
    assert_eq!(ProviderId::AzureDevops.as_str(), "azure_devops");
  }

  #[test]
  fn ids_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for provider in ALL_PROVIDERS {
      assert!(seen.insert(provider.id()), "duplicate id {:?}", provider.id());
    }
  }
}
