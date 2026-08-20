[English](README.md) | **中文**

# fastpaper CLI with Skill

一个为 AI 智能体（Claude Code、Codex、Opencode 等）提供学术论文及科技文献快速检索、下载和阅读能力的 CLI 工具。配套 [SKILL](skills/fastpaper/SKILL.md) 文件，教会智能体按领域选择数据源并构造命令。

单条命令、单个数据源、零配置。多源并行检索由智能体启动多个进程实现。

## 安装

### CLI

**Homebrew (macOS / Linux)**

```sh
brew install zhangyee/tap/fastpaper
```

**Shell 脚本 (macOS / Linux)**

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/zhangyee/fastpaper-cli/releases/latest/download/fastpaper-cli-installer.sh | sh
```

**PowerShell (Windows)**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/zhangyee/fastpaper-cli/releases/latest/download/fastpaper-cli-installer.ps1 | iex"
```

**Cargo**

```sh
cargo install fastpaper-cli
```

### Skill

安装 Skill 文件，让 AI 智能体学会使用 fastpaper。使用 [Vercel Skills](https://github.com/vercel-labs/skills)，一个将 SKILL.md 安装到各智能体的工具：

```sh
npx skills add zhangyee/fastpaper-cli --skill fastpaper
```

SKILL.md 教会智能体按研究领域选择数据源并构造命令。使用 `--format json` 获取结构化输出。所有 JSON 字段缺失时统一输出 `null`（不省略），保证 schema 稳定。

## 快速开始

```sh
# 搜索 arXiv
fastpaper search arxiv "transformer attention mechanism"

# 带过滤条件的搜索——每个源各自声明能支持哪些
fastpaper search arxiv "large language model" --after 2024-01-01 --field cs.CL --limit 20

# 通过 DOI 获取论文（自动识别数据源）
fastpaper get 10.1038/nature12373

# 通过 arXiv ID 获取
fastpaper get 2301.08745

# 显式指定数据源，控制由谁来答
fastpaper get openalex 10.1038/nature12373

# 下载 PDF 到 ./papers
fastpaper download arxiv 2301.08745

# ……或让标识符自己选源
fastpaper download 2301.08745

# 阅读刚下载的 PDF
fastpaper read papers/2301.08745.pdf

# 阅读它的特定章节
fastpaper read papers/2301.08745.pdf --section methods

# JSON 输出，供脚本 / AI 智能体使用
fastpaper search semantic "CRISPR gene editing" --format json

# 多源并行搜索
fastpaper search arxiv "protein folding" --format json &
fastpaper search pubmed "protein folding" --format json &
fastpaper search semantic "protein folding" --format json &
wait
```

## 数据源

18 个学术数据源，每条命令独立访问单个数据源。

这里不列 `read`：它读的是已经落到磁盘上的 PDF，所以凡是 `download` 列能取到的，
它都能读。

| 数据源 | 全称 | search | get | download | cite | figures | 覆盖领域 |
|--------|------|:------:|:---:|:--------:|:----:|:-------:|----------|
| `arxiv` | arXiv | yes | yes | yes | | yes | 物理、数学、计算机、统计、电子工程、量化生物/金融、经济学 |
| `biorxiv` | bioRxiv | yes | yes | yes | | | 生命科学 |
| `medrxiv` | medRxiv | yes | yes | | | | 医学 / 健康科学（medRxiv 拦截 PDF 抓取）|
| `pubmed` | PubMed | yes | yes | | | | 生物医学与生命科学（仅元数据） |
| `pmc` | PubMed Central | yes | yes | yes | | | 生物医学与生命科学（全文） |
| `europepmc` | Europe PMC | yes | yes | yes | | yes | PMC 的生命科学超集，另含预印本、专利、临床指南 |
| `scholar` | Google Scholar | yes | | | | | 全学科（实验性，有频率限制） |
| `xueshu` | 百度学术 | yes | | | | | 全学科，中文文献覆盖强（实验性，非官方接口） |
| `semantic` | Semantic Scholar | yes | yes | yes | yes | | 全学科，AI 驱动的引用图谱 |
| `crossref` | CrossRef | yes | yes | | | | DOI 元数据，全学科 |
| `openalex` | OpenAlex | yes | yes | | yes | | 开放元数据索引，2 亿+ 作品 |
| `dblp` | DBLP | yes | | | | | 计算机科学 |
| `core` | CORE | yes | yes | yes | | | 开放获取聚合器（需 `CORE_API_KEY`）|
| `openaire` | OpenAIRE | yes | yes | | | | 欧盟开放科学 |
| `doaj` | DOAJ | yes | yes | | | | 开放获取期刊，全学科（全文链接指向出版商页面而非 PDF）|
| `unpaywall` | Unpaywall | | yes | | | | OA 链接解析（需设置 `UNPAYWALL_EMAIL`） |
| `zenodo` | Zenodo | yes | yes | yes | | | 全学科（数据集、软件、论文） |
| `hal` | HAL | yes | yes | yes | | | 多学科，法国国家开放存档 |

`figures` 取的是作者的原始插图文件，而不是 PDF 页面——详见下文。只有 `arxiv`
（其源码包）与 `europepmc`（其附件包）能提供；PMC ID 或 DOI 都会路由到
`europepmc`。

各源支持的检索过滤参数、以及单次请求的结果上限并不相同。
`fastpaper sources --capabilities` 会打印完整矩阵和每个源的注意事项。

## 命令

四条命令各做一件事：`search` 找论文，`get` 取元数据，`download` 存 PDF，
`read` 从磁盘上的 PDF 抽文本。

### `search` -- 搜索论文

数据源必填：自由文本没有标识符形状可供推断。

```
fastpaper search <SOURCE> <QUERY> [OPTIONS]

选项:
  -n, --limit <N>        最大结果数 [默认: 10]
      --offset <N>       跳过前 N 条结果 [默认: 0]
      --sort <FIELD>     排序: relevance, date, citations
      --order <DIR>      asc 或 desc [默认: desc]
      --author <NAME>    按作者过滤
      --after <DATE>     指定日期之后 (YYYY-MM-DD)
      --before <DATE>    指定日期之前 (YYYY-MM-DD)
      --year <YEAR>      指定年份
      --field <FIELD>    学科领域 / 分类 (如 cs.CL)
      --open-access      仅开放获取论文
      --patents          仅专利(europepmc、xueshu)
  -f, --format <FMT>     table, json, jsonl, csv, bibtex [默认: table]
  -o, --output <PATH>    输出到文件
```

过滤参数会映射到各源自己的 API 参数。源不支持某个参数时**直接报错，既列出它支持哪些，
也列出哪些源支持你要的那个**，而不是静默忽略——被悄悄丢掉的过滤条件会产出看着对、
其实不对的结果。`-n` 超过某源单次请求上限时同理。用 `fastpaper sources --capabilities`
查看每个源接受什么。

### `get` -- 按标识符取元数据

给一个参数时它是标识符，数据源由其形状推断；给两个参数时第一个显式指定数据源。

```
fastpaper get <IDENTIFIER>
fastpaper get <SOURCE> <IDENTIFIER>
```

路由：arXiv→`arxiv`、PMC→`pmc`、PMID→`pubmed`、DOI→`crossref`、`S2:`→`semantic`。

注意这与 `download` 不同——`download` 把 DOI 送给 `semantic`，两个命令看的是**不同源
的记录**。Crossref 是 DOI 的注册机构，元数据最权威，但它不提供文件，所以
`get <DOI>` 的 `pdf_url` 恒为 `null`；而 `download <DOI>` 走 Semantic Scholar 解析
开放获取副本。不要用 `get <DOI>` 的 `pdf_url` 去预判 `download` 会不会成功。

### `download` -- 下载 PDF

与 `get` 同样的两种形态。文件存为 `<标识符>.pdf`。

```
fastpaper download <IDENTIFIER> [OPTIONS]
fastpaper download <SOURCE> <IDENTIFIER> [OPTIONS]

选项:
  -d, --dir <PATH>       下载目录 [默认: ./papers]
  -s, --max-size <SIZE>  可接受的最大响应体,如 10MiB、200MB。
                         不带单位的数字按字节算,`--max-size 10` 就是 10 字节。
                         0 表示不限制 [默认: 100MiB]
      --overwrite        覆盖已有文件
```

### `figures` -- 取论文的原始插图文件

与 `get` 相同的两种形式。取论文的原始插图文件——arXiv 源码包或 Europe PMC
附件包——存到以你输入的标识符命名的子目录下(`/` 会被替换成 `_`,与
`download` 一致)。

```
fastpaper figures <IDENTIFIER> [OPTIONS]
fastpaper figures <SOURCE> <IDENTIFIER> [OPTIONS]

选项:
  -d, --dir <PATH>       下载目录 [默认: ./papers]
  -s, --max-size <SIZE>  可接受的最大压缩包体积,如 10MiB、200MB。
                         不带单位的数字按字节算。0 表示不限制 [默认: 100MiB]
      --overwrite        覆盖已有文件
```

只有 `arxiv` 与 `europepmc` 能提供插图。PMC ID 或 DOI 都会路由到
`europepmc`;DOI 会先经 NCBI 的 ID 转换服务解析成 PMC ID。指定其他源会以
退出码 `1` 失败。在支持的源上没有插图文件的论文会以退出码 `4` 失败。

它不从 PDF 里抽图,不做任何渲染,也不解析图号——落盘的就是压缩包里原样的
文件,文件名也原样保留。**文件名与图号并不对应**:对 `2511.11035` 而言,
文件 `3.pdf` 实际上是 Figure 1。

在 39 篇论文的语料上实测,已于 2026-08-20 对活跃服务端到端验证:31 篇
(79%)取到了插图文件。未取到的 8 篇中,4 篇是 HTTP 404,3 篇的压缩包里没有
图片(非开放获取),1 篇 arXiv 论文提交的是 PDF,没有源码包。该语料里全部
22 个 DOI 都解析出了 PMC ID。

```sh
fastpaper figures 2511.11035 -d papers/       # -> papers/2511.11035/{1,2,3}.pdf
fastpaper figures PMC7075534 -d papers/       # -> papers/PMC7075534/*.jpg
```

### `cite` -- 遍历引用关系

与 `get` 相同的两种形式。返回引用边另一端的论文,结果结构与其他命令一致。
只有 `semantic` 和 `openalex` 提供引用边;裸 DOI 路由到 `openalex`(无需 API
key),arXiv 与 `S2:` 标识符路由到 `semantic`。

```
fastpaper cite <IDENTIFIER> [OPTIONS]
fastpaper cite <SOURCE> <IDENTIFIER> [OPTIONS]

Options:
      --direction <DIR>  incoming(引用了这篇的论文)或 outgoing(这篇引用的
                         论文)[默认: incoming]
  -n, --limit <N>        最大边数 [默认: 20]
  -o, --output <PATH>    写入文件而非标准输出
```

### `read` -- 阅读本地 PDF

接收路径，不联网。先 download，再读落地的文件。

```
fastpaper read <PATH> [OPTIONS]

选项:
      --section <SEC>    abstract, introduction, methods, results,
                         discussion, conclusion, references, full [默认: full]
      --list-sections    列出 PDF 里能切出哪些段落,而不是读取内容
      --grep <PATTERN>   用正则表达式搜索文本。
                         默认不区分大小写;用 (?-i) 切换为区分大小写
      --context <N>      每个匹配两侧各保留多少字符的上下文
                         (需要 --grep)[默认: 500]
      --max-matches <N>  最多显示 N 个匹配(需要 --grep)[默认: 10]
      --max-length <N>   截断输出到 N 个字符
  -o, --output <PATH>    输出到文件
```

```sh
fastpaper download 2301.08745            # -> ./papers/2301.08745.pdf
fastpaper read papers/2301.08745.pdf --section abstract
fastpaper read papers/2301.08745.pdf --grep 'limitations?'
```

PDF 本身不记录章节结构,所以 `--section` 读的是排版:标题就是那种独占一行、且字号
不小于正文的行。期刊实际用的那些体例都能应付——带编号的 `II.METHODS`、写成
`Materials and methods` 的、Nature 那种 Results 排在 Methods 前面的——但排版异常的
论文可能切不出某一段,这时 `--section` 退出码为 4,而不是猜一个给你。要在开口要之前
先看有哪些段,用 `--list-sections`:

```sh
fastpaper read papers/2301.08745.pdf --list-sections
```
```
abstract       p1    @105       12.0pt  Abstract
introduction   p1    @1763      12.0pt  1Introduction
conclusion     p8    @30510     12.0pt  5Conclusion
references     p9    @31793     12.0pt  References
```

这篇实际不止四段:其中一段的标题叫 `Analysis`,不在 `--section` 收的名字里。页码是
为了能拿去和印出来的页面对照——这是判断切得对不对的唯一办法。

如果列表里没有 `abstract` 或 `introduction`,通常是论文本身如此而不是没读出来——
Nature 系的摘要上面根本不印标题。这时直接读开头就行,拿到的还是那几千字:

```sh
fastpaper read papers/2301.08745.pdf --max-length 3000
```

加上 `--format json` 时,`--section` 的输出还会报出这一刀实际是从哪个标题起的——
`heading.text`、`heading.offset`、`heading.font_size`。要引用某一段的调用方,可以拿
它来核对切得对不对。

有两件事 `read` 不做。它不剔除页眉、页码和期刊页脚,所以引用的段落里可能混进这类
文字——只有在我们能实测的排版上才分得清它们和正文,靠猜会静默删掉真内容。它也不给
置信度:切出来了不等于切对了。

`--grep` 搜索的是 `--section` 选中的那部分,不给 `--section` 时就搜索全文,所以
"先试某一节,不行就退回全文"只需要改一个参数就行。命中的位置以字符偏移量报告,
相互重叠的窗口会合并而不是重复打印。零命中时退出码为 4。

`--max-length` 只限制输出长度,不会把命中本身裁掉:超出预算的片段是围绕命中裁剪的,
而不是从头截断,而且只统计仍然显示出来的命中。有内容被略去时,末尾那行会指名是哪个
参数造成的——`--max-length` 触顶时去调大 `--max-matches` 毫无作用——`--format json`
则把同样的答案放在 `truncated_by` 字段里。

### `sources` -- 列出数据源及能力

```
fastpaper sources [--capabilities]
```

### `completions` -- Shell 补全

```
fastpaper completions fish > ~/.config/fish/completions/fastpaper.fish
fastpaper completions zsh > ~/.zfunc/_fastpaper
fastpaper completions bash >> ~/.bashrc
```

## 环境变量

除特别标注外均为可选。18 个数据源中有 17 个无需任何配置即可使用。

| 变量 | 用途 |
|------|------|
| `FASTPAPER_DOWNLOAD_DIR` | 默认下载目录（未设置则为 `./papers`） |
| `FASTPAPER_MAX_DOWNLOAD_SIZE` | `download` 可接受的最大响应体，以及 `figures` 可接受的最大压缩包（未设置则为 `100MiB`）；`0` 表示不限制 |
| `FASTPAPER_EMAIL` | 发给 CrossRef、OpenAlex 和 NCBI 的联系邮箱。不设置就不发这个参数，三者都接受 |
| `SEMANTIC_SCHOLAR_API_KEY` | 提升 Semantic Scholar 频率限制 |
| `OPENALEX_API_KEY` | OpenAlex 自 2026-02 起按用量计费，设置后可用更大的免费额度 |
| `CORE_API_KEY` | 提升 CORE 频率限制 |
| `NCBI_API_KEY` | 提升 PubMed / PMC 频率限制 |
| `UNPAYWALL_EMAIL` | Unpaywall **必需**，且必须是真实邮箱 |

每个源还支持用 `FASTPAPER_<SOURCE>_URL` 覆盖它的 base URL——`FASTPAPER_ARXIV_URL`、
`FASTPAPER_PUBMED_URL` 等等——测试就是靠它指向本地 mock 服务器的。文件与 API 不在同一
主机的源另有一个文件主机的覆盖项：`FASTPAPER_ARXIV_PDF_URL`、`FASTPAPER_BIORXIV_DL_URL`、
`FASTPAPER_PMC_DL_URL`（指向 AWS Open Data 上的 PMC Cloud Service，不是文章页）。

有一个覆盖项不符合这个模式，因为它不是数据源：`FASTPAPER_IDCONV_URL` 指向 NCBI 的
ID 转换服务（默认 `https://pmc.ncbi.nlm.nih.gov`），`figures` 用它把 DOI 换成
PMC ID。只有 DOI 那条路会用到它——arXiv ID 和 PMC ID 都不碰。

## 退出码

| 代码 | 含义 |
|------|------|
| `0` | 成功 |
| `1` | 一般错误（参数被拒、解析失败、网络失败） |
| `2` | 命令行解析层面的用法错误：未知参数、取值非法，或缺少配套参数（`--context` 没配 `--grep`） |
| `4` | 没有东西可返回：论文不存在、`--grep` 零命中、`--section` 未命中、这个源没有这篇的 PDF、这篇没有插图文件 |

只有这四个。凡是调用方能据以分支的都在表里——频率限制、403、`--max-size` 拒绝，
全都以 `1` 返回，靠错误信息而不是退出码区分。

值得分支的是 `4`：它表示请求本身没毛病，只是这个源没有，下一步该换源而不是改写法。

带 `-o` 的命令在文件写完后会向 stderr 打一行回执——
`Saved: results.json (12 results)`——调用方不用回读文件就知道落地了什么。
`-q` 会抑制这行输出。零结果时也照样打印,因为这恰恰是最该被看见的情况。

`download` 同样会向 stderr 打一行 `Saved: papers/2301.08745.pdf (2.2 MiB)`，
`figures` 则打 `Saved: 3 figure files to papers/2511.11035 (382.2 KiB)`。
这两行 `-q` 都不会抑制:两条命令都不往标准输出写任何东西,而落盘路径也没法由参数
推出来——DOI 里的 `/` 会变成 `_`。

这两条命令在目标文件已存在时也都返回 `0` 而不是报错,只打印 `File already exists: ...`
——重复运行是安全的,`1` 留给真正出问题的情况。要覆盖已有文件请加 `--overwrite`。

## 参与贡献

欢迎贡献!请阅读 [CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md) 与 [docs/](docs/) 下的开发者文档。

## 致谢

本项目受 [paper-search-mcp](https://github.com/openags/paper-search-mcp)（一个支持多数据源学术论文搜索与下载的 MCP 服务器）启发，在此向其作者致以诚挚感谢。

## 许可证

[GPL-3.0](LICENSE)
