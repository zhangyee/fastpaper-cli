[English](README.md) | **中文**

# fastpaper CLI with Skill

一个为 AI 编程智能体（Claude Code、Codex、Gemini CLI 等）提供学术论文及科技文献快速检索、下载和阅读能力的 CLI 工具。配套 [SKILL](skills/fastpaper/SKILL.md) 文件，教会智能体按领域选择数据源并构造命令。

单条命令、单个数据源、零配置。多源并行检索由智能体启动多个进程实现。

## 安装

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

## 快速开始

```sh
# 搜索 arXiv
fastpaper search arxiv "transformer attention mechanism"

# 带过滤条件的搜索
fastpaper search arxiv "large language model" --after 2024-01-01 --field cs.CL --limit 20

# 通过 DOI 获取论文（自动检测数据源）
fastpaper get 10.1038/nature12373

# 通过 arXiv ID 获取
fastpaper get 2301.08745

# 下载 PDF
fastpaper download arxiv 2301.08745

# 阅读全文
fastpaper read arxiv 2301.08745

# 阅读特定章节
fastpaper read pmc PMC7318926 --section methods

# 阅读本地 PDF
fastpaper read local ./paper.pdf

# JSON 输出，供脚本 / AI 智能体使用
fastpaper search semantic "CRISPR gene editing" --format json

# 多源并行搜索
fastpaper search arxiv "protein folding" --format json &
fastpaper search pubmed "protein folding" --format json &
fastpaper search semantic "protein folding" --format json &
wait
```

## 数据源

17 个学术数据源，每条命令独立访问单个数据源。

| 数据源 | 全称 | search | download | read | 覆盖领域 |
|--------|------|:------:|:--------:|:----:|----------|
| `arxiv` | arXiv | yes | yes | yes | 物理、数学、计算机、统计、电子工程、量化生物/金融、经济学 |
| `biorxiv` | bioRxiv | yes | yes | yes | 生命科学 |
| `medrxiv` | medRxiv | yes | yes | yes | 医学 / 健康科学 |
| `pubmed` | PubMed | yes | | | 生物医学与生命科学（仅元数据） |
| `pmc` | PubMed Central | yes | yes | yes | 生物医学与生命科学（全文） |
| `europepmc` | Europe PMC | yes | | | PMC 的生命科学超集 |
| `scholar` | Google Scholar | yes | | | 全学科（实验性，有频率限制） |
| `semantic` | Semantic Scholar | yes | yes | yes | 全学科，AI 驱动的引用图谱 |
| `crossref` | CrossRef | yes | | | DOI 元数据，全学科 |
| `openalex` | OpenAlex | yes | | | 开放元数据索引，2 亿+ 作品 |
| `dblp` | DBLP | yes | | | 计算机科学 |
| `core` | CORE | yes | yes | yes | 开放获取聚合器 |
| `openaire` | OpenAIRE | yes | | | 欧盟开放科学 |
| `doaj` | DOAJ | yes | yes | yes | 开放获取期刊，全学科 |
| `unpaywall` | Unpaywall | yes | | | OA 链接解析（需设置 `UNPAYWALL_EMAIL`） |
| `zenodo` | Zenodo | yes | yes | yes | 全学科（数据集、软件、论文） |
| `hal` | HAL | yes | yes | yes | 多学科，法国国家开放存档 |

## 命令

### `search` -- 搜索论文

```
fastpaper search <SOURCE> <QUERY> [OPTIONS]

选项:
  -n, --limit <N>        最大结果数 [默认: 10]
      --offset <N>       跳过前 N 条结果 [默认: 0]
      --sort <FIELD>     排序: relevance, date, citations [默认: relevance]
      --author <NAME>    按作者过滤
      --after <DATE>     指定日期之后 (YYYY-MM-DD)
      --before <DATE>    指定日期之前 (YYYY-MM-DD)
      --year <YEAR>      指定年份
      --field <FIELD>    学科领域 / 分类 (如 cs.AI)
      --open-access      仅开放获取论文
  -f, --format <FMT>     table, json, jsonl, csv, bibtex [默认: table]
  -o, --output <PATH>    输出到文件
```

### `get` -- 通过标识符获取论文

根据标识符格式（DOI、arXiv ID、PMID、PMC ID、URL）自动检测数据源。

```
fastpaper get <IDENTIFIER> [OPTIONS]

选项:
      --resolve           查找所有可用的开放获取版本
      --with-citations    包含引用数和参考文献
      --with-abstract     包含摘要
```

### `download` -- 下载 PDF

```
fastpaper download <SOURCE> <IDENTIFIER> [OPTIONS]

选项:
  -d, --dir <PATH>       下载目录 [默认: ./papers]
      --filename <FMT>   文件名模板: {id}, {title}, {authors}, {year}, {doi}
      --overwrite        覆盖已有文件
      --source-files     下载 LaTeX 源码（仅 arXiv）
```

### `read` -- 阅读论文内容

```
fastpaper read <SOURCE> <IDENTIFIER> [OPTIONS]

选项:
      --section <SEC>    abstract, introduction, methods, results,
                         discussion, conclusion, references, full [默认: full]
      --metadata-only    仅显示元数据
      --raw              无格式纯文本
      --max-length <N>   截断输出到 N 个字符
  -o, --output <PATH>    输出到文件
```

### `sources` -- 列出数据源及能力

```
fastpaper sources [--check] [--capabilities]
```

### `completions` -- Shell 补全

```
fastpaper completions fish > ~/.config/fish/completions/fastpaper.fish
fastpaper completions zsh > ~/.zfunc/_fastpaper
fastpaper completions bash >> ~/.bashrc
```

## 环境变量

除特别标注外均为可选。20 个数据源中有 19 个无需任何配置即可使用。

| 变量 | 用途 |
|------|------|
| `FASTPAPER_DOWNLOAD_DIR` | 默认下载目录（未设置则为 `./papers`） |
| `FASTPAPER_EMAIL` | CrossRef / OpenAlex 礼貌池邮箱 |
| `SEMANTIC_SCHOLAR_API_KEY` | 提升 Semantic Scholar 频率限制 |
| `CORE_API_KEY` | 提升 CORE 频率限制 |
| `NCBI_API_KEY` | 提升 PubMed / PMC 频率限制 |
| `UNPAYWALL_EMAIL` | Unpaywall **必需** |

## 退出码

| 代码 | 含义 |
|------|------|
| `0` | 成功 |
| `1` | 一般错误（无效参数、解析失败） |
| `2` | 网络错误（连接超时、DNS 失败） |
| `3` | 数据源错误（API 报错、频率限制重试耗尽） |
| `4` | 未找到结果 |
| `5` | 权限错误（论文未开放获取、缺少必需环境变量） |

## 安装 Skill

安装 Skill 文件，让 AI 编程智能体学会使用 fastpaper。使用 [Vercel Skills](https://github.com/vercel-labs/skills)，一个将 SKILL.md 安装到各编程智能体（Claude Code、Codex、Cursor、Gemini CLI 等）的工具：

```sh
npx skills add zhangyee/fastpaper-cli
```

SKILL.md 教会智能体按研究领域选择数据源并构造命令。使用 `--format json` 获取结构化输出。所有 JSON 字段缺失时统一输出 `null`（不省略），保证 schema 稳定。

## 许可证

[GPL-3.0](LICENSE)
