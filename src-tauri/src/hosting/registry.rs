//! The list of known hosts, and how a repository finds its own.
//!
//! One table. Adding a host means adding a struct that implements
//! [`HostProvider`] and one line in [`ALL_PROVIDERS`]; routing, the settings
//! list, and the "which provider owns this repo" lookup all read from here.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::azure::AzureDevOps;
use super::bitbucket::Bitbucket;
use super::github::GitHub;
use super::gitlab::GitLab;
use super::HostProvider;
use crate::git::remote_url;

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

/// Every known host, in the order the settings list shows them.
pub const ALL_PROVIDERS: &[&dyn HostProvider] = &[&GitHub, &GitLab, &Bitbucket, &AzureDevOps];

/// The provider for an id.
pub fn provider_for_id(id: ProviderId) -> Option<&'static dyn HostProvider> {
    ALL_PROVIDERS.iter().copied().find(|p| p.id() == id)
}

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
    use crate::hosting::AuthKind;

    #[test]
    fn routes_remotes_to_their_host() {
        let cases = [
            ("git@github.com:o/r.git", ProviderId::Github),
            ("https://github.com/o/r.git", ProviderId::Github),
            ("https://gitlab.com/o/r.git", ProviderId::Gitlab),
            ("git@bitbucket.org:o/r.git", ProviderId::Bitbucket),
            (
                "https://dev.azure.com/org/proj/_git/r",
                ProviderId::AzureDevops,
            ),
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

    /// Every listed provider must resolve by id, or the settings screen shows a
    /// row whose connect button cannot find its own implementation.
    #[test]
    fn every_provider_resolves_by_id() {
        for provider in ALL_PROVIDERS {
            let found = provider_for_id(provider.id()).expect("resolves");
            assert_eq!(found.display_name(), provider.display_name());
        }
    }

    /// A host that says it has no issue tracker must not be asked for issues.
    /// Azure is the case: its work items are process-template-defined.
    #[test]
    fn azure_reports_no_issue_tracker() {
        let azure = provider_for_id(ProviderId::AzureDevops).expect("azure");
        assert!(!azure.capabilities().issues);
        // Everyone else does have one.
        for id in [
            ProviderId::Github,
            ProviderId::Gitlab,
            ProviderId::Bitbucket,
        ] {
            assert!(
                provider_for_id(id).expect("provider").capabilities().issues,
                "{id:?}"
            );
        }
    }

    /// Only GitHub reports line counts in its PR payload; the rest must say so
    /// rather than showing a confident zero.
    #[test]
    fn only_github_reports_line_counts() {
        for provider in ALL_PROVIDERS {
            let expected = provider.id() == ProviderId::Github;
            assert_eq!(
                provider.capabilities().pr_line_counts,
                expected,
                "{:?}",
                provider.id()
            );
        }
    }

    /// Only GitHub serves a pull request's files and commits so far. The rest keep
    /// the trait's empty default, and the flag is what hides those sections.
    #[test]
    fn only_github_reports_pr_contents() {
        for provider in ALL_PROVIDERS {
            let expected = provider.id() == ProviderId::Github;
            assert_eq!(
                provider.capabilities().pr_contents,
                expected,
                "{:?}",
                provider.id()
            );
        }
    }

    /// Every token-based host must tell the user where to make a token and which
    /// permissions to tick; a missing scope fails later without naming the cause.
    #[test]
    fn token_hosts_document_their_setup() {
        for provider in ALL_PROVIDERS {
            if provider.auth_kind() == AuthKind::DeviceCode {
                continue;
            }
            assert!(
                provider.token_url().is_some(),
                "{:?} has no token URL",
                provider.id()
            );
            assert!(
                !provider.required_scopes().is_empty(),
                "{:?} lists no scopes",
                provider.id()
            );
        }
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
            assert!(
                seen.insert(provider.id()),
                "duplicate id {:?}",
                provider.id()
            );
        }
    }
}
