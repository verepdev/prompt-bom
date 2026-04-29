use git2::{Repository, Signature};
use prompt_bom::schemas::attribution::{AttributionRecord, ClaudeTool, EditOperation, ToolPayload};
use prompt_bom::services::blame::find_attributed_ranges;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn init_repo() -> (TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    {
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test User").unwrap();
        cfg.set_str("user.email", "test@example.com").unwrap();
    }
    (dir, repo)
}

fn commit_file(repo: &Repository, relative: &str, contents: &str, msg: &str) {
    let workdir = repo.workdir().unwrap().to_path_buf();
    let path = workdir.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, contents).unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new(relative)).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let sig = Signature::now("Test User", "test@example.com").unwrap();
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .into_iter()
        .collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parent_refs)
        .unwrap();
}

fn write_record(file_path: &Path, content: &str) -> AttributionRecord {
    AttributionRecord {
        session_id: "sess-1".into(),
        uuid: "u-write-1".into(),
        parent_uuid: None,
        timestamp: "2026-04-28T10:00:00Z".into(),
        model: "claude-sonnet-4-6".into(),
        tool_use_id: "toolu_w1".into(),
        tool: ClaudeTool::Write,
        file_path: file_path.to_path_buf(),
        payload: ToolPayload::Write {
            content: content.into(),
        },
        git_branch: Some("main".into()),
        cwd: None,
    }
}

#[test]
fn write_inside_committed_file_yields_one_range_with_blame() {
    let (dir, repo) = init_repo();
    let body = "fn hello() {\n    println!(\"hi\");\n}\n";
    commit_file(&repo, "src/lib.rs", body, "init");

    let abs = dir.path().join("src/lib.rs");
    let record = write_record(&abs, body);

    let ranges = find_attributed_ranges(&[record], dir.path()).unwrap();
    assert_eq!(ranges.len(), 1);
    let r = &ranges[0];
    assert_eq!(r.file_path, PathBuf::from("src").join("lib.rs"));
    assert_eq!(r.start_line, 1);
    assert_eq!(r.end_line, 3);
    let blame = r.blame.as_ref().expect("committed file should have blame");
    assert!(!blame.first_commit_oid.is_empty());
    assert_eq!(blame.author_emails, vec!["test@example.com".to_string()]);
}

#[test]
fn edit_needle_present_in_current_content_matches() {
    let (dir, repo) = init_repo();
    let body = "alpha\nbeta\ngamma\n";
    commit_file(&repo, "notes.txt", body, "init");

    let abs = dir.path().join("notes.txt");
    let record = AttributionRecord {
        session_id: "s".into(),
        uuid: "u-edit".into(),
        parent_uuid: None,
        timestamp: "2026-04-28T10:00:00Z".into(),
        model: "claude-sonnet-4-6".into(),
        tool_use_id: "t".into(),
        tool: ClaudeTool::Edit,
        file_path: abs,
        payload: ToolPayload::Edit {
            old_string: "beta".into(),
            new_string: "beta".into(),
            replace_all: false,
        },
        git_branch: None,
        cwd: None,
    };

    let ranges = find_attributed_ranges(&[record], dir.path()).unwrap();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start_line, 2);
    assert_eq!(ranges[0].end_line, 2);
    assert_eq!(
        ranges[0].content_hash,
        blake3::hash(b"beta").to_hex().to_string()
    );
}

#[test]
fn multiedit_yields_one_range_per_edit() {
    let (dir, repo) = init_repo();
    let body = "alpha\nbeta\ngamma\ndelta\n";
    commit_file(&repo, "lines.txt", body, "init");

    let abs = dir.path().join("lines.txt");
    let record = AttributionRecord {
        session_id: "s".into(),
        uuid: "u-multi".into(),
        parent_uuid: None,
        timestamp: "2026-04-28T10:00:00Z".into(),
        model: "claude-sonnet-4-6".into(),
        tool_use_id: "t".into(),
        tool: ClaudeTool::MultiEdit,
        file_path: abs,
        payload: ToolPayload::MultiEdit {
            edits: vec![
                EditOperation {
                    old_string: "alpha".into(),
                    new_string: "alpha".into(),
                    replace_all: false,
                },
                EditOperation {
                    old_string: "delta".into(),
                    new_string: "delta".into(),
                    replace_all: false,
                },
            ],
        },
        git_branch: None,
        cwd: None,
    };

    let ranges = find_attributed_ranges(&[record], dir.path()).unwrap();
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0].start_line, 1);
    assert_eq!(ranges[1].start_line, 4);
}

#[test]
fn record_pointing_outside_repo_is_skipped() {
    let (dir, _repo) = init_repo();
    let other_dir = tempfile::tempdir().unwrap();
    let elsewhere = other_dir.path().join("elsewhere.rs");
    fs::write(&elsewhere, "fn x() {}\n").unwrap();

    let record = write_record(&elsewhere, "fn x() {}\n");

    let ranges = find_attributed_ranges(&[record], dir.path()).unwrap();
    assert!(ranges.is_empty());
}

#[test]
fn record_for_deleted_file_is_skipped() {
    let (dir, _repo) = init_repo();
    let abs = dir.path().join("ghost.rs");
    let record = write_record(&abs, "fn ghost() {}\n");

    let ranges = find_attributed_ranges(&[record], dir.path()).unwrap();
    assert!(ranges.is_empty());
}

#[test]
fn untracked_file_yields_range_without_blame() {
    let (dir, _repo) = init_repo();
    let abs = dir.path().join("scratch.rs");
    let body = "let x = 42;\n";
    fs::write(&abs, body).unwrap();

    let record = write_record(&abs, body);
    let ranges = find_attributed_ranges(&[record], dir.path()).unwrap();
    assert_eq!(ranges.len(), 1);
    assert!(ranges[0].blame.is_none());
}
