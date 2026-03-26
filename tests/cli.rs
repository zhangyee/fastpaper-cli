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

#[test]
fn get_unknown_identifier_fails() {
    cmd()
        .args(["get", "blahblah"])
        .assert()
        .failure()
        .stderr(contains("Unrecognized identifier format"));
}

#[test]
fn get_arxiv_id_routes_to_arxiv() {
    // arXiv get not implemented yet, but should recognize the ID
    let output = cmd()
        .args(["get", "2301.08745"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("arXiv"), "should route to arXiv, got: {}", stderr);
    assert!(!stderr.contains("Unrecognized"));
}

#[test]
fn get_pmc_id_routes_to_pmc() {
    let output = cmd()
        .args(["get", "PMC7318926"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("pmc"), "should route to pmc, got: {}", stderr);
    assert!(!stderr.contains("Unrecognized"));
}

#[test]
fn get_format_json_flag_accepted() {
    // --format json should be accepted as a valid flag (even if source not implemented)
    let output = cmd()
        .args(["get", "2301.08745", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should route to arXiv, not complain about invalid format
    assert!(!stderr.contains("Unrecognized"));
    assert!(!stderr.contains("invalid"));
}
