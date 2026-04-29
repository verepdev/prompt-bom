//! Attribution records derived from a Claude Code session transcript.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One AI-attributed file modification observed in a Claude Code session.
///
/// Each record corresponds to a single `tool_use` invocation of `Write`,
/// `Edit`, `MultiEdit`, or `NotebookEdit` by the assistant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttributionRecord {
    pub session_id: String,
    pub uuid: String,
    pub parent_uuid: Option<String>,
    pub timestamp: String,
    pub model: String,
    pub tool_use_id: String,
    pub tool: ClaudeTool,
    pub file_path: PathBuf,
    pub payload: ToolPayload,
    pub git_branch: Option<String>,
    pub cwd: Option<PathBuf>,
}

/// Claude Code tool that produced the attribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaudeTool {
    Write,
    Edit,
    MultiEdit,
    NotebookEdit,
}

/// Tool-specific payload carried alongside an attribution record.
///
/// Raw strings are preserved at this stage; M3 hashes them before emitting
/// SPDX-AI snippets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolPayload {
    Write {
        content: String,
    },
    Edit {
        old_string: String,
        new_string: String,
        #[serde(default)]
        replace_all: bool,
    },
    MultiEdit {
        edits: Vec<EditOperation>,
    },
    NotebookEdit {
        new_source: String,
        cell_id: Option<String>,
        edit_mode: Option<String>,
    },
}

/// One edit inside a `MultiEdit` invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditOperation {
    pub old_string: String,
    pub new_string: String,
    #[serde(default)]
    pub replace_all: bool,
}

/// A line range in a repository file that an `AttributionRecord` resolved to.
///
/// `start_line` and `end_line` are 1-based and inclusive. `file_path` is
/// stored relative to the repository root so the same record stays valid
/// regardless of where the repo is checked out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotatedRange {
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub content_hash: String,
    pub attribution_uuid: String,
    pub model: String,
    pub session_id: String,
    pub timestamp: String,
    pub blame: Option<BlameInfo>,
}

/// Git-blame summary for the lines covered by an `AnnotatedRange`.
///
/// `None` on the parent `AnnotatedRange` means the file isn't tracked yet
/// or no commit has touched the matched lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlameInfo {
    pub first_commit_oid: String,
    pub last_commit_oid: String,
    pub author_emails: Vec<String>,
}
