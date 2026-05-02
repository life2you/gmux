use anyhow::{Context, Result, bail};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::path::Path;
use std::process::Command;

use crate::config::Config;
use crate::git;
use crate::gitlab::GitLabClient;
use crate::project::{self, Project};
use crate::tui::checklist::{ChecklistAction, ChecklistState};
use crate::tui::input::{InputAction, InputState};
use crate::tui::menu::{MenuAction, MenuState};

pub struct App {
    config: Config,
    projects: Vec<Project>,
    gitlab: GitLabClient,
}

#[derive(Clone)]
struct BranchMapDraft {
    original_source: Option<String>,
    source_branch: String,
}

enum Page {
    MainMenu,
    ConfigMenu,
    ConfigProjectRootsMenu,
    ConfigProjectRootActions {
        index: usize,
    },
    ConfigGitLabHostInput {
        state: InputState,
    },
    ConfigGitLabTokenInput {
        state: InputState,
    },
    ConfigProjectRootInput {
        index: Option<usize>,
        state: InputState,
    },
    ConfigMergeMiddleInput {
        state: InputState,
    },
    ConfigEnvBranchesMenu,
    ConfigEnvBranchActions {
        index: usize,
    },
    ConfigEnvBranchDeleteConfirm {
        index: usize,
        branch: String,
        linked_mappings: Vec<String>,
    },
    ConfigEnvBranchInput {
        index: Option<usize>,
        state: InputState,
    },
    ConfigBranchMapMenu,
    ConfigBranchMapActions {
        source_branch: String,
    },
    ConfigBranchMapSourceInput {
        original_source: Option<String>,
        state: InputState,
    },
    ConfigBranchMapTargetSelect {
        draft: BranchMapDraft,
    },
    ConfigBranchMapTargetCustomInput {
        draft: BranchMapDraft,
        state: InputState,
    },
    ConfigBranchMapResetPreview {
        mappings: Vec<(String, String)>,
    },
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
    BranchMapMultiSelect {
        project_id: u64,
        project_name: String,
        state: ChecklistState,
        mappings: Vec<(String, String)>,
    },
}

#[derive(Clone)]
enum LocalOp {
    Sync,
    MergeAll,
    MergeSingle,
    MergeCustom,
}

#[derive(Clone)]
enum MrMode {
    Single,
    Batch,
    BatchCustom,
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
                        Some(MainMenuAction::ConfigManage) => {
                            page_stack.push(Page::ConfigMenu);
                        }
                        Some(MainMenuAction::GitLabMr) => {
                            page_stack.push(Page::MrMenu);
                        }
                        Some(MainMenuAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigMenu => {
                    let action = self.show_config_menu(terminal)?;
                    match action {
                        Some(ConfigMenuAction::EditProjectRoots) => {
                            page_stack.push(Page::ConfigProjectRootsMenu);
                        }
                        Some(ConfigMenuAction::EditGitLabHost) => {
                            page_stack.push(Page::ConfigGitLabHostInput {
                                state: self.config_gitlab_host_input(),
                            });
                        }
                        Some(ConfigMenuAction::EditGitLabToken) => {
                            page_stack.push(Page::ConfigGitLabTokenInput {
                                state: self.config_gitlab_token_input(),
                            });
                        }
                        Some(ConfigMenuAction::EditMergeMiddle) => {
                            page_stack.push(Page::ConfigMergeMiddleInput {
                                state: self.config_merge_middle_input(),
                            });
                        }
                        Some(ConfigMenuAction::EditEnvBranches) => {
                            page_stack.push(Page::ConfigEnvBranchesMenu);
                        }
                        Some(ConfigMenuAction::EditBranchMap) => {
                            page_stack.push(Page::ConfigBranchMapMenu);
                        }
                        Some(ConfigMenuAction::ResetBranchMap) => {
                            page_stack.push(Page::ConfigBranchMapResetPreview {
                                mappings: self.default_branch_map_entries(),
                            });
                        }
                        Some(ConfigMenuAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ConfigMenuAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigProjectRootsMenu => {
                    let action = self.show_config_project_roots_menu(terminal)?;
                    match action {
                        Some(ConfigProjectRootsAction::Add) => {
                            page_stack.push(Page::ConfigProjectRootInput {
                                index: None,
                                state: self.config_project_root_input(None),
                            });
                        }
                        Some(ConfigProjectRootsAction::Select(index)) => {
                            page_stack.push(Page::ConfigProjectRootActions { index });
                        }
                        Some(ConfigProjectRootsAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ConfigProjectRootsAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigProjectRootActions { index } => {
                    let current_index = *index;
                    let action = self.show_config_project_root_actions(terminal, current_index)?;
                    match action {
                        Some(ConfigProjectRootAction::Edit) => {
                            page_stack.push(Page::ConfigProjectRootInput {
                                index: Some(current_index),
                                state: self.config_project_root_input(Some(current_index)),
                            });
                        }
                        Some(ConfigProjectRootAction::Delete) => {
                            if self.config.project.root_dirs.len() <= 1 {
                                page_stack.push(Page::ExecuteResult {
                                    lines: vec![(false, "至少需要保留一个项目根目录".to_string())],
                                });
                            } else {
                                match self.persist_config_change(|config| {
                                    config.project.root_dirs.remove(current_index);
                                }) {
                                    Ok(()) => {
                                        page_stack.pop();
                                    }
                                    Err(err) => {
                                        page_stack.push(Page::ExecuteResult {
                                            lines: vec![(false, format!("删除项目根目录失败: {err:#}"))],
                                        });
                                    }
                                }
                            }
                        }
                        Some(ConfigProjectRootAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ConfigProjectRootAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigProjectRootInput { index, state } => {
                    terminal.draw(|f| state.render(f))?;
                    match state.handle_key_event() {
                        Some(InputAction::Submit(value)) => {
                            match Self::normalize_project_root_input(&value) {
                                Ok(new_value) => {
                                    let edit_index = *index;
                                    match self.persist_config_change(|config| {
                                        match edit_index {
                                            Some(index) => config.project.root_dirs[index] = new_value,
                                            None => config.project.root_dirs.push(new_value),
                                        }
                                        Self::dedupe_root_dirs(&mut config.project.root_dirs);
                                    }) {
                                        Ok(()) => {
                                            page_stack.pop();
                                        }
                                        Err(err) => {
                                            state.error = Some(format!("保存失败: {err:#}"));
                                        }
                                    }
                                }
                                Err(err) => {
                                    state.error = Some(err.to_string());
                                }
                            }
                        }
                        Some(InputAction::PickFolder) => {
                            if let Some(path) = Self::choose_folder_with_dialog("请选择项目根目录")
                            {
                                state.value = path;
                                state.cursor_pos = state.value.len();
                                state.error = None;
                            }
                        }
                        Some(InputAction::Back) => {
                            page_stack.pop();
                        }
                        Some(InputAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigGitLabHostInput { state } => {
                    terminal.draw(|f| state.render(f))?;
                    match state.handle_key_event() {
                        Some(InputAction::Submit(value)) => {
                            let value = value.trim();
                            if value.is_empty() {
                                state.error = Some("GitLab 地址不能为空".to_string());
                            } else {
                                let new_value = value.to_string();
                                match self.persist_config_change(|config| {
                                    config.gitlab.host = new_value;
                                }) {
                                    Ok(()) => {
                                        self.gitlab = GitLabClient::new(
                                            &self.config.gitlab.host,
                                            &self.config.gitlab.token,
                                        );
                                        page_stack.pop();
                                    }
                                    Err(err) => {
                                        state.error = Some(format!("保存失败: {err:#}"));
                                    }
                                }
                            }
                        }
                        Some(InputAction::PickFolder) => {}
                        Some(InputAction::Back) => {
                            page_stack.pop();
                        }
                        Some(InputAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigGitLabTokenInput { state } => {
                    terminal.draw(|f| state.render(f))?;
                    match state.handle_key_event() {
                        Some(InputAction::Submit(value)) => {
                            let value = value.trim();
                            if value.is_empty() {
                                state.error = Some("GitLab Token 不能为空".to_string());
                            } else {
                                let new_value = value.to_string();
                                match self.persist_config_change(|config| {
                                    config.gitlab.token = new_value;
                                }) {
                                    Ok(()) => {
                                        self.gitlab = GitLabClient::new(
                                            &self.config.gitlab.host,
                                            &self.config.gitlab.token,
                                        );
                                        page_stack.pop();
                                    }
                                    Err(err) => {
                                        state.error = Some(format!("保存失败: {err:#}"));
                                    }
                                }
                            }
                        }
                        Some(InputAction::PickFolder) => {}
                        Some(InputAction::Back) => {
                            page_stack.pop();
                        }
                        Some(InputAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigMergeMiddleInput { state } => {
                    terminal.draw(|f| state.render(f))?;
                    match state.handle_key_event() {
                        Some(InputAction::Submit(value)) => {
                            let value = value.trim();
                            if value.is_empty() {
                                state.error = Some("输入不能为空".to_string());
                            } else {
                                let new_value = value.to_string();
                                match self.persist_config_change(|config| {
                                    config.project.merge_branch_middle = new_value;
                                }) {
                                    Ok(()) => {
                                        page_stack.pop();
                                    }
                                    Err(err) => {
                                        state.error = Some(format!("保存失败: {err:#}"));
                                    }
                                }
                            }
                        }
                        Some(InputAction::PickFolder) => {}
                        Some(InputAction::Back) => {
                            page_stack.pop();
                        }
                        Some(InputAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigEnvBranchesMenu => {
                    let action = self.show_config_env_branches_menu(terminal)?;
                    match action {
                        Some(ConfigEnvBranchesAction::Add) => {
                            page_stack.push(Page::ConfigEnvBranchInput {
                                index: None,
                                state: self.config_env_branch_input(None),
                            });
                        }
                        Some(ConfigEnvBranchesAction::Select(index)) => {
                            page_stack.push(Page::ConfigEnvBranchActions { index });
                        }
                        Some(ConfigEnvBranchesAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ConfigEnvBranchesAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigEnvBranchActions { index } => {
                    let current_index = *index;
                    let action = self.show_config_env_branch_actions(terminal, current_index)?;
                    match action {
                        Some(ConfigEnvBranchAction::Rename) => {
                            page_stack.push(Page::ConfigEnvBranchInput {
                                index: Some(current_index),
                                state: self.config_env_branch_input(Some(current_index)),
                            });
                        }
                        Some(ConfigEnvBranchAction::Delete) => {
                            if self.config.project.env_branches.len() <= 1 {
                                page_stack.push(Page::ExecuteResult {
                                    lines: vec![(false, "至少需要保留一个环境分支".to_string())],
                                });
                            } else {
                                let branch =
                                    self.config.project.env_branches[current_index].clone();
                                let linked_mappings = self.linked_branch_map_sources(&branch);
                                page_stack.push(Page::ConfigEnvBranchDeleteConfirm {
                                    index: current_index,
                                    branch,
                                    linked_mappings,
                                });
                            }
                        }
                        Some(ConfigEnvBranchAction::MoveUp) => {
                            if current_index > 0 {
                                match self.persist_config_change(|config| {
                                    config
                                        .project
                                        .env_branches
                                        .swap(current_index, current_index - 1);
                                }) {
                                    Ok(()) => {
                                        *index -= 1;
                                    }
                                    Err(err) => {
                                        page_stack.push(Page::ExecuteResult {
                                            lines: vec![(
                                                false,
                                                format!("调整环境分支顺序失败: {err:#}"),
                                            )],
                                        });
                                    }
                                }
                            }
                        }
                        Some(ConfigEnvBranchAction::MoveDown) => {
                            if current_index + 1 < self.config.project.env_branches.len() {
                                match self.persist_config_change(|config| {
                                    config
                                        .project
                                        .env_branches
                                        .swap(current_index, current_index + 1);
                                }) {
                                    Ok(()) => {
                                        *index += 1;
                                    }
                                    Err(err) => {
                                        page_stack.push(Page::ExecuteResult {
                                            lines: vec![(
                                                false,
                                                format!("调整环境分支顺序失败: {err:#}"),
                                            )],
                                        });
                                    }
                                }
                            }
                        }
                        Some(ConfigEnvBranchAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ConfigEnvBranchAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigEnvBranchDeleteConfirm {
                    index,
                    branch,
                    linked_mappings,
                } => {
                    let current_index = *index;
                    let current_branch = branch.clone();
                    let linked = linked_mappings.clone();
                    let action = self.show_config_env_branch_delete_confirm(
                        terminal,
                        &current_branch,
                        &linked,
                    )?;
                    match action {
                        Some(ConfigEnvBranchDeleteConfirmAction::DeleteOnly) => {
                            match self.persist_config_change(|config| {
                                config.project.env_branches.remove(current_index);
                            }) {
                                Ok(()) => {
                                    page_stack.pop();
                                    page_stack.pop();
                                }
                                Err(err) => {
                                    page_stack.push(Page::ExecuteResult {
                                        lines: vec![(false, format!("删除环境分支失败: {err:#}"))],
                                    });
                                }
                            }
                        }
                        Some(ConfigEnvBranchDeleteConfirmAction::DeleteWithMappings) => {
                            match self.persist_config_change(|config| {
                                config.project.env_branches.remove(current_index);
                                for source in &linked {
                                    config.branch_map.remove(source);
                                }
                            }) {
                                Ok(()) => {
                                    page_stack.pop();
                                    page_stack.pop();
                                }
                                Err(err) => {
                                    page_stack.push(Page::ExecuteResult {
                                        lines: vec![(
                                            false,
                                            format!("删除环境分支及关联映射失败: {err:#}"),
                                        )],
                                    });
                                }
                            }
                        }
                        Some(ConfigEnvBranchDeleteConfirmAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ConfigEnvBranchDeleteConfirmAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigEnvBranchInput { index, state } => {
                    terminal.draw(|f| state.render(f))?;
                    match state.handle_key_event() {
                        Some(InputAction::Submit(value)) => {
                            let branch = value.trim();
                            if branch.is_empty() {
                                state.error = Some("环境分支不能为空".to_string());
                            } else {
                                let new_branch = branch.to_string();
                                match self.persist_config_change(|config| match index {
                                    Some(edit_index) => {
                                        config.project.env_branches[*edit_index] = new_branch;
                                    }
                                    None => {
                                        config.project.env_branches.push(new_branch);
                                    }
                                }) {
                                    Ok(()) => {
                                        page_stack.pop();
                                    }
                                    Err(err) => {
                                        state.error = Some(format!("保存失败: {err:#}"));
                                    }
                                }
                            }
                        }
                        Some(InputAction::PickFolder) => {}
                        Some(InputAction::Back) => {
                            page_stack.pop();
                        }
                        Some(InputAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigBranchMapMenu => {
                    let action = self.show_config_branch_map_menu(terminal)?;
                    match action {
                        Some(ConfigBranchMapMenuAction::Add) => {
                            page_stack.push(Page::ConfigBranchMapSourceInput {
                                original_source: None,
                                state: self.config_branch_map_source_input(None),
                            });
                        }
                        Some(ConfigBranchMapMenuAction::Select(source_branch)) => {
                            page_stack.push(Page::ConfigBranchMapActions { source_branch });
                        }
                        Some(ConfigBranchMapMenuAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ConfigBranchMapMenuAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigBranchMapActions { source_branch } => {
                    let selected_source = source_branch.clone();
                    let action = self.show_config_branch_map_actions(terminal, &selected_source)?;
                    match action {
                        Some(ConfigBranchMapAction::Edit) => {
                            let input_source = selected_source.clone();
                            page_stack.push(Page::ConfigBranchMapSourceInput {
                                original_source: Some(selected_source),
                                state: self
                                    .config_branch_map_source_input(Some(input_source.as_str())),
                            });
                        }
                        Some(ConfigBranchMapAction::Delete) => {
                            match self.persist_config_change(|config| {
                                config.branch_map.remove(&selected_source);
                            }) {
                                Ok(()) => {
                                    page_stack.pop();
                                }
                                Err(err) => {
                                    page_stack.push(Page::ExecuteResult {
                                        lines: vec![(false, format!("删除分支映射失败: {err:#}"))],
                                    });
                                }
                            }
                        }
                        Some(ConfigBranchMapAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ConfigBranchMapAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigBranchMapSourceInput {
                    original_source,
                    state,
                } => {
                    terminal.draw(|f| state.render(f))?;
                    match state.handle_key_event() {
                        Some(InputAction::Submit(value)) => {
                            let source_branch = value.trim();
                            if source_branch.is_empty() {
                                state.error = Some("源分支不能为空".to_string());
                            } else {
                                let draft = BranchMapDraft {
                                    original_source: original_source.clone(),
                                    source_branch: source_branch.to_string(),
                                };
                                page_stack.push(Page::ConfigBranchMapTargetSelect { draft });
                            }
                        }
                        Some(InputAction::PickFolder) => {}
                        Some(InputAction::Back) => {
                            page_stack.pop();
                        }
                        Some(InputAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigBranchMapTargetSelect { draft } => {
                    let current_draft = draft.clone();
                    let action =
                        self.show_config_branch_map_target_select(terminal, &current_draft)?;
                    match action {
                        Some(ConfigBranchMapTargetSelectAction::Select(target_branch)) => {
                            let draft = current_draft.clone();
                            match self.persist_config_change(|config| {
                                if let Some(original) = &draft.original_source {
                                    config.branch_map.remove(original);
                                }
                                config
                                    .branch_map
                                    .insert(draft.source_branch.clone(), target_branch);
                            }) {
                                Ok(()) => {
                                    page_stack.pop();
                                    page_stack.pop();
                                }
                                Err(err) => {
                                    page_stack.push(Page::ExecuteResult {
                                        lines: vec![(false, format!("保存映射失败: {err:#}"))],
                                    });
                                }
                            }
                        }
                        Some(ConfigBranchMapTargetSelectAction::CustomInput) => {
                            page_stack.push(Page::ConfigBranchMapTargetCustomInput {
                                state: self.config_branch_map_target_input(
                                    current_draft.original_source.as_deref(),
                                    &current_draft.source_branch,
                                ),
                                draft: current_draft,
                            });
                        }
                        Some(ConfigBranchMapTargetSelectAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ConfigBranchMapTargetSelectAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigBranchMapTargetCustomInput { draft, state } => {
                    terminal.draw(|f| state.render(f))?;
                    match state.handle_key_event() {
                        Some(InputAction::Submit(value)) => {
                            let target_branch = value.trim();
                            if target_branch.is_empty() {
                                state.error = Some("目标分支不能为空".to_string());
                            } else {
                                let draft = draft.clone();
                                let new_target = target_branch.to_string();
                                match self.persist_config_change(|config| {
                                    if let Some(original) = &draft.original_source {
                                        config.branch_map.remove(original);
                                    }
                                    config
                                        .branch_map
                                        .insert(draft.source_branch.clone(), new_target);
                                }) {
                                    Ok(()) => {
                                        page_stack.pop();
                                        page_stack.pop();
                                    }
                                    Err(err) => {
                                        state.error = Some(format!("保存失败: {err:#}"));
                                    }
                                }
                            }
                        }
                        Some(InputAction::PickFolder) => {}
                        Some(InputAction::Back) => {
                            page_stack.pop();
                        }
                        Some(InputAction::Quit) => break,
                        None => {}
                    }
                }
                Page::ConfigBranchMapResetPreview { mappings } => {
                    let preview = mappings.clone();
                    let action = self.show_config_branch_map_reset_preview(terminal, &preview)?;
                    match action {
                        Some(ConfigBranchMapResetAction::Confirm) => {
                            match self.persist_config_change(|config| {
                                config.regenerate_branch_map();
                            }) {
                                Ok(()) => {
                                    page_stack.pop();
                                }
                                Err(err) => {
                                    page_stack.push(Page::ExecuteResult {
                                        lines: vec![(
                                            false,
                                            format!("重建默认 branch_map 失败: {err:#}"),
                                        )],
                                    });
                                }
                            }
                        }
                        Some(ConfigBranchMapResetAction::Back) => {
                            page_stack.pop();
                        }
                        Some(ConfigBranchMapResetAction::Quit) => break,
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
                                    | Some(Page::BranchMapMultiSelect { .. })
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
                        Some(MrMenuAction::BatchCustom) => {
                            page_stack.push(Page::GitLabProjectSelect {
                                mr_mode: MrMode::BatchCustom,
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
                                lines: vec![(false, format!("加载 GitLab 项目列表失败: {err:#}"))],
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
                                        mappings: self.branch_map_entries(),
                                    },
                                });
                            }
                            MrMode::BatchCustom => {
                                page_stack.push(Page::BranchMapMultiSelect {
                                    project_id: id,
                                    project_name: name.clone(),
                                    state: self.branch_map_multi_state(),
                                    mappings: self.branch_map_entries(),
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
                Page::BranchMapMultiSelect {
                    project_id,
                    project_name,
                    state,
                    mappings,
                } => {
                    let pid = *project_id;
                    let pname = project_name.clone();
                    let mapping_options = mappings.clone();
                    terminal.draw(|f| state.render(f))?;
                    match state.handle_key_event() {
                        Some(ChecklistAction::Submit(indexes)) => {
                            let selected_mappings: Vec<(String, String)> = indexes
                                .into_iter()
                                .filter_map(|index| mapping_options.get(index).cloned())
                                .collect();
                            page_stack.push(Page::ExecutionPreview {
                                plan: ExecutionPlan::MrBatch {
                                    project_id: pid,
                                    project_name: pname,
                                    mappings: selected_mappings,
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
            "配置管理".to_string(),
            "退出程序".to_string(),
        ];
        let details = vec![
            vec!["适合处理本地项目的环境分支同步、批量合并、单分支合并和推送。".to_string()],
            vec!["适合直接创建单个或批量 Merge Request，并支持后续审批合并。".to_string()],
            vec![
                "在 TUI 里直接修改分支相关配置，改动会自动写回 ~/.config/gmux/gmux.toml。"
                    .to_string(),
            ],
            vec!["结束 gmux。".to_string()],
        ];

        let mut menu = MenuState::new("gmux", "终端 Git 工作流工具", items)
            .with_details(details)
            .with_help(vec![
                "本地分支同步 / 合并：用于本地仓库的环境分支同步、批量 merge 和 push。".to_string(),
                "GitLab MR 创建：用于创建单个或批量 Merge Request，并在成功后自动尝试审批与合并。"
                    .to_string(),
                "配置管理：可以直接在界面里调整 merge_branch_middle、env_branches 和 branch_map，改动会自动保存。".to_string(),
                "按 Enter 进入当前选中的功能，按 b 或 Esc 返回，按 q 退出程序。".to_string(),
            ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(MainMenuAction::LocalOps),
                    MenuAction::Select(1) => Some(MainMenuAction::GitLabMr),
                    MenuAction::Select(2) => Some(MainMenuAction::ConfigManage),
                    MenuAction::Select(3) | MenuAction::Back => Some(MainMenuAction::Quit),
                    MenuAction::Quit => Some(MainMenuAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_config_menu(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<ConfigMenuAction>> {
        let items = vec![
            "管理项目根目录".to_string(),
            "管理环境分支".to_string(),
            "管理 MR 映射".to_string(),
            "修改合并分支中间名".to_string(),
            "修改 GitLab 地址".to_string(),
            "修改 GitLab Token".to_string(),
            "预览并重建默认 MR 映射".to_string(),
            "返回主菜单".to_string(),
        ];
        let branch_map_preview = self.branch_map_entries();
        let preview_lines = if branch_map_preview.is_empty() {
            vec!["当前没有映射".to_string()]
        } else {
            branch_map_preview
                .iter()
                .take(4)
                .map(|(src, tgt)| format!("{src} -> {tgt}"))
                .collect()
        };
        let details = vec![
            vec![
                format!("当前共 {} 个项目根目录", self.config.project.root_dirs.len()),
                format!("当前值: {}", self.config.project.root_dirs.join(" | ")),
                "gmux 会汇总扫描这些目录下的本地 Git 仓库。".to_string(),
            ],
            vec![
                format!(
                    "当前共 {} 个环境分支",
                    self.config.project.env_branches.len()
                ),
                format!("当前值: {}", self.config.project.env_branches.join(" ")),
            ],
            {
                let mut lines = vec![format!("当前共 {} 组映射", branch_map_preview.len())];
                lines.extend(preview_lines);
                lines
            },
            vec![
                format!("当前值: {}", self.config.project.merge_branch_middle),
                "会影响默认生成的 merge 分支名和默认 branch_map。".to_string(),
            ],
            vec![
                format!("当前值: {}", self.config.gitlab.host),
                "用于加载 GitLab 项目和创建/审批/合并 MR。".to_string(),
            ],
            vec![
                format!(
                    "当前 Token: {}",
                    Self::mask_token(&self.config.gitlab.token)
                ),
                "修改后后续 GitLab API 请求会立即使用新的 Token。".to_string(),
            ],
            vec![
                "先预览即将生成的默认映射，再决定是否覆盖当前 branch_map。".to_string(),
                "适合在你调整了环境分支或 merge_branch_middle 之后使用。".to_string(),
            ],
            vec![
                format!("配置路径: {}", Config::config_path().display()),
                "这里的改动会立即自动保存并立刻影响后续操作。".to_string(),
            ],
        ];

        let mut menu = MenuState::new(
            "gmux / 配置管理",
            "动态修改分支相关配置，改动会自动保存",
            items,
        )
        .with_details(details)
        .with_help(vec![
            "这里更像一个工作流设置台：改完会自动保存并立刻生效。".to_string(),
            "项目根目录支持多目录配置，适合把不同业务线或不同工作区一起纳入扫描。".to_string(),
            "环境分支列表决定本地同步和本地合并的目标分支集合。".to_string(),
            "MR 映射决定 GitLab 批量创建 MR 时会使用哪些 source -> target 关系。".to_string(),
            "这里的修改会自动写回 ~/.config/gmux/gmux.toml，不需要额外手动保存。".to_string(),
        ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(ConfigMenuAction::EditProjectRoots),
                    MenuAction::Select(1) => Some(ConfigMenuAction::EditEnvBranches),
                    MenuAction::Select(2) => Some(ConfigMenuAction::EditBranchMap),
                    MenuAction::Select(3) => Some(ConfigMenuAction::EditMergeMiddle),
                    MenuAction::Select(4) => Some(ConfigMenuAction::EditGitLabHost),
                    MenuAction::Select(5) => Some(ConfigMenuAction::EditGitLabToken),
                    MenuAction::Select(6) => Some(ConfigMenuAction::ResetBranchMap),
                    MenuAction::Select(7) => Some(ConfigMenuAction::Back),
                    MenuAction::Back => Some(ConfigMenuAction::Back),
                    MenuAction::Quit => Some(ConfigMenuAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_config_env_branches_menu(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<ConfigEnvBranchesAction>> {
        let mut items = vec!["新增环境分支".to_string()];
        items.extend(self.config.project.env_branches.iter().cloned());
        items.push("返回配置管理".to_string());

        let mut details = vec![vec!["添加一个新的环境分支条目。".to_string()]];
        details.extend(self.config.project.env_branches.iter().enumerate().map(
            |(index, branch)| {
                vec![
                    format!("当前顺序: {}", index + 1),
                    format!("分支名: {branch}"),
                    "进入后可以重命名、删除或调整顺序。".to_string(),
                ]
            },
        ));
        details.push(vec!["返回上一层配置管理。".to_string()]);

        let mut menu = MenuState::new(
            "gmux / 环境分支管理",
            "逐条管理 env_branches，不再需要一整行输入",
            items,
        )
        .with_details(details)
        .with_help(vec![
            "这里管理本地同步和本地合并会使用到的环境分支列表。".to_string(),
            "你可以逐条新增、重命名、删除，或调整顺序。".to_string(),
            "如果你还需要同步修改 GitLab MR 映射，去配置管理里的 branch_map 再调整。".to_string(),
        ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                let branch_count = self.config.project.env_branches.len();
                return Ok(match action {
                    MenuAction::Select(0) => Some(ConfigEnvBranchesAction::Add),
                    MenuAction::Select(i) if i == branch_count + 1 => {
                        Some(ConfigEnvBranchesAction::Back)
                    }
                    MenuAction::Select(i) if i > 0 && i <= branch_count => {
                        Some(ConfigEnvBranchesAction::Select(i - 1))
                    }
                    MenuAction::Back => Some(ConfigEnvBranchesAction::Back),
                    MenuAction::Quit => Some(ConfigEnvBranchesAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_config_project_roots_menu(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<ConfigProjectRootsAction>> {
        let mut items = vec!["新增项目根目录".to_string()];
        items.extend(
            self.config
                .project
                .root_dirs
                .iter()
                .enumerate()
                .map(|(index, root)| format!("目录 {}: {}", index + 1, Self::root_label(root))),
        );
        items.push("返回配置管理".to_string());

        let mut details = vec![vec!["添加新的项目扫描目录。".to_string()]];
        details.extend(self.config.project.root_dirs.iter().map(|root| {
            vec![
                format!("完整路径: {root}"),
                "进入后可以编辑或删除这个项目根目录。".to_string(),
            ]
        }));
        details.push(vec!["返回上一层配置管理。".to_string()]);

        let mut menu = MenuState::new(
            "gmux / 项目根目录",
            "管理一个或多个项目扫描目录",
            items,
        )
        .with_details(details)
        .with_search("过滤项目根目录")
        .with_help(vec![
            "这里管理 gmux 会扫描哪些本地工作目录。".to_string(),
            "支持同时配置多个项目根目录，适合不同业务线或不同代码区。".to_string(),
            "如果多个目录里有同名仓库，项目选择页会显示来源目录帮助你区分。".to_string(),
        ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                let root_count = self.config.project.root_dirs.len();
                return Ok(match action {
                    MenuAction::Select(0) => Some(ConfigProjectRootsAction::Add),
                    MenuAction::Select(i) if i == root_count + 1 => {
                        Some(ConfigProjectRootsAction::Back)
                    }
                    MenuAction::Select(i) if i > 0 && i <= root_count => {
                        Some(ConfigProjectRootsAction::Select(i - 1))
                    }
                    MenuAction::Back => Some(ConfigProjectRootsAction::Back),
                    MenuAction::Quit => Some(ConfigProjectRootsAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_config_project_root_actions(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        index: usize,
    ) -> Result<Option<ConfigProjectRootAction>> {
        let root = self
            .config
            .project
            .root_dirs
            .get(index)
            .cloned()
            .unwrap_or_default();
        let mut items = vec!["编辑路径".to_string()];
        let mut details = vec![vec![
            format!("当前路径: {root}"),
            "重新指定这个项目根目录。".to_string(),
        ]];
        if self.config.project.root_dirs.len() > 1 {
            items.push("删除目录".to_string());
            details.push(vec![
                format!("当前路径: {root}"),
                "从扫描列表中移除这个项目根目录。".to_string(),
            ]);
        }
        items.push("返回项目根目录列表".to_string());
        details.push(vec!["返回项目根目录列表。".to_string()]);

        let mut menu = MenuState::new(
            "gmux / 项目根目录操作",
            "选择对当前项目根目录执行的操作",
            items,
        )
        .with_details(details)
        .with_help(vec![
            "每个项目根目录都可以单独编辑或删除。".to_string(),
            "至少需要保留一个项目根目录。".to_string(),
        ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(ConfigProjectRootAction::Edit),
                    MenuAction::Select(1) if self.config.project.root_dirs.len() > 1 => {
                        Some(ConfigProjectRootAction::Delete)
                    }
                    MenuAction::Select(_) => Some(ConfigProjectRootAction::Back),
                    MenuAction::Back => Some(ConfigProjectRootAction::Back),
                    MenuAction::Quit => Some(ConfigProjectRootAction::Quit),
                });
            }
        }
    }

    fn show_config_env_branch_actions(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        index: usize,
    ) -> Result<Option<ConfigEnvBranchAction>> {
        let branch = self
            .config
            .project
            .env_branches
            .get(index)
            .cloned()
            .unwrap_or_default();
        let items = vec![
            "重命名".to_string(),
            "上移".to_string(),
            "下移".to_string(),
            "删除".to_string(),
            "返回环境分支列表".to_string(),
        ];
        let details = vec![
            vec![format!("当前分支: {branch}")],
            vec![format!("当前顺序: {}", index + 1)],
            vec![format!("当前顺序: {}", index + 1)],
            vec!["删除当前环境分支。".to_string()],
            vec!["返回环境分支列表。".to_string()],
        ];

        let mut menu = MenuState::new(
            "gmux / 环境分支操作",
            &format!("正在管理环境分支: {branch}"),
            items,
        )
        .with_details(details)
        .with_help(vec![
            "重命名：只修改这个环境分支自己的名称。".to_string(),
            "上移/下移：调整分支顺序，影响列表展示和默认映射生成顺序。".to_string(),
            "删除时如果有映射仍然指向这个环境分支，gmux 会继续问你要不要一起清理。".to_string(),
        ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(ConfigEnvBranchAction::Rename),
                    MenuAction::Select(1) => Some(ConfigEnvBranchAction::MoveUp),
                    MenuAction::Select(2) => Some(ConfigEnvBranchAction::MoveDown),
                    MenuAction::Select(3) => Some(ConfigEnvBranchAction::Delete),
                    MenuAction::Select(4) => Some(ConfigEnvBranchAction::Back),
                    MenuAction::Back => Some(ConfigEnvBranchAction::Back),
                    MenuAction::Quit => Some(ConfigEnvBranchAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_config_branch_map_menu(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<ConfigBranchMapMenuAction>> {
        let mappings = self.branch_map_entries();
        let mut items = vec!["新增映射".to_string()];
        items.extend(mappings.iter().map(|(src, tgt)| format!("{src} -> {tgt}")));
        items.push("返回配置管理".to_string());

        let mut details = vec![vec!["添加一组新的 GitLab MR 映射。".to_string()]];
        details.extend(mappings.iter().map(|(src, tgt)| {
            vec![
                format!("源分支: {src}"),
                format!("目标分支: {tgt}"),
                "进入后可以编辑或删除这组映射。".to_string(),
            ]
        }));
        details.push(vec!["返回上一层配置管理。".to_string()]);

        let mut menu = MenuState::new(
            "gmux / branch_map 管理",
            "逐条管理 GitLab MR 的源分支与目标分支映射",
            items,
        )
        .with_details(details)
        .with_search("输入源分支或目标分支关键词")
        .with_help(vec![
            "这里管理 GitLab MR 使用的 branch_map。".to_string(),
            "批量 MR 会按这里的映射逐组创建。".to_string(),
            "你可以逐条新增、编辑或删除，不需要再手写整行映射文本。".to_string(),
        ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                let mapping_count = mappings.len();
                return Ok(match action {
                    MenuAction::Select(0) => Some(ConfigBranchMapMenuAction::Add),
                    MenuAction::Select(i) if i == mapping_count + 1 => {
                        Some(ConfigBranchMapMenuAction::Back)
                    }
                    MenuAction::Select(i) if i > 0 && i <= mapping_count => {
                        Some(ConfigBranchMapMenuAction::Select(mappings[i - 1].0.clone()))
                    }
                    MenuAction::Back => Some(ConfigBranchMapMenuAction::Back),
                    MenuAction::Quit => Some(ConfigBranchMapMenuAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_config_branch_map_actions(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        source_branch: &str,
    ) -> Result<Option<ConfigBranchMapAction>> {
        let target_branch = self
            .config
            .branch_map
            .get(source_branch)
            .cloned()
            .unwrap_or_default();
        let items = vec![
            "编辑映射".to_string(),
            "删除映射".to_string(),
            "返回映射列表".to_string(),
        ];
        let details = vec![
            vec![
                format!("源分支: {source_branch}"),
                format!("目标分支: {target_branch}"),
            ],
            vec!["删除当前这组映射。".to_string()],
            vec!["返回 branch_map 列表。".to_string()],
        ];

        let mut menu = MenuState::new(
            "gmux / 映射操作",
            &format!("正在管理映射: {source_branch} -> {target_branch}"),
            items,
        )
        .with_details(details)
        .with_help(vec![
            "编辑映射：修改当前这组 source -> target。".to_string(),
            "删除映射后，这组关系不会再出现在单个 MR、批量 MR 和多选 MR 中。".to_string(),
        ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(ConfigBranchMapAction::Edit),
                    MenuAction::Select(1) => Some(ConfigBranchMapAction::Delete),
                    MenuAction::Select(2) => Some(ConfigBranchMapAction::Back),
                    MenuAction::Back => Some(ConfigBranchMapAction::Back),
                    MenuAction::Quit => Some(ConfigBranchMapAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_config_env_branch_delete_confirm(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        branch: &str,
        linked_mappings: &[String],
    ) -> Result<Option<ConfigEnvBranchDeleteConfirmAction>> {
        let items = if linked_mappings.is_empty() {
            vec!["确认删除环境分支".to_string(), "取消".to_string()]
        } else {
            vec![
                format!("删除环境分支，并删除 {} 组关联映射", linked_mappings.len()),
                "只删除环境分支，保留现有映射".to_string(),
                "取消".to_string(),
            ]
        };

        let details = if linked_mappings.is_empty() {
            vec![
                vec![format!("将删除环境分支: {branch}")],
                vec!["不执行删除。".to_string()],
            ]
        } else {
            vec![
                {
                    let mut lines = vec![format!("将删除环境分支: {branch}")];
                    lines.extend(
                        linked_mappings
                            .iter()
                            .map(|source| format!("关联映射: {source} -> {branch}")),
                    );
                    lines
                },
                vec![
                    format!("将删除环境分支: {branch}"),
                    "当前 branch_map 仍然会保留这些映射。".to_string(),
                ],
                vec!["不执行删除。".to_string()],
            ]
        };

        let mut menu = MenuState::new(
            "gmux / 删除环境分支",
            &format!("你正在删除环境分支: {branch}"),
            items,
        )
        .with_details(details)
        .with_help(vec![
            "如果一个环境分支仍然被 branch_map 当作目标分支引用，最好一起清理对应映射。"
                .to_string(),
            "保留映射也不是不可以，但后续批量 MR 可能会继续用到已经不在环境列表里的目标分支。"
                .to_string(),
        ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match (linked_mappings.is_empty(), action) {
                    (true, MenuAction::Select(0)) => {
                        Some(ConfigEnvBranchDeleteConfirmAction::DeleteOnly)
                    }
                    (true, MenuAction::Select(1)) => Some(ConfigEnvBranchDeleteConfirmAction::Back),
                    (false, MenuAction::Select(0)) => {
                        Some(ConfigEnvBranchDeleteConfirmAction::DeleteWithMappings)
                    }
                    (false, MenuAction::Select(1)) => {
                        Some(ConfigEnvBranchDeleteConfirmAction::DeleteOnly)
                    }
                    (false, MenuAction::Select(2)) => {
                        Some(ConfigEnvBranchDeleteConfirmAction::Back)
                    }
                    (_, MenuAction::Back) => Some(ConfigEnvBranchDeleteConfirmAction::Back),
                    (_, MenuAction::Quit) => Some(ConfigEnvBranchDeleteConfirmAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_config_branch_map_target_select(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        draft: &BranchMapDraft,
    ) -> Result<Option<ConfigBranchMapTargetSelectAction>> {
        let existing_target = draft
            .original_source
            .as_deref()
            .and_then(|source| self.config.branch_map.get(source))
            .cloned();
        let mut items = Vec::new();
        let mut details = Vec::new();

        if let Some(target) = existing_target.as_ref() {
            if !self
                .config
                .project
                .env_branches
                .iter()
                .any(|env| env == target)
            {
                items.push(format!("保留当前自定义目标分支: {target}"));
                details.push(vec![
                    format!("源分支: {}", draft.source_branch),
                    format!("目标分支: {target}"),
                ]);
            }
        }

        for env in &self.config.project.env_branches {
            items.push(format!("使用环境分支: {env}"));
            details.push(vec![
                format!("源分支: {}", draft.source_branch),
                format!("目标分支: {env}"),
            ]);
        }

        items.push("手动输入其他目标分支".to_string());
        details.push(vec![
            format!("源分支: {}", draft.source_branch),
            "适合目标分支不在环境分支列表中的场景。".to_string(),
        ]);
        items.push("返回上一步".to_string());
        details.push(vec!["返回源分支编辑页。".to_string()]);

        let custom_offset = if existing_target.as_ref().is_some_and(|target| {
            !self
                .config
                .project
                .env_branches
                .iter()
                .any(|env| env == target)
        }) {
            1
        } else {
            0
        };

        let mut menu = MenuState::new(
            "gmux / 选择目标分支",
            &format!("为 `{}` 选择目标分支", draft.source_branch),
            items,
        )
        .with_details(details)
        .with_help(vec![
            "优先直接从环境分支列表里选目标分支，这样最不容易拼错。".to_string(),
            "只有目标分支不在环境分支列表里时，再使用“手动输入其他目标分支”。".to_string(),
        ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                let env_start = custom_offset;
                let env_end = env_start + self.config.project.env_branches.len();
                let custom_index = env_end;
                let back_index = custom_index + 1;
                return Ok(match action {
                    MenuAction::Select(0) if custom_offset == 1 => {
                        Some(ConfigBranchMapTargetSelectAction::Select(
                            existing_target.clone().unwrap_or_default(),
                        ))
                    }
                    MenuAction::Select(i) if i >= env_start && i < env_end => {
                        Some(ConfigBranchMapTargetSelectAction::Select(
                            self.config.project.env_branches[i - env_start].clone(),
                        ))
                    }
                    MenuAction::Select(i) if i == custom_index => {
                        Some(ConfigBranchMapTargetSelectAction::CustomInput)
                    }
                    MenuAction::Select(i) if i == back_index => {
                        Some(ConfigBranchMapTargetSelectAction::Back)
                    }
                    MenuAction::Back => Some(ConfigBranchMapTargetSelectAction::Back),
                    MenuAction::Quit => Some(ConfigBranchMapTargetSelectAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_config_branch_map_reset_preview(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        mappings: &[(String, String)],
    ) -> Result<Option<ConfigBranchMapResetAction>> {
        let items = vec!["确认覆盖当前映射".to_string(), "取消".to_string()];
        let mut preview_lines = vec![
            format!("当前映射数: {}", self.config.branch_map.len()),
            format!("新默认映射数: {}", mappings.len()),
        ];
        preview_lines.extend(
            mappings
                .iter()
                .take(8)
                .map(|(src, tgt)| format!("{src} -> {tgt}")),
        );
        let details = vec![
            preview_lines,
            vec!["取消本次重建，保持当前 branch_map 不变。".to_string()],
        ];

        let mut menu = MenuState::new(
            "gmux / 预览默认 MR 映射",
            "确认是否用新的默认映射覆盖当前 branch_map",
            items,
        )
        .with_details(details)
        .with_help(vec![
            "这一步会用当前环境分支列表和 merge_branch_middle 重新生成默认 MR 映射。".to_string(),
            "如果你已经手工定制过 branch_map，确认前先看清楚会生成哪些新映射。".to_string(),
        ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(ConfigBranchMapResetAction::Confirm),
                    MenuAction::Select(1) => Some(ConfigBranchMapResetAction::Back),
                    MenuAction::Back => Some(ConfigBranchMapResetAction::Back),
                    MenuAction::Quit => Some(ConfigBranchMapResetAction::Quit),
                    _ => None,
                });
            }
        }
    }

    fn show_project_select(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<ProjectAction>> {
        let items: Vec<String> = self.projects.iter().map(|p| p.display_name.clone()).collect();
        let details: Vec<Vec<String>> = self
            .projects
            .iter()
            .map(|p| {
                let mut d = vec![
                    format!("名称: {}", p.name),
                    format!("路径: {}", p.path.display()),
                    format!("来源目录: {}", p.source_root.display()),
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
            "这里显示所有项目根目录下扫描到的本地 Git 仓库。".to_string(),
            "如果多个工作目录里有同名仓库，项目名后会带上来源目录。".to_string(),
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
            "将指定分支合并到单个目标合并分支".to_string(),
            "自定义选择多个目标合并分支".to_string(),
        ];
        let details = vec![
            vec!["依次更新各环境分支，再同步到对应合并分支并 push。".to_string()],
            vec!["从本地分支列表中选择源分支，再合并到所有目标合并分支并分别 push。".to_string()],
            vec!["先选择源分支，再选择一个目标合并分支进行 merge + push。".to_string()],
            vec!["从目标分支列表中手动勾选多个环境分支，适合灰度或局部回合并。".to_string()],
        ];

        let mut menu = MenuState::new("gmux / 本地操作", "上下选择操作类型，Enter 确认", items)
            .with_details(details)
            .with_help(vec![
                "同步：更新各环境分支，再同步到对应合并分支并 push。".to_string(),
                "批量合并：选择一个源分支后，将其 merge 到全部目标合并分支。".to_string(),
                "自定义多选：适合灰度、局部回合并或临时只处理部分环境。".to_string(),
            ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(LocalOpAction::Select(LocalOp::Sync)),
                    MenuAction::Select(1) => Some(LocalOpAction::Select(LocalOp::MergeAll)),
                    MenuAction::Select(2) => Some(LocalOpAction::Select(LocalOp::MergeSingle)),
                    MenuAction::Select(3) => Some(LocalOpAction::Select(LocalOp::MergeCustom)),
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
        let items = vec![
            "单个创建".to_string(),
            format!("批量创建（全部 {} 组映射）", self.config.branch_map.len()),
            "自定义选择多组映射".to_string(),
            "返回主菜单".to_string(),
        ];
        let details = vec![
            vec!["先选项目，再选一组源/目标分支映射，创建一个 MR。".to_string()],
            vec!["按配置中的 branch_map，对一个项目批量创建全部映射对应的 MR，并自动尝试审批和合并。".to_string()],
            vec!["从 branch_map 中手动勾选部分映射，适合只处理部分环境或临时链路。".to_string()],
            vec!["不执行 MR 操作。".to_string()],
        ];

        let mut menu = MenuState::new("gmux / MR 模式", "选择 MR 处理方式", items)
            .with_details(details)
            .with_help(vec![
                "单个创建：手动选一组源/目标分支映射创建 MR。".to_string(),
                "批量创建：按配置中的全部分支映射批量创建 MR，并对成功的 MR 自动尝试审批与合并。"
                    .to_string(),
                "自定义多选：只处理你本次勾选的映射，适合非标准发布链路。".to_string(),
            ]);

        loop {
            terminal.draw(|f| menu.render(f))?;
            if let Some(action) = menu.handle_key_event() {
                return Ok(match action {
                    MenuAction::Select(0) => Some(MrMenuAction::Single),
                    MenuAction::Select(1) => Some(MrMenuAction::Batch),
                    MenuAction::Select(2) => Some(MrMenuAction::BatchCustom),
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
        self.projects = project::scan_projects(&self.config.project.root_dirs)?;
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
            LocalOp::MergeSingle | LocalOp::MergeCustom | LocalOp::Sync => Vec::new(),
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
        }
    }

    fn render_help_overlay(&self, frame: &mut ratatui::Frame, title: &str, lines: &[String]) {
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

    fn branch_map_entries(&self) -> Vec<(String, String)> {
        let mut mappings: Vec<(String, String)> = self
            .config
            .branch_map
            .iter()
            .map(|(src, tgt)| (src.clone(), tgt.clone()))
            .collect();
        mappings.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        mappings
    }

    fn branch_map_multi_state(&self) -> ChecklistState {
        let mappings = self.branch_map_entries();
        let items: Vec<String> = mappings
            .iter()
            .map(|(src, tgt)| format!("{src} -> {tgt}"))
            .collect();
        let details: Vec<Vec<String>> = mappings
            .iter()
            .map(|(src, tgt)| vec![format!("源分支: {src}"), format!("目标分支: {tgt}")])
            .collect();

        ChecklistState::new(
            "gmux / 分支映射多选",
            "空格勾选多组映射，Enter 进入执行预览",
            items,
        )
        .with_details(details)
        .with_help(vec![
            "这里展示配置文件 branch_map 中的全部映射关系。".to_string(),
            "用空格勾选一组或多组映射，适合只处理部分环境或临时链路。".to_string(),
            "Enter 后会先进入执行预览，不会立刻请求 GitLab API。".to_string(),
        ])
    }

    fn linked_branch_map_sources(&self, env_branch: &str) -> Vec<String> {
        let mut sources: Vec<String> = self
            .config
            .branch_map
            .iter()
            .filter_map(|(src, tgt)| (tgt == env_branch).then_some(src.clone()))
            .collect();
        sources.sort();
        sources
    }

    fn default_branch_map_entries(&self) -> Vec<(String, String)> {
        let mut mappings: Vec<(String, String)> = self
            .config
            .project
            .env_branches
            .iter()
            .map(|env| {
                (
                    format!("{}_{}_meger", env, self.config.project.merge_branch_middle),
                    env.clone(),
                )
            })
            .collect();
        mappings.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        mappings
    }

    fn mask_token(token: &str) -> String {
        if token.is_empty() {
            return "(未设置)".to_string();
        }
        let prefix_len = token.len().min(6);
        format!("{}****", &token[..prefix_len])
    }

    fn normalize_project_root_input(value: &str) -> Result<String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            bail!("项目根目录不能为空");
        }
        let path = Path::new(trimmed);
        if !path.is_dir() {
            bail!("项目根目录不存在: {}", path.display());
        }
        let canonical = path
            .canonicalize()
            .with_context(|| format!("规范化项目根目录失败: {}", path.display()))?;
        Ok(canonical.display().to_string())
    }

    fn choose_folder_with_dialog(prompt: &str) -> Option<String> {
        if !cfg!(target_os = "macos") {
            return None;
        }

        let output = Command::new("osascript")
            .arg("-e")
            .arg("try")
            .arg("-e")
            .arg(format!(
                "POSIX path of (choose folder with prompt \"{}\")",
                prompt.replace('"', "\\\"")
            ))
            .arg("-e")
            .arg("on error number -128")
            .arg("-e")
            .arg("return \"\"")
            .arg("-e")
            .arg("end try")
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    }

    fn dedupe_root_dirs(root_dirs: &mut Vec<String>) {
        let mut seen = std::collections::HashSet::new();
        root_dirs.retain(|root| seen.insert(root.clone()));
    }

    fn root_label(root: &str) -> String {
        Path::new(root)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| root.to_string())
    }

    fn config_project_root_input(&self, index: Option<usize>) -> InputState {
        let mut state = InputState::new(
            "gmux / 配置管理",
            "新增或编辑一个项目根目录",
            "项目根目录",
            "例如 /Users/you/workspaces",
        )
        .with_file_picker();
        if let Some(index) = index {
            state.value = self
                .config
                .project
                .root_dirs
                .get(index)
                .cloned()
                .unwrap_or_default();
        }
        state.cursor_pos = state.value.len();
        state
    }

    fn config_gitlab_host_input(&self) -> InputState {
        let mut state = InputState::new(
            "gmux / 配置管理",
            "修改 GitLab 地址",
            "GitLab Host",
            "例如 gitlab.example.com:8099",
        );
        state.value = self.config.gitlab.host.clone();
        state.cursor_pos = state.value.len();
        state
    }

    fn config_gitlab_token_input(&self) -> InputState {
        let mut state = InputState::new(
            "gmux / 配置管理",
            "修改 GitLab Token",
            "GitLab Token",
            "例如 glpat-xxxxxxxxxxxx",
        );
        state.value = self.config.gitlab.token.clone();
        state.cursor_pos = state.value.len();
        state
    }

    fn config_merge_middle_input(&self) -> InputState {
        let mut state = InputState::new(
            "gmux / 配置管理",
            "修改 merge_branch_middle",
            "合并分支中间名",
            "例如 henry 或 PROJECT_NAME",
        );
        state.value = self.config.project.merge_branch_middle.clone();
        state.cursor_pos = state.value.len();
        state
    }

    fn config_env_branch_input(&self, index: Option<usize>) -> InputState {
        let mut state = InputState::new(
            "gmux / 配置管理",
            "逐条编辑环境分支",
            "环境分支名称",
            "例如 dev / test / uat / stage / prod",
        );
        if let Some(index) = index {
            state.value = self
                .config
                .project
                .env_branches
                .get(index)
                .cloned()
                .unwrap_or_default();
        }
        state.cursor_pos = state.value.len();
        state
    }

    fn config_branch_map_source_input(&self, source_branch: Option<&str>) -> InputState {
        let mut state = InputState::new(
            "gmux / 配置管理",
            "编辑 branch_map / 第一步",
            "源分支",
            "例如 pre_prod 或 uat_henry_meger",
        );
        if let Some(src) = source_branch {
            state.value = src.to_string();
        }
        state.cursor_pos = state.value.len();
        state
    }

    fn config_branch_map_target_input(
        &self,
        original_source: Option<&str>,
        source_branch: &str,
    ) -> InputState {
        let mut state = InputState::new(
            "gmux / 配置管理",
            &format!("编辑 branch_map / 自定义目标  [源分支: {source_branch}]"),
            "目标分支",
            "例如 master / stage / prod",
        );
        if let Some(original_source) = original_source {
            if let Some(target) = self.config.branch_map.get(original_source) {
                state.value = target.clone();
            }
        }
        state.cursor_pos = state.value.len();
        state
    }

    fn save_current_config(&self) -> Result<std::path::PathBuf> {
        let path = Config::config_path();
        self.config.validate()?;
        self.config.save(&path)?;
        Ok(path)
    }

    fn persist_config_change<F>(&mut self, mutate: F) -> Result<()>
    where
        F: FnOnce(&mut Config),
    {
        let previous = self.config.clone();
        mutate(&mut self.config);
        if let Err(err) = self.save_current_config() {
            self.config = previous;
            return Err(err);
        }
        Ok(())
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
                mappings,
            } => self.execute_mr_batch(*project_id, project_name, mappings),
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

    fn execute_mr_batch(
        &self,
        project_id: u64,
        project_name: &str,
        mappings: &[(String, String)],
    ) -> Vec<(bool, String)> {
        let mut results = Vec::new();
        let mut mr_list: Vec<(u64, String, String)> = Vec::new();

        if mappings.is_empty() {
            return vec![(false, "未选择任何分支映射".to_string())];
        }

        for (src, tgt) in mappings {
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
}

fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    area: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
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
    ConfigManage,
    LocalOps,
    GitLabMr,
    Quit,
}

enum ConfigMenuAction {
    EditProjectRoots,
    EditGitLabHost,
    EditGitLabToken,
    EditMergeMiddle,
    EditEnvBranches,
    EditBranchMap,
    ResetBranchMap,
    Back,
    Quit,
}

enum ConfigProjectRootsAction {
    Add,
    Select(usize),
    Back,
    Quit,
}

enum ConfigProjectRootAction {
    Edit,
    Delete,
    Back,
    Quit,
}

enum ConfigEnvBranchesAction {
    Add,
    Select(usize),
    Back,
    Quit,
}

enum ConfigEnvBranchAction {
    Rename,
    MoveUp,
    MoveDown,
    Delete,
    Back,
    Quit,
}

enum ConfigEnvBranchDeleteConfirmAction {
    DeleteOnly,
    DeleteWithMappings,
    Back,
    Quit,
}

enum ConfigBranchMapMenuAction {
    Add,
    Select(String),
    Back,
    Quit,
}

enum ConfigBranchMapAction {
    Edit,
    Delete,
    Back,
    Quit,
}

enum ConfigBranchMapTargetSelectAction {
    Select(String),
    CustomInput,
    Back,
    Quit,
}

enum ConfigBranchMapResetAction {
    Confirm,
    Back,
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
    BatchCustom,
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
