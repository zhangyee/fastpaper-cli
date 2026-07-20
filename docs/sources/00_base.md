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
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String>
// 部分源另有:get_by_id / get_by_doi / get_by_pmid 等
```

- `base_url` 由 `main.rs` 注入(默认真实 URL,`FASTPAPER_<SRC>_URL` 可覆盖),保证可对 mockito 测试。
- 错误即 `Err(String)`(人类可读),`main.rs` 打印并非零退出。
- 下载/读取能力由 `cli.rs` 的 `supports_download()`/`supports_read()` 声明,PDF 获取逻辑在 `download.rs`/`main.rs` 组织。

## 查询编码

`src/sources/mod.rs` 提供 `encode_query`(空格编码为 `+`,RFC 3986 unreserved 之外百分号编码),所有源统一使用。

## HTTP 共用约定

所有源用 `ureq` 同步请求。**没有共享的 retry 函数**——按"源完全独立"原则,退避逻辑由需要它的源自行实现(参考实现:`src/sources/semantic.rs`,含指数退避 + jitter、Retry-After 解析、进程级节流)。共同惯例:

- 发送项目 User-Agent:`fastpaper-cli/<version> (+https://github.com/zhangyee/fastpaper-cli)`;
- 429 → 退避重试或返回明确的限频错误;5xx → 明确的服务端错误;
- 需要请求头/风控配合的源,把相关常量集中定义并注释来源(参考 `src/sources/xueshu.rs` 的 `ACS_TOKEN_FALLBACK`)。
