---
name: fastpaper
description: Search, download, and read academic papers and patents across 18 sources (arXiv, PubMed, Europe PMC, Semantic Scholar, OpenAlex, Baidu Xueshu, ...). Use when the user asks about research papers, literature search, citation analysis, tracking a field's frontier, finding a PDF, or reading one. Routes by discipline, content type and identifier; supports native per-source query syntax and rate-aware parallel multi-source search.
---

# fastpaper

Academic search / download / read across 18 sources.

`fastpaper` is a pre-installed standalone CLI. Verify once with `which fastpaper`.
It is NOT a Python package — never try to `pip install` it.

Always pass `--format json`. Output shape is `{"source": "...", "results": [...]}`,
so jq paths start at `.results[]`. Exit code 0 = success. Missing fields are
`null` — report them as unknown, never fill them in from memory.

Full per-source detail (native syntax, discipline codes, quirks):
[references/source-matrix.md](references/source-matrix.md). Read it before
constructing a non-trivial query for a specific source.

## Choosing sources

Ask three questions, in this order.

### 1. What content type? (decides more than discipline does)

| Need | Source |
|---|---|
| **Patents only** | `--patents` on `europepmc` (EPO, native) or `xueshu` (Chinese patents) |
| Patents excluded | the default everywhere — no source mixes them in unasked |
| Preprints | `arxiv`, `biorxiv`, `medrxiv`, or `europepmc 'SRC:PPR'` |
| Clinical guidelines (NICE) | `europepmc 'SRC:CTX'` |
| Agriculture (Agricola) | `europepmc 'SRC:AGR'` |
| Chinese literature | `xueshu`; Chinese biomedical also `europepmc 'SRC:CBA'` |
| Datasets / software | `zenodo` |
| Theses, institutional repositories | `core`, `hal`, `openaire` |

`--patents` means **patents only**, and results never mix: with the flag you get
patents, without it you get none. So the record itself needs no type label —
the query already tells you what you asked for.

```bash
fastpaper search europepmc 'CRISPR' -n 20 --patents --format json   # -> WO…/US… patent numbers
fastpaper search xueshu '锂离子电池正极材料' -n 20 --patents --format json
```

`europepmc` filters natively (`SRC:PAT`). `xueshu` has no server-side filter, so
it splits locally and **pages further to fill `-n`** — keep `-n` modest there,
it is a rate-limited endpoint. Asking any other source for `--patents` is an
error naming these two.

`scholar` cannot do patents at all here: Google's switch only *widens* results to
mix patents in, never narrows to them, so the CLI leaves it off.

### 2. What discipline?

- **CS / AI / math / physics** — `arxiv`; `dblp` (CS only, metadata only)
- **Biomedical / life sciences** — `europepmc` (widest), `pubmed` (abstracts),
  `pmc` (full text), `biorxiv`, `medrxiv`
- **Cross-discipline** — `openalex` (200M+ works), `crossref` (DOI registry),
  `semantic` (best citation data, needs a key)
- **Open access** — `core` (400M+), `doaj`, `openaire`, `unpaywall` (resolver only)
- **Repositories** — `zenodo` (CERN), `hal` (French, multi-disciplinary)

### 3. What role does each source play?

| Role | Sources |
|---|---|
| **Web-scale scout** — fuzzy, no field syntax, mixed content types. Use only to gather terminology and landmark papers; never as the final search | `WebSearch` (your own tool, most reliable), `xueshu`, `scholar` |
| **Discovery** — real API search, filterable, reproducible | `openalex`, `europepmc`, `semantic`, `crossref`, `arxiv`, `pubmed`, `dblp` |
| **Full text** — can actually produce a PDF | `arxiv`, `pmc`, `europepmc`, `biorxiv`, `semantic`, `core`, `zenodo`, `hal` |
| **Resolver** — no search, identifier lookup only | `unpaywall` (DOI→OA link), `crossref` (DOI→metadata) |
| **Index only** — metadata, no full text; hand the DOI to a full-text source | `pubmed`, `dblp`, `crossref`, `openalex`, `openaire`, `doaj`, `medrxiv` |

## Native query syntax

Whether `fastpaper` passes your query string through verbatim differs per source.
Using a source's native syntax is usually worth far more than any CLI flag.

| | Sources | What you can write |
|---|---|---|
| **Passed through** | `europepmc` | `SRC:` `CITED:>N` `AUTH:` `PUB_YEAR:` `OPEN_ACCESS:y` `HAS_FT:y` `PUB_TYPE:` `LANG:` `KW:` |
| | `pubmed`, `pmc` | Entrez tags: `[pt]` `[mh]` `[tiab]` `[au]` `[dp]` |
| | `doaj` | Lucene: `bibjson.year:` `bibjson.subject.term:` |
| | `zenodo`, `hal`, `core`, `dblp` | Elasticsearch / Solr / their own field syntax |
| **NOT passed through** | `arxiv` | every word is wrapped as `all:{word}` — `ti:`/`cat:` in the query do nothing. Use `--field cs.CL` and `--author` instead |
| **Free text only** | `crossref`, `openalex`, `semantic`, `scholar`, `xueshu` | relevance matching; filter with CLI flags |

```bash
fastpaper search europepmc 'CRISPR AND CITED:>500' --sort citations -n 20 --format json
fastpaper search pubmed 'CRISPR AND systematic review[pt]' -n 20 --format json
```

CLI filters (`--year`, `--after`/`--before`, `--author`, `--field`,
`--open-access`, `--sort`, `--offset`) are validated per source: one a source
cannot honour is an error naming what it does support, never a silent no-op.
Run `fastpaper sources --capabilities` for the matrix.

## Running searches in parallel

Multi-source search is the main speedup, but **do not fan out uniformly** —
limits differ enormously. Use whatever parallelism your runtime supports
(parallel tool calls within one response, or shell backgrounding in a single
command), respecting these tiers:

| Tier | Sources | Rule |
|---|---|---|
| **A — parallel** (≤4 at once) | `arxiv`, `europepmc`, `crossref`, `openalex`, `dblp`, `doaj`, `zenodo`, `hal`, `openaire` | no auth needed |
| **A when a key is set** | `semantic` (`SEMANTIC_SCHOLAR_API_KEY`, 1→100 req/s), `core` (`CORE_API_KEY`) | promote out of tier C |
| **B — one shared slot** | `pubmed` + `pmc` | they share one NCBI budget of 3 req/s (10 with `NCBI_API_KEY`); together they count as a single slot |
| **C — always serial, run last** | `xueshu`, `scholar` | `xueshu` has bot detection and needs low frequency; `scholar` is HTML scraping and trips captchas |
| **C without a key** | `semantic`, `core` | `semantic` without a key fails outright: `rate limited after 5 retries` |

```bash
mkdir -p out
for s in arxiv europepmc crossref openalex; do
  fastpaper search $s "<query>" -n 20 --format json -o out/$s.json 2>out/$s.err &
done
wait
fastpaper search xueshu "<中文 query>" -n 20 --format json -o out/xueshu.json   # tier C, serial
```

One source failing loses only that source; check `out/*.err` and skip empty files.

**Merge and dedupe by normalised title, not by DOI.** `arxiv` records mostly
lack DOIs, so DOI-first grouping splits the same paper into two entries:

```bash
jq -s '[.[].results[]]
  | group_by(.title | ascii_downcase | gsub("[^a-z0-9]"; "") | .[0:60])
  | map({ title: .[0].title,
          doi:   ([.[] | .doi // empty] | first),
          cites: ([.[] | .citations // 0] | max),
          pdf:   ([.[] | .pdf_url // empty] | first),
          seen:  (map(.source) | unique) })
  | sort_by(-(.seen|length), -.cites)' out/*.json
```

`seen | length` is the cross-source agreement count — a useful importance signal.

## Workflow 1 — literature search

**Mode A · parallel fan-out** (default)

1. Pick 3–5 sources from the three questions above. Include **at least one that
   reports citations** and **one that can download**.
2. Fan out per the tier table.
3. Add tier C serially if you need Chinese coverage or an OA fallback.
4. Merge with the jq recipe.
5. Report total hits, per-source contribution, and the cross-source top N.

**Mode B · scout then refine** (terminology uncertain, unfamiliar field, or a
vague description from the user)

1. **Scout** with `WebSearch` (most reliable), `xueshu` (Chinese), or `scholar`
   (best effort) → real terms of art, key authors, landmark papers, main venues.
2. **Refine** into 3–5 query variants, a time window, and discipline codes
   (arXiv category / HAL domain / OpenAlex concept ID).
3. **Search for real**: native syntax on `europepmc`/`pubmed`, `--field`/`--author`
   on `arxiv`, structured flags on `openalex`/`crossref`.
4. Merge as in Mode A.

Never stop at step 1 — the scout tier reports no citation counts, often no DOI,
and cannot sort or filter.

## Workflow 2 — frontier tracking / knowledge map

**Sources that report `citations`:** `semantic` (richest by far, key required),
`europepmc`, `openalex`, `crossref`, `openaire`, `core` (often 0), `xueshu` (often 0).

**Never report citation counts from** `arxiv`, `pubmed`, `pmc`, `dblp`, `doaj`,
`zenodo`, `hal`, `biorxiv`, `medrxiv`, `scholar` — they have none.

Run the same query three ways:

| Layer | Query | Yields |
|---|---|---|
| Foundational | `--sort citations`, no date limit | the classics |
| Frontier | `--after <today−18mo> --sort date` | newest work |
| **Hot** | `--after <today−3y> --sort citations` | **recently highly cited = the actual hotspots** |

On `europepmc` you can threshold directly with `CITED:>N`.

**Importance proxies** — this tool exposes no impact factor anywhere:

- `citations`, absolute
- **citations per year = `citations / (current_year − year + 1)`** — the primary
  hotspot signal, and fair to recent papers
- cross-source agreement (`seen | length`)
- venue name — state it; never assert a journal's ranking

**Graph edges — use `fastpaper cite`, which returns real edges.**

```bash
fastpaper cite <DOI|arXiv|S2:id> -n 30 --format json                        # who cites it
fastpaper cite <id> --direction outgoing -n 30 --format json               # what it cites
fastpaper cite semantic <id> -n 30 --format json                           # richer, needs the key
```

A bare DOI routes to `openalex` (no key required); an arXiv or `S2:` id routes
to `semantic`. Only those two carry edges — every other source errors and says so.

Snowball a map from a seed: walk `--direction outgoing` for the foundations it
rests on, then incoming for what came after, then repeat on the highest-cited
nodes. Two hops is usually enough; each hop multiplies requests, so keep `-n`
tight and respect the tier table.

Proxy edges are now a **supplement**, not a substitute — reach for them only to
connect nodes citation data misses (very recent preprints, non-indexed venues):
co-authorship · shared venue · shared field/concept (`fields` carries OpenAlex
concepts, arXiv categories, Semantic Scholar fieldsOfStudy) · title/abstract term
co-occurrence. **Label proxy edges as proxies in the report**; real `cite` edges
need no such caveat.

Report as: foundations / mainline methods / current frontier / disputes and gaps.
Every claim carries its evidence — citations, citations-per-year, year, source, DOI.

## Workflow 3 — getting the PDF

Descend a level only when the level above fails.

| Step | Action |
|---|---|
| 0 | **Get an identifier.** Have a DOI / arXiv ID / PMC ID → step 1. Title only → `fastpaper search crossref "<title>" -n 3` for the DOI (more precise than openalex, whose relevance drifts). Chinese title → `search xueshu` |
| 1 | `fastpaper download <id>` — routes arXiv→arxiv, PMC→pmc, DOI/S2→semantic |
| 2 | Name a source: `fastpaper download europepmc\|core\|zenodo\|hal\|biorxiv\|arxiv\|pmc <id>` |
| 3 | `fastpaper get unpaywall <DOI>` → take `.pdf_url` → `curl` it |
| 4 | `fastpaper get openalex <id>` or `search openalex "<title>"` → `.pdf_url` → `curl` |
| 5 | `fastpaper search xueshu "<title>"` → `.pdf_url` → `curl` |
| 6 | All failed → report the landing page URL and which steps you tried. **Never claim a download succeeded when it did not.** |

`fastpaper` cannot download a bare URL, so steps 3–5 need `curl`. Verify what
you fetched really is a PDF:

```bash
curl -sL "<pdf_url>" -o papers/out.pdf && head -c 4 papers/out.pdf   # must print %PDF
```

`fastpaper download` already refuses to write HTML, so its successes are real PDFs.

**`scholar` is not part of this chain** — its parser never emits `pdf_url` or
`doi`, and `[CITATION]` stub entries have no link at all. Use it to discover, not to fetch.

## Base commands

```bash
fastpaper search <source> <query> -n 20 --format json [-o file]
fastpaper get <DOI|arXiv|PMID|PMC_ID>          # source inferred from the id
fastpaper get <source> <id>                    # or name it (15 sources support get)
fastpaper download <id> [-d dir]               # -> ./papers/<id>.pdf
fastpaper cite <id> [--direction incoming|outgoing] [-n 20]   # citation edges
fastpaper read papers/<id>.pdf [--section methods] [--max-length 4000]
fastpaper sources --capabilities
```

`read` works on a **local PDF only** — download first, then read what landed.
Sections: abstract, introduction, methods, results, discussion, conclusion,
references, full. For metadata rather than full text use `get`; it never
downloads a file.

Keys and env vars that change behaviour: `UNPAYWALL_EMAIL` (required by
unpaywall), `SEMANTIC_SCHOLAR_API_KEY` and `CORE_API_KEY` (promote those sources
to tier A), `FASTPAPER_EMAIL` (polite pool for crossref/openalex),
`NCBI_API_KEY`, `OPENALEX_API_KEY`.

## Hard boundaries

1. **No impact factors.** No source here provides JIF or quartile. Use the proxies above.
2. **Citation edges come only from `semantic` and `openalex`** (`fastpaper cite`).
   Edges you build any other way are proxies — say so.
3. **`--patents` is patents-only and works on `europepmc` and `xueshu` alone.**
   Results are never mixed, so no per-record type label exists or is needed.
4. **Verify curl'd PDFs** with the `%PDF` check.
5. **Missing fields are `null`** and mean unknown, not zero and not absent.
