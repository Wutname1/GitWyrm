//! Redaction for text that leaves the machine (Sentry events, panic payloads).
//!
//! Mirrors `src/lib/scrub.ts` on the frontend. Three kinds of text must never be
//! uploaded verbatim: access tokens, filesystem paths carrying the user's account
//! name, and personal data like the git author email. Error messages pick all
//! three up incidentally - a failed API call embeds its URL, a git error embeds
//! the repo path.
//!
//! Deliberately dependency-free (no `regex`) so this stays a small, auditable
//! function in a security-sensitive path. Scanning is done with plain string
//! walks over whitespace/quote-delimited words.

/// Token prefixes whose whole word is replaced when the tail looks like a secret.
const SECRET_PREFIXES: &[&str] = &[
  "github_pat_",
  "ghp_",
  "gho_",
  "ghu_",
  "ghs_",
  "ghr_",
  "sk-",
  "AIza",
  "AKIA",
  "ASIA",
  "eyJ",
];

/// Minimum tail length after a prefix before it is treated as a real token, so
/// prose like "the sk- prefix" is left alone.
const MIN_SECRET_TAIL: usize = 12;

/// Characters that can appear inside a single scanned token. Includes the path
/// separators so a whole path is one token, and `:` / `@` so URLs and emails
/// stay intact. `=` is excluded so `key=<token>` splits at the delimiter.
fn is_word_char(c: char) -> bool {
  c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '\\' | '+' | '~' | ':' | '@')
}

/// A token may arrive with leading delimiters still attached (`"sk-...`, or the
/// `C:\Users\...` drive prefix). Split off a leading run that cannot begin a
/// secret so the prefix test sees the real start of the value.
fn split_leading_noise(word: &str) -> (&str, &str) {
  // A Windows drive prefix: exactly one letter, then ':'.
  let bytes = word.as_bytes();
  if bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
    return word.split_at(2);
  }
  ("", word)
}

/// True when `word` starts with a known token prefix and has enough tail to be
/// a credential rather than a mention of the prefix.
fn looks_secret(word: &str) -> Option<&'static str> {
  for prefix in SECRET_PREFIXES {
    if let Some(tail) = word.strip_prefix(*prefix) {
      if tail.len() >= MIN_SECRET_TAIL {
        return Some(prefix);
      }
    }
  }
  None
}

/// An email-shaped word: something@something.tld with no spaces.
fn looks_email(word: &str) -> bool {
  let Some((local, domain)) = word.split_once('@') else {
    return false;
  };
  if local.is_empty() || domain.is_empty() || local.contains('@') {
    return false;
  }
  match domain.rsplit_once('.') {
    Some((host, tld)) => {
      !host.is_empty() && tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
    }
    None => false,
  }
}

/// Replace `//user:pass@host` credentials inside a URL-ish word.
fn scrub_url_credentials(word: &str) -> Option<String> {
  let idx = word.find("//")?;
  let after = &word[idx + 2..];
  let at = after.find('@')?;
  let authority = &after[..at];
  // Must look like user:pass, and neither half may contain a path separator.
  let (user, pass) = authority.split_once(':')?;
  if user.is_empty() || pass.is_empty() || authority.contains('/') {
    return None;
  }
  Some(format!("{}//[redacted]@{}", &word[..idx], &after[at + 1..]))
}

/// Replace the account name in a home path, keeping the rest of the path.
/// `C:\Users\jerem\code\repo` -> `C:\Users\[user]\code\repo`
fn scrub_home_path(word: &str) -> Option<String> {
  let lower = word.to_ascii_lowercase();
  for marker in ["users", "home"] {
    let mut search_from = 0usize;
    while let Some(found) = lower[search_from..].find(marker) {
      let start = search_from + found;
      let before_ok = start == 0
        || matches!(word.as_bytes()[start - 1], b'\\' | b'/');
      let after_idx = start + marker.len();
      if before_ok && after_idx < word.len() {
        let sep = word.as_bytes()[after_idx];
        if sep == b'\\' || sep == b'/' {
          let name_start = after_idx + 1;
          let rest = &word[name_start..];
          let name_end = rest
            .find(['\\', '/'])
            .map_or(word.len(), |i| name_start + i);
          if name_end > name_start {
            let mut out = String::with_capacity(word.len());
            out.push_str(&word[..name_start]);
            out.push_str("[user]");
            out.push_str(&word[name_end..]);
            return Some(out);
          }
        }
      }
      search_from = start + marker.len();
      if search_from >= lower.len() {
        break;
      }
    }
  }
  None
}

/// Redact secrets, emails, and home-path account names from a string.
pub fn scrub_text(value: &str) -> String {
  let mut out = String::with_capacity(value.len());
  let mut rest = value;

  while !rest.is_empty() {
    // Copy the non-word run verbatim so spacing and punctuation survive.
    let word_start = rest.find(is_word_char).unwrap_or(rest.len());
    out.push_str(&rest[..word_start]);
    rest = &rest[word_start..];
    if rest.is_empty() {
      break;
    }

    let word_end = rest.find(|c: char| !is_word_char(c)).unwrap_or(rest.len());
    let full_word = &rest[..word_end];
    rest = &rest[word_end..];

    // Keep any leading drive prefix verbatim; scan what follows it.
    let (lead, word) = split_leading_noise(full_word);
    out.push_str(lead);

    if let Some(prefix) = looks_secret(word) {
      out.push_str(prefix);
      out.push_str("[redacted]");
    } else if let Some(replaced) = scrub_url_credentials(word) {
      // Credentials in a URL; the host may still contain a home path.
      out.push_str(&scrub_home_path(&replaced).unwrap_or(replaced));
    } else if looks_email(word) {
      out.push_str("[email]");
    } else if let Some(replaced) = scrub_home_path(word) {
      out.push_str(&replaced);
    } else {
      out.push_str(word);
    }
  }

  out
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Build a token-shaped string at runtime from a prefix and a repeated filler.
  ///
  /// Deliberately NOT written as a literal: a hard-coded `AIza...`/`ghp_...`
  /// string in the source trips secret scanners (GitHub opened a "publicly
  /// leaked secret" alert on an earlier version of this file), and a scanner
  /// cannot tell a synthetic fixture from a real key. Assembling the value here
  /// keeps the test honest while leaving nothing scannable on disk.
  fn fake_token(prefix: &str, len: usize) -> String {
    let mut out = String::from(prefix);
    while out.len() < prefix.len() + len {
      out.push_str("Zq7");
    }
    out.truncate(prefix.len() + len);
    out
  }

  #[test]
  fn redacts_github_tokens() {
    let classic = fake_token("ghp_", 30);
    assert_eq!(
      scrub_text(&format!("auth failed for {classic}")),
      "auth failed for ghp_[redacted]"
    );

    let fine_grained = fake_token("github_pat_", 24);
    let scrubbed = scrub_text(&fine_grained);
    assert_eq!(scrubbed, "github_pat_[redacted]");
    // The tail must be gone, not merely shortened.
    assert!(!scrubbed.contains("Zq7"), "got: {scrubbed}");
  }

  #[test]
  fn redacts_provider_api_keys() {
    for token in [
      fake_token("sk-proj-", 24),
      fake_token("sk-ant-api03-", 20),
      fake_token("AIza", 35),
      fake_token("AKIA", 16),
      fake_token("ASIA", 16),
    ] {
      let token = token.as_str();
      let got = scrub_text(&format!("key={token}"));
      assert!(!got.contains(token), "{token} survived as {got}");
      assert!(got.contains("[redacted]"), "no marker for {token}: {got}");
    }
  }

  #[test]
  fn redacts_jwts() {
    // Assembled, not a literal, for the same reason as `fake_token`.
    let jwt = format!(
      "{}.{}.{}",
      fake_token("eyJ", 16),
      fake_token("eyJ", 16),
      fake_token("Sig", 20)
    );
    let got = scrub_text(&jwt);
    assert!(!got.contains("Zq7"), "jwt survived as {got}");
  }

  #[test]
  fn redacts_credentials_embedded_in_a_url() {
    let secret = fake_token("pw", 8);
    let got = scrub_text(&format!("clone https://alice:{secret}@github.com/o/r.git failed"));
    assert!(!got.contains(&secret), "got: {got}");
    assert!(got.contains("[redacted]@github.com/o/r.git"), "got: {got}");
  }

  #[test]
  fn redacts_the_account_name_from_windows_and_unix_home_paths() {
    assert_eq!(
      scrub_text("failed to open C:\\Users\\jerem\\code\\GitWyrm\\src"),
      "failed to open C:\\Users\\[user]\\code\\GitWyrm\\src"
    );
    assert_eq!(
      scrub_text("failed to open /home/jerem/code/repo"),
      "failed to open /home/[user]/code/repo"
    );
    assert_eq!(
      scrub_text("open /Users/jerem/dev/x"),
      "open /Users/[user]/dev/x"
    );
  }

  #[test]
  fn redacts_email_addresses() {
    assert_eq!(
      scrub_text("author jeremy12@gmail.com could not be resolved"),
      "author [email] could not be resolved"
    );
  }

  #[test]
  fn keeps_ordinary_diagnostics_readable() {
    // The point of scrubbing is a report that is still useful.
    let msg = "git checkout failed: pathspec 'feature/login' did not match";
    assert_eq!(scrub_text(msg), msg);
    assert_eq!(scrub_text("branch main is 3 ahead, 2 behind"), "branch main is 3 ahead, 2 behind");
    // A short mention of a prefix is prose, not a token.
    assert_eq!(scrub_text("keys use the sk- prefix"), "keys use the sk- prefix");
  }

  #[test]
  fn handles_empty_and_punctuation_only_input() {
    assert_eq!(scrub_text(""), "");
    assert_eq!(scrub_text("   \n\t"), "   \n\t");
    assert_eq!(scrub_text("!!! ??? ..."), "!!! ??? ...");
  }
}
