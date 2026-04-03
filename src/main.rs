//! API TUI（lazypost）：Spring MVC/Boot 接口扫描与调试终端工具。
//! 详见 `api_tui_v_1_prd_and_srd.md` 与 `DEVELOPMENT.md`。

mod cli;
mod http_exec;
mod model;
mod scanner;
mod tui;
mod user_config;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Scan { project, json }) => {
            let report = scanner::scan_project(&project);
            for err in &report.file_errors {
                eprintln!("{err}");
            }
            if json {
                let out = serde_json::json!({
                    "apis": report.apis,
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("项目: {}", project.display());
                println!("接口数: {}", report.apis.len());
                for api in &report.apis {
                    let bucket_hint = if api.project_bucket != "." && !api.project_bucket.is_empty()
                    {
                        format!(" [{}]", api.project_bucket)
                    } else {
                        String::new()
                    };
                    println!(
                        "  {:7} {}  —  {}  [{}:{}]{}",
                        api.http_method, api.path, api.name, api.source_file, api.line, bucket_hint
                    );
                }
            }
            Ok(())
        }
        None => {
            let project = match cli.project {
                Some(p) => p,
                None => match tui::pick_project_dir()? {
                    Some(p) => p,
                    None => return Ok(()),
                },
            };
            let _ = user_config::record_recent_project(&project);
            tui::run(project)?;
            Ok(())
        }
    }
}
