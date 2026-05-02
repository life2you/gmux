[English](README.md) | [简体中文](README.zh-CN.md)

# gmux

`gmux` is a terminal Git workflow tool for multi-environment branch sync, batch merge, and GitLab merge request automation.

## What It Does

- Runs a fullscreen TUI for guided multi-step Git workflows
- Syncs a source branch across multiple environment branches
- Supports single-target and custom multi-target merge flows
- Creates GitLab merge requests from the terminal
- Shows preflight checks and action previews before executing risky operations
- Supports searchable selection menus and in-app help with `?`
- Lets you manage branch settings, GitLab connection info, and multiple project roots directly inside the TUI
- Uses a single global config file at `~/.config/gmux/gmux.toml`

## Project Layout

- `src/main.rs`: application entrypoint
- `src/config.rs`: config loading and initialization flow
- `src/tui/`: fullscreen TUI pages, menus, and interaction state
- `Cargo.toml`: Rust package manifest
- `scripts/update-homebrew-formula.sh`: Homebrew formula generation script
- `RELEASING.md`: maintainer release SOP

## Requirements

- Rust toolchain
- Git
- A GitLab token with the required API permissions
- A config file at `~/.config/gmux/gmux.toml`

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

## Config

`gmux` only reads and writes:

```text
~/.config/gmux/gmux.toml
```

Example:

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

## TUI Highlights

- Local workflows now include preview pages with branch existence, dirty working tree, detached HEAD, and ahead/behind checks before execution.
- GitLab MR workflows also show a preview before sending API requests.
- Search is available in the main selection flows. Press `/` to filter large project or branch lists.
- Press `?` on supported screens to see contextual usage help inside the app.
- `Config Management` lets you edit `project.root_dirs`, `gitlab.host`, `gitlab.token`, `merge_branch_middle`, `env_branches`, and `branch_map` with immediate auto-save.
- If multiple roots contain repositories with the same name, the project picker shows the source root to help you choose the right repo.

## Homebrew

This repository is prepared for publishing through a personal Homebrew tap.

Files related to Homebrew publishing:

- `packaging/homebrew-tap/Formula/gmux.rb`
- `packaging/homebrew-tap/README.md`
- `RELEASING.md`
- `scripts/update-homebrew-formula.sh`

Recommended release flow:

```bash
git tag -a v<version> -m "v<version>"
git push origin main
git push origin v<version>
./scripts/update-homebrew-formula.sh <version>
```

Then copy the generated formula into:

```text
life2you/homebrew-tap
```

After the tap repository is published, users can install with:

```bash
brew install life2you/tap/gmux
```

## Release Docs

- English: [`RELEASING.md`](RELEASING.md)
- 简体中文: [`RELEASING.zh-CN.md`](RELEASING.zh-CN.md)

## License

MIT
