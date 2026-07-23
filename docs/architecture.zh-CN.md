# 架构

> English: [architecture.md](architecture.md)

描述当前代码的实际状态。代码与本页不一致时以代码为准,并修正本页。

## 模块地图

```
src/
├── main.rs          # 命令分发:读取 env var 覆盖的 base URL,调用数据源,渲染输出
├── cli.rs           # 全部 clap 定义:Cli、GlobalOpts、Commands、各命令 Args,
│                    #   以及 Source / OutputFormat / SortField / Section 枚举
├── sources/
│   ├── mod.rs       # Paper struct(共享数据契约)+ encode_query 工具函数
│   └── <source>.rs  # 每个数据源一个完全自包含的模块(目前 18 个)
├── download.rs      # fetch_pdf、各源 download_<src> 函数、save_pdf
├── read.rs          # PDF 文本提取(pdf_oxide):extract_text、extract_text_from_bytes、
│                    #   extract_section
├── output.rs        # 渲染器:to_table、to_json、to_csv、to_bibtex
└── identifier.rs    # `get` 用的 detect_id_type:Arxiv、ArxivOld、Doi、Pmc、Pmid、S2、Url、Unknown
```

## 数据流

主路径 `fastpaper search <source> <query>`:

1. clap 解析命令行(`cli.rs`)。
2. `main.rs` 匹配数据源,解析 base URL——默认常量,可用 `FASTPAPER_<SOURCE>_URL` 环境变量覆盖——然后调用源模块。
3. 每个源实现相同的入口:

   ```rust
   pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String>
   ```

4. 返回的 `Vec<Paper>` 按 `--format` 由 `output::to_table` / `to_json` / `to_csv` / `to_bibtex` 渲染。

其他命令:

- **`download`** — 为标识符解析出 PDF URL,经 `download.rs` 保存(仅支持能提供 PDF 的源)。
- **`read`** — 获取 PDF 字节(本地文件或远程抓取),用 `read.rs` 提取文本,可选切出某一章节。
- **`get`** — `identifier::detect_id_type` 按标识符形状(DOI、arXiv ID、PMC ID、PMID、S2 ID)推断数据源,再调用该源的查询函数。

## Paper 数据契约

`sources/mod.rs` 定义唯一的共享数据结构,每个源把各自的 API 响应映射到它:

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | `String` | 源自身的标识符 |
| `title` | `String` | 纯文本(各源负责去除 HTML 高亮标签) |
| `authors` | `Vec<String>` | |
| `abstract_text` | `Option<String>` | 序列化名为 `"abstract"`(serde rename) |
| `year` | `Option<u16>` | |
| `doi` | `Option<String>` | 空字符串转为 `None` |
| `url` | `Option<String>` | 落地页 |
| `pdf_url` | `Option<String>` | 已知时为 PDF 直链 |
| `venue` | `Option<String>` | 期刊 / 出版方 |
| `citations` | `Option<u32>` | |
| `fields` | `Vec<String>` | 学科领域(如 `cs.CL`) |
| `open_access` | `Option<bool>` | |
| `source` | `String` | 源名,如 `"arxiv"` |

JSON 输出中缺失值一律为 `null`,绝不省略字段——schema 稳定,agent 和脚本可以放心依赖。

## 数据源模块契约

一个数据源模块就是 `src/sources/` 下的一个文件,完全自包含:

- 入口:上面的 `search` 签名。部分源额外提供 `get_by_id` / `get_by_doi` / `get_by_pmid` 等查询。
- `base_url` 始终由调用方注入——这是每个源都能对着本地 mockito server 测试的关键。
- 错误用 `Result<_, String>` 返回可读消息;`main.rs` 打印后以非零码退出。
- 单元测试放在同文件的 `#[cfg(test)]` 里,由 `tests/fixtures/` 中录制的 fixture 驱动。

源模块之间的代码重复是**有意接受**的——见下面的设计哲学。

## 设计哲学

| 原则 | 理由 |
|---|---|
| 零配置 | 所有源开箱即用(Unpaywall 需邮箱是唯一例外);无配置文件。 |
| 单源单命令 | 并行属于外层 agent/shell 多进程调度,不属于本二进制。 |
| Source 作为位置参数 | `fastpaper search arxiv <q>` 打字快,agent 构造命令直接。 |
| 输出可管道化 | 默认人类可读表格,`--format json` 供机器消费。 |
| 同步阻塞 I/O | 用 `ureq` 不引入 async runtime:二进制小、编译快、代码简单。 |
| 源完全独立实现 | 每源一个文件;接受重复,换来任何源可被独立修改或删除。 |
| 纯无状态无缓存 | 每次调用都是新鲜的 API 请求——简单、可靠、无副作用。 |
| 429 自动退避 | 有限频的 API 上,源内部做指数退避重试,对用户透明。 |
| 标识符自动检测 | `get` 从标识符格式推断数据源。 |

## 非目标

设计哲学的自然推论,刻意不做:不引入 async runtime、不做本地缓存或状态目录、不加配置文件、不在二进制内做跨源聚合。
