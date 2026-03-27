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

/// Download a PDF from arXiv and save it to disk.
pub fn download_arxiv(
    base_url: &str,
    identifier: &str,
    dir: &Path,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let bytes = sources::arxiv::download_pdf(base_url, identifier)?;
    save_pdf(&bytes, dir, identifier, overwrite)
}

/// Download a PDF from bioRxiv.
pub fn download_biorxiv(
    base_url: &str,
    identifier: &str,
    dir: &Path,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let url = format!("{}/content/{}v1.full.pdf", base_url, identifier);
    let bytes = fetch_pdf(&url)?;
    save_pdf(&bytes, dir, identifier, overwrite)
}

/// Download a PDF from medRxiv.
pub fn download_medrxiv(
    base_url: &str,
    identifier: &str,
    dir: &Path,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let url = format!("{}/content/{}v1.full.pdf", base_url, identifier);
    let bytes = fetch_pdf(&url)?;
    save_pdf(&bytes, dir, identifier, overwrite)
}

/// Download a PDF from PMC.
pub fn download_pmc(
    base_url: &str,
    identifier: &str,
    dir: &Path,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let numeric_id = identifier.strip_prefix("PMC").unwrap_or(identifier);
    let url = format!("{}/pmc/articles/PMC{}/pdf/", base_url, numeric_id);
    let bytes = fetch_pdf(&url)?;
    save_pdf(&bytes, dir, identifier, overwrite)
}

/// Download a PDF from Europe PMC (fetch metadata first to get full text URL).
pub fn download_europepmc(
    base_url: &str,
    identifier: &str,
    dir: &Path,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let meta_url = format!(
        "{}/europepmc/webservices/rest/search?query={}&pageSize=1&format=json&resultType=core",
        base_url, identifier
    );
    let body = ureq::get(&meta_url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Read error: {}", e))?;
    let papers = sources::europepmc::parse_search_response(&body)?;
    let pdf_url = papers
        .first()
        .and_then(|p| p.url.as_deref())
        .ok_or_else(|| "No PDF URL found".to_string())?;
    let bytes = fetch_pdf(pdf_url)?;
    save_pdf(&bytes, dir, identifier, overwrite)
}

/// Download a PDF from Semantic Scholar (fetch metadata to get openAccessPdf URL).
pub fn download_semantic(
    base_url: &str,
    identifier: &str,
    dir: &Path,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let paper = sources::semantic::get_by_id(base_url, identifier)?
        .ok_or_else(|| format!("Paper not found: {}", identifier))?;
    let pdf_url = paper
        .pdf_url
        .ok_or_else(|| "No open access PDF available".to_string())?;
    let bytes = fetch_pdf(&pdf_url)?;
    save_pdf(&bytes, dir, identifier, overwrite)
}

/// Download a PDF from CORE (fetch metadata to get downloadUrl).
pub fn download_core(
    base_url: &str,
    identifier: &str,
    dir: &Path,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let meta_url = format!(
        "{}/v3/search/works?q={}&limit=1",
        base_url, identifier
    );
    let body = ureq::get(&meta_url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Read error: {}", e))?;
    let papers = sources::core::parse_search_response(&body)?;
    let pdf_url = papers
        .first()
        .and_then(|p| p.pdf_url.as_deref())
        .ok_or_else(|| "No PDF URL found".to_string())?;
    let bytes = fetch_pdf(pdf_url)?;
    save_pdf(&bytes, dir, identifier, overwrite)
}

/// Download a PDF from DOAJ (fetch metadata to get fulltext URL).
pub fn download_doaj(
    base_url: &str,
    identifier: &str,
    dir: &Path,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let meta_url = format!(
        "{}/api/search/articles/{}?pageSize=1",
        base_url, identifier
    );
    let body = ureq::get(&meta_url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Read error: {}", e))?;
    let papers = sources::doaj::parse_search_response(&body)?;
    let pdf_url = papers
        .first()
        .and_then(|p| p.pdf_url.as_deref())
        .ok_or_else(|| "No PDF URL found".to_string())?;
    let bytes = fetch_pdf(pdf_url)?;
    save_pdf(&bytes, dir, identifier, overwrite)
}

/// Download a PDF from Zenodo (fetch metadata to get file URL).
pub fn download_zenodo(
    base_url: &str,
    identifier: &str,
    dir: &Path,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let meta_url = format!(
        "{}/api/records?q={}&size=1&type=publication",
        base_url, identifier
    );
    let body = ureq::get(&meta_url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Read error: {}", e))?;
    let papers = sources::zenodo::parse_search_response(&body)?;
    let pdf_url = papers
        .first()
        .and_then(|p| p.pdf_url.as_deref())
        .ok_or_else(|| "No PDF URL found".to_string())?;
    let bytes = fetch_pdf(pdf_url)?;
    save_pdf(&bytes, dir, identifier, overwrite)
}

/// Download a PDF from HAL (fetch metadata to get fileMain_s URL).
pub fn download_hal(
    base_url: &str,
    identifier: &str,
    dir: &Path,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let meta_url = format!(
        "{}/search/?q={}&rows=1&wt=json&fl=halId_s,title_s,authFullName_s,abstract_s,doiId_s,publicationDateY_i,fileMain_s,uri_s",
        base_url, identifier
    );
    let body = ureq::get(&meta_url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Read error: {}", e))?;
    let papers = sources::hal::parse_search_response(&body)?;
    let pdf_url = papers
        .first()
        .and_then(|p| p.pdf_url.as_deref())
        .ok_or_else(|| "No PDF URL found".to_string())?;
    let bytes = fetch_pdf(pdf_url)?;
    save_pdf(&bytes, dir, identifier, overwrite)
}

/// Save PDF bytes to a file. Returns the path where the file was saved.
pub fn save_pdf(
    bytes: &[u8],
    dir: &Path,
    identifier: &str,
    overwrite: bool,
) -> Result<PathBuf, String> {
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
    fn download_arxiv_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4 fake content".as_slice())
            .create();
        let dir = temp_dir();
        let path = download_arxiv(&server.url(), "2301.08745", &dir, false).unwrap();
        assert!(path.exists());
        assert_eq!(fs::read(&path).unwrap(), b"%PDF-1.4 fake content");
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 2: saved filename contains paper ID
    #[test]
    fn download_arxiv_filename_contains_id() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        let dir = temp_dir();
        let path = download_arxiv(&server.url(), "2301.08745", &dir, false).unwrap();
        assert!(
            path.file_name().unwrap().to_str().unwrap().contains("2301.08745"),
            "filename {:?} should contain paper ID",
            path.file_name()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 3: --dir saves to specified directory
    #[test]
    fn download_arxiv_custom_dir() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        let dir = temp_dir().join("custom_subdir");
        let path = download_arxiv(&server.url(), "2301.08745", &dir, false).unwrap();
        assert!(path.starts_with(&dir));
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    // Behavior 4: file already exists → error with "already exists"
    #[test]
    fn download_arxiv_file_exists_returns_err() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        let dir = temp_dir();
        // First download succeeds
        download_arxiv(&server.url(), "2301.08745", &dir, false).unwrap();
        // Second download fails because file exists
        let result = download_arxiv(&server.url(), "2301.08745", &dir, false);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("already exists"),
            "error should mention 'already exists'"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 5: --overwrite overwrites existing file
    #[test]
    fn download_arxiv_overwrite_succeeds() {
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
        let path = download_arxiv(&server.url(), "2301.08745", &dir, true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"%PDF-1.4 version2");
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 6: mock 404 → error
    #[test]
    fn download_arxiv_404_returns_err() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .create();
        let dir = temp_dir();
        let result = download_arxiv(&server.url(), "9999.99999", &dir, false);
        assert!(result.is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    // ── additional source download tests ────────

    #[test]
    fn download_biorxiv_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("content.*full.pdf".to_string()))
            .with_status(200)
            .with_body(b"%PDF-1.4 biorxiv".as_slice())
            .create();
        let dir = temp_dir();
        let path = download_biorxiv(&server.url(), "10.1101/2024.01.01.574894", &dir, false).unwrap();
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn download_medrxiv_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("content.*full.pdf".to_string()))
            .with_status(200)
            .with_body(b"%PDF-1.4 medrxiv".as_slice())
            .create();
        let dir = temp_dir();
        let path = download_medrxiv(&server.url(), "10.1101/2024.01.01.123456", &dir, false).unwrap();
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn download_pmc_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("pmc/articles/PMC".to_string()))
            .with_status(200)
            .with_body(b"%PDF-1.4 pmc".as_slice())
            .create();
        let dir = temp_dir();
        let path = download_pmc(&server.url(), "PMC7318926", &dir, false).unwrap();
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn download_pmc_strips_prefix() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("PMC7318926".to_string()))
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        let dir = temp_dir();
        let result = download_pmc(&server.url(), "PMC7318926", &dir, false);
        assert!(result.is_ok());
        let _ = fs::remove_dir_all(&dir);
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
