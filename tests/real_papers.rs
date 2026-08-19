//! Section extraction against real publisher PDFs.
//!
//! The corpus is not in the repo -- these are copyrighted papers -- so the
//! tests are ignored by default. Point `FASTPAPER_PAPERS` at a directory of
//! PDFs and run:
//!
//! ```text
//! FASTPAPER_PAPERS=/path/to/papers cargo test --test real_papers -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};

fn papers_dir() -> Option<PathBuf> {
    std::env::var_os("FASTPAPER_PAPERS").map(PathBuf::from)
}

fn read(dir: &Path, name: &str) -> Option<fastpaper::read::Document> {
    let path = dir.join(name);
    if !path.exists() {
        eprintln!("skipping {}: not in the corpus", name);
        return None;
    }
    Some(fastpaper::read::extract_document(&path).expect("should read"))
}

/// Papers that used to be sliced from the wrong place, with the opening of the
/// section as the publisher actually prints it. Each was verified by hand
/// against the PDF.
const KNOWN_SECTIONS: &[(&str, &str, &str)] = &[
    // Scientific Reports: the old search matched `results` in the abstract's
    // closing sentence and `methods` in "conventional interpretation methods".
    (
        "10.1038_s41598-020-77599-6.pdf",
        "results",
        "The study population included",
    ),
    (
        "10.1038_s41598-020-77599-6.pdf",
        "methods",
        "This study was approved by the Institutional Review Boards",
    ),
    // EHJ Digital Health: the abstract's `Methods and results` label used to
    // win, and the section it produced was the single word "and".
    ("10.1093_ehjdh_ztaf125.pdf", "methods", "Ethical approval"),
    // European Heart Journal: the structured abstract's `Methods` label won.
    (
        "10.1093_eurheartj_ehaf004.pdf",
        "methods",
        "This study was a prospective multicentre",
    ),
    // EuroIntervention and Elsevier: `Results:` in the structured abstract.
    (
        "10.4244_eij-d-20-01155.pdf",
        "results",
        "BASELINE CHARACTERISTICS",
    ),
    (
        "10.1016_j.jelectrocard.2024.153792.pdf",
        "results",
        "Table 1 summarizes accuracy",
    ),
    // Cardiovascular Diagnosis and Therapy sets the `Background` subsection in
    // the same size as the `Introduction` chapter above it, telling them apart
    // by typeface and colour alone -- neither of which survives into text. The
    // introduction has to keep the subsection rather than end at it. Verified
    // against a render of page 2.
    ("10.21037_cdt-2025-aw-561.pdf", "introduction", "Background"),
    // Same paper, plural heading: `Conclusions`. Verified against page 15.
    (
        "10.21037_cdt-2025-aw-561.pdf",
        "conclusion",
        "AI-enhanced ECG for AMI detection shows promising",
    ),
    // JMIR heads its sections in a face pdf_oxide reports as italic, though it
    // renders upright -- style flags are not a reliable heading signal.
    ("10.2196_31129.pdf", "introduction", "Wearable devices"),
];

#[test]
#[ignore = "needs a local corpus of publisher PDFs"]
fn known_sections_start_where_the_publisher_prints_them() {
    let Some(dir) = papers_dir() else {
        eprintln!("set FASTPAPER_PAPERS to run this");
        return;
    };
    for (name, section, opening) in KNOWN_SECTIONS {
        let Some(doc) = read(&dir, name) else { continue };
        let found = fastpaper::read::find_section(&doc, section)
            .unwrap_or_else(|| panic!("{}: no '{}' section found", name, section));
        assert!(
            found.body.starts_with(opening),
            "{} --section {} should open with {:?}, got {:?}",
            name,
            section,
            opening,
            found.body.chars().take(90).collect::<String>()
        );
    }
}

// European Heart Journal prints `Background and Aims` directly under
// `Abstract`. The old extractor cut between them, got whitespace, and reported
// that a paper with an abstract right there had none.
#[test]
#[ignore = "needs a local corpus of publisher PDFs"]
fn an_abstract_under_a_structured_label_is_still_found() {
    let Some(dir) = papers_dir() else { return };
    let Some(doc) = read(&dir, "10.1093_eurheartj_ehaf004.pdf") else {
        return;
    };
    let found = fastpaper::read::find_section(&doc, "abstract").expect("abstract should be found");
    assert!(
        found.body.contains("Emerging evidence supports"),
        "got: {:?}",
        found.body.chars().take(120).collect::<String>()
    );
}

// The `and` bug: a section cut between two headings a dozen bytes apart. No
// real section of a paper is one word long, so a very short body means the
// slice was taken from the wrong place.
#[test]
#[ignore = "needs a local corpus of publisher PDFs"]
fn no_section_in_the_corpus_comes_back_as_a_fragment() {
    let Some(dir) = papers_dir() else { return };
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().is_none_or(|x| x != "pdf") {
            continue;
        }
        let Ok(doc) = fastpaper::read::extract_document(&path) else {
            continue;
        };
        for heading in fastpaper::read::detect_headings(&doc.lines) {
            let found = fastpaper::read::find_section(&doc, heading.section).unwrap();
            assert!(
                found.body.len() > 40,
                "{}: '{}' came back as {:?}",
                path.display(),
                heading.section,
                found.body
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no sections were checked");
    println!("checked {} sections", checked);
}

/// Not an assertion -- a triage aid. Prints what the detector makes of every
/// paper in the corpus, which is how to see whether a new publisher's layout
/// is being read correctly.
#[test]
#[ignore = "diagnostic, not an assertion"]
fn report_detected_headings() {
    let Some(dir) = papers_dir() else { return };
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
        .collect();
    entries.sort();

    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        match fastpaper::read::extract_document(&path) {
            Ok(doc) => {
                let found: Vec<String> = fastpaper::read::detect_headings(&doc.lines)
                    .iter()
                    .map(|h| format!("{}@{}({:.1})", h.section, h.offset, h.font_size))
                    .collect();
                println!("{:<42} {}", name, found.join(" "));
            }
            Err(e) => println!("{:<42} ERROR {}", name, e),
        }
    }
}

/// Not an assertion -- a triage aid. Prints heading-shaped lines the alias
/// table did not recognise, which is how a publisher's wording variant gets
/// found rather than guessed at.
#[test]
#[ignore = "diagnostic, not an assertion"]
fn report_unrecognised_heading_shapes() {
    let Some(dir) = papers_dir() else { return };
    const WORDS: &[&str] = &[
        "abstract",
        "introduction",
        "background",
        "method",
        "result",
        "discussion",
        "conclusion",
        "reference",
        "related work",
        "summary",
    ];
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
        .collect();
    entries.sort();
    for path in entries {
        let Ok(doc) = fastpaper::read::extract_document(&path) else {
            continue;
        };
        let recognised: Vec<usize> = fastpaper::read::detect_headings(&doc.lines)
            .iter()
            .map(|h| h.offset)
            .collect();
        for line in &doc.lines {
            let lower = line.text.to_lowercase();
            if line.text.len() < 40
                && WORDS.iter().any(|w| lower.contains(w))
                && !recognised.contains(&line.offset)
            {
                println!(
                    "{:<40} {:>6.2} | {}",
                    path.file_name().unwrap().to_string_lossy(),
                    line.font_size,
                    line.text
                );
            }
        }
    }
}
