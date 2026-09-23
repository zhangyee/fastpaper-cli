use quick_xml::Reader;
use quick_xml::events::Event;

use super::Paper;

const ESEARCH_URL: &str = "/entrez/eutils/esearch.fcgi";
const EFETCH_URL: &str = "/entrez/eutils/efetch.fcgi";
const MAX_ATTEMPTS: usize = 3;

/// Build the esearch URL for a full query.
///
/// PubMed expresses author and year filters as Entrez field tags inside `term`
/// (`Doudna[au]`, `2017[dp]`) while date ranges get their own
/// `datetype`/`mindate`/`maxdate` parameters.
fn build_esearch_url_q(
    base_url: &str,
    q: &super::SearchQuery,
    api_key: Option<&str>,
    email: Option<&str>,
) -> Result<String, String> {
    let mut term = super::encode_query(&q.query);
    if let Some(ref author) = q.author {
        term.push_str(&format!("+AND+{}%5Bau%5D", super::encode_query(author)));
    }
    if let Some(year) = q.year {
        term.push_str(&format!("+AND+{}%5Bdp%5D", year));
    }

    let mut url = format!(
        "{}{}?db=pubmed&term={}&retmax={}&retstart={}&retmode=json&tool=fastpaper",
        base_url, ESEARCH_URL, term, q.limit, q.offset
    );

    // Only meaningful when --year was not used; the two express the same thing.
    if q.year.is_none() && (q.after.is_some() || q.before.is_some()) {
        url.push_str("&datetype=pdat");
        let min = match q.after {
            Some(ref d) => super::validate_ymd(d)?.replace('-', "/"),
            None => "1800/01/01".to_string(),
        };
        let max = match q.before {
            Some(ref d) => super::validate_ymd(d)?.replace('-', "/"),
            None => "3000/12/31".to_string(),
        };
        url.push_str(&format!("&mindate={}&maxdate={}", min, max));
    }

    if let Some(sort) = q.sort {
        let by = match sort {
            super::SortField::Relevance => "relevance",
            super::SortField::Date => "pub_date",
            super::SortField::Citations => {
                return Err(
                    "pubmed cannot sort by citations: E-utilities offers no citation ordering.\n\
                     Try semantic or openalex."
                        .to_string(),
                );
            }
        };
        url.push_str(&format!("&sort={}", by));
    }

    if let Some(email) = email {
        url.push_str(&format!("&email={}", email));
    }
    if let Some(key) = api_key {
        url.push_str(&format!("&api_key={}", key));
    }
    Ok(url)
}

fn build_efetch_url(
    base_url: &str,
    ids: &str,
    api_key: Option<&str>,
    email: Option<&str>,
) -> String {
    let mut url = format!(
        "{}{}?db=pubmed&id={}&retmode=xml&tool=fastpaper",
        base_url, EFETCH_URL, ids
    );
    if let Some(email) = email {
        url.push_str(&format!("&email={}", email));
    }
    if let Some(key) = api_key {
        url.push_str(&format!("&api_key={}", key));
    }
    url
}

/// Search PubMed: esearch for IDs, then efetch for details.
pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let api_key = std::env::var("NCBI_API_KEY").ok();
    let email = super::contact_email();

    // Step 1: esearch to get PMID list
    let esearch_url = build_esearch_url_q(base_url, q, api_key.as_deref(), email.as_deref())?;

    let ids = send(
        "PubMed esearch",
        || {
            crate::http::api()
                .get(&esearch_url)
                .config()
                .http_status_as_error(false)
                .build()
                .call()
        },
        parse_esearch_response,
    )?;

    if ids.is_empty() {
        return Ok(vec![]);
    }

    // Step 2: efetch to get article details
    let url = format!("{}{}", base_url, EFETCH_URL);
    let mut papers = Vec::new();
    for batch in ids.chunks(EFETCH_BATCH) {
        let form = efetch_form(batch, api_key.as_deref(), email.as_deref());
        papers.extend(send(
            "PubMed efetch",
            || {
                crate::http::api()
                    .post(&url)
                    .config()
                    .http_status_as_error(false)
                    .build()
                    .send_form(form.clone())
            },
            parse_efetch_response,
        )?);
    }
    // Keep esearch's ranking whatever order efetch answers in.
    papers.sort_by_key(|p| ids.iter().position(|id| *id == p.id).unwrap_or(usize::MAX));
    Ok(papers)
}

fn parse_esearch_response(json: &str) -> Result<Vec<String>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;
    let ids = root["esearchresult"]["idlist"]
        .as_array()
        .ok_or("missing 'esearchresult.idlist' array")?;
    Ok(ids
        .iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect())
}

/// PMIDs per efetch request.
///
/// Every PMID used to ride in the GET URL, and NCBI answered 414 from about
/// 500 of them (past ~7000 the URL is too long to send at all). NCBI asks for
/// POST beyond about 200 UIDs. The batch size keeps each answer inside the
/// request limits: 200 records measured 3.5 MB and 5.5 s, where 1000 in one
/// request were 17 MB (over the 10 MB read limit) and 39 s.
const EFETCH_BATCH: usize = 200;

/// Form fields for one efetch POST.
fn efetch_form(
    ids: &[String],
    api_key: Option<&str>,
    email: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut form = vec![
        ("db", "pubmed".to_string()),
        ("id", ids.join(",")),
        ("retmode", "xml".to_string()),
        ("tool", "fastpaper".to_string()),
    ];
    if let Some(email) = email {
        form.push(("email", email.to_string()));
    }
    if let Some(key) = api_key {
        form.push(("api_key", key.to_string()));
    }
    form
}

/// Send and validate a request, backing off on transient gateway responses and
/// syntactically invalid or structurally incomplete 2xx bodies.
fn send<T>(
    stage: &str,
    request: impl Fn() -> Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    parse: impl Fn(&str) -> Result<T, String>,
) -> Result<T, String> {
    let mut last_retryable = None;
    for attempt in 0..MAX_ATTEMPTS {
        let resp = request().map_err(|e| format!("{} HTTP error: {}", stage, e))?;
        let status = resp.status().as_u16();
        let body = match resp.into_body().read_to_string() {
            Ok(body) => body,
            Err(e) if retryable_status(status) => {
                last_retryable = Some(format!(
                    "{} returned HTTP {} but its response could not be read: {}",
                    stage, status, e
                ));
                if attempt + 1 < MAX_ATTEMPTS {
                    retry_pause(attempt);
                    continue;
                }
                break;
            }
            Err(e) => return Err(format!("{} failed to read response: {}", stage, e)),
        };

        match status {
            200..=299 => match parse(&body) {
                Ok(value) => return Ok(value),
                Err(err) => {
                    last_retryable = Some(format!(
                        "{} returned HTTP {} without a valid success response: {}; response: {}",
                        stage,
                        status,
                        err,
                        diagnostic_summary(&body)
                    ));
                }
            },
            429 => {
                last_retryable = Some(format!(
                    "{} was rate limited (429); response: {}",
                    stage,
                    diagnostic_summary(&body)
                ));
            }
            502..=504 => {
                last_retryable = Some(format!(
                    "{} returned HTTP {}; response: {}",
                    stage,
                    status,
                    diagnostic_summary(&body)
                ));
            }
            500..=599 => {
                return Err(format!(
                    "{} server error {}; response: {}",
                    stage,
                    status,
                    diagnostic_summary(&body)
                ));
            }
            _ => {
                return Err(format!(
                    "{} HTTP error {}; response: {}",
                    stage,
                    status,
                    diagnostic_summary(&body)
                ));
            }
        }

        if attempt + 1 < MAX_ATTEMPTS {
            retry_pause(attempt);
        }
    }

    Err(format!(
        "{} after {} attempts",
        last_retryable.unwrap_or_else(|| format!("{} failed", stage)),
        MAX_ATTEMPTS
    ))
}

fn retryable_status(status: u16) -> bool {
    matches!(status, 429 | 502..=504)
}

fn retry_pause(attempt: usize) {
    std::thread::sleep(std::time::Duration::from_millis(200 * (1_u64 << attempt)));
}

fn diagnostic_summary(body: &str) -> String {
    const LIMIT: usize = 512;
    let head: String = body.chars().take(LIMIT).collect();
    let mut summary = head.split_whitespace().collect::<Vec<_>>().join(" ");
    if summary.is_empty() {
        summary.push_str("<empty body>");
    }
    if body.chars().nth(LIMIT).is_some() {
        summary.push('…');
    }
    summary
}

/// Fetch a single paper by PMID.
pub fn get_by_pmid(base_url: &str, pmid: &str) -> Result<Option<Paper>, String> {
    let api_key = std::env::var("NCBI_API_KEY").ok();
    let email = super::contact_email();
    let url = build_efetch_url(base_url, pmid, api_key.as_deref(), email.as_deref());
    let papers = send(
        "PubMed efetch",
        || {
            crate::http::api()
                .get(&url)
                .config()
                .http_status_as_error(false)
                .build()
                .call()
        },
        parse_efetch_response,
    )?;
    Ok(papers.into_iter().next())
}

/// Parse PubMed efetch XML response into a list of Papers.
pub fn parse_efetch_response(xml: &str) -> Result<Vec<Paper>, String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut papers = Vec::new();
    let mut saw_article_set = false;

    // State
    let mut in_article = false;
    let mut in_author = false;
    let mut tag_stack: Vec<String> = Vec::new();
    let mut pmid = String::new();
    let mut title = String::new();
    let mut authors: Vec<String> = Vec::new();
    let mut last_name = String::new();
    let mut initials = String::new();
    let mut abstract_text = String::new();
    let mut year = String::new();
    let mut doi = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    "PubmedArticleSet" => saw_article_set = true,
                    "PubmedArticle" => {
                        in_article = true;
                        pmid.clear();
                        title.clear();
                        authors.clear();
                        abstract_text.clear();
                        year.clear();
                        doi.clear();
                    }
                    "Author" if in_article => {
                        in_author = true;
                        last_name.clear();
                        initials.clear();
                    }
                    "ELocationID" if in_article => {
                        // Check EIdType attribute
                        let mut is_doi = false;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"EIdType" && attr.value.as_ref() == b"doi" {
                                is_doi = true;
                            }
                        }
                        if is_doi {
                            tag_stack.push("doi".to_string());
                        } else {
                            tag_stack.push(local.to_string());
                        }
                        buf.clear();
                        continue;
                    }
                    _ => {}
                }
                tag_stack.push(local.to_string());
            }
            Ok(Event::Text(ref e)) => {
                if !in_article {
                    buf.clear();
                    continue;
                }
                let text = e.decode().unwrap_or_default().to_string();
                let current = tag_stack.last().map(|s| s.as_str()).unwrap_or("");
                match current {
                    "PMID" if pmid.is_empty() => pmid.push_str(text.trim()),
                    "ArticleTitle" => title.push_str(&text),
                    "LastName" if in_author => last_name.push_str(text.trim()),
                    "Initials" if in_author => initials.push_str(text.trim()),
                    "AbstractText" => {
                        if !abstract_text.is_empty() {
                            abstract_text.push(' ');
                        }
                        abstract_text.push_str(text.trim());
                    }
                    // Only the <Year> inside <PubDate>. DateCompleted and
                    // DateRevised are NLM's own processing dates and appear
                    // earlier in the record, so matching any <Year> reports
                    // when NLM indexed the article, not when it was published.
                    "Year"
                        if year.is_empty()
                            && tag_stack
                                .iter()
                                .rev()
                                .nth(1)
                                .map(|parent| parent == "PubDate")
                                .unwrap_or(false) =>
                    {
                        year.push_str(text.trim())
                    }
                    "doi" => doi.push_str(text.trim()),
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    "PubmedArticle" => {
                        if !pmid.is_empty() && !title.is_empty() {
                            papers.push(Paper {
                                id: pmid.clone(),
                                title: title.trim().to_string(),
                                authors: authors.clone(),
                                abstract_text: if abstract_text.is_empty() {
                                    None
                                } else {
                                    Some(abstract_text.clone())
                                },
                                year: year.parse::<u16>().ok(),
                                doi: if doi.is_empty() {
                                    None
                                } else {
                                    Some(doi.clone())
                                },
                                url: Some(format!("https://pubmed.ncbi.nlm.nih.gov/{}/", pmid)),
                                pdf_url: None,
                                venue: None,
                                citations: None,
                                fields: vec![],
                                open_access: None,
                                source: "pubmed".to_string(),
                                community: None,
                            });
                        }
                        in_article = false;
                    }
                    "Author" if in_author => {
                        let name_str = format!("{} {}", last_name, initials).trim().to_string();
                        if !name_str.is_empty() {
                            authors.push(name_str);
                        }
                        in_author = false;
                    }
                    _ => {}
                }
                tag_stack.pop();
            }
            Ok(Event::Empty(ref e)) if local_name(e.name().as_ref()) == "PubmedArticleSet" => {
                saw_article_set = true;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    if !saw_article_set {
        return Err("missing 'PubmedArticleSet' root element".to_string());
    }
    Ok(papers)
}

fn local_name(name: &[u8]) -> &str {
    let s = std::str::from_utf8(name).unwrap_or("");
    s.rsplit(':').next().unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    const FIXTURE: &str = include_str!("../../tests/fixtures/pubmed_efetch.xml");

    #[test]
    fn parse_returns_ok() {
        let result = parse_efetch_response(FIXTURE);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_papers_not_empty() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        assert!(!papers.is_empty());
    }

    #[test]
    fn parse_source_is_pubmed() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "pubmed");
        }
    }

    #[test]
    fn parse_title_from_article_title() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_authors_lastname_initials() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
        for p in &with_authors {
            for a in &p.authors {
                assert!(!a.is_empty());
                // Should have format "LastName Initials"
                assert!(a.len() > 1, "author name too short: {}", a);
            }
        }
    }

    #[test]
    fn parse_abstract_from_abstract_text() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers
            .iter()
            .filter(|p| p.abstract_text.is_some())
            .collect();
        assert!(!with_abstract.is_empty(), "no papers with abstract");
        for p in &with_abstract {
            assert!(p.abstract_text.as_ref().unwrap().len() > 10);
        }
    }

    #[test]
    fn parse_year_from_pubdate() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.year.is_some(), "paper {} missing year", p.id);
            assert!(p.year.unwrap() >= 2000);
        }
    }

    #[test]
    fn parse_doi_from_elocationid() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        assert!(!with_doi.is_empty(), "no papers with DOI");
        for p in &with_doi {
            let doi = p.doi.as_ref().unwrap();
            assert!(doi.starts_with("10."), "DOI should start with 10.: {}", doi);
        }
    }

    #[test]
    fn parse_id_is_pmid() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(
                p.id.chars().all(|c| c.is_ascii_digit()),
                "PMID should be numeric: {}",
                p.id
            );
        }
    }

    #[test]
    fn parse_pdf_url_is_none() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.pdf_url.is_none(), "pubmed should not have pdf_url");
        }
    }

    #[test]
    fn empty_success_envelopes_are_valid_empty_results() {
        assert!(
            parse_esearch_response(r#"{"esearchresult":{"idlist":[]}}"#)
                .unwrap()
                .is_empty()
        );
        assert!(
            parse_efetch_response("<PubmedArticleSet/>")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn missing_success_envelopes_are_errors() {
        assert!(parse_esearch_response(r#"{"version":"0.3"}"#).is_err());
        assert!(parse_efetch_response("<ERROR>temporary failure</ERROR>").is_err());
    }

    const ESEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/pubmed_esearch.json");

    #[test]
    fn search_calls_esearch_then_efetch() {
        let mut server = mockito::Server::new();
        let esearch_mock = server
            .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .create();
        let efetch_mock = server
            .mock("POST", EFETCH_URL)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let papers = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        )
        .unwrap();
        assert!(!papers.is_empty());
        esearch_mock.assert();
        efetch_mock.assert();
    }

    #[test]
    fn search_retries_a_502_from_esearch_then_succeeds() {
        let mut server = mockito::Server::new();
        let gateway = server
            .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
            .with_status(502)
            .with_body("temporary NCBI gateway failure")
            .expect(1)
            .create();
        let esearch = server
            .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .expect(1)
            .create();
        let efetch = server
            .mock("POST", EFETCH_URL)
            .with_status(200)
            .with_body(FIXTURE)
            .expect(1)
            .create();

        let papers = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        )
        .unwrap();

        assert!(!papers.is_empty());
        gateway.assert();
        esearch.assert();
        efetch.assert();
    }

    #[test]
    fn search_retries_an_incomplete_esearch_2xx_then_succeeds() {
        let mut server = mockito::Server::new();
        let incomplete = server
            .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
            .with_status(200)
            .with_body(r#"{"version":"0.3","trace":"ncbi-incomplete"}"#)
            .expect(1)
            .create();
        let esearch = server
            .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .expect(1)
            .create();
        server
            .mock("POST", EFETCH_URL)
            .with_status(200)
            .with_body(FIXTURE)
            .expect(1)
            .create();

        let papers = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        )
        .unwrap();

        assert!(!papers.is_empty());
        incomplete.assert();
        esearch.assert();
    }

    #[test]
    fn search_retries_a_504_from_efetch_then_succeeds() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .expect(1)
            .create();
        let gateway = server
            .mock("POST", EFETCH_URL)
            .with_status(504)
            .with_body("NCBI efetch timed out")
            .expect(1)
            .create();
        let efetch = server
            .mock("POST", EFETCH_URL)
            .with_status(200)
            .with_body(FIXTURE)
            .expect(1)
            .create();

        let papers = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        )
        .unwrap();

        assert!(!papers.is_empty());
        gateway.assert();
        efetch.assert();
    }

    #[test]
    fn exhausted_gateway_retries_name_the_stage_and_keep_the_summary() {
        let mut server = mockito::Server::new();
        let gateway = server
            .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
            .with_status(503)
            .with_body("NCBI trace ncbi-503-xyz")
            .expect(MAX_ATTEMPTS)
            .create();

        let err = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        )
        .unwrap_err();

        assert!(err.contains("PubMed esearch"), "got: {err}");
        assert!(err.contains("HTTP 503"), "got: {err}");
        assert!(err.contains("ncbi-503-xyz"), "got: {err}");
        assert!(err.contains("after 3 attempts"), "got: {err}");
        gateway.assert();
    }

    #[test]
    fn search_esearch_contains_db_pubmed() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("db=pubmed".to_string()))
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .expect_at_least(1)
            .create();
        // efetch also has db=pubmed, so just mock any other request too
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        );
        mock.assert();
    }

    #[test]
    fn search_esearch_contains_retmax() {
        let mut server = mockito::Server::new();
        let esearch_mock = server
            .mock("GET", mockito::Matcher::Regex("retmax=3".to_string()))
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .create();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        );
        esearch_mock.assert();
    }

    #[test]
    #[serial]
    fn search_with_api_key() {
        unsafe { std::env::set_var("NCBI_API_KEY", "test-ncbi-key") };
        let mut server = mockito::Server::new();
        // Both esearch and efetch should contain api_key
        server
            .mock(
                "GET",
                mockito::Matcher::Regex("esearch.*api_key=test-ncbi-key".to_string()),
            )
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .create();
        server
            .mock("POST", EFETCH_URL)
            .match_body(mockito::Matcher::Regex("api_key=test-ncbi-key".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let result = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        );
        unsafe { std::env::remove_var("NCBI_API_KEY") };
        assert!(
            result.is_ok(),
            "search should succeed with api key: {:?}",
            result.err()
        );
    }

    #[test]
    #[serial]
    fn search_works_without_api_key() {
        unsafe { std::env::remove_var("NCBI_API_KEY") };
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .create();
        server
            .mock("POST", EFETCH_URL)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let result = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        );
        assert!(result.is_ok());
    }

    // Contact info identifies the caller. Sending the maintainer's address on
    // behalf of every user misattributes the traffic and risks getting that
    // address throttled, so omit the parameter unless the user supplies one.
    // NCBI treats `email` as recommended, not required.
    #[test]
    fn build_esearch_url_omits_email_when_none() {
        let url = build_esearch_url_q(
            "https://eutils.ncbi.nlm.nih.gov",
            &crate::sources::SearchQuery::simple("test", 3),
            None,
            None,
        )
        .unwrap();
        assert!(!url.contains("email="), "got: {}", url);
        assert!(url.contains("tool=fastpaper"), "tool should stay: {}", url);
    }

    #[test]
    fn build_esearch_url_includes_email_when_some() {
        let url = build_esearch_url_q(
            "https://eutils.ncbi.nlm.nih.gov",
            &crate::sources::SearchQuery::simple("test", 3),
            None,
            Some("a@b.com"),
        )
        .unwrap();
        assert!(url.contains("email=a@b.com"), "got: {}", url);
    }

    #[test]
    fn build_efetch_url_omits_email_when_none() {
        let url = build_efetch_url("https://eutils.ncbi.nlm.nih.gov", "123,456", None, None);
        assert!(!url.contains("email="), "got: {}", url);
    }

    #[test]
    #[serial]
    fn search_sends_env_email_when_set() {
        unsafe { std::env::set_var("FASTPAPER_EMAIL", "custom@example.com") };
        let mut server = mockito::Server::new();
        let mock = server
            .mock(
                "GET",
                mockito::Matcher::Regex("esearch.*email=custom".to_string()),
            )
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .create();
        server
            .mock("POST", EFETCH_URL)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        );
        unsafe { std::env::remove_var("FASTPAPER_EMAIL") };
        mock.assert();
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use crate::sources::{SearchQuery, SortField};

    fn url(q: &SearchQuery) -> String {
        build_esearch_url_q("https://eutils.ncbi.nlm.nih.gov", q, None, None).unwrap()
    }

    // PubMed carries author and year as Entrez field tags inside `term`.
    #[test]
    fn author_becomes_an_au_field_tag() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.author = Some("Doudna".into());
        let u = url(&q);
        assert!(u.contains("Doudna%5Bau%5D"), "got: {}", u);
        assert!(u.contains("+AND+"), "got: {}", u);
    }

    #[test]
    fn year_becomes_a_dp_field_tag() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.year = Some(2017);
        assert!(url(&q).contains("2017%5Bdp%5D"), "got: {}", url(&q));
    }

    #[test]
    fn offset_becomes_retstart() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.offset = 20;
        assert!(url(&q).contains("retstart=20"));
    }

    #[test]
    fn dates_use_datetype_mindate_maxdate() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.after = Some("2024-01-01".into());
        q.before = Some("2024-03-31".into());
        let u = url(&q);
        assert!(u.contains("datetype=pdat"), "got: {}", u);
        assert!(u.contains("mindate=2024/01/01"), "got: {}", u);
        assert!(u.contains("maxdate=2024/03/31"), "got: {}", u);
    }

    #[test]
    fn year_takes_precedence_over_a_date_range() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.year = Some(2017);
        q.after = Some("2024-01-01".into());
        let u = url(&q);
        assert!(u.contains("2017%5Bdp%5D"), "got: {}", u);
        assert!(
            !u.contains("mindate"),
            "should not also send a range: {}",
            u
        );
    }

    #[test]
    fn malformed_date_is_rejected() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.before = Some("March 2024".into());
        let err =
            build_esearch_url_q("https://eutils.ncbi.nlm.nih.gov", &q, None, None).unwrap_err();
        assert!(err.contains("YYYY-MM-DD"), "got: {}", err);
    }

    #[test]
    fn sort_by_date_uses_pub_date() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.sort = Some(SortField::Date);
        assert!(url(&q).contains("sort=pub_date"));
    }

    #[test]
    fn sort_by_citations_is_rejected_with_alternatives() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.sort = Some(SortField::Citations);
        let err =
            build_esearch_url_q("https://eutils.ncbi.nlm.nih.gov", &q, None, None).unwrap_err();
        assert!(err.contains("semantic"), "got: {}", err);
    }

    #[test]
    fn a_filterless_query_is_unchanged() {
        let u = url(&SearchQuery::simple("crispr", 10));
        assert!(u.contains("term=crispr&"), "got: {}", u);
        assert!(!u.contains("AND"), "no filter terms expected: {}", u);
    }
}

#[cfg(test)]
mod year_tests {
    use super::*;

    // PubMed puts NLM's own processing dates (DateCompleted, DateRevised)
    // *before* the article's PubDate, so taking the first <Year> in the record
    // reports when NLM indexed it rather than when it was published. The
    // recorded fixture happens to have all three equal, which hid this.
    const MIXED_DATES: &str = r#"<?xml version="1.0"?>
<PubmedArticleSet><PubmedArticle><MedlineCitation>
<PMID Version="1">35349206</PMID>
<DateCompleted><Year>2022</Year><Month>04</Month><Day>04</Day></DateCompleted>
<DateRevised><Year>2022</Year><Month>04</Month><Day>05</Day></DateRevised>
<Article PubModel="Print-Electronic"><Journal><JournalIssue CitedMedium="Internet">
<PubDate><Year>2021</Year><Month>Mar</Month></PubDate>
</JournalIssue><Title>Reviews in medical virology</Title></Journal>
<ArticleTitle>SARS-CoV-2: Mechanism of infection.</ArticleTitle>
</Article></MedlineCitation></PubmedArticle></PubmedArticleSet>"#;

    #[test]
    fn year_comes_from_pubdate_not_the_nlm_processing_date() {
        let papers = parse_efetch_response(MIXED_DATES).unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(
            papers[0].year,
            Some(2021),
            "should report the PubDate year, not DateCompleted"
        );
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/pubmed_efetch.xml");
    const ESEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/pubmed_esearch.json");

    fn esearch_with(ids: &[String]) -> String {
        serde_json::json!({ "esearchresult": { "idlist": ids } }).to_string()
    }

    fn efetch_with(pmids: &[&str]) -> String {
        let articles: String = pmids
            .iter()
            .map(|id| {
                format!(
                    "<PubmedArticle><MedlineCitation><PMID Version=\"1\">{id}</PMID>\
                     <Article><ArticleTitle>Paper {id}</ArticleTitle></Article>\
                     </MedlineCitation></PubmedArticle>"
                )
            })
            .collect();
        format!("<PubmedArticleSet>{}</PubmedArticleSet>", articles)
    }

    // Every PMID in a GET URL is what broke -n 500 and up: NCBI answers 414,
    // and past ~7000 ids the URL is too long to even send. NCBI asks for POST
    // beyond about 200 UIDs.
    #[test]
    fn efetch_ids_are_posted_not_packed_into_the_url() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("esearch".into()))
            .with_body(ESEARCH_FIXTURE)
            .create();
        let get = server
            .mock("GET", mockito::Matcher::Regex("efetch".into()))
            .expect(0)
            .create();
        let post = server
            .mock("POST", EFETCH_URL)
            .match_body(mockito::Matcher::Regex("id=".into()))
            .with_body(FIXTURE)
            .create();
        search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        )
        .unwrap();
        post.assert();
        get.assert();
    }

    // A batch of 200 records is ~3.5 MB and ~5.5 s, inside the 10 MB read
    // limit and the 30 s request limit; 1000 in one request is ~17 MB and ~39 s.
    #[test]
    fn a_full_page_is_fetched_in_batches_of_200() {
        let cap = crate::registry::Source::Pubmed.caps().max_limit.unwrap();
        let ids: Vec<String> = (1..=cap).map(|i| i.to_string()).collect();
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("esearch".into()))
            .with_body(esearch_with(&ids))
            .create();
        let post = server
            .mock("POST", EFETCH_URL)
            .with_body(efetch_with(&["1"]))
            .expect(cap.div_ceil(200) as usize)
            .create();
        search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", cap),
        )
        .unwrap();
        post.assert();
    }

    #[test]
    fn records_keep_the_order_esearch_ranked_them_in() {
        let ranked = ["30".to_string(), "10".to_string(), "20".to_string()];
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("esearch".into()))
            .with_body(esearch_with(&ranked))
            .create();
        server
            .mock("POST", EFETCH_URL)
            .with_body(efetch_with(&["10", "20", "30"]))
            .create();
        let ids: Vec<String> = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        )
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
        assert_eq!(ids, ranked);
    }
}
