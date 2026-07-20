# 发布与分发

> English: [release.md](release.md)

发版由 maintainer 执行。Contributor 不修改 `Cargo.toml` 的 `version`、不推 tag——本页的目的是让你了解 PR 合并之后会发生什么。

## 分发

打包由 [cargo-dist](https://opensource.axo.dev/cargo-dist/) 驱动(`dist-workspace.toml` 固定在 0.31.0):

- **安装器**:shell 脚本、PowerShell 脚本,以及推送到 tap `zhangyee/homebrew-tap` 的 Homebrew formula。
- **目标平台**(5 个):`aarch64-apple-darwin`、`x86_64-apple-darwin`、`aarch64-unknown-linux-gnu`、`x86_64-unknown-linux-gnu`、`x86_64-pc-windows-msvc`。
- 安装器把二进制放进 `CARGO_HOME`。

## 发版流程

1. maintainer 修改 `Cargo.toml` 的 `version` 并推送版本形状的 git tag(如 `v0.2.0`)。
2. tag 触发 `.github/workflows/release.yml`(由 cargo-dist 生成),为所有目标平台构建产物并创建 GitHub Release。
3. 随后运行发布任务:更新 Homebrew formula,以及 `.github/workflows/publish-crates.yml`(经 `workflow_call` 调用),后者使用 `CARGO_REGISTRY_TOKEN` secret 执行 `cargo publish --locked`。

## 版本策略

SemVer,由 maintainer 在发版时根据上个 tag 以来的 commits 判断:

- 新增数据源或新的用户可见能力 → **minor**
- bug 修复、文档、内部改动 → **patch**
- CLI 参数或 JSON 输出 schema 的破坏性变更 → **major**
