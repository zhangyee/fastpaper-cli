//! Find a pattern in a block of text and cut context windows around the hits.
//!
//! Separate from `read` on purpose: that module turns a PDF into text, this one
//! knows nothing about PDFs. Offsets and context are counted in **characters**,
//! never bytes -- `--max-length` already counts characters, and a byte offset
//! into a paper with non-ASCII text means nothing to the caller.

use regex::{Regex, RegexBuilder};

/// One match: the character offsets of its first and one-past-last character.
///
/// The end is carried, not just the start, so that a window cut to fit
/// `--max-length` can tell a match it still holds whole from one it sliced.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Hit {
    pub start: usize,
    pub end: usize,
}

/// One contiguous slice of text covering one or more matches.
#[derive(Debug, PartialEq)]
pub struct Window {
    /// Character offset where `text` begins in the searched text.
    pub start: usize,
    /// Every match this window still holds whole, in order.
    pub hits: Vec<Hit>,
    /// The excerpt, with up to `context` characters on each side.
    pub text: String,
}

impl Window {
    /// Character offset of the first match inside this window.
    pub fn offset(&self) -> usize {
        self.hits.first().map_or(self.start, |hit| hit.start)
    }

    /// How many matches this window covers.
    pub fn matches(&self) -> usize {
        self.hits.len()
    }

    /// Cut this window down to `max_length` characters *around its first
    /// match*, and report whether a whole match survived.
    ///
    /// Cutting from the head is what `--max-length 40` used to do, and with
    /// `--context 500` that threw away the match and kept 40 characters of
    /// lead-in: an excerpt containing none of what was searched for. So the
    /// context is shrunk symmetrically towards the match instead. Matches the
    /// cut pushes outside the excerpt stop being counted -- a match that is
    /// not in the output is not shown, whatever the tally used to say.
    fn cut_around_first_hit(&mut self, max_length: usize) -> bool {
        // No excerpt can hold a match, so there is nothing honest to keep.
        if max_length == 0 {
            self.hits.clear();
            return false;
        }
        let chars: Vec<char> = self.text.chars().collect();
        let Some(first) = self.hits.first().copied() else {
            return false;
        };
        // A budget smaller than the match itself can only show a fragment;
        // reporting that as a hit is the bug this method exists to close.
        let Some(slack) = max_length.checked_sub(first.end - first.start) else {
            self.hits.clear();
            return false;
        };
        let local = first.start - self.start;
        // Split what is left over after the match evenly between its two
        // sides, then slide the excerpt back inside the window if that ran
        // past the end. Both moves keep the match itself covered.
        let begin = local
            .saturating_sub(slack / 2)
            .min(chars.len().saturating_sub(max_length));
        let end = (begin + max_length).min(chars.len());

        let (kept_from, kept_to) = (self.start + begin, self.start + end);
        // Only matches the excerpt still holds whole are still shown matches.
        self.hits
            .retain(|hit| hit.start >= kept_from && hit.end <= kept_to);
        if self.hits.is_empty() {
            return false;
        }
        self.text = chars[begin..end].iter().collect();
        self.start = kept_from;
        true
    }
}

#[derive(Debug, PartialEq)]
pub struct GrepResult {
    pub windows: Vec<Window>,
    /// Every match in the text, including the ones `max_matches` cut.
    pub total_matches: usize,
    /// How many matches the windows actually cover.
    pub shown_matches: usize,
    /// Set when `max_matches` stopped the search short of `total_matches`.
    pub capped_by_max_matches: bool,
    /// Set when `max_length` dropped or trimmed an excerpt that would
    /// otherwise have been shown.
    pub capped_by_max_length: bool,
}

impl GrepResult {
    pub fn truncated(&self) -> bool {
        self.capped_by_max_matches || self.capped_by_max_length
    }

    /// The flags that actually bound this output, in the order they applied.
    ///
    /// Truncation used to be derived from `shown < total`, so the only advice
    /// on offer was "--max-matches to raise" -- useless when `--max-length` was
    /// what cut, since raising `--max-matches` then changes nothing at all.
    pub fn limits_hit(&self) -> Vec<&'static str> {
        let mut flags = Vec::new();
        if self.capped_by_max_matches {
            flags.push("--max-matches");
        }
        if self.capped_by_max_length {
            flags.push("--max-length");
        }
        flags
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

    let hits: Vec<Hit> = re
        .find_iter(text)
        .map(|m| Hit {
            start: to_char(m.start()),
            end: to_char(m.end()),
        })
        .collect();
    let total_matches = hits.len();
    let shown = hits.len().min(max_matches);

    let mut windows: Vec<Window> = Vec::new();
    let mut bounds: Vec<(usize, usize)> = Vec::new();
    for &hit in &hits[..shown] {
        let from = hit.start.saturating_sub(context);
        let to = (hit.end + context).min(char_len);
        match bounds.last_mut() {
            // Sorted by construction, so an overlap can only be with the last.
            Some(last) if from <= last.1 => {
                last.1 = last.1.max(to);
                let window = windows.last_mut().expect("bounds and windows stay in step");
                window.hits.push(hit);
            }
            _ => {
                bounds.push((from, to));
                windows.push(Window {
                    start: from,
                    hits: vec![hit],
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
        capped_by_max_matches: shown < total_matches,
        capped_by_max_length: false,
    }
}

/// Trim the result so the windows together hold at most `max_length`
/// characters.
///
/// Windows are kept whole: the first one that would not fit is dropped along
/// with everything after it, so no excerpt is cut mid-sentence for the sake of
/// the budget. The one exception is a first window that is on its own too big,
/// which is cut around its match instead of dropped -- `--max-length` should
/// bound the output, not erase a real hit.
///
/// Whatever the budget does, the result reports it: `shown_matches` counts only
/// the matches still in an excerpt, and `capped_by_max_length` records that
/// `--max-length`, not `--max-matches`, is the flag to raise.
pub fn apply_budget(result: &mut GrepResult, max_length: usize) {
    let mut spent = 0usize;
    let mut keep = 0usize;
    let mut cut = false;
    for window in &mut result.windows {
        let size = window.text.chars().count();
        if spent + size <= max_length {
            spent += size;
            keep += 1;
            continue;
        }
        cut = true;
        if keep == 0 && window.cut_around_first_hit(max_length) {
            keep = 1;
        }
        break;
    }
    result.windows.truncate(keep);
    result.shown_matches = result.windows.iter().map(Window::matches).sum();
    result.capped_by_max_length = cut;
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
        assert_eq!(result.windows[0].offset(), 10);
    }

    // A byte offset would be 12 here, not 10; slicing on it would also panic.
    #[test]
    fn offsets_and_slicing_count_characters_not_bytes() {
        let text = "日本語needle";
        let result = grep(text, &re("needle"), 2, 10);
        assert_eq!(result.windows[0].offset(), 3);
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
        assert_eq!(result.windows[0].matches(), 2);
        assert_eq!(result.windows[0].offset(), 3);
        assert_eq!(result.total_matches, 2);
    }

    #[test]
    fn distant_windows_stay_separate() {
        let text = format!("NEEDLE{}NEEDLE", " ".repeat(200));
        let result = grep(&text, &re("NEEDLE"), 5, 10);
        assert_eq!(result.windows.len(), 2);
        assert_eq!(result.windows[0].matches(), 1);
        assert_eq!(result.windows[1].matches(), 1);
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
        apply_budget(&mut result, 8);
        assert_eq!(result.windows.len(), 1);
        assert_eq!(result.windows[0].text.chars().count(), 8);
    }

    // The bug this test exists for: the cut used to start at the window head,
    // so with `context` characters of lead-in the match itself was the first
    // thing sliced away -- an excerpt containing none of what was searched for.
    #[test]
    fn a_cut_window_still_contains_the_match() {
        let text = format!("{}NEEDLE{}", "a".repeat(500), "b".repeat(500));
        let mut result = grep(&text, &re("NEEDLE"), 500, 10);
        apply_budget(&mut result, 40);
        assert_eq!(result.windows.len(), 1);
        assert!(
            result.windows[0].text.contains("NEEDLE"),
            "excerpt lost the match: {:?}",
            result.windows[0].text
        );
        assert_eq!(result.windows[0].text.chars().count(), 40);
        // The offset still points at the match, and the excerpt now starts
        // where `start` says it does.
        assert_eq!(result.windows[0].offset(), 500);
        assert_eq!(
            result.windows[0].start,
            500 - (40 - "NEEDLE".chars().count()) / 2
        );
    }

    // A budget that cannot hold the match itself has nothing honest to show,
    // so it shows nothing rather than a fragment counted as a hit.
    #[test]
    fn a_budget_too_small_for_the_match_reports_nothing_shown() {
        let text = "aaaaaNEEDLEaaaaa";
        let mut result = grep(text, &re("NEEDLE"), 5, 10);
        apply_budget(&mut result, 4);
        assert!(result.windows.is_empty());
        assert_eq!(result.shown_matches, 0);
        assert_eq!(result.total_matches, 1);
        assert!(result.truncated());
    }

    // `--max-length 0` used to print an empty body under a "1 match" receipt.
    #[test]
    fn a_zero_budget_shows_no_matches_at_all() {
        let text = "aa NEEDLE bb";
        let mut result = grep(text, &re("NEEDLE"), 2, 10);
        apply_budget(&mut result, 0);
        assert!(result.windows.is_empty());
        assert_eq!(result.shown_matches, 0);
        assert!(result.truncated());
    }

    // A merged window carries several matches; cutting it drops the ones that
    // fall outside the surviving excerpt, so the count has to follow.
    #[test]
    fn cutting_a_merged_window_stops_counting_the_matches_it_lost() {
        let text = "NEEDLE aaaaaaaaaaaaaaaaaaaa NEEDLE";
        let mut result = grep(text, &re("NEEDLE"), 20, 10);
        assert_eq!(result.windows.len(), 1);
        assert_eq!(result.windows[0].matches(), 2);
        apply_budget(&mut result, 10);
        assert_eq!(result.shown_matches, 1);
        assert_eq!(result.total_matches, 2);
        assert!(result.truncated());
    }

    // Truncation used to be derived from `shown < total`, which said nothing
    // about which flag to raise -- and stayed false after a budget cut.
    #[test]
    fn truncation_names_the_flag_that_actually_bound() {
        let text = "x NEEDLE ".repeat(20);
        let capped = grep(&text, &re("NEEDLE"), 0, 3);
        assert_eq!(capped.limits_hit(), vec!["--max-matches"]);

        let text = format!("{}NEEDLE{}", "a".repeat(500), "b".repeat(500));
        let mut budgeted = grep(&text, &re("NEEDLE"), 500, 10);
        apply_budget(&mut budgeted, 40);
        assert_eq!(budgeted.limits_hit(), vec!["--max-length"]);
        assert!(budgeted.truncated());

        let text = "x NEEDLE ".repeat(20);
        let mut both = grep(&text, &re("NEEDLE"), 3, 3);
        apply_budget(&mut both, 12);
        assert_eq!(both.limits_hit(), vec!["--max-matches", "--max-length"]);
    }

    #[test]
    fn an_uncut_result_names_no_limit() {
        let mut result = grep("aa NEEDLE bb", &re("NEEDLE"), 2, 10);
        apply_budget(&mut result, 10_000);
        assert!(result.limits_hit().is_empty());
        assert!(!result.truncated());
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
