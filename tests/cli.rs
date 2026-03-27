use assert_cmd::Command;
use predicates::str::contains;
use std::path::PathBuf;

fn cmd() -> Command {
    Command::cargo_bin("fastpaper").unwrap()
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fastpaper_cli_test_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
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

// ── download integration tests ──────────────────

#[test]
fn download_arxiv_saves_pdf_to_dir() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(b"%PDF-1.4 fake".as_slice())
        .create();
    let dir = temp_dir();
    cmd()
        .args(["download", "arxiv", "2301.08745", "--dir"])
        .arg(dir.to_str().unwrap())
        .env("FASTPAPER_ARXIV_URL", server.url())
        .assert()
        .success();
    assert!(dir.join("2301.08745.pdf").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_arxiv_file_exists_exits_0_with_stderr() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(b"%PDF-1.4 fake".as_slice())
        .create();
    let dir = temp_dir();
    // Create existing file
    std::fs::write(dir.join("2301.08745.pdf"), b"old").unwrap();
    cmd()
        .args(["download", "arxiv", "2301.08745", "--dir"])
        .arg(dir.to_str().unwrap())
        .env("FASTPAPER_ARXIV_URL", server.url())
        .assert()
        .success()
        .stderr(contains("already exists"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_arxiv_overwrite_replaces_file() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(b"%PDF-1.4 new content".as_slice())
        .create();
    let dir = temp_dir();
    std::fs::write(dir.join("2301.08745.pdf"), b"old").unwrap();
    cmd()
        .args(["download", "arxiv", "2301.08745", "--dir"])
        .arg(dir.to_str().unwrap())
        .arg("--overwrite")
        .env("FASTPAPER_ARXIV_URL", server.url())
        .assert()
        .success();
    assert_eq!(
        std::fs::read(dir.join("2301.08745.pdf")).unwrap(),
        b"%PDF-1.4 new content"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_arxiv_404_exits_nonzero() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(404)
        .create();
    let dir = temp_dir();
    cmd()
        .args(["download", "arxiv", "9999.99999", "--dir"])
        .arg(dir.to_str().unwrap())
        .env("FASTPAPER_ARXIV_URL", server.url())
        .assert()
        .failure();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_pubmed_exits_nonzero_with_not_supported() {
    cmd()
        .args(["download", "pubmed", "12345678"])
        .assert()
        .failure()
        .stderr(contains("does not support"));
}

// ── read --metadata-only integration tests ──────

#[test]
fn read_arxiv_metadata_only_json_contains_title() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["read", "arxiv", "2301.08745", "--metadata-only", "--format", "json"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    let title = v["results"][0]["title"].as_str().unwrap_or("");
    assert!(!title.is_empty(), "title should not be empty");
}

#[test]
fn read_arxiv_metadata_only_json_contains_authors() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["read", "arxiv", "2301.08745", "--metadata-only", "--format", "json"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    let authors = v["results"][0]["authors"].as_array().expect("authors should be array");
    assert!(!authors.is_empty(), "authors should not be empty");
}

#[test]
fn read_arxiv_metadata_only_json_no_full_text() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["read", "arxiv", "2301.08745", "--metadata-only", "--format", "json"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("full_text"), "metadata-only should not contain full_text");
}

#[test]
fn read_pubmed_metadata_only_exits_nonzero() {
    cmd()
        .args(["read", "pubmed", "12345678", "--metadata-only"])
        .assert()
        .failure()
        .stderr(contains("does not support"));
}
