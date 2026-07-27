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
pub mod scholar;
pub mod semantic;
pub mod unpaywall;
pub mod xueshu;
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