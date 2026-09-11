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

/// Say what to do about a corrupt git index.
///
/// `.git/index` is a cache, not history: every byte in it is re-derivable from
/// HEAD and the working tree. So corruption is annoying but fully recoverable -
/// delete the file and let git rebuild it. libgit2 says only
/// `invalid data in index - incorrect header signature` (class=Index), and git's
/// own transport says `fatal: index file corrupt`; neither hints that the repo is
/// fine or that the fix is one command.
///
/// Seen as a burst of six in one second on a single machine (GITWYRM-BACKEND-2,
/// 0.12.0) - every operation that touched the index failing in turn, each showing
/// the same opaque line. Nothing in the app caused it (an interrupted write, a
/// crash mid-stage, or antivirus are the usual causes) but the user was left with
/// no idea it was fixable.
///
/// Deliberately still REPORTED, unlike the file lock. This must not go on the
/// EXPECTED list: index_refusals_are_expected_but_corruption_is_not pins that,
/// and the reason holds - corruption is a real fault worth seeing, even when the
/// remedy is the user's. This only improves the wording.
///
/// Keyed off the formatted message rather than a `git2::Error` because the
/// Serialize boundary below has already lost the typed error, and that boundary
/// is the ONE place every AppError from every command passes through.
/// Translating there covers the whole app without each call site having to
/// remember - which is exactly the coverage gap the file-lock hint hit (one call
/// site, then four more found later). Matching on the wording also catches git's
/// own porcelain phrasing, which carries no Index class at all.
fn corrupt_index_hint_for(message: &str) -> Option<String> {
    let lowered = message.to_lowercase();
    if !lowered.contains("invalid data in index") && !lowered.contains("index file corrupt") {
        return None;
    }
    Some(
        "this repository's git index file is damaged. Nothing in your history is lost - the index is a cache git rebuilds. Close other git tools, delete the `.git/index` file in this repository, then run `git reset` to rebuild it."
            .to_string(),
    )
}

/// Turn a Windows file-lock refusal into something the user can act on.
///
/// A checkout that has to remove a directory fails when anything else holds a
/// handle inside it, and Windows says so through libgit2 as
/// `could not rmdir '<path>': The process cannot access the file because it is
/// being used by another process.` (class=Os). That names the directory but not
/// the cause, and the cause is always something outside GitWyrm: a running
/// build, a debugger, an editor indexing the folder, or antivirus.
///
/// Reported from a .NET service directory (GITWYRM-BACKEND-2), which is the
/// classic shape - MSBuild or an IDE holding bin/obj open while the working tree
/// is rewritten. Nothing here is ours to fix, but the raw message reads like a
/// git fault, so say what to do instead.
///
/// Lives here rather than beside one caller because EVERY path that rewrites
/// working files can hit it: branch switch, merge checkout, and the hard resets
/// behind history rewrites. It was originally applied only to the branch switch,
/// and a recurrence on a pull/merge (a second repo, 2026-09-10) proved that gap
/// real rather than theoretical.
///
/// Only the lock wording is rewritten. Other Os-class errors (permissions, a
/// full disk, a missing path) are real and keep their own message.
pub fn windows_lock_hint(e: &git2::Error) -> Option<String> {
    if e.class() != git2::ErrorClass::Os {
        return None;
    }
    let message = e.message();
    if !message.contains("being used by another process") {
        return None;
    }
    // Keep the path: it is the one genuinely useful part, and it tells the user
    // which folder to go and close.
    let subject = message
        .split_once('\'')
        .and_then(|(_, rest)| rest.split_once('\''))
        .map(|(path, _)| path);
    Some(match subject {
        Some(path) => format!(
            "another program is holding files open in {path}, so they could not be replaced. Close anything using that folder - a running build, debugger, editor or antivirus scan - then try again."
        ),
        None => "another program is holding files open in this repository, so they could not be replaced. Close anything using the folder - a running build, debugger, editor or antivirus scan - then try again.".to_string(),
    })
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
    // Another git process holds the index -- a terminal running beside the app,
    // or a crashed process that left index.lock behind. Nothing in the app is
    // broken; the answer is to wait or clear the stale lock.
    "the index is locked",
    "as it is the current head of a linked repository",
    "cannot delete branch",
    "you are not currently on a branch",
    // Our own guard, raised before merge, cherry-pick, revert, branch switch,
    // checkout and history rewrite: the operation would overwrite uncommitted
    // work, so we refuse it. Declining to eat the user's changes is the feature
    // working, not a fault -- there is nothing here to fix.
    "working tree has changes",
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
    // Pulling a branch that has no upstream, or one that diverged with no
    // reconcile strategy set. Both are ordinary local state the user resolves by
    // choosing a remote branch or a merge/rebase preference -- nothing to fix.
    "there is no tracking information for the current branch",
    "you have divergent branches and need to specify how to reconcile them",
    // The network being unavailable is not an application error.
    "could not resolve host",
    "failed to connect",
    // The host declining a request on its own rules. The user is told, and the
    // answer is always theirs or their admin's -- there is nothing here we could
    // change. "merge conflicts" alone accounted for 522 reports in 20 days.
    "merge conflicts",
    // Committing while a merge is still half-resolved. git refuses to build a
    // tree until every conflicted path is staged, which is the whole point of a
    // conflict -- the answer is to finish resolving, not anything we could fix.
    "not fully merged index",
    // A repository with nowhere to send work. Push publishes an unlinked branch
    // by itself, so reaching this means no remote is set up at all -- a one-time
    // setup step, not a failure. The frontend has phrased this for users since
    // before the backend classified it, which is how 6 reports arrived for a
    // condition both layers already understood.
    "no remote to push to",
    "sign-in is no longer valid",
    // The same "connect an account" answer via the git shell rather than the
    // API. remote.rs translates git's "could not read username/password" into
    // this sentence, so the wording here is our own and already plain: nothing
    // is broken, the remote simply has no credentials yet.
    "sign-in needed for",
    // The third wording in this family, and the one for a host that was never
    // connected at all. hosting::not_connected builds it for GitHub, GitLab,
    // Bitbucket and Azure DevOps alike, so match the part that does not carry
    // the host name. Its own doc comment says the fix is always the same:
    // connect the host in Settings > Integrations. Nothing is broken.
    "not signed in to",
    // A remote-qualified name reaching a local-only command (delete, rename,
    // fast-forward). reject_remote_qualified turns libgit2's "cannot locate
    // local branch" into this sentence, which names the local branch to use
    // instead -- a refusal the user can act on, not a fault.
    "is a branch on the remote",
    "rate limit reached",
    "review is required",
    "not mergeable",
    // The host has no such repo, or the token cannot see it. Private, renamed,
    // deleted, or a token missing a scope -- all the user's to sort out, and
    // indistinguishable from each other because the host deliberately answers
    // 404 rather than admitting the thing exists.
    "could not find that. it may be private",
    // The SAME 404 in the fetch path's wording, which is built separately in
    // commands/remote.rs and never matched the needle above. It has been filing
    // as an error since the phrasing landed on 2026-08-28 - caught when a fetch
    // 404 and an unrelated file lock arrived three minutes apart on the same repo
    // (GITWYRM-BACKEND-2) and only the lock had been accounted for.
    //
    // Matched on the distinctive half. "could not find" alone is far too broad -
    // it would swallow "could not find commit <sha>" and the object-not-found
    // faults that real_failures_are_still_reported pins as must-report.
    "with your sign-in. it may have been moved or renamed",
    // The same condition in git's own words rather than the API's. A push or
    // fetch to a repo the host will not admit exists prints
    // `fatal: repository '<url>' not found`, and the host answers 404 whether it
    // is private, renamed, deleted, or simply outside the token's scope -- all
    // the user's to sort out, none of them ours.
    //
    // Matched with the quote so it cannot reach the bare words: "object not
    // found" and "repository not open" are real faults that must keep reporting,
    // and both are pinned in real_failures_are_still_reported below.
    "fatal: repository '",
    // No build for the running platform in the update manifest. Raised by the
    // updater plugin on a flavor we do not publish (a dev or WSL/Linux run
    // against a Windows-only manifest); nothing is wrong with the app.
    "were found in the response `platforms` object",
    // A checkout could not replace files because something outside GitWyrm holds
    // them open on Windows - a running build, a debugger, an editor indexing the
    // folder, antivirus. Genuinely the user's to sort out, and the one action
    // that fixes it is in the message.
    //
    // Matched on OUR wording (windows_lock_hint above), not
    // libgit2's. The raw text is class=Os, which also carries permission
    // failures and full disks - those are real and must keep reporting, so a
    // needle broad enough to reach them would be exactly the over-broad match
    // this list warns against.
    "another program is holding files open",
];

fn is_expected(message: &str) -> bool {
    let lowered = message.to_lowercase();
    EXPECTED.iter().any(|needle| lowered.contains(needle))
}

/// The classifier, for tests in modules that build these messages.
///
/// Every transport that phrases a host refusal has to be able to prove its
/// wording still lands in [`EXPECTED`]. Without this the check lives only here,
/// and a module can change its phrasing into a Sentry flood with all its own
/// tests green -- which is exactly how the CLI's 404 wording escaped.
#[cfg(test)]
pub fn is_expected_for_tests(message: &str) -> bool {
    is_expected(message)
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // A corrupt index is recoverable and the raw wording does not say so, but
        // it still REPORTS - unlike the file lock, this is a real fault. Only the
        // sentence the user reads changes.
        let message = corrupt_index_hint_for(&self.to_string()).unwrap_or_else(|| self.to_string());
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

    /// Refusing to overwrite uncommitted work is the guard doing its job. The
    /// tail names the operation and varies across the eight sites that raise it.
    #[test]
    fn a_dirty_working_tree_refusal_is_expected() {
        assert!(is_expected(
            "working tree has changes; commit or stash before merging"
        ));
        assert!(is_expected(
            "working tree has changes; commit or stash before rewriting history"
        ));
    }

    /// A project with no remote configured. The frontend had a rule for this
    /// long before EXPECTED did, so the two layers disagreed and 6 routine
    /// reports arrived for a setup step both already understood.
    #[test]
    fn a_project_with_no_remote_is_expected() {
        assert!(is_expected(
            "Command failed: This repository has no remote to push to."
        ));
    }

    /// A remote with no credentials yet, in our own words rather than the API's.
    ///
    /// Verbatim from GITWYRM-BACKEND-4. remote.rs translates git's "could not
    /// read username/password" into this sentence; the API transport's wording
    /// for a DEAD sign-in was already covered, this one for a MISSING sign-in
    /// was not.
    #[test]
    fn a_remote_needing_sign_in_is_expected() {
        assert!(is_expected(
            "Command failed: git fetch failed: Sign-in needed for https://github.com. Connect the account, then try again."
        ));
        assert!(is_expected(
            "Sign-in needed for this remote. Connect the account, then try again."
        ));
        // The API transport's wording for the same family, already covered.
        assert!(is_expected(
            "GitHub sign-in is no longer valid; connect GitHub again"
        ));
    }

    /// The index conditions, which split three ways and must not be conflated.
    #[test]
    /// The hint must explain the fix without making corruption look routine:
    /// it stays reportable, only its wording improves.
    #[test]
    fn a_corrupt_index_gets_recovery_advice() {
        let hint = super::corrupt_index_hint_for(
            "git error: invalid data in index - incorrect header signature; class=Index (10)",
        )
        .expect("should be translated");
        assert!(hint.contains(".git/index"), "names the file to delete: {hint}");
        assert!(hint.contains("Nothing in your history is lost"), "reassures: {hint}");
        // git's own porcelain wording, which carries no Index class at all.
        assert!(super::corrupt_index_hint_for("git fetch failed: fatal: index file corrupt").is_some());
        // A half-resolved merge is a REFUSAL, not corruption - it must not be
        // dressed up as a damaged file.
        assert!(super::corrupt_index_hint_for(
            "git error: cannot create a tree from a not fully merged index.; class=Index (10)"
        )
        .is_none());
        // And the translated wording must STILL report - never silenced.
        assert!(!is_expected(&hint));
    }

    #[test]
    fn index_refusals_are_expected_but_corruption_is_not() {
        // Refusal: a merge that is still half-resolved. Nothing to fix here.
        assert!(is_expected(
            "git error: cannot create a tree from a not fully merged index.; class=Index (10); code=Unmerged (-10)"
        ));
        // Corruption is a real fault and MUST keep reporting. Both transports:
        // libgit2's wording and git's own.
        assert!(!is_expected(
            "git error: invalid data in index - incorrect header signature; class=Index (10)"
        ));
        assert!(!is_expected("git fetch failed: fatal: index file corrupt"));
    }

    /// The guard that replaces libgit2's "cannot locate local branch" when a
    /// remote-qualified name reaches a local-only command. The refusal is
    /// expected; the raw git error it replaces is NOT, and must keep reporting.
    #[test]
    fn a_remote_qualified_name_refusal_is_expected() {
        assert!(is_expected(
            "'origin/development' is a branch on the remote. To link the local copy, use 'development'."
        ));
        assert!(!is_expected(
            "git error: cannot locate local branch 'origin/development'; class=Reference (4); code=NotFound (-3)"
        ));
    }

    /// A host that was never connected at all - the third wording in this
    /// family, after a DEAD sign-in and a MISSING one on a git remote.
    ///
    /// Verbatim from GITWYRM-BACKEND-4. hosting::not_connected interpolates the
    /// host name, so the needle matches the part that does not.
    #[test]
    fn a_host_that_was_never_connected_is_expected() {
        assert!(is_expected(
            "Command failed: not signed in to GitHub; connect GitHub first"
        ));
        for host in ["GitLab", "Bitbucket", "Azure DevOps"] {
            assert!(is_expected(&format!(
                "not signed in to {host}; connect {host} first"
            )));
        }
    }

    /// A host that will not admit the repo exists, in git's own words.
    ///
    /// Verbatim from GITWYRM-BACKEND-2, and reproduced locally to confirm the
    /// shape: git prints `remote: Repository not found.` followed by
    /// `fatal: repository '<url>' not found`.
    /// The fetch path phrases the same 404 differently, and built its own
    /// wording in commands/remote.rs without ever reaching the needle the API
    /// path uses. Both must classify as refusals.
    #[test]
    fn a_fetch_404_is_expected_in_its_own_wording() {
        assert!(is_expected(
            "Command failed: git fetch failed: Could not find https://github.com/acme/thing.git with your sign-in. It may have been moved or renamed, or your account may not have access to it."
        ));
        assert!(is_expected(
            "Could not find this repository with your sign-in. It may have been moved or renamed, or your account may not have access to it."
        ));
        // The needle must not reach a real lookup failure that merely says
        // "could not find".
        assert!(!is_expected("could not find commit 0123456789abcdef"));
    }

    #[test]
    fn a_missing_remote_repository_is_expected() {
        assert!(is_expected(
            "Command failed: git push failed: fatal: repository 'https://github.com/owner/repo.git/' not found"
        ));
        // The API transport's wording for the same condition, already covered.
        assert!(is_expected(
            "GitHub could not find that. It may be private, renamed, or your token may not cover it."
        ));
        // The needle keeps the quote so it cannot reach the bare words. These
        // two are real faults and are also pinned in the not-expected test.
        assert!(!is_expected(
            "git error: object not found - no match for id (abc123); class=Odb (9)"
        ));
        assert!(!is_expected("repository not open: 0123456789abcdef"));
    }

    /// Verbatim from the Sentry reports that kept arriving after these were
    /// added to EXPECTED. Both turned out to be stale-build noise, but the
    /// wording is the contract: `AppError::Other` prefixes vary by call site, so
    /// the needles have to survive whatever precedes them.
    #[test]
    fn refusals_are_expected_with_their_reported_prefixes() {
        assert!(is_expected(
            "Command failed: working tree has changes; commit or stash before cherry-picking"
        ));
        assert!(is_expected(
            "mutation failed [error]: git pull failed: There is no tracking information for the current branch."
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

    /// Both taken verbatim from Sentry issues that were filed as faults.
    #[test]
    fn host_404_and_missing_update_platform_are_expected() {
        assert!(is_expected(
      "GitHub could not find that. It may be private, renamed, or your token may not cover it."
    ));
        assert!(is_expected(
      "None of the fallback platforms `[\"linux-x86_64-deb\", \"linux-x86_64\"]` were found in the response `platforms` object"
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
    /// A file lock during checkout is the user's to clear (close the build, the
    /// debugger, the editor), so it is a refusal rather than a fault - but only
    /// in OUR wording. See windows_lock_hint above.
    #[test]
    fn a_windows_file_lock_is_expected() {
        assert!(is_expected(
            "another program is holding files open in C:/Code/EmailService/, so they could not be replaced. Close anything using that folder - a running build, debugger, editor or antivirus scan - then try again."
        ));
        // The RAW libgit2 text must NOT match: it is class=Os, which also covers
        // permission failures and full disks. Only the translated message is a
        // refusal, and the translation happens at exactly one call site.
        assert!(!is_expected(
            "git error: could not rmdir 'C:/Code/EmailService/': The process cannot access the file because it is being used by another process.; class=Os (2)"
        ));
    }

    /// The lock hint must name the folder to close, and must leave other
    /// Os-class errors (permissions, full disk) alone - those are real faults.
    #[test]
    fn a_file_lock_becomes_actionable_advice() {
        let locked = git2::Error::new(
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Os,
            "could not rmdir 'C:/Code/EmailService/': The process cannot access the file because it is being used by another process.",
        );
        let hint = super::windows_lock_hint(&locked).expect("lock should be translated");
        assert!(hint.contains("C:/Code/EmailService/"), "keeps the folder: {hint}");
        assert!(hint.contains("try again"), "tells the user what to do: {hint}");

        let denied = git2::Error::new(
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Os,
            "failed to write '.git/index': permission denied",
        );
        assert!(super::windows_lock_hint(&denied).is_none(), "a real Os fault must pass through");

        let not_os = git2::Error::new(
            git2::ErrorCode::NotFound,
            git2::ErrorClass::Reference,
            "cannot locate local branch 'x'; being used by another process",
        );
        assert!(super::windows_lock_hint(&not_os).is_none(), "class must gate the match");
    }

    #[test]
    fn real_failures_are_still_reported() {
        assert!(!is_expected("io error: permission denied (os error 5)"));
        assert!(!is_expected(
            "git error: object not found - no match for id (abc123); class=Odb (9)"
        ));
        assert!(!is_expected("repository not open: 0123456789abcdef"));
        assert!(!is_expected(
            "index.lock exists, another process may be running"
        ));
        assert!(!is_expected("bare repositories are not supported"));
        // Corruption must never read as routine just because it mentions a branch.
        assert!(!is_expected("git error: branch data is corrupt"));
        // The push needles are short. A real fault that merely talks about
        // fetching or rejection must still be reported.
        assert!(!is_expected(
            "git error: could not read from remote repository"
        ));
        assert!(!is_expected(
            "io error: failed to fetch first 8 bytes of pack"
        ));
        // The updater needle names the manifest key. A real failure to read the
        // manifest, or to parse it, is still a fault worth reporting.
        assert!(!is_expected(
            "updater: failed to parse the response `platforms` object"
        ));
    }
}
