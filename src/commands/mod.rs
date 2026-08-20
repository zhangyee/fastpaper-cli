//! One module per command. Each owns its argument validation, source calls and
//! rendering; `main.rs` only dispatches and maps errors to exit codes.

pub mod cite;
pub mod download;
pub mod figures;
pub mod get;
pub mod read;
pub mod search;
pub mod sources;

use std::path::Path;

use crate::cli::OutputFormat;
use crate::output;
use crate::sources::Paper;

/// What went wrong, and how the process should exit.
#[derive(Debug)]
pub enum CommandError {
    /// Exit 1: bad arguments, network failure, parse failure.
    Failed(String),
    /// Exit 4: the request was well-formed but matched nothing.
    NotFound(String),
    /// Exit 0: the file was already there, which is not a failure.
    AlreadyExists(String),
}

impl CommandError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CommandError::Failed(_) => 1,
            CommandError::NotFound(_) => 4,
            CommandError::AlreadyExists(_) => 0,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            CommandError::Failed(m)
            | CommandError::NotFound(m)
            | CommandError::AlreadyExists(m) => m,
        }
    }
}

pub type CommandResult = Result<(), CommandError>;

pub fn failed(message: impl Into<String>) -> CommandError {
    CommandError::Failed(message.into())
}

/// Render papers in the requested format.
pub fn render(papers: &[Paper], format: OutputFormat) -> String {
    match format {
        OutputFormat::Json => output::to_json(papers),
        OutputFormat::Jsonl => output::to_jsonl(papers),
        OutputFormat::Csv => output::to_csv(papers),
        OutputFormat::Bibtex => output::to_bibtex(papers),
        OutputFormat::Table => output::to_table(papers),
    }
}

/// How many papers were written, for the `-o` receipt.
pub fn results_summary(n: usize) -> String {
    format!("{} {}", n, if n == 1 { "result" } else { "results" })
}

/// How much text was written, for the `-o` receipt.
pub fn chars_summary(n: usize) -> String {
    format!("{} {}", n, if n == 1 { "char" } else { "chars" })
}

/// How many matches were written, for the `-o` receipt. Names the total only
/// when the output was cut, so `3 matches` never has to be read as a subset.
pub fn matches_summary(shown: usize, total: usize) -> String {
    let noun = if total == 1 { "match" } else { "matches" };
    if shown < total {
        format!("{} of {} {}", shown, total, noun)
    } else {
        format!("{} {}", total, noun)
    }
}

/// Write to a file when `-o` was given, otherwise to stdout.
///
/// A write to a file leaves stdout empty, so without a receipt the caller has
/// no way to tell 12 results from 0 short of reading the file back. The line
/// goes to stderr because that is where `download` already puts its own
/// `Saved:` line, and because stdout is for what the program was asked to
/// produce -- "I wrote a file" is not that.
pub fn emit(text: &str, output_path: Option<&Path>, summary: &str, quiet: bool) -> CommandResult {
    match output_path {
        Some(path) => {
            std::fs::write(path, text)
                .map_err(|e| failed(format!("Failed to write {}: {}", path.display(), e)))?;
            if !quiet {
                eprintln!("Saved: {} ({})", path.display(), summary);
            }
            Ok(())
        }
        None => {
            print!("{}", text);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_summary_uses_the_plural_that_matches_the_count() {
        assert_eq!(results_summary(0), "0 results");
        assert_eq!(results_summary(1), "1 result");
        assert_eq!(results_summary(12), "12 results");
    }

    #[test]
    fn chars_summary_counts_characters() {
        assert_eq!(chars_summary(0), "0 chars");
        assert_eq!(chars_summary(1), "1 char");
        assert_eq!(chars_summary(18432), "18432 chars");
    }

    // Untruncated output says how many there were; a truncated one has to say
    // both numbers, or the caller cannot tell it is looking at a subset.
    #[test]
    fn matches_summary_names_the_total_only_when_it_was_cut() {
        assert_eq!(matches_summary(3, 3), "3 matches");
        assert_eq!(matches_summary(1, 1), "1 match");
        assert_eq!(matches_summary(10, 12), "10 of 12 matches");
    }
}
