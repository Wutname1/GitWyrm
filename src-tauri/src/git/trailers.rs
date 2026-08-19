//! Git trailers GitWyrm reads and writes on commit messages.
//!
//! Two keys matter here:
//! - `Spec: <change-id>` links a commit to an OpenSpec change, so the graph can
//!   show which change a commit belongs to long after the branch is gone.
//! - `Assisted-by: <provider>` marks a commit an in-app AI run created.
//!
//! Trailers live in the last paragraph of the message, one `Key: value` per
//! line, which is the same shape `git interpret-trailers` uses. Parsing stays
//! deliberately narrow: only a final block whose every line is `Key: value`
//! counts, so a message ending in prose that happens to contain a colon is left
//! alone.

/// Trailer key linking a commit to an OpenSpec change.
pub const SPEC_KEY: &str = "Spec";
/// Trailer key marking a commit as written by an in-app AI run.
pub const ASSISTED_KEY: &str = "Assisted-by";

/// Split `line` into a trailer key and value.
///
/// The key must be non-empty and free of whitespace, which is what keeps a
/// prose line like `note: see above` from being read as a trailer while
/// `Spec: add-thing` is.
fn split_line(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(':')?;
    if key.is_empty() || key.chars().any(char::is_whitespace) {
        return None;
    }
    Some((key, value.trim()))
}

/// Index into `message` where the trailing trailer block starts, if there is
/// one. The block is the final paragraph, and every one of its lines must parse
/// as a trailer.
fn trailer_block_start(message: &str) -> Option<usize> {
    let trimmed = message.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    // Walk back over the final run of non-blank lines.
    let mut start = None;
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut cursor = 0usize;
    for line in trimmed.split('\n') {
        lines.push((cursor, line));
        cursor += line.len() + 1;
    }

    for (index, (offset, line)) in lines.iter().enumerate().rev() {
        if line.trim().is_empty() {
            // Blank line closes the block. A trailer block cannot be the whole
            // message -- a one-line `Spec: x` commit is a subject, not a trailer.
            return if index + 1 < lines.len() { start } else { None };
        }
        if split_line(line.trim_end()).is_none() {
            return None;
        }
        start = Some(*offset);
        if index == 0 {
            // Reached the top without a blank line: the whole message is trailers,
            // which means it has no subject. Treat it as prose.
            return None;
        }
    }
    start
}

/// Read the value of trailer `key` from `message`, if present.
///
/// Matching on the key is case-insensitive, the way git treats trailer keys.
pub fn get(message: &str, key: &str) -> Option<String> {
    let start = trailer_block_start(message)?;
    message[start..]
        .lines()
        .filter_map(|line| split_line(line.trim_end()))
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.to_string())
        .filter(|value| !value.is_empty())
}

/// The OpenSpec change id this commit is linked to.
pub fn spec_id(message: &str) -> Option<String> {
    get(message, SPEC_KEY)
}

/// The AI provider that wrote this commit, if any.
pub fn assisted_by(message: &str) -> Option<String> {
    get(message, ASSISTED_KEY)
}

/// Return `message` with `key: value` set in its trailer block.
///
/// An existing trailer with the same key is replaced rather than duplicated, so
/// calling this twice is the same as calling it once. When the message has no
/// trailer block yet, one is started after a blank line.
pub fn upsert(message: &str, key: &str, value: &str) -> String {
    let body = message.trim_end();
    let line = format!("{key}: {value}");

    match trailer_block_start(body) {
        Some(start) => {
            let (head, block) = body.split_at(start);
            let mut replaced = false;
            let mut out: Vec<String> = Vec::new();
            for existing in block.lines() {
                match split_line(existing.trim_end()) {
                    Some((k, _)) if k.eq_ignore_ascii_case(key) => {
                        if !replaced {
                            out.push(line.clone());
                            replaced = true;
                        }
                    }
                    _ => out.push(existing.trim_end().to_string()),
                }
            }
            if !replaced {
                out.push(line);
            }
            format!("{head}{}", out.join("\n"))
        }
        None => {
            if body.is_empty() {
                line
            } else {
                format!("{body}\n\n{line}")
            }
        }
    }
}

/// Return `message` with every trailer named `key` removed.
///
/// If that empties the trailer block, the blank line separating it from the
/// body goes too, so removing the only trailer restores the original message
/// exactly.
pub fn remove(message: &str, key: &str) -> String {
    let body = message.trim_end();
    let Some(start) = trailer_block_start(body) else {
        return body.to_string();
    };

    let (head, block) = body.split_at(start);
    let kept: Vec<&str> = block
        .lines()
        .filter(|existing| {
            split_line(existing.trim_end())
                .map(|(k, _)| !k.eq_ignore_ascii_case(key))
                .unwrap_or(true)
        })
        .collect();

    if kept.is_empty() {
        head.trim_end().to_string()
    } else {
        format!("{head}{}", kept.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_spec_trailer() {
        let msg = "new: thing\n\nBody text.\n\nSpec: add-thing";
        assert_eq!(spec_id(msg).as_deref(), Some("add-thing"));
    }

    #[test]
    fn reads_a_trailer_with_no_body() {
        let msg = "new: thing\n\nSpec: add-thing";
        assert_eq!(spec_id(msg).as_deref(), Some("add-thing"));
    }

    #[test]
    fn reads_several_trailers_from_one_block() {
        let msg = "new: thing\n\nSpec: add-thing\nAssisted-by: copilot";
        assert_eq!(spec_id(msg).as_deref(), Some("add-thing"));
        assert_eq!(assisted_by(msg).as_deref(), Some("copilot"));
    }

    #[test]
    fn key_match_ignores_case() {
        let msg = "new: thing\n\nspec: add-thing";
        assert_eq!(spec_id(msg).as_deref(), Some("add-thing"));
    }

    #[test]
    fn subject_only_message_has_no_trailers() {
        // A single line is the subject even when it looks like `Key: value`.
        assert_eq!(spec_id("Spec: add-thing"), None);
    }

    #[test]
    fn prose_ending_in_a_colon_is_not_a_trailer() {
        let msg = "new: thing\n\nsee the notes: they explain the rest";
        assert_eq!(spec_id(msg), None);
        assert_eq!(get(msg, "see the notes"), None);
    }

    #[test]
    fn a_prose_line_disqualifies_the_whole_block() {
        let msg = "new: thing\n\nSpec: add-thing\nand some trailing prose";
        assert_eq!(spec_id(msg), None);
    }

    #[test]
    fn empty_trailer_value_reads_as_absent() {
        let msg = "new: thing\n\nSpec:";
        assert_eq!(spec_id(msg), None);
    }

    #[test]
    fn upsert_starts_a_block_when_there_is_none() {
        let out = upsert("new: thing\n\nBody text.", SPEC_KEY, "add-thing");
        assert_eq!(out, "new: thing\n\nBody text.\n\nSpec: add-thing");
    }

    #[test]
    fn upsert_appends_to_an_existing_block() {
        let out = upsert("new: thing\n\nAssisted-by: copilot", SPEC_KEY, "add-thing");
        assert_eq!(out, "new: thing\n\nAssisted-by: copilot\nSpec: add-thing");
    }

    #[test]
    fn upsert_replaces_rather_than_duplicates() {
        let once = upsert("new: thing\n\nBody.", SPEC_KEY, "add-thing");
        let twice = upsert(&once, SPEC_KEY, "add-thing");
        assert_eq!(once, twice);

        let changed = upsert(&once, SPEC_KEY, "other-thing");
        assert_eq!(spec_id(&changed).as_deref(), Some("other-thing"));
        assert_eq!(changed.matches("Spec:").count(), 1);
    }

    #[test]
    fn upsert_on_a_subject_only_message() {
        let out = upsert("new: thing", SPEC_KEY, "add-thing");
        assert_eq!(out, "new: thing\n\nSpec: add-thing");
        assert_eq!(spec_id(&out).as_deref(), Some("add-thing"));
    }

    #[test]
    fn remove_restores_the_original_message() {
        let original = "new: thing\n\nBody text.";
        let with = upsert(original, SPEC_KEY, "add-thing");
        assert_eq!(remove(&with, SPEC_KEY), original);
    }

    #[test]
    fn remove_keeps_other_trailers() {
        let msg = "new: thing\n\nSpec: add-thing\nAssisted-by: copilot";
        let out = remove(msg, SPEC_KEY);
        assert_eq!(out, "new: thing\n\nAssisted-by: copilot");
        assert_eq!(assisted_by(&out).as_deref(), Some("copilot"));
    }

    #[test]
    fn remove_is_a_no_op_when_the_key_is_absent() {
        let msg = "new: thing\n\nAssisted-by: copilot";
        assert_eq!(remove(msg, SPEC_KEY), msg);
    }

    #[test]
    fn trailing_whitespace_does_not_defeat_parsing() {
        let msg = "new: thing\n\nSpec: add-thing   \n";
        assert_eq!(spec_id(msg).as_deref(), Some("add-thing"));
    }

    #[test]
    fn crlf_message_is_parsed() {
        let msg = "new: thing\r\n\r\nSpec: add-thing";
        assert_eq!(spec_id(msg).as_deref(), Some("add-thing"));
    }
}
