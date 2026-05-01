[English](README.md) | [简体中文](README.zh-CN.md)

# gmux

`gmux` is a terminal Git workflow tool for multi-environment branch sync, batch merge, and GitLab merge request automation.

## What It Does

- Runs a fullscreen TUI for guided multi-step Git workflows
- Syncs a source branch across multiple environment branches
- Supports single-target and custom multi-target merge flows
- Creates GitLab merge requests from the terminal
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
gitlab_url = "https://gitlab.example.com"
gitlab_token = "glpat-xxxx"
group_name = "my-group"

[[projects]]
name = "repo-a"
path = "/Users/you/code/repo-a"
development = "development"
test = "test"
pre_release = "pre-release"
main = "main"
```

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
