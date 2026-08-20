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
    out.push_str(
        "Source      search  get  download  cite  figures   pdf_url  open_access  citations\n",
    );
    out.push_str(
        "─────────────────────────────────────────────────────────────────────────────────\n",
    );
    for source in registry::ALL {
        let caps = source.caps();
        out.push_str(&format!(
            "{:<11} {:^6}  {:^3}  {:^8}  {:^4}  {:^7}  {:^7}  {:^11}  {:^9}\n",
            source.name(),
            mark(caps.search.is_some()),
            mark(caps.get),
            mark(caps.download),
            mark(caps.cite),
            // Read straight off the capability slot rather than a bool
            // mirrored into `Capabilities`, so this column cannot fall out
            // of step with the function the command actually calls.
            mark(source.entry().figures.is_some()),
            mark(caps.fields.pdf_url),
            mark(caps.fields.open_access),
            mark(caps.fields.citations),
        ));
    }
    // The first five columns say which commands work; the last three say which
    // fields come back filled. Both are needed to pick a source: `crossref`
    // answers `get` but never carries a PDF link, so a caller who reads only
    // the left half asks it for something it structurally cannot supply.
    out.push_str(
        "\nThe last three columns are which fields this source can fill.\n\
         `null` in a result means unknown -- a `✗` here says the source never\n\
         supplies it, so ask a source marked `✓` instead of giving up.\n",
    );

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

    // The figures column is derived from the registry's capability slot, so
    // this pins the derivation rather than a copy of it: wiring a new source
    // for figures moves this listing without anyone editing it, and dropping
    // the column shows up here rather than in a user's terminal.
    #[test]
    fn the_figures_column_tracks_the_registry() {
        let out = render(false);
        assert!(out.contains("figures"), "header lost the column:\n{}", out);

        for source in registry::ALL {
            let row = out
                .lines()
                .find(|l| l.starts_with(source.name()))
                .unwrap_or_else(|| panic!("no row for {}", source.name()));
            let marks: Vec<&str> = row.split_whitespace().skip(1).collect();
            // search, get, download, cite, figures -- figures is the fifth.
            let shown = marks[4];
            let expected = if source.entry().figures.is_some() {
                "\u{2713}"
            } else {
                "\u{2717}"
            };
            assert_eq!(
                shown,
                expected,
                "{} figures column disagrees with the registry",
                source.name()
            );
        }
    }

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
