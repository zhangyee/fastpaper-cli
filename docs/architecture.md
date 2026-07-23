# Architecture

> 中文版:[architecture.zh-CN.md](architecture.zh-CN.md)

Describes the code as it is. If code and this page disagree, the code wins — fix this page.

## Module map

```
src/
├── main.rs          # Command dispatch: reads env-var base URLs, calls a source, renders output
├── cli.rs           # All clap definitions: Cli, GlobalOpts, Commands, per-command Args,
│                    #   and the Source / OutputFormat / SortField / Section enums
├── sources/
│   ├── mod.rs       # The Paper struct (shared data contract) + encode_query helper
│   └── <source>.rs  # One fully self-contained module per source (18 today)
├── download.rs      # fetch_pdf, per-source download_<src> functions, save_pdf
├── read.rs          # PDF text extraction (pdf_oxide): extract_text, extract_text_from_bytes,
│                    #   extract_section
├── output.rs        # Renderers: to_table, to_json, to_csv, to_bibtex
└── identifier.rs    # detect_id_type for `get`: Arxiv, ArxivOld, Doi, Pmc, Pmid, S2, Url, Unknown
```

## Data flow

The main path, `fastpaper search <source> <query>`:

1. clap parses the command line (`cli.rs`).
2. `main.rs` matches on the source, resolves the base URL — default constant, overridable via `FASTPAPER_<SOURCE>_URL` — and calls the source module.
3. Every source implements the same entry point:

   ```rust
   pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String>
   ```

4. The resulting `Vec<Paper>` is rendered by `output::to_table` / `to_json` / `to_csv` / `to_bibtex` according to `--format`.

Other commands:

- **`download`** — resolves a PDF URL for the identifier and saves it via `download.rs` (only sources that can provide PDFs support this).
- **`read`** — obtains PDF bytes (local file or remote fetch), extracts text with `read.rs`, optionally slices a section.
- **`get`** — `identifier::detect_id_type` infers the source from the identifier shape (DOI, arXiv ID, PMC ID, PMID, S2 ID), then calls that source's lookup function.

## The Paper contract

`sources/mod.rs` defines the single shared data structure. Every source maps its API response into this and nothing else:

| Field | Type | Notes |
|---|---|---|
| `id` | `String` | Source-native identifier |
| `title` | `String` | Plain text (sources strip HTML highlight tags) |
| `authors` | `Vec<String>` | |
| `abstract_text` | `Option<String>` | Serialized as `"abstract"` (serde rename) |
| `year` | `Option<u16>` | |
| `doi` | `Option<String>` | Empty strings become `None` |
| `url` | `Option<String>` | Landing page |
| `pdf_url` | `Option<String>` | Direct PDF when known |
| `venue` | `Option<String>` | Journal / publisher |
| `citations` | `Option<u32>` | |
| `fields` | `Vec<String>` | Subject areas (e.g. `cs.CL`) |
| `open_access` | `Option<bool>` | |
| `source` | `String` | Source name, e.g. `"arxiv"` |

In JSON output missing values are `null`, never omitted — the schema is stable so agents and scripts can rely on it.

## Source module contract

A source module is one file under `src/sources/`, fully self-contained:

- Entry point: the `search` signature above. Some sources additionally expose lookups such as `get_by_id` / `get_by_doi` / `get_by_pmid`.
- `base_url` is always injected by the caller — this is what makes every source testable against a local mockito server.
- Errors are `Result<_, String>` with human-readable messages; `main.rs` prints them and exits non-zero.
- Unit tests live in the same file under `#[cfg(test)]`, driven by a recorded fixture in `tests/fixtures/`.

Duplication between source modules is accepted on purpose — see the philosophy below.

## Design philosophy

| Principle | Why |
|---|---|
| Zero configuration | Every source works out of the box (Unpaywall's email is the one exception); no config file. |
| One source per command | Parallelism belongs to the agent/shell spawning multiple processes, not to this binary. |
| Source as positional arg | `fastpaper search arxiv <q>` is fast to type and trivial for agents to construct. |
| Pipeable output | Human table by default, `--format json` for machines. |
| Synchronous, blocking I/O | `ureq` instead of an async runtime: small binary, fast compile, simple code. |
| Fully independent sources | Each source is one file; duplication is accepted so any source can be modified or deleted in isolation. |
| Stateless, no cache | Every invocation is a fresh API request — simple, reliable, no side effects. |
| Automatic backoff on 429 | Sources retry with exponential backoff where the API rate-limits, transparent to the user. |
| Identifier auto-detection | `get` infers the source from the identifier format. |

## Non-goals

Consequences of the philosophy, kept deliberately out of scope: no async runtime, no local cache or state directory, no configuration file, no cross-source federation inside the binary.
