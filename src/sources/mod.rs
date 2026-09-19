pub mod arxiv;
pub mod core;
pub mod crossref;
pub mod datacite;
pub mod dblp;
pub mod doaj;
pub mod eric;
pub mod europepmc;
pub mod hal;
pub mod huggingface;
pub mod inspire;
pub mod ntrs;
pub mod openaire;
pub mod openreview;
pub mod openalex;
pub mod osf;
pub mod osti;
pub mod pmc;
pub mod pubmed;
pub mod semantic;
pub mod unpaywall;
pub mod zbmath;
pub mod zenodo;

use serde::Serialize;

/// Percent-encode a query string for use in URLs.
pub fn encode_query(query: &str) -> String {
    let mut encoded = String::with_capacity(query.len() * 3);
    for byte in query.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push('%');
                encoded.push_str(&format!("{:02X}", byte));
            }
        }
    }
    encoded
}

/// Field to sort search results by.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum SortField {
    Relevance,
    Date,
    Citations,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum SortOrder {
    Asc,
    Desc,
}

/// A ranked list a source publishes on its own, as opposed to results matched
/// against a query. Only huggingface has these today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    /// The source's rolling "hot right now" order, which ignores dates.
    Trending,
    /// Papers listed on one day, `YYYY-MM-DD`.
    Day(String),
    /// Papers listed in one ISO week, `YYYY-Www`.
    Week(String),
    /// Papers listed in one month, `YYYY-MM`.
    Month(String),
}

impl Listing {
    /// Read a `--top` period; its shape says which kind it is.
    pub fn parse_period(raw: &str) -> Result<Listing, String> {
        let raw = raw.trim();
        let digits = |s: &str, n: usize| s.len() == n && s.chars().all(|c| c.is_ascii_digit());
        let in_range = |s: &str, max: u32| s.parse::<u32>().is_ok_and(|v| (1..=max).contains(&v));

        if validate_ymd(raw).is_ok() {
            return Ok(Listing::Day(raw.to_string()));
        }
        if let Some((year, week)) = raw.split_once("-W")
            && digits(year, 4)
            && digits(week, 2)
            && in_range(week, 53)
        {
            return Ok(Listing::Week(raw.to_string()));
        }
        if let Some((year, month)) = raw.split_once('-')
            && digits(year, 4)
            && digits(month, 2)
            && in_range(month, 12)
        {
            return Ok(Listing::Month(raw.to_string()));
        }
        Err(format!(
            "'{}' is not a period. Use a day (2026-09-18), an ISO week (2026-W38) or a month (2026-08).",
            raw
        ))
    }

    /// The CLI flag that asks for this listing.
    pub fn flag(&self) -> &'static str {
        match self {
            Listing::Trending => "--trending",
            _ => "--top",
        }
    }
}

/// A normalized search request.
///
/// Each source maps the fields it supports onto its own API parameters. The
/// command layer consults `Capabilities` first and rejects any field the source
/// cannot honour, so a filter is never silently dropped.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub query: String,
    pub limit: u32,
    pub offset: u32,
    pub sort: Option<SortField>,
    pub order: SortOrder,
    pub year: Option<u16>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub author: Option<String>,
    pub field: Option<String>,
    pub open_access: bool,
    /// Return patents only. Sources that can honour it filter to the patent
    /// subset; without it they exclude patents. Results are never mixed, so a
    /// caller always knows which it asked for.
    pub patents: bool,
    /// A published ranked list instead of a keyword search (`--trending`,
    /// `--top`). The query is empty when this is set.
    pub listing: Option<Listing>,
}

impl SearchQuery {
    /// A query with no filters — what every source could already do.
    pub fn simple(query: &str, limit: u32) -> Self {
        SearchQuery {
            query: query.to_string(),
            limit,
            offset: 0,
            sort: None,
            order: SortOrder::Desc,
            year: None,
            after: None,
            before: None,
            author: None,
            field: None,
            open_access: false,
            patents: false,
            listing: None,
        }
    }

    /// Names of the filters this query actually sets, in CLI-flag form.
    pub fn active_filters(&self) -> Vec<&'static str> {
        let mut used = Vec::new();
        if self.offset > 0 {
            used.push("--offset");
        }
        if self.sort.is_some() {
            used.push("--sort");
        }
        if self.year.is_some() {
            used.push("--year");
        }
        if self.after.is_some() {
            used.push("--after");
        }
        if self.before.is_some() {
            used.push("--before");
        }
        if self.author.is_some() {
            used.push("--author");
        }
        if self.field.is_some() {
            used.push("--field");
        }
        if self.open_access {
            used.push("--open-access");
        }
        if self.patents {
            used.push("--patents");
        }
        if let Some(ref listing) = self.listing {
            used.push(listing.flag());
        }
        used
    }
}

/// Which search filters a source can honour natively.
#[derive(Debug, Clone, Copy, Default)]
pub struct SearchCaps {
    pub offset: bool,
    pub sort: bool,
    pub year: bool,
    pub date_range: bool,
    pub author: bool,
    pub field: bool,
    pub open_access: bool,
    pub patents: bool,
    /// `--trending` / `--top`: the source publishes ranked lists of its own.
    pub trending: bool,
}

impl SearchCaps {
    /// Query and limit only — no filters.
    pub const BASIC: SearchCaps = SearchCaps {
        offset: false,
        sort: false,
        year: false,
        date_range: false,
        author: false,
        field: false,
        open_access: false,
        patents: false,
        trending: false,
    };

    /// Whether this source supports the named CLI flag.
    pub fn supports(&self, flag: &str) -> bool {
        match flag {
            "--offset" => self.offset,
            "--sort" => self.sort,
            "--year" => self.year,
            "--after" | "--before" => self.date_range,
            "--author" => self.author,
            "--field" => self.field,
            "--open-access" => self.open_access,
            "--patents" => self.patents,
            "--trending" | "--top" => self.trending,
            _ => false,
        }
    }

    /// The flags this source does support, for use in error messages.
    pub fn supported_flags(&self) -> Vec<&'static str> {
        let mut flags = Vec::new();
        if self.offset {
            flags.push("--offset");
        }
        if self.sort {
            flags.push("--sort");
        }
        if self.year {
            flags.push("--year");
        }
        if self.date_range {
            flags.push("--after/--before");
        }
        if self.author {
            flags.push("--author");
        }
        if self.field {
            flags.push("--field");
        }
        if self.open_access {
            flags.push("--open-access");
        }
        if self.patents {
            flags.push("--patents");
        }
        if self.trending {
            flags.push("--trending/--top");
        }
        flags
    }
}

/// What a source can do. Drives both argument validation and `fastpaper sources`.
#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    /// `None` when the source has no keyword search at all.
    pub search: Option<SearchCaps>,
    pub get: bool,
    pub download: bool,
    /// Can walk citation edges in both directions.
    pub cite: bool,
    /// Hard per-request result cap imposed by the source, if any.
    pub max_limit: Option<u32>,
    /// Which `Paper` fields this source can actually fill.
    pub fields: FieldCaps,
    /// Caveat shown by `fastpaper sources --capabilities`; empty when none.
    pub notes: &'static str,
}

/// Which optional `Paper` fields a source is able to supply.
///
/// `null` in the output means "unknown", but that covers two opposite
/// situations: a source that structurally has nothing to say about a field
/// (Crossref registers DOIs and never carries a PDF link) and a source that
/// does carry it but has nothing for this particular paper. The caller should
/// change source in the first case and stop looking in the second, and the
/// record alone cannot tell them which they are in. This is what separates the
/// two, so `sources --capabilities` can answer "who would know".
///
/// A `true` here is a claim about the parser, not about any one paper --
/// `tests/field_capabilities.rs` holds it to captured responses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldCaps {
    /// A link to the file itself.
    pub pdf_url: bool,
    /// Whether the paper is openly readable.
    pub open_access: bool,
    /// How many papers cite this one.
    pub citations: bool,
    /// Social signals (`Paper::community`).
    pub community: bool,
}

impl FieldCaps {
    /// Fills none of them: the source answers only the core bibliographic
    /// fields every source has.
    pub const NONE: FieldCaps = FieldCaps {
        pdf_url: false,
        open_access: false,
        citations: false,
        community: false,
    };

    /// Everything a source that only indexes open access material can say
    /// about access, with no citation data.
    pub const OPEN_FILES: FieldCaps = FieldCaps {
        pdf_url: true,
        open_access: true,
        citations: false,
        community: false,
    };

    /// Files, access and citation counts -- the full set.
    pub const ALL: FieldCaps = FieldCaps {
        pdf_url: true,
        open_access: true,
        citations: true,
        community: false,
    };
}

/// Check that a `--after` / `--before` value is a plain `YYYY-MM-DD` date.
///
/// Sources reshape it into whatever their API wants, but they all reject the
/// same malformed input, and they should reject it before the request goes out
/// rather than passing it along for the server to misinterpret.
pub fn validate_ymd(date: &str) -> Result<&str, String> {
    let parts: Vec<&str> = date.split('-').collect();
    let ok = parts.len() == 3
        && parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()));
    if ok {
        Ok(date)
    } else {
        Err(format!("Invalid date '{}': expected YYYY-MM-DD", date))
    }
}

/// Contact address used to identify this client to APIs that ask for one
/// (Crossref's polite pool, OpenAlex, NCBI E-utilities).
///
/// Returns `None` unless the user sets `FASTPAPER_EMAIL`. Sending a third
/// party's address on the user's behalf misattributes the traffic and risks
/// getting that address throttled, so the parameter is simply omitted when
/// the user has not supplied one — every such API treats it as optional.
/// Unpaywall is the exception: it *requires* an address and errors without one.
pub fn contact_email() -> Option<String> {
    std::env::var("FASTPAPER_EMAIL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Which way along a citation edge to walk.
///
/// Naming is deliberately about direction rather than the words "citations"
/// and "references", which flip meaning depending on who is speaking.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Papers that cite this one — what the field did next.
    Incoming,
    /// Papers this one cites — what it was built on.
    Outgoing,
}

/// Community signals: what a paper's audience does with it, as opposed to
/// what the literature does with it (`citations`).
///
/// Only huggingface fills this. Every other source leaves `Paper::community`
/// as `None`, which serializes as `null` like every other unknown.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Community {
    pub upvotes: Option<u32>,
    pub comments: Option<u32>,
    pub organization: Option<String>,
    pub github_repo: Option<String>,
    pub github_stars: Option<u32>,
    /// The day the paper went onto Hugging Face's daily list, `YYYY-MM-DD`.
    pub listed_on: Option<String>,
    /// Only the single-paper endpoint (`get`) reports these three.
    pub linked_models: Option<u32>,
    pub linked_datasets: Option<u32>,
    pub linked_spaces: Option<u32>,
}

/// A paper returned from any source.
#[derive(Debug, Clone, Serialize)]
pub struct Paper {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    #[serde(rename = "abstract")]
    pub abstract_text: Option<String>,
    pub year: Option<u16>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub pdf_url: Option<String>,
    pub venue: Option<String>,
    pub citations: Option<u32>,
    pub fields: Vec<String>,
    pub open_access: Option<bool>,
    pub source: String,
    /// Social signals; `None` everywhere except huggingface.
    pub community: Option<Community>,
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_simple_query_asks_for_no_patents() {
        assert!(!SearchQuery::simple("attention", 10).patents);
    }

    #[test]
    fn setting_patents_shows_up_as_an_active_filter() {
        let mut q = SearchQuery::simple("attention", 10);
        q.patents = true;
        assert!(q.active_filters().contains(&"--patents"));
    }

    #[test]
    fn patents_is_not_a_basic_capability() {
        assert!(!SearchCaps::BASIC.supports("--patents"));
    }

    #[test]
    fn a_source_declaring_patents_supports_the_flag() {
        let caps = SearchCaps {
            patents: true,
            ..SearchCaps::BASIC
        };
        assert!(caps.supports("--patents"));
        assert!(caps.supported_flags().contains(&"--patents"));
    }

    #[test]
    fn a_period_is_read_by_its_shape() {
        assert_eq!(
            Listing::parse_period("2026-09-18"),
            Ok(Listing::Day("2026-09-18".into()))
        );
        assert_eq!(
            Listing::parse_period("2026-W38"),
            Ok(Listing::Week("2026-W38".into()))
        );
        assert_eq!(
            Listing::parse_period("2026-08"),
            Ok(Listing::Month("2026-08".into()))
        );
    }

    // W53 is a real ISO week; whether a source takes it is the source's call.
    #[test]
    fn week_53_parses() {
        assert_eq!(
            Listing::parse_period("2026-W53"),
            Ok(Listing::Week("2026-W53".into()))
        );
    }

    #[test]
    fn a_malformed_period_says_which_shapes_work() {
        for bad in ["2026-8", "2026-W60", "2026-13", "last week", ""] {
            let err = Listing::parse_period(bad).unwrap_err();
            assert!(err.contains("2026-W38"), "{bad}: {err}");
        }
    }

    #[test]
    fn a_listing_reports_the_flag_that_asked_for_it() {
        let mut q = SearchQuery::simple("", 10);
        q.listing = Some(Listing::Trending);
        assert_eq!(q.active_filters(), vec!["--trending"]);
        q.listing = Some(Listing::Month("2026-08".into()));
        assert_eq!(q.active_filters(), vec!["--top"]);
    }

    #[test]
    fn both_listing_flags_follow_one_capability() {
        let caps = SearchCaps {
            trending: true,
            ..SearchCaps::BASIC
        };
        assert!(caps.supports("--trending") && caps.supports("--top"));
        assert_eq!(caps.supported_flags(), vec!["--trending/--top"]);
        assert!(!SearchCaps::BASIC.supports("--top"));
    }
}
