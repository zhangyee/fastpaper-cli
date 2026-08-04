# Architecture

> 中文版:[architecture.zh-CN.md](architecture.zh-CN.md)

Describes the code as it is. If code and this page disagree, the code wins — fix this page.

## Module map

```
src/
├── main.rs          # Parses the command line and dispatches; maps errors to exit codes
├── cli.rs           # All clap definitions: Cli, GlobalOpts, Commands, per-command Args,
│                    #   OutputFormat / Section, and the [source] <id> disambiguation
├── commands/        # One module per command; each owns its validation, calls and rendering
│   ├── mod.rs       # CommandError (carries the exit code), render, emit
│   └── search.rs · get.rs · download.rs · read.rs · sources.rs
├── registry.rs      # The Source enum and the single table mapping each source to its
│                    #   base URLs, Capabilities and functions (plain fn pointers, no trait)
├── sources/
│   ├── mod.rs       # Paper (shared data contract), SearchQuery, Capabilities / SearchCaps,
│   │                #   encode_query, validate_ymd, contact_email
│   └── <source>.rs  # One fully self-contained module per source (18 today)
├── download.rs      # fetch_pdf, per-source pdf_bytes_<src> resolvers, save_pdf
├── read.rs          # PDF text extraction (pdf_oxide): extract_text, extract_text_from_bytes,
│                    #   extract_section
├── output.rs        # Renderers: to_table, to_json, to_jsonl, to_csv, to_bibtex
└── identifier.rs    # detect_id_type: Arxiv, ArxivOld, Doi, Pmc, Pmid, S2, Url, Unknown
```

## Data flow

The main path, `fastpaper search <source> <query>`:

1. clap parses the command line (`cli.rs`).
2. `main.rs` matches on the source, resolves the base URL — default constant, overridable via `FASTPAPER_<SOURCE>_URL` — and calls the source module.
3. Every source implements the same entry point:

   ```rust
   pub fn search(base_url: &str, q: &SearchQuery) -> Result<Vec<Paper>, String>
   ```

4. The resulting `Vec<Paper>` is rendered by `output::to_table` / `to_json` / `to_jsonl` / `to_csv` / `to_bibtex` according to `--format`.

Before step 3, `commands/search.rs` checks the request against the source's
`Capabilities`: a blank query, a `-n` above the source's cap, or a filter the
source cannot honour is refused with an error naming what *is* supported here,
plus the sources that do honour the rejected filter — the usual mistake is the
source, not the flag. A silently dropped filter would produce results that look
right and are not.

Other commands:

- **`download`** — resolves the identifier to PDF bytes via the source's `pdf` function, then `download::save_pdf` writes them. One argument means the source is inferred from the identifier shape; two means it was named.
- **`read`** — reads a local PDF only. It never touches the network: `download` fetches, `read` extracts.
- **`get`** — same two argument forms as `download`. When the source is inferred, `identifier::detect_id_type` maps the shape (DOI, arXiv ID, PMC ID, PMID, S2 ID) to a source; note the routing differs from `download`'s, because Crossref owns DOI metadata but serves no files.

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
- URL construction lives in a pure `build_*_url` function so filter mapping can be asserted directly, without a mock. (Mockito prefers the most recently created matching mock, which makes `expect(0)` probes behind a catch-all pass vacuously.)
- Errors are `Result<_, String>` with human-readable messages; the command layer wraps them in `CommandError` and `main.rs` exits non-zero.
- Unit tests live in the same file under `#[cfg(test)]`, driven by a recorded fixture in `tests/fixtures/`.

Nothing in a source file references `registry.rs`; the wiring points the other
way. `registry.rs` holds `fn` pointers rather than a trait, so a source stays a
set of free functions and implements nothing.

Duplication between source modules is accepted on purpose — see the philosophy below.

## Design philosophy

| Principle | Why |
|---|---|
| Zero configuration | Every source works out of the box (Unpaywall's email is the one exception); no config file. API keys only ever raise limits. |
| One source per command | Parallelism belongs to the agent/shell spawning multiple processes, not to this binary. |
| Source as positional arg | `fastpaper search arxiv <q>` is fast to type and trivial for agents to construct. |
| Pipeable output | Human table by default, `--format json` for machines. |
| Synchronous, blocking I/O | `ureq` instead of an async runtime: small binary, fast compile, simple code. |
| Fully independent sources | Each source is one file; duplication is accepted so any source can be modified or deleted in isolation. |
| Stateless, no cache | Every invocation is a fresh API request — simple, reliable, no side effects. |
| Automatic backoff on 429 | Sources retry with exponential backoff where the API rate-limits, transparent to the user. |
| Identifier auto-detection | `get` and `download` infer the source from the identifier shape; naming one explicitly overrides that. |
| Capabilities declared once | `registry.rs` states what each source can do; argument validation, `sources --capabilities` and the error messages all read from it, so the listing cannot drift from the behaviour. |
| Loud refusal over silent fallback | An unsupported filter, an over-cap `-n` or a blank query is an error, not a quietly different request. |

## Non-goals

Consequences of the philosophy, kept deliberately out of scope: no async runtime, no local cache or state directory, no configuration file, no cross-source federation inside the binary.
