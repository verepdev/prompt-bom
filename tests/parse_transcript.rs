use prompt_bom::schemas::attribution::{ClaudeTool, ToolPayload};
use prompt_bom::services::transcript::parse_claude_code_jsonl;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn parses_sample_transcript_into_four_records() {
    let path = fixture("claude_code_sample.jsonl");
    let records = parse_claude_code_jsonl(&path).expect("parse should succeed");

    let tools: Vec<_> = records.iter().map(|r| r.tool.clone()).collect();
    assert_eq!(
        tools,
        vec![
            ClaudeTool::Write,
            ClaudeTool::Edit,
            ClaudeTool::MultiEdit,
            ClaudeTool::NotebookEdit,
        ],
        "expected exactly the four mutating tool invocations, in order"
    );

    let write = &records[0];
    assert_eq!(write.session_id, "sess-fixture");
    assert_eq!(write.model, "claude-sonnet-4-6");
    assert_eq!(write.git_branch.as_deref(), Some("main"));
    assert_eq!(write.cwd.as_deref(), Some(Path::new("/repo")));
    assert_eq!(write.tool_use_id, "toolu_w1");
    match &write.payload {
        ToolPayload::Write { content } => assert!(content.contains("hello")),
        other => panic!("expected Write payload, got {other:?}"),
    }

    let edit = &records[1];
    match &edit.payload {
        ToolPayload::Edit {
            old_string,
            new_string,
            replace_all,
        } => {
            assert_eq!(old_string, "\"hi\"");
            assert_eq!(new_string, "\"hello\"");
            assert!(!replace_all);
        }
        other => panic!("expected Edit payload, got {other:?}"),
    }

    let multi = &records[2];
    match &multi.payload {
        ToolPayload::MultiEdit { edits } => {
            assert_eq!(edits.len(), 2);
            assert_eq!(edits[0].old_string, "foo");
            assert!(!edits[0].replace_all);
            assert!(edits[1].replace_all);
        }
        other => panic!("expected MultiEdit, got {other:?}"),
    }

    let nb = &records[3];
    match &nb.payload {
        ToolPayload::NotebookEdit {
            new_source,
            cell_id,
            edit_mode,
        } => {
            assert!(new_source.contains("import math"));
            assert_eq!(cell_id.as_deref(), Some("cell-1"));
            assert_eq!(edit_mode.as_deref(), Some("replace"));
        }
        other => panic!("expected NotebookEdit, got {other:?}"),
    }
    assert_eq!(nb.file_path, PathBuf::from("notebook.ipynb"));
}

#[test]
fn missing_session_id_drops_the_record() {
    let path = fixture("claude_code_sample.jsonl");
    let records = parse_claude_code_jsonl(&path).unwrap();
    assert!(
        !records.iter().any(|r| r.uuid == "u-8"),
        "u-8 lacks sessionId and must be skipped"
    );
}

#[test]
fn malformed_line_does_not_abort_parsing() {
    let path = fixture("claude_code_sample.jsonl");
    let records = parse_claude_code_jsonl(&path).expect("malformed line should not error out");
    assert_eq!(records.len(), 4);
}
