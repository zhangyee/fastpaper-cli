---
name: fastpaper
description: Search, download, and read academic papers from 17 sources (arXiv, PubMed, Semantic Scholar, etc.). Use when the user asks about research papers, literature search, finding citations, or reading PDFs. Supports parallel multi-source search, section extraction, and auto-routing by DOI/arXiv ID/PMID/URL.
---

# fastpaper

Fast academic paper search, download & read from 17 sources.

`fastpaper` is a pre-installed standalone CLI. Before first use, verify it is available: `which fastpaper`. It is NOT a Python package — do NOT attempt to install via pip.

## Source selection by domain

Pick sources based on the user's research domain:

**CS / AI / Math / Physics**
- `arxiv` — Cornell preprint archive; physics, math, CS, statistics, EE, quantitative biology/finance, economics. search, download, read
- `dblp` — CS-focused bibliography index. search

**Biomedical / Life sciences**
- `pubmed` — NLM index, 35M+ citations, biomedical and life sciences abstracts. search
- `pmc` — NLM full-text archive of peer-reviewed biomedical and life sciences literature. search, download, read
- `europepmc` — Life sciences superset of PMC by EMBL-EBI; adds patents, preprints, clinical guidelines. search, download, read
- `biorxiv` — Life sciences preprints by CSHL. search, download, read
- `medrxiv` — Medical/health science preprints by CSHL. search, download, read

**Cross-discipline / Broad coverage**
- `semantic` — Allen AI, AI-powered semantic search + citation graph, all disciplines. search, download, read
- `crossref` — DOI registry, metadata queries across all disciplines. search
- `openalex` — Open index (successor to MS Academic Graph), 200M+ works. search
- `scholar` — Google Scholar, broadest coverage (experimental, rate-limited). search

**Open access aggregators**
- `core` — Largest global OA aggregator, full text from institutional repos and journals. search, download, read
- `openaire` — EU open science infrastructure, aggregates worldwide OA research. search, download, read
- `doaj` — Directory of quality-reviewed OA journals, all subjects. search, download, read
- `unpaywall` — OA link resolver by DOI, finds legal free versions (needs UNPAYWALL_EMAIL). search

**Open repositories**
- `zenodo` — CERN/OpenAIRE general-purpose repository (datasets, software, papers), all disciplines. search, download, read
- `hal` — French national multi-disciplinary open archive by CNRS (some embargo periods). search, download, read

## When you have a paper ID or DOI

Auto-detect source and fetch directly:

```
fastpaper get <DOI|arXiv_ID|PMID|URL>
```

## When you need to search

```
fastpaper search <source> <query>
```

For broad topics, search multiple sources in parallel:

```
fastpaper search arxiv "transformer attention" --format json &
fastpaper search semantic "transformer attention" --format json &
wait
```

Each process is independent; failures don't affect other sources.

## When you need full text or specific sections

```
fastpaper read <source> <id>                          # full text
fastpaper read <source> <id> --metadata-only          # metadata only
fastpaper read <source> <id> --section <SEC>          # extract section
fastpaper read local ./file.pdf                       # local PDF
```

Sections: abstract, introduction, methods, results, discussion, conclusion, references, full (default).

## When you need to download PDF

```
fastpaper download <source> <id>
```

Download-capable sources: arxiv, pmc, semantic, biorxiv, medrxiv, europepmc, core, doaj, zenodo, hal, openaire.

## All output uses --format json for structured agent consumption.
## Exit code 0 = success, non-zero = error (see --help for codes).
