use anyhow::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

use crate::config::Config;
use crate::git;
use crate::gitlab::GitLabClient;
use crate::project::{self, Project};
use crate::tui::menu::{MenuAction, MenuState};

pub struct App {
    config: Config,
    projects: Vec<Project>,
    gitlab: GitLabClient,
}

enum Page {
    MainMenu,
    ProjectSelect,
    LocalOperation { project_idx: usize },
    SourceBranch { project_idx: usize, operation: LocalOp },
    ExecuteResult { lines: Vec<(bool, String)> },
    MrMenu,
    GitLabProjectSelect { mr_mode: MrMode },
    BranchMapSelect { project_id: u64, project_name: String },
    MrResult { lines: Vec<(bool, String)> },
}

#[derive(Clone)]
enum LocalOp {
    Sync,
    MergeAll,
    MergeFixed,
    MergeSingle,
}

#[derive(Clone)]
enum MrMode {
    Single,
    Batch,
    FixedThree,
}

impl App {
    pub fn new(config: Config) -> Self {
        let gitlab = GitLabClient::new(&config.gitlab.host, &config.gitlab.token);
        Self {
            config,
            projects: Vec::new(),
            gitlab,
        }
    }

    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.main_loop(&mut terminal);

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    fn main_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        let mut page_stack: Vec<Page> = vec![Page::MainMenu];

        loop {
            let current_page = match page_stack.last() {
                Some(p) => p,
                None => break,
            };

            match current_page {
                Page::MainMenu => {
                    let action = self.show_main_menu(terminal)?;
                    match action {
                        Some(MainMenuAction::LocalOps) => {
                            self.scan_projects()?;
                            page_stack.push(Page::ProjectSelect);
                        }
                        Some(MainMenuAction::GitLabMr) => {
                            page_stack.push(Page::MrMenu);
                        }
                        Some(MainMenuAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ProjectSelect => {
                    let action = self.show_project_select(terminal)?;
                    match action {
                        Some(ProjectAction::Select(idx)) => {
                            page_stack.push(Page::LocalOperation { project_idx: idx });
                        }
                        Some(ProjectAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ProjectAction::Quit) => break,
                        None => {}
                    }
                }
                Page::LocalOperation { project_idx } => {
                    let pidx = *project_idx;
                    let action = self.show_local_operation(terminal)?;
                    match action {
                        Some(LocalOpAction::Select(op)) => match op {
                            LocalOp::Sync => {
                                let results = self.execute_sync(pidx);
                                page_stack.push(Page::ExecuteResult { lines: results });
                            }
                            _ => {
                                page_stack.push(Page::SourceBranch {
                                    project_idx: pidx,
                                    operation: op,
                                });
                            }
                        },
                        Some(LocalOpAction::Back) => {
                            page_stack.pop();
                        }
                        Some(LocalOpAction::Quit) => break,
                        None => {}
                    }
                }
                Page::SourceBranch {
                    project_idx,
                    operation,
                } => {
                    let pidx = *project_idx;
                    let op = operation.clone();
                    let action = self.show_source_branch(terminal, pidx)?;
                    match action {
                        Some(BranchAction::Select(branch)) => {
                            let results = self.execute_merge(pidx, &branch, &op);
                            page_stack.push(Page::ExecuteResult { lines: results });
                        }
                        Some(BranchAction::Back) => {
                            page_stack.pop();
                        }
                        Some(BranchAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ExecuteResult { .. } | Page::MrResult { .. } => {
                    let lines = match page_stack.last().unwrap() {
                        Page::ExecuteResult { lines } => lines.clone(),
                        Page::MrResult { lines } => lines.clone(),
                        _ => unreachable!(),
                    };
                    let action = self.show_results(terminal, &lines)?;
                    match action {
                        Some(ResultAction::Back) => {
                            // Pop back to a menu page
                            page_stack.pop();
                            // Also pop the operation page to go back to project select
                            while matches!(
                                page_stack.last(),
                                Some(Page::SourceBranch { .. })
                                    | Some(Page::LocalOperation { .. })
                                    | Some(Page::BranchMapSelect { .. })
                                    | Some(Page::GitLabProjectSelect { .. })
                            ) {
                                page_stack.pop();
                            }
                        }
                        None => {}
                    }
                }
                Page::MrMenu => {
                    let action = self.show_mr_menu(terminal)?;
                    match action {
                        Some(MrMenuAction::Single) => {
                            page_stack.push(Page::GitLabProjectSelect {
                                mr_mode: MrMode::Single,
                            });
                        }
                        Some(MrMenuAction::Batch) => {
                            page_stack.push(Page::GitLabProjectSelect {
                                mr_mode: MrMode::Batch,
                            });
                        }
                        Some(MrMenuAction::FixedThree) => {
                            page_stack.push(Page::GitLabProjectSelect {
                                mr_mode: MrMode::FixedThree,
                            });
                        }
                        Some(MrMenuAction::Back) => {
                            page_stack.pop();
                        }
                        Some(MrMenuAction::Quit) => break,
                        None => {}
                    }
                }
                Page::GitLabProjectSelect { mr_mode } => {
                    let mode = mr_mode.clone();
                    let action = self.show_gitlab_project_select(terminal)?;
                    match action {
                        Some(GitLabAction::Select(id, name)) => match mode {
                            MrMode::Single => {
                                page_stack.push(Page::BranchMapSelect {
                                    project_id: id,
                                    project_name: name,
                                });
                            }
                            MrMode::Batch => {
                                let results = self.execute_mr_batch(id, &name);
                                page_stack.push(Page::MrResult { lines: results });
                            }
                            MrMode::FixedThree => {
                                let results = self.execute_mr_fixed_three(id, &name);
                                page_stack.push(Page::MrResult { lines: results });
                            }
                        },
                        Some(GitLabAction::Back) => {
                            page_stack.pop();
                        }
                        Some(GitLabAction::Quit) => break,
                        None => {}
                    }
                }
                Page::BranchMapSelect {
                    project_id,
                    project_name,
                } => {
                    let pid = *project_id;
                    let pname = project_name.clone();
                    let action = self.show_branch_map_select(terminal)?;
                    match action {
                        Some(BranchMapAction::Select(src, tgt)) => {
                            let results = self.execute_mr_single(pid, &pname, &src, &tgt);
                            page_stack.push(Page::MrResult { lines: results });
                        }
                        Some(BranchMapAction::Back) => {
                            page_stack.pop();
                        }
                        Some(BranchMapAction::Quit) => break,
                        None => {}
                    }
                }
            }
        }

        Ok(())
    }

    // ---- Menu handlers ----

    fn show_main_menu(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<MainMenuAction>> {
        let items = vec![
            "本地分支同步 / 合并（含推送）".to_string(),
            "GitLab MR 创建".to_string(),
            "退出程序".to_string(),
        ];
        let details = vec![
            vec!["适合处理本地项目的环境分支同步、批量合并、单分支合并和推送。".to_string()],
            vec!["适合直接创建单个或批量 Merge Request，并支持后续审批合并。".to_string()],
            vec!["结束 gmux。".to_string()],
        ];

        let mut menu = MenuState::new("gmux", "终端 Git 工作流工具", items).with_details(details);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(MainMenuAction::LocalOps),
                    MenuAction::Select(1) => Some(MainMenuAction::GitLabMr),
                    MenuAction::Select(2) | MenuAction::Back => Some(MainMenuAction::Quit),
                    MenuAction::Quit => Some(MainMenuAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_project_select(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<ProjectAction>> {
        let items: Vec<String> = self.projects.iter().map(|p| p.name.clone()).collect();
        let details: Vec<Vec<String>> = self
            .projects
            .iter()
            .map(|p| {
                let mut d = vec![
                    format!("名称: {}", p.name),
                    format!("路径: {}", p.path.display()),
                ];
                for env in &self.config.project.env_branches {
                    let merge = self.config.get_merge_branch_name(env, &p.name);
                    d.push(format!("{env} -> {merge}"));
                }
                d
            })
            .collect();

        let mut menu = MenuState::new(
            "gmux / 项目选择",
            "选择一个本地 Git 仓库进行同步或合并操作",
            items,
        )
        .with_details(details);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(i) => Some(ProjectAction::Select(i)),
                    MenuAction::Back => Some(ProjectAction::Back),
                    MenuAction::Quit => Some(ProjectAction::Quit),
                });
            }
        }
    }

    fn show_local_operation(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<LocalOpAction>> {
        let env_count = self.config.project.env_branches.len();
        let items = vec![
            format!("同步 {env_count} 个环境分支并推送到对应合并分支"),
            format!("将指定分支合并到全部 {env_count} 个目标合并分支"),
            format!(
                "将指定分支合并到 {} 个目标合并分支",
                env_count.saturating_sub(1)
            ),
            "将指定分支合并到单个目标合并分支".to_string(),
        ];
        let details = vec![
            vec!["依次更新各环境分支，再同步到对应合并分支并 push。".to_string()],
            vec!["从本地分支列表中选择源分支，再合并到所有目标合并分支并分别 push。".to_string()],
            vec!["从本地分支列表中选择源分支，合并到除最后一个以外的目标合并分支。".to_string()],
            vec!["先选择源分支，再选择一个目标合并分支进行 merge + push。".to_string()],
        ];

        let mut menu =
            MenuState::new("gmux / 本地操作", "上下选择操作类型，Enter 确认", items)
                .with_details(details);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(LocalOpAction::Select(LocalOp::Sync)),
                    MenuAction::Select(1) => Some(LocalOpAction::Select(LocalOp::MergeAll)),
                    MenuAction::Select(2) => Some(LocalOpAction::Select(LocalOp::MergeFixed)),
                    MenuAction::Select(3) => Some(LocalOpAction::Select(LocalOp::MergeSingle)),
                    MenuAction::Back => Some(LocalOpAction::Back),
                    MenuAction::Quit => Some(LocalOpAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_source_branch(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        project_idx: usize,
    ) -> Result<Option<BranchAction>> {
        let project = &self.projects[project_idx];
        let branches = git::list_local_branches(&project.path)?;
        let current = git::current_branch(&project.path)?;

        let details: Vec<Vec<String>> = branches
            .iter()
            .map(|b| {
                let mut d = vec![format!("分支: {b}")];
                if current.as_deref() == Some(b.as_str()) {
                    d.push("当前检出: 是".to_string());
                }
                d
            })
            .collect();

        let mut menu = MenuState::new(
            "gmux / 源分支",
            "选择一个本地分支作为源分支",
            branches.clone(),
        )
        .with_details(details);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(i) => Some(BranchAction::Select(branches[i].clone())),
                    MenuAction::Back => Some(BranchAction::Back),
                    MenuAction::Quit => Some(BranchAction::Quit),
                });
            }
        }
    }

    fn show_results(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        lines: &[(bool, String)],
    ) -> Result<Option<ResultAction>> {
        use ratatui::{
            style::{Color, Style},
            text::{Line, Span},
            widgets::{Block, Borders, Paragraph, Wrap},
        };

        let text_lines: Vec<Line> = lines
            .iter()
            .map(|(ok, msg)| {
                let (prefix, color) = if *ok {
                    ("[SUCCESS] ", Color::Green)
                } else {
                    ("[ERROR]   ", Color::Red)
                };
                Line::from(vec![
                    Span::styled(prefix, Style::default().fg(color)),
                    Span::raw(msg),
                ])
            })
            .collect();

        let mut footer_lines = text_lines.clone();
        footer_lines.push(Line::raw(""));
        footer_lines.push(Line::from(Span::styled(
            "按任意键返回",
            Style::default().fg(Color::DarkGray),
        )));

        loop {
            terminal.draw(|f| {
                let area = f.area();
                let p = Paragraph::new(footer_lines.clone())
                    .block(
                        Block::default()
                            .title("  执行结果  ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Rgb(81, 81, 81))),
                    )
                    .wrap(Wrap { trim: false });
                f.render_widget(p, area);
            })?;

            if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    return Ok(Some(ResultAction::Back));
                }
            }
        }
    }

    fn show_mr_menu(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<MrMenuAction>> {
        let env_count = self.config.project.env_branches.len();
        let items = vec![
            "单个创建".to_string(),
            "批量创建（排除最后一组映射）".to_string(),
            format!(
                "批量创建（前 {} 组映射）",
                env_count.saturating_sub(1)
            ),
            "返回主菜单".to_string(),
        ];
        let details = vec![
            vec!["先选项目，再选一组源/目标分支映射，创建一个 MR。".to_string()],
            vec!["对一个项目批量创建多组 MR，并自动尝试审批和合并。".to_string()],
            vec![format!(
                "只批量创建前 {} 组固定映射的 MR，并自动尝试审批和合并。",
                env_count.saturating_sub(1)
            )],
            vec!["不执行 MR 操作。".to_string()],
        ];

        let mut menu = MenuState::new("gmux / MR 模式", "选择 MR 处理方式", items)
            .with_details(details);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(MrMenuAction::Single),
                    MenuAction::Select(1) => Some(MrMenuAction::Batch),
                    MenuAction::Select(2) => Some(MrMenuAction::FixedThree),
                    MenuAction::Select(3) => Some(MrMenuAction::Back),
                    MenuAction::Back => Some(MrMenuAction::Back),
                    MenuAction::Quit => Some(MrMenuAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_gitlab_project_select(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<GitLabAction>> {
        // Show loading message
        terminal.draw(|f| {
            let area = f.area();
            let p = ratatui::widgets::Paragraph::new("  正在加载 GitLab 项目列表...");
            f.render_widget(p, area);
        })?;

        let gl_projects = self.gitlab.list_projects()?;

        if gl_projects.is_empty() {
            return Ok(Some(GitLabAction::Back));
        }

        let items: Vec<String> = gl_projects
            .iter()
            .map(|p| format!("{}  [ID: {}]", p.name, p.id))
            .collect();
        let details: Vec<Vec<String>> = gl_projects
            .iter()
            .map(|p| {
                vec![
                    format!("名称: {}", p.name),
                    format!("ID: {}", p.id),
                ]
            })
            .collect();

        let mut menu = MenuState::new(
            "gmux / GitLab 项目",
            "选择一个 GitLab 项目用于创建 MR",
            items,
        )
        .with_details(details);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(i) => Some(GitLabAction::Select(
                        gl_projects[i].id,
                        gl_projects[i].name.clone(),
                    )),
                    MenuAction::Back => Some(GitLabAction::Back),
                    MenuAction::Quit => Some(GitLabAction::Quit),
                });
            }
        }
    }

    fn show_branch_map_select(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<BranchMapAction>> {
        let mut keys: Vec<String> = self.config.branch_map.keys().cloned().collect();
        keys.sort();

        let items: Vec<String> = keys
            .iter()
            .map(|k| format!("{k} -> {}", self.config.branch_map[k]))
            .collect();
        let details: Vec<Vec<String>> = keys
            .iter()
            .map(|k| {
                vec![
                    format!("源分支: {k}"),
                    format!("目标分支: {}", self.config.branch_map[k]),
                ]
            })
            .collect();

        let mut menu = MenuState::new(
            "gmux / 分支映射",
            "选择源分支与目标分支的映射关系",
            items,
        )
        .with_details(details);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(i) => {
                        let src = keys[i].clone();
                        let tgt = self.config.branch_map[&src].clone();
                        Some(BranchMapAction::Select(src, tgt))
                    }
                    MenuAction::Back => Some(BranchMapAction::Back),
                    MenuAction::Quit => Some(BranchMapAction::Quit),
                });
            }
        }
    }

    // ---- Business logic ----

    fn scan_projects(&mut self) -> Result<()> {
        self.projects = project::scan_projects(&self.config.project.root_dir)?;
        Ok(())
    }

    fn execute_sync(&self, project_idx: usize) -> Vec<(bool, String)> {
        let project = &self.projects[project_idx];
        let results = project::sync_and_push(project, &self.config);
        results
            .into_iter()
            .map(|r| (r.success, format!("{} -> {}: {}", r.branch, r.target, r.message)))
            .collect()
    }

    fn execute_merge(
        &self,
        project_idx: usize,
        source_branch: &str,
        operation: &LocalOp,
    ) -> Vec<(bool, String)> {
        let project = &self.projects[project_idx];
        let targets: Vec<String> = match operation {
            LocalOp::MergeAll => project::get_target_merge_branches(&self.config, &project.name)
                .into_iter()
                .map(|(_, b)| b)
                .collect(),
            LocalOp::MergeFixed => {
                project::get_fixed_target_merge_branches(&self.config, &project.name)
                    .into_iter()
                    .map(|(_, b)| b)
                    .collect()
            }
            LocalOp::MergeSingle => {
                // For single, we'd need another menu selection.
                // For now use all targets - the caller should handle this differently.
                // This is handled in the page flow.
                Vec::new()
            }
            _ => Vec::new(),
        };

        if targets.is_empty() {
            return vec![(false, "未选择目标分支".to_string())];
        }

        let results = project::merge_to_targets(project, source_branch, &targets);
        results
            .into_iter()
            .map(|r| (r.success, format!("{} -> {}: {}", r.branch, r.target, r.message)))
            .collect()
    }

    fn execute_mr_single(
        &self,
        project_id: u64,
        project_name: &str,
        src: &str,
        tgt: &str,
    ) -> Vec<(bool, String)> {
        let mut results = Vec::new();

        match self.gitlab.create_mr(project_id, project_name, src, tgt) {
            Ok(mr) => {
                results.push((true, format!("MR 创建成功: {} (IID: {})", mr.web_url, mr.iid)));
                // Try approve and merge
                match self.gitlab.approve_and_merge(project_id, mr.iid) {
                    Ok(()) => results.push((true, "MR 已审批并合并".to_string())),
                    Err(e) => results.push((false, format!("MR 审批/合并失败: {e}"))),
                }
            }
            Err(e) => results.push((false, format!("创建 MR 失败: {e}"))),
        }

        results
    }

    fn execute_mr_batch(&self, project_id: u64, project_name: &str) -> Vec<(bool, String)> {
        let mut results = Vec::new();
        let mut mr_list: Vec<(u64, String, String)> = Vec::new();

        for (src, tgt) in &self.config.branch_map {
            if tgt == "master" {
                continue;
            }
            match self.gitlab.create_mr(project_id, project_name, src, tgt) {
                Ok(mr) => {
                    results.push((
                        true,
                        format!("MR 创建成功: {src} -> {tgt} (IID: {})", mr.iid),
                    ));
                    mr_list.push((mr.iid, src.clone(), tgt.clone()));
                }
                Err(e) => {
                    results.push((false, format!("创建 MR 失败 {src} -> {tgt}: {e}")));
                }
            }
        }

        // Auto approve and merge
        for (iid, src, tgt) in &mr_list {
            match self.gitlab.approve_and_merge(project_id, *iid) {
                Ok(()) => results.push((true, format!("已审批并合并: {src} -> {tgt}"))),
                Err(e) => results.push((false, format!("审批/合并失败 {src} -> {tgt}: {e}"))),
            }
        }

        results
    }

    fn execute_mr_fixed_three(
        &self,
        project_id: u64,
        project_name: &str,
    ) -> Vec<(bool, String)> {
        let mut results = Vec::new();
        let env_branches = &self.config.project.env_branches;
        let middle = &self.config.project.merge_branch_middle;
        let mut mr_list: Vec<(u64, String, String)> = Vec::new();

        let count = env_branches.len().saturating_sub(1);
        for env in &env_branches[..count] {
            let src = format!("{env}_{middle}_meger");
            let tgt = env.clone();

            match self.gitlab.create_mr(project_id, project_name, &src, &tgt) {
                Ok(mr) => {
                    results.push((
                        true,
                        format!("MR 创建成功: {src} -> {tgt} (IID: {})", mr.iid),
                    ));
                    mr_list.push((mr.iid, src, tgt));
                }
                Err(e) => {
                    results.push((false, format!("创建 MR 失败 {src} -> {tgt}: {e}")));
                }
            }
        }

        for (iid, src, tgt) in &mr_list {
            match self.gitlab.approve_and_merge(project_id, *iid) {
                Ok(()) => results.push((true, format!("已审批并合并: {src} -> {tgt}"))),
                Err(e) => results.push((false, format!("审批/合并失败 {src} -> {tgt}: {e}"))),
            }
        }

        results
    }
}

// ---- Action enums ----

enum MainMenuAction {
    LocalOps,
    GitLabMr,
    Quit,
}

enum ProjectAction {
    Select(usize),
    Back,
    Quit,
}

enum LocalOpAction {
    Select(LocalOp),
    Back,
    Quit,
}

enum BranchAction {
    Select(String),
    Back,
    Quit,
}

enum ResultAction {
    Back,
}

enum MrMenuAction {
    Single,
    Batch,
    FixedThree,
    Back,
    Quit,
}

enum GitLabAction {
    Select(u64, String),
    Back,
    Quit,
}

enum BranchMapAction {
    Select(String, String),
    Back,
    Quit,
}
