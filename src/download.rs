use std::path::{Path, PathBuf};

use crate::sources;

/// Download PDF bytes from a URL.
pub fn fetch_pdf(url: &str) -> Result<Vec<u8>, String> {
    match ureq::get(url).call() {
        Ok(resp) => resp
            .into_body()
            .read_to_vec()
            .map_err(|e| format!("Failed to read PDF: {}", e)),
        Err(ureq::Error::StatusCode(404)) => Err(format!("Not found: {}", url)),
        Err(e) => Err(format!("HTTP error: {}", e)),
    }
}

// ── PDF byte fetchers ───────────────────────────
//
// Resolving "identifier -> PDF bytes" is separate from writing the file, so the
// same resolution serves both `download` (save to disk) and any caller that
// only wants the bytes. `save_pdf` below does the writing.

/// Fetch arXiv PDF bytes.
pub fn pdf_bytes_arxiv(base_url: &str, identifier: &str) -> Result<Vec<u8>, String> {
    sources::arxiv::download_pdf(base_url, identifier)
}

/// Fetch bioRxiv PDF bytes.
pub fn pdf_bytes_biorxiv(base_url: &str, identifier: &str) -> Result<Vec<u8>, String> {
    fetch_pdf(&format!("{}/content/{}v1.full.pdf", base_url, identifier))
}

/// Fetch medRxiv PDF bytes.
pub fn pdf_bytes_medrxiv(base_url: &str, identifier: &str) -> Result<Vec<u8>, String> {
    fetch_pdf(&format!("{}/content/{}v1.full.pdf", base_url, identifier))
}

/// Fetch PMC PDF bytes from the PMC Cloud Service.
///
/// The article page's `/pdf/` URL is not usable programmatically: it answers
/// every client -- browser user agents included -- with a small
/// "Preparing to download" HTML interstitial. The OA Web Service does return a
/// PDF link, but as an `ftp://` URL, and NCBI has retired anonymous FTP; the
/// files it points at were moved under `/deprecated/` in April 2026 and are due
/// to be deleted in August 2026.
///
/// The Cloud Service (AWS Open Data) serves the same files over plain HTTPS and
/// is the route that survives. Objects are keyed by article and version, so the
/// version has to be discovered by listing the prefix first.
pub fn pdf_bytes_pmc(base_url: &str, identifier: &str) -> Result<Vec<u8>, String> {
    let numeric_id = identifier.strip_prefix("PMC").unwrap_or(identifier);
    let listing = http_get_text(&format!(
        "{}/?list-type=2&prefix=PMC{}",
        base_url, numeric_id
    ))?;
    let key = first_pdf_key(&listing).ok_or_else(|| {
        format!(
            "PMC{} has no PDF in the open access subset.\n\
             Not every article ships one -- some are XML or tgz only.",
            numeric_id
        )
    })?;
    fetch_pdf(&format!("{}/{}", base_url, key))
}

/// Pick the `.pdf` object out of an S3 `ListBucketResult`.
fn first_pdf_key(listing: &str) -> Option<String> {
    listing
        .split("<Key>")
        .skip(1)
        .filter_map(|chunk| chunk.split("</Key>").next())
        .find(|key| key.ends_with(".pdf"))
        .map(|key| key.to_string())
}

fn http_get_text(url: &str) -> Result<String, String> {
    ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Read error: {}", e))
}

/// Fetch Semantic Scholar PDF bytes via the record's openAccessPdf URL.
pub fn pdf_bytes_semantic(base_url: &str, identifier: &str) -> Result<Vec<u8>, String> {
    let paper = sources::semantic::get_by_id(base_url, identifier)?
        .ok_or_else(|| format!("Paper not found: {}", identifier))?;
    let pdf_url = paper
        .pdf_url
        .ok_or_else(|| "No open access PDF available".to_string())?;
    fetch_pdf(&pdf_url)
}

/// Fetch CORE PDF bytes from its dedicated download endpoint.
///
/// Resolved in two explicit steps rather than by following a redirect.
///
/// `GET /v3/works/{id}/download` answers 302 to a file on `core.ac.uk`, a
/// different host. Letting the client chase that is fragile: the API key must
/// not travel to the redirect target -- sending it there is refused with a 400
/// -- and whether a given client strips it is a detail we would be depending
/// on. So the record is fetched first, authenticated, and its `downloadUrl` is
/// then fetched on its own with no credentials attached.
pub fn pdf_bytes_core(base_url: &str, identifier: &str) -> Result<Vec<u8>, String> {
    let paper = sources::core::get_by_id(base_url, identifier)?
        .ok_or_else(|| format!("Not found in CORE: {}", identifier))?;
    let pdf_url = paper
        .pdf_url
        .ok_or_else(|| format!("CORE has no downloadable file for {}", identifier))?;
    fetch_pdf(&pdf_url)
}

/// Fetch DOAJ PDF bytes via the record's fulltext link.
pub fn pdf_bytes_doaj(base_url: &str, identifier: &str) -> Result<Vec<u8>, String> {
    resolve_and_fetch(
        &doaj_meta_url(base_url, identifier),
        sources::doaj::parse_search_response,
    )
}

/// Fetch Zenodo PDF bytes via the record's file link.
pub fn pdf_bytes_zenodo(base_url: &str, identifier: &str) -> Result<Vec<u8>, String> {
    resolve_and_fetch(
        &zenodo_meta_url(base_url, identifier),
        sources::zenodo::parse_search_response,
    )
}

/// Fetch HAL PDF bytes via the record's fileMain_s URL.
pub fn pdf_bytes_hal(base_url: &str, identifier: &str) -> Result<Vec<u8>, String> {
    resolve_and_fetch(
        &hal_meta_url(base_url, identifier),
        sources::hal::parse_search_response,
    )
}

// The identifier reaches these straight from the command line, so it has to be
// encoded: a multi-word one used to produce an invalid URI rather than a query.

/// DOAJ puts the query in a path segment, where `+` stays a literal plus.
fn doaj_meta_url(base_url: &str, identifier: &str) -> String {
    format!(
        "{}/api/v4/search/articles/{}?pageSize=1",
        base_url,
        sources::encode_query(identifier).replace('+', "%20")
    )
}

fn zenodo_meta_url(base_url: &str, identifier: &str) -> String {
    format!(
        "{}/api/records?q={}&size=1&type=publication",
        base_url,
        sources::encode_query(identifier)
    )
}

fn hal_meta_url(base_url: &str, identifier: &str) -> String {
    format!(
        "{}/search/?q={}&rows=1&wt=json&fl=halId_s,title_s,authFullName_s,abstract_s,doiId_s,publicationDateY_i,fileMain_s,uri_s",
        base_url,
        sources::encode_query(identifier)
    )
}

/// Fetch Europe PMC PDF bytes via the record's `?pdf=render` URL.
pub fn pdf_bytes_europepmc(base_url: &str, identifier: &str) -> Result<Vec<u8>, String> {
    let paper = sources::europepmc::get_by_id(base_url, identifier)?
        .ok_or_else(|| format!("Not found in Europe PMC: {}", identifier))?;
    let pdf_url = paper
        .pdf_url
        .ok_or_else(|| format!("Europe PMC has no open access PDF for {}", identifier))?;
    fetch_pdf(&pdf_url)
}

/// Fetch a metadata URL, take the first record's `pdf_url`, and download it.
fn resolve_and_fetch(
    meta_url: &str,
    parse: fn(&str) -> Result<Vec<sources::Paper>, String>,
) -> Result<Vec<u8>, String> {
    let body = ureq::get(meta_url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Read error: {}", e))?;
    let papers = parse(&body)?;
    let pdf_url = papers
        .first()
        .and_then(|p| p.pdf_url.as_deref())
        .ok_or_else(|| "No PDF URL found".to_string())?;
    fetch_pdf(pdf_url)
}

/// Save PDF bytes to a file. Returns the path where the file was saved.
pub fn save_pdf(
    bytes: &[u8],
    dir: &Path,
    identifier: &str,
    overwrite: bool,
) -> Result<PathBuf, String> {
    // Several sources answer a PDF request with HTML -- an interstitial, a
    // login wall, a publisher landing page -- and writing that out under a .pdf
    // name while reporting success is worse than failing.
    if !bytes.starts_with(b"%PDF") {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]);
        let looks_like = if head.trim_start().to_lowercase().starts_with("<!doctype html")
            || head.trim_start().to_lowercase().starts_with("<html")
        {
            " The server returned HTML, not a file."
        } else if bytes.is_empty() {
            " The response was empty."
        } else {
            ""
        };
        return Err(format!(
            "Response is not a PDF.{}\nNothing was written.",
            looks_like
        ));
    }

    std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create directory: {}", e))?;

    let sanitized = identifier.replace('/', "_");
    let filename = format!("{}.pdf", sanitized);
    let path = dir.join(&filename);

    if path.exists() && !overwrite {
        return Err(format!("File already exists: {}", path.display()));
    }

    std::fs::write(&path, bytes).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fastpaper_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn save_pdf_creates_file() {
        let dir = temp_dir();
        let path = save_pdf(b"%PDF-1.4 fake", &dir, "2301.08745", false).unwrap();
        assert!(path.exists());
        assert!(path.to_str().unwrap().contains("2301.08745"));
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 1: download arxiv → mock PDF → file saved to dir
    #[test]
    fn arxiv_pdf_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4 fake content".as_slice())
            .create();
        let dir = temp_dir();
        let bytes = pdf_bytes_arxiv(&server.url(), "2301.08745").unwrap();
        let path = save_pdf(&bytes, &dir, "2301.08745", false).unwrap();
        assert!(path.exists());
        assert_eq!(fs::read(&path).unwrap(), b"%PDF-1.4 fake content");
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 2: saved filename contains paper ID
    #[test]
    fn arxiv_pdf_filename_contains_id() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        let dir = temp_dir();
        let bytes = pdf_bytes_arxiv(&server.url(), "2301.08745").unwrap();
        let path = save_pdf(&bytes, &dir, "2301.08745", false).unwrap();
        assert!(
            path.file_name().unwrap().to_str().unwrap().contains("2301.08745"),
            "filename {:?} should contain paper ID",
            path.file_name()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 3: --dir saves to specified directory
    #[test]
    fn arxiv_pdf_custom_dir() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        let dir = temp_dir().join("custom_subdir");
        let bytes = pdf_bytes_arxiv(&server.url(), "2301.08745").unwrap();
        let path = save_pdf(&bytes, &dir, "2301.08745", false).unwrap();
        assert!(path.starts_with(&dir));
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    // Behavior 4: file already exists → error with "already exists"
    #[test]
    fn arxiv_pdf_file_exists_returns_err() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        let dir = temp_dir();
        // First download succeeds
        let bytes = pdf_bytes_arxiv(&server.url(), "2301.08745").unwrap();
        save_pdf(&bytes, &dir, "2301.08745", false).unwrap();
        // Second save fails because the file exists
        let result = save_pdf(&bytes, &dir, "2301.08745", false);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("already exists"),
            "error should mention 'already exists'"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 5: --overwrite overwrites existing file
    #[test]
    fn arxiv_pdf_overwrite_succeeds() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4 version2".as_slice())
            .create();
        let dir = temp_dir();
        // Create existing file
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("2301.08745.pdf"), b"old content").unwrap();
        // Download with overwrite
        let bytes = pdf_bytes_arxiv(&server.url(), "2301.08745").unwrap();
        let path = save_pdf(&bytes, &dir, "2301.08745", true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"%PDF-1.4 version2");
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 6: mock 404 → error
    #[test]
    fn arxiv_pdf_404_returns_err() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .create();
        let dir = temp_dir();
        let result = pdf_bytes_arxiv(&server.url(), "9999.99999");
        assert!(result.is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    // ── additional source download tests ────────

    #[test]
    fn biorxiv_pdf_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("content.*full.pdf".to_string()))
            .with_status(200)
            .with_body(b"%PDF-1.4 biorxiv".as_slice())
            .create();
        let dir = temp_dir();
        let bytes = pdf_bytes_biorxiv(&server.url(), "10.1101/2024.01.01.574894").unwrap();
        let path = save_pdf(&bytes, &dir, "10.1101/2024.01.01.574894", false).unwrap();
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn medrxiv_pdf_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("content.*full.pdf".to_string()))
            .with_status(200)
            .with_body(b"%PDF-1.4 medrxiv".as_slice())
            .create();
        let dir = temp_dir();
        let bytes = pdf_bytes_medrxiv(&server.url(), "10.1101/2024.01.01.123456").unwrap();
        let path = save_pdf(&bytes, &dir, "10.1101/2024.01.01.123456", false).unwrap();
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    // Two steps now: list the article's prefix to learn the version, then
    // fetch the .pdf object.
    #[test]
    fn pmc_pdf_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("list-type=2".to_string()))
            .with_status(200)
            .with_body("<ListBucketResult><Contents><Key>PMC7318926.1/PMC7318926.1.pdf</Key></Contents></ListBucketResult>")
            .create();
        server
            .mock("GET", mockito::Matcher::Regex(r"\.pdf$".to_string()))
            .with_status(200)
            .with_body(b"%PDF-1.4 pmc".as_slice())
            .create();
        let dir = temp_dir();
        let bytes = pdf_bytes_pmc(&server.url(), "PMC7318926").unwrap();
        let path = save_pdf(&bytes, &dir, "PMC7318926", false).unwrap();
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pmc_pdf_strips_prefix() {
        let mut server = mockito::Server::new();
        let listing = server
            .mock("GET", mockito::Matcher::Regex("prefix=PMC7318926".to_string()))
            .with_status(200)
            .with_body("<ListBucketResult><Contents><Key>PMC7318926.1/PMC7318926.1.pdf</Key></Contents></ListBucketResult>")
            .create();
        server
            .mock("GET", mockito::Matcher::Regex(r"\.pdf$".to_string()))
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        // "PMC7318926" and "7318926" must produce the same prefix query.
        assert!(pdf_bytes_pmc(&server.url(), "PMC7318926").is_ok());
        listing.assert();
    }

    // An article that ships XML only should say so, not fail obscurely.
    #[test]
    fn pmc_without_a_pdf_reports_why() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body("<ListBucketResult><Contents><Key>PMC7318926.1/PMC7318926.1.xml</Key></Contents></ListBucketResult>")
            .create();
        let err = pdf_bytes_pmc(&server.url(), "PMC7318926").unwrap_err();
        assert!(err.contains("no PDF"), "got: {}", err);
    }


    #[test]
    fn fetch_pdf_returns_bytes() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4 test".as_slice())
            .create();
        let bytes = fetch_pdf(&format!("{}/test.pdf", server.url())).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn fetch_pdf_404_returns_err() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .create();
        let result = fetch_pdf(&format!("{}/missing.pdf", server.url()));
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod meta_url_tests {
    use super::*;

    // These resolvers take whatever the user typed. Interpolating it raw makes
    // any multi-word identifier an invalid URI: `download zenodo "machine
    // learning dataset"` failed with "http: invalid uri character".
    #[test]
    fn zenodo_meta_url_encodes_spaces() {
        let u = zenodo_meta_url("https://zenodo.org", "machine learning");
        assert!(!u.contains(' '), "raw space in URL: {}", u);
        assert!(u.contains("machine+learning"), "got: {}", u);
    }

    #[test]
    fn hal_meta_url_encodes_spaces() {
        let u = hal_meta_url("https://api.archives-ouvertes.fr", "machine learning");
        assert!(!u.contains(' '), "raw space in URL: {}", u);
    }

    // DOAJ carries the query in a path segment, where '+' is a literal plus.
    #[test]
    fn doaj_meta_url_uses_percent_twenty_not_plus() {
        let u = doaj_meta_url("https://doaj.org", "machine learning");
        assert!(u.contains("machine%20learning"), "got: {}", u);
        assert!(!u.contains("machine+learning"), "path segments need %20: {}", u);
    }
}

#[cfg(test)]
mod pdf_guard_tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fastpaper_guard_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    // PMC and DOAJ both used to answer a PDF request with HTML -- an
    // interstitial page and a publisher landing page -- and save_pdf wrote them
    // out under a .pdf name and reported success.
    #[test]
    fn save_pdf_rejects_html() {
        let dir = temp_dir();
        let err = save_pdf(b"<!DOCTYPE html><html>...", &dir, "x", false).unwrap_err();
        assert!(err.contains("not a PDF"), "got: {}", err);
        assert!(!dir.join("x.pdf").exists(), "nothing should be written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_pdf_error_names_html_when_that_is_what_arrived() {
        let dir = temp_dir();
        let err = save_pdf(b"\n\n  <html><head>", &dir, "x", false).unwrap_err();
        assert!(err.to_lowercase().contains("html"), "got: {}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_pdf_rejects_an_empty_body() {
        let dir = temp_dir();
        assert!(save_pdf(b"", &dir, "x", false).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_pdf_accepts_a_real_pdf() {
        let dir = temp_dir();
        assert!(save_pdf(b"%PDF-1.7\n...", &dir, "ok", false).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The PMC Cloud Service lists one prefix per article, versioned.
    const S3_LISTING: &str = r#"<?xml version="1.0"?><ListBucketResult>
<Contents><Key>PMC5334499.1/PMC5334499.1.json</Key></Contents>
<Contents><Key>PMC5334499.1/PMC5334499.1.pdf</Key></Contents>
<Contents><Key>PMC5334499.1/PMC5334499.1.txt</Key></Contents>
<Contents><Key>PMC5334499.1/WJR-9-27-g001.jpg</Key></Contents>
</ListBucketResult>"#;

    #[test]
    fn picks_the_pdf_key_out_of_the_listing() {
        assert_eq!(
            first_pdf_key(S3_LISTING),
            Some("PMC5334499.1/PMC5334499.1.pdf".to_string())
        );
    }

    // Not every open access article ships a standalone PDF; some are tgz only.
    #[test]
    fn returns_none_when_the_article_has_no_pdf() {
        let listing = r#"<ListBucketResult>
<Contents><Key>PMC7318926.1/PMC7318926.1.xml</Key></Contents>
</ListBucketResult>"#;
        assert_eq!(first_pdf_key(listing), None);
    }
}
