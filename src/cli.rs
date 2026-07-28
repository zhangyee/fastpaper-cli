use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::registry::{self, Source};
use crate::sources::{SearchQuery, SortField, SortOrder};

/// Fast academic paper search, download & read
#[derive(Parser)]
#[command(name = "fastpaper", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[command(flatten)]
    pub global: GlobalOpts,
}

#[derive(clap::Args)]
pub struct GlobalOpts {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress non-essential output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Output format
    #[arg(short, long, global = true, default_value = "table")]
    pub format: OutputFormat,
}

/// Each command does exactly one thing:
/// `search` finds papers, `get` reads metadata, `download` saves a PDF,
/// `read` extracts text from a PDF already on disk.
#[derive(Subcommand)]
pub enum Commands {
    /// Search papers in one academic source
    Search(SearchArgs),

    /// Fetch metadata for a single paper by identifier
    Get(GetArgs),

    /// Download a paper's PDF (default: ./papers)
    Download(DownloadArgs),

    /// Extract text from a local PDF file
    Read(ReadArgs),

    /// List available sources and what each can do
    Sources(SourcesArgs),

    /// Generate shell completions
    Completions { shell: clap_complete::Shell },
}

// ── search ──────────────────────────────────────

#[derive(clap::Args)]
pub struct SearchArgs {
    /// Academic source to search
    pub source: Source,

    /// Search query string
    pub query: String,

    /// Max results
    #[arg(short = 'n', long, default_value = "10")]
    pub limit: u32,

    /// Skip the first N results
    #[arg(long, default_value = "0")]
    pub offset: u32,

    /// Sort by field
    #[arg(long)]
    pub sort: Option<SortField>,

    /// Sort direction
    #[arg(long, default_value = "desc")]
    pub order: SortOrder,

    /// Papers in a specific year
    #[arg(long)]
    pub year: Option<u16>,

    /// Papers published on or after this date (YYYY-MM-DD)
    #[arg(long)]
    pub after: Option<String>,

    /// Papers published on or before this date (YYYY-MM-DD)
    #[arg(long)]
    pub before: Option<String>,

    /// Filter by author
    #[arg(long)]
    pub author: Option<String>,

    /// Field of study / category
    #[arg(long)]
    pub field: Option<String>,

    /// Only open access papers
    #[arg(long)]
    pub open_access: bool,

    /// Return patents only (europepmc, xueshu)
    #[arg(long)]
    pub patents: bool,

    /// Write results to a file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

impl SearchArgs {
    pub fn to_query(&self) -> SearchQuery {
        SearchQuery {
            query: self.query.clone(),
            limit: self.limit,
            offset: self.offset,
            sort: self.sort,
            order: self.order,
            year: self.year,
            after: self.after.clone(),
            before: self.before.clone(),
            author: self.author.clone(),
            field: self.field.clone(),
            open_access: self.open_access,
            patents: self.patents,
        }
    }
}

// ── get ─────────────────────────────────────────

#[derive(clap::Args)]
pub struct GetArgs {
    /// Source name, or the identifier itself when no source is given
    #[arg(value_name = "SOURCE_OR_ID")]
    pub first: String,

    /// Identifier (DOI, arXiv ID, PMID, PMC ID, ...) when a source is given
    #[arg(value_name = "ID")]
    pub second: Option<String>,
}

impl GetArgs {
    pub fn resolve(&self) -> Result<(Option<Source>, &str), String> {
        resolve_source_and_id(&self.first, self.second.as_deref())
    }
}

// ── download ────────────────────────────────────

#[derive(clap::Args)]
pub struct DownloadArgs {
    /// Source name, or the identifier itself when no source is given
    #[arg(value_name = "SOURCE_OR_ID")]
    pub first: String,

    /// Identifier when a source is given
    #[arg(value_name = "ID")]
    pub second: Option<String>,

    /// Directory to save into
    #[arg(short, long, env = "FASTPAPER_DOWNLOAD_DIR", default_value = "./papers")]
    pub dir: PathBuf,

    /// Overwrite an existing file
    #[arg(long)]
    pub overwrite: bool,
}

impl DownloadArgs {
    pub fn resolve(&self) -> Result<(Option<Source>, &str), String> {
        resolve_source_and_id(&self.first, self.second.as_deref())
    }
}

/// Disambiguate the `[source] <id>` forms.
///
/// The first positional cannot be typed as `Source`: clap would reject
/// `get 10.1038/nature12373` during parsing, before we ever get to look at it.
/// So both arrive as strings and the rule is positional — one argument is an
/// identifier, two means the first names a source.
fn resolve_source_and_id<'a>(
    first: &'a str,
    second: Option<&'a str>,
) -> Result<(Option<Source>, &'a str), String> {
    match second {
        None => Ok((None, first)),
        Some(id) => match Source::from_name(first) {
            Some(source) => Ok((Some(source), id)),
            None => Err(format!(
                "'{}' is not a known source.\nValid sources: {}",
                first,
                registry::ALL
                    .iter()
                    .map(|s| s.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        },
    }
}

// ── read ────────────────────────────────────────

#[derive(clap::Args)]
pub struct ReadArgs {
    /// Path to a local PDF file
    pub path: PathBuf,

    /// Extract a specific section
    #[arg(long, default_value = "full")]
    pub section: Section,

    /// Truncate output to N characters
    #[arg(long)]
    pub max_length: Option<usize>,

    /// Write content to a file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

// ── sources ─────────────────────────────────────

#[derive(clap::Args)]
pub struct SourcesArgs {
    /// Show per-source search filters and caveats
    #[arg(long)]
    pub capabilities: bool,
}

// ── enums ───────────────────────────────────────

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum OutputFormat {
    Table,
    Json,
    Jsonl,
    Csv,
    Bibtex,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum Section {
    Abstract,
    Introduction,
    Methods,
    Results,
    Discussion,
    Conclusion,
    References,
    Full,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_argument_is_an_identifier() {
        let (source, id) = resolve_source_and_id("10.1038/nature12373", None).unwrap();
        assert_eq!(source, None);
        assert_eq!(id, "10.1038/nature12373");
    }

    #[test]
    fn two_arguments_name_a_source() {
        let (source, id) = resolve_source_and_id("arxiv", Some("2301.08745")).unwrap();
        assert_eq!(source, Some(Source::Arxiv));
        assert_eq!(id, "2301.08745");
    }

    #[test]
    fn unknown_source_name_is_rejected_with_the_valid_list() {
        let err = resolve_source_and_id("arxvi", Some("2301.08745")).unwrap_err();
        assert!(err.contains("arxvi"), "should quote the bad token: {}", err);
        assert!(err.contains("arxiv"), "should list valid sources: {}", err);
    }

    // A lone identifier that happens to look like nothing is still an
    // identifier -- source detection happens later, in the get command.
    #[test]
    fn one_argument_is_never_treated_as_a_source() {
        let (source, id) = resolve_source_and_id("arxiv", None).unwrap();
        assert_eq!(source, None);
        assert_eq!(id, "arxiv");
    }
}
