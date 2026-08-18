//! Check the declared field coverage against what each parser actually produces.
//!
//! `sources --capabilities` answers "who can tell me whether this paper has an
//! open access PDF". An answer that is merely asserted in a table is worth
//! nothing: a source declared `pdf_url ✓` that never fills the field sends the
//! caller somewhere useless, and one declared `✗` that does fill it hides a
//! source that would have worked. So the table is checked against real
//! responses rather than trusted.
//!
//! Fixtures are the yardstick because they are captured API responses. A source
//! whose fixtures never populate a field it genuinely supports would fail here
//! -- correctly, since at that point nothing in the repo demonstrates the claim.
//! Fix it by capturing a response that does, the way `semantic` needs its
//! citation fixtures to show `openAccessPdf`.

use fastpaper::registry::{ALL, Source};
use fastpaper::sources::{self, Direction, Paper};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {}", path, e))
}

/// Every paper this source's parsers can produce from the captured responses.
///
/// A source gets the union of its fixtures: one endpoint may omit a field that
/// another returns, and the question is what the source can supply, not what
/// any single response happened to carry.
fn papers(source: Source) -> Vec<Paper> {
    let f = fixture;
    match source {
        Source::Arxiv => sources::arxiv::parse_search_response(&f("arxiv_search.xml")).unwrap(),
        Source::Biorxiv => {
            sources::biorxiv::parse_search_response(&f("biorxiv_search.json")).unwrap()
        }
        Source::Medrxiv => {
            sources::medrxiv::parse_search_response(&f("medrxiv_search.json")).unwrap()
        }
        Source::Pubmed => sources::pubmed::parse_efetch_response(&f("pubmed_efetch.xml")).unwrap(),
        Source::Pmc => sources::pmc::parse_efetch_response(&f("pmc_efetch.xml")).unwrap(),
        Source::Europepmc => {
            sources::europepmc::parse_search_response(&f("europepmc_search.json")).unwrap()
        }
        Source::Scholar => {
            sources::scholar::parse_search_response(&f("scholar_search.html")).unwrap()
        }
        Source::Xueshu => {
            sources::xueshu::parse_search_response(&f("xueshu_search.json"), false).unwrap()
        }
        // The search fixture carries no `openAccessPdf`; the edge fixtures do,
        // and both come from the same parser.
        Source::Semantic => {
            let mut all =
                sources::semantic::parse_search_response(&f("semantic_search.json")).unwrap();
            all.extend(
                sources::semantic::parse_edge_response(
                    &f("semantic_citations.json"),
                    Direction::Incoming,
                )
                .unwrap(),
            );
            all.extend(
                sources::semantic::parse_edge_response(
                    &f("semantic_references.json"),
                    Direction::Outgoing,
                )
                .unwrap(),
            );
            all
        }
        Source::Crossref => {
            sources::crossref::parse_search_response(&f("crossref_search.json")).unwrap()
        }
        Source::Openalex => {
            sources::openalex::parse_search_response(&f("openalex_search.json")).unwrap()
        }
        Source::Dblp => sources::dblp::parse_search_response(&f("dblp_search.xml")).unwrap(),
        Source::Core => sources::core::parse_search_response(&f("core_search.json")).unwrap(),
        Source::Openaire => {
            sources::openaire::parse_search_response(&f("openaire_graph_v3.json")).unwrap()
        }
        Source::Doaj => sources::doaj::parse_search_response(&f("doaj_search.json")).unwrap(),
        Source::Zenodo => sources::zenodo::parse_search_response(&f("zenodo_search.json")).unwrap(),
        Source::Hal => sources::hal::parse_search_response(&f("hal_search.json")).unwrap(),
        Source::Unpaywall => {
            vec![sources::unpaywall::parse_response(&f("unpaywall_lookup.json")).unwrap()]
        }
    }
}

/// The three fields, as `(column name, declared, observed)`.
fn coverage(source: Source) -> Vec<(&'static str, bool, bool)> {
    let caps = source.caps().fields;
    let papers = papers(source);
    vec![
        (
            "pdf_url",
            caps.pdf_url,
            papers.iter().any(|p| p.pdf_url.is_some()),
        ),
        (
            "open_access",
            caps.open_access,
            papers.iter().any(|p| p.open_access.is_some()),
        ),
        (
            "citations",
            caps.citations,
            papers.iter().any(|p| p.citations.is_some()),
        ),
    ]
}

#[test]
fn every_source_declares_the_fields_its_parser_actually_fills() {
    let mut wrong = Vec::new();
    for source in ALL {
        for (field, declared, observed) in coverage(*source) {
            if declared != observed {
                wrong.push(format!(
                    "{}.{}: declared {}, fixtures produce {}",
                    source.name(),
                    field,
                    if declared { "✓" } else { "✗" },
                    if observed { "✓" } else { "✗" },
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "field coverage is wrong:\n{}",
        wrong.join("\n")
    );
}

// The table exists so a caller can pick a source by what it will answer. If a
// field were `✗` everywhere the column would be dead weight, and if it were `✓`
// everywhere it would tell nobody anything -- either way the column would be
// worth deleting rather than printing.
#[test]
fn each_field_column_actually_discriminates_between_sources() {
    for (i, field) in ["pdf_url", "open_access", "citations"].iter().enumerate() {
        let declared: Vec<bool> = ALL.iter().map(|s| coverage(*s)[i].1).collect();
        assert!(
            declared.iter().any(|d| *d) && declared.iter().any(|d| !*d),
            "{} is the same for every source; the column says nothing",
            field
        );
    }
}
