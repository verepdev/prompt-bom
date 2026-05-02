//! Emit a deterministic SPDX 2.3 document from a list of `AnnotatedRange`s.
//!
//! Pure function: takes ranges plus a path→content map and an `EmitOptions`,
//! returns an `SpdxDocument`. Filesystem reads happen in `collect_file_contents`
//! so unit tests can exercise emission without touching disk.
//!
//! Determinism is mandatory: repeated runs over the same inputs must produce
//! byte-identical JSON. We rely on:
//! - `BTreeSet` over file paths for stable file ordering and SPDXID assignment.
//! - Snippet order = caller-supplied range order.
//! - All timestamps and tool ids passed through `EmitOptions`, never read from
//!   the system clock.

use crate::error::{AppError, Result};
use crate::schemas::attribution::AnnotatedRange;
use crate::schemas::spdx::{
    AiProvenance, Annotation, BlameSummary, Checksum, CreationInfo, Relationship, SnippetPointer,
    SnippetRange, SpdxDocument, SpdxFile, SpdxPackage, SpdxSnippet,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct EmitOptions {
    pub project_name: String,
    pub document_namespace: String,
    pub created_iso: String,
    pub tool_id: String,
}

/// Build an `SpdxDocument` from the existing pipeline output.
///
/// `file_contents` must contain every distinct `file_path` referenced by
/// `ranges`. Missing entries return `AppError::Config` rather than producing a
/// document with a placeholder hash — a wrong checksum is worse than a hard
/// failure for compliance use.
pub fn emit_spdx(
    ranges: &[AnnotatedRange],
    file_contents: &BTreeMap<PathBuf, Vec<u8>>,
    opts: &EmitOptions,
) -> Result<SpdxDocument> {
    let unique_paths: BTreeSet<&PathBuf> = ranges.iter().map(|r| &r.file_path).collect();

    let mut files = Vec::with_capacity(unique_paths.len());
    let mut path_to_spdxid: BTreeMap<PathBuf, String> = BTreeMap::new();
    for (idx, path) in unique_paths.iter().enumerate() {
        let bytes = file_contents.get(*path).ok_or_else(|| {
            AppError::Config(format!(
                "spdx_emit: missing file content for {}",
                display_path(path)
            ))
        })?;
        let spdxid = format!("SPDXRef-File-{}", idx + 1);
        files.push(SpdxFile {
            spdxid: spdxid.clone(),
            file_name: spdx_file_name(path),
            checksums: vec![Checksum {
                algorithm: "BLAKE3".into(),
                checksum_value: blake3::hash(bytes).to_hex().to_string(),
            }],
            license_concluded: "NOASSERTION".into(),
            copyright_text: "NOASSERTION".into(),
        });
        path_to_spdxid.insert((*path).clone(), spdxid);
    }

    let mut snippets = Vec::with_capacity(ranges.len());
    for (idx, range) in ranges.iter().enumerate() {
        let file_spdxid = path_to_spdxid.get(&range.file_path).ok_or_else(|| {
            AppError::Config(format!(
                "spdx_emit: range references unknown file {}",
                display_path(&range.file_path)
            ))
        })?;
        let provenance = AiProvenance {
            model: range.model.clone(),
            session_id: range.session_id.clone(),
            attribution_uuid: range.attribution_uuid.clone(),
            timestamp: range.timestamp.clone(),
            blame: range.blame.as_ref().map(|b| BlameSummary {
                first_commit_oid: b.first_commit_oid.clone(),
                last_commit_oid: b.last_commit_oid.clone(),
                author_emails: b.author_emails.clone(),
            }),
        };
        snippets.push(SpdxSnippet {
            spdxid: format!("SPDXRef-Snippet-{}", idx + 1),
            snippet_from_file: file_spdxid.clone(),
            ranges: vec![SnippetRange {
                start_pointer: SnippetPointer {
                    reference: file_spdxid.clone(),
                    line_number: range.start_line,
                },
                end_pointer: SnippetPointer {
                    reference: file_spdxid.clone(),
                    line_number: range.end_line,
                },
            }],
            license_concluded: "NOASSERTION".into(),
            copyright_text: "NOASSERTION".into(),
            annotations: vec![Annotation {
                annotation_type: "OTHER".into(),
                annotator: opts.tool_id.clone(),
                annotation_date: range.timestamp.clone(),
                comment: serde_json::to_string(&provenance)?,
            }],
        });
    }

    let package = SpdxPackage {
        spdxid: "SPDXRef-Package-Project".into(),
        name: opts.project_name.clone(),
        download_location: "NOASSERTION".into(),
        files_analyzed: false,
        primary_package_purpose: "SOURCE".into(),
        license_concluded: "NOASSERTION".into(),
        copyright_text: "NOASSERTION".into(),
    };

    let mut relationships = Vec::with_capacity(1 + files.len());
    relationships.push(Relationship {
        spdx_element_id: "SPDXRef-DOCUMENT".into(),
        relationship_type: "DESCRIBES".into(),
        related_spdx_element: package.spdxid.clone(),
    });
    for f in &files {
        relationships.push(Relationship {
            spdx_element_id: package.spdxid.clone(),
            relationship_type: "CONTAINS".into(),
            related_spdx_element: f.spdxid.clone(),
        });
    }

    Ok(SpdxDocument {
        spdx_version: "SPDX-2.3".into(),
        data_license: "CC0-1.0".into(),
        spdxid: "SPDXRef-DOCUMENT".into(),
        name: opts.project_name.clone(),
        document_namespace: opts.document_namespace.clone(),
        creation_info: CreationInfo {
            creators: vec![opts.tool_id.clone()],
            created: opts.created_iso.clone(),
        },
        packages: vec![package],
        files,
        snippets,
        relationships,
    })
}

/// Read every distinct file referenced by `ranges` from `repo_root`. Each
/// `file_path` is treated as relative to `repo_root` (it already is, by
/// `find_attributed_ranges` contract).
pub fn collect_file_contents(
    ranges: &[AnnotatedRange],
    repo_root: &Path,
) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut out: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
    for r in ranges {
        if out.contains_key(&r.file_path) {
            continue;
        }
        let abs = repo_root.join(&r.file_path);
        let bytes = std::fs::read(&abs)?;
        out.insert(r.file_path.clone(), bytes);
    }
    Ok(out)
}

fn spdx_file_name(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if s.starts_with("./") || s.starts_with('/') {
        s
    } else {
        format!("./{s}")
    }
}

fn display_path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
