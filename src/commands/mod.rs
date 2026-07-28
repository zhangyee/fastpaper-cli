//! One module per command. Each owns its argument validation, source calls and
//! rendering; `main.rs` only dispatches and maps errors to exit codes.

pub mod cite;
pub mod download;
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
            CommandError::Failed(m) | CommandError::NotFound(m) | CommandError::AlreadyExists(m) => m,
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

/// Write to a file when `-o` was given, otherwise to stdout.
pub fn emit(text: &str, output_path: Option<&Path>) -> CommandResult {
    match output_path {
        Some(path) => std::fs::write(path, text)
            .map_err(|e| failed(format!("Failed to write {}: {}", path.display(), e))),
        None => {
            print!("{}", text);
            Ok(())
        }
    }
}
