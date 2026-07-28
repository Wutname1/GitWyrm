//! Guards the one file GitWyrm edits that it does not own: `~/.ssh/config`.
//!
//! People keep real, load-bearing work in there - non-standard ports, proxy
//! jumps, per-host users, comments explaining why. Losing any of it to a
//! "helpful" rewrite would be far worse than the problem being solved, so this
//! exercises the rewriter against a config with all of those shapes at once.

use gitwyrm_lib::ssh_config_rewrite as rewrite;

/// A config with every shape that matters: a block with a key, a block with a
/// non-default port and NO key, an indented wildcard, bare blocks, comments.
const CONFIG: &str = "\
Host homeassistant.local
  HostName homeassistant.local
  User root
  IdentityFile ~/.ssh/homeassistant

# GitHub over 443 because 22 is blocked on this network
Host github.com
  Hostname ssh.github.com
  Port 443
  User git

Host *
    User root
    IdentityFile ~/.ssh/id_ed25519
    ServerAliveInterval 60

Host hstest
    HostName 198.23.238.47
";

#[test]
fn every_unrelated_line_survives_a_rewrite() {
  let out = rewrite(CONFIG, "github.com", "C:/Users/me/.ssh/work");

  for required in [
    "Host homeassistant.local",
    "IdentityFile ~/.ssh/homeassistant",
    "# GitHub over 443 because 22 is blocked on this network",
    "Hostname ssh.github.com",
    "Port 443",
    "User git",
    "ServerAliveInterval 60",
    "Host hstest",
    "HostName 198.23.238.47",
  ] {
    assert!(out.contains(required), "rewrite lost: {required}");
  }
}

#[test]
fn the_target_host_gains_the_key_inside_its_own_block() {
  let out = rewrite(CONFIG, "github.com", "C:/Users/me/.ssh/work");

  // The new IdentityFile must land between "Host github.com" and the next Host.
  let after_github = out.split("Host github.com").nth(1).expect("github block");
  let block = after_github.split("\nHost ").next().expect("block body");
  assert!(
    block.contains("IdentityFile C:/Users/me/.ssh/work"),
    "key did not land in the github.com block: {block}"
  );
}

#[test]
fn the_wildcard_keeps_its_own_key() {
  // Writing into `Host *` would silently change every other host on the
  // machine, including ones GitWyrm knows nothing about.
  let out = rewrite(CONFIG, "github.com", "C:/Users/me/.ssh/work");
  let wildcard = out.split("Host *").nth(1).expect("wildcard block");
  let block = wildcard.split("\nHost ").next().expect("block body");
  assert!(block.contains("IdentityFile ~/.ssh/id_ed25519"));
  assert!(!block.contains("C:/Users/me/.ssh/work"));
}

#[test]
fn an_existing_key_is_replaced_not_duplicated() {
  let out = rewrite(CONFIG, "homeassistant.local", "C:/keys/new");
  assert_eq!(out.matches("IdentityFile C:/keys/new").count(), 1);
  assert!(!out.contains("IdentityFile ~/.ssh/homeassistant"));
}

#[test]
fn a_host_with_no_block_gets_one_appended() {
  let out = rewrite(CONFIG, "gitlab.com", "C:/keys/lab");
  assert!(out.contains("Host gitlab.com"));
  assert!(out.contains("IdentityFile C:/keys/lab"));
  // And the original content is still all there.
  assert!(out.contains("Host homeassistant.local"));
  assert!(out.contains("Host hstest"));
}

#[test]
fn rewriting_twice_is_stable() {
  // The user may switch keys more than once; the second pass must not stack up
  // duplicate IdentityFile lines or reshape the file further.
  let once = rewrite(CONFIG, "github.com", "C:/keys/a");
  let twice = rewrite(&once, "github.com", "C:/keys/b");
  assert_eq!(twice.matches("IdentityFile C:/keys/b").count(), 1);
  assert!(!twice.contains("C:/keys/a"));

  let third = rewrite(&twice, "github.com", "C:/keys/b");
  assert_eq!(third, twice, "a no-op rewrite changed the file");
}

#[test]
fn line_count_only_grows_by_what_was_added() {
  // A blunt check that nothing is being silently dropped: replacing a key in an
  // existing block should leave the line count untouched.
  let out = rewrite(CONFIG, "homeassistant.local", "C:/keys/new");
  assert_eq!(
    out.lines().count(),
    CONFIG.lines().count(),
    "replacing a key changed the number of lines"
  );
}
