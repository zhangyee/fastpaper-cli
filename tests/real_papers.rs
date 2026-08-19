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

/// Not an assertion -- a triage aid. Prints every detected heading with the
/// page and baseline it sits on, so the page can be rendered and the reading
/// checked against what the publisher actually printed.
///
/// Rows are rebuilt here exactly as `extract_document` builds them, so the
/// running offset lines up with the heading offsets the library reports and
/// the lookup is exact rather than a text match.
#[test]
#[ignore = "diagnostic, not an assertion"]
fn report_heading_positions() {
    let Some(dir) = papers_dir() else { return };
    let only = std::env::var("FASTPAPER_ONLY").unwrap_or_default();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
        .collect();
    entries.sort();

    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !only.is_empty() && !name.contains(&only) {
            continue;
        }
        let Ok(document) = fastpaper::read::extract_document(&path) else {
            continue;
        };
        let Ok(pdf) = pdf_oxide::PdfDocument::open(&path) else {
            continue;
        };
        let Ok(pages) = pdf.page_count() else { continue };

        // (offset, page, y, page_height) for every rebuilt row
        let mut placed: Vec<(usize, usize, f32, f32)> = Vec::new();
        let mut offset = 0usize;
        for page in 0..pages {
            let height = pdf.extract_page_text(page).map(|t| t.page_height).unwrap_or(792.0);
            let Ok(spans) =
                pdf.extract_spans_with_reading_order(page, pdf_oxide::ReadingOrder::ColumnAware)
            else {
                continue;
            };
            let mut row: Vec<&pdf_oxide::layout::TextSpan> = Vec::new();
            let mut flush = |row: &Vec<&pdf_oxide::layout::TextSpan>, offset: &mut usize| {
                let Some(first) = row.first() else { return };
                let joined: String = row.iter().map(|s| s.text.as_str()).collect();
                let trimmed = joined.trim();
                if trimmed.is_empty() {
                    return;
                }
                placed.push((*offset, page, first.bbox.y, height));
                *offset += trimmed.len() + 1;
            };
            for span in &spans {
                if span.text.trim().is_empty() {
                    continue;
                }
                let moved = row.last().is_some_and(|last: &&pdf_oxide::layout::TextSpan| {
                    (last.bbox.y - span.bbox.y).abs() > (span.font_size * 0.4).max(1.0)
                });
                if moved {
                    flush(&row, &mut offset);
                    row.clear();
                }
                row.push(span);
            }
            flush(&row, &mut offset);
        }

        let headings = fastpaper::read::detect_headings(&document.lines);
        let mut emit = |kind: &str, section: &str, offset: usize, text: &str| {
            if let Some((_, page, y, height)) = placed.iter().find(|(at, ..)| *at == offset) {
                println!(
                    "{}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{}",
                    name, kind, section, page + 1, y, height, text
                );
            }
        };
        for heading in &headings {
            emit("HEAD", heading.section, heading.offset, &heading.text);
        }

        // Lines that look like a heading but were not read as one. Rendered
        // alongside, these are what a false negative looks like.
        const WORDS: &[&str] = &[
            "abstract", "introduction", "background", "method", "result",
            "discussion", "conclusion", "reference",
        ];
        let taken: Vec<usize> = headings.iter().map(|h| h.offset).collect();
        let body = {
            let mut tally: Vec<(f32, usize)> = Vec::new();
            for line in &document.lines {
                let size = (line.font_size * 10.0).round() / 10.0;
                match tally.iter_mut().find(|(seen, _)| *seen == size) {
                    Some((_, chars)) => *chars += line.text.len(),
                    None => tally.push((size, line.text.len())),
                }
            }
            tally.iter().max_by_key(|(_, c)| *c).map(|(s, _)| *s).unwrap_or(0.0)
        };
        for line in &document.lines {
            let lower = line.text.to_lowercase();
            if line.font_size > body * 1.02
                && line.text.len() < 72
                && WORDS.iter().any(|w| lower.contains(w))
                && !taken.contains(&line.offset)
            {
                emit("MISS", "-", line.offset, &line.text);
            }
        }
    }
}
