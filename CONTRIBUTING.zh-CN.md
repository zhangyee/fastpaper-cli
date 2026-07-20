# 参与贡献 fastpaper-cli

> English: [CONTRIBUTING.md](CONTRIBUTING.md)

感谢你有兴趣参与贡献!本页是入口,详细的开发者文档在 [docs/](docs/) 目录下。

## 开发环境

只需要 stable Rust 工具链:

```bash
git clone https://github.com/zhangyee/fastpaper-cli
cd fastpaper-cli
cargo build
cargo test
```

不需要任何 API key 或配置——整个测试套件基于录制好的 fixture 离线运行。

## 项目结构

`fastpaper` 是一个同步、无状态的 CLI:clap 解析命令,`main.rs` 分发到某一个自包含的数据源模块,结果由输出模块渲染。模块地图、数据流和设计哲学见 [docs/architecture.zh-CN.md](docs/architecture.zh-CN.md)。

## 提交改动

- **测试先行。**每个 PR 都必须附带测试。解析逻辑用 fixture 测试,HTTP 行为用 mockito 测试。参见 [docs/testing.zh-CN.md](docs/testing.zh-CN.md)。
- **Conventional commits。**使用 `feat` / `fix` / `docs` / `chore` / `style` 加可选 scope,例如 `feat(xueshu): add fixture and basic search response parsing`。
- **PR 流程。**Fork → feature 分支 → 向 `main` 发 PR。PR 描述里写清动机和测试方式。

## 新增数据源

这是最常见的贡献类型。完整的分步指南(含实例)见 [docs/adding-a-source.zh-CN.md](docs/adding-a-source.zh-CN.md)。

## 测试

测试布局、fixture 脱敏规则、`#[ignore]` 真实 API 测试惯例见 [docs/testing.zh-CN.md](docs/testing.zh-CN.md)。

## 版本与发布

版本号和 tag 由 maintainer 管理。**PR 中不要修改 `Cargo.toml` 的 `version`。**发布的构建与分发机制见 [docs/release.zh-CN.md](docs/release.zh-CN.md)。

## 参考资料

每个数据源的 API 调研笔记(端点、响应结构、限频——中文撰写)索引见 [docs/sources/README.md](docs/sources/README.md)。
