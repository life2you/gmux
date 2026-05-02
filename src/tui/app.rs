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
use crate::tui::checklist::{ChecklistAction, ChecklistState};
use crate::tui::menu::{MenuAction, MenuState};

pub struct App {
    config: Config,
    projects: Vec<Project>,
    gitlab: GitLabClient,
}

enum Page {
    MainMenu,
    ProjectSelect,
    LocalOperation {
        project_idx: usize,
    },
    SourceBranch {
        project_idx: usize,
        operation: LocalOp,
    },
    TargetBranchMulti {
        project_idx: usize,
        source_branch: String,
        state: ChecklistState,
        targets: Vec<String>,
    },
    TargetBranch {
        project_idx: usize,
        source_branch: String,
    },
    ExecutionPreview {
        plan: ExecutionPlan,
    },
    ExecuteResult {
        lines: Vec<(bool, String)>,
    },
    MrMenu,
    GitLabProjectSelect {
        mr_mode: MrMode,
    },
    BranchMapSelect {
        project_id: u64,
        project_name: String,
    },
}

#[derive(Clone)]
enum LocalOp {
    Sync,
    MergeAll,
    MergeFixed,
    MergeCustom,
    MergeSingle,
}

#[derive(Clone)]
enum MrMode {
    Single,
    Batch,
    FixedThree,
}

#[derive(Clone)]
enum ExecutionPlan {
    Sync {
        project_idx: usize,
    },
    Merge {
        project_idx: usize,
        source_branch: String,
        targets: Vec<String>,
    },
    MrSingle {
        project_id: u64,
        project_name: String,
        source_branch: String,
        target_branch: String,
    },
    MrBatch {
        project_id: u64,
        project_name: String,
        mappings: Vec<(String, String)>,
    },
    MrFixedThree {
        project_id: u64,
        project_name: String,
        mappings: Vec<(String, String)>,
    },
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
            let current_page = match page_stack.last_mut() {
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
                                page_stack.push(Page::ExecutionPreview {
                                    plan: ExecutionPlan::Sync { project_idx: pidx },
                                });
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
                        Some(BranchAction::Select(branch)) => match op {
                            LocalOp::MergeCustom => match self
                                .target_branch_multi_page(pidx, branch.clone())
                            {
                                Ok(page) => page_stack.push(page),
                                Err(err) => {
                                    page_stack.push(Page::ExecuteResult {
                                        lines: vec![(false, format!("读取目标分支失败: {err:#}"))],
                                    });
                                }
                            },
                            LocalOp::MergeSingle => {
                                page_stack.push(Page::TargetBranch {
                                    project_idx: pidx,
                                    source_branch: branch,
                                });
                            }
                            _ => {
                                let targets = self.targets_for_operation(pidx, &op);
                                page_stack.push(Page::ExecutionPreview {
                                    plan: ExecutionPlan::Merge {
                                        project_idx: pidx,
                                        source_branch: branch,
                                        targets,
                                    },
                                });
                            }
                        },
                        Some(BranchAction::Back) => {
                            page_stack.pop();
                        }
                        Some(BranchAction::Quit) => break,
                        None => {}
                    }
                }
                Page::TargetBranchMulti {
                    project_idx,
                    source_branch,
                    state,
                    targets,
                } => {
                    let pidx = *project_idx;
                    let source = source_branch.clone();
                    let target_options = targets.clone();
                    terminal.draw(|f| state.render(f))?;
                    match state.handle_key_event() {
                        Some(ChecklistAction::Submit(indexes)) => {
                            let selected_targets: Vec<String> = indexes
                                .into_iter()
                                .filter_map(|index| target_options.get(index).cloned())
                                .collect();
                            page_stack.push(Page::ExecutionPreview {
                                plan: ExecutionPlan::Merge {
                                    project_idx: pidx,
                                    source_branch: source,
                                    targets: selected_targets,
                                },
                            });
                        }
                        Some(ChecklistAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ChecklistAction::Quit) => break,
                        None => {}
                    }
                }
                Page::TargetBranch {
                    project_idx,
                    source_branch,
                } => {
                    let pidx = *project_idx;
                    let source = source_branch.clone();
                    let action = self.show_target_branch(terminal, pidx)?;
                    match action {
                        Some(TargetBranchAction::Select(target)) => {
                            page_stack.push(Page::ExecutionPreview {
                                plan: ExecutionPlan::Merge {
                                    project_idx: pidx,
                                    source_branch: source,
                                    targets: vec![target],
                                },
                            });
                        }
                        Some(TargetBranchAction::Back) => {
                            page_stack.pop();
                        }
                        Some(TargetBranchAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ExecutionPreview { plan } => {
                    let current_plan = plan.clone();
                    let action = self.show_execution_preview(terminal, &current_plan)?;
                    match action {
                        Some(PreviewAction::Confirm) => {
                            let results = self.execute_plan(&current_plan);
                            page_stack.push(Page::ExecuteResult { lines: results });
                        }
                        Some(PreviewAction::Back) => {
                            page_stack.pop();
                        }
                        Some(PreviewAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ExecuteResult { lines } => {
                    let lines = lines.clone();
                    let action = self.show_results(terminal, &lines)?;
                    match action {
                        Some(ResultAction::Back) => {
                            // Pop back to a menu page
                            page_stack.pop();
                            // Also pop the operation page to go back to project select
                            while matches!(
                                page_stack.last(),
                                Some(Page::ExecutionPreview { .. })
                                    | Some(Page::TargetBranchMulti { .. })
                                    | Some(Page::TargetBranch { .. })
                                    | Some(Page::SourceBranch { .. })
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
                    let action = match self.show_gitlab_project_select(terminal) {
                        Ok(action) => action,
                        Err(err) => {
                            page_stack.push(Page::ExecuteResult {
                                lines: vec![(
                                    false,
                                    format!("加载 GitLab 项目列表失败: {err:#}"),
                                )],
                            });
                            continue;
                        }
                    };
                    match action {
                        Some(GitLabAction::Select(id, name)) => match mode {
                            MrMode::Single => {
                                page_stack.push(Page::BranchMapSelect {
                                    project_id: id,
                                    project_name: name,
                                });
                            }
                            MrMode::Batch => {
                                page_stack.push(Page::ExecutionPreview {
                                    plan: ExecutionPlan::MrBatch {
                                        project_id: id,
                                        project_name: name.clone(),
                                        mappings: self.branch_map_without_master(),
                                    },
                                });
                            }
                            MrMode::FixedThree => {
                                page_stack.push(Page::ExecutionPreview {
                                    plan: ExecutionPlan::MrFixedThree {
                                        project_id: id,
                                        project_name: name.clone(),
                                        mappings: self.fixed_three_mappings(),
                                    },
                                });
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
                            page_stack.push(Page::ExecutionPreview {
                                plan: ExecutionPlan::MrSingle {
                                    project_id: pid,
                                    project_name: pname.clone(),
                                    source_branch: src,
                                    target_branch: tgt,
                                },
                            });
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

        let mut menu = MenuState::new("gmux", "终端 Git 工作流工具", items)
            .with_details(details)
            .with_help(vec![
                "本地分支同步 / 合并：用于本地仓库的环境分支同步、批量 merge 和 push。".to_string(),
                "GitLab MR 创建：用于创建单个或批量 Merge Request，并在成功后自动尝试审批与合并。"
                    .to_string(),
                "按 Enter 进入当前选中的功能，按 b 或 Esc 返回，按 q 退出程序。".to_string(),
            ]);

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
        .with_details(details)
        .with_search("输入项目名或路径关键词")
        .with_help(vec![
            "这里显示项目根目录下扫描到的本地 Git 仓库。".to_string(),
            "按 / 搜索项目名或路径关键词，便于仓库较多时快速定位。".to_string(),
            "进入项目后可以继续做本地同步、批量 merge 或单目标 merge。".to_string(),
        ]);

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
            "自定义选择多个目标合并分支".to_string(),
            "将指定分支合并到单个目标合并分支".to_string(),
        ];
        let details = vec![
            vec!["依次更新各环境分支，再同步到对应合并分支并 push。".to_string()],
            vec!["从本地分支列表中选择源分支，再合并到所有目标合并分支并分别 push。".to_string()],
            vec!["从本地分支列表中选择源分支，合并到除最后一个以外的目标合并分支。".to_string()],
            vec!["从目标分支列表中手动勾选多个环境分支，适合灰度或局部回合并。".to_string()],
            vec!["先选择源分支，再选择一个目标合并分支进行 merge + push。".to_string()],
        ];

        let mut menu = MenuState::new("gmux / 本地操作", "上下选择操作类型，Enter 确认", items)
            .with_details(details)
            .with_help(vec![
                "同步：更新各环境分支，再同步到对应合并分支并 push。".to_string(),
                "批量合并：选择一个源分支后，将其 merge 到多个目标合并分支。".to_string(),
                "自定义多选：适合灰度、局部回合并或临时只处理部分环境。".to_string(),
            ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(LocalOpAction::Select(LocalOp::Sync)),
                    MenuAction::Select(1) => Some(LocalOpAction::Select(LocalOp::MergeAll)),
                    MenuAction::Select(2) => Some(LocalOpAction::Select(LocalOp::MergeFixed)),
                    MenuAction::Select(3) => Some(LocalOpAction::Select(LocalOp::MergeCustom)),
                    MenuAction::Select(4) => Some(LocalOpAction::Select(LocalOp::MergeSingle)),
                    MenuAction::Back => Some(LocalOpAction::Back),
                    MenuAction::Quit => Some(LocalOpAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn target_branch_multi_page(&self, project_idx: usize, source_branch: String) -> Result<Page> {
        let project = &self.projects[project_idx];
        let target_options = project::get_target_merge_branches(&self.config, &project.name);
        if target_options.is_empty() {
            return Ok(Page::ExecuteResult {
                lines: vec![(
                    false,
                    format!("项目 {} 没有可用的目标合并分支", project.name),
                )],
            });
        }

        let targets: Vec<String> = target_options
            .iter()
            .map(|(_, target)| target.clone())
            .collect();
        let details: Vec<Vec<String>> = target_options
            .iter()
            .map(|(env, target)| {
                vec![
                    format!("环境分支: {env}"),
                    format!("目标合并分支: {target}"),
                    format!("源分支: {source_branch}"),
                ]
            })
            .collect();

        Ok(Page::TargetBranchMulti {
            project_idx,
            source_branch,
            targets: targets.clone(),
            state: ChecklistState::new(
                "gmux / 目标分支多选",
                "空格勾选多个目标分支，Enter 进入执行预览",
                targets,
            )
            .with_details(details)
            .with_help(vec![
                "用空格勾选一个或多个目标分支。".to_string(),
                "至少需要选择一个目标分支，Enter 后会先进入执行预览，不会立刻执行。".to_string(),
                "适合只对部分环境分支做 merge 的场景。".to_string(),
            ]),
        })
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
        .with_details(details)
        .with_search("输入分支关键词")
        .with_help(vec![
            "源分支是这次 merge 或同步的输入分支。".to_string(),
            "列表来自当前本地仓库的本地分支；可按 / 搜索。".to_string(),
            "进入下一步后，gmux 会先展示执行前检查和完整预览。".to_string(),
        ]);

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
        let help_lines = vec![
            "这里显示本次执行的最终结果，包括成功项和失败项。".to_string(),
            "失败信息会尽量保留原始错误，便于判断是 Git、网络还是 GitLab API 问题。".to_string(),
            "按任意键返回上一层；按 ? 可再次查看这页说明。".to_string(),
        ];
        let mut help_visible = false;

        let mut footer_lines = text_lines.clone();
        footer_lines.push(Line::raw(""));
        footer_lines.push(Line::from(Span::styled(
            "按任意键返回，按 ? 查看说明",
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
                if help_visible {
                    self.render_help_overlay(f, "结果说明", &help_lines);
                }
            })?;

            if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    if help_visible {
                        match key.code {
                            crossterm::event::KeyCode::Char('?')
                            | crossterm::event::KeyCode::Char('b')
                            | crossterm::event::KeyCode::Esc => {
                                help_visible = false;
                                continue;
                            }
                            _ => continue,
                        }
                    }
                    if key.code == crossterm::event::KeyCode::Char('?') {
                        help_visible = true;
                        continue;
                    }
                    return Ok(Some(ResultAction::Back));
                }
            }
        }
    }

    fn show_target_branch(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        project_idx: usize,
    ) -> Result<Option<TargetBranchAction>> {
        let project = &self.projects[project_idx];
        let targets = project::get_target_merge_branches(&self.config, &project.name);
        let items: Vec<String> = targets.iter().map(|(_, target)| target.clone()).collect();
        let details: Vec<Vec<String>> = targets
            .iter()
            .map(|(env, target)| {
                vec![
                    format!("环境分支: {env}"),
                    format!("目标合并分支: {target}"),
                ]
            })
            .collect();

        let mut menu = MenuState::new("gmux / 目标分支", "选择一个目标合并分支", items.clone())
            .with_details(details)
            .with_search("输入环境名或目标分支关键词")
            .with_help(vec![
                "这里选择一个目标合并分支，用于单目标 merge。".to_string(),
                "环境分支和目标合并分支的对应关系由配置文件决定。".to_string(),
                "确认后仍会先进入执行预览，再决定是否真正执行。".to_string(),
            ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(i) => Some(TargetBranchAction::Select(items[i].clone())),
                    MenuAction::Back => Some(TargetBranchAction::Back),
                    MenuAction::Quit => Some(TargetBranchAction::Quit),
                });
            }
        }
    }

    fn show_execution_preview(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        plan: &ExecutionPlan,
    ) -> Result<Option<PreviewAction>> {
        use ratatui::{
            layout::{Constraint, Direction, Layout},
            style::{Color, Modifier, Style},
            text::{Line, Span},
            widgets::{Block, Borders, Paragraph, Wrap},
        };

        let (title, subtitle, lines) = self.build_execution_preview(plan);
        let help_lines = self.build_execution_help(plan);
        let mut help_visible = false;

        loop {
            terminal.draw(|f| {
                let area = f.area();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(8),
                        Constraint::Length(2),
                    ])
                    .split(area);

                let header = Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!("  {title}"),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(Span::styled(
                        format!("  {subtitle}"),
                        Style::default().fg(Color::DarkGray),
                    )),
                ])
                .block(
                    Block::default()
                        .borders(Borders::BOTTOM)
                        .border_style(Style::default().fg(Color::Rgb(81, 81, 81))),
                );

                let body_lines: Vec<Line> = lines
                    .iter()
                    .map(|line| {
                        Line::from(Span::styled(
                            format!("  {line}"),
                            Style::default().fg(Color::Rgb(220, 220, 220)),
                        ))
                    })
                    .collect();

                let body = Paragraph::new(body_lines)
                    .block(
                        Block::default()
                            .title("  Dry Run / 执行预览  ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Rgb(81, 81, 81))),
                    )
                    .wrap(Wrap { trim: false });

                let footer = Paragraph::new(Line::from(vec![
                    Span::styled("  Enter", Style::default().fg(Color::DarkGray)),
                    Span::styled(" 执行  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("?", Style::default().fg(Color::DarkGray)),
                    Span::styled(" 说明  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("b", Style::default().fg(Color::DarkGray)),
                    Span::styled(" 返回  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("q", Style::default().fg(Color::DarkGray)),
                    Span::styled(" 退出", Style::default().fg(Color::DarkGray)),
                ]));

                f.render_widget(header, chunks[0]);
                f.render_widget(body, chunks[1]);
                f.render_widget(footer, chunks[2]);
                if help_visible {
                    self.render_help_overlay(f, "执行说明", &help_lines);
                }
            })?;

            if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                if key.kind != crossterm::event::KeyEventKind::Press {
                    continue;
                }
                if help_visible {
                    match key.code {
                        crossterm::event::KeyCode::Char('?')
                        | crossterm::event::KeyCode::Char('b')
                        | crossterm::event::KeyCode::Esc => {
                            help_visible = false;
                            continue;
                        }
                        crossterm::event::KeyCode::Char('q') => {
                            return Ok(Some(PreviewAction::Quit));
                        }
                        _ => continue,
                    }
                }
                if key.code == crossterm::event::KeyCode::Char('?') {
                    help_visible = true;
                    continue;
                }
                return Ok(match key.code {
                    crossterm::event::KeyCode::Enter => Some(PreviewAction::Confirm),
                    crossterm::event::KeyCode::Char('b') | crossterm::event::KeyCode::Esc => {
                        Some(PreviewAction::Back)
                    }
                    crossterm::event::KeyCode::Char('q') => Some(PreviewAction::Quit),
                    _ => None,
                });
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
            format!("批量创建（前 {} 组映射）", env_count.saturating_sub(1)),
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
            .with_details(details)
            .with_help(vec![
                "单个创建：手动选一组源/目标分支映射创建 MR。".to_string(),
                "批量创建：按分支映射批量创建 MR，并对成功的 MR 自动尝试审批与合并。".to_string(),
                "固定映射：只处理前 N 组固定环境映射，适合标准发布链路。".to_string(),
            ]);

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
            .map(|p| vec![format!("名称: {}", p.name), format!("ID: {}", p.id)])
            .collect();

        let mut menu = MenuState::new(
            "gmux / GitLab 项目",
            "选择一个 GitLab 项目用于创建 MR",
            items,
        )
        .with_details(details)
        .with_search("输入项目名或 ID 关键词")
        .with_help(vec![
            "这里列出当前 token 可访问的 GitLab 项目。".to_string(),
            "按 / 可以按项目名或项目 ID 搜索。".to_string(),
            "如果加载失败，gmux 会留在 TUI 内显示错误，而不会直接退出程序。".to_string(),
        ]);

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

        let mut menu = MenuState::new("gmux / 分支映射", "选择源分支与目标分支的映射关系", items)
            .with_details(details)
            .with_search("输入源分支或目标分支关键词")
            .with_help(vec![
                "这里使用配置文件中的 branch_map 定义源分支和目标分支关系。".to_string(),
                "选择后会先进入 GitLab MR 执行预览，再决定是否真正创建。".to_string(),
                "按 / 可以搜索源分支名或目标分支名。".to_string(),
            ]);

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
            .map(|r| {
                (
                    r.success,
                    format!("{} -> {}: {}", r.branch, r.target, r.message),
                )
            })
            .collect()
    }

    fn targets_for_operation(&self, project_idx: usize, operation: &LocalOp) -> Vec<String> {
        let project = &self.projects[project_idx];
        match operation {
            LocalOp::MergeAll => project::get_target_merge_branches(&self.config, &project.name)
                .into_iter()
                .map(|(_, branch)| branch)
                .collect(),
            LocalOp::MergeFixed => {
                project::get_fixed_target_merge_branches(&self.config, &project.name)
                    .into_iter()
                    .map(|(_, branch)| branch)
                    .collect()
            }
            LocalOp::MergeCustom | LocalOp::MergeSingle | LocalOp::Sync => Vec::new(),
        }
    }

    fn execute_merge(
        &self,
        project_idx: usize,
        source_branch: &str,
        targets: &[String],
    ) -> Vec<(bool, String)> {
        if targets.is_empty() {
            return vec![(false, "未选择目标分支".to_string())];
        }

        let project = &self.projects[project_idx];
        let results = project::merge_to_targets(project, source_branch, &targets);
        results
            .into_iter()
            .map(|r| {
                (
                    r.success,
                    format!("{} -> {}: {}", r.branch, r.target, r.message),
                )
            })
            .collect()
    }

    fn build_execution_preview(&self, plan: &ExecutionPlan) -> (String, String, Vec<String>) {
        match plan {
            ExecutionPlan::Sync { project_idx } => {
                let project = &self.projects[*project_idx];
                let mut lines = vec![
                    format!("项目: {}", project.name),
                    format!("仓库路径: {}", project.path.display()),
                    String::new(),
                    "执行前检查:".to_string(),
                ];
                lines.extend(self.build_sync_preflight(*project_idx));
                lines.push(String::new());
                lines.push("即将执行以下步骤:".to_string());
                for (env, merge) in project::get_target_merge_branches(&self.config, &project.name)
                {
                    lines.push(format!("- 更新环境分支 `{env}` 并 pull 最新代码"));
                    lines.push(format!("- 同步到合并分支 `{merge}` 并 push"));
                }
                lines.push(String::new());
                lines.push("当前只是预览，按 Enter 后才会真正执行。".to_string());

                (
                    "gmux / 执行预览".to_string(),
                    "确认本地同步操作影响范围".to_string(),
                    lines,
                )
            }
            ExecutionPlan::Merge {
                project_idx,
                source_branch,
                targets,
            } => {
                let project = &self.projects[*project_idx];
                let mut lines = vec![
                    format!("项目: {}", project.name),
                    format!("仓库路径: {}", project.path.display()),
                    format!("源分支: {source_branch}"),
                    format!("目标分支数量: {}", targets.len()),
                    String::new(),
                    "执行前检查:".to_string(),
                ];
                lines.extend(self.build_merge_preflight(*project_idx, source_branch, targets));
                lines.push(String::new());
                lines.push("即将执行以下步骤:".to_string());
                for target in targets {
                    lines.push(format!("- checkout `{target}`"));
                    lines.push(format!("- merge `{source_branch}` 到 `{target}`"));
                    lines.push(format!("- push `{target}`"));
                }
                lines.push(String::new());
                lines.push("当前只是预览，按 Enter 后才会真正执行。".to_string());

                (
                    "gmux / 执行预览".to_string(),
                    "确认本地合并操作影响范围".to_string(),
                    lines,
                )
            }
            ExecutionPlan::MrSingle {
                project_id,
                project_name,
                source_branch,
                target_branch,
            } => {
                let lines = vec![
                    format!("GitLab 项目: {project_name}"),
                    format!("项目 ID: {project_id}"),
                    format!("源分支: {source_branch}"),
                    format!("目标分支: {target_branch}"),
                    String::new(),
                    "即将执行以下步骤:".to_string(),
                    format!("- 调用 GitLab API 创建 MR: `{source_branch}` -> `{target_branch}`"),
                    "- 如果创建成功，将继续自动审批并尝试自动合并".to_string(),
                    String::new(),
                    "当前只是预览，按 Enter 后才会真正执行。".to_string(),
                ];

                (
                    "gmux / 执行预览".to_string(),
                    "确认单个 GitLab MR 的创建与后续动作".to_string(),
                    lines,
                )
            }
            ExecutionPlan::MrBatch {
                project_id,
                project_name,
                mappings,
            } => {
                let mut lines = vec![
                    format!("GitLab 项目: {project_name}"),
                    format!("项目 ID: {project_id}"),
                    format!("计划创建 MR 数量: {}", mappings.len()),
                    String::new(),
                    "即将执行以下步骤:".to_string(),
                ];
                for (src, tgt) in mappings {
                    lines.push(format!("- 创建 MR: `{src}` -> `{tgt}`"));
                }
                lines.push("- 对创建成功的 MR 继续自动审批并尝试自动合并".to_string());
                lines.push(String::new());
                lines.push("当前只是预览，按 Enter 后才会真正执行。".to_string());

                (
                    "gmux / 执行预览".to_string(),
                    "确认批量 GitLab MR 的创建与后续动作".to_string(),
                    lines,
                )
            }
            ExecutionPlan::MrFixedThree {
                project_id,
                project_name,
                mappings,
            } => {
                let mut lines = vec![
                    format!("GitLab 项目: {project_name}"),
                    format!("项目 ID: {project_id}"),
                    format!("固定映射 MR 数量: {}", mappings.len()),
                    String::new(),
                    "即将执行以下步骤:".to_string(),
                ];
                for (src, tgt) in mappings {
                    lines.push(format!("- 创建 MR: `{src}` -> `{tgt}`"));
                }
                lines.push("- 对创建成功的 MR 继续自动审批并尝试自动合并".to_string());
                lines.push(String::new());
                lines.push("当前只是预览，按 Enter 后才会真正执行。".to_string());

                (
                    "gmux / 执行预览".to_string(),
                    "确认固定映射 GitLab MR 的创建与后续动作".to_string(),
                    lines,
                )
            }
        }
    }

    fn build_sync_preflight(&self, project_idx: usize) -> Vec<String> {
        let project = &self.projects[project_idx];
        let mut lines = self.build_repo_preflight(project_idx);
        for (env, merge) in project::get_target_merge_branches(&self.config, &project.name) {
            lines.push(self.describe_branch_presence(
                project_idx,
                &env,
                "环境分支",
                "必须可更新并 pull",
            ));
            lines.push(self.describe_branch_presence(
                project_idx,
                &merge,
                "目标合并分支",
                "不存在时会在执行中创建",
            ));
        }
        lines
    }

    fn build_merge_preflight(
        &self,
        project_idx: usize,
        source_branch: &str,
        targets: &[String],
    ) -> Vec<String> {
        let mut lines = self.build_repo_preflight(project_idx);
        lines.push(self.describe_branch_presence(
            project_idx,
            source_branch,
            "源分支",
            "必须存在于本地",
        ));
        lines.push(self.describe_branch_tracking(project_idx, source_branch));
        for target in targets {
            lines.push(self.describe_branch_presence(
                project_idx,
                target,
                "目标合并分支",
                "不存在时会在执行中创建",
            ));
            lines.push(self.describe_branch_tracking(project_idx, target));
        }
        lines
    }

    fn build_repo_preflight(&self, project_idx: usize) -> Vec<String> {
        let project = &self.projects[project_idx];
        let mut lines = Vec::new();

        match git::has_uncommitted_changes(&project.path) {
            Ok(true) => lines.push(
                "[WARN] 工作区存在未提交改动，切换/合并分支时可能失败或引入额外风险".to_string(),
            ),
            Ok(false) => lines.push("[OK] 工作区干净，没有未提交改动".to_string()),
            Err(err) => lines.push(format!("[WARN] 无法检查工作区状态: {err}")),
        }

        match git::current_branch(&project.path) {
            Ok(Some(branch)) => lines.push(format!("[OK] 当前检出分支: {branch}")),
            Ok(None) => lines.push("[WARN] 当前仓库处于 detached HEAD".to_string()),
            Err(err) => lines.push(format!("[WARN] 无法检测当前分支: {err}")),
        }

        lines
    }

    fn describe_branch_presence(
        &self,
        project_idx: usize,
        branch: &str,
        role: &str,
        fallback_hint: &str,
    ) -> String {
        let project = &self.projects[project_idx];
        let local = git::local_branch_exists(&project.path, branch);
        let remote = git::remote_branch_exists(&project.path, branch);

        match (local, remote) {
            (true, true) => format!("[OK] {role} `{branch}` 已存在于本地和 origin"),
            (true, false) => format!("[WARN] {role} `{branch}` 仅存在于本地，origin 上不存在"),
            (false, true) => format!("[OK] {role} `{branch}` 仅存在于 origin，本地将在需要时检出"),
            (false, false) => {
                format!("[WARN] {role} `{branch}` 本地和 origin 都不存在，{fallback_hint}")
            }
        }
    }

    fn describe_branch_tracking(&self, project_idx: usize, branch: &str) -> String {
        let project = &self.projects[project_idx];
        match git::branch_ahead_behind(&project.path, branch) {
            Ok(Some((0, 0))) => format!("[OK] 分支 `{branch}` 与 origin 保持同步"),
            Ok(Some((ahead, 0))) => {
                format!("[WARN] 分支 `{branch}` 比 origin 领先 {ahead} 个提交")
            }
            Ok(Some((0, behind))) => {
                format!("[WARN] 分支 `{branch}` 比 origin 落后 {behind} 个提交")
            }
            Ok(Some((ahead, behind))) => {
                format!("[WARN] 分支 `{branch}` 与 origin 已分叉：领先 {ahead}，落后 {behind}")
            }
            Ok(None) => format!("[INFO] 分支 `{branch}` 缺少本地或远端一侧，无法计算 ahead/behind"),
            Err(err) => format!("[WARN] 无法检查分支 `{branch}` 的远端跟踪状态: {err}"),
        }
    }

    fn build_execution_help(&self, plan: &ExecutionPlan) -> Vec<String> {
        match plan {
            ExecutionPlan::Sync { .. } => vec![
                "同步操作会依次更新环境分支，再同步到对应合并分支并 push。".to_string(),
                "预览中的执行前检查会告诉你工作区是否干净、相关分支是否存在。".to_string(),
                "按 Enter 才会真正开始执行；按 b 返回上一层。".to_string(),
            ],
            ExecutionPlan::Merge { .. } => vec![
                "合并操作会把选定源分支 merge 到一个或多个目标合并分支，并逐个 push。".to_string(),
                "如果目标分支不存在，执行阶段会按当前逻辑自动创建。".to_string(),
                "预览页中的 ahead/behind 信息可以帮助你先判断远端分支状态。".to_string(),
            ],
            ExecutionPlan::MrSingle { .. } => vec![
                "单个 MR 会先创建 Merge Request，再自动尝试审批与合并。".to_string(),
                "这里只有预览，按 Enter 才会真正请求 GitLab API。".to_string(),
                "如果自动审批或自动合并失败，结果页会单独显示失败阶段。".to_string(),
            ],
            ExecutionPlan::MrBatch { .. } => vec![
                "批量 MR 会依次创建多组 MR，再对创建成功的 MR 自动尝试审批与合并。".to_string(),
                "如果其中某组失败，不会阻止其它组继续执行。".to_string(),
                "预览页会先列出这次计划处理的全部映射关系。".to_string(),
            ],
            ExecutionPlan::MrFixedThree { .. } => vec![
                "固定映射 MR 只会处理前 N 组标准环境映射。".to_string(),
                "执行顺序与批量 MR 相同：先创建，再对成功的 MR 自动尝试审批与合并。".to_string(),
                "如果你只想处理部分环境，建议使用单个 MR 或后续扩展的自定义能力。".to_string(),
            ],
        }
    }

    fn render_help_overlay(
        &self,
        frame: &mut ratatui::Frame,
        title: &str,
        lines: &[String],
    ) {
        use ratatui::{
            style::{Color, Modifier, Style},
            text::{Line, Span},
            widgets::{Block, Borders, Clear, Paragraph, Wrap},
        };

        let area = centered_rect(80, 70, frame.area());
        let mut body = vec![
            Line::from(Span::styled(
                "帮助说明",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
        ];

        if lines.is_empty() {
            body.push(Line::from(Span::raw("• 当前页暂无额外说明")));
        } else {
            for line in lines {
                body.push(Line::from(Span::raw(format!("• {line}"))));
            }
        }

        body.push(Line::raw(""));
        body.push(Line::from(Span::styled(
            "? / Esc / b 关闭说明",
            Style::default().fg(Color::DarkGray),
        )));

        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(body)
                .block(
                    Block::default()
                        .title(format!("  {title}  "))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Yellow)),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn branch_map_without_master(&self) -> Vec<(String, String)> {
        let mut mappings: Vec<(String, String)> = self
            .config
            .branch_map
            .iter()
            .filter(|(_, tgt)| tgt.as_str() != "master")
            .map(|(src, tgt)| (src.clone(), tgt.clone()))
            .collect();
        mappings.sort_by(|a, b| a.0.cmp(&b.0));
        mappings
    }

    fn fixed_three_mappings(&self) -> Vec<(String, String)> {
        let env_branches = &self.config.project.env_branches;
        let middle = &self.config.project.merge_branch_middle;
        let count = env_branches.len().saturating_sub(1);
        env_branches[..count]
            .iter()
            .map(|env| (format!("{env}_{middle}_meger"), env.clone()))
            .collect()
    }

    fn execute_plan(&self, plan: &ExecutionPlan) -> Vec<(bool, String)> {
        match plan {
            ExecutionPlan::Sync { project_idx } => self.execute_sync(*project_idx),
            ExecutionPlan::Merge {
                project_idx,
                source_branch,
                targets,
            } => self.execute_merge(*project_idx, source_branch, targets),
            ExecutionPlan::MrSingle {
                project_id,
                project_name,
                source_branch,
                target_branch,
            } => self.execute_mr_single(*project_id, project_name, source_branch, target_branch),
            ExecutionPlan::MrBatch {
                project_id,
                project_name,
                ..
            } => self.execute_mr_batch(*project_id, project_name),
            ExecutionPlan::MrFixedThree {
                project_id,
                project_name,
                ..
            } => self.execute_mr_fixed_three(*project_id, project_name),
        }
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
                results.push((
                    true,
                    format!("MR 创建成功: {} (IID: {})", mr.web_url, mr.iid),
                ));
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

    fn execute_mr_fixed_three(&self, project_id: u64, project_name: &str) -> Vec<(bool, String)> {
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

fn centered_rect(percent_x: u16, percent_y: u16, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Direction, Layout};

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
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

enum TargetBranchAction {
    Select(String),
    Back,
    Quit,
}

enum PreviewAction {
    Confirm,
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
