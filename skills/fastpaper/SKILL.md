---
name: fastpaper
description: Search, download, and read academic papers from 18 sources (arXiv, PubMed, Semantic Scholar, etc.). Use when the user asks about research papers, literature search, finding citations, or reading PDFs. Supports parallel multi-source search, section extraction, and auto-routing by DOI/arXiv ID/PMID/URL.
---

# fastpaper

Fast academic paper search, download & read from 18 sources.

`fastpaper` is a pre-installed standalone CLI. Before first use, verify it is available: `which fastpaper`. It is NOT a Python package — do NOT attempt to install via pip.

## Source selection by domain

Pick sources based on the user's research domain:

**CS / AI / Math / Physics**
- `arxiv` — Cornell preprint archive; physics, math, CS, statistics, EE, quantitative biology/finance, economics. search, get, download
- `dblp` — CS-focused bibliography index. search
**Biomedical / Life sciences**
- `pubmed` — NLM index, 35M+ citations, biomedical and life sciences abstracts. search, get
- `pmc` — NLM full-text archive of peer-reviewed biomedical and life sciences literature. search, get, download
- `europepmc` — Life sciences superset of PMC by EMBL-EBI; adds patents, preprints, clinical guidelines. search, get, download
- `biorxiv` — Life sciences preprints by CSHL. search, download
- `medrxiv` — Medical/health science preprints by CSHL. search
**Cross-discipline / Broad coverage**
- `semantic` — Allen AI, AI-powered semantic search + citation graph, all disciplines. search, get, download
- `crossref` — DOI registry, metadata queries across all disciplines. search, get
- `openalex` — Open index (successor to MS Academic Graph), 200M+ works. search, get
- `scholar` — Google Scholar, broadest coverage (experimental, rate-limited). search
- `xueshu` — Baidu Xueshu (百度学术), strong Chinese-language coverage (experimental, unofficial API; may require captcha and stop working). search
**Open access aggregators**
- `core` — Largest global OA aggregator, full text from institutional repos and journals. search, get, download
- `openaire` — EU open science infrastructure, aggregates worldwide OA research. search
- `doaj` — Directory of quality-reviewed OA journals, all subjects. search, get
- `unpaywall` — OA link resolver by DOI, finds legal free versions (needs UNPAYWALL_EMAIL). get
**Open repositories**
- `zenodo` — CERN/OpenAIRE general-purpose repository (datasets, software, papers), all disciplines. search, download
- `hal` — French national multi-disciplinary open archive by CNRS (some embargo periods). search, download
## When you have a paper ID or DOI

Auto-detect source and fetch directly:

```
fastpaper get <DOI|arXiv_ID|PMID|PMC_ID>          # source inferred from the id
fastpaper get <source> <DOI|arXiv_ID|PMID|PMC_ID>  # or name it explicitly
```

## When you need to search

```
fastpaper search <source> <query>
```

Filters (`--year`, `--after`/`--before`, `--author`, `--field`, `--open-access`,
`--sort`, `--offset`) are validated per source: one a source cannot honour is an
error naming what it does support, never a silent no-op. Run `fastpaper sources
--capabilities` to see the matrix before constructing a filtered query.

For broad topics, search multiple sources in parallel:

```
fastpaper search arxiv "transformer attention" --format json &
fastpaper search semantic "transformer attention" --format json &
wait
```

Each process is independent; failures don't affect other sources.

## When you need full text or specific sections

`read` works on a local PDF only. Download first, then read what landed:

```
fastpaper download <id>                               # -> ./papers/<id>.pdf
fastpaper read papers/<id>.pdf                        # full text
fastpaper read papers/<id>.pdf --section <SEC>        # one section
fastpaper read papers/<id>.pdf --max-length 4000      # truncate
```

Sections: abstract, introduction, methods, results, discussion, conclusion, references, full (default).

For metadata rather than full text, use `get` — it never downloads a file.

## When you need to download PDF

```
fastpaper download <id>                  # source inferred from the id
fastpaper download <source> <id>         # or name it explicitly
```

Saves to `./papers/<id>.pdf` by default (`-d` to change it).

Download-capable sources: arxiv, biorxiv, pmc, europepmc, semantic, core, zenodo, hal.
CORE needs CORE_API_KEY -- anonymous requests are throttled to 429.

A download that would have produced HTML rather than a file now fails instead of
writing it out, so a reported success is a real PDF.

## All output uses --format json for structured agent consumption.
## Exit code 0 = success, non-zero = error (see --help for codes).
