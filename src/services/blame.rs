//! Join `AttributionRecord`s from a Claude Code transcript with the current
//! state of files in a git repository.
//!
//! Strategy for the M2 baseline: substring-exact-match. For each record, look
//! at the AI-written needle (a `Write` content, an `Edit` `new_string`, the
//! `new_string` of each `MultiEdit` op, or a `NotebookEdit` `new_source`) and
//! locate it inside the current file content. If found, the matching line
//! range becomes an `AnnotatedRange`. Optional blame data is attached when
//! the file has been committed.
//!
//! Hand-edits after the AI run will silently drop a record from the output —
//! that's the conservative choice for compliance use, where a wrong "AI
//! wrote this" claim is more damaging than a missing one.

use crate::error::Result;
use crate::repos::git_repo::{self, BlameLine};
use crate::schemas::attribution::{AnnotatedRange, AttributionRecord, BlameInfo, ToolPayload};
use std::path::{Path, PathBuf};

pub fn find_attributed_ranges(
    records: &[AttributionRecord],
    repo_root: &Path,
) -> Result<Vec<AnnotatedRange>> {
    let repo = git_repo::open_repo(repo_root)?;
    let repo_canonical = repo_root.canonicalize()?;
    let mut out = Vec::new();

    for record in records {
        let Some(abs_path) =
            resolve_to_repo(&record.file_path, &repo_canonical, record.cwd.as_deref())
        else {
            tracing::debug!(
                "skipping record {}: file_path {:?} not inside repo {:?}",
                record.uuid,
                record.file_path,
                repo_canonical
            );
            continue;
        };

        let content = match std::fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(
                    "skipping record {}: read {:?} failed: {e}",
                    record.uuid,
                    abs_path
                );
                continue;
            }
        };

        let blame_lines = git_repo::blame_file(&repo, &abs_path).unwrap_or_default();

        let relative = abs_path
            .strip_prefix(&repo_canonical)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| abs_path.clone());

        for needle in extract_needles(&record.payload) {
            if needle.is_empty() {
                continue;
            }
            let Some((start_line, end_line)) = find_substring_lines(&content, &needle) else {
                continue;
            };
            let blame = enrich_with_blame(&blame_lines, start_line, end_line);
            out.push(AnnotatedRange {
                file_path: relative.clone(),
                start_line,
                end_line,
                content_hash: blake3::hash(needle.as_bytes()).to_hex().to_string(),
                attribution_uuid: record.uuid.clone(),
                model: record.model.clone(),
                session_id: record.session_id.clone(),
                timestamp: record.timestamp.clone(),
                blame,
            });
        }
    }
    Ok(out)
}

fn extract_needles(payload: &ToolPayload) -> Vec<String> {
    match payload {
        ToolPayload::Write { content } => vec![content.clone()],
        ToolPayload::Edit { new_string, .. } => vec![new_string.clone()],
        ToolPayload::MultiEdit { edits } => edits
            .iter()
            .filter(|e| !e.new_string.is_empty())
            .map(|e| e.new_string.clone())
            .collect(),
        ToolPayload::NotebookEdit { new_source, .. } => vec![new_source.clone()],
    }
}

/// Locate `needle` inside `haystack` and return the inclusive 1-based line
/// range it occupies. Returns `None` if `needle` is empty or absent.
fn find_substring_lines(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    let offset = haystack.find(needle)?;
    let start_line = haystack[..offset].matches('\n').count() + 1;
    let internal_newlines = needle.matches('\n').count();
    let end_line = if needle.ends_with('\n') {
        start_line + internal_newlines.saturating_sub(1)
    } else {
        start_line + internal_newlines
    };
    Some((start_line, end_line))
}

fn resolve_to_repo(file_path: &Path, repo_canonical: &Path, cwd: Option<&Path>) -> Option<PathBuf> {
    let candidate = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        cwd.unwrap_or(repo_canonical).join(file_path)
    };
    let canonical = candidate.canonicalize().ok()?;
    canonical.starts_with(repo_canonical).then_some(canonical)
}

fn enrich_with_blame(
    blame_lines: &[BlameLine],
    start_line: usize,
    end_line: usize,
) -> Option<BlameInfo> {
    let in_range: Vec<&BlameLine> = blame_lines
        .iter()
        .filter(|b| b.line_number >= start_line && b.line_number <= end_line)
        .collect();
    if in_range.is_empty() {
        return None;
    }

    let earliest = in_range.iter().min_by_key(|b| b.commit_time_unix)?;
    let latest = in_range.iter().max_by_key(|b| b.commit_time_unix)?;

    let mut emails: Vec<String> = in_range
        .iter()
        .filter_map(|b| b.author_email.clone())
        .collect();
    emails.sort();
    emails.dedup();

    Some(BlameInfo {
        first_commit_oid: earliest.commit_oid.clone(),
        last_commit_oid: latest.commit_oid.clone(),
        author_emails: emails,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn find_substring_lines_single_line() {
        let h = "alpha\nbeta\ngamma\n";
        assert_eq!(find_substring_lines(h, "beta"), Some((2, 2)));
    }

    #[test]
    fn find_substring_lines_multi_line_no_trailing_newline() {
        let h = "a\nb\nc\nd\n";
        assert_eq!(find_substring_lines(h, "b\nc"), Some((2, 3)));
    }

    #[test]
    fn find_substring_lines_multi_line_with_trailing_newline() {
        let h = "a\nb\nc\nd\n";
        assert_eq!(find_substring_lines(h, "b\nc\n"), Some((2, 3)));
    }

    #[test]
    fn find_substring_lines_empty_needle_returns_none() {
        assert_eq!(find_substring_lines("anything", ""), None);
    }

    #[test]
    fn find_substring_lines_absent_needle_returns_none() {
        assert_eq!(find_substring_lines("hello", "world"), None);
    }

    #[test]
    fn enrich_with_blame_picks_earliest_and_latest_by_time() {
        let blame = vec![
            BlameLine {
                line_number: 5,
                commit_oid: "old".into(),
                commit_time_unix: 1_000,
                author_email: Some("a@x".into()),
            },
            BlameLine {
                line_number: 6,
                commit_oid: "new".into(),
                commit_time_unix: 2_000,
                author_email: Some("b@x".into()),
            },
            BlameLine {
                line_number: 99,
                commit_oid: "out-of-range".into(),
                commit_time_unix: 3_000,
                author_email: Some("c@x".into()),
            },
        ];
        let info = enrich_with_blame(&blame, 5, 6).unwrap();
        assert_eq!(info.first_commit_oid, "old");
        assert_eq!(info.last_commit_oid, "new");
        assert_eq!(
            info.author_emails,
            vec!["a@x".to_string(), "b@x".to_string()]
        );
    }

    #[test]
    fn enrich_with_blame_returns_none_when_no_lines_in_range() {
        let blame = vec![BlameLine {
            line_number: 1,
            commit_oid: "x".into(),
            commit_time_unix: 0,
            author_email: None,
        }];
        assert!(enrich_with_blame(&blame, 50, 60).is_none());
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, .. ProptestConfig::default() })]

        #[test]
        fn find_substring_lines_does_not_panic(
            haystack in "[\\PC]{0,400}",
            needle in "[\\PC]{0,80}",
        ) {
            let _ = find_substring_lines(&haystack, &needle);
        }
    }
}
