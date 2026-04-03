# API TUI（lazypost）开发文档

本文档描述 `lazypost` 当前代码实现的技术栈、仓库结构、运行方式与功能边界。产品目标与需求范围以 `api_tui_v_1_prd_and_srd.md` 为准；若两者冲突，以产品文档中带“当前实现 / 未实现 / 候选”标注的条目为准。

---

## 1. 技术栈

| 领域 | 选型 | 说明 |
|------|------|------|
| 语言 | Rust（Edition 2021，最低版本见 `Cargo.toml` 的 `rust-version`） | 单二进制分发；运行本工具不要求本机安装 Java |
| 终端 UI | [ratatui](https://github.com/ratatui/ratatui) + [crossterm](https://github.com/crossterm-rs/crossterm) | 全屏、多面板、键盘优先，同时支持鼠标点击与滚轮 |
| CLI | [clap](https://github.com/clap-rs/clap) | 负责 `scan` 子命令与启动参数 |
| Java 源码解析 | [tree-sitter](https://github.com/tree-sitter/tree-sitter) + [tree-sitter-java](https://github.com/tree-sitter/tree-sitter-java) | 静态扫描 Spring MVC / Boot 源码，不依赖被扫项目编译 |
| HTTP 客户端 | [reqwest](https://github.com/seanmonstar/reqwest) `blocking` | 在后台线程发送请求，避免阻塞 TUI 主循环 |
| 序列化 | [serde](https://serde.rs/) + [serde_json](https://github.com/serde-rs/json) | 本地配置、扫描模型、Body 模板 |
| 目录遍历 | [walkdir](https://github.com/BurntSushi/walkdir) | 收集 `.java` 文件 |
| 并行 | [rayon](https://github.com/rayon-rs/rayon) | 大仓库扫描时并行解析 |
| 纯文本索引 | [regex](https://github.com/rust-lang/regex) | 为懒加载 `@RequestBody` DTO 解析建立简单类型名索引 |

当前版本**未**引入 `notify` 或其他文件监听依赖；源码变更后需用户手动重扫。

若后续需要补更强的 Java 语义能力，可增加独立 Java 扫描进程，通过 `stdin/stdout JSON` 与 Rust 主进程通信；这不是当前 V1 默认实现。

---

## 2. 仓库结构

当前为单 crate：`lazypost`。

```text
src/
  main.rs             # 入口：解析 CLI，启动 TUI 或一次性扫描
  cli.rs              # clap 定义
  model.rs            # LocalApi、ApiParam 等统一模型
  scanner/            # tree-sitter 管线、Spring 规则、参数提取、懒加载 Body
  tui/                # ratatui 全屏界面、事件循环、目录选择器
  http_exec.rs        # URL 组装、Header 合并、HTTP 发送
  user_config.rs      # Host / 全局 Header / 最近项目等本地配置
tests/fixtures/       # Spring MVC / Swagger / 跨文件 DTO 等测试样例工程
```

主要模块边界：

- `scanner/`：把 Java 源码归一化成 `LocalApi`
- `tui/`：负责状态管理、绘制、输入事件、后台扫描与后台请求
- `http_exec.rs`：不依赖 TUI，只接收最终请求参数并返回展示结果
- `user_config.rs`：配置持久化与旧配置迁移

---

## 3. 构建与运行

**前置条件**：安装 [Rust toolchain](https://rustup.rs/)（`rustc`、`cargo`）。

```bash
# 调试构建
cargo build

# 发布构建
cargo build --release

# 查看帮助
cargo run -- --help

# 启动 TUI，并直接扫描指定项目
cargo run -- /path/to/java-project

# 不传路径时，先进入终端目录选择器
cargo run --

# 仅扫描并输出接口索引
cargo run -- scan /path/to/java-project

# 仅扫描并输出 JSON
cargo run -- scan /path/to/java-project --json
```

注意：

- 当前 **没有** `--host`、`--auth`、`refresh` 等 CLI 参数或子命令。
- Base URL 在程序内按 `h` 打开面板管理。
- 全局附加请求头在程序内按 `a` 打开面板管理。
- 配置会写入本机配置目录，而不是仅会话内生效。

TUI 当前关键快捷键以代码内帮助页为准：

- `/`：聚焦底栏筛选
- `g`：切换列表分组（按项目目录 / 平铺 / 按类）
- `r`：重扫当前项目
- `h`：管理多套 Base URL
- `a`：管理全局附加请求头
- `e`：打开请求参数编辑窗口
- `s`：发送请求
- `?`：显示帮助
- `q`：退出

---

## 4. 当前实现快照

### 4.1 已实现

- 扫描 Spring MVC / Boot Controller 与常见映射注解：`@GetMapping`、`@PostMapping`、`@PutMapping`、`@DeleteMapping`、`@PatchMapping`、`@RequestMapping`
- 提取类级与方法级路径，并拼接为完整 Path
- 提取参数：`@PathVariable`、`@RequestParam`、`@RequestHeader`、`@CookieValue`、`@RequestBody`、`@ModelAttribute`
- 提取方法 Javadoc 首段，以及 `@Operation` / `@ApiOperation`、类上 `@Tag`
- TUI 列表支持关键字搜索、分组、折叠、焦点切换、鼠标点击与滚轮
- 请求编辑支持：
  - 多套 Base URL
  - 全局附加 Header
  - Path 参数
  - 扫描得到的 Query 参数
  - 手工附加 Query
  - 方法预设 Header（如 `Accept`、`Content-Type`）
  - Body JSON
- 配置持久化：Host、全局 Header、最近项目路径
- `scan` 子命令输出纯文本或 JSON

### 4.2 部分实现

- `@RequestBody` JSON 模板：
  - `scan` 子命令与测试默认走全量模式，启动即生成
  - TUI 启动走懒加载模式，先扫接口，再后台建立源码索引；选中接口时按需补全 DTO 模板
- `@ModelAttribute`：
  - 已扫描并记录到 `model_params`
  - 当前不在详情或请求编辑窗口中展开，也不参与请求发送

### 4.3 当前未实现

- 文件监听与自动增量刷新
- 监听后驱动的 TUI 自动刷新
- 独立 HTTP 方法筛选器（如“只看 GET/POST”）；当前仅支持关键字匹配
- URL / Header / Body 中的 `${VAR}` 或系统环境变量占位替换
- 发送前对 Body 做 JSON 语法校验
- 响应 JSON 自动 Pretty Print
- 错误字段或错误结构的专门高亮

---

## 5. 测试与质量

- 现有测试以单元测试为主，覆盖：
  - Spring MVC 基本扫描
  - 类级 / 方法级路径拼接
  - 参数提取
  - `@RequestBody` 模板生成
  - 跨文件 DTO 懒加载补全
  - Swagger / OpenAPI 文案提取
  - TUI 内部 UTF-8 光标辅助逻辑
- 样例工程位于 `tests/fixtures/`
- 当前仓库可直接运行：

```bash
cargo test
```

---

## 6. 与产品文档的对应关系

| 产品条目 | 当前实现落点 |
|----------|--------------|
| FR-01 | `src/main.rs`、`src/cli.rs`、`src/tui/dir_picker.rs` |
| FR-02～FR-05 | `src/scanner/`、`src/model.rs` |
| FR-06～FR-09 | `src/tui/` |
| FR-10～FR-11 | `src/http_exec.rs` + `src/tui/` |
| FR-12 | `src/user_config.rs` + `src/tui/` |
| FR-13 | `src/tui/` 中的 `r` 手动重扫 |
| FR-14～FR-15 | 当前未实现，仅在 PRD 中作为候选增强保留 |

---

## 7. 代码风格

- 遵循 `cargo fmt` / `cargo clippy`
- 公共结构尽量保持可测试、可序列化、可脱离 TUI 复用
- 用户可见文案保持简短、可操作
- 设计与实现不一致时，优先修正文档或帮助文案，避免隐含行为

---

## 8. 变更流程

- 产品范围或验收标准变化：先更新 `api_tui_v_1_prd_and_srd.md`
- 仅实现细节、运行方式、模块边界变化：更新本文档与相应代码注释
- 若 CLI 帮助、TUI 帮助、开发文档三者不一致，以代码行为为准，并应在同一次修改中同步修正
