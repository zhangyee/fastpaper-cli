# 架构

> English: [architecture.md](architecture.md)

描述当前代码的实际状态。代码与本页不一致时以代码为准,并修正本页。

## 模块地图

```
src/
├── main.rs          # 解析命令行并分发;把错误映射成退出码
├── cli.rs           # 全部 clap 定义:Cli、GlobalOpts、Commands、各命令 Args、
│                    #   OutputFormat / Section,以及 [source] <id> 的消歧逻辑
├── commands/        # 每条命令一个模块,各自负责校验、调用与渲染
│   ├── mod.rs       # CommandError(自带退出码)、render、emit(+ -o 回执)
│   └── search.rs · get.rs · download.rs · read.rs · sources.rs
├── registry.rs      # Source 枚举,以及把每个源映射到其 base URL、Capabilities
│                    #   和函数的唯一一张表(纯函数指针,不引入 trait)
├── sources/
│   ├── mod.rs       # Paper(共享数据契约)、SearchQuery、Capabilities / SearchCaps、
│   │                #   encode_query、validate_ymd、contact_email
│   └── <source>.rs  # 每个数据源一个完全自包含的模块(目前 18 个)
├── download.rs      # fetch_pdf、各源 pdf_bytes_<src> 解析函数、save_pdf
├── read.rs          # PDF 文本提取(pdf_oxide):extract_text、extract_text_from_bytes、
│                    #   extract_section
├── grep.rs          # 在文本中查找匹配,并在命中位置两侧截取上下文窗口
├── output.rs        # 渲染器:to_table、to_json、to_jsonl、to_csv、to_bibtex
└── identifier.rs    # detect_id_type:Arxiv、ArxivOld、Doi、Pmc、Pmid、S2、Url、Unknown
```

## 数据流

主路径 `fastpaper search <source> <query>`:

1. clap 解析命令行(`cli.rs`)。
2. `main.rs` 分发到对应的 `commands/` 模块,由 `registry.rs` 解析 base URL——默认常量,可用 `FASTPAPER_<SOURCE>_URL` 环境变量覆盖——然后调用源模块。
3. 每个源实现相同的入口:

   ```rust
   pub fn search(base_url: &str, q: &SearchQuery) -> Result<Vec<Paper>, String>
   ```

4. 返回的 `Vec<Paper>` 按 `--format` 由 `output::to_table` / `to_json` / `to_jsonl` / `to_csv` / `to_bibtex` 渲染。

第 3 步之前,`commands/search.rs` 会拿请求去对该源的 `Capabilities`:空查询、
超过该源上限的 `-n`、或它无法支持的过滤参数,都会直接报错并说明**它支持哪些**,
以及**哪些源支持被拒的那个参数**——错的通常是源,不是参数本身。被静默丢掉的过滤
条件会产出看着对、其实不对的结果。

其他命令:

- **`download`** — 经该源的 `pdf` 函数把标识符解析成 PDF 字节,再由 `download::save_pdf` 落盘。给一个参数时数据源由标识符形状推断,给两个参数时是显式指定的。
- **`read`** — 只读本地 PDF,完全不联网:`download` 负责取,`read` 负责抽。
- **`get`** — 与 `download` 同样的两种参数形态。推断数据源时由 `identifier::detect_id_type` 把形状(DOI、arXiv ID、PMC ID、PMID、S2 ID)映射到源;注意它的路由与 `download` 不同——Crossref 掌管 DOI 元数据,但不提供文件。

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
- URL 构造放在纯函数 `build_*_url` 里,这样过滤参数的映射可以直接断言,不必经过 mock。(mockito 优先匹配后创建的 mock,所以藏在 catch-all 后面的 `expect(0)` 探针会假通过。)
- 错误用 `Result<_, String>` 返回可读消息;命令层包成 `CommandError`,由 `main.rs` 以非零码退出。
- 单元测试放在同文件的 `#[cfg(test)]` 里,由 `tests/fixtures/` 中录制的 fixture 驱动。

源模块之间的代码重复是**有意接受**的——见下面的设计哲学。

源文件里不会引用 `registry.rs`,接线方向是反过来的。`registry.rs` 存的是 `fn` 指针
而非 trait,所以每个源始终只是一组自由函数,不需要 impl 任何东西。

## 设计哲学

| 原则 | 理由 |
|---|---|
| 零配置 | 所有源开箱即用(Unpaywall 需邮箱是唯一例外);无配置文件。API key 只用于提高限额。 |
| 单源单命令 | 并行属于外层 agent/shell 多进程调度,不属于本二进制。 |
| Source 作为位置参数 | `fastpaper search arxiv <q>` 打字快,agent 构造命令直接。 |
| 输出可管道化 | 默认人类可读表格,`--format json` 供机器消费。 |
| 同步阻塞 I/O | 用 `ureq` 不引入 async runtime:二进制小、编译快、代码简单。 |
| 源完全独立实现 | 每源一个文件;接受重复,换来任何源可被独立修改或删除。 |
| 纯无状态无缓存 | 每次调用都是新鲜的 API 请求——简单、可靠、无副作用。 |
| 429 自动退避 | 有限频的 API 上,源内部做指数退避重试,对用户透明。 |
| 标识符自动检测 | `get` 与 `download` 都能从标识符形状推断数据源;显式指定则覆盖推断。 |
| 能力只声明一处 | `registry.rs` 声明每个源能做什么;参数校验、`sources --capabilities` 和报错文案全都读它,所以列表不可能与实际行为脱节。 |
| 宁可吵闹报错,不做静默降级 | 不支持的过滤参数、超上限的 `-n`、空查询,都是错误,而不是悄悄换成另一个请求。 |

## 非目标

设计哲学的自然推论,刻意不做:不引入 async runtime、不做本地缓存或状态目录、不加配置文件、不在二进制内做跨源聚合。
