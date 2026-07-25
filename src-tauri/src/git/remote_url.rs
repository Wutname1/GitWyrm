//! The single place a git remote URL is turned into structure.
//!
//! Every caller that needs a host, an owner/repo pair, or a browser URL for a
//! remote goes through here. Three separate parsers used to exist (one for the
//! GitHub API, one for "open commit on web", one in the frontend) and they
//! disagreed: an enterprise host resolved in one and not the others, Azure
//! DevOps worked in one only, and an explicit ssh port broke all three
//! differently. Keeping the shapes in one module is what stops that drift.
//!
//! Supported URL shapes:
//!   - `https://host/owner/repo(.git)` (and `http://`, and an embedded `user@`)
//!   - `ssh://git@host(:port)/owner/repo(.git)`
//!   - `git@host:owner/repo(.git)` (scp-like)
//!   - `git://host/owner/repo(.git)`

use serde::Serialize;
use specta::Type;

/// The hosting product behind a remote. `SelfHosted` means the URL parsed fine
/// but the host isn't one we can build provider-specific deep links for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RemoteProvider {
  GitHub,
  GitLab,
  Bitbucket,
  AzureDevOps,
  Gitea,
  SourceHut,
  CodeCommit,
  SelfHosted,
}

/// A parsed remote. `owner` and `repo` are only set when the path actually has
/// that two-part shape; deep-nested paths (GitLab subgroups, Azure) keep the
/// full path instead and callers use `web_base`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRemote {
  /// Host without credentials or port, lowercased.
  pub host: String,
  /// Repository path with any `.git` suffix and surrounding slashes removed.
  pub path: String,
  pub provider: RemoteProvider,
  /// Scheme for the browser URL. Only an explicit `http://` stays http.
  pub secure: bool,
}

impl ParsedRemote {
  /// `owner/repo` split, when the path is exactly two segments. GitLab
  /// subgroups and Azure paths deliberately return None.
  pub fn owner_repo(&self) -> Option<(&str, &str)> {
    let (owner, repo) = self.path.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
      return None;
    }
    Some((owner, repo))
  }

  /// The repository's page in a browser.
  pub fn web_base(&self) -> String {
    let scheme = if self.secure { "https" } else { "http" };
    if self.provider == RemoteProvider::AzureDevOps {
      if let Some(base) = self.azure_web_base(scheme) {
        return base;
      }
    }
    format!("{scheme}://{}/{}", self.host, self.path)
  }

  /// Azure's ssh paths (`v3/org/project/repo`) don't match its web routes
  /// (`org/project/_git/repo`), and the legacy visualstudio.com form puts the
  /// org in the hostname.
  fn azure_web_base(&self, scheme: &str) -> Option<String> {
    let parts: Vec<&str> = self.path.split('/').filter(|s| !s.is_empty()).collect();
    let is_v3 = parts.first().map(|p| p.eq_ignore_ascii_case("v3")) == Some(true);

    if is_v3 && parts.len() >= 4 {
      let (org, project, repo) = (parts[1], parts[2], parts[3..].join("/"));
      if self.host == "ssh.dev.azure.com" {
        return Some(format!("https://dev.azure.com/{org}/{project}/_git/{repo}"));
      }
      if self.host.ends_with("vs-ssh.visualstudio.com") {
        return Some(format!("https://{org}.visualstudio.com/{project}/_git/{repo}"));
      }
    }
    // Already a web-shaped path.
    Some(format!("{scheme}://{}/{}", self.host, parts.join("/")))
  }

  /// Provider-specific branch page, or None when the host has no known route.
  pub fn branch_url(&self, branch: &str) -> Option<String> {
    let base = self.web_base();
    let encoded = encode_path(branch);
    Some(match self.provider {
      RemoteProvider::GitHub | RemoteProvider::Gitea => format!("{base}/tree/{encoded}"),
      RemoteProvider::GitLab => format!("{base}/-/tree/{encoded}"),
      RemoteProvider::Bitbucket => format!("{base}/branch/{encoded}"),
      RemoteProvider::AzureDevOps => format!("{base}?version=GB{encoded}"),
      RemoteProvider::SourceHut => format!("{base}/log/{encoded}"),
      RemoteProvider::CodeCommit | RemoteProvider::SelfHosted => return None,
    })
  }

  /// Provider-specific page for one commit, or None when unknown.
  pub fn commit_url(&self, sha: &str) -> Option<String> {
    let base = self.web_base();
    Some(match self.provider {
      RemoteProvider::GitHub | RemoteProvider::Gitea => format!("{base}/commit/{sha}"),
      RemoteProvider::GitLab => format!("{base}/-/commit/{sha}"),
      RemoteProvider::Bitbucket => format!("{base}/commits/{sha}"),
      RemoteProvider::AzureDevOps => format!("{base}/commit/{sha}"),
      RemoteProvider::SourceHut => format!("{base}/commit/{sha}"),
      RemoteProvider::CodeCommit | RemoteProvider::SelfHosted => return None,
    })
  }
}

/// Percent-encode each path segment while keeping the separators, so a branch
/// like `feature/a b` survives in a URL.
fn encode_path(value: &str) -> String {
  value
    .split('/')
    .map(|seg| {
      seg
        .chars()
        .map(|c| match c {
          'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
          other => other
            .to_string()
            .bytes()
            .map(|b| format!("%{b:02X}"))
            .collect::<String>(),
        })
        .collect::<String>()
    })
    .collect::<Vec<_>>()
    .join("/")
}

/// Classify a host. Matches on a host component rather than a bare substring so
/// `github.myco.com` resolves to GitHub while `notgithub.example.com` does not
/// accidentally match on a random substring elsewhere in the URL.
fn provider_for_host(host: &str) -> RemoteProvider {
  let labels: Vec<&str> = host.split('.').collect();
  let has = |needle: &str| labels.iter().any(|l| *l == needle);

  if has("github") {
    return RemoteProvider::GitHub;
  }
  if has("gitlab") {
    return RemoteProvider::GitLab;
  }
  if has("bitbucket") {
    return RemoteProvider::Bitbucket;
  }
  if host == "dev.azure.com"
    || host == "ssh.dev.azure.com"
    || host.ends_with("visualstudio.com")
  {
    return RemoteProvider::AzureDevOps;
  }
  if host == "codeberg.org" || has("gitea") || has("forgejo") || host == "codeberg.org" {
    return RemoteProvider::Gitea;
  }
  if host == "git.sr.ht" || host.ends_with(".sr.ht") {
    return RemoteProvider::SourceHut;
  }
  // git-codecommit.<region>.amazonaws.com
  if host.starts_with("git-codecommit.") && host.ends_with("amazonaws.com") {
    return RemoteProvider::CodeCommit;
  }
  RemoteProvider::SelfHosted
}

/// Strip credentials and a port from an authority, returning the bare host.
fn clean_host(authority: &str) -> Option<String> {
  let after_credentials = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
  // Reject an IPv6 literal rather than mangling it; no known host needs it.
  if after_credentials.starts_with('[') {
    return None;
  }
  let host = after_credentials
    .split_once(':')
    .map_or(after_credentials, |(h, _)| h);
  if host.is_empty() {
    return None;
  }
  Some(host.to_ascii_lowercase())
}

/// Trim slashes and one trailing `.git` from a repository path.
fn clean_path(path: &str) -> String {
  let trimmed = path.trim_matches('/');
  // strip_suffix, not trim_end_matches: a repo really named `x.git.git` must
  // keep one suffix rather than losing both.
  let without_git = trimmed
    .strip_suffix(".git")
    .or_else(|| trimmed.strip_suffix(".GIT"))
    .unwrap_or(trimmed);
  without_git.trim_matches('/').to_string()
}

/// Parse any supported remote URL shape. Returns None for local paths, unknown
/// schemes, and anything without both a host and a repository path.
pub fn parse(url: &str) -> Option<ParsedRemote> {
  let value = url.trim();
  if value.is_empty() {
    return None;
  }

  // A Windows drive path (C:\src\repo) is a local remote, not a URL. Checked
  // before the scp branch, which would otherwise read `C` as the host.
  let bytes = value.as_bytes();
  if bytes.len() > 2 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/') {
    return None;
  }
  if value.starts_with('/') || value.starts_with('.') || value.starts_with("file://") {
    return None;
  }

  let (authority, path, secure) = if let Some(rest) = value.split_once("://") {
    let (scheme, remainder) = rest;
    let secure = match scheme.to_ascii_lowercase().as_str() {
      "https" | "ssh" | "git" => true,
      "http" => false,
      _ => return None,
    };
    let (authority, path) = remainder.split_once('/')?;
    (authority, path, secure)
  } else if let Some(rest) = value.split_once(':') {
    // scp-like `user@host:path`. A single-character authority is a drive letter.
    let (authority, path) = rest;
    if authority.len() < 2 || authority.contains('/') {
      return None;
    }
    (authority, path, true)
  } else {
    return None;
  };

  let host = clean_host(authority)?;
  let path = clean_path(path);
  if path.is_empty() {
    return None;
  }

  Some(ParsedRemote { provider: provider_for_host(&host), host, path, secure })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn p(url: &str) -> ParsedRemote {
    parse(url).unwrap_or_else(|| panic!("expected {url} to parse"))
  }

  #[test]
  fn parses_every_supported_shape_to_the_same_result() {
    for url in [
      "https://github.com/owner/repo.git",
      "https://github.com/owner/repo",
      "git@github.com:owner/repo.git",
      "ssh://git@github.com/owner/repo.git",
      "git://github.com/owner/repo.git",
      "https://user@github.com/owner/repo.git",
      "  https://github.com/owner/repo.git/  ",
    ] {
      let r = p(url);
      assert_eq!(r.host, "github.com", "host for {url}");
      assert_eq!(r.path, "owner/repo", "path for {url}");
      assert_eq!(r.owner_repo(), Some(("owner", "repo")), "slug for {url}");
      assert_eq!(r.web_base(), "https://github.com/owner/repo", "base for {url}");
    }
  }

  /// The old parsers all broke on an explicit ssh port, each differently.
  #[test]
  fn an_explicit_port_is_dropped_from_the_host() {
    let r = p("ssh://git@github.com:22/owner/repo.git");
    assert_eq!(r.host, "github.com");
    assert_eq!(r.path, "owner/repo");
    assert_eq!(r.provider, RemoteProvider::GitHub);
  }

  /// Enterprise hosts used to resolve in the frontend but not the backend.
  #[test]
  fn enterprise_subdomains_keep_their_provider() {
    assert_eq!(p("git@github.myco.com:o/r.git").provider, RemoteProvider::GitHub);
    assert_eq!(p("https://gitlab.myco.com/o/r.git").provider, RemoteProvider::GitLab);
    assert_eq!(p("https://github.myco.com/o/r").web_base(), "https://github.myco.com/o/r");
  }

  #[test]
  fn unrelated_hosts_are_self_hosted_not_misdetected() {
    assert_eq!(p("https://git.example.com/o/r.git").provider, RemoteProvider::SelfHosted);
    // "github" appearing in the path must not classify the host.
    assert_eq!(p("https://git.example.com/github/r.git").provider, RemoteProvider::SelfHosted);
  }

  #[test]
  fn recognizes_the_other_major_hosts() {
    assert_eq!(p("git@bitbucket.org:o/r.git").provider, RemoteProvider::Bitbucket);
    assert_eq!(p("https://codeberg.org/o/r.git").provider, RemoteProvider::Gitea);
    assert_eq!(p("https://git.sr.ht/~user/repo").provider, RemoteProvider::SourceHut);
    assert_eq!(
      p("ssh://git-codecommit.us-east-1.amazonaws.com/v1/repos/r").provider,
      RemoteProvider::CodeCommit
    );
  }

  /// Azure's ssh path shape differs from its web route; this was TS-only before.
  #[test]
  fn azure_ssh_paths_map_to_web_routes() {
    assert_eq!(
      p("git@ssh.dev.azure.com:v3/org/project/repo").web_base(),
      "https://dev.azure.com/org/project/_git/repo"
    );
    assert_eq!(
      p("https://dev.azure.com/org/project/_git/repo").web_base(),
      "https://dev.azure.com/org/project/_git/repo"
    );
    assert_eq!(
      p("git@vs-ssh.visualstudio.com:v3/org/project/repo").web_base(),
      "https://org.visualstudio.com/project/_git/repo"
    );
  }

  #[test]
  fn gitlab_subgroups_keep_the_full_path_and_have_no_two_part_slug() {
    let r = p("https://gitlab.com/group/subgroup/repo.git");
    assert_eq!(r.path, "group/subgroup/repo");
    assert_eq!(r.owner_repo(), None);
    assert_eq!(r.web_base(), "https://gitlab.com/group/subgroup/repo");
  }

  #[test]
  fn only_one_dot_git_suffix_is_removed() {
    assert_eq!(p("https://github.com/o/my.git.git").path, "o/my.git");
  }

  #[test]
  fn local_paths_and_unknown_schemes_do_not_parse() {
    for url in [
      "C:\\src\\repo",
      "c:/src/repo",
      "/srv/git/repo.git",
      "./relative",
      "file:///srv/repo",
      "ftp://example.com/o/r",
      "https://github.com/",
      "",
      "   ",
    ] {
      assert!(parse(url).is_none(), "{url} should not parse");
    }
  }

  #[test]
  fn provider_deep_links_follow_each_host_route() {
    assert_eq!(
      p("https://github.com/o/r").branch_url("main").unwrap(),
      "https://github.com/o/r/tree/main"
    );
    assert_eq!(
      p("https://gitlab.com/o/r").branch_url("main").unwrap(),
      "https://gitlab.com/o/r/-/tree/main"
    );
    assert_eq!(
      p("https://bitbucket.org/o/r").commit_url("abc123").unwrap(),
      "https://bitbucket.org/o/r/commits/abc123"
    );
    // An unknown host has no route rather than a wrong guess.
    assert!(p("https://git.example.com/o/r").commit_url("abc").is_none());
  }

  #[test]
  fn branch_names_with_slashes_and_spaces_are_encoded_per_segment() {
    let r = p("https://github.com/o/r");
    assert_eq!(r.branch_url("feature/a b").unwrap(), "https://github.com/o/r/tree/feature/a%20b");
  }

  #[test]
  fn an_explicit_http_remote_stays_http() {
    let r = p("http://git.example.com/o/r.git");
    assert!(!r.secure);
    assert_eq!(r.web_base(), "http://git.example.com/o/r");
  }
}
