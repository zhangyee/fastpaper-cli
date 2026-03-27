---
name: fastpaper
description: Academic paper search, download & read CLI
---

# fastpaper

Fast academic paper search, download & read from 17 sources.

## Search tools

- `fastpaper search arxiv <query>` — CS, physics, math, econ preprints
- `fastpaper search biorxiv <query>` — Biology preprints
- `fastpaper search medrxiv <query>` — Medical preprints
- `fastpaper search pubmed <query>` — Biomedical abstracts (no full text)
- `fastpaper search pmc <query>` — Biomedical full text
- `fastpaper search europepmc <query>` — Europe PMC, broad biomedical
- `fastpaper search semantic <query>` — Cross-discipline, AI-powered
- `fastpaper search crossref <query>` — DOI metadata, broad coverage
- `fastpaper search openalex <query>` — Open metadata, 200M+ works
- `fastpaper search dblp <query>` — CS-focused index
- `fastpaper search core <query>` — Global open access aggregator
- `fastpaper search openaire <query>` — EU open science
- `fastpaper search doaj <query>` — Open access journals
- `fastpaper search zenodo <query>` — CERN open repository
- `fastpaper search hal <query>` — French national archive
- `fastpaper search scholar <query>` — Google Scholar (experimental, rate-limited)
- `fastpaper search unpaywall <DOI>` — OA link resolver (needs UNPAYWALL_EMAIL)

## Download tools (only for sources with download support)

- `fastpaper download arxiv <id>`
- `fastpaper download pmc <id>`
- `fastpaper download semantic <doi>`
- `fastpaper download biorxiv <id>`
- `fastpaper download medrxiv <id>`
- `fastpaper download europepmc <id>`
- `fastpaper download core <id>`
- `fastpaper download doaj <id>`
- `fastpaper download zenodo <id>`
- `fastpaper download hal <id>`

## Read tools (same sources as download, plus local)

- `fastpaper read <source> <id>` — Read paper content
- `fastpaper read <source> <id> --metadata-only` — Metadata only
- `fastpaper read local ./file.pdf` — Read local PDF
- `fastpaper read <source> <id> --section <SEC>` — Extract specific section
  Possible values: abstract, introduction, methods, results, discussion, conclusion, references, full (default)

## Lookup tool (auto-detect source)

- `fastpaper get <DOI|arXiv_ID|PMID|URL>` — Auto-routes by ID format

## Parallel usage

Run separate commands concurrently for multi-source search:

```
fastpaper search arxiv "query" --format json &
fastpaper search semantic "query" --format json &
wait
```

Each process is independent; failures don't affect other sources.

## All output uses --format json for structured agent consumption.
## Exit code 0 = success, non-zero = error (see --help for codes).

## Installation paths

- Claude Code: `~/.claude/skills/fastpaper/SKILL.md`
- Codex: `.codex/skills/fastpaper/SKILL.md`
- Cursor: `.cursor/skills/fastpaper/SKILL.md`
- Gemini: `.gemini/skills/fastpaper/SKILL.md`
- Qwen: `.qwen/skills/fastpaper/SKILL.md`
