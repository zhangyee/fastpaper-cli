# 新增数据源指南

> English: [adding-a-source.md](adding-a-source.md)

先阅读[数据源模块契约](architecture.zh-CN.md#数据源模块契约)。全文以 `arxiv` 为贯穿实例——它是唯一一个把下面每一步都走完的源,包括 Step 4 的 CLI 集成测试和 Step 6 的冒烟测试。它有两件事**不**典型:PDF 主机与 API 主机是分开的(`pdf_env_var` / `pdf_default_base`),以及从投稿 tarball 里抽 `figures`。多数源两样都不需要,跳过即可。

## Step 0 — 调研

写代码之前,先产出一份调研笔记,回答:

- **端点**:URL、HTTP 方法、必需请求头。
- **参数**:查询串编码、分页方案、每页条数。
- **响应结构**:论文列表在哪个字段下、字段名与类型、缺失值如何表示(`null`、空串、字段缺席——常常三者都有)。
- **认证与反爬行为**:API key、限频、风控特征。
- **分页**:offset/cursor、最后一页如何判断。

体例参考 [docs/sources/](sources/README.md) 里的现有笔记,写完放进去。

*arxiv 实例:*`GET /api/query` 返回的是 Atom/XML 而不是 JSON;查询放在 `search_query`,翻页用 `start` / `max_results`。没有 key 也没有验证码,但 arXiv 要求调用方请求之间隔 ~3 秒,所以模块用指数退避包住请求,而不是猛打。

## Step 1 — 抓取 fixture

发**一次**真实请求,把响应存为 `tests/fixtures/<source>_search.json`(或 `.xml`)。先脱敏:

- 去掉带签名的 CDN URL、会话 token、任何含 `authorization=` 的查询串;
- 匿名化请求/日志 ID;
- 保留结构和真实的字段差异(空 DOI、缺失年份、标题里的 HTML 标签)——这些差异正是测试需要的。

## Step 2 — 先写解析器

创建 `src/sources/<source>.rs`。在写任何 HTTP 代码之前,对着 fixture 写解析器:

```rust
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String>
```

参数名跟着源实际返回的格式走——多数是 `json`,arXiv 和 PubMed 是 `xml`。测试先行:每个行为先写失败测试,再写最小实现。至少覆盖:

- 正常路径:论文数量符合预期、id/title 非空、`source` 字段正确;
- 字段映射:作者、年份、引用数、venue 回退、空串 → `None`;
- 文本清理:标题**和**摘要都要去掉搜索高亮标签(如 `<em>`);
- 容错:`null`、空串、字段缺席都不能导致解析失败;
- API 层错误:业务错误码必须返回清晰的 `Err`,而不是空列表。

*arxiv 实例:*43 个模块内测试,对着 `tests/fixtures/arxiv_search.xml`,覆盖作者列表、没有 DOI 的记录,以及不带 PDF 链接的条目形态。

## Step 3 — HTTP 层

加上标准入口:

```rust
pub fn search(base_url: &str, q: &SearchQuery) -> Result<Vec<Paper>, String>
```

规则:

- `base_url` 由调用方传入——请求代码里绝不硬编码。
- 查询串用 `super::encode_query` 编码。
- 发送项目 User-Agent(`fastpaper-cli/<version> (+仓库 URL)`);有些风控引擎会拦截 HTTP 库的默认 UA。
- 若接口分页,则串行分页;拿够 `max_results`、遇到空页或出错即停;返回前截断。
- 把 HTTP 状态映射为可区分的可读错误(403 被拦截 / 429 限频 / 其他)。

用 mockito(同步)测试:请求路径与参数、必需请求头、第二页分页、满足 `max_results` 后提前停止、每种错误状态。*arxiv 实例:*`tests/cli.rs` 里 15 个测试。arXiv 自己是单请求翻页(`start` / `max_results`);要看串行翻页去读 `biorxiv` / `medrxiv`,只有这两个源真的这么做。

## Step 4 — 接线 CLI

三个接线点,全在 `src/registry.rs`:

1. `enum Source` 加变体,并加进 `ALL`。
2. 加它的 `static <SOURCE>: SourceEntry`:名字、环境变量与默认 base URL、`search` / `get` / `pdf` 函数指针,以及如实声明它能支持哪些过滤参数的 `Capabilities`。
3. `Source::entry()` 加分支。

没有第四步:`fastpaper sources` 与参数校验都读 `Capabilities`,所以列表和报错文案
由第 2 步自动跟上。**只有该源真把某个过滤参数映射到了原生参数,才把它声明为 `true`**
——撑不住的 `true` 会产出看着对、其实不对的结果。

在 `tests/cli.rs` 加集成测试(mockito server + `FASTPAPER_<SOURCE>_URL` 环境变量):搜索成功并输出已知标题、API 错误时非零退出并带消息、`sources` 列表包含新源名。

## Step 5 — 文档同步清单

- [ ] `README.md` — 源数量(两处:"N academic sources" 和 "N of M sources work with zero configuration")+ 表格行
- [ ] `README.zh-CN.md` — 同样三处
- [ ] `skills/fastpaper/SKILL.md` **和** `skills/fastpaper/SKILL.en.md`(两份都要改)— frontmatter description 里的数量**和**正文里的数量,外加领域列表里的一条;没有 author 过滤的源还要写进 `--author` 那行的例外名单
- [ ] `docs/sources/` — 放入你的调研笔记,并在其 README 索引加一行
- [ ] Shell completions 由 enum 自动生成,无需改文件,但用户升级后需重新执行 `fastpaper completions <shell>`

## Step 6 — 真实 API 冒烟测试

在 `tests/cli.rs` 加一个 `#[ignore]` 测试,通过二进制访问真实 API:

```bash
cargo test --test cli <test_name> -- --ignored
```

只手动执行,绝不进 CI。保持礼貌:验证一次即可,不要轰炸。失败时先判断是不是对方风控(换个关键词、等一等),再下结论是集成坏了。绝不自动化验证码破解或逆向风控 token——对方要求交互式验证时,数据源应返回清晰错误并停止。
