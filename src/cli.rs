use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

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

#[derive(Subcommand)]
pub enum Commands {
    /// Search papers from a single academic source
    Search(SearchArgs),

    /// Download paper PDF/source files
    Download(DownloadArgs),

    /// Extract and display paper content
    Read(ReadArgs),

    /// Fetch a single paper by identifier (auto-detect source)
    Get(GetArgs),

    /// List available sources and capabilities
    Sources(SourcesArgs),

    /// Export agent skill for AI assistants
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },

    /// Generate shell completions
    Completions {
        shell: clap_complete::Shell,
    },
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

    /// Skip first N results
    #[arg(long, default_value = "0")]
    pub offset: u32,

    /// Sort by field
    #[arg(long, default_value = "relevance")]
    pub sort: SortField,

    /// Sort direction
    #[arg(long, default_value = "desc")]
    pub order: SortOrder,

    /// Filter by author
    #[arg(long)]
    pub author: Option<String>,

    /// Papers after date (YYYY-MM-DD)
    #[arg(long)]
    pub after: Option<String>,

    /// Papers before date (YYYY-MM-DD)
    #[arg(long)]
    pub before: Option<String>,

    /// Papers in specific year
    #[arg(long)]
    pub year: Option<u16>,

    /// Field of study / category
    #[arg(long)]
    pub field: Option<String>,

    /// Only open access papers
    #[arg(long)]
    pub open_access: bool,

    /// Only peer-reviewed papers
    #[arg(long)]
    pub peer_reviewed: bool,

    /// Comma-separated fields to include in output
    #[arg(long)]
    pub fields: Option<String>,

    /// Include abstract in table output
    #[arg(long)]
    pub with_abstract: bool,

    /// Write results to file
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

// ── download ────────────────────────────────────

#[derive(clap::Args)]
pub struct DownloadArgs {
    /// Source to download from
    pub source: Source,

    /// Paper identifier
    pub identifier: String,

    /// Download directory
    #[arg(short, long, env = "FASTPAPER_DOWNLOAD_DIR", default_value = "./papers")]
    pub dir: PathBuf,

    /// Filename template: {id}, {title}, {authors}, {year}, {doi}
    #[arg(long, default_value = "{id}.{title}")]
    pub filename: String,

    /// Overwrite existing files
    #[arg(long)]
    pub overwrite: bool,

    /// Download source/LaTeX instead of PDF (arXiv only)
    #[arg(long)]
    pub source_files: bool,
}

// ── read ────────────────────────────────────────

#[derive(clap::Args)]
pub struct ReadArgs {
    /// Source to read from (use "local" for local files)
    pub source: Source,

    /// Paper identifier or file path
    pub identifier: String,

    /// Extract specific section
    #[arg(long, default_value = "full")]
    pub section: Section,

    /// Only show metadata
    #[arg(long)]
    pub metadata_only: bool,

    /// Raw text without formatting
    #[arg(long)]
    pub raw: bool,

    /// Truncate output to N characters
    #[arg(long)]
    pub max_length: Option<usize>,

    /// Write content to file
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

// ── get ─────────────────────────────────────────

#[derive(clap::Args)]
pub struct GetArgs {
    /// DOI, arXiv ID, PMID, PMC ID, URL, etc.
    pub identifier: String,

    /// Resolve all available OA versions
    #[arg(long)]
    pub resolve: bool,

    /// Include citation count and references
    #[arg(long)]
    pub with_citations: bool,

    /// Include abstract
    #[arg(long)]
    pub with_abstract: bool,

    /// Include related/recommended papers
    #[arg(long)]
    pub with_related: bool,
}

// ── sources ─────────────────────────────────────

#[derive(clap::Args)]
pub struct SourcesArgs {
    /// Test connectivity to each source
    #[arg(long)]
    pub check: bool,

    /// Show detailed capability matrix
    #[arg(long)]
    pub capabilities: bool,
}

// ── skill ───────────────────────────────────────

#[derive(Subcommand)]
pub enum SkillAction {
    /// Export SKILL.md to stdout
    Export {
        #[arg(long)]
        agent: Option<AgentTarget>,
    },
    /// Install skill to agent's skill directory
    Install {
        #[arg(long)]
        agent: Option<AgentTarget>,
    },
    /// Print bundled SKILL.md content
    Show,
}

// ── enums ───────────────────────────────────────

#[derive(ValueEnum, Clone, Debug)]
pub enum Source {
    Arxiv,
    Biorxiv,
    Medrxiv,
    Pubmed,
    Pmc,
    Europepmc,
    Scholar,
    Semantic,
    Crossref,
    Openalex,
    Dblp,
    Core,
    Openaire,
    Doaj,
    Unpaywall,
    Zenodo,
    Hal,
    Local,
}

impl Source {
    pub fn supports_search(&self) -> bool {
        !matches!(self, Source::Local)
    }

    pub fn supports_download(&self) -> bool {
        matches!(
            self,
            Source::Arxiv
                | Source::Biorxiv
                | Source::Medrxiv
                | Source::Pmc
                | Source::Europepmc
                | Source::Semantic
                | Source::Core
                | Source::Doaj
                | Source::Zenodo
                | Source::Hal
                | Source::Local
        )
    }

    pub fn supports_read(&self) -> bool {
        self.supports_download()
    }

    pub fn name(&self) -> &'static str {
        match self {
            Source::Arxiv => "arxiv",
            Source::Biorxiv => "biorxiv",
            Source::Medrxiv => "medrxiv",
            Source::Pubmed => "pubmed",
            Source::Pmc => "pmc",
            Source::Europepmc => "europepmc",
            Source::Scholar => "scholar",
            Source::Semantic => "semantic",
            Source::Crossref => "crossref",
            Source::Openalex => "openalex",
            Source::Dblp => "dblp",
            Source::Core => "core",
            Source::Openaire => "openaire",
            Source::Doaj => "doaj",
            Source::Unpaywall => "unpaywall",
            Source::Zenodo => "zenodo",
            Source::Hal => "hal",
            Source::Local => "local",
        }
    }

    pub fn download_hint(&self) -> Option<&'static str> {
        match self {
            Source::Pubmed => Some("Try: fastpaper download pmc <PMC_ID>"),
            Source::Scholar => Some("Google Scholar does not provide PDFs directly"),
            Source::Crossref | Source::Openalex | Source::Dblp => {
                Some("This source only provides metadata. Try: fastpaper get <ID> --resolve")
            }
            Source::Openaire | Source::Unpaywall => {
                Some("This source only provides metadata. Try: fastpaper get <ID> --resolve")
            }
            _ => None,
        }
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Table,
    Json,
    Jsonl,
    Csv,
    Bibtex,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum SortField {
    Relevance,
    Date,
    Citations,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(ValueEnum, Clone, Debug)]
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

#[derive(ValueEnum, Clone, Debug)]
pub enum AgentTarget {
    Claude,
    Codex,
    Cursor,
    Gemini,
}
