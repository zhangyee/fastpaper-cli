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

// ── read full text (local PDF) integration tests ──

#[test]
fn read_local_pdf_outputs_text() {
    let output = cmd()
        .args(["read", "local", "tests/fixtures/test.pdf"])
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.trim().is_empty(), "should output text content");
}

#[test]
fn read_local_pdf_section_abstract() {
    let output = cmd()
        .args(["read", "local", "tests/fixtures/test.pdf", "--section", "abstract"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.trim().is_empty(), "abstract section should not be empty");
    let lower = stdout.to_lowercase();
    assert!(
        !lower.contains("introduction"),
        "abstract should not contain introduction"
    );
}

#[test]
fn read_local_pdf_format_json_has_full_text() {
    let output = cmd()
        .args(["read", "local", "tests/fixtures/test.pdf", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    let full_text = v["content"]["full_text"].as_str().unwrap_or("");
    assert!(!full_text.is_empty(), "full_text should not be empty");
}

#[test]
fn read_local_pdf_max_length_truncates() {
    let output = cmd()
        .args(["read", "local", "tests/fixtures/test.pdf", "--max-length", "100"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().len() <= 100,
        "output should be at most 100 chars, got {}",
        stdout.trim().len()
    );
}

#[test]
fn read_local_pdf_output_to_file() {
    let dir = temp_dir();
    let out_file = dir.join("output.txt");
    let output = cmd()
        .args(["read", "local", "tests/fixtures/test.pdf", "-o"])
        .arg(out_file.to_str().unwrap())
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(out_file.exists(), "output file should exist");
    let content = std::fs::read_to_string(&out_file).unwrap();
    assert!(!content.trim().is_empty(), "output file should not be empty");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── env var integration tests ───────────────────

#[test]
fn env_download_dir_overrides_default() {
    let fake_pdf = b"%PDF-1.4 fake";
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fake_pdf.as_slice())
        .create();
    let dir = temp_dir();
    cmd()
        .args(["download", "arxiv", "2301.08745"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .env("FASTPAPER_DOWNLOAD_DIR", dir.to_str().unwrap())
        .assert()
        .success();
    assert!(
        dir.join("2301.08745.pdf").exists(),
        "should save to FASTPAPER_DOWNLOAD_DIR"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn env_semantic_api_key_sends_header() {
    let fixture = include_str!("fixtures/semantic_search.json");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .match_header("x-api-key", "test-key-abc")
        .with_status(200)
        .with_body(fixture)
        .create();
    cmd()
        .args(["search", "semantic", "attention", "--format", "json"])
        .env("FASTPAPER_SEMANTIC_URL", server.url())
        .env("SEMANTIC_SCHOLAR_API_KEY", "test-key-abc")
        .assert()
        .success();
}

#[test]
fn env_unpaywall_missing_email_exits_nonzero() {
    cmd()
        .args(["search", "unpaywall", "10.1038/nature12373"])
        .env_remove("UNPAYWALL_EMAIL")
        .assert()
        .failure()
        .stderr(contains("UNPAYWALL_EMAIL"));
}

#[test]
fn env_unpaywall_with_email_works() {
    let fixture = include_str!("fixtures/unpaywall_lookup.json");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    cmd()
        .args(["search", "unpaywall", "10.1038/nature12373", "--format", "json"])
        .env("FASTPAPER_UNPAYWALL_URL", server.url())
        .env("UNPAYWALL_EMAIL", "test@test.com")
        .assert()
        .success();
}

// ── completions integration tests ───────────────

#[test]
fn completions_zsh_exits_0_contains_compdef() {
    cmd()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(contains("compdef"));
}

#[test]
fn completions_bash_exits_0_contains_complete() {
    cmd()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(contains("complete"));
}

#[test]
fn completions_fish_exits_0_contains_fastpaper() {
    cmd()
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(contains("fastpaper"));
}

// ── skill integration tests ─────────────────────

#[test]
fn skill_show_exits_0_contains_search() {
    cmd()
        .args(["skill", "show"])
        .assert()
        .success()
        .stdout(contains("fastpaper search"));
}

#[test]
fn skill_export_has_yaml_frontmatter() {
    let output = cmd()
        .args(["skill", "export"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("---"), "should start with YAML frontmatter");
    assert!(stdout.matches("---").count() >= 2, "should have opening and closing ---");
}

#[test]
fn skill_export_agent_claude_contains_path() {
    cmd()
        .args(["skill", "export", "--agent", "claude"])
        .assert()
        .success()
        .stdout(contains(".claude/skills"));
}

// ── search command integration tests ────────────

#[test]
fn search_arxiv_mock_outputs_title() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["search", "arxiv", "attention"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.trim().is_empty(), "should output something");
}

#[test]
fn search_arxiv_format_json_valid() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["search", "arxiv", "attention", "--format", "json"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    assert!(v["results"].as_array().is_some(), "should have results array");
    assert!(!v["results"].as_array().unwrap().is_empty());
}

#[test]
fn search_arxiv_format_csv_has_header() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["search", "arxiv", "attention", "--format", "csv"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("");
    assert!(first_line.contains("id") && first_line.contains("title"), "first line should be header");
}

#[test]
fn search_arxiv_format_bibtex_has_article() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["search", "arxiv", "attention", "--format", "bibtex"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("@article"), "bibtex should contain @article");
}

#[test]
fn search_arxiv_limit_passes_to_request() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Regex("max_results=5".to_string()))
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["search", "arxiv", "attention", "-n", "5"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn search_pubmed_mock_outputs_title() {
    let esearch = include_str!("fixtures/pubmed_esearch.json");
    let efetch = include_str!("fixtures/pubmed_efetch.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
        .with_status(200)
        .with_body(esearch)
        .create();
    server
        .mock("GET", mockito::Matcher::Regex("efetch".to_string()))
        .with_status(200)
        .with_body(efetch)
        .create();
    let output = cmd()
        .args(["search", "pubmed", "test", "--format", "json"])
        .env("FASTPAPER_PUBMED_URL", server.url())
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let results = v["results"].as_array().expect("results array");
    assert!(!results.is_empty());
    let title = results[0]["title"].as_str().unwrap_or("");
    assert!(!title.is_empty(), "title should not be empty");
}

#[test]
fn search_crossref_mock_outputs_title() {
    let fixture = include_str!("fixtures/crossref_search.json");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["search", "crossref", "test", "--format", "json"])
        .env("FASTPAPER_CROSSREF_URL", server.url())
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let results = v["results"].as_array().expect("results array");
    assert!(!results.is_empty());
    let title = results[0]["title"].as_str().unwrap_or("");
    assert!(!title.is_empty(), "title should not be empty");
}
