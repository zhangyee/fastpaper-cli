use super::CommandResult;
use crate::cli::SourcesArgs;
use crate::registry::{self, Source};

pub fn run(args: &SourcesArgs) -> CommandResult {
    print!("{}", render(args.capabilities));
    Ok(())
}

/// Built from the registry rather than a hand-maintained table, so the listing
/// cannot drift from what the commands actually do.
fn render(detailed: bool) -> String {
    let mut out = String::new();
    out.push_str("Source      search  get  download  cite\n");
    out.push_str("───────────────────────────────────────\n");
    for source in registry::ALL {
        let caps = source.caps();
        out.push_str(&format!(
            "{:<11} {:^6}  {:^3}  {:^8}  {:^4}\n",
            source.name(),
            mark(caps.search.is_some()),
            mark(caps.get),
            mark(caps.download),
            mark(caps.cite),
        ));
    }

    if detailed {
        out.push_str("\nSearch filters\n──────────────\n");
        for source in registry::ALL {
            out.push_str(&format!("{:<11} {}\n", source.name(), filters(*source)));
        }

        let noted: Vec<&Source> = registry::ALL
            .iter()
            .filter(|s| !s.caps().notes.is_empty())
            .collect();
        if !noted.is_empty() {
            out.push_str("\nNotes\n─────\n");
            for source in noted {
                out.push_str(&format!("{:<11} {}\n", source.name(), source.caps().notes));
            }
        }
    }

    out
}

fn filters(source: Source) -> String {
    let caps = source.caps();
    match caps.search {
        None => "no keyword search".to_string(),
        Some(search) => {
            let flags = search.supported_flags();
            let mut line = if flags.is_empty() {
                "query and -n only".to_string()
            } else {
                flags.join(", ")
            };
            if let Some(max) = caps.max_limit {
                line.push_str(&format!("  (max -n {})", max));
            }
            line
        }
    }
}

fn mark(supported: bool) -> &'static str {
    if supported { "✓" } else { "✗" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_every_registered_source() {
        let out = render(false);
        for source in registry::ALL {
            assert!(out.contains(source.name()), "missing {}", source.name());
        }
    }

    #[test]
    fn unpaywall_is_marked_as_having_no_search() {
        assert_eq!(filters(Source::Unpaywall), "no keyword search");
    }

    #[test]
    fn zenodo_reports_its_result_cap() {
        assert!(filters(Source::Zenodo).contains("max -n 25"));
    }

    #[test]
    fn capabilities_view_surfaces_the_biorxiv_caveat() {
        let out = render(true);
        assert!(
            out.contains("no keyword search API"),
            "biorxiv's local-filtering caveat should be visible"
        );
    }

    #[test]
    fn plain_view_omits_the_notes_section() {
        assert!(!render(false).contains("Notes"));
    }

    #[test]
    fn unpaywall_note_names_the_env_var_it_actually_reads() {
        let out = render(true);
        assert!(
            out.contains("UNPAYWALL_EMAIL"),
            "the note must name the variable sources::unpaywall reads, got:\n{}",
            out
        );
    }
}
