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
        let last = index + window.matches() - 1;
        let heading = if window.matches() == 1 {
            format!("── match {} @ {} ──", index, window.offset())
        } else {
            format!("── matches {}–{} @ {} ──", index, last, window.offset())
        };
        out.push_str(&heading);
        out.push('\n');
        out.push_str(&window.text);
        out.push_str("\n\n");
        index = last + 1;
    }
    if let Some(notice) = truncation_notice(result) {
        out.push_str(&notice);
        out.push('\n');
    }
    out
}

/// Say what was left out, and which flag would bring it back.
///
/// The old line always said `--max-matches to raise`, even when `--max-length`
/// was what cut -- advice that changes nothing when followed, which is worse
/// than none. So the flags that actually bound are the ones named.
fn truncation_notice(result: &GrepResult) -> Option<String> {
    let flags = result.limits_hit();
    if flags.is_empty() {
        return None;
    }
    let raise = format!("{} to raise", flags.join(" and "));
    if result.shown_matches < result.total_matches {
        Some(format!(
            "{} matches, showing first {} ({})",
            result.total_matches, result.shown_matches, raise
        ))
    } else {
        // Every match is here, but its excerpt was trimmed to fit the budget.
        Some(format!("excerpt cut to fit --max-length ({})", raise))
    }
}

/// Render matches as JSON, inside the same envelope a plain `read` uses.
fn render_grep_json(path: &Path, section: &str, pattern: &str, result: &GrepResult) -> String {
    let matches: Vec<serde_json::Value> = result
        .windows
        .iter()
        .map(|w| {
            serde_json::json!({
                "offset": w.offset(),
                "start": w.start,
                "matches": w.matches(),
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
        // Which flag to raise. `truncated` alone left a JSON consumer unable
        // to tell a `--max-matches` cut from a `--max-length` one.
        "truncated_by": result.limits_hit(),
    }))
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grep::{GrepResult, Hit, Window};

    fn hits(offsets: &[usize]) -> Vec<Hit> {
        offsets
            .iter()
            .map(|&start| Hit {
                start,
                end: start + 11,
            })
            .collect()
    }

    fn one_window() -> GrepResult {
        GrepResult {
            windows: vec![Window {
                start: 12033,
                hits: hits(&[12043]),
                text: "tal cost. Limitations. Our evaluation covers".to_string(),
            }],
            total_matches: 1,
            shown_matches: 1,
            capped_by_max_matches: false,
            capped_by_max_length: false,
        }
    }

    fn cut_result() -> GrepResult {
        GrepResult {
            windows: vec![
                Window {
                    start: 12043,
                    hits: hits(&[12043]),
                    text: "first".to_string(),
                },
                Window {
                    start: 28871,
                    hits: hits(&[28871, 28900]),
                    text: "second".to_string(),
                },
            ],
            total_matches: 12,
            shown_matches: 3,
            capped_by_max_matches: true,
            capped_by_max_length: false,
        }
    }

    /// A result whose matches all fit but whose excerpt was trimmed.
    fn budget_cut_result() -> GrepResult {
        GrepResult {
            windows: vec![Window {
                start: 12043,
                hits: hits(&[12043]),
                text: "Limitations".to_string(),
            }],
            total_matches: 1,
            shown_matches: 1,
            capped_by_max_matches: false,
            capped_by_max_length: true,
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

    // Reported: `--max-length 15` printed "showing first 1 (--max-matches to
    // raise)" with --max-matches at 10 and only 2 matches in the file, so
    // following the advice produced byte-identical output.
    #[test]
    fn the_tally_names_the_flag_that_did_the_cutting() {
        let mut budgeted = cut_result();
        budgeted.capped_by_max_matches = false;
        budgeted.capped_by_max_length = true;
        let out = render_grep_text(&budgeted);
        assert!(
            out.contains("12 matches, showing first 3 (--max-length to raise)"),
            "got: {}",
            out
        );
        assert!(!out.contains("--max-matches"), "got: {}", out);
    }

    #[test]
    fn both_flags_are_named_when_both_bound() {
        let mut both = cut_result();
        both.capped_by_max_length = true;
        let out = render_grep_text(&both);
        assert!(
            out.contains("(--max-matches and --max-length to raise)"),
            "got: {}",
            out
        );
    }

    // Every match survived, but its excerpt did not: saying nothing here would
    // present a trimmed window as the full context that was asked for.
    #[test]
    fn a_trimmed_excerpt_says_so_even_when_no_match_was_lost() {
        let out = render_grep_text(&budget_cut_result());
        assert!(
            out.contains("excerpt cut to fit --max-length"),
            "got: {}",
            out
        );
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

    // A JSON consumer that only sees `truncated: true` cannot tell which flag
    // to raise, and raising the wrong one changes nothing.
    #[test]
    fn json_names_which_limit_did_the_cutting() {
        let capped = render_grep_json(Path::new("x.pdf"), "full", "p", &cut_result());
        let v: serde_json::Value = serde_json::from_str(&capped).unwrap();
        assert_eq!(v["truncated_by"], serde_json::json!(["--max-matches"]));

        let budgeted = render_grep_json(Path::new("x.pdf"), "full", "p", &budget_cut_result());
        let v: serde_json::Value = serde_json::from_str(&budgeted).unwrap();
        assert_eq!(v["truncated"], true);
        assert_eq!(v["truncated_by"], serde_json::json!(["--max-length"]));

        let whole = render_grep_json(Path::new("x.pdf"), "full", "p", &one_window());
        let v: serde_json::Value = serde_json::from_str(&whole).unwrap();
        assert_eq!(v["truncated"], false);
        assert_eq!(v["truncated_by"], serde_json::json!([]));
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
