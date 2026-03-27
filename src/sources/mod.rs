pub mod arxiv;
pub mod biorxiv;
pub mod core;
pub mod crossref;
pub mod dblp;
pub mod doaj;
pub mod europepmc;
pub mod hal;
pub mod medrxiv;
pub mod openaire;
pub mod openalex;
pub mod pmc;
pub mod pubmed;
pub mod semantic;
pub mod unpaywall;
pub mod zenodo;

use serde::Serialize;

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
}

/// Result of a search operation.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub source: String,
    pub query: String,
    pub total: Option<u32>,
    pub offset: u32,
    pub limit: u32,
    pub results: Vec<Paper>,
}
