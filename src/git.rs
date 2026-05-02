use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

pub enum MergeResult {
    Success,
    AlreadyUpToDate,
    Conflict { files: Vec<String> },
}

pub fn current_branch(repo_path: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("执行 git symbolic-ref 失败")?;

    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        Ok(None)
    }
}

pub fn list_local_branches(repo_path: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args([
            "for-each-ref",
            "--format=%(refname:short)",
            "--sort=refname",
            "refs/heads",
        ])
        .current_dir(repo_path)
        .output()
        .context("执行 git for-each-ref 失败")?;

    if !output.status.success() {
        bail!(
            "获取分支列表失败: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

pub fn has_uncommitted_changes(repo_path: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .context("执行 git status --porcelain 失败")?;

    if !output.status.success() {
        bail!(
            "检查工作区状态失败: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

pub fn local_branch_exists(repo_path: &Path, branch: &str) -> bool {
    Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(repo_path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn remote_branch_exists(repo_path: &Path, branch: &str) -> bool {
    Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/remotes/origin/{branch}"),
        ])
        .current_dir(repo_path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn check_branch_exists(repo_path: &Path, branch: &str) -> bool {
    let local = local_branch_exists(repo_path, branch);

    if local {
        return true;
    }

    remote_branch_exists(repo_path, branch)
}

pub fn branch_ahead_behind(repo_path: &Path, branch: &str) -> Result<Option<(u32, u32)>> {
    if !local_branch_exists(repo_path, branch) || !remote_branch_exists(repo_path, branch) {
        return Ok(None);
    }

    let output = Command::new("git")
        .args([
            "rev-list",
            "--left-right",
            "--count",
            &format!("{branch}...origin/{branch}"),
        ])
        .current_dir(repo_path)
        .output()
        .context("执行 git rev-list --left-right --count 失败")?;

    if !output.status.success() {
        bail!(
            "检查分支领先/落后状态失败 {branch}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = String::from_utf8_lossy(&output.stdout);
    let mut parts = output.split_whitespace();
    let ahead = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("ahead 计数缺失"))?
        .parse::<u32>()
        .context("解析 ahead 计数失败")?;
    let behind = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("behind 计数缺失"))?
        .parse::<u32>()
        .context("解析 behind 计数失败")?;

    Ok(Some((ahead, behind)))
}

pub fn checkout(repo_path: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["checkout", branch])
        .current_dir(repo_path)
        .output()
        .context("执行 git checkout 失败")?;

    if !output.status.success() {
        bail!(
            "切换分支失败 {branch}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub fn checkout_new_branch(repo_path: &Path, new_branch: &str, from: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["checkout", "-b", new_branch, from])
        .current_dir(repo_path)
        .output()
        .context("执行 git checkout -b 失败")?;

    if !output.status.success() {
        bail!(
            "创建分支失败 {new_branch}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub fn pull(repo_path: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["pull", "origin", branch])
        .current_dir(repo_path)
        .output()
        .context("执行 git pull 失败")?;

    if !output.status.success() {
        bail!(
            "拉取代码失败 {branch}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub fn merge(repo_path: &Path, source_branch: &str) -> Result<MergeResult> {
    let output = Command::new("git")
        .args(["merge", source_branch, "--no-edit"])
        .current_dir(repo_path)
        .output()
        .context("执行 git merge 失败")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("Already up to date") {
            return Ok(MergeResult::AlreadyUpToDate);
        }
        return Ok(MergeResult::Success);
    }

    // Check if it's a conflict
    let merge_head = repo_path.join(".git/MERGE_HEAD");
    if merge_head.exists() {
        let conflict_files = get_conflict_files(repo_path);
        // Abort the merge
        let _ = Command::new("git")
            .args(["merge", "--abort"])
            .current_dir(repo_path)
            .status();
        return Ok(MergeResult::Conflict {
            files: conflict_files,
        });
    }

    bail!(
        "合并失败 {source_branch}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn get_conflict_files(repo_path: &Path) -> Vec<String> {
    Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(repo_path)
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

pub fn push(repo_path: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["push", "origin", branch])
        .current_dir(repo_path)
        .output()
        .context("执行 git push 失败")?;

    if !output.status.success() {
        bail!(
            "推送失败 {branch}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
