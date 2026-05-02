use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::git::{self, MergeResult};

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
}

pub fn scan_projects(root_dir: &str) -> Result<Vec<Project>> {
    let root = Path::new(root_dir);
    if !root.is_dir() {
        bail!("项目根目录不存在: {root_dir}");
    }

    let mut projects = Vec::new();

    let entries = std::fs::read_dir(root)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join(".git").is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        projects.push(Project { name, path });
    }

    projects.sort_by(|a, b| a.name.cmp(&b.name));

    if projects.is_empty() {
        bail!("在目录 {root_dir} 中未找到任何 Git 仓库");
    }

    Ok(projects)
}

#[derive(Debug)]
pub struct BranchOpResult {
    pub branch: String,
    pub target: String,
    pub success: bool,
    pub message: String,
}

pub fn sync_and_push(project: &Project, config: &Config) -> Vec<BranchOpResult> {
    let mut results = Vec::new();

    for env_branch in &config.project.env_branches {
        let merge_branch = config.get_merge_branch_name(env_branch, &project.name);

        // Step 1: update env branch
        let update_result = update_branch(&project.path, env_branch);
        if let Err(e) = update_result {
            results.push(BranchOpResult {
                branch: env_branch.clone(),
                target: merge_branch,
                success: false,
                message: format!("更新分支失败: {e}"),
            });
            continue;
        }

        // Step 2: sync to merge branch and push
        match sync_to_merge_branch(&project.path, env_branch, &merge_branch) {
            Ok(msg) => {
                results.push(BranchOpResult {
                    branch: env_branch.clone(),
                    target: merge_branch,
                    success: true,
                    message: msg,
                });
            }
            Err(e) => {
                results.push(BranchOpResult {
                    branch: env_branch.clone(),
                    target: merge_branch,
                    success: false,
                    message: format!("{e}"),
                });
            }
        }
    }

    results
}

fn update_branch(repo_path: &Path, branch: &str) -> Result<()> {
    if !git::check_branch_exists(repo_path, branch) {
        bail!("分支 {branch} 不存在");
    }
    git::checkout(repo_path, branch)?;
    git::pull(repo_path, branch)?;
    Ok(())
}

fn sync_to_merge_branch(repo_path: &Path, source: &str, merge_branch: &str) -> Result<String> {
    if !git::check_branch_exists(repo_path, merge_branch) {
        git::checkout_new_branch(repo_path, merge_branch, source)?;
        git::push(repo_path, merge_branch)?;
        return Ok(format!("创建并推送新分支: {merge_branch}"));
    }

    git::checkout(repo_path, merge_branch)?;

    match git::merge(repo_path, source)? {
        MergeResult::Success => {
            git::push(repo_path, merge_branch)?;
            Ok(format!("同步完成: {source} -> {merge_branch}"))
        }
        MergeResult::AlreadyUpToDate => Ok(format!("已是最新: {source} -> {merge_branch}")),
        MergeResult::Conflict { files } => {
            let file_list = if files.is_empty() {
                "（无法获取冲突文件）".to_string()
            } else {
                files.join(", ")
            };
            bail!("合并冲突已自动中止: {source} -> {merge_branch}，冲突文件: {file_list}");
        }
    }
}

pub fn merge_to_targets(
    project: &Project,
    source_branch: &str,
    targets: &[String],
) -> Vec<BranchOpResult> {
    let mut results = Vec::new();

    for target in targets {
        match sync_to_merge_branch(&project.path, source_branch, target) {
            Ok(msg) => {
                results.push(BranchOpResult {
                    branch: source_branch.to_string(),
                    target: target.clone(),
                    success: true,
                    message: msg,
                });
            }
            Err(e) => {
                results.push(BranchOpResult {
                    branch: source_branch.to_string(),
                    target: target.clone(),
                    success: false,
                    message: format!("{e}"),
                });
            }
        }
    }

    results
}

pub fn get_target_merge_branches(config: &Config, project_name: &str) -> Vec<(String, String)> {
    config
        .project
        .env_branches
        .iter()
        .map(|env| {
            let merge_branch = config.get_merge_branch_name(env, project_name);
            (env.clone(), merge_branch)
        })
        .collect()
}
