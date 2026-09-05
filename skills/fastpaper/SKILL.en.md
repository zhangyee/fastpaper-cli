---
name: fastpaper
description: Use when the user asks to find academic papers or patents, survey a literature, look up a paper by DOI, arXiv id, PMID or PMC id, trace who cites what, get a paper's PDF, fetch its original figure files, or read one. Covers 23 sources — arXiv, PubMed, PMC, Europe PMC, bioRxiv, medRxiv, OSF Preprints, Semantic Scholar, OpenAlex, Crossref, DataCite, DBLP, CORE, OpenAIRE, DOAJ, HAL, Zenodo, Unpaywall, INSPIRE-HEP, zbMATH Open, ERIC, OSTI.GOV, NASA NTRS.
---

# fastpaper

A pre-installed CLI for academic search, download and reading. Verify once with
`which fastpaper`; it is NOT a Python package, never `pip install` it.

Pass `--format json` on every call. Output is `{"source": "...", "results": [...]}`
— the papers are under `results`. Exit codes are `0` success, `2` a malformed
command, `4` **nothing to return** — no such paper, no `--grep` match, no PDF
for this paper at this source, no figure files for this paper — and `1` for
everything else. Branch on `4`: it
means the request was fine and this source simply has nothing, so retry
elsewhere rather than rewording. Any field
the source did not supply is `null`, meaning **unknown**; report it as unknown
rather than filling it in.

`null` covers two opposite situations, and the record cannot tell them apart:
the source has nothing to say about that field *at all*, or it does carry the
field and has nothing for this paper. `fastpaper sources` separates them — its
`pdf_url` / `open_access` / `citations` columns say which fields each source can
fill. A `null` from a source marked `✗` means **ask a different source**; from
one marked `✓` it means the answer really is unknown. Checking that column first
is cheaper than a download attempt that was never going to work.

## Commands

```bash
# search one source
fastpaper search <source> "<query>" -n 20 --format json

# metadata for one paper; source inferred from the identifier shape
fastpaper get <DOI|arXiv_ID|PMID|PMC_ID>
fastpaper get <source> <id>                 # or name the source

# save a PDF (default ./papers/<id>.pdf)
fastpaper download <id> [-d <dir>] [--overwrite]

# save the authors' original figure files (default ./papers/<id>/)
fastpaper figures <id> -d papers/ [--overwrite]

# citation edges — who cites this, or what it cites
fastpaper cite <id> [--direction incoming|outgoing] [-n 20]

# extract text from a PDF already on disk
fastpaper read papers/<id>.pdf --list-sections        # which sections are there
fastpaper read papers/<id>.pdf [--section methods] [--max-length 4000]

fastpaper sources --capabilities            # what each source supports, live
```

`get` routes a bare identifier by its shape: arXiv→`arxiv`, PMC→`pmc`,
PMID→`pubmed`, DOI→`crossref`, `S2:`→`semantic`.

`download` routes a bare identifier by its shape: arXiv→`arxiv`, PMC→`pmc`,
DOI→`semantic`; a PMID or a URL is rejected outright. Naming the source —
`fastpaper download europepmc <id>` — overrides the routing.

**The two disagree on DOIs, on purpose**: `get` sends one to `crossref`, the
registrar with the most authoritative metadata, and `download` sends it to
`semantic`, which resolves open access copies — crossref serves no files. The
consequence for you is that `get <DOI>` shows a crossref record whose `pdf_url`
is always `null`, while `download <DOI>` uses a link from a semantic record you
never saw. **Do not read `pdf_url` from `get <DOI>` to predict whether
`download` will work.** To see the link download would use, ask that source
directly: `fastpaper get semantic DOI:<doi>`.

`cite` routes a bare DOI→`openalex` (no key needed), and arXiv or `S2:` ids→
`semantic`. Those two are the only sources here that carry citation edges.

`figures` fetches the authors' **original** figure files — an arXiv source
package or a Europe PMC supplementary package — it does not extract images
from the PDF, render anything, or parse figure numbers. Only `arxiv` and
`europepmc` can provide them; a PMC ID or a DOI both route to `europepmc` (a
DOI is resolved to a PMC ID first). Naming any other source is exit `1`; a
paper with no figure files at a supported source is exit `4`. **Filenames are
kept exactly as they appear in the archive, so they do not correspond to
figure numbers** — for `2511.11035`, the file `3.pdf` is actually Figure 1;
do not assume `1.pdf` is Figure 1. Measured on a 39-paper corpus, verified
end-to-end on 2026-08-20: 31 papers (79%) yielded figure files, so treat a
`4` here as normal, not as a sign something is broken.

`read` sections: abstract, introduction, methods, results, discussion,
conclusion, references, full.

**A PDF records no section structure** — `--section` infers it from the
typography, so it can fail to find a section on an unusual layout. It exits 4
rather than returning something else, but **check before you quote**: run
`fastpaper read <pdf> --list-sections` first, and only ask for a section that
appears in the list. Under `--format json` a section read reports the heading
its slice began at (`heading.text`, `heading.offset`), which is what you
verify a quotation against.

**When the list has no `abstract` or `introduction`**, that is usually the
paper, not the tool: Nature and its family print the abstract with no heading
over it at all. Read the opening instead — it is the same few thousand
characters you were after:

```
fastpaper read papers/<id>.pdf --max-length 3000
```

The point of `--section` is to keep the paper out of your context, and the
opening of a paper is its abstract and the start of its introduction. Falling
back to it costs the same as the section would have.

Two limits to plan around. Running heads, folios and journal footers are left
in the text and can land in the middle of a quoted sentence — strip them
yourself if you are quoting. And "the section was found" is not "the section
is right": if a passage matters, confirm it with
`--section full --grep '<phrase>'`.

**Search filters** — every one is validated per source. Asking for one a source
cannot honour is a hard error that names what it *does* support **and which
sources do have the filter you asked for**, so a filter is never silently
dropped and the retry is written out for you. That message is on stderr — never
run these commands with `2>/dev/null`, or an exit code is all you get back:

| Flag | Meaning |
|---|---|
| `-n <N>` | max results (per-source caps differ; zenodo is 25) |
| `--offset <N>` | skip N; several sources require a multiple of `-n` |
| `--sort relevance\|date` + `--order asc\|desc` | ordering — not every source accepts `--sort`; those that do not raise an explicit error |
| `--sort citations` | **only `europepmc`, `semantic`, `crossref`, `openalex`, `openaire`, `inspire` have citation counts to sort on**, and that is the whole list; the rest either raise an explicit error (`arxiv`, `pubmed`, `pmc`, `zenodo`, `hal`, `osf`, `osti`) or do not take `--sort` at all. When in doubt run `fastpaper sources --capabilities`, whose Notes spell it out per source |
| `--year <YYYY>` · `--after <YYYY-MM-DD>` · `--before <YYYY-MM-DD>` | dates |
| `--author "<name>"` | author — *not* on `semantic`, `dblp`, `biorxiv`, `medrxiv`, `osf`, `eric`, `osti`, `ntrs`, `datacite`; put the name in the query there |
| `--field <code>` | subject/category — takes a *source-specific code*, see below |
| `--open-access` | OA only |
| `--patents` | **patents only** (europepmc) |
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
fastpaper search europepmc 'CRISPR AND CITED:[500 TO *]' --sort citations -n 20 --format json

# patents only
fastpaper search europepmc "gene editing" --patents -n 10 --format json

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
| Mathematics (published record) | `zbmath` (MSC classes + reviews, 1868 onwards) | `crossref` `openalex` |
| High-energy physics | `inspire` (surest citation counts; `--sort citations` works) | `arxiv` `openalex` |
| Biomedicine · clinical | `pubmed` `pmc` `europepmc` `biorxiv` `medrxiv` | `semantic` `openalex` |
| Chemistry · materials · engineering | — | `openalex` `semantic` `crossref` `core` |
| Earth · environment · agriculture | `europepmc 'SRC:AGR'` (Agricola) | `openalex` `core` `doaj` |
| Humanities · social science | — | `openalex` `core` `doaj` `hal` (strong on French/European work) |
| Psychology · sociology · education · law (preprints) | `osf --field psyarxiv` (or `socarxiv` / `edarxiv` / `lawarxiv`) | `openalex` `core` |
| Education research | `eric` | `openalex` `core` `doaj` |
| Technical reports · grey literature | `osti` (US DOE), `ntrs` (NASA aerospace) | `core` |
| Datasets · software · theses | `datacite` `zenodo` | `openaire` |
| Chinese literature | — | `europepmc 'SRC:CBA'` (Chinese biomedical only) |
| Patents | `europepmc --patents` | — |

`PDF` = `fastpaper download` works against this source. A `—` does not mean no
full text is reachable: `openalex` and `doaj` return a `pdf_url` on some hits
that you can fetch yourself. It means the CLI will not
fetch it for you.

`Cites` = does `citations` come back populated. `✓` nearly every hit · `~` only
under some conditions · `0` field present but 0 on most records · `—` never.

Even on a `✓` PDF source an individual record may hold no file — `pdf_url` is
`null` and the download fails with "No PDF URL found", **exit 4**. That is
normal rather than a fault.

**A `403` is a different thing and worth reading, not retrying.** The message
names the host and prints the full URL:

```
Error: 403 from www.mdpi.com
https://www.mdpi.com/1424-8220/21/16/5542/pdf?version=1629270899
The server refused this request. fastpaper cannot tell a paywall from a bot
block here -- they look the same from outside. ...
```

Do **not** loop over other sources after one of these. Measured: that MDPI
paper is fully open access and still 403s, an AHA subscription paper 403s
identically, and Semantic Scholar reports `open_access: true` for both — so
the status tells you nothing about whether a free copy exists. And the
alternatives are not independent: unpaywall resolves that DOI to the *same*
publisher URL byte for byte, and `download europepmc <DOI>` lands on it too.
Report the URL to the user instead; exit is `1`, not `4`.

| Source | PDF | Figures | Cites | Use it for | Watch out |
|---|:--:|:-----:|:--:|---|---|
| `arxiv` | ✓ | ✓ | — | **CS, AI, math, physics, stats, q-bio, q-fin, econ** preprints, every one of them free to read | query syntax is rewritten — see below. `pdf_url` on every hit, but a DOI on only about half |
| `pubmed` | — | — | — | **biomedicine**, 35M+ records — the reference index for clinical and life-science work | abstracts only, no PDFs and no journal name; move to `pmc` or `europepmc` for either |
| `pmc` | ✓ | — | — | **biomedical full text** (NLM) — the OA subset of what pubmed indexes | PDFs come from the OA subset only, so a pubmed hit may have no pmc record |
| `europepmc` | ✓ | ✓ | ~ | **widest biomedical** — 45M+ abstracts, 9M+ full text, plus EPO patents, NICE guidelines, Agricola, preprints and Chinese Biological Abstracts | richest query syntax here, and the only source that can threshold on citations (`CITED:[N TO *]`). Relevance-ranked hits are mostly uncited, so sort or threshold explicitly when you want impact |
| `biorxiv` | ✓ | — | — | **life-science preprints** (CSHL), full text on all of them | **no keyword search API** — browses a date window and matches locally, so `--after`/`--before` decide what is even searched |
| `medrxiv` | — | — | — | **medical / health preprints** (CSHL) | same date-window search as biorxiv, and its PDFs are blocked (403) — take the DOI elsewhere |
| `semantic` | ✓ | — | ✓ | **cross-discipline**, and the surest citation counts here — the basis for any ranking by impact | throttles hard without `SEMANTIC_SCHOLAR_API_KEY`. Carries a DOI on most hits and a PDF on about half |
| `openalex` | — | — | ✓ | **cross-discipline**, 200M+ works | `--field` takes a concept ID (`C154945302`), not a name. Rarely carries a PDF link — good for finding and ranking, not for fetching. Relevance drifts on title lookups |
| `crossref` | — | — | ✓ | **cross-discipline** DOI registry — the best title→DOI lookup here | registered metadata only: no PDFs, no OA status, and an abstract on few hits. Use it to resolve, then go elsewhere for content |
| `dblp` | — | — | — | **computer science** bibliography — conference and journal records, curated and clean | no abstracts at all; metadata only. The API takes a query and paging, nothing else |
| `core` | ✓ | — | 0 | **cross-discipline** OA aggregate, 400M+ from repositories and journals — a PDF on nearly every hit | a DOI on few hits and no journal name. `CORE_API_KEY` lifts the rate limit |
| `openaire` | — | — | ✓ | **cross-discipline** EU open science graph | `download` does not work here, but a `pdf_url` comes back on the minority of hits where a publisher file link is on record — most of its links are DOI resolvers and are not offered as PDFs. `get` wants an OpenAIRE id, not a DOI |
| `doaj` | — | — | — | **cross-discipline** peer-reviewed OA journals — complete metadata, with a journal name and abstract on nearly every hit | **its `pdf_url` is a landing page, not a file** — fetching one returns HTML. Year granularity only, no sorting |
| `hal` | ✓ | — | — | **cross-discipline** French national archive, with an abstract on nearly every hit and full text on most | `--field` takes a domain code: `math` `phys` `chim` `sdv` `shs` `spi` `info` `sde`. No journal name, and some records are metadata-only — filter on `pdf_url` before downloading |
| `osf` | ✓ | — | — | **social science, psychology, education and law preprints** — one API over ~32 communities (PsyArXiv, SocArXiv, EdArXiv, EcoEvoRxiv, engrXiv, MetaArXiv, Thesis Commons…), disciplines nothing else here reaches | **searches titles only**; the API has no full-text search (`filter[q]` returns 400). Every hit carries a DOI and a downloadable PDF. `--field` takes a provider id; `--offset` must be a multiple of `-n`. **The API itself is slow — 12–16s for one search** in testing; it is not hung |
| `inspire` | — | — | ✓ | **high-energy physics** — the surest citation counts here, and `--sort citations` really works | overlaps arXiv almost entirely; the value is citations and author disambiguation. `pdf_url` points at arXiv, where the file is, so `download` is off — fetch it by arXiv id |
| `zbmath` | — | — | — | **published mathematics** — 1868 onwards with MSC classes and reviews; arXiv has only preprints, so this complements it | **no abstracts** (it publishes reviews, not author abstracts), no full text, no citation counts. `--author` uses an `au:` prefix; `--offset` must be a multiple of `-n` |
| `eric` | ~ | ~ | — | **education research** — the US Department of Education index; this discipline is otherwise a blank here | only the ERIC-hosted subset (`e_fulltextauth=1`) has a `pdf_url` and `open_access`; both are null on the rest. **Never carries a DOI.** `--offset` is a true record offset |
| `osti` | ~ | ~ | — | **US Department of Energy technical reports** — grey literature nothing else here covers | `open_access` follows whether the record has a fulltext link. Date filters are converted to `MM/DD/YYYY` internally. `--offset` must be a multiple of `-n` |
| `ntrs` | ~ | ~ | — | **NASA aerospace reports** | the API **ignores every page-size parameter and always returns 10 per page**, so a larger `-n` costs one request per 10. **Never carries a DOI** (report numbers are not DOIs). `--offset` is a true record offset |
| `datacite` | — | — | — | **the DOI registry for datasets, software and theses** — the half crossref does not cover, including many university theses and repository deposits | registry metadata only: no full text, no access status. `--year` rides in the query because its own `publication-year` parameter **is silently ignored**. `--offset` must be a multiple of `-n` |
| `zenodo` | ✓ | — | — | **cross-discipline** CERN general-purpose repository — papers, datasets and software, with a DOI on nearly every hit | `-n` capped at 25. No journal name, and some records are metadata-only |
| `unpaywall` | — | — | — | **DOI → a downloadable URL.** No search, no discipline — a resolver | needs `UNPAYWALL_EMAIL`; one DOI per call, and the URL must be fetched separately |

**One field that lies.** `core` returns `citations` but leaves it 0 on most
records. The source does not accept `--sort` in the first place (it raises an
explicit error), so what this catches is **ranking the returned JSON yourself**:
it looks sorted while silently burying the well-cited papers. And `arxiv`
carries a DOI on only about half its records, so a DOI is not a stable key
across sources.

### Content types worth routing on

Most content-type distinctions are not actionable. These are:

| Need | How |
|---|---|
| **Patents only** | `--patents` on `europepmc` |
| **Chinese literature** | no general Chinese source in the CLI; Chinese biomedical via `europepmc 'SRC:CBA'` |
| **Preprints** | `arxiv`, `biorxiv`, `medrxiv`, or `europepmc 'SRC:PPR'` |
| **Clinical guidelines** | `europepmc 'SRC:CTX'` (NICE) |

`--patents` means patents only: with the flag you get patents, without it none,
never a mix. `europepmc` narrows natively and is the only source that takes the
flag — the rest either carry no patents at all, or their own switch can only
*widen* results to mix patents in, never narrow to them, so the CLI leaves it off.

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
- `europepmc`: `CITED:[N TO *]` · `AUTH:` · `PUB_YEAR:` · `OPEN_ACCESS:y` · `HAS_FT:y` · `LANG:` · `KW:` · `SRC:` subsets (`PPR` preprints, `CTX` NICE guidelines, `AGR` Agricola, `CBA` Chinese Biological Abstracts, `MED`, `PMC`)
- `doaj`: Lucene on `bibjson.*` · `zenodo`: Elasticsearch · `hal`: Solr · `dblp`: `year:` `author:` `venue:`

**`crossref`, `openalex`, `semantic` are free-text only** —
relevance matching, no field syntax. Filter them with the CLI flags.

## Rate limits

Each `fastpaper search` is one process against one source, so a source that
fails or is throttled takes down only itself.

| Tier | Sources | Limit |
|---|---|---|
| **A — generous** | `arxiv` `europepmc` `crossref` `openalex` `dblp` `doaj` `zenodo` `hal` `openaire` `osf` `inspire` `zbmath` `eric` `osti` `ntrs` `datacite` | no auth needed; arXiv asks for ~3s between requests |
| **A with a key** | `semantic` (`SEMANTIC_SCHOLAR_API_KEY`: 1 req/s → 100), `core` (`CORE_API_KEY`) | without a key both have to be treated as serial-only — anonymous `semantic` has been observed failing outright with "rate limited after 5 retries" |
| **B — shared budget** | `pubmed` + `pmc` | one NCBI E-utilities budget between them: 3 req/s, 10 with `NCBI_API_KEY` |

## What this tool cannot do

1. **No impact factors.** No source provides JIF or quartiles.
2. **Citation edges come only from `semantic` and `openalex`**, via `cite`.
   Anything else you assemble (shared authors, venue, concepts) is a proxy — say
   so when you report it.
3. **It cannot fetch an arbitrary URL.** A `pdf_url` from `unpaywall` or
   `openalex` has to be downloaded with something else.
4. **Missing fields mean unknown**, not zero and not absent.
