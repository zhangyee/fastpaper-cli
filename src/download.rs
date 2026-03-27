use std::path::{Path, PathBuf};

use crate::sources::arxiv;

/// Download a PDF from arXiv and save it to disk.
pub fn download_arxiv(
    base_url: &str,
    identifier: &str,
    dir: &Path,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let bytes = arxiv::download_pdf(base_url, identifier)?;
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
}
