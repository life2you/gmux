[English](README.md) | [简体中文](README.zh-CN.md)

# Homebrew Tap 脚手架

这个目录是 `life2you/homebrew-tap` 仓库的脚手架内容。

## 预期的 Tap 仓库名

```text
homebrew-tap
```

## 预期的 Formula 路径

```text
Formula/gmux.rb
```

## 发布后的安装命令

```bash
brew install life2you/tap/gmux
```

## 发布方式

1. 创建 GitHub 仓库 `life2you/homebrew-tap`。
2. 将当前目录内容复制到该仓库根目录。
3. 使用下面命令重新生成 formula：

```bash
./scripts/update-homebrew-formula.sh <version>
```

4. 提交并推送 tap 仓库更新。
