# API TUI（lazypost）开发文档

本文档描述 **V1 基线** 的技术栈、仓库结构、构建方式与模块边界；与产品/需求以 `api_tui_v_1_prd_and_srd.md` 为准。

---

## 1. 技术栈

| 领域 | 选型 | 说明 |
|------|------|------|
| 语言 | Rust（Edition 2021，最低版本见 `Cargo.toml` 的 `rust-version`） | 单二进制分发，用户运行**不需要**安装 Java |
| 终端 UI | [ratatui](https://github.com/ratatui/ratatui) + [crossterm](https://github.com/crossterm-rs/crossterm) | 与 yazi 同叙事：全屏、多面板、跨 macOS/Linux |
| CLI | [clap](https://github.com/clap-rs/clap) | 子命令、参数、帮助信息 |
| Java 源码解析 | [tree-sitter](https://github.com/tree-sitter/tree-sitter) + [tree-sitter-java](https://github.com/tree-sitter/tree-sitter-java) | 静态扫描 Spring 注解与路径；不依赖被扫项目编译 |
| HTTP 客户端 | [reqwest](https://github.com/seanmonstar/reqwest)（blocking 或配合运行时，按 TUI 主循环选型） | 发送 GET/POST/PUT/DELETE/PATCH |
| 序列化 | [serde](https://serde.rs/) + [serde_json](https://github.com/serde-rs/json) | 配置、环境、`LocalApi` 与请求体模板 |
| 目录遍历 | [walkdir](https://github.com/BurntSushi/walkdir) 或 [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore) | 收集 `.java` 文件 |
| 并行（可选） | [rayon](https://github.com/rayon-rs/rayon) | 大仓库多文件解析时加速 |
| 文件监听（V1.1 / FR-14） | [notify](https://github.com/notify-rs/notify) | 源码变更后刷新索引 |

后续若需 **JavaParser 级**语义补充，可单独增加 **Java 子进程扫描器**，通过 **stdin/stdout JSON** 与 Rust 主程序通信；不作为 V1 默认路径。

---

## 2. 仓库与 crate 布局（目标结构）

当前采用 **单 crate** `lazypost`；源码按模块拆分，便于测试与替换：

```text
src/
  main.rs           # 入口：解析 CLI，启动 TUI 或一次性扫描
  cli.rs            # clap 定义
  model.rs          # LocalApi、ApiParam
  scanner/          # tree-sitter 管线、Spring 规则、参数提取
  tui/              # ratatui 全屏界面
  http_exec.rs      # URL 拼装、reqwest 同步请求（在后台线程调用）
```

随实现推进可拆分为 **workspace 多 crate**（例如 `lazypost-scanner`），仅在出现清晰边界时再做。

---

## 3. 构建与运行

**前置条件**：安装 [Rust toolchain](https://rustup.rs/)（`rustc`、`cargo`）。

```bash
# 调试构建
cargo build

# 发布构建（体积与性能更优）
cargo build --release

# 运行（具体子命令以 `cargo run -- --help` 为准）
cargo run -- --help

# 全屏 TUI：扫描项目并发请求（需本地已启动 Spring 等 HTTP 服务）
cargo run -- /path/to/java-project --host http://localhost:8080
# 可选鉴权
cargo run -- /path/to/java-project --host http://localhost:8080 --auth "Bearer <token>"

# 仅打印扫描结果（JSON）
cargo run -- scan /path/to/java-project --json
```

TUI 快捷键见界面内 `?` 帮助：`/` 筛选、`h` Base URL、`a` Authorization、`s` 发送、`e` 编辑 Body、`r` 重扫、`q` 退出。

交叉编译 macOS / Linux release 产物在 CI 或发布流程中配置 **目标三元组**（如 `x86_64-unknown-linux-gnu`、`aarch64-apple-darwin`）；细节见后续 `release` 说明或脚本。

---

## 4. 测试与质量

- **单元测试**：`scanner` 对典型 `.java` 片段或 `tests/fixtures` 下样例工程做快照/断言。
- **集成测试**：可选 `tests/` 下对整个 CLI 子进程做端到端（需控制终端环境时可用 `script` 或剥离 TUI 的纯扫描模式）。

---

## 5. 与产品文档的对应关系

| 文档章节 | 实现落点 |
|----------|----------|
| FR-01～FR-05、LocalApi | `scanner/` + `model/` |
| FR-06～FR-09、NFR-05 | `tui/` |
| FR-10～FR-11 | `http_exec.rs` + `tui/` 响应区；环境为 CLI `--host` / `--auth` 与 TUI 内编辑 |
| FR-12、配置 | 当前：会话内配置；持久化见后续 `config/` |
| FR-13 | CLI `refresh` 或 TUI 内动作 |
| FR-14～FR-15 | `notify` + 扫描器增量策略 + `tui/` 刷新 |

---

## 6. 代码风格

- 遵循 `cargo fmt` / `cargo clippy` 默认规则；公共 API 与错误类型使用明确枚举，避免裸 `String` 滥用。
- 与用户可见字符串：中文或英文需与产品一致；错误信息简短、可行动。

---

## 7. 变更流程

- 产品范围或验收标准变更：先改 `api_tui_v_1_prd_and_srd.md`，再改本文件与代码。
- 仅实现细节、依赖升级：更新本文件「技术栈」与 `Cargo.toml` 即可。
