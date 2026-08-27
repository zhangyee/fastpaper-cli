---
name: fastpaper
description: 当用户要查找学术论文或专利、做文献调研、按 DOI / arXiv id / PMID / PMC id 查某篇论文、追踪谁引用了谁、获取论文 PDF、抓取论文原始图片文件,或阅读论文时使用。覆盖 18 个来源 —— arXiv、PubMed、PMC、Europe PMC、bioRxiv、medRxiv、Semantic Scholar、OpenAlex、Crossref、DBLP、CORE、OpenAIRE、DOAJ、HAL、Zenodo、Unpaywall、Google Scholar、Baidu Xueshu。
---

# fastpaper

一个已经预装好的 CLI,用于学术检索、下载和阅读。用 `which fastpaper` 确认一次
即可;它**不是** Python 包,永远不要 `pip install` 它。

每次调用都带上 `--format json`。输出是 `{"source": "...", "results": [...]}`
—— 论文在 `results` 下面。退出码:`0` 成功,`2` 命令写错了,`4` **没有东西可
返回** —— 没有这篇论文、`--grep` 没匹配上、这个来源没有这篇论文的 PDF、这篇
论文没有图片文件 —— 其余情况一律 `1`。要对 `4` 单独分支:它意味着请求本身没
问题,只是这个来源确实没有,所以应该换个来源重试,而不是换个说法。来源没有
提供的字段一律是 `null`,意思是**未知**;要如实报告为未知,不要自己把它填上。

`null` 覆盖了两种相反的情况,而记录本身分辨不出是哪一种:这个来源*根本*就不
提供该字段,或者它确实带这个字段、只是这篇论文没有值。`fastpaper sources` 能把
两者分开 —— 它的 `pdf_url` / `open_access` / `citations` 三列说明每个来源能填
哪些字段。来自标 `✗` 的来源的 `null` 意味着**换个来源问**;来自标 `✓` 的来源
则意味着答案确实是未知。先查这一列,比发起一次注定不会成功的下载要便宜。

## Commands

```bash
# 搜索单个来源
fastpaper search <source> "<query>" -n 20 --format json

# 取单篇论文的元数据;来源由标识符的形态推断
fastpaper get <DOI|arXiv_ID|PMID|PMC_ID>
fastpaper get <source> <id>                 # 或者直接指名来源

# 保存 PDF(默认 ./papers/<id>.pdf)
fastpaper download <id> [-d <dir>] [--overwrite]

# 保存作者的原始图片文件(默认 ./papers/<id>/)
fastpaper figures <id> -d papers/ [--overwrite]

# 引用边 —— 谁引用了它,或者它引用了谁
fastpaper cite <id> [--direction incoming|outgoing] [-n 20]

# 从磁盘上已有的 PDF 里抽取文本
fastpaper read papers/<id>.pdf --list-sections        # 有哪些章节
fastpaper read papers/<id>.pdf [--section methods] [--max-length 4000]

fastpaper sources --capabilities            # 每个来源支持什么,实时查
```

`get` 按裸标识符的形态路由:arXiv→`arxiv`,PMC→`pmc`,PMID→`pubmed`,
DOI→`crossref`,`S2:`→`semantic`。

`download` 也按裸标识符的形态路由:arXiv→`arxiv`,PMC→`pmc`,DOI→`semantic`;
PMID 和 URL 会被直接拒绝。指名来源 —— `fastpaper download europepmc <id>`
—— 可以覆盖这套路由。

**两者对 DOI 的处理故意不一致**:`get` 把 DOI 送给 `crossref`,元数据最权威的
那个注册机构;`download` 把它送给 `semantic`,后者能解析出开放获取的副本 ——
crossref 不提供任何文件。对你的影响是:`get <DOI>` 给出的是一条 crossref 记录,
它的 `pdf_url` 永远是 `null`;而 `download <DOI>` 用的是一条你从没见过的
semantic 记录里的链接。**不要拿 `get <DOI>` 的 `pdf_url` 去预判 `download`
能不能成功。** 想看 download 会用哪个链接,就直接问那个来源:
`fastpaper get semantic DOI:<doi>`。

`cite` 把裸 DOI 路由到 `openalex`(不需要 key),把 arXiv 或 `S2:` 标识符路由到
`semantic`。这两个是这里仅有的带引用边的来源。

`figures` 抓的是作者的**原始**图片文件 —— arXiv 的源码包,或者 Europe PMC 的
补充材料包 —— 它不从 PDF 里抽图,不渲染任何东西,也不解析图号。只有 `arxiv` 和
`europepmc` 能提供;PMC ID 和 DOI 都路由到 `europepmc`(DOI 会先解析成 PMC
ID)。指名其他任何来源都是退出 `1`;在支持的来源上论文没有图片文件是退出 `4`。
**文件名完全按归档里的样子保留,所以它们和图号并不对应** —— 对
`2511.11035` 来说,文件 `3.pdf` 其实是 Figure 1;不要以为 `1.pdf` 就是 Figure 1。
在一个 39 篇论文的语料上实测,2026-08-20 端到端验证过:31 篇(79%)拿到了图片
文件,所以这里的 `4` 属于正常,不是哪里坏了的信号。

`read` 的章节:abstract、introduction、methods、results、discussion、
conclusion、references、full。

**PDF 本身并不记录章节结构** —— `--section` 是从排版推断出来的,所以在不常见
的版式上可能找不到某个章节。它会退出 4 而不是返回点别的东西,但**引用之前先
核对**:先跑 `fastpaper read <pdf> --list-sections`,只要列表里出现过的章节。
在 `--format json` 下,读章节会报告这一段是从哪个标题开始切的
(`heading.text`、`heading.offset`),这就是你用来核对引文的东西。

**当列表里没有 `abstract` 或 `introduction` 时**,通常是论文的问题,不是工具的
问题:Nature 及其同门刊物就是把摘要印在那儿、上面根本不加标题。那就改读开头
—— 那就是你本来想要的那几千个字符:

```
fastpaper read papers/<id>.pdf --max-length 3000
```

`--section` 的意义在于把整篇论文挡在你的上下文之外,而一篇论文的开头就是它的
摘要加上引言的起始部分。退回到这一招,代价和读那个章节是一样的。

有两个限制要提前想好。页眉、页码和期刊页脚会留在文本里,可能正好落在被引句子
的中间 —— 要引用的话自己把它们剔掉。另外,"章节找到了"不等于"章节找对了":
如果某段话很关键,用 `--section full --grep '<phrase>'` 确认一遍。

**搜索过滤参数** —— 每一个都会按来源做校验。要一个某来源不支持的参数会直接
报错,错误里会写明它*确实*支持哪些,**以及哪些来源有你要的这个参数**,所以
过滤参数绝不会被悄悄丢掉,重试的写法也已经替你写出来了。这条消息在 stderr 上
—— 绝对不要带着 `2>/dev/null` 跑这些命令,否则你拿回来的就只剩一个退出码:

| 参数 | 含义 |
|---|---|
| `-n <N>` | 最多返回多少条(各来源上限不同;zenodo 是 25,xueshu 约 10) |
| `--offset <N>` | 跳过 N 条;若干来源要求它是 `-n` 的整数倍 |
| `--sort relevance\|date\|citations` + `--order asc\|desc` | 排序 |
| `--year <YYYY>` · `--after <YYYY-MM-DD>` · `--before <YYYY-MM-DD>` | 日期 |
| `--author "<name>"` | 作者 —— `semantic`、`dblp`、`scholar`、`xueshu`、`biorxiv`、`medrxiv` 上*没有*这个参数;在这些来源上把人名写进 query |
| `--field <code>` | 学科/分类 —— 取的是*各来源自己的代码*,见下文 |
| `--open-access` | 只要开放获取的 |
| `--patents` | **只要专利**(europepmc、xueshu) |
| `-o <file>` | 写进文件而不是 stdout |

**环境变量。** unpaywall 必须要 `UNPAYWALL_EMAIL`。`SEMANTIC_SCHOLAR_API_KEY`
和 `CORE_API_KEY` 能把这两个源从限流档位里拉出来。`FASTPAPER_EMAIL` 让你加入
crossref/openalex 的礼貌池。还有 `NCBI_API_KEY`、`OPENALEX_API_KEY`。

## Examples

```bash
# arXiv 会改写 query 字符串,所以分类和日期要走 flag
fastpaper search arxiv "attention mechanism" --field cs.CL --after 2023-01-01 -n 20 --format json

# PubMed 原样接收 Entrez 语法 —— 文献类型和 MeSH
fastpaper search pubmed 'CRISPR AND systematic review[pt] AND humans[mh]' -n 20 --format json

# Europe PMC:先按引用数设阈值,再按它排序
fastpaper search europepmc 'CRISPR AND CITED:[500 TO *]' --sort citations -n 20 --format json

# 只要专利
fastpaper search europepmc "gene editing" --patents -n 10 --format json

# 中文文献 —— xueshu 只读一页,所以 ~10 就是它的天花板
fastpaper search xueshu "深度学习 医学影像" -n 8 --format json

# 单篇论文:元数据、一个 OA 链接、文件、正文
fastpaper get 10.1038/nature12373 --format json
fastpaper get unpaywall 10.1038/nature12373 --format json     # → .pdf_url
fastpaper download 1706.03762 -d papers/
fastpaper read papers/1706.03762.pdf --section methods --max-length 4000

# 引用边
fastpaper cite 10.1038/nature12373 --direction incoming -n 30 --format json
```

## Choosing a source

两个互相独立的维度。**学科覆盖**决定东西能不能被找到;**能力**决定一条命中能
不能被排序、解析或抓取。能力几乎都集中在跨学科来源手里 —— 几乎每条命中都带
`citations`(`semantic`、`openalex`)、最精准的 title→DOI 查找(`crossref`)、
OA 全文命中率最高(`core`)—— 所以它们不是排在专门来源后面的备胎,它们回答的
是另一个问题。下面有好几个领域根本没有专门来源,它们是唯一的选择。

| 领域 | 专门来源 | 能把它服务好的跨学科来源 |
|---|---|---|
| CS · AI · ML | `arxiv`(`cs.*`)、`dblp` | `semantic`(三者里 CS 覆盖最好,还带引用数)、`openalex` |
| 数学 · 物理 · 统计 · 量化生物/金融/经济 | `arxiv`(`math.*` `physics.*` `stat.*` `q-bio.*` `q-fin.*` `econ.*` `eess.*`) | `openalex` `core` `hal` |
| 生物医学 · 临床 | `pubmed` `pmc` `europepmc` `biorxiv` `medrxiv` | `semantic` `openalex` |
| 化学 · 材料 · 工程 | — | `openalex` `semantic` `crossref` `core` |
| 地球 · 环境 · 农业 | `europepmc 'SRC:AGR'`(Agricola) | `openalex` `core` `doaj` |
| 人文 · 社会科学 | — | `openalex` `core` `doaj` `hal`(法语/欧洲研究上很强) |
| 中文文献 | `xueshu` | `europepmc 'SRC:CBA'`(中国生物医学文摘) |
| 专利 | `europepmc --patents`、`xueshu --patents` | — |

`PDF` = `fastpaper download` 在这个来源上能用。`—` 并不表示全文完全拿不到:
`scholar`、`openalex`、`doaj` 和 `xueshu` 在部分命中上都会返回一个 `pdf_url`,
你可以自己去抓。它表示的是 CLI 不会替你抓。

`Cites` = `citations` 会不会有值回来。`✓` 几乎每条命中都有 · `~` 只在某些条件下
有 · `0` 字段在但大多数记录上是 0 · `—` 从来没有。

即使是标 `✓` 的 PDF 来源,某一条记录上也可能根本没有文件 —— `pdf_url` 是
`null`,下载以 "No PDF URL found" 失败,**退出 4**。这属于正常,不是故障。

**`403` 是另一回事,值得读一读,而不是重试。** 消息里会写明主机名并打印完整
URL:

```
Error: 403 from www.mdpi.com
https://www.mdpi.com/1424-8220/21/16/5542/pdf?version=1629270899
The server refused this request. fastpaper cannot tell a paywall from a bot
block here -- they look the same from outside. ...
```

碰到这种情况**不要**接着去轮别的来源。实测:那篇 MDPI 论文完全开放获取,照样
403;一篇 AHA 的订阅论文 403 得一模一样;而 Semantic Scholar 对这两篇都报
`open_access: true` —— 所以这个状态码根本说明不了有没有免费副本存在。而且这些
替代路径并不互相独立:unpaywall 把那个 DOI 解析到的是*同一个*出版商 URL,一个
字节都不差,`download europepmc <DOI>` 也落在同一个地方。把 URL 报告给用户就好;
退出码是 `1`,不是 `4`。

| 来源 | PDF | Figures | Cites | 用它做什么 | 注意 |
|---|:--:|:-----:|:--:|---|---|
| `arxiv` | ✓ | ✓ | — | **CS、AI、数学、物理、统计、q-bio、q-fin、经济**预印本,每一篇都免费可读 | query 语法会被改写 —— 见下文。每条命中都有 `pdf_url`,但只有大约一半带 DOI |
| `pubmed` | — | — | — | **生物医学**,35M+ 条记录 —— 临床和生命科学工作的基准索引 | 只有摘要,没有 PDF 也没有期刊名;要这两样就转到 `pmc` 或 `europepmc` |
| `pmc` | ✓ | — | — | **生物医学全文**(NLM)—— pubmed 所索引内容里的 OA 子集 | PDF 只来自 OA 子集,所以一条 pubmed 命中可能根本没有对应的 pmc 记录 |
| `europepmc` | ✓ | ✓ | ~ | **生物医学里覆盖最广的** —— 45M+ 摘要、9M+ 全文,外加 EPO 专利、NICE 指南、Agricola、预印本和中国生物医学文摘 | query 语法在这里最丰富,而且它是唯一能按引用数设阈值的来源(`CITED:[N TO *]`)。按相关度排出来的命中大多没被引用过,想看影响力就显式排序或设阈值 |
| `biorxiv` | ✓ | — | — | **生命科学预印本**(CSHL),每一篇都有全文 | **没有关键词搜索 API** —— 它是浏览一个日期窗口然后在本地匹配,所以 `--after`/`--before` 决定了到底在搜什么 |
| `medrxiv` | — | — | — | **医学 / 健康预印本**(CSHL) | 和 biorxiv 一样是日期窗口搜索,而且它的 PDF 被拦(403)—— 拿 DOI 去别处 |
| `semantic` | ✓ | — | ✓ | **跨学科**,而且引用数在这里最靠得住 —— 任何按影响力排序的基础 | 没有 `SEMANTIC_SCHOLAR_API_KEY` 会被限流得很厉害。大多数命中带 DOI,大约一半带 PDF |
| `openalex` | — | — | ✓ | **跨学科**,200M+ 作品 | `--field` 取的是概念 ID(`C154945302`),不是名字。很少带 PDF 链接 —— 适合找和排序,不适合抓文件。标题查找时相关度会飘 |
| `crossref` | — | — | ✓ | **跨学科** DOI 注册库 —— 这里最好的 title→DOI 查找 | 只有注册的元数据:没有 PDF,没有 OA 状态,带摘要的命中也很少。用它来解析,然后去别处取内容 |
| `dblp` | — | — | — | **计算机科学**书目 —— 会议和期刊记录,人工整理,很干净 | 完全没有摘要;只有元数据。它的 API 只接受一个 query 和翻页,别的都不接受 |
| `core` | ✓ | — | 0 | **跨学科** OA 聚合库,来自仓储和期刊的 400M+ 条 —— 几乎每条命中都有 PDF | 带 DOI 的命中很少,也没有期刊名。`CORE_API_KEY` 能放宽速率限制 |
| `openaire` | — | — | ✓ | **跨学科**欧盟开放科学图谱 | `download` 在这里用不了,但在少数记录上确实有出版商文件链接时会返回 `pdf_url` —— 它的链接大多是 DOI 解析器,不会被当作 PDF 提供。`get` 要的是 OpenAIRE id,不是 DOI |
| `doaj` | — | — | — | **跨学科**同行评议 OA 期刊 —— 元数据完整,几乎每条命中都有期刊名和摘要 | **它的 `pdf_url` 是落地页,不是文件** —— 去抓会拿回 HTML。日期只到年份粒度,不能排序 |
| `hal` | ✓ | — | — | **跨学科**法国国家档案库,几乎每条命中都有摘要,大多数有全文 | `--field` 取的是学科代码:`math` `phys` `chim` `sdv` `shs` `spi` `info` `sde`。没有期刊名,而且有些记录只有元数据 —— 下载前先按 `pdf_url` 过滤 |
| `zenodo` | ✓ | — | — | **跨学科** CERN 通用仓储 —— 论文、数据集和软件,几乎每条命中都带 DOI | `-n` 上限 25。没有期刊名,而且有些记录只有元数据 |
| `unpaywall` | — | — | — | **DOI → 一个可下载的 URL。**没有搜索,不分学科 —— 一个解析器 | 需要 `UNPAYWALL_EMAIL`;一次一个 DOI,而且那个 URL 得另外去抓 |
| `scholar` | — | — | — | 这里覆盖面最广的 —— 它能翻出那些被索引来源漏掉的工作,以及别处都没有时的一个 PDF 链接 | HTML 抓取,容易撞验证码;`doi` 永远是 `null`,只有部分命中带 `pdf_url`。`[CITATION]` 这类存根根本没有链接,会被丢掉 |
| `xueshu` | — | — | 0 | **中文文献** —— 期刊、硕博学位论文、会议论文、专利、标准;700M+ 条记录,覆盖 500+ 个学科。它也索引英文工作,但那些别的来源做得更好 | 非官方端点,有机器人检测;只有搜索,没有 `get`。**一次搜索一个请求,只有第一页** —— 一页 10 条记录,所以 `-n` 超过 ~10 会悄悄返回更少,`--offset` 翻过这一页就什么都没有了。`pdf_url` 很少见,`doi` 大约一半的记录有,不过是经过校验的,所以专利号或裸 URL 绝不会冒充成 DOI |

**两个会骗人的字段。** `core` 和 `xueshu` 会返回 `citations`,但在大多数记录上
留成 0,所以按它排序看起来像是生效了,实际上悄悄把被引多的论文埋了。另外
`arxiv` 只有大约一半的记录带 DOI,所以 DOI 不是一个跨来源稳定的键。

### Content types worth routing on

大多数内容类型的区分都不值得据此行动。这几个值得:

| 需求 | 怎么做 |
|---|---|
| **只要专利** | 在 `europepmc` 或 `xueshu` 上加 `--patents` |
| **中文文献** | `xueshu`;中文生物医学还可以用 `europepmc 'SRC:CBA'` |
| **预印本** | `arxiv`、`biorxiv`、`medrxiv`,或者 `europepmc 'SRC:PPR'` |
| **临床指南** | `europepmc 'SRC:CTX'`(NICE) |

`--patents` 的意思是只要专利:加了这个 flag 就只有专利,不加就一篇专利都没有,
绝不会混着来。`europepmc` 是原生做的收窄;`xueshu` 是在它那单独一页上本地切分,
所以两种模式都只返回那一页里属于自己的那部分,比单看 ~10 这个天花板所能解释的
更容易凑不满 `-n`。其他来源都不接受这个 flag —— Google Scholar 自己的开关只能
把结果*放宽*到把专利混进来,不能收窄到只要专利,所以 CLI 干脆不开它。

## Query syntax differs per source

这是收益最高的一件事,而且做错了不会有任何提示。

**`arxiv` 会改写你的 query。**每个词都会变成 `all:{word}`,所以把 `cat:cs.CL`
或 `ti:"..."` 写进 query 字符串没有任何用,只会返回不相干的结果,而且**不报任何
错**。改用 flag —— `--field`、`--author`、`--after`/`--before`。

`--field` 用的 arXiv 分类:`cs.CL` `cs.LG` `cs.CV` `cs.AI` `cs.RO`,
`math.*`,`physics.*`,`q-bio.*`,`q-fin.*`,`econ.*`,`stat.ML`,`eess.*`。

**`europepmc`、`pubmed`、`pmc`、`doaj`、`zenodo`、`hal`、`core` 和 `dblp` 会把你的
query 原样透传**,所以它们自己的字段语法是能用的:

- `pubmed` / `pmc`:`[pt]` 文献类型 · `[mh]` MeSH · `[tiab]` 标题/摘要 · `[au]` 作者 · `[dp]` 日期
- `europepmc`:`CITED:[N TO *]` · `AUTH:` · `PUB_YEAR:` · `OPEN_ACCESS:y` · `HAS_FT:y` · `LANG:` · `KW:` · `SRC:` 子集(`PPR` 预印本、`CTX` NICE 指南、`AGR` Agricola、`CBA` 中国生物医学文摘、`MED`、`PMC`)
- `doaj`:在 `bibjson.*` 上用 Lucene · `zenodo`:Elasticsearch · `hal`:Solr · `dblp`:`year:` `author:` `venue:`

**`crossref`、`openalex`、`semantic`、`scholar`、`xueshu` 只接受自由文本** ——
按相关度匹配,没有字段语法。要过滤就用 CLI 的 flag。

## Rate limits

每次 `fastpaper search` 都是一个进程打一个来源,所以某个来源挂了或者被限流,
拖下水的只有它自己。

| 档位 | 来源 | 限制 |
|---|---|---|
| **A —— 宽松** | `arxiv` `europepmc` `crossref` `openalex` `dblp` `doaj` `zenodo` `hal` `openaire` | 不需要认证;arXiv 希望请求之间隔 ~3s |
| **A —— 带 key** | `semantic`(`SEMANTIC_SCHOLAR_API_KEY`:1 req/s → 100)、`core`(`CORE_API_KEY`) | 不带 key 这两个都属于 C 档 —— 匿名的 `semantic` 已被观察到直接失败并报 "rate limited after 5 retries" |
| **B —— 共享额度** | `pubmed` + `pmc` | 两者共用一份 NCBI E-utilities 额度:3 req/s,带 `NCBI_API_KEY` 是 10 |
| **C —— 只能串行** | `xueshu` `scholar` | 有机器人检测和验证码;只容得下低频率的串行使用 |

## What this tool cannot do

1. **没有影响因子。**没有任何来源提供 JIF 或分区。
2. **引用边只来自 `semantic` 和 `openalex`**,经由 `cite`。你自己拼出来的任何
   东西(共同作者、刊物或会议、概念)都是代理指标 —— 报告的时候要讲明白。
3. **它抓不了任意 URL。**来自 `unpaywall`、`openalex`、`scholar` 或 `xueshu` 的
   `pdf_url` 必须用别的东西去下载。
4. **字段缺失意味着未知**,不是零,也不是不存在。
