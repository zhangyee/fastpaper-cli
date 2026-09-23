# Changelog

## v0.9.1

### Fixes

- Europe PMC and PubMed now retry transient HTTP 502, 503 and 504 responses with bounded exponential backoff, in addition to HTTP 429.
- Successful HTTP responses with malformed or incomplete result structures are retried instead of failing immediately. Final errors retain the request stage, status, parser error and a bounded response-body summary for diagnosis; valid empty result sets remain successful.

## v0.9.0

### Breaking changes

- `biorxiv` and `medrxiv` are no longer sources: their API cannot search by keyword, so every search paged a date window for a minute or more. Naming them now exits 2 and points at Europe PMC, which indexes both: `fastpaper search europepmc '<query> AND SRC:PPR AND PUBLISHER:"bioRxiv"'`.
- `dblp`: `url` is now the publisher's landing page (`dblp:primaryDocumentPage`, often a DOI link), and falls back to the dblp record page only when dblp has none. `id` is still the dblp key.
- `pubmed`, `pmc` and `hal` cap `-n` at what one request can actually fetch (1000, 100 and 1000). A larger `-n` is refused up front instead of failing partway.

### New sources (21 → 26)

- `huggingface` — Hugging Face Papers, for finding what the AI community is paying attention to. `--top` lists one day, ISO week or month ranked by upvotes (`--top 2026-09-18`, `--top 2026-W38`, `--top 2026-08`; up to 1000 with paging), and `--trending` is what is hot right now. Keyword search and `get` too. Ids are arXiv ids, so `fastpaper download <id>` fetches the PDF from arXiv.
- `openreview` — ML conference submissions (ICLR, NeurIPS, …) with their decision in `venue`, rejected and withdrawn ones included. Search only; `--field` takes a group such as `ICLR.cc/2025/Conference`.
- `jstage` — Japanese society journals on J-STAGE. Japanese titles where the record has them; PDF download.
- `oapen` — peer-reviewed open access books and chapters from the OAPEN Library: search, `get` by handle, download.
- `ads` — NASA ADS (SciX) for astronomy and physics: search with ADS query syntax, `get` by bibcode, DOI or arXiv id, download (the arXiv copy, ADS's scans of historical journals, then the publisher's), and citation edges with `fastpaper cite ads <bibcode>`. Needs a free token in `ADS_API_TOKEN`.

### Changes

- JSON output gains a `community` key on every record: upvotes, comments, organization, GitHub repo and stars, the day it was listed, and linked models, datasets and Spaces. Only `huggingface` fills it; it is `null` for every other source. `fastpaper sources` gains a matching `community` column.
- `search` takes no query with `--trending` or `--top`; combining a query with either is a usage error (exit 2). Both flags work on `huggingface` only; elsewhere they are refused with a pointer to it.
- Every HTTP request is bounded: 10 s to connect and 30 s without new bytes, plus 30 s in total for API calls. A stalled server now ends in an error instead of a hang.
- arXiv API requests are queued across every fastpaper process on the machine, spaced 3 s apart, and back off when arXiv answers 429.
- `dblp` searches through sparql.dblp.org, since dblp.org now answers with a bot challenge.
- New environment variables: `ADS_API_TOKEN` (required by `ads`) and `HF_TOKEN` (optional; raises Hugging Face's rate limit).

### Other

- Dependency `aes` moved off the yanked 0.9.0 to 0.9.3.
