use assert_cmd::Command;

#[test]
fn binary_runs_and_prints_version() {
    Command::cargo_bin("prompt-bom")
        .unwrap()
        .arg("--version")
        .assert()
        .success();
}
