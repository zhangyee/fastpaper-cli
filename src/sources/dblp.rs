//! dblp, searched through its SPARQL service.
//!
//! In September 2026 dblp.org put an Anubis proof-of-work challenge in front of
//! every page and API, mirrors included: `/search/publ/api` answers HTTP 200
//! with an HTML page titled "Making sure you're not a bot!", which the old XML
//! parser reported as `ill-formed document: expected '</meta>'`. Passing the
//! challenge means running its JavaScript, so the search API is closed to a CLI.
//!
//! dblp's own SPARQL service (sparql.dblp.org, a QLever instance) is not behind
//! the challenge, and its text index answers a title-word search in well under
//! a second.

use super::Paper;

/// Kept from the search API this replaces, so paging deep into a common word
/// still stops somewhere.
const MAX_OFFSET: u32 = 999_970;

const USER_AGENT: &str = concat!(
    "fastpaper-cli/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/zhangyee/fastpaper-cli)"
);

/// Shortest word matched as a prefix unless the caller asks with `*`.
const MIN_PREFIX_CHARS: usize = 3;

/// Field prefixes the old search API understood inside the query string.
const FIELD_PREFIXES: [&str; 5] = ["year:", "author:", "venue:", "type:", "stream:"];

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    if q.offset > MAX_OFFSET {
        return Err(format!(
            "dblp supports an offset up to {} (asked for {})",
            MAX_OFFSET, q.offset
        ));
    }
    let url = format!(
        "{}/sparql?query={}",
        base_url.trim_end_matches('/'),
        super::encode_query(&build_query(q)?)
    );
    parse_search_response(&http_get(&url)?)
}

/// Build the SPARQL query for a search.
///
/// Every query word has to appear in the title. The index has no relevance
/// score worth ranking by -- its score counts repeated words, so it favours
/// titles that say "all you need" twice -- so records are ranked by how little
/// else the title says: the shortest title holding every word first, the
/// newest first among equals. The records are chosen and paged in the inner
/// select, and ordered again outside it because the joins that fetch venue,
/// DOI and authors do not keep the order.
///
/// Only records with a byline are searched. Fetching authors in an OPTIONAL
/// costs the service two seconds a query, and splitting that OPTIONAL up is
/// wrong, not just slow: a record with no signature leaves the join variable
/// unbound and collects other papers' authors. Requiring a signature up front
/// keeps the byline join plain and pages full; what it drops is the 0.8% of
/// dblp that has none -- anonymous front matter, interviews, biographies.
fn build_query(q: &super::SearchQuery) -> Result<String, String> {
    let words = title_words(&q.query)?;

    let mut filters = String::new();
    if let Some(year) = q.year {
        filters.push_str(&format!(
            "      ?pub dblp:yearOfPublication \"{}\"^^xsd:gYear .\n",
            year
        ));
    }
    if let Some(ref author) = q.author {
        filters.push_str(&format!(
            "      ?pub dblp:hasSignature ?as . ?as dblp:signatureDblpName ?an .\n      \
             FILTER(CONTAINS(LCASE(?an), {}))\n",
            sparql_string(&author.to_lowercase())
        ));
    }

    Ok(format!(
        "PREFIX dblp: <https://dblp.org/rdf/schema#>\n\
         PREFIX ql: <http://qlever.cs.uni-freiburg.de/builtin-functions/>\n\
         PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\n\
         SELECT ?pub ?title ?year ?venue ?doi ?ordinal ?name WHERE {{\n  \
           {{ SELECT DISTINCT ?pub ?title ?year WHERE {{\n      \
               ?pub dblp:title ?title .\n      \
               ?t ql:contains-entity ?title .\n      \
               ?t ql:contains-word {words} .\n      \
               ?pub dblp:hasSignature ?any .\n      \
               OPTIONAL {{ ?pub dblp:yearOfPublication ?year }}\n\
         {filters}    \
           }} ORDER BY STRLEN(?title) DESC(?year) LIMIT {limit} OFFSET {offset} }}\n  \
           OPTIONAL {{ ?pub dblp:publishedIn ?venue }}\n  \
           OPTIONAL {{ ?pub dblp:doi ?doi }}\n  \
           ?pub dblp:hasSignature ?s . ?s dblp:signatureOrdinal ?ordinal ; dblp:signatureDblpName ?name .\n\
         }} ORDER BY STRLEN(?title) DESC(?year) ?pub ?ordinal\n",
        words = sparql_string(&words),
        filters = filters,
        limit = q.limit,
        offset = q.offset,
    ))
}

/// Turn the caller's query into the words the title index holds.
///
/// The index splits titles on punctuation and ignores case, so `KV-Cache`
/// is the two words `kv cache`. dblp's own search matches every word as a
/// prefix and takes a trailing `$` to mean the exact word; the same holds
/// here, except that one- and two-letter words stay exact -- as prefixes they
/// match a large share of all titles and take the service seconds to expand.
/// An explicit trailing `*` is honoured whatever the length.
fn title_words(query: &str) -> Result<String, String> {
    if let Some(token) = query.split_whitespace().find(|t| {
        let t = t.to_lowercase();
        FIELD_PREFIXES.iter().any(|p| t.starts_with(p))
    }) {
        return Err(format!(
            "dblp does not take field syntax in the query ('{}'); it searches title words.\n\
             Use --year <YYYY> or --author \"<name>\" instead; venues cannot be filtered.",
            token
        ));
    }

    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric() && c != '*' && c != '$')
        .filter_map(|token| {
            let core: String = token
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            let prefix = token.ends_with('*')
                || (!token.ends_with('$') && core.chars().count() >= MIN_PREFIX_CHARS);
            match (core.is_empty(), prefix) {
                (true, _) => None,
                (false, true) => Some(core + "*"),
                (false, false) => Some(core),
            }
        })
        .collect();

    if words.is_empty() {
        return Err(format!(
            "dblp searches title words, and '{}' has none",
            query
        ));
    }
    Ok(words.join(" "))
}

/// Quote a value as a SPARQL string literal.
fn sparql_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// GET with a retry on 429.
///
/// The service rate-limits "aggressive scripting" and says so. It answers a
/// query it cannot run with an `exception` message, which is worth passing
/// on. An HTML body is what broke the search API this replaced, so it is
/// named rather than handed to the JSON parser.
fn http_get(url: &str) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();

    for attempt in 0..3u32 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(u64::from(attempt)));
        }
        let resp = agent
            .get(url)
            .header("Accept", "application/sparql-results+json")
            .header("User-Agent", USER_AGENT)
            .call()
            .map_err(|e| format!("HTTP error: {}", e))?;
        let status = resp.status().as_u16();
        let html = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/html"));
        let body = resp
            .into_body()
            .read_to_string()
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if status == 429 {
            continue;
        }
        if html || body.trim_start().starts_with('<') {
            return Err(format!(
                "dblp's SPARQL service answered HTTP {} with an HTML page instead of results \
                 (a bot challenge or an outage page). Search CS literature on semantic or \
                 openalex instead.",
                status
            ));
        }
        if !(200..300).contains(&status) {
            let reason = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["exception"].as_str().map(str::to_string))
                .unwrap_or_default();
            return Err(format!("dblp SPARQL error {}: {}", status, reason)
                .trim_end_matches([':', ' '])
                .to_string());
        }
        return Ok(body);
    }
    Err("dblp rate limited (429) after 3 attempts".to_string())
}

/// Parse a SPARQL JSON result into one `Paper` per dblp record.
///
/// The result has a row per author, so rows are folded by record, keeping the
/// order the query ranked the records in.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;
    let rows = root["results"]["bindings"]
        .as_array()
        .ok_or("missing 'results.bindings' array")?;

    let value = |row: &serde_json::Value, var: &str| row[var]["value"].as_str().map(str::to_string);

    let mut papers: Vec<Paper> = Vec::new();
    let mut bylines: Vec<Vec<(u32, String)>> = Vec::new();
    for row in rows {
        let Some(record) = value(row, "pub") else {
            continue;
        };
        let key = record
            .strip_prefix("https://dblp.org/rec/")
            .unwrap_or(&record)
            .to_string();

        let i = match papers.iter().position(|p| p.id == key) {
            Some(i) => i,
            None => {
                papers.push(Paper {
                    id: key,
                    title: value(row, "title").unwrap_or_default().trim().to_string(),
                    authors: Vec::new(),
                    abstract_text: None,
                    year: value(row, "year").and_then(|y| y.parse::<u16>().ok()),
                    doi: value(row, "doi")
                        .map(|d| d.strip_prefix("https://doi.org/").unwrap_or(&d).to_string()),
                    url: Some(record.clone()),
                    pdf_url: None,
                    venue: value(row, "venue"),
                    citations: None,
                    fields: vec![],
                    open_access: None,
                    source: "dblp".to_string(),
                });
                bylines.push(Vec::new());
                papers.len() - 1
            }
        };

        if let (Some(ordinal), Some(name)) = (
            value(row, "ordinal").and_then(|o| o.parse::<u32>().ok()),
            value(row, "name"),
        ) {
            let entry = (ordinal, without_homonym_number(&name).to_string());
            if !bylines[i].contains(&entry) {
                bylines[i].push(entry);
            }
        }
    }

    for (paper, mut byline) in papers.iter_mut().zip(bylines) {
        byline.sort_by_key(|(ordinal, _)| *ordinal);
        paper.authors = byline.into_iter().map(|(_, name)| name).collect();
    }
    papers.retain(|p| !p.title.is_empty());
    Ok(papers)
}

/// dblp tells homonyms apart with a four-digit number ("Jian Sun 0001").
fn without_homonym_number(name: &str) -> &str {
    match name.rsplit_once(' ') {
        Some((rest, tail)) if tail.len() == 4 && tail.chars().all(|c| c.is_ascii_digit()) => rest,
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::SearchQuery;

    const FIXTURE: &str = include_str!("../../tests/fixtures/dblp_sparql.json");

    fn papers() -> Vec<Paper> {
        parse_search_response(FIXTURE).unwrap()
    }

    // The result has one row per author; a record is one paper.
    #[test]
    fn rows_are_grouped_into_one_paper_per_record() {
        assert_eq!(papers().len(), 3);
    }

    #[test]
    fn papers_keep_the_order_the_query_ranked_them_in() {
        let ids: Vec<String> = papers().into_iter().map(|p| p.id).collect();
        assert_eq!(
            ids,
            [
                "conf/cvpr/HeZRS16",
                "journals/corr/HeZRS15",
                "journals/corr/abs-2211-12320"
            ]
        );
    }

    #[test]
    fn authors_come_in_byline_order() {
        assert_eq!(papers()[0].authors[0], "Kaiming He");
        assert_eq!(papers()[0].authors.len(), 4);
        assert_eq!(papers()[0].authors[3], "Jian Sun");
    }

    // dblp tells homonyms apart with a number ("Jian Sun 0001"); every other
    // source gives the plain name.
    #[test]
    fn homonym_numbers_are_dropped_from_names() {
        assert_eq!(papers()[0].authors[1], "Xiangyu Zhang");
    }

    #[test]
    fn doi_loses_its_resolver_prefix() {
        assert_eq!(papers()[0].doi.as_deref(), Some("10.1109/CVPR.2016.90"));
        assert_eq!(papers()[1].doi, None);
    }

    #[test]
    fn id_is_the_dblp_key_and_url_its_record_page() {
        let p = &papers()[0];
        assert_eq!(p.id, "conf/cvpr/HeZRS16");
        assert_eq!(
            p.url.as_deref(),
            Some("https://dblp.org/rec/conf/cvpr/HeZRS16")
        );
    }

    #[test]
    fn year_venue_and_title_are_filled() {
        let p = &papers()[0];
        assert_eq!(p.year, Some(2016));
        assert_eq!(p.venue.as_deref(), Some("CVPR"));
        assert_eq!(p.title, "Deep Residual Learning for Image Recognition.");
        assert_eq!(p.source, "dblp");
    }

    // dblp is bibliographic metadata: no abstracts, files or citation counts.
    #[test]
    fn metadata_only() {
        for p in papers() {
            assert!(p.abstract_text.is_none());
            assert!(p.pdf_url.is_none());
            assert!(p.citations.is_none());
        }
    }

    #[test]
    fn an_empty_result_is_no_papers() {
        let empty = r#"{"head":{"vars":["pub"]},"results":{"bindings":[]}}"#;
        assert!(parse_search_response(empty).unwrap().is_empty());
    }

    // ── query building ──────────────────────────

    fn query(text: &str) -> String {
        build_query(&SearchQuery::simple(text, 10)).unwrap()
    }

    // The text index splits titles on punctuation and ignores case, so the
    // words are handed over the same way.
    #[test]
    fn the_query_becomes_lowercase_title_words() {
        assert!(
            query("KV-Cache Compression").contains(r#"ql:contains-word "kv cache* compression*""#),
            "got: {}",
            query("KV-Cache Compression")
        );
    }

    // dblp's own search matches every word as a prefix, so "network" finds
    // "Networks". An exact-word index would miss every plural.
    #[test]
    fn words_match_as_prefixes_like_dblps_own_search() {
        assert!(query("graph neural network").contains(r#""graph* neural* network*""#));
    }

    // dblp's way to ask for the word itself.
    #[test]
    fn a_dollar_sign_asks_for_the_exact_word() {
        assert!(query("graph$ theory").contains(r#""graph theory*""#));
    }

    // A one- or two-letter prefix matches a large share of all titles and
    // takes the service seconds to expand.
    #[test]
    fn short_words_are_matched_exactly() {
        assert!(query("is ai").contains(r#""is ai""#));
    }

    #[test]
    fn a_trailing_star_keeps_prefix_matching() {
        assert!(query("transform*").contains(r#""transform*""#));
    }

    #[test]
    fn limit_and_offset_page_the_records() {
        let mut q = SearchQuery::simple("graph", 20);
        q.offset = 40;
        let s = build_query(&q).unwrap();
        assert!(s.contains("LIMIT 20 OFFSET 40"), "got: {}", s);
    }

    #[test]
    fn year_matches_dblps_typed_year() {
        let mut q = SearchQuery::simple("graph", 10);
        q.year = Some(2023);
        assert!(build_query(&q).unwrap().contains(r#""2023"^^xsd:gYear"#));
    }

    #[test]
    fn author_is_matched_within_dblp_names() {
        let mut q = SearchQuery::simple("graph", 10);
        q.author = Some("Jure Leskovec".into());
        assert!(build_query(&q).unwrap().contains(r#""jure leskovec""#));
    }

    // The author name is the one free-form string that reaches the query.
    #[test]
    fn an_author_name_cannot_break_out_of_its_string() {
        let mut q = SearchQuery::simple("graph", 10);
        q.author = Some(r#"o"brien\"#.into());
        assert!(build_query(&q).unwrap().contains(r#""o\"brien\\""#));
    }

    // The old API took `year:` / `author:` / `venue:` inside the query; here
    // they would silently become title words and match nothing.
    #[test]
    fn old_field_syntax_is_rejected_with_the_flags_to_use() {
        let err = build_query(&SearchQuery::simple("year:2020 transformers", 10)).unwrap_err();
        assert!(err.contains("--year"), "got: {}", err);
        assert!(err.contains("--author"), "got: {}", err);
    }

    // A colon inside a pasted title is not field syntax.
    #[test]
    fn a_title_with_a_colon_is_still_searched() {
        assert!(query("BERT: pre-training").contains(r#""bert* pre* training*""#));
    }

    // An OPTIONAL byline join costs the service two seconds, and splitting it
    // into separate OPTIONALs is worse: a record with no signature leaves the
    // join variable unbound and collects other papers' authors. So only
    // records with a byline are searched (0.8% of dblp has none: anonymous
    // front matter, interviews), and the byline join is a plain one.
    #[test]
    fn only_records_with_a_byline_are_searched() {
        let s = query("graph");
        assert!(s.contains("?pub dblp:hasSignature ?any ."), "got: {}", s);
        assert!(
            !s.contains("OPTIONAL { ?pub dblp:hasSignature"),
            "got: {}",
            s
        );
    }

    #[test]
    fn a_query_with_no_words_is_rejected() {
        assert!(build_query(&SearchQuery::simple("?!", 10)).is_err());
    }

    #[test]
    fn offset_past_dblps_cap_is_rejected() {
        let mut q = SearchQuery::simple("graph", 10);
        q.offset = MAX_OFFSET + 1;
        assert!(search("http://unused", &q).unwrap_err().contains("offset"));
    }

    // ── requests ────────────────────────────────

    #[test]
    fn search_asks_the_sparql_endpoint_for_json() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("GET", "/sparql")
            .match_query(mockito::Matcher::Regex("contains-word".into()))
            .match_header("accept", "application/sparql-results+json")
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let got = search(&server.url(), &SearchQuery::simple("residual learning", 3)).unwrap();
        assert_eq!(got.len(), 3);
        m.assert();
    }

    // What broke the old source: a bot challenge served as a 200 page. If it
    // ever reaches this endpoint too, say so instead of failing to parse.
    #[test]
    fn an_html_page_is_named_as_such() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "text/html; charset=utf-8")
            .with_body(
                "<!doctype html><html><head><title>Making sure you&#39;re not a bot!</title>",
            )
            .create();
        let err = search(&server.url(), &SearchQuery::simple("graph", 3)).unwrap_err();
        assert!(err.contains("HTML"), "got: {}", err);
    }

    #[test]
    fn a_429_is_retried() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(429)
            .expect(1)
            .create();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        assert!(search(&server.url(), &SearchQuery::simple("graph", 3)).is_ok());
    }

    // QLever explains a rejected query in an `exception` field.
    #[test]
    fn a_rejected_query_reports_the_services_reason() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(400)
            .with_body(r#"{"exception":"Invalid SPARQL query: token recognition error"}"#)
            .create();
        let err = search(&server.url(), &SearchQuery::simple("graph", 3)).unwrap_err();
        assert!(err.contains("token recognition error"), "got: {}", err);
    }
}
