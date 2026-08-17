use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
  #[error("git error: {0}")]
  Git(#[from] git2::Error),
  #[error("io error: {0}")]
  Io(#[from] std::io::Error),
  #[error("{0}")]
  Other(String),
}

/// Conditions the app is expected to hit in normal use.
///
/// These are refusals, not faults: the remote said no, or git declined an
/// operation that was never going to work. The user still needs to be told,
/// but there is nothing here for us to fix, so they are logged below `error`
/// and do not become Sentry issues.
///
/// Matched on the lowercased message. Kept deliberately narrow -- a substring
/// broad enough to swallow a real failure would hide the bugs this reporting
/// exists to catch.
const EXPECTED: &[&str] = &[
  // The org has OAuth app restrictions turned on. Entirely the remote's call;
  // 351 reports of this in one week said nothing we could act on.
  "oauth app access restrictions",
  // Auth failures generally: wrong or expired credentials, not a defect.
  "authentication failed",
  "401 unauthorized",
  "403 forbidden",
  // git refusing an operation whose preconditions are not met.
  "as it is the current head of a linked repository",
  "cannot delete branch",
  "you are not currently on a branch",
  // The remote moved on since the last fetch, so a plain push was refused.
  // Everyday collaboration, not a fault: the answer is to pull and retry, which
  // the UI now says outright. 124 reports in 19 days told us nothing to fix.
  //
  // `(fetch first)` keeps its parentheses because that is how git prints it on
  // the rejection line; the bare words appear in unrelated failures such as
  // "failed to fetch first 8 bytes of pack".
  "[rejected]",
  "[remote rejected]",
  "non-fast-forward",
  "(fetch first)",
  // The network being unavailable is not an application error.
  "could not resolve host",
  "failed to connect",
  // The host declining a request on its own rules. The user is told, and the
  // answer is always theirs or their admin's -- there is nothing here we could
  // change. "merge conflicts" alone accounted for 522 reports in 20 days.
  "merge conflicts",
  "sign-in is no longer valid",
  "rate limit reached",
  "review is required",
  "not mergeable",
];

fn is_expected(message: &str) -> bool {
  let lowered = message.to_lowercase();
  EXPECTED.iter().any(|needle| lowered.contains(needle))
}

impl Serialize for AppError {
  fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    let message = self.to_string();
    // Tauri serializes command errors before returning them to the UI. Logging
    // at this boundary guarantees that an error shown to the user also has a
    // durable entry in the app log.
    //
    // Expected conditions log at `warn` rather than `error` because
    // SentryLogger turns every `error!` into an issue. They stay in
    // the app log either way -- and as Sentry breadcrumbs, so they still give
    // context to a real error that follows -- without burying genuine bugs
    // under hundreds of reports of the remote saying no.
    if is_expected(&message) {
      log::warn!("Command refused: {message}");
    } else {
      log::error!("Command failed: {message}");
    }
    serializer.serialize_str(&message)
  }
}

impl specta::Type for AppError {
  fn inline(_: &mut specta::TypeCollection, _: specta::Generics) -> specta::datatype::DataType {
    specta::datatype::DataType::Primitive(specta::datatype::PrimitiveType::String)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn remote_refusals_are_expected() {
    // The two that actually flooded Sentry, verbatim from the reports.
    assert!(is_expected(
      "GitHub said: Although you appear to have the correct authorization credentials, the `some-org` organization has enabled OAuth App access restrictions, meaning that data access to third-parties is limited."
    ));
    assert!(is_expected(
      "git error: Cannot delete branch 'refs/heads/feature' as it is the current HEAD of a linked repository.; class=Reference (4)"
    ));
  }

  /// A push refused because the remote moved on is everyday collaboration.
  /// `failure_detail` now surfaces the rejection line rather than git's
  /// trailing hint, so these are the shapes that reach us.
  #[test]
  fn a_refused_push_is_expected() {
    assert!(is_expected(
      "git push failed: ! [rejected]        main -> main (fetch first)"
    ));
    assert!(is_expected(
      "git push failed: ! [rejected]        main -> main (non-fast-forward)"
    ));
    assert!(is_expected(
      "git push failed: ! [remote rejected] main -> main (pre-receive hook declined)"
    ));
  }

  #[test]
  fn matching_ignores_case() {
    assert!(is_expected("AUTHENTICATION FAILED"));
    assert!(is_expected("Could Not Resolve Host: github.com"));
  }

  /// The filter must not swallow real faults. Anything matched here stops
  /// being reported, so a needle broad enough to catch a genuine bug would
  /// hide exactly what the reporting exists to surface.
  #[test]
  fn real_failures_are_still_reported() {
    assert!(!is_expected("io error: permission denied (os error 5)"));
    assert!(!is_expected(
      "git error: object not found - no match for id (abc123); class=Odb (9)"
    ));
    assert!(!is_expected("repository not open: 0123456789abcdef"));
    assert!(!is_expected("index.lock exists, another process may be running"));
    assert!(!is_expected("bare repositories are not supported"));
    // Corruption must never read as routine just because it mentions a branch.
    assert!(!is_expected("git error: branch data is corrupt"));
    // The push needles are short. A real fault that merely talks about
    // fetching or rejection must still be reported.
    assert!(!is_expected("git error: could not read from remote repository"));
    assert!(!is_expected("io error: failed to fetch first 8 bytes of pack"));
  }
}
