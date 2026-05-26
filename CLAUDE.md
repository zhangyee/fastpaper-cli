# CLAUDE.md

## 项目
fastpaper — Rust CLI，学术论文搜索/下载/阅读工具。

## 开发规则
- 使用 TDD：先写失败的测试，再写实现，每次只执行一个行为。
- 测试命令：cargo test
- HTTP mock 用 mockito crate（同步）
- API 响应 fixture 放在 tests/fixtures/
- 真实 API 测试放在 tests/ 里标记 #[ignore]

## 发版规则
- **平时（包括 TDD 的 red/green/refactor 各步 commit）不要询问版本号**，正常 commit 即可，保持 Cargo.toml 的 `version` 不变。
- **只在我明确说“准备发版 / 发布 / release / 打 tag”等意图时**，才执行下列流程：
  1. 查看当前版本（Cargo.toml 的 `version`）和上一个 git tag。
  2. 汇总自上个 tag 以来的所有 commit，按改动性质（feat / fix / breaking / docs / chore）给出 SemVer 升级建议。
  3. 提醒我是否需要打 tag（如 `v0.2.0`）以及是否需要 `cargo publish` / 触发 release workflow。
  4. **由我决定**最终版本号和 tag；我确认后，Claude 再修改 Cargo.toml（及 Cargo.lock、相关配置），然后 commit + 打 tag。
- 任何情况下都不要在未经我确认时自行改 `version` 字段或创建 git tag。

## 工具
- 解析 JSON 用 jq（已安装），不要用 python

## 关键类型
- Paper struct: id, title, authors, abstract, year, doi, url, pdf_url, source
- Source enum: Arxiv, Pubmed, Pmc, Semantic, Crossref, ...
- IdType enum: Arxiv, Doi, Pmc, Pmid, Unknown