//! Find a pattern in a block of text and cut context windows around the hits.
//!
//! Separate from `read` on purpose: that module turns a PDF into text, this one
//! knows nothing about PDFs. Offsets and context are counted in **characters**,
//! never bytes -- `--max-length` already counts characters, and a byte offset
//! into a paper with non-ASCII text means nothing to the caller.

use regex::{Regex, RegexBuilder};

/// One contiguous slice of text covering one or more matches.
#[derive(Debug, PartialEq)]
pub struct Window {
    /// Character offset of the first match inside this window.
    pub offset: usize,
    /// How many matches this window covers.
    pub matches: usize,
    /// The excerpt, with up to `context` characters on each side.
    pub text: String,
}

#[derive(Debug, PartialEq)]
pub struct GrepResult {
    pub windows: Vec<Window>,
    /// Every match in the text, including the ones `max_matches` cut.
    pub total_matches: usize,
    /// How many matches the windows actually cover.
    pub shown_matches: usize,
}

impl GrepResult {
    pub fn truncated(&self) -> bool {
        self.shown_matches < self.total_matches
    }
}

/// Compile a pattern, case-insensitive unless the pattern says otherwise.
///
/// Case insensitivity is a builder flag rather than a lowercased haystack so
/// that `(?-i)` still works: `Limitations`, `LIMITATIONS` and `limitations` all
/// occur in real papers, but a caller who genuinely wants one of them can ask.
pub fn build_regex(pattern: &str) -> Result<Regex, String> {
    if pattern.is_empty() {
        return Err("Search pattern is empty.".to_string());
    }
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|e| format!("Invalid pattern '{}': {}", pattern, e))
}

/// Find `re` in `text` and cut a window of `context` characters around each of
/// the first `max_matches` hits, merging windows that overlap.
pub fn grep(text: &str, re: &Regex, context: usize, max_matches: usize) -> GrepResult {
    // Byte offset of every character, so a regex byte offset can be turned into
    // a character offset and back without ever slicing mid-character.
    let char_starts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let char_len = char_starts.len();
    let to_char = |byte: usize| char_starts.partition_point(|&start| start < byte);
    let slice = |from: usize, to: usize| -> &str {
        let start = char_starts.get(from).copied().unwrap_or(text.len());
        let end = char_starts.get(to).copied().unwrap_or(text.len());
        &text[start..end]
    };

    let hits: Vec<(usize, usize)> = re
        .find_iter(text)
        .map(|m| (to_char(m.start()), to_char(m.end())))
        .collect();
    let total_matches = hits.len();
    let shown = hits.len().min(max_matches);

    let mut windows: Vec<Window> = Vec::new();
    let mut bounds: Vec<(usize, usize)> = Vec::new();
    for &(hit_start, hit_end) in &hits[..shown] {
        let from = hit_start.saturating_sub(context);
        let to = (hit_end + context).min(char_len);
        match bounds.last_mut() {
            // Sorted by construction, so an overlap can only be with the last.
            Some(last) if from <= last.1 => {
                last.1 = last.1.max(to);
                let window = windows.last_mut().expect("bounds and windows stay in step");
                window.matches += 1;
            }
            _ => {
                bounds.push((from, to));
                windows.push(Window {
                    offset: hit_start,
                    matches: 1,
                    text: String::new(),
                });
            }
        }
    }
    for (window, (from, to)) in windows.iter_mut().zip(bounds) {
        window.text = slice(from, to).to_string();
    }

    GrepResult {
        windows,
        total_matches,
        shown_matches: shown,
    }
}

/// Trim the result so the windows together hold at most `max_length`
/// characters.
///
/// Windows are kept whole: the first one that would not fit is dropped along
/// with everything after it, so no excerpt is cut mid-sentence for the sake of
/// the budget. The one exception is a first window that is on its own too big,
/// which is cut instead of dropped -- `--max-length` should bound the output,
/// not erase a real hit.
pub fn apply_budget(result: &mut GrepResult, max_length: usize) {
    let mut spent = 0usize;
    let mut keep = 0usize;
    for window in &mut result.windows {
        let size = window.text.chars().count();
        if spent + size <= max_length {
            spent += size;
            keep += 1;
            continue;
        }
        if keep == 0 {
            window.text = window.text.chars().take(max_length).collect();
            keep = 1;
        }
        break;
    }
    result.windows.truncate(keep);
    result.shown_matches = result.windows.iter().map(|w| w.matches).sum();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn re(pattern: &str) -> Regex {
        build_regex(pattern).unwrap()
    }

    #[test]
    fn matching_is_case_insensitive_by_default() {
        let text = "LIMITATIONS here and limitations there";
        let result = grep(text, &re("Limitations"), 5, 10);
        assert_eq!(result.total_matches, 2);
    }

    // The default is a builder flag rather than lowercasing the haystack, so a
    // caller who does want case can say so inline instead of asking for a flag.
    #[test]
    fn an_inline_flag_restores_case_sensitivity() {
        let text = "LIMITATIONS here and limitations there";
        let result = grep(text, &re("(?-i)LIMITATIONS"), 5, 10);
        assert_eq!(result.total_matches, 1);
    }

    #[test]
    fn an_invalid_pattern_is_rejected_with_the_regex_diagnostic() {
        let err = build_regex("Limitations(").unwrap_err();
        assert!(
            err.contains("Limitations("),
            "should quote the pattern: {}",
            err
        );
    }

    #[test]
    fn an_empty_pattern_is_rejected() {
        let err = build_regex("").unwrap_err();
        assert!(err.contains("empty"), "got: {}", err);
    }

    #[test]
    fn offset_is_the_character_position_of_the_match() {
        //          0123456789
        let text = "0123456789needle";
        let result = grep(text, &re("needle"), 0, 10);
        assert_eq!(result.windows[0].offset, 10);
    }

    // A byte offset would be 12 here, not 10; slicing on it would also panic.
    #[test]
    fn offsets_and_slicing_count_characters_not_bytes() {
        let text = "日本語needle";
        let result = grep(text, &re("needle"), 2, 10);
        assert_eq!(result.windows[0].offset, 3);
        assert_eq!(result.windows[0].text, "本語needle");
    }

    #[test]
    fn context_is_taken_from_both_sides_and_clamped_at_the_edges() {
        let text = "abcdeNEEDLEfghij";
        let result = grep(text, &re("NEEDLE"), 3, 10);
        assert_eq!(result.windows[0].text, "cdeNEEDLEfgh");

        let result = grep(text, &re("NEEDLE"), 100, 10);
        assert_eq!(result.windows[0].text, text);
    }

    #[test]
    fn no_match_produces_no_windows() {
        let result = grep("nothing here", &re("needle"), 5, 10);
        assert_eq!(result.total_matches, 0);
        assert_eq!(result.shown_matches, 0);
        assert!(result.windows.is_empty());
        assert!(!result.truncated());
    }

    // Two hits close together would otherwise print the same sentence twice and
    // read as two independent pieces of evidence.
    #[test]
    fn overlapping_windows_merge_into_one() {
        let text = "aa NEEDLE bb NEEDLE cc";
        let result = grep(text, &re("NEEDLE"), 5, 10);
        assert_eq!(result.windows.len(), 1);
        assert_eq!(result.windows[0].matches, 2);
        assert_eq!(result.windows[0].offset, 3);
        assert_eq!(result.total_matches, 2);
    }

    #[test]
    fn distant_windows_stay_separate() {
        let text = format!("NEEDLE{}NEEDLE", " ".repeat(200));
        let result = grep(&text, &re("NEEDLE"), 5, 10);
        assert_eq!(result.windows.len(), 2);
        assert_eq!(result.windows[0].matches, 1);
        assert_eq!(result.windows[1].matches, 1);
    }

    #[test]
    fn max_matches_caps_the_windows_but_not_the_total() {
        let text = "x NEEDLE ".repeat(20);
        let result = grep(&text, &re("NEEDLE"), 0, 3);
        assert_eq!(result.total_matches, 20);
        assert_eq!(result.shown_matches, 3);
        assert_eq!(result.windows.len(), 3);
        assert!(result.truncated());
    }

    #[test]
    fn budget_keeps_whole_windows_and_drops_the_rest() {
        let text = format!("NEEDLE{}NEEDLE{}NEEDLE", " ".repeat(200), " ".repeat(200));
        let mut result = grep(&text, &re("NEEDLE"), 2, 10);
        assert_eq!(result.windows.len(), 3);
        // Each window is "NEEDLE" plus up to 2 characters of context.
        apply_budget(&mut result, 20);
        assert_eq!(result.windows.len(), 2);
        assert_eq!(result.shown_matches, 2);
        assert_eq!(result.total_matches, 3);
        assert!(result.truncated());
    }

    // A budget smaller than the first window still has to return something, or
    // --max-length silently turns a hit into no output at all.
    #[test]
    fn budget_smaller_than_the_first_window_cuts_it_rather_than_dropping_it() {
        let text = "aaaaaNEEDLEaaaaa";
        let mut result = grep(text, &re("NEEDLE"), 5, 10);
        apply_budget(&mut result, 4);
        assert_eq!(result.windows.len(), 1);
        assert_eq!(result.windows[0].text.chars().count(), 4);
    }

    #[test]
    fn a_budget_nothing_exceeds_changes_nothing() {
        let text = "aa NEEDLE bb";
        let mut result = grep(text, &re("NEEDLE"), 2, 10);
        let before = format!("{:?}", result);
        apply_budget(&mut result, 10_000);
        assert_eq!(format!("{:?}", result), before);
    }
}
