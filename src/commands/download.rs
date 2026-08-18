use super::{CommandError, CommandResult, failed};
use crate::cli::{DownloadArgs, GlobalOpts};
use crate::download::save_pdf;
use crate::identifier::{IdType, detect_id_type};
use crate::registry::Source;

pub fn run(args: &DownloadArgs, global: &GlobalOpts) -> CommandResult {
    let (explicit, raw_id) = args.resolve().map_err(CommandError::Failed)?;

    let source = match explicit {
        Some(source) => source,
        None => route(raw_id)?,
    };

    let entry = source.entry();
    let fetch = entry.pdf.ok_or_else(|| unsupported_download(source))?;

    let id = normalize_id(source, raw_id);
    let bytes = fetch(&source.pdf_base_url(), &id, args.max_size).map_err(CommandError::Failed)?;

    match save_pdf(&bytes, &args.dir, raw_id, args.overwrite) {
        Ok(path) => {
            if !global.quiet {
                eprintln!("Saved: {}", path.display());
            }
            Ok(())
        }
        Err(e) if e.contains("already exists") => Err(CommandError::AlreadyExists(e)),
        Err(e) => Err(CommandError::Failed(e)),
    }
}

/// Pick a source that can actually produce a PDF for this identifier shape.
/// This differs from `get`'s routing: Crossref owns DOI metadata but serves no
/// files, so a DOI goes to Semantic Scholar's open access resolver instead.
fn route(id: &str) -> Result<Source, CommandError> {
    match detect_id_type(id) {
        IdType::Arxiv | IdType::ArxivOld => Ok(Source::Arxiv),
        IdType::Pmc => Ok(Source::Pmc),
        IdType::Doi | IdType::S2 => Ok(Source::Semantic),
        IdType::Pmid => Err(failed(format!(
            "PubMed indexes abstracts, not PDFs: {}\nLook up the PMC ID for the same article, then: fastpaper download pmc <PMC_ID>",
            id
        ))),
        IdType::Url => Err(failed(format!(
            "Cannot download from a URL directly: {}\nPass the DOI or arXiv ID instead, or name a source: fastpaper download <source> <id>",
            id
        ))),
        IdType::Unknown => Err(failed(format!(
            "Unrecognized identifier: '{}'\nExpected an arXiv ID, DOI, PMC ID or S2 ID.",
            id
        ))),
    }
}

fn unsupported_download(source: Source) -> CommandError {
    let hint = match source {
        Source::Pubmed => "\nPubMed indexes abstracts only. Try: fastpaper download pmc <PMC_ID>",
        Source::Scholar => "\nGoogle Scholar does not serve PDFs itself.",
        Source::Xueshu => "\nBaidu Xueshu only aggregates external links.",
        Source::Crossref
        | Source::Openalex
        | Source::Dblp
        | Source::Openaire
        | Source::Unpaywall
        | Source::Europepmc => {
            "\nThis source is metadata only. Resolve the DOI to an open access copy: fastpaper download <DOI>"
        }
        _ => "",
    };
    failed(format!("'{}' cannot provide PDFs.{}", source.name(), hint))
}

/// Semantic Scholar addresses external identifiers by prefix; a bare DOI is not
/// a valid paper id there.
fn normalize_id(source: Source, id: &str) -> String {
    match source {
        Source::Semantic => {
            if let Some(rest) = id.strip_prefix("S2:") {
                rest.to_string()
            } else if detect_id_type(id) == IdType::Doi {
                format!("DOI:{}", id)
            } else {
                id.to_string()
            }
        }
        _ => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arxiv_id_routes_to_arxiv() {
        assert_eq!(route("2301.08745").unwrap(), Source::Arxiv);
    }

    #[test]
    fn pmc_id_routes_to_pmc() {
        assert_eq!(route("PMC7318926").unwrap(), Source::Pmc);
    }

    // Crossref owns DOI metadata but has no files, so a bare DOI has to go
    // somewhere that resolves open access copies.
    #[test]
    fn doi_routes_to_semantic_not_crossref() {
        assert_eq!(route("10.1038/nature12373").unwrap(), Source::Semantic);
    }

    #[test]
    fn pmid_is_rejected_and_points_at_pmc() {
        let err = route("33475315").unwrap_err();
        assert!(
            err.message().contains("download pmc"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn url_is_rejected_with_advice() {
        let err = route("https://arxiv.org/abs/2301.08745").unwrap_err();
        assert!(
            err.message().contains("DOI or arXiv ID"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn doi_is_prefixed_for_semantic_scholar() {
        assert_eq!(
            normalize_id(Source::Semantic, "10.1038/nature12373"),
            "DOI:10.1038/nature12373"
        );
    }

    #[test]
    fn s2_prefix_is_stripped_for_semantic_scholar() {
        assert_eq!(normalize_id(Source::Semantic, "S2:abc123"), "abc123");
    }

    #[test]
    fn arxiv_ids_pass_through_untouched() {
        assert_eq!(normalize_id(Source::Arxiv, "2301.08745"), "2301.08745");
        assert_eq!(normalize_id(Source::Pmc, "PMC7318926"), "PMC7318926");
    }

    #[test]
    fn metadata_only_source_error_suggests_doi_resolution() {
        let err = unsupported_download(Source::Crossref);
        assert!(
            err.message().contains("metadata only"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn pubmed_download_error_points_at_pmc() {
        let err = unsupported_download(Source::Pubmed);
        assert!(
            err.message().contains("download pmc"),
            "got: {}",
            err.message()
        );
    }
}
