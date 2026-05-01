# Homebrew Tap Scaffold

This directory is a scaffold for the `life2you/homebrew-tap` repository.

## Expected tap repo name

```text
homebrew-tap
```

## Expected formula path

```text
Formula/gmux.rb
```

## Install command after publishing

```bash
brew install life2you/tap/gmux
```

## How to publish

1. Create the GitHub repository `life2you/homebrew-tap`
2. Copy the contents of this directory into that repository root
3. Replace the formula SHA or regenerate it with:

```bash
./scripts/update-homebrew-formula.sh
```

4. Commit and push
