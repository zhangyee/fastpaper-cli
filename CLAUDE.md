# CLAUDE.md

## 项目
fastpaper — Rust CLI，学术论文搜索/下载/阅读工具。

## 开发规则
- 使用 TDD：先写失败的测试，再写实现，每次只执行一个行为。
- 测试命令：cargo test
- HTTP mock 用 mockito crate（同步）
- API 响应 fixture 放在 tests/fixtures/
- 真实 API 测试放在 tests/ 里标记 #[ignore]

## 关键类型
- Paper struct: id, title, authors, abstract, year, doi, url, pdf_url, source
- Source enum: Arxiv, Pubmed, Pmc, Semantic, Crossref, ...
- IdType enum: Arxiv, Doi, Pmc, Pmid, Unknown