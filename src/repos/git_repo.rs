//! Thin wrapper around `git2` for the repository operations prompt-bom needs.
//!
//! Returns plain data types (`BlameLine`) so the services layer never sees a
//! `git2::Blame` directly. Errors that mean "file isn't tracked yet" surface
//! as an empty `Vec`, not as `Err`, because skipping uncommitted attributions
//! is the normal flow — only programmer / IO errors should propagate.

use crate::error::{AppError, Result};
use git2::Repository;
use std::path::Path;

pub fn open_repo(path: &Path) -> Result<Repository> {
    Ok(Repository::open(path)?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    pub line_number: usize,
    pub commit_oid: String,
    pub commit_time_unix: i64,
    pub author_email: Option<String>,
}

pub fn blame_file(repo: &Repository, file_path: &Path) -> Result<Vec<BlameLine>> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Config("bare repository — blame requires a workdir".into()))?;
    let workdir_canonical = workdir
        .canonicalize()
        .unwrap_or_else(|_| workdir.to_path_buf());

    let absolute = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        workdir.join(file_path)
    };
    let abs_canonical = absolute.canonicalize().unwrap_or(absolute);

    let relative = match abs_canonical.strip_prefix(&workdir_canonical) {
        Ok(p) => p.to_path_buf(),
        Err(_) => return Ok(Vec::new()),
    };

    let blame = match repo.blame_file(&relative, None) {
        Ok(b) => b,
        Err(_) => return Ok(Vec::new()),
    };

    let mut lines = Vec::new();
    for hunk in blame.iter() {
        let oid = hunk.final_commit_id().to_string();
        let sig = hunk.final_signature();
        let email = sig.email().map(str::to_owned);
        let time_unix = sig.when().seconds();
        let start = hunk.final_start_line();
        let count = hunk.lines_in_hunk();
        for offset in 0..count {
            lines.push(BlameLine {
                line_number: start + offset,
                commit_oid: oid.clone(),
                commit_time_unix: time_unix,
                author_email: email.clone(),
            });
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn blame_file_returns_empty_for_untracked_file() {
        let dir = tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let file_path = dir.path().join("untracked.rs");
        fs::write(&file_path, "fn main() {}\n").unwrap();

        let result = blame_file(&repo, &file_path).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn blame_file_returns_empty_for_path_outside_workdir() {
        let dir = tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let outside = tempdir().unwrap().path().join("foreign.rs");
        let result = blame_file(&repo, &outside).unwrap();
        assert!(result.is_empty());
    }
}
