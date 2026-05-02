use anyhow::{Context, Result, bail};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{self, ClearType},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub gitlab: GitLabConfig,
    pub project: ProjectConfig,
    #[serde(default)]
    pub branch_map: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabConfig {
    pub host: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub root_dir: String,
    pub merge_branch_middle: String,
    #[serde(default = "default_env_branches")]
    pub env_branches: Vec<String>,
}

fn default_env_branches() -> Vec<String> {
    vec![
        "uat".to_string(),
        "test".to_string(),
        "stage".to_string(),
        "pre_prod".to_string(),
    ]
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::find_config_file();

        match config_path {
            Some(path) => {
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("无法读取配置文件: {}", path.display()))?;
                let mut config: Config =
                    toml::from_str(&content).with_context(|| "配置文件解析失败")?;
                config.ensure_branch_map();
                config.validate()?;
                Ok(config)
            }
            None => Self::run_init_wizard(),
        }
    }

    fn find_config_file() -> Option<PathBuf> {
        // Only ~/.config/gmux/gmux.toml
        let xdg = Self::xdg_config_path();
        if xdg.is_file() {
            return Some(xdg);
        }

        None
    }

    fn default_global_config_path() -> PathBuf {
        Self::xdg_config_path()
    }

    fn xdg_config_path() -> PathBuf {
        if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(config_home).join("gmux").join("gmux.toml");
        }

        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("gmux")
            .join("gmux.toml")
    }

    fn ensure_branch_map(&mut self) {
        if self.branch_map.is_empty() {
            self.branch_map = Self::generate_default_branch_map(
                &self.project.env_branches,
                &self.project.merge_branch_middle,
            );
        }
    }

    fn generate_default_branch_map(
        env_branches: &[String],
        merge_branch_middle: &str,
    ) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for env in env_branches {
            let src = format!("{env}_{merge_branch_middle}_meger");
            map.insert(src, env.clone());
        }
        map
    }

    fn validate(&self) -> Result<()> {
        let mut missing = Vec::new();
        if self.gitlab.host.is_empty() {
            missing.push("gitlab.host");
        }
        if self.gitlab.token.is_empty() {
            missing.push("gitlab.token");
        }
        if self.project.root_dir.is_empty() {
            missing.push("project.root_dir");
        }
        if self.project.merge_branch_middle.is_empty() {
            missing.push("project.merge_branch_middle");
        }
        if !missing.is_empty() {
            bail!("配置缺少必填项: {}", missing.join(", "));
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn get_merge_branch_name(&self, env_branch: &str, project_name: &str) -> String {
        let middle = if self.project.merge_branch_middle == "PROJECT_NAME" {
            project_name
        } else {
            &self.project.merge_branch_middle
        };
        format!("{env_branch}_{middle}_meger")
    }

    pub fn run_init_wizard() -> Result<Self> {
        println!("\n\x1b[1m\x1b[38;5;81m══════════════════════════════════════\x1b[0m");
        println!("\x1b[1m\x1b[0;36m  gmux 初始化配置向导\x1b[0m");
        println!("\x1b[1m\x1b[38;5;81m══════════════════════════════════════\x1b[0m\n");

        let host = prompt_value("GitLab 服务器地址", "例如 gitlab.example.com:8099")?;
        let token = prompt_value("GitLab API Token", "例如 glpat-xxxxxxxxxxxx")?;
        let root_dir = prompt_directory("本地项目根目录（包含多个 Git 仓库的父目录）")?;
        let merge_branch_middle = prompt_value(
            "合并分支中间名",
            "例如你的用户名，或输入 PROJECT_NAME 使用项目名",
        )?;
        let env_branches = prompt_env_branches()?;

        let branch_map = Self::generate_default_branch_map(&env_branches, &merge_branch_middle);

        let config = Config {
            gitlab: GitLabConfig { host, token },
            project: ProjectConfig {
                root_dir,
                merge_branch_middle,
                env_branches,
            },
            branch_map,
        };

        // Preview
        println!("\n\x1b[1m配置预览：\x1b[0m");
        println!("  gitlab.host           = {}", config.gitlab.host);
        println!(
            "  gitlab.token          = {}****",
            &config.gitlab.token[..config.gitlab.token.len().min(8)]
        );
        println!("  project.root_dir      = {}", config.project.root_dir);
        println!(
            "  merge_branch_middle   = {}",
            config.project.merge_branch_middle
        );
        println!(
            "  env_branches          = {:?}",
            config.project.env_branches
        );
        println!("  branch_map:");
        for (src, tgt) in &config.branch_map {
            println!("    {src} -> {tgt}");
        }

        let save_path = Self::default_global_config_path();

        config.save(&save_path)?;
        println!(
            "\n\x1b[0;32m[SUCCESS]\x1b[0m 配置已保存到: {}\n",
            save_path.display()
        );

        Ok(config)
    }
}

fn prompt_value(label: &str, hint: &str) -> Result<String> {
    loop {
        print!("\x1b[0;36m{label}\x1b[0m \x1b[2m({hint})\x1b[0m: ");
        io::stdout().flush()?;
        let mut value = String::new();
        io::stdin().read_line(&mut value)?;
        let value = value.trim().to_string();
        if value.is_empty() {
            println!("\x1b[0;31m  不能为空，请重新输入\x1b[0m");
            continue;
        }
        return Ok(value);
    }
}

fn prompt_directory(label: &str) -> Result<String> {
    let mut current = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    current = current.canonicalize().unwrap_or(current);
    let mut selected: usize = 0;

    terminal::enable_raw_mode()?;
    let result = browse_directory_loop(&mut current, &mut selected, label);
    terminal::disable_raw_mode()?;

    result
}

fn browse_directory_loop(
    current: &mut PathBuf,
    selected: &mut usize,
    label: &str,
) -> Result<String> {
    loop {
        let mut entries: Vec<(String, bool)> = Vec::new(); // (name, is_dir)

        // ".." entry for parent
        if current.parent().is_some() {
            entries.push(("..".to_string(), true));
        }

        // List directory contents (directories first, then files - but we only show dirs)
        if let Ok(read_dir) = std::fs::read_dir(&*current) {
            let mut subdirs: Vec<String> = Vec::new();
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name() {
                        let name = name.to_string_lossy().to_string();
                        if !name.starts_with('.') {
                            subdirs.push(name);
                        }
                    }
                }
            }
            subdirs.sort();
            for name in subdirs {
                entries.push((name, true));
            }
        }

        if *selected >= entries.len() {
            *selected = entries.len().saturating_sub(1);
        }

        // Render
        let mut stdout = io::stdout();
        execute!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;

        println!("\x1b[38;5;81m══════════════════════════════════════\x1b[0m\r");
        println!("\x1b[1m\x1b[0;36m  {label}\x1b[0m\r");
        println!("\x1b[38;5;81m──────────────────────────────────────\x1b[0m\r");
        println!("\r");
        println!("  \x1b[1m当前目录:\x1b[0m {}\r", current.display());
        println!("\r");

        let term_height = terminal::size().map(|(_, h)| h as usize).unwrap_or(30);
        let max_visible = term_height.saturating_sub(12);
        let total = entries.len();

        // Calculate scroll window
        let start = if total <= max_visible {
            0
        } else if *selected < max_visible / 2 {
            0
        } else if *selected + max_visible / 2 >= total {
            total.saturating_sub(max_visible)
        } else {
            selected.saturating_sub(max_visible / 2)
        };
        let end = (start + max_visible).min(total);

        if entries.is_empty() {
            println!("  \x1b[2m（空目录）\x1b[0m\r");
        } else {
            for i in start..end {
                let (name, _) = &entries[i];
                if i == *selected {
                    println!("  \x1b[48;5;25m\x1b[38;5;255m  ▶ {name}/  \x1b[0m\r");
                } else {
                    println!("    \x1b[38;5;153m{name}/\x1b[0m\r");
                }
            }
            if total > max_visible {
                println!("\r");
                println!("  \x1b[2m{}/{} 项\x1b[0m\r", *selected + 1, total);
            }
        }

        println!("\r");
        println!("  \x1b[2m↑/↓ 移动  Enter 进入/确认  ← 上级  Space 选定当前目录  q 退出\x1b[0m\r");

        // Handle key
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if *selected > 0 {
                        *selected -= 1;
                    } else {
                        *selected = total.saturating_sub(1);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if *selected + 1 < total {
                        *selected += 1;
                    } else {
                        *selected = 0;
                    }
                }
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                    if let Some((name, true)) = entries.get(*selected) {
                        if name == ".." {
                            if let Some(parent) = current.parent() {
                                *current = parent.to_path_buf();
                                *selected = 0;
                            }
                        } else {
                            *current = current.join(name);
                            *selected = 0;
                        }
                    }
                }
                KeyCode::Left | KeyCode::Backspace | KeyCode::Char('h') => {
                    if let Some(parent) = current.parent() {
                        *current = parent.to_path_buf();
                        *selected = 0;
                    }
                }
                KeyCode::Char(' ') => {
                    // Confirm current directory
                    let result = current.canonicalize().unwrap_or(current.clone());
                    execute!(
                        io::stdout(),
                        terminal::Clear(ClearType::All),
                        cursor::MoveTo(0, 0)
                    )?;
                    println!("\x1b[0;32m  已选择:\x1b[0m {}\r", result.display());
                    return Ok(result.to_string_lossy().to_string());
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    bail!("用户取消选择");
                }
                _ => {}
            }
        }
    }
}

fn prompt_env_branches() -> Result<Vec<String>> {
    print!(
        "\x1b[0;36m环境分支列表\x1b[0m \x1b[2m(空格分隔，回车使用默认值: uat test stage pre_prod)\x1b[0m\n> "
    );
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    let branches = if input.is_empty() {
        default_env_branches()
    } else {
        input.split_whitespace().map(String::from).collect()
    };

    println!("\x1b[0;32m  已设置环境分支:\x1b[0m {}", branches.join(" "));
    Ok(branches)
}
