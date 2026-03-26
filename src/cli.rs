//! 命令行入口（clap）。

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// 本地 Spring MVC/Boot 接口扫描与调试 TUI（V1 开发中）。
#[derive(Parser, Debug)]
#[command(
    name = "lazypost",
    version,
    about,
    after_help = "提示：用 cargo run 调试时，项目路径须写在 -- 之后，例如：\n  cargo run -- /path/to/project\n域名在程序内按 h；全局附加请求头按 a（持久化）；各接口 @RequestHeader 在详情 / e 表单里编辑，配置保存在本机配置目录。"
)]
pub struct Cli {
    /// 扫描根目录：可为单项目根或工作区根；省略则启动后在终端内选择（类似 yazi）。
    #[arg(value_name = "PROJECT_DIR")]
    pub project: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 仅扫描并输出接口索引（用于调试与 CI）。
    Scan {
        /// 扫描根目录（可与 TUI 相同，支持工作区下多子项目）。
        #[arg(default_value = ".", value_name = "PROJECT_DIR")]
        project: PathBuf,
        /// 以 JSON 打印结果。
        #[arg(long)]
        json: bool,
    },
}
