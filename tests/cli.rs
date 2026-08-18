use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
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
        .stderr(contains("cannot provide PDFs"))
        .stderr(contains("pmc"));
}

// Reported from a downstream agent: it asked semantic for --author, was told
// only which filters semantic does have, and had no way to learn that eleven
// other sources take the flag. The rejection is correct; leaving the caller
// stranded is not.
#[test]
fn search_semantic_author_redirects_to_sources_that_have_it() {
    cmd()
        .args([
            "search",
            "semantic",
            "Hairong Zheng",
            "--author",
            "Hairong Zheng",
            "--year",
            "2026",
        ])
        .assert()
        .failure()
        .stderr(contains("--author"))
        .stderr(contains("works on"))
        .stderr(contains("crossref"));
}

#[test]
fn search_pubmed_no_capability_error() {
    // pubmed supports search, so it should NOT fail with "does not support"
    // It will fail with "not yet implemented" but that's fine
    let output = cmd().args(["search", "pubmed", "test"]).output().unwrap();
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
        .stderr(contains("Unrecognized identifier"));
}

#[test]
fn get_arxiv_id_routes_to_arxiv() {
    let output = cmd().args(["get", "2301.08745"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Unrecognized"),
        "should recognize arXiv ID"
    );
}

#[test]
fn get_pmc_id_routes_to_pmc() {
    let output = cmd().args(["get", "PMC7318926"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("Unrecognized"), "should recognize PMC ID");
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
        .stderr(contains("cannot provide PDFs"));
}

// ── read --metadata-only integration tests ──────

// ── read full text (local PDF) integration tests ──

#[test]
fn read_local_pdf_outputs_text() {
    let output = cmd()
        .args(["read", "tests/fixtures/test.pdf"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.trim().is_empty(), "should output text content");
}

#[test]
fn read_local_pdf_section_abstract() {
    let output = cmd()
        .args(["read", "tests/fixtures/test.pdf", "--section", "abstract"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.trim().is_empty(),
        "abstract section should not be empty"
    );
    let lower = stdout.to_lowercase();
    assert!(
        !lower.contains("introduction"),
        "abstract should not contain introduction"
    );
}

#[test]
fn read_local_pdf_format_json_has_full_text() {
    let output = cmd()
        .args(["read", "tests/fixtures/test.pdf", "--format", "json"])
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
        .args(["read", "tests/fixtures/test.pdf", "--max-length", "100"])
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
        .args(["read", "tests/fixtures/test.pdf", "-o"])
        .arg(out_file.to_str().unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out_file.exists(), "output file should exist");
    let content = std::fs::read_to_string(&out_file).unwrap();
    assert!(
        !content.trim().is_empty(),
        "output file should not be empty"
    );
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
        .args(["get", "unpaywall", "10.1038/nature12373"])
        .env_remove("UNPAYWALL_EMAIL")
        .assert()
        .failure()
        .stderr(contains("UNPAYWALL_EMAIL"));
}

/// The SCREAMING_SNAKE tokens in `text`, which is what an env var looks like.
/// The underscore requirement keeps acronyms like DOI and PDF out.
fn env_var_tokens(text: &str) -> Vec<String> {
    let mut names: Vec<String> = text
        .split(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
        .filter(|t| t.len() >= 4 && t.contains('_'))
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Guards the drift that shipped in 0.3.0: the capability note named a variable
/// the source never reads, so following it did not fix the error.
#[test]
fn capabilities_names_the_env_var_the_unpaywall_error_demands() {
    let failure = cmd()
        .args(["get", "unpaywall", "10.1038/nature12373"])
        .env_remove("UNPAYWALL_EMAIL")
        .output()
        .unwrap();
    let demanded = env_var_tokens(&String::from_utf8_lossy(&failure.stderr));
    assert!(
        !demanded.is_empty(),
        "the error should name the variable it wants: {}",
        String::from_utf8_lossy(&failure.stderr)
    );

    let listing = cmd().args(["sources", "--capabilities"]).output().unwrap();
    let listing = String::from_utf8_lossy(&listing.stdout);
    for var in demanded {
        assert!(
            listing.contains(&var),
            "`get unpaywall` demands {}, but `sources --capabilities` never names it",
            var
        );
    }
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
        .args([
            "get",
            "unpaywall",
            "10.1038/nature12373",
            "--format",
            "json",
        ])
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

// ── search command integration tests ────────────

#[test]
fn search_arxiv_query_with_spaces_works() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["search", "arxiv", "attention mechanism"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "query with spaces should work, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.trim().is_empty());
}

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
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    assert!(
        v["results"].as_array().is_some(),
        "should have results array"
    );
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
    assert!(
        first_line.contains("id") && first_line.contains("title"),
        "first line should be header"
    );
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
    assert!(
        stdout.contains("@article"),
        "bibtex should contain @article"
    );
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
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let results = v["results"].as_array().expect("results array");
    assert!(!results.is_empty());
    let title = results[0]["title"].as_str().unwrap_or("");
    assert!(!title.is_empty(), "title should not be empty");
}

// ── get command integration tests ───────────────

#[test]
fn get_arxiv_id_returns_paper() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["get", "2301.08745", "--format", "json"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(!v["results"][0]["title"].as_str().unwrap_or("").is_empty());
}

#[test]
fn get_pmc_id_returns_paper() {
    let efetch = include_str!("fixtures/pmc_efetch.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(efetch)
        .create();
    let output = cmd()
        .args(["get", "PMC7318926", "--format", "json"])
        .env("FASTPAPER_PMC_URL", server.url())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(!v["results"][0]["title"].as_str().unwrap_or("").is_empty());
}

#[test]
fn get_pmid_returns_paper() {
    let efetch = include_str!("fixtures/pubmed_efetch.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(efetch)
        .create();
    let output = cmd()
        .args(["get", "PMID:33475315", "--format", "json"])
        .env("FASTPAPER_PUBMED_URL", server.url())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(!v["results"][0]["title"].as_str().unwrap_or("").is_empty());
}

#[test]
fn get_s2_id_returns_paper() {
    // S2 get returns a single paper object, extract first from search fixture
    let fixture = include_str!("fixtures/semantic_search.json");
    let v: serde_json::Value = serde_json::from_str(fixture).unwrap();
    let single_paper = serde_json::to_string(&v["data"][0]).unwrap();
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(single_paper)
        .create();
    let output = cmd()
        .args([
            "get",
            "S2:649def34f8be52c8b66281af98ae884c09aef38b",
            "--format",
            "json",
        ])
        .env("FASTPAPER_SEMANTIC_URL", server.url())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(!v["results"][0]["title"].as_str().unwrap_or("").is_empty());
}

// ── read remote full text integration tests ─────

// ── xueshu integration tests ────────────────────

#[test]
fn search_xueshu_mock_outputs_title() {
    let fixture = include_str!("fixtures/xueshu_search.json");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["search", "xueshu", "attention mechanism", "-n", "3"])
        .env("FASTPAPER_XUESHU_URL", server.url())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Visual Attention Mechanisms"),
        "got: {stdout}"
    );
}

#[test]
fn search_xueshu_captcha_exits_nonzero_with_message() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(206)
        .with_body(r#"{"status":{"code":7350001,"msg":"请输入验证码"}}"#)
        .create();
    cmd()
        .args(["search", "xueshu", "test"])
        .env("FASTPAPER_XUESHU_URL", server.url())
        .assert()
        .failure()
        .stderr(contains("captcha"));
}

#[test]
fn sources_lists_xueshu() {
    cmd()
        .arg("sources")
        .assert()
        .success()
        .stdout(contains("xueshu"));
}

#[test]
fn download_xueshu_rejects_with_hint() {
    cmd()
        .args(["download", "xueshu", "5a4afe9b6df8b07138fb6bf9b275251f"])
        .assert()
        .failure()
        .stderr(contains("cannot provide PDFs"));
}

// 真实接口健康检查,手动执行:cargo test --test cli -- --ignored
// 若开始返回 7350001/403,按调研文档第 14.8 条重新审查官网调用链。
// 注意(2026-07-20 实测):短时间内重复相同关键词会触发 7350001 验证码;
// 偶发失败时换个关键词重试一次即可区分"查询级风控"和"接口失效"。
#[test]
#[ignore]
fn real_xueshu_search_works() {
    let output = cmd()
        .args([
            "search",
            "xueshu",
            "convolutional neural network",
            "-n",
            "3",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON output");
    assert!(
        !out["results"].as_array().expect("results array").is_empty(),
        "results empty"
    );
}

// 真实接口健康检查(手动:cargo test --test cli -- --ignored)。
// 这三个源的 search URL 曾缺 /v3 或 /api 前缀,对真实 API 返回 404/403 HTML;
// mockito 用宽松路径匹配未能发现,这些冒烟测试是防回归的最后一道关。
fn real_search_returns_results(source: &str) {
    let output = cmd()
        .args([
            "search",
            source,
            "machine learning",
            "-n",
            "2",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{source} search failed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let out: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON output");
    assert!(
        !out["results"].as_array().expect("results array").is_empty(),
        "{source} returned no results"
    );
}

#[test]
#[ignore]
fn real_core_search_works() {
    real_search_returns_results("core");
}

#[test]
#[ignore]
fn real_doaj_search_works() {
    real_search_returns_results("doaj");
}

#[test]
#[ignore]
fn real_zenodo_search_works() {
    real_search_returns_results("zenodo");
}

// ── new command surface ─────────────────────────
//
// `get` and `download` accept both `<id>` (source auto-detected) and
// `<source> <id>` (source forced), mirroring `search <source> <query>`.

#[test]
fn get_accepts_an_explicit_source_before_the_id() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["get", "arxiv", "2301.08745", "--format", "json"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("valid JSON");
    assert!(!v["results"][0]["title"].as_str().unwrap_or("").is_empty());
}

#[test]
fn get_with_an_unknown_source_lists_the_valid_ones() {
    cmd()
        .args(["get", "arxvi", "2301.08745"])
        .assert()
        .failure()
        .stderr(contains("not a known source"))
        .stderr(contains("arxiv"));
}

#[test]
fn get_rejects_a_source_that_cannot_resolve_identifiers() {
    cmd()
        .args(["get", "dblp", "10.1038/nature12373"])
        .assert()
        .failure()
        .stderr(contains("cannot fetch a paper by identifier"));
}

#[test]
fn download_auto_detects_the_source_from_the_id() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(b"%PDF-1.4 auto".as_slice())
        .create();
    let dir = temp_dir();
    cmd()
        .args(["download", "2301.08745", "--dir"])
        .arg(dir.to_str().unwrap())
        .env("FASTPAPER_ARXIV_URL", server.url())
        .assert()
        .success();
    assert_eq!(
        std::fs::read(dir.join("2301.08745.pdf")).unwrap(),
        b"%PDF-1.4 auto"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_defaults_to_a_papers_directory() {
    let output = cmd().args(["download", "--help"]).output().unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("./papers"));
}

#[test]
fn download_of_a_pmid_points_at_pmc() {
    cmd()
        .args(["download", "33475315"])
        .assert()
        .failure()
        .stderr(contains("download pmc"));
}

// `read` is local-only now: no source argument, no network.

#[test]
fn read_takes_a_path_with_no_source_argument() {
    cmd()
        .args(["read", "tests/fixtures/test.pdf"])
        .assert()
        .success();
}

#[test]
fn read_rejects_a_missing_file_with_advice() {
    cmd()
        .args(["read", "definitely/not/here.pdf"])
        .assert()
        .failure()
        .stderr(contains("No such file"))
        .stderr(contains("fastpaper download"));
}

#[test]
fn read_no_longer_accepts_a_source_argument() {
    // Two positionals used to mean `read <source> <id>`.
    cmd()
        .args(["read", "arxiv", "2301.08745"])
        .assert()
        .failure();
}

// Filters are validated against what the source can actually do.

#[test]
fn search_rejects_a_filter_the_source_cannot_honour() {
    cmd()
        .args(["search", "dblp", "crispr", "--year", "2024"])
        .assert()
        .failure()
        .stderr(contains("--year"))
        .stderr(contains("dblp"));
}

#[test]
fn search_rejects_a_limit_above_the_source_cap() {
    cmd()
        .args(["search", "zenodo", "crispr", "-n", "50"])
        .assert()
        .failure()
        .stderr(contains("25"));
}

#[test]
fn search_on_unpaywall_points_at_get() {
    cmd()
        .args(["search", "unpaywall", "10.1038/nature12373"])
        .assert()
        .failure()
        .stderr(contains("fastpaper get unpaywall"));
}

#[test]
fn sources_capabilities_shows_filters_and_caveats() {
    cmd()
        .args(["sources", "--capabilities"])
        .assert()
        .success()
        .stdout(contains("Search filters"))
        .stdout(contains("no keyword search API"));
}

#[test]
fn format_jsonl_emits_one_object_per_line() {
    let fixture = include_str!("fixtures/arxiv_search.xml");
    let mut server = mockito::Server::new();
    server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_body(fixture)
        .create();
    let output = cmd()
        .args(["search", "arxiv", "attention", "--format", "jsonl"])
        .env("FASTPAPER_ARXIV_URL", server.url())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.trim().is_empty(), "jsonl should not be empty");
    for line in stdout.lines() {
        serde_json::from_str::<serde_json::Value>(line)
            .unwrap_or_else(|e| panic!("line is not JSON ({}): {}", e, line));
    }
}

#[test]
fn search_with_a_blank_query_is_rejected() {
    cmd()
        .args(["search", "hal", "", "--author", "Aubry"])
        .assert()
        .failure()
        .stderr(contains("empty"));
}

fn fixture_pdf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.pdf")
}

#[test]
fn output_flag_reports_what_it_wrote_on_stderr() {
    let out = temp_dir().join("receipt.txt");
    cmd()
        .arg("read")
        .arg(fixture_pdf())
        .arg("-o")
        .arg(&out)
        .assert()
        .success()
        .stdout("")
        .stderr(contains("Saved:").and(contains("chars")));
    assert!(out.exists(), "the file should still be written");
}

#[test]
fn quiet_suppresses_the_receipt() {
    let out = temp_dir().join("quiet.txt");
    cmd()
        .arg("read")
        .arg(fixture_pdf())
        .arg("-o")
        .arg(&out)
        .arg("-q")
        .assert()
        .success()
        .stdout("")
        .stderr("");
    assert!(out.exists(), "-q silences the receipt, not the write");
}

#[test]
fn without_output_flag_there_is_no_receipt() {
    cmd()
        .arg("read")
        .arg(fixture_pdf())
        .assert()
        .success()
        .stderr("");
}
