use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{thread, time::Duration};

pub struct GitLabClient {
    base_url: String,
    token: String,
    client: reqwest::blocking::Client,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitLabProject {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MergeRequest {
    pub id: u64,
    pub iid: u64,
    pub web_url: String,
    pub state: String,
}

impl GitLabClient {
    pub fn new(host: &str, token: &str) -> Self {
        Self {
            base_url: normalize_base_url(host),
            token: token.to_string(),
            client: reqwest::blocking::Client::new(),
        }
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}/api/v4{}", self.base_url, path)
    }

    pub fn list_projects(&self) -> Result<Vec<GitLabProject>> {
        let resp = self
            .client
            .get(self.api_url("/projects?membership=true&per_page=100"))
            .header("PRIVATE-TOKEN", &self.token)
            .send()
            .context("请求 GitLab 项目列表失败")?;

        if !resp.status().is_success() {
            bail!("GitLab API 返回错误: {}", resp.status());
        }

        let projects: Vec<GitLabProject> = resp.json().context("解析项目列表 JSON 失败")?;
        Ok(projects)
    }

    pub fn create_mr(
        &self,
        project_id: u64,
        project_name: &str,
        source_branch: &str,
        target_branch: &str,
    ) -> Result<MergeRequest> {
        let body = serde_json::json!({
            "source_branch": source_branch,
            "target_branch": target_branch,
            "title": format!("Auto MR: {project_name} {source_branch} → {target_branch}"),
            "description": "由 gmux 自动创建的合并请求"
        });

        let resp = self
            .client
            .post(self.api_url(&format!("/projects/{project_id}/merge_requests")))
            .header("PRIVATE-TOKEN", &self.token)
            .json(&body)
            .send()
            .context("创建 MR 请求失败")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("创建 MR 失败 (HTTP {status}): {text}");
        }

        let mr: MergeRequest = resp.json().context("解析 MR 响应失败")?;
        Ok(mr)
    }

    pub fn approve_mr(&self, project_id: u64, mr_iid: u64) -> Result<()> {
        let resp = self
            .client
            .post(self.api_url(&format!(
                "/projects/{project_id}/merge_requests/{mr_iid}/approve"
            )))
            .header("PRIVATE-TOKEN", &self.token)
            .send()
            .context("审批 MR 请求失败")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("审批 MR 失败 (HTTP {status}): {text}");
        }

        Ok(())
    }

    pub fn merge_mr(&self, project_id: u64, mr_iid: u64) -> Result<String> {
        let body = serde_json::json!({
            "merge_when_pipeline_succeeds": false
        });

        let resp = self
            .client
            .put(self.api_url(&format!(
                "/projects/{project_id}/merge_requests/{mr_iid}/merge"
            )))
            .header("PRIVATE-TOKEN", &self.token)
            .json(&body)
            .send()
            .context("合并 MR 请求失败")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            bail!("合并 MR 失败 (HTTP {status}): {text}");
        }

        let result: serde_json::Value = resp.json().context("解析合并响应失败")?;
        let state = result["state"].as_str().unwrap_or("unknown").to_string();
        Ok(state)
    }

    pub fn merge_mr_with_retry(
        &self,
        project_id: u64,
        mr_iid: u64,
        delay_seconds: u64,
        retry_count: u32,
    ) -> Result<Vec<String>> {
        let mut logs = Vec::new();
        let total_attempts = retry_count.saturating_add(1);

        for attempt in 1..=total_attempts {
            if delay_seconds > 0 {
                logs.push(format!("第 {attempt} 次合并前等待 {delay_seconds} 秒"));
                thread::sleep(Duration::from_secs(delay_seconds));
            }

            match self.merge_mr(project_id, mr_iid) {
                Ok(state) if state == "merged" => {
                    logs.push(format!("第 {attempt} 次合并成功"));
                    return Ok(logs);
                }
                Ok(state) => {
                    logs.push(format!("第 {attempt} 次合并返回状态: {state}"));
                    if attempt == total_attempts {
                        bail!("MR 合并状态异常: {state}");
                    }
                }
                Err(err) => {
                    logs.push(format!("第 {attempt} 次合并失败: {err}"));
                    if attempt == total_attempts {
                        return Err(err);
                    }
                }
            }
        }

        bail!("MR 合并流程异常结束")
    }
}

fn normalize_base_url(host: &str) -> String {
    let trimmed = host.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}
