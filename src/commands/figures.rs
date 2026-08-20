use super::{CommandError, CommandResult, failed};
use crate::cli::FiguresArgs;
use crate::download::{FetchError, human_size};
use crate::figures::{SaveError, save_figures};
use crate::identifier::{IdType, detect_id_type};
use crate::registry::Source;

/// Where NCBI's DOI→PMCID converter lives. Its own variable because the
/// service sits on a different host from either PMC base URL in the registry.
fn idconv_base() -> String {
    std::env::var("FASTPAPER_IDCONV_URL")
        .unwrap_or_else(|_| "https://pmc.ncbi.nlm.nih.gov".to_string())
}

pub fn run(args: &FiguresArgs) -> CommandResult {
    let (explicit, raw_id) = args.resolve().map_err(CommandError::Failed)?;

    // A DOI names no archive on either source, so it is resolved to a PMC ID
    // first. Same shape as `download` sending a DOI to Semantic Scholar: the
    // source that owns the identifier is not the one that serves the files.
    let (source, id) = match explicit {
        Some(source) => (source, raw_id.to_string()),
        None => route(raw_id)?,
    };

    let entry = source.entry();
    let fetch = entry.figures.ok_or_else(|| unsupported(source))?;

    let base = match source {
        // arXiv's e-print endpoint lives with the files, not the Atom API.
        Source::Arxiv => source.pdf_base_url(),
        _ => source.base_url(),
    };

    let files = fetch(&base, &id, args.max_size).map_err(|e| match e {
        FetchError::NotFound(m) => CommandError::NotFound(m),
        FetchError::Failed(m) => CommandError::Failed(m),
    })?;

    // No source decides this for itself: both `unzip_images` and
    // `untar_gz_images` hand back `Ok(vec![])` for an archive that holds no
    // figure-extension files (e.g. an arXiv source package that is a single
    // .tex, or a Europe PMC package that carries only tables). Owning the
    // case here is what makes "no figure files" the same `NotFound` for
    // every source, instead of a false "Saved: 0 figure files" success.
    if files.is_empty() {
        return Err(CommandError::NotFound(format!(
            "No figure files were found for {}.",
            raw_id
        )));
    }

    // `raw_id` rather than `id`: the directory should be named for what the
    // user typed, not for a PMC ID they never mentioned.
    match save_figures(&files, &args.dir, raw_id, args.overwrite) {
        Ok((dir, total)) => {
            eprintln!(
                "Saved: {} figure files to {} ({})",
                files.len(),
                dir.display(),
                human_size(total)
            );
            Ok(())
        }
        // Matched on the typed variant, not by string-searching the message:
        // a hostile-entry rejection's text must never be able to collide
        // with "already exists" and get reported as success.
        Err(e @ SaveError::AlreadyExists(_)) => Err(CommandError::AlreadyExists(e.to_string())),
        Err(e) => Err(CommandError::Failed(e.to_string())),
    }
}

/// Pick the source that actually serves figure archives for this identifier.
fn route(id: &str) -> Result<(Source, String), CommandError> {
    match detect_id_type(id) {
        IdType::Arxiv | IdType::ArxivOld => Ok((Source::Arxiv, id.to_string())),
        IdType::Pmc => Ok((Source::Europepmc, id.to_string())),
        IdType::Doi => {
            let pmcid = crate::sources::pmc::pmcid_for_doi(&idconv_base(), id)
                .map_err(|e| CommandError::Failed(e.message().to_string()))?
                .ok_or_else(|| {
                    CommandError::NotFound(format!(
                        "{} is not in PMC, so there are no figure files to fetch.",
                        id
                    ))
                })?;
            Ok((Source::Europepmc, pmcid))
        }
        IdType::Pmid => Err(failed(format!(
            "PubMed indexes abstracts, not files: {}\nLook up the PMC ID for the same article, then: fastpaper figures <PMC_ID>",
            id
        ))),
        IdType::S2 | IdType::Url => Err(failed(format!(
            "Cannot fetch figures for: {}\nPass an arXiv ID, a PMC ID, or a DOI.",
            id
        ))),
        IdType::Unknown => Err(failed(format!(
            "Unrecognized identifier: '{}'\nExpected an arXiv ID, PMC ID or DOI.",
            id
        ))),
    }
}

fn unsupported(source: Source) -> CommandError {
    failed(format!(
        "'{}' cannot provide figures.\nOnly arXiv (source packages) and Europe PMC (supplementary packages) publish the authors' original figure files. Try: fastpaper figures <arXiv ID | PMC ID | DOI>",
        source.name()
    ))
}
