//! GitHub as a [`HostProvider`].
//!
//! The REST calls themselves still live in `commands::github`; this is the
//! description of GitHub that the registry routes on. Splitting it this way
//! keeps the working integration untouched while the seam is introduced -- the
//! commands module is what a second host will be refactored against, and doing
//! both at once would mean changing behaviour and structure in one step.

use super::registry::ProviderId;
use super::{AuthKind, HostProvider, RepoSlug};
use crate::git::remote_url::{self, RemoteProvider};

pub struct GitHub;

impl HostProvider for GitHub {
  fn id(&self) -> ProviderId {
    ProviderId::Github
  }

  fn display_name(&self) -> &'static str {
    "GitHub"
  }

  fn matches(&self, provider: RemoteProvider) -> bool {
    provider == RemoteProvider::GitHub
  }

  fn auth_kind(&self) -> AuthKind {
    AuthKind::DeviceCode
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
    Some(RepoSlug {
      owner: owner.to_string(),
      repo: repo.to_string(),
    })
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
    assert!(GitHub.slug_from_remote("https://github.mycorp.com/o/r.git").is_none());
  }

  #[test]
  fn other_hosts_do_not_resolve() {
    assert!(GitHub.slug_from_remote("https://gitlab.com/o/r.git").is_none());
  }
}
