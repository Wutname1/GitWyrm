//! Ordering for tag names that look like version numbers.
//!
//! Plain string ordering puts `0.0.2` above `0.0.11`, because it compares the
//! `1` of `11` against the `2` one character at a time. Users read those as
//! version numbers, so the tag list has to compare the numeric runs as numbers.
//!
//! The comparison splits a name into alternating text and number chunks and
//! walks them in step: text chunks compare as text, number chunks as numbers.
//! That handles the shapes tags actually take (`v1.2.3`, `release-2026-01`,
//! `0.0.2-Beta1`) without needing the name to be valid semver.
//!
//! One semver rule is worth honoring on top of natural order: a version with a
//! pre-release suffix comes *before* the same version without one, so `0.0.2`
//! sorts above `0.0.2-Beta1` in a newest-first list. A longer name that only
//! adds more numeric parts (`1.2.3.4` vs `1.2.3`) is still the later version, so
//! only a `-` suffix triggers this.

use std::cmp::Ordering;

#[derive(Debug, PartialEq, Eq)]
enum Chunk<'a> {
    /// A run of digits, kept as text so leading zeros don't change the value and
    /// arbitrarily long runs can't overflow.
    Num(&'a str),
    Text(&'a str),
}

fn chunks(name: &str) -> Vec<Chunk<'_>> {
    let bytes = name.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let digit = bytes[i].is_ascii_digit();
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() == digit {
            i += 1;
        }
        let slice = &name[start..i];
        out.push(if digit {
            Chunk::Num(slice)
        } else {
            Chunk::Text(slice)
        });
    }
    out
}

/// Compare two digit runs numerically without parsing them: strip leading zeros,
/// then longer wins, and equal lengths compare lexically (which is numeric order
/// for equal-width digit strings).
fn cmp_num(a: &str, b: &str) -> Ordering {
    let a_t = a.trim_start_matches('0');
    let b_t = b.trim_start_matches('0');
    a_t.len().cmp(&b_t.len()).then_with(|| a_t.cmp(b_t))
}

/// True when the name carries a semver-style pre-release suffix (`1.2.3-rc1`).
fn has_prerelease(name: &str) -> bool {
    name.contains('-')
}

/// Ascending "natural" order for tag names: numeric runs compare as numbers, and
/// a pre-release sorts below the release it precedes.
pub fn compare_tag_names(a: &str, b: &str) -> Ordering {
    let (ca, cb) = (chunks(a), chunks(b));
    let mut ia = ca.iter();
    let mut ib = cb.iter();
    loop {
        match (ia.next(), ib.next()) {
            (None, None) => break,
            // One name ran out of chunks. If the longer one continues with a `-`
            // pre-release suffix it is the *earlier* version; otherwise it is later.
            (None, Some(_)) => {
                return if has_prerelease(b) {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
            (Some(_), None) => {
                return if has_prerelease(a) {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            (Some(x), Some(y)) => {
                let ord = match (x, y) {
                    (Chunk::Num(m), Chunk::Num(n)) => cmp_num(m, n),
                    (Chunk::Text(m), Chunk::Text(n)) => m.cmp(n),
                    // Mixed kinds: compare the raw text so the result is at least stable.
                    (Chunk::Num(m), Chunk::Text(n)) | (Chunk::Text(m), Chunk::Num(n)) => m.cmp(n),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted_desc(names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        v.sort_by(|a, b| compare_tag_names(b, a));
        v
    }

    #[test]
    fn numeric_runs_compare_as_numbers() {
        assert_eq!(compare_tag_names("0.0.2", "0.0.11"), Ordering::Less);
        assert_eq!(compare_tag_names("0.0.11", "0.0.9"), Ordering::Greater);
        assert_eq!(compare_tag_names("1.10.0", "1.9.0"), Ordering::Greater);
    }

    #[test]
    fn the_reported_tag_list_orders_newest_first() {
        let got = sorted_desc(&[
            "0.0.9",
            "0.0.8",
            "0.0.7",
            "0.0.6",
            "0.0.5",
            "0.0.4",
            "0.0.3",
            "0.0.2-R2",
            "0.0.2-Beta1",
            "0.0.2",
            "0.0.11",
            "0.0.10",
        ]);
        assert_eq!(
            got,
            vec![
                "0.0.11",
                "0.0.10",
                "0.0.9",
                "0.0.8",
                "0.0.7",
                "0.0.6",
                "0.0.5",
                "0.0.4",
                "0.0.3",
                "0.0.2",
                "0.0.2-R2",
                "0.0.2-Beta1",
            ]
        );
    }

    #[test]
    fn prerelease_sorts_below_its_release() {
        assert_eq!(compare_tag_names("1.2.3-rc1", "1.2.3"), Ordering::Less);
        assert_eq!(compare_tag_names("1.2.3", "1.2.3-rc1"), Ordering::Greater);
        // But an extra numeric part is a later version, not a pre-release.
        assert_eq!(compare_tag_names("1.2.3.4", "1.2.3"), Ordering::Greater);
    }

    #[test]
    fn leading_zeros_do_not_change_the_value() {
        assert_eq!(compare_tag_names("1.02.0", "1.2.0"), Ordering::Equal);
        assert_eq!(compare_tag_names("v007", "v7"), Ordering::Equal);
    }

    #[test]
    fn v_prefixed_and_dated_tags_order_sensibly() {
        assert_eq!(
            sorted_desc(&["v1.2.0", "v1.10.0", "v1.9.0"]),
            vec!["v1.10.0", "v1.9.0", "v1.2.0"]
        );
        assert_eq!(
            sorted_desc(&[
                "release-2026-01-05",
                "release-2026-01-20",
                "release-2025-12-31"
            ]),
            vec![
                "release-2026-01-20",
                "release-2026-01-05",
                "release-2025-12-31"
            ]
        );
    }

    #[test]
    fn non_version_tags_keep_alphabetical_order() {
        assert_eq!(
            sorted_desc(&["alpha", "beta", "gamma"]),
            vec!["gamma", "beta", "alpha"]
        );
    }

    #[test]
    fn ordering_is_total_and_consistent() {
        // Sorting must not depend on input order.
        let a = sorted_desc(&["0.0.10", "0.0.2", "0.0.11", "0.0.9"]);
        let b = sorted_desc(&["0.0.9", "0.0.11", "0.0.2", "0.0.10"]);
        assert_eq!(a, b);
    }
}
