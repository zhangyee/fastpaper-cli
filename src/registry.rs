//! The one place that maps a source name to everything the commands need:
//! its base URL, what it can do, and which functions to call.
//!
//! Sources stay plain free functions in their own files — this table holds
//! function pointers rather than a trait, so no source has to implement
//! anything. Where a source's signature does not yet match the table's, a
//! three-line adapter bridges it; those adapters disappear as each source is
//! migrated.

use clap::ValueEnum;

use crate::download;
use crate::sources::{self, Capabilities, Paper, SearchCaps, SearchQuery};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Arxiv,
    Biorxiv,
    Medrxiv,
    Pubmed,
    Pmc,
    Europepmc,
    Scholar,
    Xueshu,
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
}

/// Every source, in the order `fastpaper sources` lists them.
pub const ALL: &[Source] = &[
    Source::Arxiv,
    Source::Biorxiv,
    Source::Medrxiv,
    Source::Pubmed,
    Source::Pmc,
    Source::Europepmc,
    Source::Scholar,
    Source::Xueshu,
    Source::Semantic,
    Source::Crossref,
    Source::Openalex,
    Source::Dblp,
    Source::Core,
    Source::Openaire,
    Source::Doaj,
    Source::Unpaywall,
    Source::Zenodo,
    Source::Hal,
];

pub struct SourceEntry {
    pub name: &'static str,
    pub caps: Capabilities,
    /// Env var that overrides `default_base`.
    pub env_var: &'static str,
    pub default_base: &'static str,
    /// Separate host for PDF downloads where the API and the files differ.
    pub pdf_env_var: Option<&'static str>,
    pub pdf_default_base: Option<&'static str>,
    pub search: Option<fn(&str, &SearchQuery) -> Result<Vec<Paper>, String>>,
    pub get: Option<fn(&str, &str) -> Result<Option<Paper>, String>>,
    pub pdf: Option<fn(&str, &str) -> Result<Vec<u8>, String>>,
}

impl Source {
    pub fn name(&self) -> &'static str {
        self.entry().name
    }

    pub fn caps(&self) -> Capabilities {
        self.entry().caps
    }

    /// Base URL for metadata requests, overridable via this source's env var.
    pub fn base_url(&self) -> String {
        let e = self.entry();
        std::env::var(e.env_var).unwrap_or_else(|_| e.default_base.to_string())
    }

    /// Base URL for PDF downloads.
    ///
    /// Sources whose files live on a different host than their API get their
    /// own override; failing that an explicitly-set general override still
    /// applies, so pointing one variable at a test server redirects both.
    /// Only the *defaults* are kept separate.
    pub fn pdf_base_url(&self) -> String {
        let e = self.entry();
        if let Some(var) = e.pdf_env_var {
            if let Ok(value) = std::env::var(var) {
                return value;
            }
        }
        if let Ok(value) = std::env::var(e.env_var) {
            return value;
        }
        e.pdf_default_base.unwrap_or(e.default_base).to_string()
    }

    /// Resolve a source from a CLI token, for the `[source] <id>` forms where
    /// clap cannot type the argument as an enum.
    pub fn from_name(token: &str) -> Option<Source> {
        ALL.iter().copied().find(|s| s.name() == token)
    }

    pub fn entry(&self) -> &'static SourceEntry {
        match self {
            Source::Arxiv => &ARXIV,
            Source::Biorxiv => &BIORXIV,
            Source::Medrxiv => &MEDRXIV,
            Source::Pubmed => &PUBMED,
            Source::Pmc => &PMC,
            Source::Europepmc => &EUROPEPMC,
            Source::Scholar => &SCHOLAR,
            Source::Xueshu => &XUESHU,
            Source::Semantic => &SEMANTIC,
            Source::Crossref => &CROSSREF,
            Source::Openalex => &OPENALEX,
            Source::Dblp => &DBLP,
            Source::Core => &CORE,
            Source::Openaire => &OPENAIRE,
            Source::Doaj => &DOAJ,
            Source::Unpaywall => &UNPAYWALL,
            Source::Zenodo => &ZENODO,
            Source::Hal => &HAL,
        }
    }
}

// ── adapters ────────────────────────────────────

/// Unpaywall returns a bare `Paper`; a missing DOI surfaces as an error there.
fn g_unpaywall(base: &str, id: &str) -> Result<Option<Paper>, String> {
    sources::unpaywall::lookup_doi(base, id).map(Some)
}

// ── entries ─────────────────────────────────────

static ARXIV: SourceEntry = SourceEntry {
    name: "arxiv",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: true,
            year: true,
            date_range: true,
            author: true,
            field: true,
            // Every arXiv paper is freely readable, so the filter is
            // trivially satisfied rather than unsupported.
            open_access: true,
        }),
        get: true,
        download: true,
        max_limit: Some(2000),
        notes: "--sort citations is unavailable: arXiv publishes no citation counts",
    },
    env_var: "FASTPAPER_ARXIV_URL",
    default_base: "https://export.arxiv.org",
    pdf_env_var: Some("FASTPAPER_ARXIV_PDF_URL"),
    pdf_default_base: Some("https://arxiv.org"),
    search: Some(sources::arxiv::search),
    get: Some(sources::arxiv::get_by_id),
    pdf: Some(download::pdf_bytes_arxiv),
};

static BIORXIV: SourceEntry = SourceEntry {
    name: "biorxiv",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: false,
            year: true,
            date_range: true,
            author: false,
            field: false,
            // Every preprint here is freely readable.
            open_access: true,
        }),
        get: false,
        download: true,
        max_limit: None,
        notes: "no keyword search API: browses a date window and matches the \
                keyword locally, so --after/--before/--year decide what is searched",
    },
    env_var: "FASTPAPER_BIORXIV_URL",
    default_base: "https://api.biorxiv.org",
    pdf_env_var: Some("FASTPAPER_BIORXIV_DL_URL"),
    pdf_default_base: Some("https://www.biorxiv.org"),
    search: Some(sources::biorxiv::search),
    get: None,
    pdf: Some(download::pdf_bytes_biorxiv),
};

static MEDRXIV: SourceEntry = SourceEntry {
    name: "medrxiv",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: false,
            year: true,
            date_range: true,
            author: false,
            field: false,
            // Every preprint here is freely readable.
            open_access: true,
        }),
        get: false,
        download: false,
        max_limit: None,
        notes: "no keyword search API: browses a date window and matches the \
                keyword locally, so --after/--before/--year decide what is \
                searched; PDF downloads are blocked by medRxiv (HTTP 403)",
    },
    env_var: "FASTPAPER_MEDRXIV_URL",
    default_base: "https://api.biorxiv.org",
    pdf_env_var: Some("FASTPAPER_MEDRXIV_DL_URL"),
    pdf_default_base: Some("https://www.medrxiv.org"),
    search: Some(sources::medrxiv::search),
    get: None,
    pdf: None,
};

static PUBMED: SourceEntry = SourceEntry {
    name: "pubmed",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: true,
            year: true,
            date_range: true,
            author: true,
            // PubMed classifies by MeSH heading, which is not the same notion
            // as a field of study and would mislead under this flag.
            field: false,
            open_access: false,
        }),
        get: true,
        download: false,
        max_limit: Some(10000),
        notes: "metadata only, for full text try pmc; --sort citations unavailable",
    },
    env_var: "FASTPAPER_PUBMED_URL",
    default_base: "https://eutils.ncbi.nlm.nih.gov",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::pubmed::search),
    get: Some(sources::pubmed::get_by_pmid),
    pdf: None,
};

static PMC: SourceEntry = SourceEntry {
    name: "pmc",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: true,
            year: true,
            date_range: true,
            author: true,
            field: false,
            open_access: true,
        }),
        get: true,
        download: true,
        max_limit: Some(10000),
        notes: "--sort citations unavailable; PDFs come from the OA subset only",
    },
    env_var: "FASTPAPER_PMC_URL",
    default_base: "https://eutils.ncbi.nlm.nih.gov",
    pdf_env_var: Some("FASTPAPER_PMC_DL_URL"),
    // The PMC Cloud Service on AWS Open Data; see download::pdf_bytes_pmc for
    // why the article page's /pdf/ URL and the OA Web Service's ftp:// links
    // are both unusable.
    pdf_default_base: Some("https://pmc-oa-opendata.s3.amazonaws.com"),
    search: Some(sources::pmc::search),
    get: Some(sources::pmc::get_by_pmc_id),
    pdf: Some(download::pdf_bytes_pmc),
};

static EUROPEPMC: SourceEntry = SourceEntry {
    name: "europepmc",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: true,
            year: true,
            date_range: true,
            author: true,
            field: false,
            open_access: true,
        }),
        get: true,
        download: true,
        max_limit: Some(1000),
        notes: "orders newest/most-cited first only, --order asc unavailable; \
                --offset must be a multiple of -n; PDFs come from the open \
                access subset",
    },
    env_var: "FASTPAPER_EUROPEPMC_URL",
    default_base: "https://www.ebi.ac.uk",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::europepmc::search),
    get: Some(sources::europepmc::get_by_id),
    pdf: Some(download::pdf_bytes_europepmc),
};

static SCHOLAR: SourceEntry = SourceEntry {
    name: "scholar",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            ..SearchCaps::BASIC
        }),
        get: false,
        download: false,
        max_limit: None,
        notes: "HTML scraping, no official API; brittle and rate-limited",
    },
    env_var: "FASTPAPER_SCHOLAR_URL",
    default_base: "https://scholar.google.com",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::scholar::search),
    get: None,
    pdf: None,
};

static XUESHU: SourceEntry = SourceEntry {
    name: "xueshu",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            ..SearchCaps::BASIC
        }),
        get: false,
        download: false,
        max_limit: None,
        notes: "unofficial endpoint; subject to bot detection",
    },
    env_var: "FASTPAPER_XUESHU_URL",
    default_base: "https://xueshu.baidu.com",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::xueshu::search),
    get: None,
    pdf: None,
};

static SEMANTIC: SourceEntry = SourceEntry {
    name: "semantic",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: true,
            year: true,
            date_range: true,
            // Paper search exposes no author parameter.
            author: false,
            field: true,
            open_access: true,
        }),
        get: true,
        download: true,
        max_limit: Some(100),
        notes: "--sort switches to the bulk endpoint, which pages by token, so it \
                cannot be combined with --offset; set SEMANTIC_SCHOLAR_API_KEY to \
                avoid heavy throttling",
    },
    env_var: "FASTPAPER_SEMANTIC_URL",
    default_base: "https://api.semanticscholar.org",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::semantic::search),
    get: Some(sources::semantic::get_by_id),
    pdf: Some(download::pdf_bytes_semantic),
};

static CROSSREF: SourceEntry = SourceEntry {
    name: "crossref",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: true,
            year: true,
            date_range: true,
            author: true,
            // Crossref indexes registered metadata; it classifies neither
            // subject area nor open access status.
            field: false,
            open_access: false,
        }),
        get: true,
        download: false,
        max_limit: Some(1000),
        notes: "metadata only, no PDF links; --offset caps at 10000",
    },
    env_var: "FASTPAPER_CROSSREF_URL",
    default_base: "https://api.crossref.org",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::crossref::search),
    get: Some(sources::crossref::get_by_doi),
    pdf: None,
};

static OPENALEX: SourceEntry = SourceEntry {
    name: "openalex",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: true,
            year: true,
            date_range: true,
            author: true,
            field: true,
            open_access: true,
        }),
        get: true,
        download: false,
        max_limit: Some(100),
        notes: "usage-metered since 2026-02, set OPENALEX_API_KEY for the larger free tier; \
                --field takes a concept ID (e.g. C154945302), not a name; \
                --offset must be a multiple of -n",
    },
    env_var: "FASTPAPER_OPENALEX_URL",
    default_base: "https://api.openalex.org",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::openalex::search),
    get: Some(sources::openalex::get_by_id),
    pdf: None,
};

static DBLP: SourceEntry = SourceEntry {
    name: "dblp",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            ..SearchCaps::BASIC
        }),
        get: false,
        download: false,
        max_limit: Some(1000),
        notes: "computer science only, metadata only; the API takes a query and \
                paging, nothing else",
    },
    env_var: "FASTPAPER_DBLP_URL",
    // dblp.org's search API has been returning HTTP 500 for every query.
    default_base: "https://dblp.uni-trier.de",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::dblp::search),
    get: None,
    pdf: None,
};

static CORE: SourceEntry = SourceEntry {
    name: "core",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            year: true,
            author: true,
            ..SearchCaps::BASIC
        }),
        get: true,
        download: true,
        max_limit: Some(100),
        notes: "CORE_API_KEY is effectively required -- anonymous requests are \
                throttled to 429; --author matches authors.name",
    },
    env_var: "FASTPAPER_CORE_URL",
    default_base: "https://api.core.ac.uk",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::core::search),
    get: Some(sources::core::get_by_id),
    pdf: Some(download::pdf_bytes_core),
};

static OPENAIRE: SourceEntry = SourceEntry {
    name: "openaire",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: true,
            year: true,
            date_range: true,
            author: true,
            field: true,
            open_access: true,
        }),
        get: false,
        download: false,
        max_limit: None,
        notes: "--field takes an OpenAIRE field-of-science value; an unrecognised \
                one is answered with the allowed list",
    },
    env_var: "FASTPAPER_OPENAIRE_URL",
    default_base: "https://api.openaire.eu",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::openaire::search),
    get: None,
    pdf: None,
};

static DOAJ: SourceEntry = SourceEntry {
    name: "doaj",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            // bibjson.year is not a sortable field and created_date is the
            // indexing date, not the publication date.
            sort: false,
            year: true,
            // DOAJ records carry a year, not a date, so a day-granular range
            // would have to be silently widened.
            date_range: false,
            author: true,
            field: false,
            // Everything in the Directory of Open Access Journals is open
            // access, so the filter is trivially satisfied.
            open_access: true,
        }),
        get: true,
        download: false,
        max_limit: Some(100),
        notes: "year granularity only, use --year rather than --after/--before; \
                no PDFs -- its fulltext links point at publisher landing pages, \
                not files",
    },
    env_var: "FASTPAPER_DOAJ_URL",
    default_base: "https://doaj.org",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::doaj::search),
    get: Some(sources::doaj::get_by_id),
    pdf: None,
};

static UNPAYWALL: SourceEntry = SourceEntry {
    name: "unpaywall",
    caps: Capabilities {
        // Unpaywall is a DOI lookup service, not a search engine.
        search: None,
        get: true,
        download: false,
        max_limit: None,
        notes: "DOI lookup only; requires a real address in FASTPAPER_EMAIL",
    },
    env_var: "FASTPAPER_UNPAYWALL_URL",
    default_base: "https://api.unpaywall.org",
    pdf_env_var: None,
    pdf_default_base: None,
    search: None,
    get: Some(g_unpaywall),
    pdf: None,
};

static ZENODO: SourceEntry = SourceEntry {
    name: "zenodo",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: true,
            year: true,
            date_range: true,
            author: true,
            field: false,
            open_access: true,
        }),
        get: false,
        download: true,
        max_limit: Some(25),
        notes: "anonymous callers get 25 results per request; --sort citations \
                unavailable; --offset must be a multiple of -n",
    },
    env_var: "FASTPAPER_ZENODO_URL",
    default_base: "https://zenodo.org",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::zenodo::search),
    get: None,
    pdf: Some(download::pdf_bytes_zenodo),
};

static HAL: SourceEntry = SourceEntry {
    name: "hal",
    caps: Capabilities {
        search: Some(SearchCaps {
            offset: true,
            sort: true,
            year: true,
            // publicationDateY_i has year granularity only, so a day-granular
            // range could only be honoured by widening it.
            date_range: false,
            author: true,
            field: true,
            open_access: true,
        }),
        get: false,
        download: true,
        max_limit: Some(10000),
        notes: "year granularity only, use --year rather than --after/--before; \
                --field takes a HAL domain code such as sdv or info",
    },
    env_var: "FASTPAPER_HAL_URL",
    default_base: "https://api.archives-ouvertes.fr",
    pdf_env_var: None,
    pdf_default_base: None,
    search: Some(sources::hal::search),
    get: None,
    pdf: Some(download::pdf_bytes_hal),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_source_has_a_matching_entry() {
        for s in ALL {
            assert_eq!(
                s.name(),
                s.entry().name,
                "entry for {:?} is wired to the wrong record",
                s
            );
        }
    }

    #[test]
    fn source_names_are_unique() {
        let mut names: Vec<_> = ALL.iter().map(|s| s.name()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate source name in ALL");
    }

    #[test]
    fn from_name_round_trips() {
        for s in ALL {
            assert_eq!(Source::from_name(s.name()), Some(*s));
        }
    }

    #[test]
    fn from_name_rejects_unknown() {
        assert_eq!(Source::from_name("nope"), None);
        assert_eq!(Source::from_name("10.1038/nature12373"), None);
    }

    #[test]
    fn download_capable_sources_have_a_pdf_fn() {
        for s in ALL {
            let e = s.entry();
            assert_eq!(
                e.caps.download,
                e.pdf.is_some(),
                "{} declares download={} but pdf fn present={}",
                e.name,
                e.caps.download,
                e.pdf.is_some()
            );
        }
    }

    #[test]
    fn get_capable_sources_have_a_get_fn() {
        for s in ALL {
            let e = s.entry();
            assert_eq!(
                e.caps.get,
                e.get.is_some(),
                "{} declares get={} but get fn present={}",
                e.name,
                e.caps.get,
                e.get.is_some()
            );
        }
    }

    #[test]
    fn search_capable_sources_have_a_search_fn() {
        for s in ALL {
            let e = s.entry();
            assert_eq!(
                e.caps.search.is_some(),
                e.search.is_some(),
                "{} declares search={} but search fn present={}",
                e.name,
                e.caps.search.is_some(),
                e.search.is_some()
            );
        }
    }

    #[test]
    #[serial_test::serial]
    fn base_url_prefers_env_override() {
        unsafe { std::env::set_var("FASTPAPER_ARXIV_URL", "http://localhost:1234") };
        assert_eq!(Source::Arxiv.base_url(), "http://localhost:1234");
        unsafe { std::env::remove_var("FASTPAPER_ARXIV_URL") };
        assert_eq!(Source::Arxiv.base_url(), "https://export.arxiv.org");
    }

    #[test]
    fn pdf_base_url_falls_back_to_base_url() {
        // semantic serves PDFs off the same host it serves metadata from
        assert_eq!(Source::Semantic.pdf_base_url(), Source::Semantic.base_url());
    }

    #[test]
    #[serial_test::serial]
    fn pdf_base_url_defaults_to_the_file_host_not_the_api_host() {
        unsafe { std::env::remove_var("FASTPAPER_ARXIV_URL") };
        unsafe { std::env::remove_var("FASTPAPER_ARXIV_PDF_URL") };
        assert_eq!(Source::Arxiv.base_url(), "https://export.arxiv.org");
        assert_eq!(Source::Arxiv.pdf_base_url(), "https://arxiv.org");
    }

    // Pointing the general override at a test server has always redirected
    // downloads too; only the defaults differ per host.
    #[test]
    #[serial_test::serial]
    fn general_override_still_redirects_pdf_downloads() {
        unsafe { std::env::remove_var("FASTPAPER_ARXIV_PDF_URL") };
        unsafe { std::env::set_var("FASTPAPER_ARXIV_URL", "http://localhost:9999") };
        assert_eq!(Source::Arxiv.pdf_base_url(), "http://localhost:9999");
        unsafe { std::env::remove_var("FASTPAPER_ARXIV_URL") };
    }

    #[test]
    #[serial_test::serial]
    fn pdf_override_wins_over_the_general_one() {
        unsafe { std::env::set_var("FASTPAPER_ARXIV_URL", "http://api.test") };
        unsafe { std::env::set_var("FASTPAPER_ARXIV_PDF_URL", "http://files.test") };
        assert_eq!(Source::Arxiv.pdf_base_url(), "http://files.test");
        unsafe { std::env::remove_var("FASTPAPER_ARXIV_URL") };
        unsafe { std::env::remove_var("FASTPAPER_ARXIV_PDF_URL") };
    }
}
