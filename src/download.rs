use std::path::{Path, PathBuf};

use crate::sources;

const KIB: u64 = 1024;
const MIB: u64 = 1024 * 1024;

/// Sizes to suggest, in MiB, when telling the caller how to raise the limit.
const SUGGESTIONS: &[u64] = &[50, 100, 200, 500, 1024, 2048, 5120];

/// Render a byte count for a human.
///
/// Limits are MiB-scale and stay in MiB so two of them read as comparable at a
/// glance. Actual file sizes are not: this also renders the `download` receipt,
/// where a 40 KiB note printed as `0.0 MiB` would read like nothing arrived.
pub(crate) fn human_size(bytes: u64) -> String {
    if bytes < KIB {
        format!("{} B", bytes)
    } else if bytes < MIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else if bytes.is_multiple_of(MIB) {
        format!("{} MiB", bytes / MIB)
    } else {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    }
}

/// The next suggestion above `bytes`, so the printed fix actually clears it.
fn suggested_limit(bytes: u64) -> u64 {
    SUGGESTIONS
        .iter()
        .map(|mib| mib * MIB)
        .find(|candidate| *candidate > bytes)
        // `bytes` came off a server's Content-Length, so it is attacker-
        // controlled: `saturating_mul` keeps the doubling from wrapping (and
        // panicking in debug) when `bytes` is within a factor of two of
        // `u64::MAX`.
        .unwrap_or_else(|| (bytes.saturating_add(MIB) / MIB * MIB).saturating_mul(2))
}

/// Explain a refused body.
///
/// Deliberately avoids the word "read": this CLI has a `read` subcommand, and
/// the old `Failed to read PDF` was routinely taken to mean it. Naming the
/// actual size matters too -- 11 MiB means "raise the limit", 500 MiB means
/// "take another route", and the old message let you tell neither.
fn over_limit(actual: Option<u64>, limit: u64, what: &str) -> String {
    let head = match actual {
        Some(bytes) => format!(
            "{} is {}, over the {} download limit.",
            what,
            human_size(bytes),
            human_size(limit)
        ),
        None => format!(
            "{} exceeds the {} download limit (server did not report its size).",
            what,
            human_size(limit)
        ),
    };
    let suggestion = human_size(suggested_limit(actual.unwrap_or(limit)));
    format!(
        "{}\nRaise it with --max-size {} (or FASTPAPER_MAX_DOWNLOAD_SIZE={}).",
        head,
        suggestion.replace(' ', ""),
        suggestion.replace(' ', "")
    )
}

/// The host part of a URL, for naming who refused a request.
fn host_of(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1)?;
    let host = after_scheme.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then(|| host.to_string())
}

/// Explain a request the server refused outright.
///
/// The old message was `HTTP error: http status: 403` and stopped there, which
/// left the caller with no host, no link and no next move.
///
/// It deliberately does not say *why* the server refused, because nothing here
/// can tell: an MDPI paper (fully open access) and an AHA one (subscription)
/// both answer 403, and Semantic Scholar reports `open_access: true` for both.
/// Guessing "paywall" would be wrong half the time and would send the caller
/// away from a paper that is in fact free. The one refusal whose cause *is*
/// visible from outside is a Cloudflare challenge; `challenged` handles it.
///
/// What it can do is hand over the link. Retrying elsewhere is usually wasted:
/// unpaywall resolves this DOI to the same publisher URL byte for byte, and
/// `download europepmc <DOI>` reaches it too, so the alternatives mostly lead
/// back to the same door.
fn refused(status: u16, url: &str) -> String {
    let who = host_of(url).unwrap_or_else(|| "the server".to_string());
    format!(
        "{} from {}\n{}\n\
         The server refused this request. fastpaper cannot tell a paywall from \
         a bot block here -- they look the same from outside. Other resolvers \
         usually hand back this same URL, so opening it yourself is more likely \
         to help than retrying through another source.",
        status, who, url
    )
}

/// Whether a response is Cloudflare's challenge page rather than the origin's
/// answer. Cloudflare marks every challenge it serves with
/// `cf-mitigated: challenge`, so the header is its own statement that the
/// request never reached the site.
fn is_cloudflare_challenge(resp: &ureq::http::Response<ureq::Body>) -> bool {
    resp.headers()
        .get("cf-mitigated")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("challenge"))
}

/// Explain a refusal that Cloudflare, not the origin, produced.
///
/// Measured on CORE, 2026-09-05: `core.ac.uk/download/*` answers every
/// non-browser client 403 with `cf-mitigated: challenge`. The rule is scoped
/// to that path (the rest of the site loads) and the challenge is the
/// interactive kind, which Cloudflare documents as needing a visitor to act,
/// with the clearance cookie bound to the device that solved it. So no
/// header, retry or credential sent from here can pass it -- and, unlike a
/// plain refusal, the cause is known, so this says so instead of shrugging.
///
/// The advice is the opposite of `refused`: a bot block is about the client,
/// not the paper, so another source is worth trying.
fn challenged(status: u16, url: &str) -> String {
    let who = host_of(url).unwrap_or_else(|| "the server".to_string());
    format!(
        "{} from {}\n{}\n\
         Cloudflare answered with its bot challenge (cf-mitigated: challenge) \
         instead of the file. This is a bot block, not a paywall: passing it \
         takes a real browser, so no retry, header or key from fastpaper will \
         get through. Fetch this paper from another source, or open the link \
         in a browser yourself.",
        status, who, url
    )
}

/// Why a PDF could not be produced.
///
/// The two cases need different exit codes because they call for different
/// next moves. `NotFound` means this source has nothing to give — no such
/// record, or a record with no open access file — and the caller should try
/// another source, which is `get`'s exit 4 situation exactly. Everything else
/// is a genuine failure: a refused size, a 403, a broken connection. Collapsing
/// them into one code, as this used to, left a caller unable to tell "look
/// elsewhere" from "your command was wrong".
#[derive(Debug)]
pub enum FetchError {
    NotFound(String),
    Failed(String),
}

impl FetchError {
    pub fn message(&self) -> &str {
        match self {
            FetchError::NotFound(m) | FetchError::Failed(m) => m,
        }
    }
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// Anything a source reports while resolving a record is a real failure: it
/// got far enough to talk to the API and the API did not cooperate.
impl From<String> for FetchError {
    fn from(message: String) -> Self {
        FetchError::Failed(message)
    }
}

/// Download PDF bytes from a URL, refusing a body over `limit` bytes.
///
/// `limit` is a byte count; `u64::MAX` means unlimited. The whole body is held
/// in memory, which is exactly why a default limit exists at all.
pub fn fetch_pdf(url: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
    let not_found = format!("Not found: {}", url);
    fetch_pdf_named(url, limit, &not_found)
}

/// `fetch_pdf` with a caller-supplied 404 message.
///
/// Sources that address papers by identifier can say which paper was missing;
/// a bare URL fetch cannot. See `fetch_limited` for the shared body-reading
/// path both this and `fetch_archive` go through.
pub fn fetch_pdf_named(url: &str, limit: u64, not_found: &str) -> Result<Vec<u8>, FetchError> {
    fetch_limited(url, limit, not_found, "PDF")
}

/// The shared implementation behind `fetch_pdf_named` and `fetch_archive`:
/// downloads `url`, enforces `limit`, and turns a 404 into `not_found`.
///
/// This is the only place in the crate that reads a response body -- so the
/// size limit cannot be forgotten on one path. `what` names the thing being
/// refused so an oversized tarball does not report itself as a PDF.
fn fetch_limited(
    url: &str,
    limit: u64,
    not_found: &str,
    what: &str,
) -> Result<Vec<u8>, FetchError> {
    // `--max-size` is there to bound memory, and compression is what stopped
    // it doing that: ureq's `.limit()` counts bytes inside its decompressor,
    // so a gzip response could inflate ~1000x under the 100 MiB default before
    // the post-read check -- which only runs once the whole body is buffered
    // -- got a say. A PDF is already compressed, so refusing an encoding costs
    // no bandwidth, keeps Content-Length on the response for the precheck
    // below, and makes `.limit()` count the bytes that land in memory. Servers
    // that ignore the header are why that post-read check stays.
    //
    // 4xx/5xx come back as `Ok` rather than `Err(StatusCode)` so the headers
    // survive: `cf-mitigated` on a 403 is what tells a Cloudflare challenge
    // from the origin refusing, and ureq's error keeps only the number.
    let result = ureq::get(url)
        .config()
        .http_status_as_error(false)
        .build()
        .header("accept-encoding", "identity")
        .call();
    match result {
        Ok(resp) if !resp.status().is_success() => {
            let status = resp.status().as_u16();
            match status {
                404 => Err(FetchError::NotFound(not_found.to_string())),
                // 401/402/403 are the server saying "no", as opposed to being
                // unable: worth naming the host and handing back the link,
                // since the caller can act on those and cannot act on
                // `http status: 403`. 401 Unauthorized, 402 Payment Required,
                // 403 Forbidden.
                401..=403 if is_cloudflare_challenge(&resp) => {
                    Err(FetchError::Failed(challenged(status, url)))
                }
                401..=403 => Err(FetchError::Failed(refused(status, url))),
                _ => Err(FetchError::Failed(format!(
                    "HTTP error: http status: {}",
                    status
                ))),
            }
        }
        Ok(mut resp) => {
            // A fast path, not the enforcement: oversized papers ideally cost
            // no bandwidth, and content-length is the only place an exact size
            // is available before reading starts. It is skipped by ureq
            // whenever it is decompressing the response (e.g. gzip), because
            // Content-Length then describes the wire size, not what
            // `read_to_vec` below hands back -- so this check alone cannot be
            // trusted to catch an oversized body.
            if let Some(size) = resp.body().content_length()
                && size > limit
            {
                return Err(FetchError::Failed(over_limit(Some(size), limit, what)));
            }
            // `.limit()` also cannot be trusted alone: it wraps the reader
            // *inside* the decompressor, so for a compressed response it
            // counts compressed bytes while decompression can still produce
            // an unbounded `Vec` in memory. It stays here only to cut off a
            // stream that never ends. `limit + 1` rather than `limit`: ureq's
            // limited reader treats hitting the count exactly as "exceeded"
            // even on genuine EOF, which would wrongly refuse a body of
            // precisely `limit` bytes.
            let bytes = resp
                .body_mut()
                .with_config()
                .limit(limit.saturating_add(1))
                .read_to_vec()
                .map_err(|e| match e {
                    ureq::Error::BodyExceedsLimit(_) => {
                        FetchError::Failed(over_limit(None, limit, what))
                    }
                    other => FetchError::Failed(format!(
                        "Could not fetch the {}: {}",
                        what.to_lowercase(),
                        other
                    )),
                })?;
            // The actual enforcement: bounds what is handed back to the
            // caller regardless of what Content-Length claimed or what
            // `.limit()` was counting. This bounds the *returned* value, not
            // peak memory -- ureq has already buffered the full decompressed
            // body in `bytes` by the time this check runs.
            if bytes.len() as u64 > limit {
                return Err(FetchError::Failed(over_limit(
                    Some(bytes.len() as u64),
                    limit,
                    what,
                )));
            }
            Ok(bytes)
        }
        Err(e) => Err(FetchError::Failed(format!("HTTP error: {}", e))),
    }
}

/// Fetch an archive (zip / tar.gz) under the same size limit as a PDF.
///
/// Kept alongside `fetch_pdf_named` rather than in `figures.rs` so both go
/// through the one body-reading path that enforces `--max-size`.
pub fn fetch_archive(url: &str, limit: u64, not_found: &str) -> Result<Vec<u8>, FetchError> {
    fetch_limited(url, limit, not_found, "Archive")
}

// ── PDF byte fetchers ───────────────────────────
//
// Resolving "identifier -> PDF bytes" is separate from writing the file, so the
// same resolution serves both `download` (save to disk) and any caller that
// only wants the bytes. `save_pdf` below does the writing.

/// Fetch arXiv PDF bytes.
pub fn pdf_bytes_arxiv(
    base_url: &str,
    identifier: &str,
    limit: u64,
) -> Result<Vec<u8>, FetchError> {
    sources::arxiv::download_pdf(base_url, identifier, limit)
}

/// Fetch bioRxiv PDF bytes.
pub fn pdf_bytes_biorxiv(
    base_url: &str,
    identifier: &str,
    limit: u64,
) -> Result<Vec<u8>, FetchError> {
    fetch_pdf(
        &format!("{}/content/{}v1.full.pdf", base_url, identifier),
        limit,
    )
}

/// Fetch medRxiv PDF bytes.
pub fn pdf_bytes_medrxiv(
    base_url: &str,
    identifier: &str,
    limit: u64,
) -> Result<Vec<u8>, FetchError> {
    fetch_pdf(
        &format!("{}/content/{}v1.full.pdf", base_url, identifier),
        limit,
    )
}

/// Fetch PMC PDF bytes from the PMC Cloud Service.
///
/// The article page's `/pdf/` URL is not usable programmatically: it answers
/// every client -- browser user agents included -- with a small
/// "Preparing to download" HTML interstitial. The OA Web Service does return a
/// PDF link, but as an `ftp://` URL, and NCBI has retired anonymous FTP; the
/// files it points at were moved under `/deprecated/` in April 2026 and are due
/// to be deleted in August 2026.
///
/// The Cloud Service (AWS Open Data) serves the same files over plain HTTPS and
/// is the route that survives. Objects are keyed by article and version, so the
/// version has to be discovered by listing the prefix first.
pub fn pdf_bytes_pmc(base_url: &str, identifier: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
    let numeric_id = identifier.strip_prefix("PMC").unwrap_or(identifier);
    // No `limit` here on purpose: this lists an S3 prefix as XML, not a PDF.
    // `limit` applies only to the actual PDF fetch below.
    let listing = http_get_text(&format!(
        "{}/?list-type=2&prefix=PMC{}",
        base_url, numeric_id
    ))?;
    let key = first_pdf_key(&listing).ok_or_else(|| {
        format!(
            "PMC{} has no PDF in the open access subset.\n\
             Not every article ships one -- some are XML or tgz only.",
            numeric_id
        )
    })?;
    fetch_pdf(&format!("{}/{}", base_url, key), limit)
}

/// Pick the `.pdf` object out of an S3 `ListBucketResult`.
fn first_pdf_key(listing: &str) -> Option<String> {
    listing
        .split("<Key>")
        .skip(1)
        .filter_map(|chunk| chunk.split("</Key>").next())
        .find(|key| key.ends_with(".pdf"))
        .map(|key| key.to_string())
}

fn http_get_text(url: &str) -> Result<String, String> {
    ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Read error: {}", e))
}

/// Fetch Semantic Scholar PDF bytes via the record's openAccessPdf URL.
pub fn pdf_bytes_semantic(
    base_url: &str,
    identifier: &str,
    limit: u64,
) -> Result<Vec<u8>, FetchError> {
    let paper = sources::semantic::get_by_id(base_url, identifier)?
        .ok_or_else(|| FetchError::NotFound(format!("Paper not found: {}", identifier)))?;
    let pdf_url = paper
        .pdf_url
        .ok_or_else(|| FetchError::NotFound("No open access PDF available".to_string()))?;
    fetch_pdf(&pdf_url, limit)
}

/// Fetch CORE PDF bytes from its dedicated download endpoint.
///
/// Resolved in two explicit steps rather than by following a redirect.
///
/// `GET /v3/works/{id}/download` answers 302 to a file on `core.ac.uk`, a
/// different host. Letting the client chase that is fragile: the API key must
/// not travel to the redirect target -- sending it there is refused with a 400
/// -- and whether a given client strips it is a detail we would be depending
/// on. So the record is fetched first, authenticated, and its `downloadUrl` is
/// then fetched on its own with no credentials attached.
///
/// Since 2026-09 that file sits behind a Cloudflare interactive challenge for
/// every non-browser client (see `challenged`), so this fails on most networks.
/// The capability stays declared on purpose: the block is about the client and
/// the host, not the paper, and the message says where to go next.
pub fn pdf_bytes_core(base_url: &str, identifier: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
    let paper = sources::core::get_by_id(base_url, identifier)?
        .ok_or_else(|| FetchError::NotFound(format!("Not found in CORE: {}", identifier)))?;
    let pdf_url = paper.pdf_url.ok_or_else(|| {
        FetchError::NotFound(format!("CORE has no downloadable file for {}", identifier))
    })?;
    fetch_pdf(&pdf_url, limit).map_err(|e| match (e, &paper.doi) {
        // CORE only mirrors what repositories publish, so a refused copy says
        // something about CORE's host and nothing about the paper: it is still
        // out there under its DOI, and the record just fetched has it. A bare
        // DOI routes `download` to the open access resolver.
        (FetchError::Failed(msg), Some(doi)) => FetchError::Failed(format!(
            "{}\nCORE only mirrors this paper; its record carries DOI {}. \
             Ask the other sources for it: fastpaper download {}",
            msg, doi, doi
        )),
        (other, _) => other,
    })
}

/// Fetch DOAJ PDF bytes via the record's fulltext link.
pub fn pdf_bytes_doaj(base_url: &str, identifier: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
    resolve_and_fetch(
        &doaj_meta_url(base_url, identifier),
        sources::doaj::parse_search_response,
        limit,
    )
}

/// Fetch Zenodo PDF bytes via the record's file link.
pub fn pdf_bytes_zenodo(
    base_url: &str,
    identifier: &str,
    limit: u64,
) -> Result<Vec<u8>, FetchError> {
    resolve_and_fetch(
        &zenodo_meta_url(base_url, identifier),
        sources::zenodo::parse_search_response,
        limit,
    )
}

/// Fetch HAL PDF bytes via the record's fileMain_s URL.
pub fn pdf_bytes_hal(base_url: &str, identifier: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
    resolve_and_fetch(
        &hal_meta_url(base_url, identifier),
        sources::hal::parse_search_response,
        limit,
    )
}

/// Fetch an ERIC-hosted PDF. Only records ERIC hosts itself have one.
pub fn pdf_bytes_eric(base_url: &str, identifier: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
    fetch_pdf(&format!("{}/fulltext/{}.pdf", base_url, identifier), limit)
}

/// Fetch an OSTI full-text PDF from its stable purl path.
pub fn pdf_bytes_osti(base_url: &str, identifier: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
    fetch_pdf(&format!("{}/servlets/purl/{}", base_url, identifier), limit)
}

/// Fetch an NTRS PDF.
///
/// The file name is not derivable from the id, so this reads the record first
/// and follows the download link it advertises.
pub fn pdf_bytes_ntrs(base_url: &str, identifier: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
    resolve_and_fetch(
        &format!("{}/api/citations/{}", base_url, identifier),
        sources::ntrs::parse_citation_response,
        limit,
    )
}

/// Fetch an OSF preprint PDF.
///
/// OSF serves files from osf.io while its API lives on api.osf.io, so this
/// builds the path off the PDF base rather than reading a URL out of the record.
pub fn pdf_bytes_osf(base_url: &str, identifier: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
    fetch_pdf(&format!("{}/download/{}/", base_url, identifier), limit)
}

// The identifier reaches these straight from the command line, so it has to be
// encoded: a multi-word one used to produce an invalid URI rather than a query.

/// DOAJ puts the query in a path segment, where `+` stays a literal plus.
fn doaj_meta_url(base_url: &str, identifier: &str) -> String {
    format!(
        "{}/api/v4/search/articles/{}?pageSize=1",
        base_url,
        sources::encode_query(identifier).replace('+', "%20")
    )
}

fn zenodo_meta_url(base_url: &str, identifier: &str) -> String {
    format!(
        "{}/api/records?q={}&size=1&type=publication",
        base_url,
        sources::encode_query(identifier)
    )
}

fn hal_meta_url(base_url: &str, identifier: &str) -> String {
    format!(
        "{}/search/?q={}&rows=1&wt=json&fl=halId_s,title_s,authFullName_s,abstract_s,doiId_s,publicationDateY_i,fileMain_s,uri_s",
        base_url,
        sources::encode_query(identifier)
    )
}

/// Fetch Europe PMC PDF bytes via the record's `?pdf=render` URL.
pub fn pdf_bytes_europepmc(
    base_url: &str,
    identifier: &str,
    limit: u64,
) -> Result<Vec<u8>, FetchError> {
    let paper = sources::europepmc::get_by_id(base_url, identifier)?
        .ok_or_else(|| FetchError::NotFound(format!("Not found in Europe PMC: {}", identifier)))?;
    let pdf_url = paper.pdf_url.ok_or_else(|| {
        FetchError::NotFound(format!(
            "Europe PMC has no open access PDF for {}",
            identifier
        ))
    })?;
    fetch_pdf(&pdf_url, limit)
}

/// Fetch a metadata URL, take the first record's `pdf_url`, and download it.
fn resolve_and_fetch(
    meta_url: &str,
    parse: fn(&str) -> Result<Vec<sources::Paper>, String>,
    limit: u64,
) -> Result<Vec<u8>, FetchError> {
    // No `limit` here on purpose: this is the metadata lookup (JSON/XML), not
    // the PDF. `limit` applies only to `fetch_pdf` below.
    let body = ureq::get(meta_url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Read error: {}", e))?;
    let papers = parse(&body)?;
    let pdf_url = papers
        .first()
        .and_then(|p| p.pdf_url.as_deref())
        .ok_or_else(|| FetchError::NotFound("No PDF URL found".to_string()))?;
    fetch_pdf(pdf_url, limit)
}

/// Save PDF bytes to a file. Returns the path where the file was saved.
pub fn save_pdf(
    bytes: &[u8],
    dir: &Path,
    identifier: &str,
    overwrite: bool,
) -> Result<PathBuf, String> {
    // Several sources answer a PDF request with HTML -- an interstitial, a
    // login wall, a publisher landing page -- and writing that out under a .pdf
    // name while reporting success is worse than failing.
    if !bytes.starts_with(b"%PDF") {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]);
        let looks_like = if head
            .trim_start()
            .to_lowercase()
            .starts_with("<!doctype html")
            || head.trim_start().to_lowercase().starts_with("<html")
        {
            " The server returned HTML, not a file."
        } else if bytes.is_empty() {
            " The response was empty."
        } else {
            ""
        };
        return Err(format!(
            "Response is not a PDF.{}\nNothing was written.",
            looks_like
        ));
    }

    std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create directory: {}", e))?;

    let sanitized = identifier.replace('/', "_");
    let filename = format!("{}.pdf", sanitized);
    let path = dir.join(&filename);

    if path.exists() && !overwrite {
        return Err(format!("File already exists: {}", path.display()));
    }

    std::fs::write(&path, bytes).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn host_of_takes_the_authority_and_nothing_else() {
        assert_eq!(
            host_of("https://www.mdpi.com/1424-8220/21/16/5542/pdf?version=1629270899"),
            Some("www.mdpi.com".to_string())
        );
        assert_eq!(
            host_of("http://export.arxiv.org/pdf/2301.08745.pdf"),
            Some("export.arxiv.org".to_string())
        );
        assert_eq!(
            host_of("https://example.org"),
            Some("example.org".to_string())
        );
        assert_eq!(host_of("not a url"), None);
    }

    // The old message was `HTTP error: http status: 403` and nothing else: no
    // host, no link, no way to act. Naming who refused and what they refused
    // is the whole point.
    #[test]
    fn a_refusal_names_the_host_the_status_and_the_link() {
        let msg = refused(
            403,
            "https://www.mdpi.com/1424-8220/21/16/5542/pdf?version=1",
        );
        assert!(msg.contains("403"), "got: {}", msg);
        assert!(msg.contains("www.mdpi.com"), "got: {}", msg);
        assert!(
            msg.contains("https://www.mdpi.com/1424-8220/21/16/5542/pdf?version=1"),
            "the whole link has to be there to be openable: {}",
            msg
        );
    }

    // Measured, not assumed: an MDPI paper (fully open access) and an AHA one
    // (subscription) both come back 403, and semantic reports `open_access:
    // true` for both. Nothing available here separates them, so the message
    // must not pretend otherwise.
    #[test]
    fn a_refusal_does_not_claim_to_know_whether_it_is_a_paywall() {
        let msg = refused(403, "https://example.org/x.pdf").to_lowercase();
        assert!(
            !msg.contains("paywall.") && !msg.contains("subscription required"),
            "must not diagnose a cause it cannot observe: {}",
            msg
        );
        assert!(
            msg.contains("cannot tell") || msg.contains("indistinguishable"),
            "should say the cause is undeterminable: {}",
            msg
        );
    }

    // Verified live: unpaywall hands back the same publisher URL semantic
    // does, byte for byte, and `download europepmc <DOI>` lands on it too. A
    // caller who retries elsewhere without knowing that burns three requests
    // to reach the same 403.
    #[test]
    fn a_refusal_warns_that_other_resolvers_return_the_same_link() {
        let msg = refused(403, "https://example.org/x.pdf");
        assert!(msg.contains("same"), "got: {}", msg);
    }

    #[test]
    fn a_refusal_covers_the_other_refusing_statuses() {
        for status in [401, 402, 403] {
            let msg = refused(status, "https://example.org/x.pdf");
            assert!(msg.contains(&status.to_string()), "got: {}", msg);
        }
    }

    // Measured on CORE, 2026-09-05: core.ac.uk/download/* answers every
    // non-browser client 403 with `cf-mitigated: challenge`, Cloudflare's own
    // statement that this is its bot challenge and not the origin refusing.
    // That header is the one case where the cause *is* observable from
    // outside, so the message has to name it -- and tell the caller the move
    // is a different source, the opposite of what a plain refusal advises.
    #[test]
    fn a_cloudflare_challenge_is_named_rather_than_left_ambiguous() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", "/download/1.pdf")
            .with_status(403)
            .with_header("cf-mitigated", "challenge")
            .create();
        let msg = fetch_pdf(&format!("{}/download/1.pdf", server.url()), 10 * MIB)
            .unwrap_err()
            .message()
            .to_string();
        assert!(msg.contains("Cloudflare"), "got: {}", msg);
        assert!(msg.contains("challenge"), "got: {}", msg);
        assert!(msg.contains("not a paywall"), "got: {}", msg);
        assert!(msg.contains("another source"), "got: {}", msg);
        assert!(
            !msg.contains("cannot tell"),
            "must not shrug when the cause is known: {}",
            msg
        );
    }

    // The header is the evidence. Without it a 403 stays undeterminable, and
    // this pins that the challenge path did not swallow the plain one.
    #[test]
    fn a_plain_403_is_still_reported_as_undeterminable() {
        let mut server = mockito::Server::new();
        let _m = server.mock("GET", "/x.pdf").with_status(403).create();
        let msg = fetch_pdf(&format!("{}/x.pdf", server.url()), 10 * MIB)
            .unwrap_err()
            .message()
            .to_string();
        assert!(msg.contains("403"), "got: {}", msg);
        assert!(msg.contains("cannot tell"), "got: {}", msg);
    }

    // CORE only mirrors what repositories publish, so a paper whose CORE copy
    // sits behind the challenge is still out there under its DOI. The record
    // was already fetched to find the file; handing its DOI on costs nothing
    // and saves the caller a `get` before the next attempt. A bare DOI routes
    // `download` to the open access resolver, so that is the command to give.
    #[test]
    fn a_refused_core_download_hands_over_the_doi_for_another_source() {
        let mut server = mockito::Server::new();
        let work = format!(
            r#"{{"id":80549003,"title":"Whither Megaleaks",
"doi":"10.31979/2377-6188.2017.010107",
"downloadUrl":"{}/download/80549003.pdf"}}"#,
            server.url()
        );
        let _record = server
            .mock(
                "GET",
                mockito::Matcher::Regex("/v3/works/80549003".to_string()),
            )
            .with_status(200)
            .with_body(work)
            .create();
        let _file = server
            .mock("GET", "/download/80549003.pdf")
            .with_status(403)
            .with_header("cf-mitigated", "challenge")
            .create();
        let msg = pdf_bytes_core(&server.url(), "80549003", 10 * MIB)
            .unwrap_err()
            .message()
            .to_string();
        assert!(msg.contains("challenge"), "got: {}", msg);
        assert!(
            msg.contains("fastpaper download 10.31979/2377-6188.2017.010107"),
            "got: {}",
            msg
        );
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fastpaper_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn save_pdf_creates_file() {
        let dir = temp_dir();
        let path = save_pdf(b"%PDF-1.4 fake", &dir, "2301.08745", false).unwrap();
        assert!(path.exists());
        assert!(path.to_str().unwrap().contains("2301.08745"));
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 1: download arxiv → mock PDF → file saved to dir
    #[test]
    fn arxiv_pdf_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4 fake content".as_slice())
            .create();
        let dir = temp_dir();
        let bytes = pdf_bytes_arxiv(&server.url(), "2301.08745", 10 * MIB).unwrap();
        let path = save_pdf(&bytes, &dir, "2301.08745", false).unwrap();
        assert!(path.exists());
        assert_eq!(fs::read(&path).unwrap(), b"%PDF-1.4 fake content");
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 2: saved filename contains paper ID
    #[test]
    fn arxiv_pdf_filename_contains_id() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        let dir = temp_dir();
        let bytes = pdf_bytes_arxiv(&server.url(), "2301.08745", 10 * MIB).unwrap();
        let path = save_pdf(&bytes, &dir, "2301.08745", false).unwrap();
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .contains("2301.08745"),
            "filename {:?} should contain paper ID",
            path.file_name()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 3: --dir saves to specified directory
    #[test]
    fn arxiv_pdf_custom_dir() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        let dir = temp_dir().join("custom_subdir");
        let bytes = pdf_bytes_arxiv(&server.url(), "2301.08745", 10 * MIB).unwrap();
        let path = save_pdf(&bytes, &dir, "2301.08745", false).unwrap();
        assert!(path.starts_with(&dir));
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    // Behavior 4: file already exists → error with "already exists"
    #[test]
    fn arxiv_pdf_file_exists_returns_err() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        let dir = temp_dir();
        // First download succeeds
        let bytes = pdf_bytes_arxiv(&server.url(), "2301.08745", 10 * MIB).unwrap();
        save_pdf(&bytes, &dir, "2301.08745", false).unwrap();
        // Second save fails because the file exists
        let result = save_pdf(&bytes, &dir, "2301.08745", false);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("already exists"),
            "error should mention 'already exists'"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 5: --overwrite overwrites existing file
    #[test]
    fn arxiv_pdf_overwrite_succeeds() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4 version2".as_slice())
            .create();
        let dir = temp_dir();
        // Create existing file
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("2301.08745.pdf"), b"old content").unwrap();
        // Download with overwrite
        let bytes = pdf_bytes_arxiv(&server.url(), "2301.08745", 10 * MIB).unwrap();
        let path = save_pdf(&bytes, &dir, "2301.08745", true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"%PDF-1.4 version2");
        let _ = fs::remove_dir_all(&dir);
    }

    // Behavior 6: mock 404 → error
    #[test]
    fn arxiv_pdf_404_returns_err() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .create();
        let dir = temp_dir();
        let result = pdf_bytes_arxiv(&server.url(), "9999.99999", 10 * MIB);
        assert!(result.is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    // ── additional source download tests ────────

    #[test]
    fn biorxiv_pdf_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock(
                "GET",
                mockito::Matcher::Regex("content.*full.pdf".to_string()),
            )
            .with_status(200)
            .with_body(b"%PDF-1.4 biorxiv".as_slice())
            .create();
        let dir = temp_dir();
        let bytes =
            pdf_bytes_biorxiv(&server.url(), "10.1101/2024.01.01.574894", 10 * MIB).unwrap();
        let path = save_pdf(&bytes, &dir, "10.1101/2024.01.01.574894", false).unwrap();
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn medrxiv_pdf_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock(
                "GET",
                mockito::Matcher::Regex("content.*full.pdf".to_string()),
            )
            .with_status(200)
            .with_body(b"%PDF-1.4 medrxiv".as_slice())
            .create();
        let dir = temp_dir();
        let bytes =
            pdf_bytes_medrxiv(&server.url(), "10.1101/2024.01.01.123456", 10 * MIB).unwrap();
        let path = save_pdf(&bytes, &dir, "10.1101/2024.01.01.123456", false).unwrap();
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    // Two steps now: list the article's prefix to learn the version, then
    // fetch the .pdf object.
    #[test]
    fn pmc_pdf_saves_file() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("list-type=2".to_string()))
            .with_status(200)
            .with_body("<ListBucketResult><Contents><Key>PMC7318926.1/PMC7318926.1.pdf</Key></Contents></ListBucketResult>")
            .create();
        server
            .mock("GET", mockito::Matcher::Regex(r"\.pdf$".to_string()))
            .with_status(200)
            .with_body(b"%PDF-1.4 pmc".as_slice())
            .create();
        let dir = temp_dir();
        let bytes = pdf_bytes_pmc(&server.url(), "PMC7318926", 10 * MIB).unwrap();
        let path = save_pdf(&bytes, &dir, "PMC7318926", false).unwrap();
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pmc_pdf_strips_prefix() {
        let mut server = mockito::Server::new();
        let listing = server
            .mock("GET", mockito::Matcher::Regex("prefix=PMC7318926".to_string()))
            .with_status(200)
            .with_body("<ListBucketResult><Contents><Key>PMC7318926.1/PMC7318926.1.pdf</Key></Contents></ListBucketResult>")
            .create();
        server
            .mock("GET", mockito::Matcher::Regex(r"\.pdf$".to_string()))
            .with_status(200)
            .with_body(b"%PDF-1.4".as_slice())
            .create();
        // "PMC7318926" and "7318926" must produce the same prefix query.
        assert!(pdf_bytes_pmc(&server.url(), "PMC7318926", 10 * MIB).is_ok());
        listing.assert();
    }

    // An article that ships XML only should say so, not fail obscurely.
    #[test]
    fn pmc_without_a_pdf_reports_why() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body("<ListBucketResult><Contents><Key>PMC7318926.1/PMC7318926.1.xml</Key></Contents></ListBucketResult>")
            .create();
        let err = pdf_bytes_pmc(&server.url(), "PMC7318926", 10 * MIB).unwrap_err();
        assert!(err.message().contains("no PDF"), "got: {}", err);
    }

    #[test]
    fn fetch_pdf_returns_bytes() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(b"%PDF-1.4 test".as_slice())
            .create();
        let bytes = fetch_pdf(&format!("{}/test.pdf", server.url()), 10 * MIB).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn fetch_pdf_404_returns_err() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .create();
        let result = fetch_pdf(&format!("{}/missing.pdf", server.url()), 10 * MIB);
        assert!(result.is_err());
    }

    #[test]
    fn human_size_prints_whole_mib_without_a_decimal() {
        assert_eq!(human_size(104_857_600), "100 MiB");
        assert_eq!(human_size(10_485_760), "10 MiB");
    }

    #[test]
    fn human_size_keeps_one_decimal_when_it_is_not_whole() {
        assert_eq!(human_size(25_480_000), "24.3 MiB");
    }

    // The `download` receipt prints real file sizes, and most papers are not
    // MiB-scale. "0.0 MiB" for a 40 KiB note reads like nothing arrived.
    #[test]
    fn human_size_stays_readable_below_a_megabyte() {
        assert_eq!(human_size(13), "13 B");
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(40 * 1024), "40.0 KiB");
    }

    // "Failed to read PDF" sent a user looking at the `read` subcommand. In a
    // CLI that has one, the word cannot also mean "read the HTTP body".
    #[test]
    fn the_over_limit_message_never_says_read() {
        let msg = over_limit(Some(233_000_000), 104_857_600, "PDF");
        assert!(!msg.to_lowercase().contains("read"), "got: {}", msg);
    }

    #[test]
    fn the_over_limit_message_gives_the_actual_size_and_a_fix() {
        let msg = over_limit(Some(233_000_000), 104_857_600, "PDF");
        assert!(
            msg.contains("222.2 MiB"),
            "should name the actual size: {}",
            msg
        );
        assert!(msg.contains("100 MiB"), "should name the limit: {}", msg);
        assert!(msg.contains("--max-size"), "should show the fix: {}", msg);
        assert!(
            msg.contains("FASTPAPER_MAX_DOWNLOAD_SIZE"),
            "should show the env var too: {}",
            msg
        );
    }

    // Without content-length there is no honest number to report, so the
    // message says so rather than repeating the limit as if it were the size.
    #[test]
    fn an_unknown_size_is_admitted_rather_than_guessed() {
        let msg = over_limit(None, 104_857_600, "PDF");
        assert!(msg.contains("did not report its size"), "got: {}", msg);
        assert!(msg.contains("--max-size"), "got: {}", msg);
    }

    #[test]
    fn the_suggested_limit_clears_the_actual_size() {
        let msg = over_limit(Some(233_000_000), 104_857_600, "PDF");
        assert!(msg.contains("--max-size 500MiB"), "got: {}", msg);
    }

    #[test]
    fn the_over_limit_message_names_what_was_refused() {
        let pdf = over_limit(Some(233_000_000), 104_857_600, "PDF");
        assert!(pdf.starts_with("PDF is 222.2 MiB"), "got: {}", pdf);

        let archive = over_limit(Some(233_000_000), 104_857_600, "Archive");
        assert!(
            archive.starts_with("Archive is 222.2 MiB"),
            "got: {}",
            archive
        );
        // 名词变了,给出的修复办法不变。
        assert!(archive.contains("--max-size"), "got: {}", archive);
    }

    #[test]
    fn a_body_within_the_limit_comes_back_whole() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("GET", "/small.pdf")
            .with_header("content-length", "13")
            .with_body("%PDF-1.4 fake")
            .create();
        let bytes = fetch_pdf(&format!("{}/small.pdf", server.url()), 10 * MIB).unwrap();
        assert_eq!(bytes, b"%PDF-1.4 fake");
        m.assert();
    }

    // Content-length is checked before the body is touched, so an oversized
    // paper costs no bandwidth at all -- the declared size is what triggers
    // the refusal, not how many bytes the mock actually put on the wire (kept
    // tiny here: a 25 MiB `with_body` in an earlier version of this test cost
    // ~80 MiB of peak RSS in the test process for no extra coverage).
    #[test]
    fn content_length_over_the_limit_is_refused_with_the_real_size() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", "/big.pdf")
            .with_header("content-length", "25480000")
            .with_chunked_body(|w| w.write_all(&[0u8; 100]))
            .create();
        let err = fetch_pdf(&format!("{}/big.pdf", server.url()), 10 * MIB).unwrap_err();
        assert!(err.message().contains("24.3 MiB"), "got: {}", err);
        assert!(err.message().contains("--max-size"), "got: {}", err);
    }

    // Distinct from `a_body_within_the_limit_comes_back_whole`: that test's
    // declared content-length and its finite limit are both way above the
    // body, so it would pass even if the limit were never applied at all.
    // This one has no declared length to check against and a body a normal
    // limit would refuse (10 MiB elsewhere in this file), so only an
    // unlimited limit -- not a generous one -- can be why it passes.
    #[test]
    fn an_unlimited_limit_accepts_a_body_a_normal_limit_would_refuse() {
        let mut server = mockito::Server::new();
        let body = vec![0u8; 11 * 1024 * 1024];
        let _m = server
            .mock("GET", "/any.pdf")
            .with_chunked_body({
                let body = body.clone();
                move |w| w.write_all(&body)
            })
            .create();
        let bytes = fetch_pdf(&format!("{}/any.pdf", server.url()), u64::MAX).unwrap();
        assert_eq!(bytes, body);
    }

    #[test]
    fn a_404_still_reports_not_found_rather_than_a_size_problem() {
        let mut server = mockito::Server::new();
        let _m = server.mock("GET", "/gone.pdf").with_status(404).create();
        let err = fetch_pdf(&format!("{}/gone.pdf", server.url()), 10 * MIB).unwrap_err();
        assert!(err.message().contains("Not found"), "got: {}", err);
    }

    #[test]
    fn a_named_fetch_uses_its_own_not_found_wording() {
        let mut server = mockito::Server::new();
        let _m = server.mock("GET", "/gone.pdf").with_status(404).create();
        let err = fetch_pdf_named(
            &format!("{}/gone.pdf", server.url()),
            10 * MIB,
            "Paper not found: 2301.08745",
        )
        .unwrap_err();
        assert_eq!(err.message(), "Paper not found: 2301.08745");
    }

    // A chunked response has no Content-Length, so this exercises the other
    // enforcement path: ureq's own reader limit (`.limit()`), not the
    // precheck. That path is what a compressed response also takes, since
    // ureq reports no content-length while decompressing either.
    #[test]
    fn a_streaming_body_over_the_limit_is_refused_when_length_is_unknown() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", "/chunked.pdf")
            .with_chunked_body(|w| w.write_all(&[0u8; 2 * 1024 * 1024]))
            .create();
        let err = fetch_pdf(&format!("{}/chunked.pdf", server.url()), 1024 * 1024).unwrap_err();
        assert!(
            err.message().contains("did not report its size"),
            "got: {}",
            err
        );
    }

    // `--max-size` exists to bound memory, and by default it did not: ureq's
    // `.limit()` counts bytes *inside* its decompressor, and the check on
    // `bytes.len()` only fires once the whole decompressed body is already
    // buffered, so under the 100 MiB default a gzip response could inflate
    // ~1000x before anything refused it. A PDF is already compressed, so
    // asking for no transfer encoding costs nothing and buys two things: a
    // Content-Length to precheck against, and a `.limit()` that counts the
    // bytes that actually land in memory.
    #[test]
    fn a_pdf_fetch_asks_for_no_compression() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("GET", "/plain.pdf")
            .match_header("accept-encoding", "identity")
            .with_body("%PDF-1.4 fake")
            .create();
        let bytes = fetch_pdf(&format!("{}/plain.pdf", server.url()), 10 * MIB).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        m.assert();
    }

    // The regression this task exists to close: ureq reports no
    // content-length while decompressing (so the precheck above is skipped),
    // and its `.limit()` counts *compressed* bytes, inside the decompressor
    // rather than around it -- so neither guard alone catches an inflated
    // body. The fixture is ~2KiB of gzip that decompresses to 2 MiB, which
    // slips straight past a 1 MiB `.limit()` on the wire; only the check on
    // the decompressed `Vec` after `read_to_vec` catches it, and only that
    // check can report the real, decompressed size.
    #[test]
    fn a_gzip_body_whose_decompressed_size_exceeds_the_limit_is_refused() {
        let mut server = mockito::Server::new();
        // Generated with `head -c 2097152 /dev/zero | gzip -9`: 2 MiB of NUL
        // bytes, not a PDF, which is why the assertion below is about the
        // refusal and never about the content.
        let compressed = include_bytes!("../tests/fixtures/oversized_gzip_body.pdf.gz").to_vec();
        let _m = server
            .mock("GET", "/gz.pdf")
            .with_header("content-encoding", "gzip")
            .with_body(compressed)
            .create();
        let err = fetch_pdf(&format!("{}/gz.pdf", server.url()), 1024 * 1024).unwrap_err();
        assert!(
            err.message().contains("2 MiB"),
            "should name the real decompressed size: {}",
            err
        );
        assert!(err.message().contains("--max-size"), "got: {}", err);
    }
}

#[cfg(test)]
mod meta_url_tests {
    use super::*;

    // These resolvers take whatever the user typed. Interpolating it raw makes
    // any multi-word identifier an invalid URI: `download zenodo "machine
    // learning dataset"` failed with "http: invalid uri character".
    #[test]
    fn zenodo_meta_url_encodes_spaces() {
        let u = zenodo_meta_url("https://zenodo.org", "machine learning");
        assert!(!u.contains(' '), "raw space in URL: {}", u);
        assert!(u.contains("machine+learning"), "got: {}", u);
    }

    #[test]
    fn hal_meta_url_encodes_spaces() {
        let u = hal_meta_url("https://api.archives-ouvertes.fr", "machine learning");
        assert!(!u.contains(' '), "raw space in URL: {}", u);
    }

    // DOAJ carries the query in a path segment, where '+' is a literal plus.
    #[test]
    fn doaj_meta_url_uses_percent_twenty_not_plus() {
        let u = doaj_meta_url("https://doaj.org", "machine learning");
        assert!(u.contains("machine%20learning"), "got: {}", u);
        assert!(
            !u.contains("machine+learning"),
            "path segments need %20: {}",
            u
        );
    }
}

#[cfg(test)]
mod pdf_guard_tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fastpaper_guard_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    // PMC and DOAJ both used to answer a PDF request with HTML -- an
    // interstitial page and a publisher landing page -- and save_pdf wrote them
    // out under a .pdf name and reported success.
    #[test]
    fn save_pdf_rejects_html() {
        let dir = temp_dir();
        let err = save_pdf(b"<!DOCTYPE html><html>...", &dir, "x", false).unwrap_err();
        assert!(err.contains("not a PDF"), "got: {}", err);
        assert!(!dir.join("x.pdf").exists(), "nothing should be written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_pdf_error_names_html_when_that_is_what_arrived() {
        let dir = temp_dir();
        let err = save_pdf(b"\n\n  <html><head>", &dir, "x", false).unwrap_err();
        assert!(err.to_lowercase().contains("html"), "got: {}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_pdf_rejects_an_empty_body() {
        let dir = temp_dir();
        assert!(save_pdf(b"", &dir, "x", false).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_pdf_accepts_a_real_pdf() {
        let dir = temp_dir();
        assert!(save_pdf(b"%PDF-1.7\n...", &dir, "ok", false).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The PMC Cloud Service lists one prefix per article, versioned.
    const S3_LISTING: &str = r#"<?xml version="1.0"?><ListBucketResult>
<Contents><Key>PMC5334499.1/PMC5334499.1.json</Key></Contents>
<Contents><Key>PMC5334499.1/PMC5334499.1.pdf</Key></Contents>
<Contents><Key>PMC5334499.1/PMC5334499.1.txt</Key></Contents>
<Contents><Key>PMC5334499.1/WJR-9-27-g001.jpg</Key></Contents>
</ListBucketResult>"#;

    #[test]
    fn picks_the_pdf_key_out_of_the_listing() {
        assert_eq!(
            first_pdf_key(S3_LISTING),
            Some("PMC5334499.1/PMC5334499.1.pdf".to_string())
        );
    }

    // Not every open access article ships a standalone PDF; some are tgz only.
    #[test]
    fn returns_none_when_the_article_has_no_pdf() {
        let listing = r#"<ListBucketResult>
<Contents><Key>PMC7318926.1/PMC7318926.1.xml</Key></Contents>
</ListBucketResult>"#;
        assert_eq!(first_pdf_key(listing), None);
    }
}
