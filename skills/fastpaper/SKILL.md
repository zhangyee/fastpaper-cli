---
name: fastpaper
description: Use when the user asks to find academic papers or patents, survey a literature, look up a paper by DOI, arXiv id, PMID or PMC id, trace who cites what, get a paper's PDF, or read one. Covers 18 sources — arXiv, PubMed, PMC, Europe PMC, bioRxiv, medRxiv, Semantic Scholar, OpenAlex, Crossref, DBLP, CORE, OpenAIRE, DOAJ, HAL, Zenodo, Unpaywall, Google Scholar, Baidu Xueshu.
---

# fastpaper

A pre-installed CLI for academic search, download and reading. Verify once with
`which fastpaper`; it is NOT a Python package, never `pip install` it.

Pass `--format json` on every call. Output is `{"source": "...", "results": [...]}`
— the papers are under `results`. Exit 0 = success, non-zero = error. Any field
the source did not supply is `null`, meaning **unknown**; report it as unknown
rather than filling it in.

## Commands

```bash
# search one source
fastpaper search <source> "<query>" -n 20 --format json

# metadata for one paper; source inferred from the identifier shape
fastpaper get <DOI|arXiv_ID|PMID|PMC_ID>
fastpaper get <source> <id>                 # or name the source

# save a PDF (default ./papers/<id>.pdf)
fastpaper download <id> [-d <dir>] [--overwrite]

# citation edges — who cites this, or what it cites
fastpaper cite <id> [--direction incoming|outgoing] [-n 20]

# extract text from a PDF already on disk
fastpaper read papers/<id>.pdf [--section methods] [--max-length 4000]

fastpaper sources --capabilities            # what each source supports, live
```

`download` routes a bare identifier by its shape: arXiv→`arxiv`, PMC→`pmc`,
DOI→`semantic`; a PMID or a URL is rejected outright. Naming the source —
`fastpaper download europepmc <id>` — overrides the routing.

`cite` routes a bare DOI→`openalex` (no key needed), and arXiv or `S2:` ids→
`semantic`. Those two are the only sources here that carry citation edges.

`read` sections: abstract, introduction, methods, results, discussion,
conclusion, references, full.

**Search filters** — every one is validated per source. Asking for one a source
cannot honour is a hard error that names what it *does* support, so a filter is
never silently dropped:

| Flag | Meaning |
|---|---|
| `-n <N>` | max results (per-source caps differ; zenodo is 25, xueshu ~10) |
| `--offset <N>` | skip N; several sources require a multiple of `-n` |
| `--sort relevance\|date\|citations` + `--order asc\|desc` | ordering |
| `--year <YYYY>` · `--after <YYYY-MM-DD>` · `--before <YYYY-MM-DD>` | dates |
| `--author "<name>"` | author |
| `--field <code>` | subject/category — takes a *source-specific code*, see below |
| `--open-access` | OA only |
| `--patents` | **patents only** (europepmc, xueshu) |
| `-o <file>` | write to a file instead of stdout |

**Env vars.** `UNPAYWALL_EMAIL` is required by unpaywall. `SEMANTIC_SCHOLAR_API_KEY`
and `CORE_API_KEY` lift those two out of the throttled tier. `FASTPAPER_EMAIL`
joins the polite pool for crossref/openalex. Also `NCBI_API_KEY`, `OPENALEX_API_KEY`.

## Examples

```bash
# arXiv rewrites the query string, so category and date go in flags
fastpaper search arxiv "attention mechanism" --field cs.CL --after 2023-01-01 -n 20 --format json

# PubMed takes Entrez syntax verbatim — publication type and MeSH
fastpaper search pubmed 'CRISPR AND systematic review[pt] AND humans[mh]' -n 20 --format json

# Europe PMC: threshold on citations, then sort by them
fastpaper search europepmc 'CRISPR AND CITED:>500' --sort citations -n 20 --format json

# patents only
fastpaper search europepmc "gene editing" --patents -n 10 --format json

# Chinese literature — xueshu reads one page, so ~10 is its ceiling
fastpaper search xueshu "深度学习 医学影像" -n 8 --format json

# one paper: metadata, an OA link, the file, the text
fastpaper get 10.1038/nature12373 --format json
fastpaper get unpaywall 10.1038/nature12373 --format json     # → .pdf_url
fastpaper download 1706.03762 -d papers/
fastpaper read papers/1706.03762.pdf --section methods --max-length 4000

# citation edges
fastpaper cite 10.1038/nature12373 --direction incoming -n 30 --format json
```

## Choosing a source

Two independent axes. **Discipline coverage** decides what gets found at all;
**capability** decides whether a hit can be ranked, resolved or fetched. The
cross-discipline sources hold almost all the capability — `citations` on nearly
every hit (`semantic`, `openalex`), the most precise title→DOI lookup
(`crossref`), the highest OA full-text hit rate (`core`) — so they are not a
fallback behind the specialist sources, they answer a different question. For
several fields below there is no specialist source at all, and they are the
only option.

| Field | Dedicated sources | Cross-discipline sources that serve it well |
|---|---|---|
| CS · AI · ML | `arxiv` (`cs.*`), `dblp` | `semantic` (best CS coverage of the three, with citations), `openalex` |
| Math · physics · stats · quant bio/fin/econ | `arxiv` (`math.*` `physics.*` `stat.*` `q-bio.*` `q-fin.*` `econ.*` `eess.*`) | `openalex` `core` `hal` |
| Biomedicine · clinical | `pubmed` `pmc` `europepmc` `biorxiv` `medrxiv` | `semantic` `openalex` |
| Chemistry · materials · engineering | — | `openalex` `semantic` `crossref` `core` |
| Earth · environment · agriculture | `europepmc 'SRC:AGR'` (Agricola) | `openalex` `core` `doaj` |
| Humanities · social science | — | `openalex` `core` `doaj` `hal` (strong on French/European work) |
| Chinese literature | `xueshu` | `europepmc 'SRC:CBA'` (Chinese biomedical abstracts) |
| Patents | `europepmc --patents`, `xueshu --patents` | — |

`PDF` = `fastpaper download` works against this source. A `—` does not mean no
full text is reachable: `scholar`, `openalex`, `doaj` and `xueshu` all return a
`pdf_url` on some hits that you can fetch yourself. It means the CLI will not
fetch it for you.

`Cites` = does `citations` come back populated. `✓` nearly every hit · `~` only
under some conditions · `0` field present but 0 on most records · `—` never.

Even on a `✓` PDF source an individual record may hold no file — `pdf_url` is
`null` and the download fails with "No PDF URL found". That is normal rather
than a fault.

| Source | PDF | Cites | Use it for | Watch out |
|---|:--:|:--:|---|---|
| `arxiv` | ✓ | — | **CS, AI, math, physics, stats, q-bio, q-fin, econ** preprints, every one of them free to read | query syntax is rewritten — see below. `pdf_url` on every hit, but a DOI on only about half |
| `pubmed` | — | — | **biomedicine**, 35M+ records — the reference index for clinical and life-science work | abstracts only, no PDFs and no journal name; move to `pmc` or `europepmc` for either |
| `pmc` | ✓ | — | **biomedical full text** (NLM) — the OA subset of what pubmed indexes | PDFs come from the OA subset only, so a pubmed hit may have no pmc record |
| `europepmc` | ✓ | ~ | **widest biomedical** — 45M+ abstracts, 9M+ full text, plus EPO patents, NICE guidelines, Agricola, preprints and Chinese Biological Abstracts | richest query syntax here, and the only source that can threshold on citations (`CITED:>N`). Relevance-ranked hits are mostly uncited, so sort or threshold explicitly when you want impact |
| `biorxiv` | ✓ | — | **life-science preprints** (CSHL), full text on all of them | **no keyword search API** — browses a date window and matches locally, so `--after`/`--before` decide what is even searched |
| `medrxiv` | — | — | **medical / health preprints** (CSHL) | same date-window search as biorxiv, and its PDFs are blocked (403) — take the DOI elsewhere |
| `semantic` | ✓ | ✓ | **cross-discipline**, and the surest citation counts here — the basis for any ranking by impact | throttles hard without `SEMANTIC_SCHOLAR_API_KEY`. Carries a DOI on most hits and a PDF on about half |
| `openalex` | — | ✓ | **cross-discipline**, 200M+ works | `--field` takes a concept ID (`C154945302`), not a name. Rarely carries a PDF link — good for finding and ranking, not for fetching. Relevance drifts on title lookups |
| `crossref` | — | ✓ | **cross-discipline** DOI registry — the best title→DOI lookup here | registered metadata only: no PDFs, no OA status, and an abstract on few hits. Use it to resolve, then go elsewhere for content |
| `dblp` | — | — | **computer science** bibliography — conference and journal records, curated and clean | no abstracts at all; metadata only. The API takes a query and paging, nothing else |
| `core` | ✓ | 0 | **cross-discipline** OA aggregate, 400M+ from repositories and journals — a PDF on nearly every hit | a DOI on few hits and no journal name. `CORE_API_KEY` lifts the rate limit |
| `openaire` | — | ✓ | **cross-discipline** EU open science graph | no PDF links at all — resolve the DOI elsewhere. `get` wants an OpenAIRE id, not a DOI |
| `doaj` | — | — | **cross-discipline** peer-reviewed OA journals — complete metadata, with a journal name and abstract on nearly every hit | **its `pdf_url` is a landing page, not a file** — fetching one returns HTML. Year granularity only, no sorting |
| `hal` | ✓ | — | **cross-discipline** French national archive, with an abstract on nearly every hit and full text on most | `--field` takes a domain code: `math` `phys` `chim` `sdv` `shs` `spi` `info` `sde`. No journal name, and some records are metadata-only — filter on `pdf_url` before downloading |
| `zenodo` | ✓ | — | **cross-discipline** CERN general-purpose repository — papers, datasets and software, with a DOI on nearly every hit | `-n` capped at 25. No journal name, and some records are metadata-only |
| `unpaywall` | — | — | **DOI → a downloadable URL.** No search, no discipline — a resolver | needs `UNPAYWALL_EMAIL`; one DOI per call, and the URL must be fetched separately |
| `scholar` | — | — | broadest reach of anything here — it surfaces work the indexed sources miss, and a PDF link when nothing else has one | HTML scraping, captcha-prone; `doi` is always `null`, and only some hits carry a `pdf_url`. `[CITATION]` stubs have no link at all and are dropped |
| `xueshu` | — | 0 | **Chinese literature** — journals, master's/PhD theses, conference papers, patents, standards; 700M+ records over 500+ subjects. Indexes English work too, but other sources serve that better | unofficial endpoint with bot detection; search only, no `get`. **One request per search, first page only** — the page holds 10 records, so `-n` above ~10 silently returns fewer, and `--offset` past that page returns nothing. `pdf_url` is rare and `doi` present on about half, though validated, so a patent number or bare URL never masquerades as one |

**Two fields that lie.** `core` and `xueshu` return `citations` but leave it 0 on
most records, so sorting by it appears to work while silently burying the
well-cited papers. And `arxiv` carries a DOI on only about half its records, so
a DOI is not a stable key across sources.

### Content types worth routing on

Most content-type distinctions are not actionable. These are:

| Need | How |
|---|---|
| **Patents only** | `--patents` on `europepmc` or `xueshu` |
| **Chinese literature** | `xueshu`; Chinese biomedical also `europepmc 'SRC:CBA'` |
| **Preprints** | `arxiv`, `biorxiv`, `medrxiv`, or `europepmc 'SRC:PPR'` |
| **Clinical guidelines** | `europepmc 'SRC:CTX'` (NICE) |

`--patents` means patents only: with the flag you get patents, without it none,
never a mix. `europepmc` narrows natively; `xueshu` partitions its single page
locally, so either mode returns only that page's share and comes back short of
`-n` more often than the ~10 ceiling alone would explain. No other source takes
the flag — Google Scholar's own switch can only *widen* results to mix patents in,
never narrow to them, so the CLI leaves it off.

## Query syntax differs per source

This is the highest-leverage thing to get right, and the failure is silent.

**`arxiv` rewrites your query.** Every word becomes `all:{word}`, so `cat:cs.CL`
or `ti:"..."` inside the query string does nothing useful and returns unrelated
results **without any error**. Use the flags instead — `--field`, `--author`,
`--after`/`--before`.

arXiv categories for `--field`: `cs.CL` `cs.LG` `cs.CV` `cs.AI` `cs.RO`,
`math.*`, `physics.*`, `q-bio.*`, `q-fin.*`, `econ.*`, `stat.ML`, `eess.*`.

**`europepmc`, `pubmed`, `pmc`, `doaj`, `zenodo`, `hal`, `core` and `dblp` pass
your query through verbatim**, so their own field syntax works:

- `pubmed` / `pmc`: `[pt]` publication type · `[mh]` MeSH · `[tiab]` title/abstract · `[au]` author · `[dp]` date
- `europepmc`: `CITED:>N` · `AUTH:` · `PUB_YEAR:` · `OPEN_ACCESS:y` · `HAS_FT:y` · `LANG:` · `KW:` · `SRC:` subsets (`PPR` preprints, `CTX` NICE guidelines, `AGR` Agricola, `CBA` Chinese Biological Abstracts, `MED`, `PMC`)
- `doaj`: Lucene on `bibjson.*` · `zenodo`: Elasticsearch · `hal`: Solr · `dblp`: `year:` `author:` `venue:`

**`crossref`, `openalex`, `semantic`, `scholar`, `xueshu` are free-text only** —
relevance matching, no field syntax. Filter them with the CLI flags.

## Rate limits

Each `fastpaper search` is one process against one source, so a source that
fails or is throttled takes down only itself.

| Tier | Sources | Limit |
|---|---|---|
| **A — generous** | `arxiv` `europepmc` `crossref` `openalex` `dblp` `doaj` `zenodo` `hal` `openaire` | no auth needed; arXiv asks for ~3s between requests |
| **A with a key** | `semantic` (`SEMANTIC_SCHOLAR_API_KEY`: 1 req/s → 100), `core` (`CORE_API_KEY`) | without a key both belong in tier C — anonymous `semantic` has been observed failing outright with "rate limited after 5 retries" |
| **B — shared budget** | `pubmed` + `pmc` | one NCBI E-utilities budget between them: 3 req/s, 10 with `NCBI_API_KEY` |
| **C — serial only** | `xueshu` `scholar` | bot detection and captchas; they tolerate only low-frequency serial use |

## What this tool cannot do

1. **No impact factors.** No source provides JIF or quartiles.
2. **Citation edges come only from `semantic` and `openalex`**, via `cite`.
   Anything else you assemble (shared authors, venue, concepts) is a proxy — say
   so when you report it.
3. **It cannot fetch an arbitrary URL.** A `pdf_url` from `unpaywall`,
   `openalex`, `scholar` or `xueshu` has to be downloaded with something else.
4. **Missing fields mean unknown**, not zero and not absent.
