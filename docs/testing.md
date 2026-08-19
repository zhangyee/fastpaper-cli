# Testing

> 中文版:[testing.zh-CN.md](testing.zh-CN.md)

## Running tests

```bash
cargo test                 # everything (offline, no keys needed)
cargo test --lib           # unit tests only
cargo test --test cli      # CLI integration tests only
```

Layout:

- **Unit tests** live next to the code, in each source file's `#[cfg(test)]` module (≈330 today). Parser tests load a recorded fixture with `include_str!`.
- **Integration tests** live in `tests/cli.rs` (≈57 today), driving the real binary with `assert_cmd` against local mockito servers.

## HTTP mocking

We use [mockito](https://docs.rs/mockito) (the synchronous API). Typical patterns:

```rust
let mut server = mockito::Server::new();
let mock = server
    .mock("GET", mockito::Matcher::Regex("pn=10".to_string()))  // path+query regex
    .match_header("acs-token", "expected-value")                 // assert a request header
    .with_status(200)
    .with_body(FIXTURE)
    .expect(1)                                                   // exact call count
    .create();
// ... call search(&server.url(), ...) ...
mock.assert();
```

`Matcher::Any` matches everything; `.expect(0)` + `.assert()` proves an endpoint was *not* called (useful for pagination early-stop tests).

## Fixtures

- Location: `tests/fixtures/<source>_search.json` (or `.xml` / `.html`).
- Must be **sanitized real responses**, not hand-invented JSON: strip signed URLs and `authorization=` query strings, anonymize log/request IDs, remove anything personal.
- Keep realistic variance (empty strings, `null`s, HTML tags) — that's what parser tolerance tests feed on.

## Real-API tests

Tests that hit live services are marked `#[ignore]` and live in `tests/` with everything else:

```bash
cargo test --test cli -- --ignored     # run them manually
```

They never run in CI. When one fails, first decide whether the provider's rate limiting / risk control is the cause (try another query, wait, retry once) before treating it as a regression.

## Checking PDF layout against the page

`read` infers a paper's structure from its typography, and the extracted text
does not carry the evidence that reading rests on. Checking a layout change by
reading the extracted text is how the wrong rule gets written down: in one
paper `Introduction` and the `Background` subsection under it are set in the
same size, and the extracted text shows nothing else to tell them apart. The
page shows one blue upright and the other black italic.

So a change to layout reading is checked against a render, not against the
text. Install the renderer once:

```bash
brew install poppler imagemagick     # or apt-get install poppler-utils imagemagick
```

Two diagnostics in `tests/real_papers.rs` produce what to look at. Both are
`#[ignore]`d, and both need a directory of PDFs that the repo does not carry:

```bash
FASTPAPER_PAPERS=/path/to/papers cargo test --test real_papers -- --ignored --nocapture
```

`report_heading_positions` prints every heading that was read, and every
heading-shaped line that was not, with the page and baseline each sits on --
enough to crop the strip of page it came from:

```bash
pdftoppm -png -r 130 -f "$PAGE" -l "$PAGE" -singlefile paper.pdf page
# PDF y counts up from the foot of the page; image y counts down from its head
TOP=$(awk -v h="$HEIGHT" -v y="$Y" 'BEGIN{print int((h-y-11)*130/72)}')
magick page.png -crop "x72+0+$TOP" +repage strip.png
```

Stacking one strip per heading gives a sheet per paper, which is what makes
"did it read every heading, and is every heading it read real" a question you
can answer by looking. `report_detected_headings` and
`report_unrecognised_heading_shapes` give the same information as text, which
is how a publisher's wording variant gets found rather than guessed at.

Two rules this practice exists to enforce:

- **A threshold goes into the code as the value that was measured.** Copying a
  measured 0.90 into the code as a mirrored 0.07 left a folio at 0.91 of the
  page height in the text, and the tests written alongside did not catch it
  because they used made-up coordinates.
- **A corpus of one publisher proves nothing about publishers.** A rule that
  deletes text needs to be right rather than usually right; when the evidence
  cannot carry it, leave the text alone and say so in the docs.

## Conventions

- One behavior per test; the test name states the behavior (`parse_empty_doi_becomes_none`).
- Tests that set or remove environment variables use `serial_test`'s `#[serial]` to avoid cross-test races.
- New features come with tests in the same PR.
