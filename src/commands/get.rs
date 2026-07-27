use super::{emit, failed, render, CommandError, CommandResult};
use crate::cli::{GetArgs, GlobalOpts};
use crate::identifier::{detect_id_type, IdType};
use crate::registry::Source;

pub fn run(args: &GetArgs, global: &GlobalOpts) -> CommandResult {
    let (explicit, raw_id) = args.resolve().map_err(CommandError::Failed)?;

    let source = match explicit {
        Some(source) => source,
        None => route(raw_id)?,
    };

    let entry = source.entry();
    let get = entry
        .get
        .ok_or_else(|| unsupported_get(source))?;

    let id = normalize_id(source, raw_id);
    match get(&source.base_url(), &id).map_err(CommandError::Failed)? {
        Some(paper) => emit(&render(&[paper], global.format), None),
        None => Err(CommandError::NotFound(format!(
            "Paper not found in {}: {}",
            source.name(),
            raw_id
        ))),
    }
}

/// Pick the source that owns this identifier shape.
fn route(id: &str) -> Result<Source, CommandError> {
    match detect_id_type(id) {
        IdType::Arxiv | IdType::ArxivOld => Ok(Source::Arxiv),
        IdType::Doi => Ok(Source::Crossref),
        IdType::Pmc => Ok(Source::Pmc),
        IdType::Pmid => Ok(Source::Pubmed),
        IdType::S2 => Ok(Source::Semantic),
        IdType::Url => Err(failed(format!(
            "Cannot look up a URL directly: {}\nPass the DOI or arXiv ID instead, or name a source: fastpaper get <source> <id>",
            id
        ))),
        IdType::Unknown => Err(failed(format!(
            "Unrecognized identifier: '{}'\nExpected an arXiv ID, DOI, PMC ID, PMID or S2 ID.\nTo search by keyword instead: fastpaper search <source> \"{}\"",
            id, id
        ))),
    }
}

fn unsupported_get(source: Source) -> CommandError {
    let hint = match source {
        Source::Openalex | Source::Dblp | Source::Openaire | Source::Doaj | Source::Zenodo
        | Source::Hal | Source::Core | Source::Europepmc => {
            "\nTry a source that resolves identifiers: crossref (DOI), arxiv, pubmed, pmc, semantic."
        }
        _ => "",
    };
    failed(format!(
        "'{}' cannot fetch a paper by identifier.{}",
        source.name(),
        hint
    ))
}

/// Strip the disambiguating prefixes that only exist to help `detect_id_type`;
/// the APIs themselves want the bare identifier.
fn normalize_id(source: Source, id: &str) -> String {
    match source {
        Source::Pubmed => id.strip_prefix("PMID:").unwrap_or(id).to_string(),
        Source::Semantic => id.strip_prefix("S2:").unwrap_or(id).to_string(),
        _ => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arxiv_id_routes_to_arxiv() {
        assert_eq!(route("2301.08745").unwrap(), Source::Arxiv);
        assert_eq!(route("hep-th/9711200").unwrap(), Source::Arxiv);
    }

    #[test]
    fn doi_routes_to_crossref() {
        assert_eq!(route("10.1038/nature12373").unwrap(), Source::Crossref);
    }

    #[test]
    fn pmc_id_routes_to_pmc() {
        assert_eq!(route("PMC7318926").unwrap(), Source::Pmc);
    }

    #[test]
    fn pmid_routes_to_pubmed() {
        assert_eq!(route("PMID:33475315").unwrap(), Source::Pubmed);
        assert_eq!(route("33475315").unwrap(), Source::Pubmed);
    }

    #[test]
    fn s2_id_routes_to_semantic() {
        assert_eq!(route("S2:abc123def").unwrap(), Source::Semantic);
    }

    #[test]
    fn url_is_rejected_with_advice() {
        let err = route("https://arxiv.org/abs/2301.08745").unwrap_err();
        assert!(err.message().contains("DOI or arXiv ID"), "got: {}", err.message());
    }

    #[test]
    fn unknown_identifier_suggests_search() {
        let err = route("attention is all you need").unwrap_err();
        assert!(
            err.message().contains("fastpaper search"),
            "a phrase is probably a search: {}",
            err.message()
        );
    }

    #[test]
    fn pmid_prefix_is_stripped_for_the_api() {
        assert_eq!(normalize_id(Source::Pubmed, "PMID:33475315"), "33475315");
        assert_eq!(normalize_id(Source::Pubmed, "33475315"), "33475315");
    }

    #[test]
    fn s2_prefix_is_stripped_for_the_api() {
        assert_eq!(normalize_id(Source::Semantic, "S2:abc123"), "abc123");
    }

    #[test]
    fn other_sources_keep_the_identifier_verbatim() {
        assert_eq!(
            normalize_id(Source::Crossref, "10.1038/nature12373"),
            "10.1038/nature12373"
        );
        assert_eq!(normalize_id(Source::Pmc, "PMC7318926"), "PMC7318926");
    }

    #[test]
    fn metadata_only_sources_get_a_useful_error() {
        let err = unsupported_get(Source::Openalex);
        assert!(err.message().contains("openalex"), "got: {}", err.message());
        assert!(err.message().contains("crossref"), "got: {}", err.message());
    }
}
