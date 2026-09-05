# Base Types & 共用约定

> 本页描述当前代码的实际状态(2026-07 校对)。模块全景见 [../architecture.zh-CN.md](../architecture.zh-CN.md)。

## Paper(共享数据结构)

定义在 `src/sources/mod.rs`,是所有源唯一的输出契约。缺失字段在 JSON 输出中统一为 `null`。

```rust
struct Paper {
    id: String,                    // 源自身的标识符 (arXiv ID, PMID, DOI 等)
    title: String,
    authors: Vec<String>,
    abstract_text: Option<String>, // serde rename: JSON 中为 "abstract"
    year: Option<u16>,
    doi: Option<String>,           // 空串转 None
    url: Option<String>,
    pdf_url: Option<String>,
    venue: Option<String>,
    citations: Option<u32>,
    fields: Vec<String>,           // 学科领域 / 分类 (如 cs.CL, q-bio)
    open_access: Option<bool>,
    source: String,                // 来源平台名 (如 "arxiv", "pubmed")
}
```

## 源模块接口(自由函数,非 trait)

早期设计稿中的 `PaperSource` trait、`SearchResult`/`DownloadResult`/`ReadResult` 包装结构**未采用**。实际约定是每源一个文件、导出自由函数:

```rust
pub fn search(base_url: &str, q: &SearchQuery) -> Result<Vec<Paper>, String>
// 部分源另有:get_by_id / get_by_doi / get_by_pmid 等
```

- `base_url` 由 `registry.rs` 注入(默认真实 URL,`FASTPAPER_<SRC>_URL` 可覆盖),保证可对 mockito 测试。
- 错误即 `Err(String)`(人类可读),命令层包成 `CommandError`,`main.rs` 打印并非零退出。
- **URL 构造放在纯函数 `build_*_url` 里**,过滤参数的映射直接断言即可,不必绕 mock。(mockito 优先匹配后创建的 mock,藏在 catch-all 后面的 `expect(0)` 探针会假通过。)

## SearchQuery(归一化的检索请求)

定义在 `src/sources/mod.rs`。每个源把自己能支持的字段映射到原生参数,不支持的由命令层提前拦下。

```rust
struct SearchQuery {
    query: String,
    limit: u32,
    offset: u32,
    sort: Option<SortField>,   // Relevance | Date | Citations
    order: SortOrder,          // Asc | Desc
    year: Option<u16>,
    after: Option<String>,     // YYYY-MM-DD
    before: Option<String>,
    author: Option<String>,
    field: Option<String>,
    open_access: bool,
}
```

## 能力声明(Capabilities)

能力**只在 `src/registry.rs` 声明一处**:

```rust
struct Capabilities {
    search: Option<SearchCaps>,  // None = 该源没有关键词检索
    get: bool,
    download: bool,
    max_limit: Option<u32>,      // 源侧单次请求硬上限
    notes: &'static str,         // sources --capabilities 里展示的注意事项
}
```

`SearchCaps` 逐项声明 offset / sort / year / date_range / author / field / open_access。参数校验、`fastpaper sources --capabilities` 的输出、以及报错文案全都读它,所以三者不可能互相脱节。

早先版本把能力写在 `cli.rs` 的 `supports_download()` / `supports_read()` 里,并在 `main.rs` 另抄了一份硬编码表——两处都已删除。

**不支持的过滤参数一律报错退出**,并列出该源实际支持哪些、以及哪些源支持被拒的那个参数(同样查 `SearchCaps`,不写死在文案里)。静默忽略会产出看着对、其实不对的结果,且无从排查。

## 查询编码

`src/sources/mod.rs` 提供 `encode_query`(空格编码为 `+`,RFC 3986 unreserved 之外百分号编码),所有源统一使用。

## HTTP 共用约定

所有源用 `ureq` 同步请求。**没有共享的 retry 函数**——按"源完全独立"原则,退避逻辑由需要它的源自行实现(参考实现:`src/sources/semantic.rs`,含指数退避 + jitter、Retry-After 解析、进程级节流)。共同惯例:

- 发送项目 User-Agent:`fastpaper-cli/<version> (+https://github.com/zhangyee/fastpaper-cli)`;
- 429 → 退避重试或返回明确的限频错误;5xx → 明确的服务端错误;
- 需要额外请求头的源,把相关常量集中定义并注释其出处,不要散落在请求构造里。
