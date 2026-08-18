use std::path::Path;

use super::{CommandError, CommandResult, chars_summary, emit, failed, matches_summary};
use crate::cli::{GlobalOpts, OutputFormat, ReadArgs, Section};
use crate::grep::{self, GrepResult};
use crate::read as pdf;

pub fn run(args: &ReadArgs, global: &GlobalOpts) -> CommandResult {
    if !args.path.exists() {
        return Err(failed(format!(
            "No such file: {}\n`read` works on a local PDF. To fetch one first: fastpaper download <id>",
            args.path.display()
        )));
    }

    let full_text = pdf::extract_text(&args.path).map_err(failed)?;
    let section = section_name(args.section);

    // A section the heuristic could not find is an empty answer, not a
    // malformed command -- same shape as `get` failing to find a paper, so it
    // takes the same exit code instead of the "you typed it wrong" one.
    let text = match heading_for(args.section) {
        None => full_text,
        Some(heading) => pdf::extract_section(&full_text, heading).ok_or_else(|| {
            CommandError::NotFound(format!(
                "No '{}' section found in {}",
                heading,
                args.path.display()
            ))
        })?,
    };

    match args.grep.as_deref() {
        Some(pattern) => run_grep(args, global, &text, section, pattern),
        None => {
            let text = match args.max_length {
                Some(max) => text.chars().take(max).collect::<String>(),
                None => text,
            };
            let char_count = text.chars().count();
            let rendered = match global.format {
                OutputFormat::Json => serde_json::to_string_pretty(&serde_json::json!({
                    "path": args.path.to_string_lossy(),
                    "section": section,
                    "content": { "full_text": text },
                }))
                .unwrap(),
                _ => text,
            };
            let summary = chars_summary(char_count);
            emit(&rendered, args.output.as_deref(), &summary, global.quiet)
        }
    }
}

/// Search the extracted text and emit the matches.
///
/// No match exits 4 rather than 0: `--grep` names a specific thing the caller
/// expects to be there, which is `get`'s situation, not `search`'s. Exit 1 would
/// have read like a malformed command. Nothing is written either -- an empty
/// file for a failed search is the same trap as a silent `-o`.
fn run_grep(
    args: &ReadArgs,
    global: &GlobalOpts,
    text: &str,
    section: &str,
    pattern: &str,
) -> CommandResult {
    let re = grep::build_regex(pattern).map_err(failed)?;
    let mut result = grep::grep(text, &re, args.context, args.max_matches);

    if result.total_matches == 0 {
        return Err(CommandError::NotFound(format!(
            "No match for '{}' in {} (searched: {})",
            pattern,
            args.path.display(),
            section
        )));
    }

    if let Some(max) = args.max_length {
        grep::apply_budget(&mut result, max);
    }

    let rendered = match global.format {
        OutputFormat::Json => render_grep_json(&args.path, section, pattern, &result),
        _ => render_grep_text(&result),
    };
    let summary = matches_summary(result.shown_matches, result.total_matches);
    emit(&rendered, args.output.as_deref(), &summary, global.quiet)
}

/// The heading `extract_section` should look for, or `None` for the whole text.
fn heading_for(section: Section) -> Option<&'static str> {
    match section {
        Section::Full => None,
        Section::Abstract => Some("abstract"),
        Section::Introduction => Some("introduction"),
        Section::Methods => Some("methods"),
        Section::Results => Some("results"),
        Section::Discussion => Some("discussion"),
        Section::Conclusion => Some("conclusion"),
        Section::References => Some("references"),
    }
}

fn section_name(section: Section) -> &'static str {
    heading_for(section).unwrap_or("full")
}

/// Render matches for a human: one headed excerpt per window.
fn render_grep_text(result: &GrepResult) -> String {
    let mut out = String::new();
    let mut index = 1usize;
    for window in &result.windows {
        let last = index + window.matches - 1;
        let heading = if window.matches == 1 {
            format!("── match {} @ {} ──", index, window.offset)
        } else {
            format!("── matches {}–{} @ {} ──", index, last, window.offset)
        };
        out.push_str(&heading);
        out.push('\n');
        out.push_str(&window.text);
        out.push_str("\n\n");
        index = last + 1;
    }
    if result.truncated() {
        out.push_str(&format!(
            "{} matches, showing first {} (--max-matches to raise)\n",
            result.total_matches, result.shown_matches
        ));
    }
    out
}

/// Render matches as JSON, inside the same envelope a plain `read` uses.
fn render_grep_json(path: &Path, section: &str, pattern: &str, result: &GrepResult) -> String {
    let matches: Vec<serde_json::Value> = result
        .windows
        .iter()
        .map(|w| {
            serde_json::json!({
                "offset": w.offset,
                "matches": w.matches,
                "text": w.text,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "path": path.to_string_lossy(),
        "section": section,
        "pattern": pattern,
        "matches": matches,
        "total_matches": result.total_matches,
        "shown_matches": result.shown_matches,
        "truncated": result.truncated(),
    }))
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grep::{GrepResult, Window};

    fn one_window() -> GrepResult {
        GrepResult {
            windows: vec![Window {
                offset: 12043,
                matches: 1,
                text: "tal cost. Limitations. Our evaluation covers".to_string(),
            }],
            total_matches: 1,
            shown_matches: 1,
        }
    }

    fn cut_result() -> GrepResult {
        GrepResult {
            windows: vec![
                Window {
                    offset: 12043,
                    matches: 1,
                    text: "first".to_string(),
                },
                Window {
                    offset: 28871,
                    matches: 2,
                    text: "second".to_string(),
                },
            ],
            total_matches: 12,
            shown_matches: 3,
        }
    }

    #[test]
    fn text_rendering_heads_each_window_with_its_offset() {
        let out = render_grep_text(&one_window());
        assert!(out.contains("── match 1 @ 12043 ──"), "got: {}", out);
        assert!(out.contains("tal cost. Limitations."), "got: {}", out);
    }

    // A merged window covers a range of matches, so its heading has to say so
    // -- otherwise the numbering silently skips and looks like a bug.
    #[test]
    fn a_merged_window_is_headed_with_the_range_it_covers() {
        let out = render_grep_text(&cut_result());
        assert!(out.contains("── match 1 @ 12043 ──"), "got: {}", out);
        assert!(out.contains("── matches 2–3 @ 28871 ──"), "got: {}", out);
    }

    #[test]
    fn the_trailing_tally_appears_only_when_matches_were_cut() {
        let cut = render_grep_text(&cut_result());
        assert!(
            cut.contains("12 matches, showing first 3 (--max-matches to raise)"),
            "got: {}",
            cut
        );
        assert!(!render_grep_text(&one_window()).contains("--max-matches to raise"));
    }

    #[test]
    fn json_rendering_keeps_the_existing_envelope_and_adds_the_matches() {
        let json = render_grep_json(
            std::path::Path::new("x.pdf"),
            "full",
            "Limitations",
            &cut_result(),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["path"], "x.pdf");
        assert_eq!(v["section"], "full");
        assert_eq!(v["pattern"], "Limitations");
        assert_eq!(v["total_matches"], 12);
        assert_eq!(v["shown_matches"], 3);
        assert_eq!(v["truncated"], true);
        assert_eq!(v["matches"][0]["offset"], 12043);
        assert_eq!(v["matches"][1]["matches"], 2);
        assert_eq!(v["matches"][1]["text"], "second");
    }

    #[test]
    fn full_means_no_slicing() {
        assert_eq!(heading_for(Section::Full), None);
    }

    // Every --section value must map to a heading the extractor knows about,
    // otherwise the flag accepts a value it silently ignores.
    #[test]
    fn every_section_value_maps_to_a_known_heading() {
        for section in [
            Section::Abstract,
            Section::Introduction,
            Section::Methods,
            Section::Results,
            Section::Discussion,
            Section::Conclusion,
            Section::References,
        ] {
            let heading = heading_for(section).expect("should map to a heading");
            let paper = format!("Title\nAbstract\nx\n{}\ncontent here\n", heading);
            assert!(
                crate::read::extract_section(&paper, heading).is_some(),
                "{:?} maps to '{}' which the extractor does not find",
                section,
                heading
            );
        }
    }

    #[test]
    fn section_name_reports_full_for_whole_document() {
        assert_eq!(section_name(Section::Full), "full");
        assert_eq!(section_name(Section::Methods), "methods");
    }
}
