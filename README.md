**English** | [中文](README.zh-CN.md)

# fastpaper CLI with Skill

A CLI tool that gives AI agents (Claude Code, Codex, Opencode, etc.) the ability to search, download, and read academic papers and scientific literature. Ships with a [SKILL](skills/fastpaper/SKILL.md) that teaches agents how to pick sources and construct commands.

One command, one source, zero configuration. Parallel multi-source search is handled by the agent spawning multiple processes.

## Install

### CLI

**Homebrew (macOS / Linux)**

```sh
brew install zhangyee/tap/fastpaper
```

**Shell script (macOS / Linux)**

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/zhangyee/fastpaper-cli/releases/latest/download/fastpaper-cli-installer.sh | sh
```

**PowerShell (Windows)**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/zhangyee/fastpaper-cli/releases/latest/download/fastpaper-cli-installer.ps1 | iex"
```

**Cargo**

```sh
cargo install fastpaper-cli
```

### Skill

Install the skill so your AI agent knows how to use fastpaper. Uses [Vercel Skills](https://github.com/vercel-labs/skills), a tool that installs SKILL.md files into agents:

```sh
npx skills add zhangyee/fastpaper-cli --skill fastpaper
```

The SKILL.md teaches the agent how to pick sources by domain and construct commands. Use `--format json` for structured output. All JSON fields use `null` for missing values (never omitted), so the schema is stable.

## Quick start

```sh
# Search arXiv
fastpaper search arxiv "transformer attention mechanism"

# Search with filters
fastpaper search arxiv "large language model" --after 2024-01-01 --field cs.CL --limit 20

# Fetch a paper by DOI (auto-detects source)
fastpaper get 10.1038/nature12373

# Fetch by arXiv ID
fastpaper get 2301.08745

# Download PDF
fastpaper download arxiv 2301.08745

# Read full text
fastpaper read arxiv 2301.08745

# Read a specific section
fastpaper read pmc PMC7318926 --section methods

# Read a local PDF
fastpaper read local ./paper.pdf

# JSON output for scripting / AI agents
fastpaper search semantic "CRISPR gene editing" --format json

# Parallel multi-source search
fastpaper search arxiv "protein folding" --format json &
fastpaper search pubmed "protein folding" --format json &
fastpaper search semantic "protein folding" --format json &
wait
```

## Sources

17 academic sources, each accessed independently per command.

| Source | Full name | search | download | read | Domain |
|--------|-----------|:------:|:--------:|:----:|--------|
| `arxiv` | arXiv | yes | yes | yes | Physics, math, CS, statistics, EE, q-bio, q-fin, econ |
| `biorxiv` | bioRxiv | yes | yes | yes | Life sciences |
| `medrxiv` | medRxiv | yes | yes | yes | Medical / health sciences |
| `pubmed` | PubMed | yes | | | Biomedical & life sciences (metadata only) |
| `pmc` | PubMed Central | yes | yes | yes | Biomedical & life sciences (full text) |
| `europepmc` | Europe PMC | yes | | | Life sciences superset of PMC |
| `scholar` | Google Scholar | yes | | | All disciplines (experimental, rate-limited) |
| `semantic` | Semantic Scholar | yes | yes | yes | All disciplines, AI-powered citation graph |
| `crossref` | CrossRef | yes | | | DOI metadata, all disciplines |
| `openalex` | OpenAlex | yes | | | Open metadata index, 200M+ works |
| `dblp` | DBLP | yes | | | Computer science |
| `core` | CORE | yes | yes | yes | Open access aggregator |
| `openaire` | OpenAIRE | yes | | | EU open science |
| `doaj` | DOAJ | yes | yes | yes | Open access journals, all subjects |
| `unpaywall` | Unpaywall | yes | | | OA link resolver (requires `UNPAYWALL_EMAIL`) |
| `zenodo` | Zenodo | yes | yes | yes | All disciplines (datasets, software, papers) |
| `hal` | HAL | yes | yes | yes | Multi-disciplinary, French national archive |

## Commands

### `search` -- Search papers

```
fastpaper search <SOURCE> <QUERY> [OPTIONS]

Options:
  -n, --limit <N>        Max results [default: 10]
      --offset <N>       Skip first N results [default: 0]
      --sort <FIELD>     Sort by: relevance, date, citations [default: relevance]
      --author <NAME>    Filter by author
      --after <DATE>     Papers after YYYY-MM-DD
      --before <DATE>    Papers before YYYY-MM-DD
      --year <YEAR>      Papers in specific year
      --field <FIELD>    Field of study / category (e.g. cs.AI)
      --open-access      Only open access papers
  -f, --format <FMT>     table, json, jsonl, csv, bibtex [default: table]
  -o, --output <PATH>    Write results to file
```

### `get` -- Fetch paper by identifier

Auto-detects source from identifier format (DOI, arXiv ID, PMID, PMC ID, URL).

```
fastpaper get <IDENTIFIER> [OPTIONS]

Options:
      --resolve           Find all available OA versions
      --with-citations    Include citation count and references
      --with-abstract     Include abstract
```

### `download` -- Download PDF

```
fastpaper download <SOURCE> <IDENTIFIER> [OPTIONS]

Options:
  -d, --dir <PATH>       Download directory [default: ./papers]
      --filename <FMT>   Template: {id}, {title}, {authors}, {year}, {doi}
      --overwrite        Overwrite existing files
      --source-files     Download LaTeX source (arXiv only)
```

### `read` -- Read paper content

```
fastpaper read <SOURCE> <IDENTIFIER> [OPTIONS]

Options:
      --section <SEC>    abstract, introduction, methods, results,
                         discussion, conclusion, references, full [default: full]
      --metadata-only    Only show metadata
      --raw              Raw text without formatting
      --max-length <N>   Truncate output to N characters
  -o, --output <PATH>    Write to file
```

### `sources` -- List sources and capabilities

```
fastpaper sources [--check] [--capabilities]
```

### `completions` -- Shell completions

```
fastpaper completions fish > ~/.config/fish/completions/fastpaper.fish
fastpaper completions zsh > ~/.zfunc/_fastpaper
fastpaper completions bash >> ~/.bashrc
```

## Environment variables

All optional except where noted. 19 of 20 sources work with zero configuration.

| Variable | Purpose |
|----------|---------|
| `FASTPAPER_DOWNLOAD_DIR` | Default download directory (otherwise `./papers`) |
| `FASTPAPER_EMAIL` | CrossRef / OpenAlex polite pool email |
| `SEMANTIC_SCHOLAR_API_KEY` | Higher rate limit for Semantic Scholar |
| `CORE_API_KEY` | Higher rate limit for CORE |
| `NCBI_API_KEY` | Higher rate limit for PubMed / PMC |
| `UNPAYWALL_EMAIL` | **Required** for Unpaywall |

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error (invalid arguments, parse failure) |
| `2` | Network error (timeout, DNS failure) |
| `3` | Source error (API error, rate limit exhausted) |
| `4` | No results found |
| `5` | Permission error (not open access, missing env var) |

## License

[GPL-3.0](LICENSE)
