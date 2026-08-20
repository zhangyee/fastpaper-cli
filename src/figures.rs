//! Pull original figure files out of a source's archive.
//!
//! Deliberately does not parse the PDF, the LaTeX, or the article XML: the
//! archive already holds the figures the authors uploaded, and every attempt
//! to be cleverer than that costs accuracy. See
//! `docs/superpowers/specs/2026-08-20-figures-command-design.md`.

use std::path::{Path, PathBuf};

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

/// Why [`save_figures`] failed to write.
///
/// Typed rather than `String` so the caller can distinguish "a target file is
/// already there" (not an error worth a non-zero exit) from every other
/// failure without string-matching a formatted message -- see the incident
/// this guarded against: a hostile entry-name rejection whose message
/// happened to contain the substring "already exists" and so was reported as
/// success.
#[derive(Debug)]
pub enum SaveError {
    /// A target file already exists and `overwrite` was not set.
    AlreadyExists(PathBuf),
    /// The identifier itself (after flattening `/` to `_`) is not a single
    /// safe path component -- `.`, `..`, or an absolute/drive path.
    UnsafeIdentifier,
    /// An archive entry name failed `safe_entry_path` validation.
    HostileEntry,
    /// A filesystem operation failed.
    Io(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::AlreadyExists(path) => {
                write!(f, "File already exists: {}", path.display())
            }
            SaveError::UnsafeIdentifier => {
                write!(f, "Rejected an identifier that would escape the target directory.")
            }
            // Deliberately does not include the rejected name: it is
            // attacker-controlled bytes from an archive entry, and echoing it
            // verbatim to a terminal is its own hazard.
            SaveError::HostileEntry => {
                write!(f, "Rejected an archive entry with an unsafe path.")
            }
            SaveError::Io(m) => write!(f, "{}", m),
        }
    }
}

impl std::error::Error for SaveError {}

/// Write the figures under `<dir>/<identifier>/`, keeping the archive's own
/// directory layout.
///
/// The identifier gets its own subdirectory because archive filenames are not
/// unique across papers -- arXiv's are routinely just `1.pdf`, `2.pdf` -- so a
/// flat layout would have two papers overwrite each other. `/` in the
/// identifier is flattened to `_` exactly as `save_pdf` does, so a DOI names
/// one directory rather than two levels.
///
/// The identifier is attacker-influenced too -- the explicit-source CLI form
/// (`figures europepmc <id>`) skips `detect_id_type` entirely, so a value
/// like `..` or `.` reaches here unchecked. It is validated with the same
/// `safe_entry_path` used for archive entries before it becomes a directory
/// component: unlike a *filename*, where `..` merely becomes the harmless
/// `...pdf`, a `..` *directory* component is a real traversal.
///
/// Entry names are re-validated here with `safe_entry_path` to reject hostile
/// paths (absolute paths, `..` traversals) regardless of upstream filtering.
pub fn save_figures(
    files: &[(String, Vec<u8>)],
    dir: &Path,
    identifier: &str,
    overwrite: bool,
) -> Result<(PathBuf, u64), SaveError> {
    let sanitized = identifier.replace('/', "_");
    if safe_entry_path(&sanitized).is_none() {
        return Err(SaveError::UnsafeIdentifier);
    }
    let out = dir.join(&sanitized);

    // Re-validate all entry names to reject hostile paths: absolute paths,
    // `..` traversals, and other attacks. Do this before writing anything.
    for (name, _) in files {
        if safe_entry_path(name).is_none() {
            return Err(SaveError::HostileEntry);
        }
    }

    // Check every target before writing any of them: a half-written directory
    // after a collision is worse than writing nothing.
    if !overwrite {
        for (name, _) in files {
            let path = out.join(name);
            if path.exists() {
                return Err(SaveError::AlreadyExists(path));
            }
        }
    }

    let mut total = 0u64;
    for (name, body) in files {
        let path = out.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| SaveError::Io(format!("Failed to create directory: {}", e)))?;
        }
        std::fs::write(&path, body)
            .map_err(|e| SaveError::Io(format!("Failed to write file: {}", e)))?;
        total += body.len() as u64;
    }
    Ok((out, total))
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

    #[test]
    fn figures_land_in_a_subdirectory_named_for_the_identifier() {
        let dir = std::env::temp_dir().join(format!("fp_fig_{}_a", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let files = vec![
            ("figs/architecture.pdf".to_string(), b"pdf".to_vec()),
            ("saliency.png".to_string(), b"png".to_vec()),
        ];
        let (out, total) = save_figures(&files, &dir, "2408.05178", false).unwrap();

        assert_eq!(out, dir.join("2408.05178"));
        assert_eq!(total, 6);
        assert_eq!(
            std::fs::read(dir.join("2408.05178/figs/architecture.pdf")).unwrap(),
            b"pdf"
        );
        assert!(dir.join("2408.05178/saliency.png").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A DOI has a `/` in it and must not become a directory level.
    #[test]
    fn a_slash_in_the_identifier_is_flattened_like_download_does() {
        let dir = std::env::temp_dir().join(format!("fp_fig_{}_b", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let files = vec![("f1.jpg".to_string(), b"x".to_vec())];
        let (out, _) = save_figures(&files, &dir, "10.1038/s41586-025-09052-5", false).unwrap();
        assert_eq!(out, dir.join("10.1038_s41586-025-09052-5"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_existing_file_is_refused_unless_overwrite_is_set() {
        let dir = std::env::temp_dir().join(format!("fp_fig_{}_c", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("PMC1")).unwrap();
        std::fs::write(dir.join("PMC1/f1.jpg"), b"old").unwrap();

        let files = vec![("f1.jpg".to_string(), b"new".to_vec())];
        let err = save_figures(&files, &dir, "PMC1", false).unwrap_err();
        assert!(
            matches!(err, SaveError::AlreadyExists(_)),
            "got: {:?}",
            err
        );
        assert!(err.to_string().contains("already exists"), "got: {}", err);
        // Refused means nothing changed.
        assert_eq!(std::fs::read(dir.join("PMC1/f1.jpg")).unwrap(), b"old");

        save_figures(&files, &dir, "PMC1", true).unwrap();
        assert_eq!(std::fs::read(dir.join("PMC1/f1.jpg")).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hostile_entry_names_are_rejected() {
        let dir = std::env::temp_dir().join(format!("fp_fig_{}_d", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Absolute path would write outside the target directory if not rejected.
        let files = vec![("/etc/cron.d/evil".to_string(), b"hostile".to_vec())];
        let err = save_figures(&files, &dir, "test", false).unwrap_err();
        assert!(matches!(err, SaveError::HostileEntry), "got: {:?}", err);

        // `..` traversal would write outside the target directory if not rejected.
        let files = vec![("../../../etc/passwd".to_string(), b"hostile".to_vec())];
        let err = save_figures(&files, &dir, "test", false).unwrap_err();
        assert!(matches!(err, SaveError::HostileEntry), "got: {:?}", err);

        // Verify nothing was written outside the target directory.
        assert!(!std::path::Path::new("/etc/cron.d/evil").exists()
            || std::fs::read("/etc/cron.d/evil").is_err(),
            "hostile absolute path should not be written");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // A hostile-entry rejection must never collide, by substring, with the
    // "already exists" message -- that collision once made a security
    // rejection report itself as success. `Display` producing a fixed string
    // with no attacker-controlled bytes and no shared substring is the fix;
    // this pins both properties down.
    #[test]
    fn the_hostile_entry_message_does_not_collide_with_already_exists() {
        let msg = SaveError::HostileEntry.to_string();
        assert!(!msg.contains("already exists"), "got: {}", msg);
    }

    // The identifier reaches `save_figures` unchecked when the CLI's
    // explicit-source form is used (`figures europepmc <id>` skips
    // `detect_id_type`), so `..`, `.`, and an absolute/drive path must all be
    // refused here rather than trusted from a routing step that never ran.
    #[test]
    fn the_identifier_cannot_escape_dir() {
        let dir = std::env::temp_dir().join(format!("fp_fig_{}_e", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let files = vec![("f1.jpg".to_string(), b"x".to_vec())];

        // `..` as the whole identifier would land the subdirectory a level
        // above `--dir`.
        assert!(matches!(
            save_figures(&files, &dir, "..", false),
            Err(SaveError::UnsafeIdentifier)
        ));
        // `.` would collapse the per-identifier subdirectory into `--dir`
        // itself, breaking the no-collision guarantee that subdirectory
        // exists to provide.
        assert!(matches!(
            save_figures(&files, &dir, ".", false),
            Err(SaveError::UnsafeIdentifier)
        ));
        // A Windows drive path is absolute regardless of `dir`.
        assert!(matches!(
            save_figures(&files, &dir, "C:\\evil", false),
            Err(SaveError::UnsafeIdentifier)
        ));

        assert!(!dir.exists() || std::fs::read_dir(&dir).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
