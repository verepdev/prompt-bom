//! Parse Claude Code JSONL session transcripts into `AttributionRecord`s.
//!
//! Algorithm: walk each line, keep only `type == "assistant"` messages, then
//! for each `tool_use` content item whose tool name matches one of the four
//! file-mutating tools, emit one `AttributionRecord`. Lines with missing or
//! malformed metadata are silently skipped — partial transcripts are common
//! around session boundaries and we'd rather lose one record than refuse to
//! parse a 30 MB log.

use crate::error::Result;
use crate::repos::jsonl_reader;
use crate::schemas::attribution::{AttributionRecord, ClaudeTool, EditOperation, ToolPayload};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub fn parse_claude_code_jsonl(path: &Path) -> Result<Vec<AttributionRecord>> {
    let file = File::open(path)?;
    parse_from_reader(BufReader::new(file))
}

pub fn parse_from_reader<R: BufRead>(reader: R) -> Result<Vec<AttributionRecord>> {
    let mut out = Vec::new();
    for value_res in jsonl_reader::read_records(reader) {
        match value_res {
            Ok(value) => out.extend(extract_records(&value)),
            Err(e) => tracing::warn!("skipping malformed transcript line: {e}"),
        }
    }
    Ok(out)
}

fn extract_records(line: &Value) -> Vec<AttributionRecord> {
    if line.get("type").and_then(Value::as_str) != Some("assistant") {
        return Vec::new();
    }
    let (Some(session_id), Some(uuid)) = (
        line.get("sessionId").and_then(Value::as_str),
        line.get("uuid").and_then(Value::as_str),
    ) else {
        return Vec::new();
    };
    let timestamp = line.get("timestamp").and_then(Value::as_str).unwrap_or("");
    let parent_uuid = line.get("parentUuid").and_then(Value::as_str);
    let git_branch = line.get("gitBranch").and_then(Value::as_str);
    let cwd = line.get("cwd").and_then(Value::as_str);
    let model = line
        .pointer("/message/model")
        .and_then(Value::as_str)
        .unwrap_or("");

    let Some(content) = line.pointer("/message/content").and_then(Value::as_array) else {
        return Vec::new();
    };

    content
        .iter()
        .filter_map(|item| {
            let (tool, payload, file_path, tool_use_id) = parse_tool_use(item)?;
            Some(AttributionRecord {
                session_id: session_id.to_string(),
                uuid: uuid.to_string(),
                parent_uuid: parent_uuid.map(String::from),
                timestamp: timestamp.to_string(),
                model: model.to_string(),
                tool_use_id,
                tool,
                file_path,
                payload,
                git_branch: git_branch.map(String::from),
                cwd: cwd.map(PathBuf::from),
            })
        })
        .collect()
}

fn parse_tool_use(item: &Value) -> Option<(ClaudeTool, ToolPayload, PathBuf, String)> {
    if item.get("type").and_then(Value::as_str) != Some("tool_use") {
        return None;
    }
    let tool_name = item.get("name").and_then(Value::as_str)?;
    let tool_use_id = item.get("id").and_then(Value::as_str)?.to_string();
    let input = item.get("input")?;
    let file_path = input
        .get("file_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)?;

    let (tool, payload) = match tool_name {
        "Write" => {
            let content = input.get("content").and_then(Value::as_str)?.to_string();
            (ClaudeTool::Write, ToolPayload::Write { content })
        }
        "Edit" => {
            let old_string = input.get("old_string").and_then(Value::as_str)?.to_string();
            let new_string = input.get("new_string").and_then(Value::as_str)?.to_string();
            let replace_all = input
                .get("replace_all")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            (
                ClaudeTool::Edit,
                ToolPayload::Edit {
                    old_string,
                    new_string,
                    replace_all,
                },
            )
        }
        "MultiEdit" => {
            let edits_arr = input.get("edits").and_then(Value::as_array)?;
            let edits = edits_arr.iter().filter_map(parse_edit_op).collect();
            (ClaudeTool::MultiEdit, ToolPayload::MultiEdit { edits })
        }
        "NotebookEdit" => {
            let new_source = input.get("new_source").and_then(Value::as_str)?.to_string();
            let cell_id = input
                .get("cell_id")
                .and_then(Value::as_str)
                .map(String::from);
            let edit_mode = input
                .get("edit_mode")
                .and_then(Value::as_str)
                .map(String::from);
            (
                ClaudeTool::NotebookEdit,
                ToolPayload::NotebookEdit {
                    new_source,
                    cell_id,
                    edit_mode,
                },
            )
        }
        _ => return None,
    };

    Some((tool, payload, file_path, tool_use_id))
}

fn parse_edit_op(value: &Value) -> Option<EditOperation> {
    Some(EditOperation {
        old_string: value.get("old_string").and_then(Value::as_str)?.to_string(),
        new_string: value.get("new_string").and_then(Value::as_str)?.to_string(),
        replace_all: value
            .get("replace_all")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn assistant_line_with_write() -> &'static str {
        r#"{"type":"assistant","sessionId":"sess-1","uuid":"u-1","timestamp":"2026-04-28T10:00:00Z","gitBranch":"main","cwd":"C:/repo","message":{"model":"claude-sonnet-4-6","content":[{"type":"tool_use","id":"toolu_1","name":"Write","input":{"file_path":"src/lib.rs","content":"pub fn hello() {}\n"}}]}}"#
    }

    #[test]
    fn parses_assistant_write_tool_use() {
        let records = parse_from_reader(Cursor::new(assistant_line_with_write())).unwrap();
        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert_eq!(r.session_id, "sess-1");
        assert_eq!(r.uuid, "u-1");
        assert_eq!(r.model, "claude-sonnet-4-6");
        assert_eq!(r.tool, ClaudeTool::Write);
        assert_eq!(r.file_path, PathBuf::from("src/lib.rs"));
        assert_eq!(r.git_branch.as_deref(), Some("main"));
        match &r.payload {
            ToolPayload::Write { content } => assert!(content.contains("hello")),
            other => panic!("expected Write payload, got {other:?}"),
        }
    }

    #[test]
    fn user_and_system_lines_are_filtered_out() {
        let input = r#"{"type":"user","sessionId":"s","uuid":"u","message":{"content":[]}}
{"type":"system","sessionId":"s","uuid":"u2"}
"#;
        let records = parse_from_reader(Cursor::new(input)).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn assistant_without_tool_use_yields_no_records() {
        let input = r#"{"type":"assistant","sessionId":"s","uuid":"u","timestamp":"t","message":{"model":"m","content":[{"type":"text","text":"hi"}]}}"#;
        let records = parse_from_reader(Cursor::new(input)).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn unknown_tool_name_is_skipped_silently() {
        let input = r#"{"type":"assistant","sessionId":"s","uuid":"u","timestamp":"t","message":{"model":"m","content":[{"type":"tool_use","id":"t1","name":"BashFutureTool","input":{"file_path":"x","cmd":"y"}}]}}"#;
        let records = parse_from_reader(Cursor::new(input)).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn missing_session_id_skips_line() {
        let input = r#"{"type":"assistant","uuid":"u","message":{"model":"m","content":[]}}"#;
        let records = parse_from_reader(Cursor::new(input)).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn parses_edit_payload() {
        let input = r#"{"type":"assistant","sessionId":"s","uuid":"u","timestamp":"t","message":{"model":"m","content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"a.rs","old_string":"foo","new_string":"bar","replace_all":true}}]}}"#;
        let records = parse_from_reader(Cursor::new(input)).unwrap();
        assert_eq!(records.len(), 1);
        match &records[0].payload {
            ToolPayload::Edit {
                old_string,
                new_string,
                replace_all,
            } => {
                assert_eq!(old_string, "foo");
                assert_eq!(new_string, "bar");
                assert!(*replace_all);
            }
            other => panic!("expected Edit payload, got {other:?}"),
        }
    }

    #[test]
    fn parses_multiedit_with_default_replace_all() {
        let input = r#"{"type":"assistant","sessionId":"s","uuid":"u","timestamp":"t","message":{"model":"m","content":[{"type":"tool_use","id":"t1","name":"MultiEdit","input":{"file_path":"a.rs","edits":[{"old_string":"a","new_string":"b"},{"old_string":"c","new_string":"d","replace_all":true}]}}]}}"#;
        let records = parse_from_reader(Cursor::new(input)).unwrap();
        assert_eq!(records.len(), 1);
        match &records[0].payload {
            ToolPayload::MultiEdit { edits } => {
                assert_eq!(edits.len(), 2);
                assert!(!edits[0].replace_all);
                assert!(edits[1].replace_all);
            }
            other => panic!("expected MultiEdit, got {other:?}"),
        }
    }
}
