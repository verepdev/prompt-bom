//! Tests for `services::spdx_emit::emit_spdx`.
//!
//! Two layers:
//! 1. Structural assertions on the emitted `SpdxDocument`: file count, snippet
//!    count, ID stability, annotation round-trip via `AiProvenance`.
//! 2. A byte-stable JSON snapshot under `tests/fixtures/expected_emit.json`.
//!    The snapshot is the regression alarm — it fires if the document shape
//!    drifts unintentionally. Re-bless it with `BLESS=1 cargo test`.

use prompt_bom::schemas::attribution::{AnnotatedRange, BlameInfo};
use prompt_bom::schemas::spdx::{AiProvenance, SpdxDocument};
use prompt_bom::services::spdx_emit::{emit_spdx, EmitOptions};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn sample_ranges() -> Vec<AnnotatedRange> {
    vec![
        AnnotatedRange {
            file_path: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 1,
            content_hash: "abc".into(),
            attribution_uuid: "u-1".into(),
            model: "claude-sonnet-4-6".into(),
            session_id: "sess-1".into(),
            timestamp: "2026-04-28T09:00:01Z".into(),
            blame: None,
        },
        AnnotatedRange {
            file_path: PathBuf::from("src/lib.rs"),
            start_line: 5,
            end_line: 7,
            content_hash: "def".into(),
            attribution_uuid: "u-2".into(),
            model: "claude-sonnet-4-6".into(),
            session_id: "sess-1".into(),
            timestamp: "2026-04-28T09:00:02Z".into(),
            blame: Some(BlameInfo {
                first_commit_oid: "0000000000000000000000000000000000000001".into(),
                last_commit_oid: "0000000000000000000000000000000000000002".into(),
                author_emails: vec!["dev@example.com".into()],
            }),
        },
        AnnotatedRange {
            file_path: PathBuf::from("src/main.rs"),
            start_line: 1,
            end_line: 1,
            content_hash: "ghi".into(),
            attribution_uuid: "u-3".into(),
            model: "claude-sonnet-4-6".into(),
            session_id: "sess-1".into(),
            timestamp: "2026-04-28T09:00:03Z".into(),
            blame: None,
        },
    ]
}

fn sample_file_contents() -> BTreeMap<PathBuf, Vec<u8>> {
    let mut m = BTreeMap::new();
    m.insert(
        PathBuf::from("src/lib.rs"),
        b"pub fn one() {}\n\n\n\nfn two() {\n    1\n}\n".to_vec(),
    );
    m.insert(PathBuf::from("src/main.rs"), b"fn main() {}\n".to_vec());
    m
}

fn sample_opts() -> EmitOptions {
    EmitOptions {
        project_name: "demo".into(),
        document_namespace: "https://verepdev.github.io/demo/0123456789abcdef".into(),
        created_iso: "2026-04-28T09:00:00Z".into(),
        tool_id: "Tool: prompt-bom-test".into(),
    }
}

fn build_doc() -> SpdxDocument {
    emit_spdx(&sample_ranges(), &sample_file_contents(), &sample_opts()).expect("emit_spdx")
}

#[test]
fn document_top_level_fields() {
    let doc = build_doc();
    assert_eq!(doc.spdx_version, "SPDX-2.3");
    assert_eq!(doc.data_license, "CC0-1.0");
    assert_eq!(doc.spdxid, "SPDXRef-DOCUMENT");
    assert_eq!(doc.name, "demo");
    assert_eq!(doc.creation_info.created, "2026-04-28T09:00:00Z");
    assert_eq!(doc.creation_info.creators, vec!["Tool: prompt-bom-test"]);
}

#[test]
fn one_file_per_unique_path_sorted() {
    let doc = build_doc();
    assert_eq!(doc.files.len(), 2);
    assert_eq!(doc.files[0].file_name, "./src/lib.rs");
    assert_eq!(doc.files[0].spdxid, "SPDXRef-File-1");
    assert_eq!(doc.files[1].file_name, "./src/main.rs");
    assert_eq!(doc.files[1].spdxid, "SPDXRef-File-2");
}

#[test]
fn checksums_use_blake3() {
    let doc = build_doc();
    for file in &doc.files {
        assert_eq!(file.checksums.len(), 1);
        assert_eq!(file.checksums[0].algorithm, "BLAKE3");
        assert_eq!(file.checksums[0].checksum_value.len(), 64);
    }
}

#[test]
fn one_snippet_per_range_in_input_order() {
    let doc = build_doc();
    assert_eq!(doc.snippets.len(), 3);
    assert_eq!(doc.snippets[0].spdxid, "SPDXRef-Snippet-1");
    assert_eq!(doc.snippets[0].snippet_from_file, "SPDXRef-File-1");
    assert_eq!(doc.snippets[1].snippet_from_file, "SPDXRef-File-1");
    assert_eq!(doc.snippets[2].snippet_from_file, "SPDXRef-File-2");
}

#[test]
fn snippet_range_carries_line_numbers() {
    let doc = build_doc();
    let r = &doc.snippets[1].ranges[0];
    assert_eq!(r.start_pointer.line_number, 5);
    assert_eq!(r.end_pointer.line_number, 7);
    assert_eq!(r.start_pointer.reference, "SPDXRef-File-1");
}

#[test]
fn annotation_comment_round_trips_to_ai_provenance() {
    let doc = build_doc();
    let comment = &doc.snippets[0].annotations[0].comment;
    let prov: AiProvenance = serde_json::from_str(comment).expect("parse AiProvenance");
    assert_eq!(prov.attribution_uuid, "u-1");
    assert_eq!(prov.model, "claude-sonnet-4-6");
    assert!(prov.blame.is_none());

    let with_blame: AiProvenance =
        serde_json::from_str(&doc.snippets[1].annotations[0].comment).unwrap();
    let blame = with_blame.blame.expect("expected blame on snippet 2");
    assert_eq!(blame.author_emails, vec!["dev@example.com"]);
}

#[test]
fn relationships_describe_package_and_contain_files() {
    let doc = build_doc();
    assert_eq!(doc.relationships.len(), 3);
    assert_eq!(doc.relationships[0].relationship_type, "DESCRIBES");
    assert_eq!(
        doc.relationships[0].related_spdx_element,
        "SPDXRef-Package-Project"
    );
    assert!(doc.relationships.iter().skip(1).all(
        |r| r.relationship_type == "CONTAINS" && r.spdx_element_id == "SPDXRef-Package-Project"
    ));
}

#[test]
fn missing_file_content_is_an_error() {
    let ranges = sample_ranges();
    let mut contents = sample_file_contents();
    contents.remove(&PathBuf::from("src/lib.rs"));
    let err = emit_spdx(&ranges, &contents, &sample_opts()).unwrap_err();
    assert!(format!("{err}").contains("src/lib.rs"));
}

/// Snapshot test — guards the wire format against unintentional drift.
///
/// The fixture is written and compared with a single trailing newline so the
/// `end-of-file-fixer` pre-commit hook is a no-op against it.
///
/// Re-bless with `BLESS=1 cargo test snapshot_emit_matches_fixture`.
#[test]
fn snapshot_emit_matches_fixture() {
    let doc = build_doc();
    let actual = format!("{}\n", serde_json::to_string_pretty(&doc).unwrap());
    let fixture = std::path::Path::new("tests/fixtures/expected_emit.json");

    if std::env::var("BLESS").is_ok() {
        std::fs::write(fixture, &actual).expect("bless write");
        return;
    }

    let expected = std::fs::read_to_string(fixture).unwrap_or_else(|_| {
        panic!(
            "snapshot fixture missing at {}. Run BLESS=1 cargo test snapshot_emit_matches_fixture",
            fixture.display()
        )
    });
    if actual != expected {
        panic!(
            "snapshot drift. Re-bless with BLESS=1 cargo test snapshot_emit_matches_fixture if the change is intentional.\n--- expected ---\n{expected}\n--- actual ---\n{actual}"
        );
    }
}
