mod config;
mod git;
mod gitlab;
mod project;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gmux", about = "终端 Git 工作流工具", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 运行初始化配置向导
    Init,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            config::Config::run_init_wizard()?;
            Ok(())
        }
        None => {
            let config = config::Config::load()?;
            let mut app = tui::app::App::new(config);
            app.run()
        }
    }
}
