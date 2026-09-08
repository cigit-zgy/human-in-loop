//! 更新日志（release notes）获取与聚合。
//!
//! 统一从 GitHub Releases 按 tag 取 `body`（markdown）。聚合为**懒加载**：仅在用户
//! 展开查看时调用，拉一次 `/releases` 列表并过滤「当前版本→最新版本」之间的版本。

use super::{github_api_url, github_client, github_status_error, normalize_version};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;

/// 最新版日志（`/releases/latest` 的 body）。
pub async fn latest_notes() -> Result<String> {
    let resp = github_client()
        .get(github_api_url("releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("GitHub API 请求失败")?;
    if !resp.status().is_success() {
        return Err(github_status_error(resp.status()));
    }
    let release = resp
        .json::<Value>()
        .await
        .context("解析 release JSON 失败")?;
    Ok(release["body"].as_str().unwrap_or("").to_string())
}

/// 指定版本（tag `v<version>`）的日志 body；不存在则 Err。
pub async fn notes_for_tag(version: &str) -> Result<String> {
    let tag = format!("v{}", normalize_version(version));
    let resp = github_client()
        .get(github_api_url(&format!("releases/tags/{}", tag)))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("GitHub API 请求失败")?;
    if !resp.status().is_success() {
        // 404 表示该 tag 没有 release（占位即可）；403/429 归一为限流标记。
        if resp.status().as_u16() == 404 {
            return Err(anyhow!("未找到 {} 的发布说明", tag));
        }
        return Err(github_status_error(resp.status()));
    }
    let release = resp
        .json::<Value>()
        .await
        .context("解析 release JSON 失败")?;
    Ok(release["body"].as_str().unwrap_or("").to_string())
}

/// 聚合「(from, to] 区间」内所有版本的日志（懒加载，单次拉取 `/releases` 列表）。
/// `from`=本地当前版本、`to`=最新版本；按版本从新到旧拼接，每段带小标题。
pub async fn aggregated_notes(from: &str, to: &str) -> Result<String> {
    let resp = github_client()
        .get(github_api_url("releases?per_page=100"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("GitHub API 请求失败")?;
    if !resp.status().is_success() {
        return Err(github_status_error(resp.status()));
    }
    let releases = resp
        .json::<Value>()
        .await
        .context("解析 releases JSON 失败")?;
    let list = releases.as_array().cloned().unwrap_or_default();
    Ok(aggregate(&list, from, to))
}

/// 纯函数：从 releases 列表拼接 (from, to] 区间内的日志（便于单测）。
fn aggregate(list: &[Value], from: &str, to: &str) -> String {
    let mut picked: Vec<(String, String)> = Vec::new();
    for r in list {
        if r.get("prerelease")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        if r.get("draft").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let tag = r.get("tag_name").and_then(|v| v.as_str()).unwrap_or("");
        let ver = normalize_version(tag);
        if ver.is_empty() {
            continue;
        }
        // 区间 (from, to]：ver > from 且 ver <= to。
        if super::compare_versions(&ver, from) > 0 && super::compare_versions(&ver, to) <= 0 {
            let body = r
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            picked.push((ver, body));
        }
    }
    // 从新到旧。
    picked.sort_by(|a, b| super::compare_versions(&b.0, &a.0).cmp(&0));
    picked
        .into_iter()
        .map(|(ver, body)| format!("## v{}\n\n{}", ver, body.trim()))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rel(tag: &str, body: &str) -> Value {
        json!({ "tag_name": tag, "body": body, "prerelease": false, "draft": false })
    }

    #[test]
    fn aggregate_picks_interval_newest_first() {
        let list = vec![
            rel("v0.6.0", "six"),
            rel("v0.5.4", "five-four"),
            rel("v0.5.3", "five-three"),
            rel("v0.5.2", "five-two"),
        ];
        // 当前 0.5.2 → 最新 0.6.0：应含 0.6.0/0.5.4/0.5.3，排除 0.5.2(=from)。
        let out = aggregate(&list, "0.5.2", "0.6.0");
        assert!(out.contains("## v0.6.0"));
        assert!(out.contains("## v0.5.4"));
        assert!(out.contains("## v0.5.3"));
        assert!(!out.contains("## v0.5.2"));
        // 从新到旧：0.6.0 在 0.5.4 之前。
        let i6 = out.find("## v0.6.0").unwrap();
        let i54 = out.find("## v0.5.4").unwrap();
        assert!(i6 < i54);
    }

    #[test]
    fn aggregate_skips_prerelease_and_draft() {
        let list = vec![
            json!({ "tag_name": "v0.6.0-rc.1", "body": "rc", "prerelease": true, "draft": false }),
            json!({ "tag_name": "v0.6.0", "body": "draft", "prerelease": false, "draft": true }),
            rel("v0.5.4", "stable"),
        ];
        let out = aggregate(&list, "0.5.3", "0.6.0");
        assert!(out.contains("## v0.5.4"));
        assert!(!out.contains("rc"));
        assert!(!out.contains("draft"));
    }
}
