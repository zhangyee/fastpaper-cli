---
name: fastpaper
description: Search, download and read academic papers and patents from 18 sources (arXiv, PubMed, Europe PMC, Semantic Scholar, OpenAlex, CORE, Baidu Xueshu, ...). Use when the user asks to find papers, review a literature, track a field's frontier, trace citations, get a PDF, or read one. Covers patent search (--patents), citation graph edges (cite), and rate-aware parallel multi-source search.
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

**Search filters** — every one is validated per source. Asking for one a source
cannot honour is a hard error that names what it *does* support, so a filter is
never silently dropped:

| Flag | Meaning |
|---|---|
| `-n <N>` | max results (per-source caps differ; zenodo is 25) |
| `--offset <N>` | skip N; several sources require a multiple of `-n` |
| `--sort relevance\|date\|citations` + `--order asc\|desc` | ordering |
| `--year <YYYY>` · `--after <YYYY-MM-DD>` · `--before <YYYY-MM-DD>` | dates |
| `--author "<name>"` | author |
| `--field <code>` | subject/category — takes a *source-specific code*, see below |
| `--open-access` | OA only |
| `--patents` | **patents only** (europepmc, xueshu) |
| `-o <file>` | write to a file instead of stdout |

`read` sections: abstract, introduction, methods, results, discussion,
conclusion, references, full.

**Env vars.** `UNPAYWALL_EMAIL` is required by unpaywall. `SEMANTIC_SCHOLAR_API_KEY`
and `CORE_API_KEY` make those two usable in parallel rather than serial.
`FASTPAPER_EMAIL` joins the polite pool for crossref/openalex. Also
`NCBI_API_KEY`, `OPENALEX_API_KEY`.

## Choosing a source

Run `fastpaper sources --capabilities` when you need the live matrix. For
picking one:

`PDF` = can hand you the file itself; the rest give metadata only and need
their DOI passed on to a source that can (see Workflow 3). Even on a `✓`
source an individual record may hold no file — `pdf_url` is `null` and a
download fails with "No PDF URL found". That is normal, not a fault: pick
another hit, or fall through Workflow 3.

| Source | PDF | Use it for | Watch out |
|---|:--:|---|---|
| `arxiv` | ✓ | **CS, AI, math, physics, stats, q-bio, q-fin, econ** preprints, every one of them free to read | query syntax is rewritten — see below. `pdf_url` on every hit, but no citation counts at all, and a DOI on only about half |
| `pubmed` | — | **biomedicine**, 35M+ records — the reference index for clinical and life-science work | abstracts only, no PDFs and no journal name; move to `pmc` or `europepmc` for either. No citation counts |
| `pmc` | ✓ | **biomedical full text** (NLM) — the OA subset of what pubmed indexes | PDFs come from the OA subset only, so a pubmed hit may have no pmc record. No citation counts |
| `europepmc` | ✓ | **widest biomedical** — 45M+ abstracts, 9M+ full text, plus EPO patents, NICE guidelines, Agricola, preprints and Chinese Biological Abstracts | richest query syntax here, and the only source that can threshold on citations (`CITED:>N`). Relevance-ranked hits are mostly uncited, so sort or threshold explicitly when you want impact |
| `biorxiv` | ✓ | **life-science preprints** (CSHL), full text on all of them | **no keyword search API** — browses a date window and matches locally, so `--after`/`--before` decide what is even searched. No citation counts |
| `medrxiv` | — | **medical / health preprints** (CSHL) | same date-window search as biorxiv, and its PDFs are blocked (403) — take the DOI elsewhere. No citation counts |
| `semantic` | ✓ | **cross-discipline**, and the one source with citation counts on nearly every hit — the basis for any ranking by impact | unusable without `SEMANTIC_SCHOLAR_API_KEY`. Carries a DOI on most hits and a PDF on about half |
| `openalex` | — | **cross-discipline**, 200M+ works, citation counts on nearly every hit | `--field` takes a concept ID (`C154945302`), not a name. Rarely carries a PDF link — good for finding and ranking, not for fetching |
| `crossref` | — | **cross-discipline** DOI registry — the best title→DOI lookup here, and citation counts on most hits | registered metadata only: no PDFs, no OA status, and an abstract on few hits. Use it to resolve, then go elsewhere for content |
| `dblp` | — | **computer science** bibliography — conference and journal records, curated and clean | no abstracts at all and no citation counts; metadata only. The API takes a query and paging, nothing else |
| `core` | ✓ | **cross-discipline** OA aggregate, 400M+ from repositories and journals — a PDF on nearly every hit | a DOI on few hits, no journal name, and `citations` present but 0 on most records — never rank on it. `CORE_API_KEY` makes it usable in parallel |
| `openaire` | — | **cross-discipline** EU open science graph, with citation counts on most hits | no PDF links at all — resolve the DOI elsewhere. `get` wants an OpenAIRE id, not a DOI |
| `doaj` | — | **cross-discipline** peer-reviewed OA journals — complete metadata, with a journal name and abstract on nearly every hit | **its `pdf_url` is a publisher landing page, not a file** (returns HTML), which is why `download` is unavailable here. Year granularity only, no sorting, no citation counts |
| `hal` | ✓ | **cross-discipline** French national archive, with an abstract on nearly every hit and full text on most | `--field` takes a domain code: `math` `phys` `chim` `sdv` `shs` `spi` `info` `sde`. No journal name, no citation counts, and some records are metadata-only — filter on `pdf_url` before downloading |
| `zenodo` | ✓ | **cross-discipline** CERN general-purpose repository — papers, datasets and software, with a DOI on nearly every hit | `-n` capped at 25. No journal name, no citation counts, and some records are metadata-only |
| `unpaywall` | — | **DOI → a downloadable URL.** No search, no discipline — a resolver you reach for in Workflow 3 | needs `UNPAYWALL_EMAIL`; one DOI per call, and the URL must be fetched separately |
| `scholar` | — | broadest reach of anything here; scouting, and a PDF link when nothing else has one | HTML scraping, captcha-prone; `doi` is always `null`, and only some hits carry a `pdf_url` |
| `xueshu` | — | **Chinese literature** — journals, master's/PhD theses, conference papers, patents, standards; 700M+ records over 500+ subjects. Indexes English work too, but other sources serve that better | unofficial endpoint with bot detection, so keep it serial and low-frequency; search only, no `get`. `citations` is nearly always 0 — never rank on it. `pdf_url` is rare and `doi` present on about half, though validated, so a patent number or bare URL never masquerades as one |

### Content types worth routing on

Most content-type distinctions are not actionable. These are:

| Need | How |
|---|---|
| **Patents only** | `--patents` on `europepmc` or `xueshu` |
| **Chinese literature** | `xueshu`; Chinese biomedical also `europepmc 'SRC:CBA'` |
| **Preprints** | `arxiv`, `biorxiv`, `medrxiv`, or `europepmc 'SRC:PPR'` |
| **Clinical guidelines** | `europepmc 'SRC:CTX'` (NICE) |

`--patents` means patents only: with the flag you get patents, without it none,
never a mix. `europepmc` narrows natively; `xueshu` filters locally and pages
further to fill `-n`, so keep `-n` modest there. No other source takes the flag
— Google Scholar's own switch can only *widen* results to mix patents in, never
narrow to them, so the CLI leaves it off.

## Query syntax differs per source

This is the highest-leverage thing to get right, and the failure is silent.

**`arxiv` rewrites your query.** Every word becomes `all:{word}`, so `cat:cs.CL`
or `ti:"..."` inside the query string does nothing useful and returns unrelated
results **without any error**. Use the flags instead:

```bash
fastpaper search arxiv "attention mechanism" --field cs.CL --after 2023-01-01 -n 20 --format json
```

arXiv categories for `--field`: `cs.CL` `cs.LG` `cs.CV` `cs.AI` `cs.RO`,
`math.*`, `physics.*`, `q-bio.*`, `q-fin.*`, `econ.*`, `stat.ML`, `eess.*`.

**`europepmc`, `pubmed`, `pmc`, `doaj`, `zenodo`, `hal`, `core` and `dblp` pass
your query through verbatim**, so their own field syntax works:

```bash
# publication type and MeSH — the precision tools for clinical evidence
fastpaper search pubmed 'CRISPR AND systematic review[pt] AND humans[mh]' -n 20 --format json

# citation threshold, and subsets nothing else exposes
fastpaper search europepmc 'CRISPR AND CITED:>500' --sort citations -n 20 --format json
fastpaper search europepmc 'SRC:PPR AND large language model' -n 20 --format json
```

- `pubmed` / `pmc`: `[pt]` publication type · `[mh]` MeSH · `[tiab]` title/abstract · `[au]` author · `[dp]` date
- `europepmc`: `CITED:>N` · `AUTH:` · `PUB_YEAR:` · `OPEN_ACCESS:y` · `HAS_FT:y` · `LANG:` · `KW:` · `SRC:` subsets (`PPR` preprints, `CTX` NICE guidelines, `AGR` Agricola, `CBA` Chinese Biological Abstracts, `MED`, `PMC`)
- `doaj`: Lucene on `bibjson.*` · `zenodo`: Elasticsearch · `hal`: Solr · `dblp`: `year:` `author:` `venue:`

**`crossref`, `openalex`, `semantic`, `scholar`, `xueshu` are free-text only** —
relevance matching, no field syntax. Filter them with the CLI flags.

## Searching several sources at once

One `fastpaper search` per source, issued as concurrent calls — each is an
independent process, so a source that fails or is rate-limited loses only
itself. Keep them as separate calls rather than bundling them into one shell
command: you then see which source failed, instead of hunting for it inside a
single blob of output.

Fan out, but not uniformly — the limits differ enormously:

| Tier | Sources | Rule |
|---|---|---|
| **A — fan out freely** | `arxiv` `europepmc` `crossref` `openalex` `dblp` `doaj` `zenodo` `hal` `openaire` | up to 4 at once |
| **A with a key** | `semantic`, `core` | without one they belong in tier C |
| **B — one slot between them** | `pubmed` + `pmc` | they share one NCBI budget of 3 req/s |
| **C — one at a time, last** | `xueshu`, `scholar` | bot detection / captchas |

**When merging results, dedupe on the normalised title, not the DOI**: arxiv
records mostly have no DOI, so DOI-first grouping splits one paper into two.
How many sources returned the same paper is a useful importance signal.

## Workflow 1 — finding papers

1. Pick 3–5 sources. Include **at least one that ranks by citations**
   (`semantic` or `openalex`) and **one that can download** (`arxiv` `pmc`
   `europepmc` `biorxiv` `semantic` `core` `zenodo` `hal`).
2. Fan out per the tier table; add tier C serially if you need Chinese coverage.
3. Merge and dedupe by title.
4. Report the total, each source's contribution, and the papers several sources agree on.

When the terminology is uncertain or the field is unfamiliar, **scout first** to
learn the real terms of art, key authors and main venues, then search properly
with those terms. Any of `WebSearch`, `scholar` (broadest reach, but captcha-prone
— expect it to fail sometimes) or `xueshu` (Chinese) will do. Do not stop at the
scout: none of them report citation counts, they often have no DOI, and they
cannot sort or filter.

## Workflow 2 — tracking a field

Rank on citations from `semantic` or `openalex` — both populate the field on
nearly every hit. `crossref` and `openaire` carry it on most, and
`europepmc` only if you sort or threshold with `CITED:>N`, since its
relevance-ranked hits are mostly uncited.

**Never quote a citation count from** `arxiv` `pubmed` `pmc` `dblp` `doaj`
`zenodo` `hal` `biorxiv` `medrxiv` `scholar` — they have none — nor from `core`
or `xueshu`, which return the field but leave it 0 on most records. That second
group is the dangerous one: ranking appears to work and silently buries the
well-cited papers.

Run the same query three ways:

| Layer | Query | Yields |
|---|---|---|
| Foundational | `--sort citations`, no date limit | the classics |
| Frontier | `--after <today−18mo> --sort date` | newest work |
| **Hot** | `--after <today−3y> --sort citations` | recently highly cited — the real hotspots |

Then walk the citation graph from the best hits:

```bash
fastpaper cite <DOI> -n 30 --format json                      # who cites it
fastpaper cite <DOI> --direction outgoing -n 30 --format json # what it cites
```

A bare DOI routes to `openalex` (no key); arXiv and `S2:` ids route to
`semantic`. Only those two carry edges. Snowball outward from the highest-cited
nodes; two hops is usually enough, and each hop multiplies requests.

**Ranking without an impact factor** — no source here provides one. Use
`citations`, **citations per year** = `citations / (this_year − year + 1)` (the
fair measure for recent work, and the best hotspot signal), how many sources
agree, and the venue name stated plainly.

## Workflow 3 — getting the PDF

Go down a step only when the one above fails.

| Step | Action |
|---|---|
| 0 | **Get an identifier.** Title only → `fastpaper search crossref "<title>" -n 3` for the DOI (more precise than openalex, whose relevance drifts); Chinese title → `xueshu` |
| 1 | `fastpaper download <id>` — routes arXiv→arxiv, PMC→pmc, DOI→semantic |
| 2 | Name a source: `fastpaper download europepmc\|core\|zenodo\|hal\|biorxiv\|arxiv\|pmc <id>` |
| 3 | **`fastpaper get unpaywall <DOI>`** → take `.pdf_url` → fetch it yourself |
| 4 | `fastpaper get openalex <id>` → `.pdf_url` → fetch it yourself |
| 5 | `fastpaper search scholar "<title>"` → `.pdf_url` when the hit has one; many do, and they are often copies the DOI-based steps above missed |
| 6 | Chinese paper → `fastpaper search xueshu "<title>"` → `.pdf_url`. Rarely present, so treat it as a long shot |
| 7 | Report the landing page — `url` from step 5 or 6 — and which steps you tried. **Never claim a download that did not happen.** |

Steps 3–5 return a URL, not a file — `fastpaper` cannot download an arbitrary
URL, so fetch it yourself. Keep the server's own filename (`-OJ`) rather than
inventing one; rename only if it collides with a file already there:

```bash
curl -sLOJ "<pdf_url>"                  # e.g. https://arxiv.org/pdf/1304.1068 -> 1304.1068v1.pdf
fastpaper read <file> --max-length 200
```

That `read` doubles as the check: on anything that is not really a PDF it fails
with `Failed to open PDF`, and you were going to read the paper anyway. Leave
`--section` off for this — section detection depends on the paper's layout and
can miss even on a perfectly good file, which would look like a failed download.
`fastpaper download` already refuses to write HTML, so its own successes need no
check.

`scholar` sits last because it is the least dependable, not the least useful:
it reports whichever free PDF Google found, which is often one the DOI-based
steps above missed, but it is captcha-prone and gives no `doi`. Only some hits
have a `pdf_url` — the rest carry `url`, the publisher's page. `[CITATION]`
stubs have no link at all and are dropped.

## What this tool cannot do

1. **No impact factors.** No source provides JIF or quartiles — use the proxies above.
2. **Citation edges come only from `semantic` and `openalex`.** Anything else you
   assemble (shared authors, venue, concepts) is a proxy — say so when you report it.
3. **Missing fields mean unknown**, not zero and not absent.
