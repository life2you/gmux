# Releasing `gmux`

This document is the maintainer SOP for publishing a new `gmux` release and updating Homebrew.

## Preconditions

- Working tree is clean
- `cargo build` passes
- `cargo test` passes
- `Cargo.toml` version is already bumped to the target release version
- You are on the commit that should be tagged

## Release Steps

Assume the target version is `0.1.1`.

1. Verify the release commit locally:

```bash
cargo build
cargo test
git status --short
```

2. Create and push the release commit if needed:

```bash
git add Cargo.toml Cargo.lock README.md src
git commit -m "release: v0.1.1"
```

3. Tag the exact release commit:

```bash
git tag -a v0.1.1 -m "v0.1.1"
git push origin main
git push origin v0.1.1
```

4. Regenerate the packaged Homebrew formula:

```bash
./scripts/update-homebrew-formula.sh 0.1.1
```

The script validates:

- local tag exists
- tag source version matches `Cargo.toml`
- remote tag exists
- remote tag matches local tag

5. Refresh the scaffolded formula committed in this repo:

```bash
git add packaging/homebrew-tap/Formula/gmux.rb
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
git add Formula/gmux.rb
git commit -m "Update gmux formula for v0.1.1"
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
- The formula uses `cargo install --locked`; keep `Cargo.lock` committed and up to date.
