---
name: sync-docs
description: 审核本仓库文档的漂移时使用——英文与中文版本不一致,或文档与代码脱节(新增数据源、改动 CLI、修改能力、或发版前)。触发词:文档审核、同步文档、中英文不一致、check the docs、sync docs。
---

# sync-docs

审核 fastpaper-cli 文档的两类漂移,然后**先给出方案、等用户批准,再动手改**。

- **中英文漂移**——某个 `.md` 与它的 `.zh-CN.md` 双语版在结构或内容上不一致。
- **代码漂移**——文档写的东西代码已经不这么做了(计数、能力、模块布局、端点、环境变量)。

核心原则:**行为以代码为准;散文以该对的基准语言版本为准**——文档是英文基准,`skills/fastpaper/` 是中文基准,见下面的文档清单。审核阶段绝不改文件。流程是:产出发现 → 提方案 → 得到明确同意 → 执行 → 提交。

## 文档清单

**双语对**(必须结构对齐——相同标题、相同小节顺序):
- `README.md` ⇄ `README.zh-CN.md`
- `CONTRIBUTING.md` ⇄ `CONTRIBUTING.zh-CN.md`
- `docs/{architecture,adding-a-source,testing,release}.md` ⇄ 对应的 `.zh-CN.md`
- `skills/fastpaper/SKILL.md` ⇄ `skills/fastpaper/SKILL.en.md`

**skill 这一对与上面三行有三处不同,都是有意的,不要"修正"它:**
- **后缀 token 不同**:上面的文档用 `.zh-CN`(本仓库自己的约定);skill 用 `.en`。下游 KyDog 的 locale 白名单只认 `zh` / `en`,写成 `.zh-CN` 它认不出来。别把这两套统一。
- **基准方向相反**:`README` 那几对是**英文基准、中文是译本**;skill 这一对是**中文基准、英文是译本**——无 locale 中缀的 `SKILL.md` 即默认语言,而默认语言是中文。同一份审核 skill 里并存两个方向,这是刻意的。
- **没有语言切换头**:下游按用户语言把整棵 `skills/fastpaper/` 投影成单语言树,变体文件不落盘,用户目录里只会有一个 `SKILL.md`。指向另一版的链接在那边必然是死链;何况 `SKILL.md` 开头是 YAML frontmatter,切换头也无处安放。

**有意单语**(**不要**当成"缺翻译"来标记):
- `docs/sources/00_base.md` … `18_xueshu.md`——中文调研笔记。
- `docs/sources/README.md`——仅英文索引。

## Pass A — 中英文一致性

对每一对:
1. **结构**:`grep -c '^## ' 两个文件`——标题数量必须相同,小节顺序必须对应。
2. **内容**:通读两版;一版里的每条事实、列表项、表格行、代码块,另一版都必须有。数字与标识符必须完全一致。
3. **语言切换头**:每个文件都链接到它的双语版(`> 中文版:…` / `> English:…`)。不要重新引入"保持同步"之类的散文——那是本 skill 的职责,不是文档的。**`skills/fastpaper/` 那一对例外,不加切换头**——理由见文档清单。

## Pass B — 代码一致性

对当前代码树跑下列检查,逐项报告不符:

| 文档中的声明 | 事实来源 | 如何检查 |
|---|---|---|
| 源计数("N academic sources"、表格行、SKILL ×2 个语言版本各 2 处、零配置计数) | `ls src/sources/*.rs \| grep -v mod.rs \| wc -l`(18) | README×2 + `SKILL.md` 与 `SKILL.en.md` 各自的 description 和正文计数必须一致 |
| 各源 search/get/download 能力 | `registry.rs` 里各 `SourceEntry` 的 `Capabilities`(**唯一事实来源**;`cli.rs::supports_*` 已删除) | 能力声明有 **3 处**,都要对齐代码:①README 能力表各列(中英)②SKILL 的 download-capable 汇总列表 ③SKILL 领域列表里**每个源条目末尾**的 `search / get / download` 标注(易漏,必查)。②③在 `SKILL.md` 和 `SKILL.en.md` 里各有一份,两份都要查。注意 `read` 已不是按源能力——它只读本地 PDF |
| 各源支持的 search 过滤参数 | `registry.rs` 各源的 `SearchCaps`,以及该源的 `build_*_url` | `docs/sources/<n>_<src>.md` 的「CLI 过滤参数映射」表逐行核对;声明为支持的参数必须真的落到了原生参数上 |
| 模块地图 / 数据流(`architecture.md`) | `src/` 布局、`sources::*::search` 签名 | 文件清单与函数名确实如所写存在 |
| 环境变量名(`FASTPAPER_*_URL`、各 key) | `grep -rn 'env::var' src/` | 文档里的每个变量都在代码里,反之亦然 |
| 各源端点/路径(`docs/sources/*.md`) | 该源的 `src/sources/<name>.rs` | 文档里的 URL/参数/字段映射与代码相符 |
| adding-a-source 的接线点 | `registry.rs` | 三个接线点仍如所述存在(2026-07 从 cli.rs + main.rs 的四点收敛而来) |

源列表还出现在 `docs/sources/README.md`(索引行)和各源的能力行——计数检查时一并纳入。

## 产出 → 批准 → 执行 → 提交

1. **发现**:一张表,最严重的在前。每行:file:line、漂移类型(`双语` / `代码`)、哪里错了、事实来源。
2. **提议方案**,逐条分类:
   - **翻译同步**——在双语版之间补齐缺失/更新的内容(以该对的基准语言版本为准,除非译本侧才是更新的事实)。
   - **照代码更新**——改写文档使其与当前代码相符。
   - 任何模糊之处(如疑似有意为之的措辞差异)交由用户裁决,不要擅自"修正"。
3. **等待明确批准。** 用户说开始之前不要改。若用户调整方案,重新复述一遍。
4. **只执行**已批准的条目。
5. **验证**:重跑 Pass A 结构检查 + `cargo test`(文档改动不应影响测试)+ 确认链接可解析。
6. **提交**,用 `docs: sync ...` 描述本次同步了什么。

## 常见错误

- **翻译调研笔记。** `docs/sources/*.md` 是有意的中文单语。只有它们的*事实 vs 代码*才重要,永远不用管它们没有英文版。
- **未批先改。** 本 skill 只提方案,由用户批准。批准前动手就违反了流程。
- **信文档不信代码。** 二者冲突时,代码是对的、文档是 bug(除非代码本身才是 bug——那就单独标出来,别把坏行为当成"预期"写进文档)。
- **只数 README 表格。** 源计数散落在约 5 处;漏改一处就还是漂移。
