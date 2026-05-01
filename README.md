  English:

  | A terminal-based Git workflow tool for managing multi-environment branch sync, batch merge, and GitLab MR
  | automation
  | with an interactive TUI.

  中文:

  | 一个终端 Git 工作流工具，支持多环境分支同步、批量合并及 GitLab MR 自动化，带交互式 TUI 界面。

## Run

Development mode:

```bash
cargo run
```

Release build:

```bash
cargo build --release
./target/release/gmux
```

## Homebrew

This repository is prepared for publishing through a personal Homebrew tap.

Files related to Homebrew publishing:

- `packaging/homebrew-tap/Formula/gmux.rb`
- `packaging/homebrew-tap/README.md`
- `scripts/update-homebrew-formula.sh`

Recommended release flow:

```bash
git tag v0.1.0
git push origin v0.1.0
./scripts/update-homebrew-formula.sh
```

Then copy `packaging/homebrew-tap/*` into the repository:

```text
life2you/homebrew-tap
```

After the tap repository is published, users can install with:

```bash
brew install life2you/tap/gmux
```
