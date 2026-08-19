# 测试

> English: [testing.md](testing.md)

## 运行测试

```bash
cargo test                 # 全部(离线运行,无需任何 key)
cargo test --lib           # 仅单元测试
cargo test --test cli      # 仅 CLI 集成测试
```

布局:

- **单元测试**与代码同文件,在各源文件的 `#[cfg(test)]` 模块里(目前约 330 个)。解析器测试用 `include_str!` 加载录制好的 fixture。
- **集成测试**在 `tests/cli.rs`(目前约 57 个),用 `assert_cmd` 驱动真实二进制,对着本地 mockito server 跑。

## HTTP mock

使用 [mockito](https://docs.rs/mockito)(同步 API)。典型写法:

```rust
let mut server = mockito::Server::new();
let mock = server
    .mock("GET", mockito::Matcher::Regex("pn=10".to_string()))  // 路径+查询串正则
    .match_header("acs-token", "expected-value")                 // 断言请求头
    .with_status(200)
    .with_body(FIXTURE)
    .expect(1)                                                   // 精确调用次数
    .create();
// ... 调用 search(&server.url(), ...) ...
mock.assert();
```

`Matcher::Any` 匹配一切;`.expect(0)` + `.assert()` 可证明某端点**没有**被调用(分页提前停止的测试就靠它)。

## Fixture

- 位置:`tests/fixtures/<source>_search.json`(或 `.xml` / `.html`)。
- 必须是**脱敏后的真实响应**,不能手编 JSON:去掉带签名的 URL 和 `authorization=` 查询串,匿名化日志/请求 ID,删除任何个人信息。
- 保留真实的字段差异(空串、`null`、HTML 标签)——解析器容错测试正是靠这些喂养。

## 真实 API 测试

访问真实服务的测试标记 `#[ignore]`,和其他测试一起放在 `tests/` 下:

```bash
cargo test --test cli -- --ignored     # 手动执行
```

绝不进 CI。失败时,先判断是不是对方限频/风控所致(换个关键词、等一等、重试一次),再当作回归处理。

## 排版核验:对着页面看,不要对着文本看

`read` 是从排版推断论文结构的,而提取出来的文本并不携带这个判断所依据的证据。改动排版
相关逻辑时如果只看提取文本来核验,写下的规则很容易是错的:某篇论文里 `Introduction`
和它下面的子标题 `Background` 字号完全相同,提取文本里再无别的线索可以区分;而页面上
一个是蓝色正体,一个是黑色斜体。

所以排版相关的改动一律对着渲染出来的页面核验,而不是对着文本。渲染工具装一次即可:

```bash
brew install poppler imagemagick     # 或 apt-get install poppler-utils imagemagick
```

`tests/real_papers.rs` 里有两个诊断负责产出要看的东西。它们都标了 `#[ignore]`,并且都
需要一个仓库不携带的 PDF 目录:

```bash
FASTPAPER_PAPERS=/path/to/papers cargo test --test real_papers -- --ignored --nocapture
```

`report_heading_positions` 会打印每一个被读出来的标题,以及每一条长得像标题却没被读出
来的行,并带上各自所在的页码和基线——足够把那一条横带从页面上裁下来:

```bash
pdftoppm -png -r 130 -f "$PAGE" -l "$PAGE" -singlefile paper.pdf page
# PDF 的 y 从页脚往上算,图片的 y 从页顶往下算
TOP=$(awk -v h="$HEIGHT" -v y="$Y" 'BEGIN{print int((h-y-11)*130/72)}')
magick page.png -crop "x72+0+$TOP" +repage strip.png
```

一个标题一条横带,纵向拼起来就是一篇论文一张核验图。「标题读全了吗、读出来的每一条都
是真的吗」这两个问题,这样才是看一眼就能回答的。`report_detected_headings` 和
`report_unrecognised_heading_shapes` 以纯文本给出同样的信息——某家期刊的措辞变体是这么
被找出来的,而不是猜出来的。

这套做法要守住的两条规矩:

- **阈值写进代码时,必须就是量出来的那个值。** 曾把实测的 0.90 在代码里写成对称的
  0.07,结果位于页高 0.91 处的页码照样留在正文里;而同时写的测试用的是编造的坐标,
  自然也没抓住。
- **单一出版社的语料,证明不了「对 PDF 成立」。** 会删文本的规则必须是对的,而不是
  「通常是对的」;证据撑不住时,就别动那段文本,并把这一点写进文档。

## 惯例

- 每个测试只测一个行为;测试名描述该行为(`parse_empty_doi_becomes_none`)。
- 设置或删除环境变量的测试用 `serial_test` 的 `#[serial]`,避免测试间竞争。
- 新功能的测试必须随同一个 PR 提交。
