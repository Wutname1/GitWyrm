//! Unified-diff text into the shape the diff viewer already renders.
//!
//! Hosts hand back a pull request's changes as a patch string per file, which is
//! the same thing `git diff` prints. GitWyrm's local diffs are built from libgit2
//! callbacks instead, but they land in [`FileDiff`] -- so parsing the host's text
//! into that same struct means a pull request file and a working-tree file are
//! the same thing to every component downstream. No PR-specific viewer, no second
//! set of line-numbering rules to keep in step.
//!
//! Only the body is parsed. Hosts send the hunks alone, with no `diff --git`
//! preamble, so path and rename information comes from the surrounding JSON
//! rather than from here.

use crate::git::types::{DiffLineEntry, FileDiff, HunkHeader};

/// Parse one file's unified diff body into a [`FileDiff`].
///
/// Never fails: a patch we cannot make sense of yields the hunks we did read, and
/// a completely unparseable one yields no lines. A file that renders empty is a
/// far better outcome here than an error that takes down the whole file list,
/// since the row still shows the path and its counts either way.
pub fn parse_file_patch(
  path: &str,
  old_path: Option<String>,
  patch: &str,
  additions: u32,
  deletions: u32,
) -> FileDiff {
  let mut hunks: Vec<HunkHeader> = Vec::new();
  let mut lines: Vec<DiffLineEntry> = Vec::new();
  // Counters for the hunk being read; a line's numbers come from these rather
  // than from the text, exactly as the local diff builder assigns them.
  let mut old_no = 0u32;
  let mut new_no = 0u32;

  for raw in patch.lines() {
    if raw.starts_with("@@") {
      let Some(header) = parse_hunk_header(raw) else {
        continue;
      };
      old_no = header.old_start;
      new_no = header.new_start;
      lines.push(DiffLineEntry {
        sign: "@".into(),
        old_no: None,
        new_no: None,
        text: raw.to_string(),
        hunk_index: hunks.len() as u32,
      });
      hunks.push(header);
      continue;
    }

    // Body lines before the first hunk header are preamble the host should not
    // have sent; skipping them keeps a stray line out of hunk 0.
    if hunks.is_empty() {
      continue;
    }
    let hunk_index = hunks.len() as u32 - 1;

    // "\ No newline at end of file" annotates the line above and occupies no
    // line number on either side. It is dropped rather than shown, matching what
    // the local diff does with the same marker.
    if raw.starts_with('\\') {
      continue;
    }

    let (sign, text) = match raw.as_bytes().first() {
      Some(b'+') => ("+", &raw[1..]),
      Some(b'-') => ("-", &raw[1..]),
      Some(b' ') => ("", &raw[1..]),
      // A truly empty line in the body is a context line whose leading space the
      // host trimmed; treating it as context keeps the numbering aligned.
      None => ("", ""),
      _ => continue,
    };

    let (line_old, line_new) = match sign {
      "+" => {
        let n = new_no;
        new_no += 1;
        (None, Some(n))
      }
      "-" => {
        let n = old_no;
        old_no += 1;
        (Some(n), None)
      }
      _ => {
        let (o, n) = (old_no, new_no);
        old_no += 1;
        new_no += 1;
        (Some(o), Some(n))
      }
    };

    lines.push(DiffLineEntry {
      sign: sign.into(),
      old_no: line_old,
      new_no: line_new,
      text: text.to_string(),
      hunk_index,
    });
  }

  FileDiff {
    path: path.to_string(),
    old_path,
    additions,
    deletions,
    hunks,
    lines,
    binary: false,
  }
}

/// `@@ -old_start,old_lines +new_start,new_lines @@ section` into a [`HunkHeader`].
///
/// The line counts are optional in unified diff format and mean 1 when omitted,
/// which is how a single-line hunk is written.
fn parse_hunk_header(line: &str) -> Option<HunkHeader> {
  let body = line.strip_prefix("@@ ")?;
  let end = body.find(" @@")?;
  let mut ranges = body[..end].split(' ');
  let (old_start, old_lines) = parse_range(ranges.next()?.strip_prefix('-')?)?;
  let (new_start, new_lines) = parse_range(ranges.next()?.strip_prefix('+')?)?;
  Some(HunkHeader {
    old_start,
    old_lines,
    new_start,
    new_lines,
    header: line.to_string(),
  })
}

/// `start,count` or a bare `start`, which means one line.
fn parse_range(text: &str) -> Option<(u32, u32)> {
  let mut parts = text.split(',');
  let start: u32 = parts.next()?.parse().ok()?;
  let count: u32 = match parts.next() {
    Some(c) => c.parse().ok()?,
    None => 1,
  };
  Some((start, count))
}

#[cfg(test)]
mod tests {
  use super::*;

  const SIMPLE: &str = "@@ -1,3 +1,4 @@ fn main()\n context\n-old line\n+new line\n+added\n more";

  #[test]
  fn numbers_lines_from_the_hunk_header() {
    let d = parse_file_patch("a.rs", None, SIMPLE, 2, 1);
    let nums: Vec<_> = d
      .lines
      .iter()
      .map(|l| (l.sign.as_str(), l.old_no, l.new_no))
      .collect();
    assert_eq!(
      nums,
      vec![
        ("@", None, None),
        ("", Some(1), Some(1)),
        ("-", Some(2), None),
        ("+", None, Some(2)),
        ("+", None, Some(3)),
        ("", Some(3), Some(4)),
      ]
    );
  }

  #[test]
  fn strips_the_leading_sign_from_line_text() {
    let d = parse_file_patch("a.rs", None, SIMPLE, 2, 1);
    let added: Vec<_> = d
      .lines
      .iter()
      .filter(|l| l.sign == "+")
      .map(|l| l.text.as_str())
      .collect();
    assert_eq!(added, vec!["new line", "added"]);
  }

  /// A line that begins with `+` or `-` in its own right must keep that
  /// character; only the diff's own marker column is removed.
  #[test]
  fn keeps_signs_that_belong_to_the_content() {
    let d = parse_file_patch("a.rs", None, "@@ -1 +1 @@\n-- dashes\n++ plus", 1, 1);
    let text: Vec<_> = d.lines.iter().skip(1).map(|l| l.text.as_str()).collect();
    assert_eq!(text, vec!["- dashes", "+ plus"]);
  }

  #[test]
  fn assigns_each_line_to_its_hunk() {
    let patch = "@@ -1,1 +1,1 @@\n-a\n+b\n@@ -10,1 +10,1 @@\n-c\n+d";
    let d = parse_file_patch("a.rs", None, patch, 2, 2);
    assert_eq!(d.hunks.len(), 2);
    // Both the header line and its body carry the hunk's index.
    let indices: Vec<_> = d.lines.iter().map(|l| l.hunk_index).collect();
    assert_eq!(indices, vec![0, 0, 0, 1, 1, 1]);
    // The second hunk restarts numbering from its own header.
    assert_eq!(d.lines[4].old_no, Some(10));
  }

  #[test]
  fn omitted_line_count_means_one() {
    let d = parse_file_patch("a.rs", None, "@@ -5 +5 @@\n-a\n+b", 1, 1);
    assert_eq!(d.hunks[0].old_lines, 1);
    assert_eq!(d.hunks[0].new_lines, 1);
    assert_eq!(d.hunks[0].old_start, 5);
  }

  #[test]
  fn drops_the_no_newline_marker_without_consuming_a_line_number() {
    let patch = "@@ -1,2 +1,2 @@\n context\n-old\n\\ No newline at end of file\n+new";
    let d = parse_file_patch("a.rs", None, patch, 1, 1);
    assert!(!d.lines.iter().any(|l| l.text.starts_with("No newline")));
    let added = d.lines.iter().find(|l| l.sign == "+").unwrap();
    assert_eq!(added.new_no, Some(2));
  }

  #[test]
  fn an_unparseable_patch_yields_no_lines_rather_than_failing() {
    let d = parse_file_patch("a.rs", None, "not a diff at all", 0, 0);
    assert!(d.lines.is_empty());
    assert!(d.hunks.is_empty());
    assert_eq!(d.path, "a.rs");
  }

  #[test]
  fn carries_the_rename_source_through() {
    let d = parse_file_patch("new.rs", Some("old.rs".into()), SIMPLE, 2, 1);
    assert_eq!(d.old_path.as_deref(), Some("old.rs"));
  }
}
