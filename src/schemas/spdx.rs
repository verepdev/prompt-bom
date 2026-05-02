//! SPDX 2.3 document shape, hand-rolled.
//!
//! Pinned to SPDX 2.3 (`https://spdx.github.io/spdx-spec/v2.3/`). prompt-bom
//! emits only the subset listed below; fields not used here are intentionally
//! absent rather than `Option<T>` carrying `None`, keeping the wire JSON
//! dense and diff-friendly.
//!
//! AI provenance is carried as a structured payload inside a `Snippet`
//! annotation: `annotationType = "OTHER"`, `annotator = "Tool: prompt-bom-<v>"`,
//! `comment = <JSON-encoded AiProvenance>`. This keeps the document SPDX 2.3
//! valid and lets downstream tooling parse the comment back into
//! `AiProvenance` without a new schema namespace.
//!
//! Field naming uses SPDX's canonical camelCase. Top-level `SPDXID` is the
//! one exception SPDX spells in upper-case.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdxDocument {
    pub spdx_version: String,
    pub data_license: String,
    #[serde(rename = "SPDXID")]
    pub spdxid: String,
    pub name: String,
    pub document_namespace: String,
    pub creation_info: CreationInfo,
    pub packages: Vec<SpdxPackage>,
    pub files: Vec<SpdxFile>,
    pub snippets: Vec<SpdxSnippet>,
    pub relationships: Vec<Relationship>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreationInfo {
    pub creators: Vec<String>,
    pub created: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdxPackage {
    #[serde(rename = "SPDXID")]
    pub spdxid: String,
    pub name: String,
    pub download_location: String,
    pub files_analyzed: bool,
    pub primary_package_purpose: String,
    pub license_concluded: String,
    pub copyright_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdxFile {
    #[serde(rename = "SPDXID")]
    pub spdxid: String,
    pub file_name: String,
    pub checksums: Vec<Checksum>,
    pub license_concluded: String,
    pub copyright_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checksum {
    pub algorithm: String,
    pub checksum_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdxSnippet {
    #[serde(rename = "SPDXID")]
    pub spdxid: String,
    pub snippet_from_file: String,
    pub ranges: Vec<SnippetRange>,
    pub license_concluded: String,
    pub copyright_text: String,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetRange {
    pub start_pointer: SnippetPointer,
    pub end_pointer: SnippetPointer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetPointer {
    pub reference: String,
    pub line_number: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub annotation_type: String,
    pub annotator: String,
    pub annotation_date: String,
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub spdx_element_id: String,
    pub relationship_type: String,
    pub related_spdx_element: String,
}

/// Structured payload encoded as JSON inside a snippet annotation's `comment`.
///
/// Round-trips: produced by `services::spdx_emit`, consumed by any reader that
/// wants to lift AI provenance back out of an SPDX document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProvenance {
    pub model: String,
    pub session_id: String,
    pub attribution_uuid: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blame: Option<BlameSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameSummary {
    pub first_commit_oid: String,
    pub last_commit_oid: String,
    pub author_emails: Vec<String>,
}
