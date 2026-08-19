//! Section extraction against real publisher PDFs.
//!
//! These need papers that are not in the repo, so they are ignored by default.
//! Point `FASTPAPER_PAPERS` at a directory of PDFs and run:
//!
//! ```text
//! FASTPAPER_PAPERS=/path/to/papers cargo test --test real_papers -- --ignored --nocapture
//! ```

use std::path::PathBuf;

fn papers_dir() -> Option<PathBuf> {
    std::env::var_os("FASTPAPER_PAPERS").map(PathBuf::from)
}

#[test]
#[ignore = "needs a local corpus of publisher PDFs"]
fn report_detected_headings() {
    let Some(dir) = papers_dir() else {
        eprintln!("set FASTPAPER_PAPERS to run this");
        return;
    };
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
                let headings = fastpaper::read::detect_headings(&doc.lines);
                let found: Vec<String> = headings
                    .iter()
                    .map(|h| format!("{}@{}({:.1})", h.section, h.offset, h.font_size))
                    .collect();
                println!("{:<42} {}", name, found.join(" "));
            }
            Err(e) => println!("{:<42} ERROR {}", name, e),
        }
    }
}

#[test]
#[ignore = "needs a local corpus of publisher PDFs"]
fn report_near_miss_lines() {
    let Some(dir) = papers_dir() else { return };
    let only = std::env::var("FASTPAPER_ONLY").unwrap_or_default();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
        .collect();
    entries.sort();
    const WORDS: &[&str] = &[
        "abstract",
        "introduction",
        "methods",
        "results",
        "discussion",
        "conclusion",
    ];
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !only.is_empty() && !name.contains(&only) {
            continue;
        }
        let Ok(doc) = fastpaper::read::extract_document(&path) else {
            continue;
        };
        println!("===== {}", name);
        for line in &doc.lines {
            let lower = line.text.to_lowercase();
            if line.text.len() < 70 && WORDS.iter().any(|w| lower.contains(w)) {
                println!(
                    "  {:>6.2} {:<5} len={:<3} | {:?}",
                    line.font_size,
                    if line.bold { "bold" } else { "" },
                    line.text.len(),
                    line.text
                );
            }
        }
    }
}

#[test]
#[ignore = "needs a local corpus of publisher PDFs"]
fn report_exact_heading_lines() {
    let Some(dir) = papers_dir() else { return };
    let only = std::env::var("FASTPAPER_ONLY").unwrap_or_default();
    let word = std::env::var("FASTPAPER_WORD").unwrap_or_else(|_| "results".into());
    for entry in std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".pdf") || (!only.is_empty() && !name.contains(&only)) {
            continue;
        }
        let Ok(doc) = fastpaper::read::extract_document(&path) else {
            continue;
        };
        println!("===== {}", name);
        for line in &doc.lines {
            if line.text.to_lowercase().contains(&word) && line.text.len() < std::env::var("FASTPAPER_MAXLEN").ok().and_then(|v| v.parse().ok()).unwrap_or(45usize) {
                println!(
                    "  @{:<6} {:>6.2} {:<5} | {:?}",
                    line.offset,
                    line.font_size,
                    if line.bold { "bold" } else { "" },
                    line.text
                );
            }
        }
    }
}

#[test]
#[ignore = "needs a local corpus of publisher PDFs"]
fn report_lines_around_offset() {
    let Some(dir) = papers_dir() else { return };
    let only = std::env::var("FASTPAPER_ONLY").unwrap_or_default();
    let at: usize = std::env::var("FASTPAPER_AT").unwrap().parse().unwrap();
    for entry in std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".pdf") || (!only.is_empty() && !name.contains(&only)) {
            continue;
        }
        let doc = fastpaper::read::extract_document(&path).unwrap();
        println!("===== {}", name);
        for line in &doc.lines {
            if line.offset + 400 > at && line.offset < at + 400 {
                println!(
                    "  @{:<6} {:>6.2} {:<5} | {:?}",
                    line.offset,
                    line.font_size,
                    if line.bold { "bold" } else { "" },
                    line.text.chars().take(80).collect::<String>()
                );
            }
        }
    }
}
