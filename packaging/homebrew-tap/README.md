[English](README.md) | [简体中文](README.zh-CN.md)

# Homebrew Tap Scaffold

This directory is a scaffold for the `life2you/homebrew-tap` repository.

## Expected Tap Repo Name

```text
homebrew-tap
```

## Expected Formula Path

```text
Formula/gmux.rb
```

## Install Command After Publishing

```bash
brew install life2you/tap/gmux
```

## How to Publish

1. Create the GitHub repository `life2you/homebrew-tap`.
2. Copy the contents of this directory into that repository root.
3. Regenerate the formula with:

```bash
./scripts/update-homebrew-formula.sh <version>
```

4. Commit and push the tap repository update.
