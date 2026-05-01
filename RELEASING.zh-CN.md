[English](RELEASING.md) | [简体中文](RELEASING.zh-CN.md)

# 发布 `gmux`

这份文档是发布 `gmux` 新版本并更新 Homebrew 的维护者 SOP。

## 前置条件

- 工作区是干净的
- `cargo build` 通过
- `cargo test` 通过
- `Cargo.toml` 中的版本号已经更新到目标发布版本
- 当前所在提交就是要打 tag 的精确提交

## 发布步骤

假设目标版本是 `<version>`。

1. 本地确认发布提交状态：

```bash
cargo build
cargo test
git status --short
```

2. 如果需要，创建并推送发布提交：

```bash
git add Cargo.toml Cargo.lock README.md README.zh-CN.md RELEASING.md RELEASING.zh-CN.md src packaging scripts
git commit -m "release: v<version>"
git push origin main
```

3. 给准确的发布提交打 tag：

```bash
git tag -a v<version> -m "v<version>"
git push origin v<version>
```

4. 重新生成仓库内打包好的 Homebrew formula：

```bash
./scripts/update-homebrew-formula.sh <version>
```

脚本会校验：

- 本地 tag 存在
- tag 对应源码中的版本与 `Cargo.toml` 一致
- 远端 tag 存在
- 远端 tag 与本地 tag 指向一致

5. 刷新当前仓库中提交的 formula 样板：

```bash
git add packaging/homebrew-tap/Formula/gmux.rb packaging/homebrew-tap/README.md packaging/homebrew-tap/README.zh-CN.md scripts/update-homebrew-formula.sh
git commit -m "chore: refresh packaged Homebrew formula"
git push origin main
```

6. 把 formula 复制到 tap 仓库：

```bash
cp packaging/homebrew-tap/Formula/gmux.rb ../homebrew-tap/Formula/gmux.rb
```

7. 发布 tap 仓库更新：

```bash
cd ../homebrew-tap
git add Formula/gmux.rb README.md README.zh-CN.md
git commit -m "Update gmux formula for v<version>"
git push origin main
```

8. 验证发布后的安装路径：

```bash
brew update
brew upgrade gmux
gmux --version
brew info gmux
```

## 注意事项

- 不要在推送 tag 之前生成 Homebrew formula。
- 不要把旧 tag 挪用到新的版本发布上。
- formula 使用 `std_cargo_args(path: ".")`，为了保证安装可复现，请始终提交并维护好 `Cargo.lock`。
