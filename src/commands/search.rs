use super::{emit, failed, render, CommandError, CommandResult};
use crate::cli::{GlobalOpts, SearchArgs};
use crate::registry::Source;
use crate::sources::{SearchCaps, SearchQuery};

pub fn run(args: &SearchArgs, global: &GlobalOpts) -> CommandResult {
    let entry = args.source.entry();
    let caps = entry
        .caps
        .search
        .ok_or_else(|| unsupported_search(args.source))?;

    let query = args.to_query();
    check_query(&query)?;
    check_limit(&query, entry.caps.max_limit, args.source.name())?;
    check_filters(&query, &caps, args.source.name())?;

    let search = entry
        .search
        .ok_or_else(|| failed(format!("{} has no search implementation", entry.name)))?;

    let papers = search(&args.source.base_url(), &query).map_err(CommandError::Failed)?;
    emit(&render(&papers, global.format), args.output.as_deref())
}

fn unsupported_search(source: Source) -> CommandError {
    let hint = match source {
        Source::Unpaywall => "\nUnpaywall resolves a DOI to its open access copy. Try: fastpaper get unpaywall <DOI>",
        _ => "",
    };
    failed(format!(
        "'{}' does not support search.{}",
        source.name(),
        hint
    ))
}

/// Reject a blank query.
///
/// Sources that compose filters into the query string produce a malformed one
/// when the search term is empty: hal returns an unparseable body, doaj and
/// arxiv return HTTP 400, and zenodo and europepmc quietly ignore the filters
/// and answer with unrelated papers. None of that is worth passing along.
fn check_query(query: &SearchQuery) -> CommandResult {
    if query.query.trim().is_empty() {
        return Err(failed(
            "Search query is empty.\n\
             To list a single author's papers, pair a term with --author, or use \
             `fastpaper get <source> <id>` when you already have an identifier.",
        ));
    }
    Ok(())
}

/// Reject a limit the source will reject anyway, but say what the ceiling is.
fn check_limit(query: &SearchQuery, max_limit: Option<u32>, name: &str) -> CommandResult {
    match max_limit {
        Some(max) if query.limit > max => Err(failed(format!(
            "{} returns at most {} results per request (asked for {})",
            name, max, query.limit
        ))),
        _ => Ok(()),
    }
}

/// Reject filters the source cannot honour rather than dropping them silently:
/// a filter that vanishes produces results that look right and are not.
fn check_filters(query: &SearchQuery, caps: &SearchCaps, name: &str) -> CommandResult {
    let unsupported: Vec<&str> = query
        .active_filters()
        .into_iter()
        .filter(|flag| !caps.supports(flag))
        .collect();

    if unsupported.is_empty() {
        return Ok(());
    }

    let supported = caps.supported_flags();
    let available = if supported.is_empty() {
        "This source supports no search filters.".to_string()
    } else {
        format!("Supported here: {}", supported.join(", "))
    };

    Err(failed(format!(
        "{} does not support {}.\n{}",
        name,
        unsupported.join(", "),
        available
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps_none() -> SearchCaps {
        SearchCaps::BASIC
    }

    fn caps_year_only() -> SearchCaps {
        SearchCaps {
            year: true,
            ..SearchCaps::BASIC
        }
    }

    #[test]
    fn plain_query_passes_a_source_with_no_filters() {
        let q = SearchQuery::simple("attention", 10);
        assert!(check_filters(&q, &caps_none(), "dblp").is_ok());
    }

    #[test]
    fn unsupported_filter_is_rejected_by_name() {
        let mut q = SearchQuery::simple("attention", 10);
        q.year = Some(2024);
        let err = check_filters(&q, &caps_none(), "dblp").unwrap_err();
        assert!(err.message().contains("--year"), "got: {}", err.message());
        assert!(err.message().contains("dblp"), "got: {}", err.message());
    }

    #[test]
    fn error_lists_the_filters_that_do_work() {
        let mut q = SearchQuery::simple("attention", 10);
        q.author = Some("Vaswani".into());
        let err = check_filters(&q, &caps_year_only(), "arxiv").unwrap_err();
        assert!(
            err.message().contains("Supported here: --year"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn error_says_so_when_no_filter_is_supported() {
        let mut q = SearchQuery::simple("attention", 10);
        q.open_access = true;
        let err = check_filters(&q, &caps_none(), "dblp").unwrap_err();
        assert!(
            err.message().contains("supports no search filters"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn supported_filter_passes() {
        let mut q = SearchQuery::simple("attention", 10);
        q.year = Some(2024);
        assert!(check_filters(&q, &caps_year_only(), "arxiv").is_ok());
    }

    #[test]
    fn all_unsupported_filters_are_reported_at_once() {
        let mut q = SearchQuery::simple("attention", 10);
        q.year = Some(2024);
        q.author = Some("Vaswani".into());
        q.open_access = true;
        let err = check_filters(&q, &caps_none(), "dblp").unwrap_err();
        for flag in ["--year", "--author", "--open-access"] {
            assert!(err.message().contains(flag), "missing {}: {}", flag, err.message());
        }
    }

    #[test]
    fn blank_query_is_rejected() {
        for blank in ["", "   ", "\t"] {
            let err = check_query(&SearchQuery::simple(blank, 10)).unwrap_err();
            assert!(
                err.message().contains("empty"),
                "{:?} should be rejected: {}",
                blank,
                err.message()
            );
        }
    }

    #[test]
    fn a_real_query_passes() {
        assert!(check_query(&SearchQuery::simple("attention", 10)).is_ok());
    }

    #[test]
    fn limit_within_cap_passes() {
        let q = SearchQuery::simple("attention", 25);
        assert!(check_limit(&q, Some(25), "zenodo").is_ok());
    }

    #[test]
    fn limit_above_cap_names_the_ceiling() {
        let q = SearchQuery::simple("attention", 50);
        let err = check_limit(&q, Some(25), "zenodo").unwrap_err();
        assert!(err.message().contains("25"), "got: {}", err.message());
        assert!(err.message().contains("50"), "got: {}", err.message());
    }

    #[test]
    fn limit_is_unchecked_when_the_source_has_no_cap() {
        let q = SearchQuery::simple("attention", 100_000);
        assert!(check_limit(&q, None, "biorxiv").is_ok());
    }

    #[test]
    fn unpaywall_search_error_points_at_get() {
        let err = unsupported_search(Source::Unpaywall);
        assert!(
            err.message().contains("fastpaper get unpaywall"),
            "got: {}",
            err.message()
        );
    }
}
