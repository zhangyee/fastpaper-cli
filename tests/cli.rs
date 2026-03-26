use assert_cmd::Command;
use predicates::str::contains;

fn cmd() -> Command {
    Command::cargo_bin("fastpaper").unwrap()
}

#[test]
fn help_exits_0_and_contains_search() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("search"));
}

#[test]
fn version_exits_0() {
    cmd().arg("--version").assert().success();
}

#[test]
fn sources_exits_0_and_contains_arxiv() {
    cmd()
        .arg("sources")
        .assert()
        .success()
        .stdout(contains("arxiv"));
}

#[test]
fn search_no_args_exits_nonzero() {
    cmd().arg("search").assert().failure();
}

#[test]
fn search_invalid_source_exits_nonzero() {
    cmd()
        .args(["search", "nonexistent", "test"])
        .assert()
        .failure()
        .stderr(contains("invalid"));
}

#[test]
fn download_pubmed_rejects_with_hint() {
    cmd()
        .args(["download", "pubmed", "PMID:123"])
        .assert()
        .failure()
        .stderr(contains("does not support"))
        .stderr(contains("pmc"));
}

#[test]
fn search_pubmed_no_capability_error() {
    // pubmed supports search, so it should NOT fail with "does not support"
    // It will fail with "not yet implemented" but that's fine
    let output = cmd()
        .args(["search", "pubmed", "test"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("does not support"));
}

#[test]
fn download_arxiv_no_capability_error() {
    // arxiv supports download, so it should NOT fail with "does not support"
    let output = cmd()
        .args(["download", "arxiv", "2301.08745"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("does not support"));
}
