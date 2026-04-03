# lazypost

`lazypost` 是一个面向 Spring MVC / Spring Boot 项目的终端 API 扫描与调试工具。

它直接静态扫描 `.java` 源码，提取 Controller、路由、参数和请求体草稿，然后在 TUI 里浏览接口、编辑请求并发送调试请求。不依赖运行中的 Java 应用，也不靠运行时反射拿接口元数据。

## 能力概览

- 扫描 Spring MVC / Spring Boot 控制器与接口路由
- 提取 Path / Query / Header / Cookie / RequestBody 信息
- 从 DTO 推断 JSON body 草稿
- 在 TUI 中按模块编辑 `Params / Headers / Body`
- 直接发送请求并查看响应
- 响应区支持折叠 JSON 视图和原始响应弹窗
- 按项目持久化最近项目、域名、全局头、接口请求草稿

## 安装

### Homebrew

标准 tap 方式会使用独立仓库 `life2you/homebrew-lazypost4j`。

仓库发布后，安装命令是：

```bash
brew tap life2you/lazypost4j
brew install life2you/lazypost4j/lazypost
```

### Cargo

在当前仓库本地安装：

```bash
cargo install --path .
```

## 使用

启动 TUI：

```bash
lazypost /path/to/java-project
```

只做扫描：

```bash
lazypost scan /path/to/java-project
```

以 JSON 输出扫描结果：

```bash
lazypost scan /path/to/java-project --json
```

如果不传项目路径，程序会先进入目录选择页。

## TUI 快捷键

- `1..7`：切换模块
- `e`：编辑当前模块，适用于 `2..6`
- `s`：发送当前接口请求
- `v`：在 `[7] 响应` 模块打开原始响应弹窗
- `f`：模糊搜索接口
- `?`：打开帮助
- `q`：退出

## 本地配置

配置文件保存在系统配置目录：

- macOS: `~/Library/Application Support/lazypost/config.json`
- Linux: `~/.config/lazypost/config.json`

当前会保存：

- 最近浏览项目
- 域名列表
- 全局请求头
- 每个项目下的接口请求草稿

## 相关文档

- 开发说明：[DEVELOPMENT.md](./DEVELOPMENT.md)
- 产品 / 设计草稿：[api_tui_v_1_prd_and_srd.md](./api_tui_v_1_prd_and_srd.md)

## License

本项目使用双许可证：

- Apache License 2.0，见 [LICENSE-APACHE](./LICENSE-APACHE)
- MIT，见 [LICENSE-MIT](./LICENSE-MIT)
