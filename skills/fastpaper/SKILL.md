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
| `arxiv` | ✓ | CS, AI, math, physics, stats, q-bio, q-fin, econ preprints | query syntax is rewritten — see below |
| `pubmed` | — | biomedicine, 35M+ records | abstracts only, no PDFs |
| `pmc` | ✓ | biomedical **full text** | PDFs from the OA subset only |
| `europepmc` | ✓ | **widest biomedical** — 45M+ abstracts, 9M+ full text, plus EPO patents, NICE guidelines, Agricola, preprints, Chinese Biological Abstracts | richest query syntax of any source here |
| `biorxiv` | ✓ | life-science preprints | **no keyword search API** — browses a date window and matches locally, so `--after`/`--before` decide what is even searched |
| `medrxiv` | — | medical / health preprints | same date-window search as biorxiv; its PDFs are blocked (403), so route the DOI elsewhere |
| `semantic` | ✓ | **cross-discipline**; the best citation data | unusable without `SEMANTIC_SCHOLAR_API_KEY` |
| `openalex` | — | **cross-discipline**, 200M+ works | `--field` takes a concept ID (`C154945302`), not a name |
| `crossref` | — | **cross-discipline** DOI registry | best title→DOI lookup here; no PDFs, no OA status |
| `dblp` | — | computer science bibliography | CS only, metadata only, no abstracts |
| `core` | ✓ | **cross-discipline** OA aggregate, 400M+ from repositories and journals | `citations` is often 0 — do not rank on it |
| `openaire` | — | **cross-discipline** EU open science graph | `get` wants an OpenAIRE id, not a DOI |
| `doaj` | — | **cross-discipline** peer-reviewed OA journals | year granularity only, no sorting |
| `hal` | ✓ | **cross-discipline** French national archive | `--field` takes a domain code: `math` `phys` `chim` `sdv` `shs` `spi` `info` `sde`; many records are metadata-only, so filter on `pdf_url` before downloading |
| `zenodo` | ✓ | **cross-discipline** CERN general-purpose repository — papers, datasets, software | `-n` capped at 25; like hal, many records are metadata-only |
| `unpaywall` | — | **DOI → a downloadable URL.** No search | the URL must be fetched separately; see Workflow 3 |
| `scholar` | — | broadest reach of anything here; scouting and last-resort landing pages | HTML scraping, captcha-prone; `doi` and `pdf_url` always `null` (parser gap, see Workflow 3) |
| `xueshu` | — | **Chinese literature** — journals, theses, patents | unofficial API, bot detection; keep it serial |

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

1. Pick 3–5 sources. Include **at least one that reports citations**
   (`semantic` `europepmc` `openalex` `crossref` `openaire`) and **one that can
   download** (`arxiv` `pmc` `europepmc` `biorxiv` `semantic` `core` `zenodo` `hal`).
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

Sources reporting `citations`: `semantic` (richest, needs the key), `europepmc`,
`openalex`, `crossref`, `openaire`. **Never quote a citation count from**
`arxiv` `pubmed` `pmc` `dblp` `doaj` `zenodo` `hal` `biorxiv` `medrxiv`
`scholar` — they have none.

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
| 5 | `fastpaper search xueshu "<title>"` → `.pdf_url` → fetch it yourself |
| 6 | Out of automatic options. `fastpaper search scholar "<title>"` often still finds a landing page in its `url`; report that plus which steps you tried. **Never claim a download that did not happen.** |

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

`scholar` cannot serve step 3–5: its `pdf_url` and `doi` are always `null`. Note
this is a gap in *this* scraper, not in Google Scholar — the results page does
carry direct PDF links (in a `div.gs_ggs` block beside each hit) and the parser
simply does not read them. What scholar does return is `url`, the publisher
landing page, which is worth having at step 6 when everything else came up
empty. `[CITATION]` stubs have no link at all and are dropped.

## What this tool cannot do

1. **No impact factors.** No source provides JIF or quartiles — use the proxies above.
2. **Citation edges come only from `semantic` and `openalex`.** Anything else you
   assemble (shared authors, venue, concepts) is a proxy — say so when you report it.
3. **Missing fields mean unknown**, not zero and not absent.
