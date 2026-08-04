# Adding a New Source

> 中文版:[adding-a-source.zh-CN.md](adding-a-source.zh-CN.md)

Read the [Source module contract](architecture.md#source-module-contract) first. `xueshu` (Baidu Xueshu) is the worked example throughout.

## Step 0 — Research

Before writing code, produce a research note answering:

- **Endpoint(s)**: URL, HTTP method, required headers.
- **Parameters**: query encoding, pagination scheme, page size.
- **Response shape**: where the paper list lives, field names and types, how missing values are represented (`null`, empty string, absent key — often all three).
- **Auth & anti-bot behavior**: API keys, rate limits, risk-control quirks.
- **Pagination**: offset/cursor, how the last page is signaled.

Follow the format of the existing notes in [docs/sources/](sources/README.md) and add yours there.

*xueshu example:* the new web frontend calls an undocumented JSON endpoint `GET /search/api/search`; requests need an `Acs-Token` header for which the frontend has a static fallback value; business code `7350001` means "interactive captcha required"; the default curl User-Agent triggers that captcha while an honest tool UA does not.

## Step 1 — Capture a fixture

Make **one** real request and save the response as `tests/fixtures/<source>_search.json` (or `.xml`/`.html`). Sanitize it first:

- strip signed CDN URLs, session tokens, anything with an `authorization=` query;
- anonymize request/log IDs;
- keep the structure and realistic field variance (empty DOIs, missing years, HTML tags in titles) — that variance is what your tests need.

## Step 2 — Parser first

Create `src/sources/<source>.rs`. Write the parser against the fixture before any HTTP code:

```rust
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String>
```

Work test-first: each behavior gets a failing test, then the minimal implementation. Cover at least:

- happy path: expected number of papers, non-empty ids/titles, `source` set correctly;
- field mapping: authors, year, citations, venue fallbacks, empty-string → `None`;
- text cleanup: strip search-highlight tags (e.g. `<em>`) from titles *and* abstracts;
- tolerance: `null`, empty strings, and absent keys must not fail parsing;
- API-level errors: business error codes must return a clear `Err`, not an empty list.

*xueshu example:* 22 parser tests, including one that feeds an inline JSON body with `"code": 7350001` and asserts the error message mentions the captcha.

## Step 3 — HTTP layer

Add the standard entry point:

```rust
pub fn search(base_url: &str, q: &SearchQuery) -> Result<Vec<Paper>, String>
```

Rules:

- `base_url` comes from the caller — never hardcode it inside request code.
- Use `super::encode_query` for the query string.
- Send the project User-Agent (`fastpaper-cli/<version> (+repo URL)`); some risk engines block default library UAs.
- Paginate serially; stop when you have `max_results`, the page is empty, or an error occurs; truncate before returning.
- Map HTTP statuses to distinct, human-readable errors (403 blocked / 429 rate-limited / others).

Test with mockito (synchronous): request path and parameters, required headers, second-page pagination, early stop when `max_results` is satisfied, and each error status. *xueshu example:* 10 mockito tests.

## Step 4 — Wire the CLI

Four touch points:

1. `src/registry.rs` — add the variant to `enum Source` and to `ALL`.
2. `src/registry.rs` — add its `static <SOURCE>: SourceEntry`: name, env var and default base URL, the `search` / `get` / `pdf` function pointers, and `Capabilities` stating exactly which filters it can honour.
3. `src/registry.rs` — add the arm in `Source::entry()`.

There is no fourth step: `fastpaper sources` and the argument validation both
read `Capabilities`, so the listing and the error messages follow from step 2.
Declare a filter `true` only if the source really maps it onto a native
parameter — a `true` you cannot back up produces results that look right and
are not.

Add CLI integration tests in `tests/cli.rs` (mockito server + `FASTPAPER_<SOURCE>_URL` env): a successful search printing a known title, an API-error case exiting non-zero with the message, `sources` listing the new name.

## Step 5 — Docs sync checklist

- [ ] `README.md` — source count (two places: "N academic sources" and "N of M sources work with zero configuration") + table row
- [ ] `README.zh-CN.md` — the same three updates
- [ ] `skills/fastpaper/SKILL.md` — source count in the frontmatter description **and** in the body, plus a bullet in the domain list; a source without an author filter also joins the exception list in the `--author` row
- [ ] `docs/sources/` — add your research note and a line in its README index
- [ ] Shell completions regenerate from the enum automatically, but users must re-run `fastpaper completions <shell>` after upgrading — no file to edit

## Step 6 — Real-API smoke test

Add one `#[ignore]` test in `tests/cli.rs` that hits the real API through the binary:

```bash
cargo test --test cli <test_name> -- --ignored
```

It is run manually, never in CI. Be polite: verify once, don't hammer. If it fails, first determine whether it's the provider's risk control (try a different query, wait) before concluding the integration broke. Never automate captcha solving or token reverse-engineering — if the provider demands interactive verification, the source returns a clear error and stops.
