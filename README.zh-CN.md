[English](README.md) | [简体中文](README.zh-CN.md)

# gmux

`gmux` 是一个终端 Git 工作流工具，支持多环境分支同步、批量合并和 GitLab Merge Request 自动化。

## 功能说明

- 提供全屏 TUI，引导多步骤 Git 工作流
- 将源分支同步到多个环境分支
- 支持单目标合并和自定义多目标合并
- 可以直接在终端里创建 GitLab Merge Request
- 在执行高风险操作前展示执行前检查和操作预览
- 支持可搜索菜单，以及按 `?` 查看页内帮助
- 可以直接在 TUI 中管理分支配置、GitLab 连接信息和多个项目根目录
- 仅使用一个全局配置文件：`~/.config/gmux/gmux.toml`

## 项目结构

- `src/main.rs`：程序入口
- `src/config.rs`：配置加载与初始化流程
- `src/tui/`：全屏 TUI 页面、菜单与交互状态
- `Cargo.toml`：Rust 包清单
- `scripts/update-homebrew-formula.sh`：Homebrew formula 生成脚本
- `RELEASING.md`：维护者发版 SOP

## 运行要求

- Rust toolchain
- Git
- 具备所需 API 权限的 GitLab token
- 位于 `~/.config/gmux/gmux.toml` 的配置文件

## 运行方式

开发模式：

```bash
cargo run
```

发布构建：

```bash
cargo build --release
./target/release/gmux
```

## 配置

`gmux` 只会读取和写入：

```text
~/.config/gmux/gmux.toml
```

示例：

```toml
[gitlab]
host = "gitlab.example.com:8099"
token = "glpat-xxxx"

[project]
root_dirs = ["/Users/you/code", "/Users/you/client-work"]
merge_branch_middle = "henry"
env_branches = ["dev", "test", "uat", "stage", "prod"]

[branch_map]
"dev_henry_meger" = "dev"
"test_henry_meger" = "test"
"uat_henry_meger" = "uat"
"stage_henry_meger" = "stage"
"prod_henry_meger" = "prod"
```

## TUI 亮点

- 本地工作流在真正执行前会先显示预览页，包含分支存在性、工作区脏状态、detached HEAD、ahead/behind 等检查。
- GitLab MR 工作流也会在真正发 API 请求前展示预览。
- 主要选择流程都支持搜索，按 `/` 可以过滤项目或分支列表。
- 在支持的页面按 `?` 可以直接查看当前页的使用说明。
- `配置管理` 支持直接修改 `project.root_dirs`、`gitlab.host`、`gitlab.token`、`merge_branch_middle`、`env_branches` 和 `branch_map`，改动会立即自动保存。
- 如果多个项目根目录下存在同名仓库，项目选择页会显示来源目录，帮助你选对仓库。

## Homebrew

这个仓库已经为通过个人 Homebrew tap 发布做好准备。

与 Homebrew 发布相关的文件：

- `packaging/homebrew-tap/Formula/gmux.rb`
- `packaging/homebrew-tap/README.md`
- `RELEASING.md`
- `scripts/update-homebrew-formula.sh`

推荐发版流程：

```bash
git tag -a v<version> -m "v<version>"
git push origin main
git push origin v<version>
./scripts/update-homebrew-formula.sh <version>
```

然后把生成的 formula 复制到：

```text
life2you/homebrew-tap
```

发布 tap 仓库后，用户可以这样安装：

```bash
brew install life2you/tap/gmux
```

## 发版文档

- English: [`RELEASING.md`](RELEASING.md)
- 简体中文: [`RELEASING.zh-CN.md`](RELEASING.zh-CN.md)

## License

MIT
