//! The `edit_note` patch format and proposal rebasing.
//!
//! **Format: exact search/replace blocks.** Each edit names a piece of the current note (`find`,
//! copied verbatim) and what replaces it. Models are reliable with this format (it needs no line
//! numbers or unified-diff syntax), and it is validated strictly: `find` must occur exactly once,
//! and an edit may not quietly rewrite the whole note. An empty `find` appends to the end.
//!
//! **Rebasing.** A proposal stores the note's Markdown before and after. If the user changed the
//! note before applying, the change is re-applied hunk by hunk on the current text, each hunk
//! anchored by unchanged neighbouring lines that must match exactly once; otherwise it is a conflict.

use serde::{Deserialize, Serialize};
use similar::{Algorithm, DiffTag};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edit {
    /// Exact text to replace, copied from the note. Empty means "append to the end".
    #[serde(default)]
    pub find: String,
    pub replace: String,
}

/// Apply edits in order. Errors are written for the model (it gets them back as a tool error).
pub fn apply_edits(body: &str, edits: &[Edit]) -> Result<String, String> {
    if edits.is_empty() {
        return Err("`edits` is empty. Give at least one {find, replace} pair.".into());
    }
    let mut out = body.to_string();
    for (i, edit) in edits.iter().enumerate() {
        let n = i + 1;
        if edit.find.is_empty() {
            if out.trim().is_empty() {
                out = edit.replace.clone();
            } else {
                out = format!("{}\n\n{}", out.trim_end(), edit.replace.trim_start());
            }
            continue;
        }
        let count = out.matches(edit.find.as_str()).count();
        if count == 0 {
            return Err(format!(
                "Edit {n}: the `find` text is not in the note. Copy it exactly from get_note, including punctuation, Markdown and line breaks."
            ));
        }
        if count > 1 {
            return Err(format!("Edit {n}: the `find` text appears {count} times. Include more surrounding text so it matches once."));
        }
        let long_note = out.chars().count() > 400;
        if long_note && edit.find.trim().len() * 10 >= out.trim().len() * 9 {
            return Err("edit_note changes parts of a note. Replace only the sections that change, not the whole note.".into());
        }
        out = out.replacen(edit.find.as_str(), &edit.replace, 1);
    }
    if out == body {
        return Err("These edits do not change the note.".into());
    }
    Ok(out)
}

fn lines(s: &str) -> Vec<&str> {
    s.split_inclusive('\n').collect()
}

/// Re-apply the change `before → target` onto `current`. `None` when it no longer fits.
pub fn rebase(before: &str, target: &str, current: &str) -> Option<String> {
    if current == before {
        return Some(target.to_string());
    }
    if current == target {
        return Some(current.to_string());
    }
    let old = lines(before);
    let new = lines(target);
    let ops = similar::capture_diff_slices(Algorithm::Myers, &old, &new);

    // Merge adjacent non-equal ops into hunks: (old range, new range).
    let mut hunks: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> = Vec::new();
    for op in ops {
        let (tag, o, n) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            continue;
        }
        match hunks.last_mut() {
            Some((ho, hn)) if ho.end == o.start && hn.end == n.start => {
                ho.end = o.end;
                hn.end = n.end;
            }
            _ => hunks.push((o, n)),
        }
    }

    let mut out = current.to_string();
    for (o, n) in hunks {
        let old_chunk: String = old[o.clone()].concat();
        let new_chunk: String = new[n].concat();
        let mut applied = false;
        // Widen the anchor until it matches exactly once.
        for k in 1..=4usize {
            let pre: String = old[o.start.saturating_sub(k)..o.start].concat();
            let post: String = old[o.end..(o.end + k).min(old.len())].concat();
            let search = format!("{pre}{old_chunk}{post}");
            if search.is_empty() {
                break;
            }
            if out.matches(search.as_str()).count() == 1 {
                out = out.replacen(search.as_str(), &format!("{pre}{new_chunk}{post}"), 1);
                applied = true;
                break;
            }
        }
        if !applied {
            return None;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(find: &str, replace: &str) -> Edit {
        Edit { find: find.into(), replace: replace.into() }
    }

    #[test]
    fn applies_exact_unique_replacements_in_order() {
        let body = "# Plan\n\n- ship v1\n- hire\n";
        let out = apply_edits(body, &[e("- ship v1", "- ship v1 by Friday"), e("- hire\n", "- hire a designer\n")]).unwrap();
        assert_eq!(out, "# Plan\n\n- ship v1 by Friday\n- hire a designer\n");
    }

    #[test]
    fn rejects_missing_ambiguous_noop_and_full_rewrites() {
        let body = "a\nb\na\n";
        assert!(apply_edits(body, &[e("zzz", "y")]).unwrap_err().contains("not in the note"));
        assert!(apply_edits(body, &[e("a\n", "c\n")]).unwrap_err().contains("appears 2 times"));
        assert!(apply_edits(body, &[e("b", "b")]).unwrap_err().contains("do not change"));
        assert!(apply_edits(body, &[]).is_err());
        let long = "word ".repeat(200);
        assert!(apply_edits(&long, &[e(long.trim(), "short")]).unwrap_err().contains("not the whole note"));
    }

    #[test]
    fn empty_find_appends() {
        assert_eq!(apply_edits("", &[e("", "## Action items")]).unwrap(), "## Action items");
        assert_eq!(apply_edits("Intro\n", &[e("", "More")]).unwrap(), "Intro\n\nMore");
    }

    #[test]
    fn rebase_applies_onto_unrelated_user_changes() {
        let before = "# Notes\nalpha\nbeta\ngamma\ndelta\n";
        let target = "# Notes\nalpha\nBETA\ngamma\ndelta\n";
        let current = "# Notes\nalpha\nbeta\ngamma\ndelta\nuser added line\n";
        assert_eq!(rebase(before, target, current).unwrap(), "# Notes\nalpha\nBETA\ngamma\ndelta\nuser added line\n");
    }

    #[test]
    fn rebase_refuses_conflicting_changes() {
        let before = "a\nbeta\nc\n";
        let target = "a\nBETA\nc\n";
        let current = "a\nbeta edited by you\nc\n";
        assert_eq!(rebase(before, target, current), None);
    }

    #[test]
    fn rebase_handles_partial_hunks_and_inserts() {
        let before = "one\ntwo\nthree\n";
        let target = "zero\none\ntwo\nthree\nfour\n";
        let current = "one\ntwo\nthree\n\nmine\n";
        // Insert at the start anchors on "one"; insert after "three" anchors on "three\n".
        assert_eq!(rebase(before, target, current).unwrap(), "zero\none\ntwo\nthree\nfour\n\nmine\n");
    }
}
