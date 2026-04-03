# Homebrew Release Flow

这个文档用于维护 `lazypost` 的 Homebrew 发布。

相关仓库：

- 主项目仓库：`life2you/lazypost4J`
- tap 仓库：`life2you/homebrew-lazypost4j`

## 当前用户安装方式

```bash
brew tap life2you/lazypost4j
brew install life2you/lazypost4j/lazypost
```

## 发版步骤

### 1. 在主项目仓库完成版本变更

确认这些内容已经准备好：

- `Cargo.toml` 版本号已更新
- `cargo test` 通过
- `README.md` / 文档已同步

### 2. 提交并推送主项目

```bash
git add .
git commit -m "Release v0.1.1"
git push origin main
```

### 3. 打 tag

```bash
git tag -a v0.1.1 -m "v0.1.1"
git push origin v0.1.1
```

### 4. 更新 Homebrew formula

推荐直接使用仓库里的脚本：

```bash
scripts/update_homebrew_formula.sh v0.1.1
```

脚本会自动：

- 下载新 tag 的源码包
- 计算 `sha256`
- 更新 `homebrew-lazypost4j/Formula/lazypost.rb` 中的 `url` 和 `sha256`

如果 tap 仓库不在默认路径，可传环境变量：

```bash
TAP_REPO_PATH=/path/to/homebrew-lazypost4j scripts/update_homebrew_formula.sh v0.1.1
```

### 5. 提交并推送 tap 仓库

```bash
git -C /path/to/homebrew-lazypost4j add Formula/lazypost.rb
git -C /path/to/homebrew-lazypost4j commit -m "Update lazypost to v0.1.1"
git -C /path/to/homebrew-lazypost4j push origin main
```

### 6. 本地验证

```bash
brew update
brew reinstall life2you/lazypost4j/lazypost
brew test lazypost
brew info life2you/lazypost4j/lazypost
```

## 常用检查

确认 tap 已挂载：

```bash
brew tap | grep lazypost4j
```

查看 formula 来源：

```bash
brew info life2you/lazypost4j/lazypost
```

## 一键更新示例

标准目录结构下：

```bash
scripts/update_homebrew_formula.sh v0.1.1
```
