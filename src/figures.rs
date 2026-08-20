//! Pull original figure files out of a source's archive.
//!
//! Deliberately does not parse the PDF, the LaTeX, or the article XML: the
//! archive already holds the figures the authors uploaded, and every attempt
//! to be cleverer than that costs accuracy. See
//! `docs/superpowers/specs/2026-08-20-figures-command-design.md`.

/// Extensions worth pulling out of an archive.
///
/// `pdf` is on the list because arXiv figures usually *are* PDFs -- of the ten
/// figure files in 2408.05178, nine are `.pdf` and one is `.png`. Leaving it
/// off silently drops almost every arXiv figure.
const FIGURE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "tif", "tiff", "pdf", "eps", "svg",
];

/// Whether an archive entry looks like a figure worth keeping.
pub fn is_figure_file(name: &str) -> bool {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => {
            FIGURE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
        }
        _ => false,
    }
}

/// An archive entry's path, or `None` when it could write outside the target.
///
/// Archive entries are attacker-controlled: a `..` component or an absolute
/// path is how a zip escapes the directory it is being unpacked into. This
/// refuses rather than normalizes -- normalizing is its own attack surface,
/// and no legitimate figure archive needs it.
pub fn safe_entry_path(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return None;
    }
    // A Windows drive letter, e.g. `C:\...`.
    if name.len() >= 2 && name.as_bytes()[1] == b':' {
        return None;
    }
    let traverses = name
        .split(['/', '\\'])
        .any(|part| part == ".." || part == "." || part.is_empty());
    if traverses {
        return None;
    }
    Some(name.to_string())
}

/// Pull the figure files out of a zip (Europe PMC's supplementary package).
pub fn unzip_images(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, crate::download::FetchError> {
    use crate::download::FetchError;
    use std::io::Read;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| FetchError::Failed(format!("Could not open the archive: {}", e)))?;

    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| FetchError::Failed(format!("Could not read the archive: {}", e)))?;
        if !entry.is_file() {
            continue;
        }
        let Some(path) = safe_entry_path(entry.name()) else {
            continue;
        };
        if !is_figure_file(&path) {
            continue;
        }
        let mut body = Vec::new();
        entry
            .read_to_end(&mut body)
            .map_err(|e| FetchError::Failed(format!("Could not read {} from the archive: {}", path, e)))?;
        out.push((path, body));
    }
    Ok(out)
}

/// Pull the figure files out of a gzipped tarball (arXiv's e-print source).
pub fn untar_gz_images(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, crate::download::FetchError> {
    use crate::download::FetchError;
    use std::io::Read;

    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|e| FetchError::Failed(format!("Could not open the archive: {}", e)))?;

    let mut out = Vec::new();
    for entry in entries {
        let mut entry =
            entry.map_err(|e| FetchError::Failed(format!("Could not read the archive: {}", e)))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        // `to_string_lossy` rather than a hard error: one undecodable name in
        // an otherwise good tarball should cost that entry, not the request.
        // `safe_entry_path` still has the last word on whether it is written.
        let raw = entry
            .path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(path) = safe_entry_path(&raw) else {
            continue;
        };
        if !is_figure_file(&path) {
            continue;
        }
        let mut body = Vec::new();
        entry
            .read_to_end(&mut body)
            .map_err(|e| FetchError::Failed(format!("Could not read {} from the archive: {}", path, e)))?;
        out.push((path, body));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn figure_files_are_recognized_by_extension_case_insensitively() {
        assert!(is_figure_file("ocz228f1.jpg"));
        assert!(is_figure_file("figs/architecture.pdf"));
        assert!(is_figure_file("FIG1.PNG"));
        assert!(is_figure_file("plot.EpS"));
    }

    #[test]
    fn non_figure_files_are_rejected() {
        assert!(!is_figure_file("main.tex"));
        assert!(!is_figure_file("llncs.cls"));
        assert!(!is_figure_file("main.bbl"));
        assert!(!is_figure_file("ocz228-supplementary_data.zip"));
        assert!(!is_figure_file("00README.json"));
        assert!(!is_figure_file("noextension"));
    }

    // A directory entry named `figs/` ends in no extension and must not be
    // mistaken for a file.
    #[test]
    fn directory_entries_are_not_figures() {
        assert!(!is_figure_file("figs/"));
    }

    #[test]
    fn safe_paths_are_kept_verbatim() {
        assert_eq!(
            safe_entry_path("figs/architecture.pdf"),
            Some("figs/architecture.pdf".to_string())
        );
        assert_eq!(safe_entry_path("1.pdf"), Some("1.pdf".to_string()));
    }

    // An archive is attacker-controlled input: a `..` component or a leading
    // slash would let one write outside the directory the user named.
    // A bare `.` component is refused too -- refusing is cheaper to get right
    // than normalizing, and no legitimate figure archive uses `./`.
    #[test]
    fn traversing_paths_are_refused() {
        assert_eq!(safe_entry_path("../evil.png"), None);
        assert_eq!(safe_entry_path("figs/../../evil.png"), None);
        assert_eq!(safe_entry_path("/etc/passwd.png"), None);
        assert_eq!(safe_entry_path("figs/./ok.png"), None);
    }

    #[test]
    fn windows_style_traversal_is_refused() {
        assert_eq!(safe_entry_path("..\\evil.png"), None);
        assert_eq!(safe_entry_path("C:\\evil.png"), None);
    }

    #[test]
    fn empty_paths_are_refused() {
        assert_eq!(safe_entry_path(""), None);
    }

    use std::io::Write;

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            for (name, body) in entries {
                w.start_file(*name, opts).unwrap();
                w.write_all(body).unwrap();
            }
            w.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn unzip_keeps_figures_and_drops_everything_else() {
        let zip = build_zip(&[
            ("ocz228f1.jpg", b"jpeg-bytes"),
            ("ocz228f1.gif", b"gif-bytes"),
            ("ocz228-supplementary_data.zip", b"zip-bytes"),
            ("readme.txt", b"text"),
        ]);
        let got = unzip_images(&zip).unwrap();
        let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
        // The thumbnail stays: the design keeps every image file rather than
        // guessing which of a jpg/gif pair is the real one.
        assert_eq!(names, vec!["ocz228f1.jpg", "ocz228f1.gif"]);
        assert_eq!(got[0].1, b"jpeg-bytes");
    }

    #[test]
    fn unzip_returns_empty_when_the_archive_has_no_figures() {
        let zip = build_zip(&[("readme.txt", b"text")]);
        assert!(unzip_images(&zip).unwrap().is_empty());
    }

    #[test]
    fn unzip_skips_traversing_entries() {
        let zip = build_zip(&[("../evil.png", b"bad"), ("ok.png", b"good")]);
        let got = unzip_images(&zip).unwrap();
        let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["ok.png"]);
    }

    #[test]
    fn a_body_that_is_not_a_zip_is_a_failure_not_a_panic() {
        let err = unzip_images(b"<html>not a zip</html>").unwrap_err();
        assert!(err.message().contains("archive"), "got: {}", err.message());
    }

    // `tar::Builder::append_data` routes through `Header::set_path`, which
    // refuses `..` components -- exactly the kind of entry the traversal test
    // below needs to construct. Write the raw name field and use the
    // lower-level `append` instead so the test can build a malicious archive
    // on purpose; production code never takes this path.
    fn build_tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar = tar::Builder::new(Vec::new());
        for (name, body) in entries {
            let mut header = tar::Header::new_gnu();
            let name_bytes = name.as_bytes();
            header.as_old_mut().name[..name_bytes.len()].copy_from_slice(name_bytes);
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append(&header, *body).unwrap();
        }
        let raw = tar.into_inner().unwrap();
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&raw).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn untar_keeps_figures_including_nested_pdfs() {
        let tgz = build_tar_gz(&[
            ("figs/architecture.pdf", b"pdf-bytes"),
            ("figs/saliency.png", b"png-bytes"),
            ("main.tex", b"latex"),
            ("llncs.cls", b"class"),
        ]);
        let got = untar_gz_images(&tgz).unwrap();
        let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["figs/architecture.pdf", "figs/saliency.png"]);
        assert_eq!(got[0].1, b"pdf-bytes");
    }

    #[test]
    fn untar_returns_empty_when_the_archive_has_no_figures() {
        let tgz = build_tar_gz(&[("main.tex", b"latex")]);
        assert!(untar_gz_images(&tgz).unwrap().is_empty());
    }

    #[test]
    fn untar_skips_traversing_entries() {
        let tgz = build_tar_gz(&[("../evil.png", b"bad"), ("ok.png", b"good")]);
        let got = untar_gz_images(&tgz).unwrap();
        let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["ok.png"]);
    }

    #[test]
    fn a_body_that_is_not_a_tarball_is_a_failure_not_a_panic() {
        let err = untar_gz_images(b"%PDF-1.7 not a tarball").unwrap_err();
        assert!(err.message().contains("archive"), "got: {}", err.message());
    }
}
