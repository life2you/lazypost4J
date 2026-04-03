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

### 4. 计算源码包 SHA256

```bash
curl -L https://github.com/life2you/lazypost4J/archive/refs/tags/v0.1.1.tar.gz -o /tmp/lazypost-v0.1.1.tar.gz
shasum -a 256 /tmp/lazypost-v0.1.1.tar.gz
```

### 5. 更新 tap 仓库 formula

编辑 `homebrew-lazypost4j/Formula/lazypost.rb`：

- 把 `url` 改成新的 tag
- 把 `sha256` 改成上一步输出

示例：

```ruby
url "https://github.com/life2you/lazypost4J/archive/refs/tags/v0.1.1.tar.gz"
sha256 "替换为新的 SHA256"
```

### 6. 提交并推送 tap 仓库

```bash
git -C /path/to/homebrew-lazypost4j add Formula/lazypost.rb README.md
git -C /path/to/homebrew-lazypost4j commit -m "Update lazypost to v0.1.1"
git -C /path/to/homebrew-lazypost4j push origin main
```

### 7. 本地验证

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

## 版本更新模板

把下面内容里的版本号整体替换后执行：

```bash
VERSION=v0.1.1
curl -L "https://github.com/life2you/lazypost4J/archive/refs/tags/${VERSION}.tar.gz" -o "/tmp/lazypost-${VERSION}.tar.gz"
shasum -a 256 "/tmp/lazypost-${VERSION}.tar.gz"
```
