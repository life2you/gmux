[English](RELEASING.md) | [简体中文](RELEASING.zh-CN.md)

# Releasing `gmux`

This document is the maintainer SOP for publishing a new `gmux` release and updating Homebrew.

## Preconditions

- Working tree is clean
- `cargo build` passes
- `cargo test` passes
- `Cargo.toml` version is already bumped to the target release version
- You are on the exact commit that should be tagged

## Release Steps

Assume the target version is `<version>`.

1. Verify the release commit locally:

```bash
cargo build
cargo test
git status --short
```

2. Create and push the release commit if needed:

```bash
git add Cargo.toml Cargo.lock README.md README.zh-CN.md RELEASING.md RELEASING.zh-CN.md src packaging scripts
git commit -m "release: v<version>"
git push origin main
```

3. Tag the exact release commit:

```bash
git tag -a v<version> -m "v<version>"
git push origin v<version>
```

4. Regenerate the packaged Homebrew formula:

```bash
./scripts/update-homebrew-formula.sh <version>
```

The script validates:

- local tag exists
- tag source version matches `Cargo.toml`
- remote tag exists
- remote tag matches local tag

5. Refresh the scaffolded formula committed in this repo:

```bash
git add packaging/homebrew-tap/Formula/gmux.rb packaging/homebrew-tap/README.md packaging/homebrew-tap/README.zh-CN.md scripts/update-homebrew-formula.sh
git commit -m "chore: refresh packaged Homebrew formula"
git push origin main
```

6. Copy the formula into the tap repository:

```bash
cp packaging/homebrew-tap/Formula/gmux.rb ../homebrew-tap/Formula/gmux.rb
```

7. Publish the tap update:

```bash
cd ../homebrew-tap
git add Formula/gmux.rb README.md README.zh-CN.md
git commit -m "Update gmux formula for v<version>"
git push origin main
```

8. Verify the published install path:

```bash
brew update
brew upgrade gmux
gmux --version
brew info gmux
```

## Important Notes

- Do not generate the Homebrew formula before pushing the tag.
- Do not move or reuse an old tag for a new release.
- The formula uses `std_cargo_args(path: ".")`, so keep `Cargo.lock` committed and up to date for reproducible installs.
