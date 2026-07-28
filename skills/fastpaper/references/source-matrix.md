# Source matrix

Per-source detail for `fastpaper`. Read the section for a source before
constructing a non-trivial query against it. `SKILL.md` has the routing rules;
this file has the specifics.

Verified against the live APIs on 2026-07-28. When this file and
`fastpaper sources --capabilities` disagree, the CLI is right.

## The whole matrix

`cit` = does the source populate the `citations` field. `-n max` = per-request cap.

| Source | Role | Discipline / dataset | get | download | cit | -n max | Tier | Auth |
|---|---|---|---|---|---|---|---|---|
| `arxiv` | discovery, full text | physics, math, CS, stats, EE, q-bio, q-fin, econ — preprints, all freely readable | ✓ | ✓ | ✗ | 2000 | A | none |
| `biorxiv` | full text | life-science preprints (CSHL) | ✓ | ✓ | ✗ | — | A | none |
| `medrxiv` | index | medical/health preprints (CSHL) | ✓ | ✗ | ✗ | — | A | none |
| `pubmed` | discovery, index | biomedicine, 35M+ citations, abstracts only | ✓ | ✗ | ✗ | 10000 | B | `NCBI_API_KEY` opt |
| `pmc` | discovery, full text | biomedical full-text archive (NLM), OA subset downloadable | ✓ | ✓ | ✗ | 10000 | B | `NCBI_API_KEY` opt |
| `europepmc` | discovery, full text | **widest biomedical**, incl. patents via `--patents`: 45M+ abstracts, 9M+ full text, plus EPO patents, NICE guidelines, Agricola, bioRxiv/medRxiv, Chinese Biological Abstracts | ✓ | ✓ | ✓ | 1000 | A | none |
| `scholar` | scout | broadest coverage of anything; HTML scraping, no API; **no patents** | ✗ | ✗ | ✗ | — | C | none |
| `xueshu` | scout | 百度学术: 700M+ docs — Chinese journals, theses, conferences, **patents**, standards, books; 500+ subjects | ✗ | ✗ | ✓(often 0) | — | C | none |
| `semantic` | discovery, full text | all disciplines; **richest citation counts** | ✓ | ✓ | ✓ | 100 | A w/ key, else C | `SEMANTIC_SCHOLAR_API_KEY` |
| `crossref` | discovery, resolver | DOI registry, all disciplines, registered metadata only | ✓ | ✗ | ✓ | 1000 | A | `FASTPAPER_EMAIL` polite |
| `openalex` | discovery, index | 200M+ works, successor to MS Academic Graph | ✓ | ✗ | ✓ | 100 | A | `OPENALEX_API_KEY` opt |
| `dblp` | discovery, index | **computer science only**, bibliographic | ✗ | ✗ | ✗ | 1000 | A | none |
| `core` | discovery, full text | 400M+ OA resources from institutional repos and journals | ✓ | ✓ | ✓(often 0) | 100 | A w/ key, else C | `CORE_API_KEY` |
| `openaire` | discovery, index | EU open science graph, worldwide OA | ✓ | ✗ | ✓ | — | A | none |
| `doaj` | discovery, index | quality-reviewed OA journals, all subjects | ✓ | ✗ | ✗ | 100 | A | none |
| `unpaywall` | resolver | DOI → legal free copy. **No search at all** | ✓ | ✗ | ✗ | — | A | `UNPAYWALL_EMAIL` **required** |
| `zenodo` | discovery, full text | CERN/OpenAIRE repo: datasets, software, papers | ✓ | ✓ | ✗ | **25** | A | none |
| `hal` | discovery, full text | French national multi-disciplinary archive (CNRS) | ✓ | ✓ | ✗ | 10000 | A | none |

`cite` (citation edges) exists on exactly two sources: `semantic` and
`openalex`. `get` is unavailable on exactly three: `scholar`, `xueshu`, `dblp`.
`download` is available on eight: `arxiv`, `biorxiv`, `pmc`, `europepmc`,
`semantic`, `core`, `zenodo`, `hal`.

## Native query syntax

### `europepmc` — the most capable query language here

Passed through verbatim, so the full Europe PMC syntax works (200+ fields).

| Tag | Meaning |
|---|---|
| `SRC:` | subset. **`MED`** PubMed · **`PMC`** full text · **`PPR`** preprints · **`PAT`** EPO patents · **`CTX`** NICE clinical guidelines · **`AGR`** Agricola · **`HIR`** health information · **`CBA`** Chinese Biological Abstracts |
| `CITED:>N` | citation-count threshold |
| `AUTH:"Name"` | author |
| `PUB_YEAR:2024`, `FIRST_PDATE:[2020-01-01 TO 2024-12-31]` | dates |
| `OPEN_ACCESS:y`, `HAS_FT:y`, `HAS_PDF:y` | availability |
| `PUB_TYPE:`, `LANG:`, `KW:`, `TITLE:`, `ABSTRACT:`, `JOURNAL:` | others |

```bash
fastpaper search europepmc 'SRC:PAT AND CRISPR' -n 20 --format json
fastpaper search europepmc 'NOT SRC:PAT AND diabetes AND CITED:>500' --sort citations -n 20 --format json
```

Quirks: sorts newest/most-cited first only — `--order asc` is an error.
`--offset` must be a multiple of `-n`. `--field` unsupported.

### `pubmed` / `pmc` — Entrez syntax

`[pt]` publication type · `[mh]` MeSH · `[tiab]` title/abstract · `[au]` author ·
`[dp]` date of publication.

```bash
fastpaper search pubmed 'CRISPR AND systematic review[pt] AND humans[mh]' -n 20 --format json
```

`pubmed` is abstracts only — for full text move to `pmc` or `europepmc`.
`--field` is unsupported on both: MeSH headings are not fields of study and
would mislead under that flag.

### `arxiv` — the one that does NOT pass through

Every whitespace-separated word becomes `all:{word}`. Writing `cat:cs.CL` or
`ti:"..."` in the query does nothing useful. Use the flags:

```bash
fastpaper search arxiv "attention mechanism" --field cs.CL --author "Vaswani" --after 2023-01-01 --format json
```

Categories for `--field`: `cs.CL` `cs.LG` `cs.CV` `cs.AI` `cs.RO` · `math.*` ·
`physics.*` · `q-bio.*` · `q-fin.*` · `econ.*` · `stat.ML` · `eess.*`.
`--sort citations` is an error — arXiv publishes no citation counts.

### `hal` — Solr, `--field` takes a domain code

`math` · `phys` · `chim` (chemistry) · `sde` (environment) · `sdv` (life sciences) ·
`shs` (social sciences & humanities) · `spi` (engineering) · `info` (CS).
Year granularity only — use `--year`, not `--after`/`--before`.

### `openalex` — `--field` takes a concept **ID**, not a name

E.g. `C154945302`. Names are rejected by the API. `--offset` must be a multiple
of `-n`.

### `openaire` — `--field` takes a field-of-science value

Pass a wrong one and the API answers with the list of legal values.

### `doaj` / `zenodo` / `core` / `dblp`

`doaj`: Lucene on `bibjson.*` (`bibjson.year:`, `bibjson.subject.term:`). Year
granularity only; no sorting.
`zenodo`: Elasticsearch syntax. Anonymous callers are capped at **`-n 25`**.
`core`: its own field syntax; `--author` matches `authors.name`.
`dblp`: `year:2020`, `year:2020:2023`, `author:`, `venue:` inside the query. No
other filters exist.

## Per-source gotchas

- **`semantic`** — without `SEMANTIC_SCHOLAR_API_KEY` it fails with
  `rate limited after 5 retries`. With a key it is the best citation source
  available here. `--sort` switches to the bulk endpoint, which pages by cursor
  and therefore cannot be combined with `--offset`. `--author` is unsupported.
- **`core`** — anonymous requests may work but are throttled; set `CORE_API_KEY`.
  `citations` is frequently 0 even for well-cited work, so do not rank on it.
- **`biorxiv` / `medrxiv`** — **no keyword search API.** They browse a date
  window and match the keyword locally, so `--after`/`--before`/`--year` decide
  what is even searched. Default window is the last 30 days. `medrxiv` PDF
  downloads are blocked by the server (HTTP 403).
- **`zenodo`** — `-n` above 25 is rejected. Holds datasets and software as well
  as papers.
- **`hal` / `zenodo`** — many records are metadata-only; `get` succeeds but
  `pdf_url` is `null`.
- **`openaire`** — `get` wants an OpenAIRE objIdentifier; passing a DOI returns
  not-found. Search by keyword instead.
- **`crossref`** — best title→DOI lookup here, but still fuzzy: verify the title
  actually matches before using the DOI. No PDF links, no OA status.
- **`openalex`** — free-text relevance drifts on exact-title queries; prefer
  `crossref` for title→DOI. Good `pdf_url` / `oa_url` once you have the right record.
- **`unpaywall`** — lookup only, one DOI at a time; `UNPAYWALL_EMAIL` must be a
  real address. Returns a `pdf_url` you must fetch yourself.
- **`scholar`** — no API; rate-limited and captcha-prone. Never emits `doi` or
  `pdf_url`. `[CITATION]` stub results have no link and are dropped, so the
  returned count can be well under `-n`. **No `--patents`**: Google's `as_sdt`
  switch can only widen results to include patents, never narrow to them.
- **`xueshu`** — unofficial endpoint, bot detection; keep it serial and
  low-frequency. `--patents` returns patents only; without it they are excluded.
  The filter is local (Baidu's own `filter` parameter is ignored), so it pages
  further to fill `-n` — keep `-n` modest. Its API overloads its own `doi` field
  with patent numbers and bare repository URLs; those are dropped, so `doi` here
  is always a real DOI or `null`. `citations` is often 0 even for cited work.
- **`dblp`** — computer science only, metadata only, no abstracts.
- **`pmc`** — PDFs come from the OA subset only.

## Identifier routing

`get` (`commands/get.rs`): arXiv→`arxiv` · DOI→`crossref` · PMC→`pmc` ·
PMID→`pubmed` · `S2:`→`semantic`. URLs are rejected.

`download` (`commands/download.rs`): arXiv→`arxiv` · PMC→`pmc` ·
DOI and `S2:`→`semantic` (its OA resolver). PMID and URLs are rejected —
a PMID must be converted to a PMC ID first.
