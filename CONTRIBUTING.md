# Contributing to fastpaper-cli

> 中文版:[CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)

Entry point for contributors. Detailed developer docs live under [docs/](docs/).

## Development setup

All you need is a stable Rust toolchain:

```bash
git clone https://github.com/zhangyee/fastpaper-cli
cd fastpaper-cli
cargo build
cargo test
```

No API keys or configuration are required — the entire test suite runs offline against recorded fixtures.

## Project layout

`fastpaper` is a synchronous, stateless CLI: clap parses the command, `main.rs` dispatches to one self-contained source module, and the result is rendered by the output module. Read [docs/architecture.md](docs/architecture.md) for the module map, data flow, and design philosophy.

## Making changes

- **Tests first.** Every PR must come with tests. Parser changes are tested against fixtures; HTTP behavior is tested with mockito. See [docs/testing.md](docs/testing.md).
- **Conventional commits.** Use `feat` / `fix` / `docs` / `chore` / `style` with an optional scope, e.g. `feat(xueshu): add fixture and basic search response parsing`.
- **PR flow.** Fork → feature branch → PR against `main`. In the PR description, explain the motivation and how you tested the change.

## Adding a new source

The most common contribution. Step-by-step guide with a worked example: [docs/adding-a-source.md](docs/adding-a-source.md).

## Versioning & releases

Versions and tags are managed by the maintainer. **Do not change `version` in `Cargo.toml` in a PR.** How releases are built and published is described in [docs/release.md](docs/release.md).

## Reference

Per-source API research notes (endpoints, response shapes, rate limits — written in Chinese) are indexed in [docs/sources/README.md](docs/sources/README.md).
