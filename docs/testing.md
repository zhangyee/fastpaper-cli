# Testing

> 中文版:[testing.zh-CN.md](testing.zh-CN.md) — keep both in sync.

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

## Conventions

- One behavior per test; the test name states the behavior (`parse_empty_doi_becomes_none`).
- Tests that set or remove environment variables use `serial_test`'s `#[serial]` to avoid cross-test races.
- New features come with tests in the same PR — reviewers will ask.
