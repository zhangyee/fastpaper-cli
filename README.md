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

# Search with filters -- each source declares which ones it can honour
fastpaper search arxiv "large language model" --after 2024-01-01 --field cs.CL --limit 20

# Fetch a paper by DOI (source auto-detected)
fastpaper get 10.1038/nature12373

# Fetch by arXiv ID
fastpaper get 2301.08745

# Name the source explicitly to control which one answers
fastpaper get openalex 10.1038/nature12373

# Download a PDF into ./papers
fastpaper download arxiv 2301.08745

# ...or let the identifier pick the source
fastpaper download 2301.08745

# Read a PDF you already downloaded
fastpaper read papers/2301.08745.pdf

# Read a specific section of it
fastpaper read papers/2301.08745.pdf --section methods

# JSON output for scripting / AI agents
fastpaper search semantic "CRISPR gene editing" --format json

# Parallel multi-source search
fastpaper search arxiv "protein folding" --format json &
fastpaper search pubmed "protein folding" --format json &
fastpaper search semantic "protein folding" --format json &
wait
```

## Sources

18 academic sources, each accessed independently per command.

`read` is not listed here: it works on a PDF already on disk, so it applies
equally to anything the `download` column can fetch.

| Source | Full name | search | get | download | cite | Domain |
|--------|-----------|:------:|:---:|:--------:|:----:|--------|
| `arxiv` | arXiv | yes | yes | yes | | Physics, math, CS, statistics, EE, q-bio, q-fin, econ |
| `biorxiv` | bioRxiv | yes | yes | yes | | Life sciences |
| `medrxiv` | medRxiv | yes | yes | | | Medical / health sciences (medRxiv blocks PDF fetches) |
| `pubmed` | PubMed | yes | yes | | | Biomedical & life sciences (metadata only) |
| `pmc` | PubMed Central | yes | yes | yes | | Biomedical & life sciences (full text) |
| `europepmc` | Europe PMC | yes | yes | yes | | Life sciences superset of PMC; adds preprints, patents, guidelines |
| `scholar` | Google Scholar | yes | | | | All disciplines (experimental, rate-limited) |
| `xueshu` | Baidu Xueshu (百度学术) | yes | | | | All disciplines, strong Chinese-language coverage (experimental, unofficial API) |
| `semantic` | Semantic Scholar | yes | yes | yes | yes | All disciplines, AI-powered citation graph |
| `crossref` | CrossRef | yes | yes | | | DOI metadata, all disciplines |
| `openalex` | OpenAlex | yes | yes | | yes | Open metadata index, 200M+ works |
| `dblp` | DBLP | yes | | | | Computer science |
| `core` | CORE | yes | yes | yes | | Open access aggregator (needs `CORE_API_KEY`) |
| `openaire` | OpenAIRE | yes | yes | | | EU open science |
| `doaj` | DOAJ | yes | yes | | | Open access journals, all subjects (links go to publisher pages, not PDFs) |
| `unpaywall` | Unpaywall | | yes | | | OA link resolver (requires `UNPAYWALL_EMAIL`) |
| `zenodo` | Zenodo | yes | yes | yes | | All disciplines (datasets, software, papers) |
| `hal` | HAL | yes | yes | yes | | Multi-disciplinary, French national archive |

Sources differ in which search filters they can honour, and in their per-request
result caps. `fastpaper sources --capabilities` prints the full matrix along
with each source's caveats.

## Commands

Each command does exactly one thing: `search` finds papers, `get` reads
metadata, `download` saves a PDF, `read` extracts text from a PDF on disk.

### `search` -- Search papers

The source is required: a free-text query has no identifier shape to infer one
from.

```
fastpaper search <SOURCE> <QUERY> [OPTIONS]

Options:
  -n, --limit <N>        Max results [default: 10]
      --offset <N>       Skip first N results [default: 0]
      --sort <FIELD>     Sort by: relevance, date, citations
      --order <DIR>      asc or desc [default: desc]
      --author <NAME>    Filter by author
      --after <DATE>     Papers published on or after YYYY-MM-DD
      --before <DATE>    Papers published on or before YYYY-MM-DD
      --year <YEAR>      Papers in a specific year
      --field <FIELD>    Field of study / category (e.g. cs.CL)
      --open-access      Only open access papers
      --patents          Patents only (europepmc, xueshu)
  -f, --format <FMT>     table, json, jsonl, csv, bibtex [default: table]
  -o, --output <PATH>    Write results to file
```

Filters map onto each source's own API parameters. A source that cannot honour
one **fails with an error naming the filters it does support, and the sources
that do have the one you asked for**, rather than dropping it silently — a
filter that vanishes yields results that look right and are not. The same goes
for `-n` above a source's per-request cap. Run
`fastpaper sources --capabilities` to see what each source accepts.

### `get` -- Fetch metadata by identifier

One argument is an identifier and the source is inferred from its shape (DOI,
arXiv ID, PMID, PMC ID, S2 ID). Two arguments name the source explicitly.

```
fastpaper get <IDENTIFIER>
fastpaper get <SOURCE> <IDENTIFIER>
```

### `download` -- Download PDF

Same two forms as `get`. Saves as `<identifier>.pdf`.

```
fastpaper download <IDENTIFIER> [OPTIONS]
fastpaper download <SOURCE> <IDENTIFIER> [OPTIONS]

Options:
  -d, --dir <PATH>       Download directory [default: ./papers]
  -s, --max-size <SIZE>  Largest response body to accept, e.g. 10MiB, 200MB.
                         0 means unlimited [default: 100MiB]
      --overwrite        Overwrite an existing file
```

### `cite` -- Walk citation edges

Same two forms as `get`. Returns the papers on the other end of a citation
edge, in the usual result shape. Only `semantic` and `openalex` hold edges; a
bare DOI routes to `openalex`, which needs no API key, while arXiv and `S2:`
identifiers route to `semantic`.

```
fastpaper cite <IDENTIFIER> [OPTIONS]
fastpaper cite <SOURCE> <IDENTIFIER> [OPTIONS]

Options:
      --direction <DIR>  incoming (papers citing this one) or outgoing
                         (papers it cites) [default: incoming]
  -n, --limit <N>        Max edges [default: 20]
  -o, --output <PATH>    Write to a file instead of stdout
```

### `read` -- Read a local PDF

Takes a path, never the network. Download first, then read what landed.

```
fastpaper read <PATH> [OPTIONS]

Options:
      --section <SEC>    abstract, introduction, methods, results,
                         discussion, conclusion, references, full [default: full]
      --grep <PATTERN>   Search the text for a regular expression.
                         Case-insensitive; use (?-i) for a case-sensitive one
      --context <N>      Characters of context on each side of a match
                         [default: 500, requires --grep]
      --max-matches <N>  Show at most N matches [default: 10, requires --grep]
      --max-length <N>   Truncate output to N characters
  -o, --output <PATH>    Write to file
```

```sh
fastpaper download 2301.08745            # -> ./papers/2301.08745.pdf
fastpaper read papers/2301.08745.pdf --section abstract
fastpaper read papers/2301.08745.pdf --grep 'limitations?'
```

`--grep` searches the section `--section` selected, or the whole paper when it
is not given, so "try the section, fall back to full text" is one command with
one flag changed. Matches are reported with their character offset, and windows
that overlap are merged rather than printed twice. No match exits 4.

### `sources` -- List sources and capabilities

```
fastpaper sources [--capabilities]
```

### `completions` -- Shell completions

```
fastpaper completions fish > ~/.config/fish/completions/fastpaper.fish
fastpaper completions zsh > ~/.zfunc/_fastpaper
fastpaper completions bash >> ~/.bashrc
```

## Environment variables

All optional except where noted. 17 of 18 sources work with zero configuration.

| Variable | Purpose |
|----------|---------|
| `FASTPAPER_DOWNLOAD_DIR` | Default download directory (otherwise `./papers`) |
| `FASTPAPER_MAX_DOWNLOAD_SIZE` | Largest response body `download` will accept (otherwise `100MiB`); `0` means unlimited |
| `FASTPAPER_EMAIL` | Contact address sent to CrossRef, OpenAlex and NCBI. Unset means the parameter is omitted, which all three accept |
| `SEMANTIC_SCHOLAR_API_KEY` | Higher rate limit for Semantic Scholar |
| `OPENALEX_API_KEY` | Larger free tier for OpenAlex, which has metered usage since 2026-02 |
| `CORE_API_KEY` | Higher rate limit for CORE |
| `NCBI_API_KEY` | Higher rate limit for PubMed / PMC |
| `UNPAYWALL_EMAIL` | **Required** for Unpaywall, and it must be a real address |

Every source also takes `FASTPAPER_<SOURCE>_URL` to override its base URL —
`FASTPAPER_ARXIV_URL`, `FASTPAPER_PUBMED_URL` and so on — which is what the
tests point at a local mock server. Sources whose files live on a different host
than their API have a second override for that host: `FASTPAPER_ARXIV_PDF_URL`,
`FASTPAPER_BIORXIV_DL_URL`, `FASTPAPER_PMC_DL_URL` (which points at the PMC
Cloud Service on AWS Open Data, not the article pages).

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error (invalid arguments, parse failure) |
| `2` | Network error (timeout, DNS failure) |
| `3` | Source error (API error, rate limit exhausted) |
| `4` | Nothing matched: no such paper, no `--grep` match, no such `--section` |
| `5` | Permission error (not open access, missing env var) |

Commands that take `-o` print a receipt to stderr once the file is written —
`Saved: results.json (12 results)` — so a caller never has to read the file back
to find out what landed. `-q` suppresses it. Zero results still print, since
that is the case most worth seeing.

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) and the developer docs under [docs/](docs/).

## Acknowledgements

This project was inspired by [paper-search-mcp](https://github.com/openags/paper-search-mcp), an MCP server for searching and downloading academic papers from multiple sources. Many thanks to its authors for showing what a multi-source paper tool can look like.

## License

[GPL-3.0](LICENSE)
