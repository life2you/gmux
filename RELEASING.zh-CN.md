[English](RELEASING.md) | [简体中文](RELEASING.zh-CN.md)

# 发布 `gmux`

这份文档是发布 `gmux` 新版本、上传预编译 macOS 二进制并更新 Homebrew 的维护者 SOP。

## 前置条件

- 工作区是干净的
- `cargo check` 通过
- `cargo test` 通过
- `Cargo.toml` 和 `Cargo.lock` 已经是目标版本
- 当前所在提交就是要打 tag 的精确提交

## 发布步骤

假设目标版本是 `<version>`。

1. 本地确认发布提交状态：

```bash
cargo check
cargo test
git status --short
```

2. 如果需要，提交并推送发布变更：

```bash
git add Cargo.toml Cargo.lock README.md README.zh-CN.md RELEASING.md RELEASING.zh-CN.md src .github packaging scripts
git commit -m "release: v<version>"
git push origin main
```

3. 给准确的发布提交打 tag 并推送：

```bash
git tag -a v<version> -m "v<version>"
git push origin v<version>
```

4. 等待 GitHub Actions 的 `release` workflow 跑完。

GitHub Release 中应该出现这两个资源文件：

- `gmux-aarch64-apple-darwin.tar.gz`
- `gmux-x86_64-apple-darwin.tar.gz`

如果 workflow 没有自动触发，就手动以 `v<version>` 为 tag 触发一次。

5. 重新生成仓库内打包好的 Homebrew formula：

```bash
./scripts/update-homebrew-formula.sh <version>
```

脚本会校验：

- 本地 tag 存在
- tag 对应源码中的版本与 `Cargo.toml` 一致
- 远端 tag 存在
- 远端 tag 与本地 tag 指向一致
- 两个预编译二进制资源都已经可以下载

6. 提交当前仓库中的 formula 样板更新：

```bash
git add packaging/homebrew-tap/Formula/gmux.rb scripts/update-homebrew-formula.sh .github/workflows/release.yml
git commit -m "chore: refresh packaged Homebrew formula"
git push origin main
```

7. 把 formula 复制到 tap 仓库：

```bash
cp packaging/homebrew-tap/Formula/gmux.rb ../homebrew-tap/Formula/gmux.rb
```

8. 发布 tap 仓库更新：

```bash
cd ../homebrew-tap
git add Formula/gmux.rb README.md README.zh-CN.md
git commit -m "Update gmux formula for v<version>"
git push origin main
```

9. 验证发布后的安装路径：

```bash
brew update
brew upgrade gmux
gmux --version
brew info life2you/tap/gmux
```

## 注意事项

- 不要在 release 资源生成之前更新 tap 公式。
- 不要把旧 tag 挪用到新的版本发布上。
- 现在的 Homebrew 安装走的是预编译二进制，终端用户机器上不需要再安装 Rust。
