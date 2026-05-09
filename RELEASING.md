[English](RELEASING.md) | [简体中文](RELEASING.zh-CN.md)

# Releasing `gmux`

This document is the maintainer SOP for publishing a new `gmux` release with prebuilt macOS binaries and updating Homebrew.

## Preconditions

- Working tree is clean
- `cargo check` passes
- `cargo test` passes
- `Cargo.toml` and `Cargo.lock` already contain the target version
- You are on the exact commit that should be tagged

## Release Steps

Assume the target version is `<version>`.

1. Verify the release commit locally:

```bash
cargo check
cargo test
git status --short
```

2. Commit and push the release changes if needed:

```bash
git add Cargo.toml Cargo.lock README.md README.zh-CN.md RELEASING.md RELEASING.zh-CN.md src .github packaging scripts
git commit -m "release: v<version>"
git push origin main
```

3. Create and push the release tag:

```bash
git tag -a v<version> -m "v<version>"
git push origin v<version>
```

4. Wait for the GitHub Actions `release` workflow to finish for that tag.

The workflow should publish these assets to the GitHub Release:

- `gmux-aarch64-apple-darwin.tar.gz`
- `gmux-x86_64-apple-darwin.tar.gz`

If the workflow did not run automatically, trigger it manually with tag `v<version>`.

5. Regenerate the packaged Homebrew formula:

```bash
./scripts/update-homebrew-formula.sh <version>
```

The script validates:

- local tag exists
- tag source version matches `Cargo.toml`
- remote tag exists
- remote tag matches local tag
- both prebuilt release assets are already downloadable

6. Commit the refreshed formula template in this repository:

```bash
git add packaging/homebrew-tap/Formula/gmux.rb scripts/update-homebrew-formula.sh .github/workflows/release.yml
git commit -m "chore: refresh packaged Homebrew formula"
git push origin main
```

7. Copy the formula into the tap repository:

```bash
cp packaging/homebrew-tap/Formula/gmux.rb ../homebrew-tap/Formula/gmux.rb
```

8. Publish the tap update:

```bash
cd ../homebrew-tap
git add Formula/gmux.rb README.md README.zh-CN.md
git commit -m "Update gmux formula for v<version>"
git push origin main
```

9. Verify the published install path:

```bash
brew update
brew upgrade gmux
gmux --version
brew info life2you/tap/gmux
```

## Important Notes

- Do not update the tap formula before the release assets exist.
- Do not move or reuse an old tag for a new release.
- The Homebrew formula now installs prebuilt binaries, so users no longer need Rust on their machines.
