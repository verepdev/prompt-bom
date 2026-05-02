use assert_cmd::Command;
use git2::{Repository, Signature};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn binary_runs_and_prints_version() {
    Command::cargo_bin("prompt-bom")
        .unwrap()
        .arg("--version")
        .assert()
        .success();
}

#[test]
fn emit_pipeline_runs_end_to_end() {
    let dir = tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    {
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test User").unwrap();
        cfg.set_str("user.email", "test@example.com").unwrap();
    }

    let body = "pub fn hello() -> &'static str { \"hello\" }\n";
    let demo_path = dir.path().join("demo.rs");
    fs::write(&demo_path, body).unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("demo.rs")).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let sig = Signature::now("Test User", "test@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();

    let transcript_path = dir.path().join("session.jsonl");
    let line = serde_json::json!({
        "type": "assistant",
        "sessionId": "s",
        "uuid": "u-1",
        "timestamp": "2026-04-28T09:00:00Z",
        "message": {
            "model": "claude-sonnet-4-6",
            "content": [{
                "type": "tool_use",
                "id": "t1",
                "name": "Write",
                "input": { "file_path": "demo.rs", "content": body }
            }]
        }
    });
    fs::write(&transcript_path, serde_json::to_string(&line).unwrap()).unwrap();

    let out_path = dir.path().join("out.spdx.json");

    Command::cargo_bin("prompt-bom")
        .unwrap()
        .args([
            "emit",
            "--transcript",
            transcript_path.to_str().unwrap(),
            "--repo",
            dir.path().to_str().unwrap(),
            "--out",
            out_path.to_str().unwrap(),
            "--name",
            "demo",
            "--created",
            "2026-04-28T09:00:00Z",
        ])
        .assert()
        .success();

    let written = fs::read_to_string(&out_path).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(doc["spdxVersion"], "SPDX-2.3");
    assert_eq!(doc["name"], "demo");
    let snippets = doc["snippets"].as_array().expect("snippets array");
    assert!(
        !snippets.is_empty(),
        "expected at least one snippet from the AI-attributed write"
    );
    let files = doc["files"].as_array().expect("files array");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["fileName"], "./demo.rs");
}
