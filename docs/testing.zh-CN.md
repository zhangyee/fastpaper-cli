# 测试

> English: [testing.md](testing.md) — 与英文版保持同步。

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

## 惯例

- 每个测试只测一个行为;测试名描述该行为(`parse_empty_doi_becomes_none`)。
- 设置或删除环境变量的测试用 `serial_test` 的 `#[serial]`,避免测试间竞争。
- 新功能的测试必须随同一个 PR 提交——reviewer 一定会问。
