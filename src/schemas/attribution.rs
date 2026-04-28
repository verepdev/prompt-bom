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
