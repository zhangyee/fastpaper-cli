use super::{CommandError, CommandResult, emit, failed, render, results_summary};
use crate::cli::{CiteArgs, GlobalOpts};
use crate::identifier::{IdType, detect_id_type};
use crate::registry::Source;

pub fn run(args: &CiteArgs, global: &GlobalOpts) -> CommandResult {
    let (explicit, raw_id) = args.resolve().map_err(CommandError::Failed)?;

    let source = match explicit {
        Some(source) => source,
        None => route(raw_id)?,
    };

    let cite = source
        .entry()
        .cite
        .ok_or_else(|| unsupported_cite(source))?;
    let id = normalize_id(source, raw_id);

    let papers =
        cite(&source.base_url(), &id, args.direction, args.limit).map_err(CommandError::Failed)?;

    // An empty edge list is a real answer — a 2026 preprint nobody has cited
    // yet, or a record whose references are not indexed. `search` returns an
    // empty array and exit 0 for the same situation; matching that keeps
    // scripts from treating "no citations yet" as a failure.
    let summary = results_summary(papers.len());
    emit(
        &render(&papers, global.format),
        args.output.as_deref(),
        &summary,
        global.quiet,
    )
}

/// Pick a source that actually holds citation edges for this identifier.
///
/// Unlike `get`, which sends a DOI to its registrar, this defaults to OpenAlex:
/// it resolves DOIs itself and needs no key, so the command works out of the
/// box. Semantic Scholar has richer edges but fails hard without a key, so it
/// is opt-in except for identifiers only it can address.
fn route(id: &str) -> Result<Source, CommandError> {
    match detect_id_type(id) {
        IdType::Doi => Ok(Source::Openalex),
        // arXiv publishes no citation data of its own, and OpenAlex keys off
        // DOIs; Semantic Scholar addresses arXiv ids directly.
        IdType::Arxiv | IdType::ArxivOld | IdType::S2 => Ok(Source::Semantic),
        _ => Err(failed(format!(
            "Cannot tell what '{id}' is.\n\
             Citation edges need a DOI, an arXiv id or an S2 id, or name a \
             source: fastpaper cite <semantic|openalex> <id>"
        ))),
    }
}

fn unsupported_cite(source: Source) -> CommandError {
    failed(format!(
        "'{}' exposes no citation edges.\n\
         Sources that do: semantic (richest, needs SEMANTIC_SCHOLAR_API_KEY), openalex (no key needed).",
        source.name()
    ))
}

/// Semantic Scholar addresses external identifiers by prefix; a bare DOI or
/// arXiv id is not a valid paper id there.
fn normalize_id(source: Source, id: &str) -> String {
    match source {
        Source::Semantic => {
            if let Some(rest) = id.strip_prefix("S2:") {
                rest.to_string()
            } else {
                match detect_id_type(id) {
                    IdType::Doi => format!("DOI:{id}"),
                    IdType::Arxiv | IdType::ArxivOld => format!("ARXIV:{id}"),
                    _ => id.to_string(),
                }
            }
        }
        _ => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // OpenAlex needs no key and resolves DOIs itself, so it is the default
    // that works out of the box; semantic is richer but key-gated.
    #[test]
    fn a_doi_routes_to_openalex() {
        assert_eq!(route("10.1038/nature12373").unwrap(), Source::Openalex);
    }

    #[test]
    fn an_s2_id_routes_to_semantic() {
        assert_eq!(route("S2:abc123").unwrap(), Source::Semantic);
    }

    // arXiv publishes no citation data, so an arXiv id has to be resolved by a
    // source that does rather than silently going nowhere.
    #[test]
    fn an_arxiv_id_routes_to_a_source_with_citation_data() {
        assert_eq!(route("2301.08745").unwrap(), Source::Semantic);
    }

    #[test]
    fn an_unrecognised_id_says_what_it_expected() {
        let err = route("some random string").unwrap_err();
        assert!(err.message().contains("DOI"), "got: {}", err.message());
    }

    #[test]
    fn a_source_without_citation_edges_names_the_ones_that_have_them() {
        let err = unsupported_cite(Source::Arxiv);
        assert!(err.message().contains("semantic"), "got: {}", err.message());
        assert!(err.message().contains("openalex"), "got: {}", err.message());
    }

    #[test]
    fn s2_prefix_is_stripped_before_the_request() {
        assert_eq!(normalize_id(Source::Semantic, "S2:abc123"), "abc123");
    }

    #[test]
    fn external_ids_are_prefixed_for_semantic_scholar() {
        assert_eq!(
            normalize_id(Source::Semantic, "10.1038/nature12373"),
            "DOI:10.1038/nature12373"
        );
        assert_eq!(
            normalize_id(Source::Semantic, "2301.08745"),
            "ARXIV:2301.08745"
        );
    }

    #[test]
    fn openalex_takes_identifiers_untouched() {
        assert_eq!(
            normalize_id(Source::Openalex, "10.1038/nature12373"),
            "10.1038/nature12373"
        );
    }
}
