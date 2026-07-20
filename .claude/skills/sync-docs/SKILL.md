---
name: sync-docs
description: Use when auditing this repo's documentation for drift — English/Chinese versions out of sync, or docs no longer matching the code (after adding a source, changing the CLI, editing capabilities, or before a release). Triggers on "check the docs", "sync docs", "文档审核", "中英文不一致".
---

# sync-docs

Audit fastpaper-cli's docs for two kinds of drift, then **propose a plan and wait for approval before changing anything**.

- **Bilingual drift** — an `.md` and its `.zh-CN.md` twin diverge in structure or content.
- **Code drift** — a doc states something the code no longer does (counts, capabilities, module layout, endpoints, env vars).

Core rule: **the code is the source of truth for behavior; English is the authoring base for prose.** Never edit during the audit. Produce findings → propose → get a yes → execute → commit.

## Doc inventory

**Bilingual pairs** (must stay structurally parallel — same headings, same section order):
- `README.md` ⇄ `README.zh-CN.md`
- `CONTRIBUTING.md` ⇄ `CONTRIBUTING.zh-CN.md`
- `docs/{architecture,adding-a-source,testing,release}.md` ⇄ their `.zh-CN.md`

**Single-language by design** (do NOT flag as "missing translation"):
- `docs/sources/00_base.md` … `18_xueshu.md` — Chinese-only research notes.
- `docs/sources/README.md` — English index only.
- `skills/fastpaper/SKILL.md` — English, the published skill (audited for code drift only).

## Pass A — bilingual consistency

For each pair:
1. **Structure**: `grep -c '^## ' both files` — heading counts must match; section order must correspond.
2. **Content**: read both; every fact, list item, table row, code block present in one must exist in the other. Numbers and identifiers must be identical.
3. **Language-switch header**: each file links to its twin (`> 中文版:…` / `> English:…`). Do not re-introduce "keep in sync" prose — that is this skill's job, not the docs'.

## Pass B — code consistency

Run these against the current tree, report every mismatch:

| Claim in docs | Source of truth | How to check |
|---|---|---|
| Source count ("N academic sources", table rows, SKILL.md ×2, zero-config count) | `ls src/sources/*.rs \| grep -v mod.rs \| wc -l` (18) | counts must agree across README×2 + SKILL.md description + SKILL.md body |
| Per-source search/download/read capabilities | `cli.rs::supports_download` / `supports_read`, and `main.rs` dispatch arms | table columns match the code's capability sets |
| Module map / data flow (`architecture.md`) | `src/` layout, `sources::*::search` signature | file list and function names exist as written |
| Env var names (`FASTPAPER_*_URL`, keys) | `grep -rn 'env::var' src/` | every documented var appears in code and vice-versa |
| Per-source endpoints/paths (`docs/sources/*.md`) | that source's `src/sources/<name>.rs` | documented URL/params/field mappings match the code |
| adding-a-source touch points | `cli.rs` + `main.rs` | the four wiring points still exist as described |

The source list also appears in `docs/sources/README.md` (index rows) and each source's capability line — include them in the count check.

## Output → approval → execute → commit

1. **Findings**: one table, most-severe first. Each row: file:line, drift type (`bilingual` / `code`), what's wrong, source of truth.
2. **Proposed plan**, per finding, classified:
   - **translate-sync** — port the missing/updated content between twins (English base wins unless the Chinese side is the newer fact).
   - **update-to-code** — rewrite the doc to match current code.
   - Flag anything ambiguous (e.g. an intentional wording difference) for the user to decide rather than "fixing".
3. **Wait for explicit approval.** Do not edit until the user says go. If they amend the plan, re-state it.
4. **Execute** only the approved items.
5. **Verify**: re-run Pass A structure check + `cargo test` (docs changes shouldn't break tests) + confirm links resolve.
6. **Commit** with `docs: sync ...` describing what was reconciled.

## Common mistakes

- **Translating the source notes.** `docs/sources/*.md` are Chinese-only on purpose. Only their *facts vs code* matter, never their English absence.
- **Auto-fixing.** This skill proposes; the user approves. Editing before approval violates the workflow.
- **Trusting the doc over the code** for a behavior claim. When they disagree, the code is right and the doc is the bug (unless the code itself is the bug — then flag it separately, don't silently document the broken behavior as intended).
- **Counting only the README table.** The source count lives in ~5 places; a fix that misses one leaves drift.
