//! Bounded informational delivery using the existing configured channel transports.

use super::imessage;
use crate::config::AppConfig;
use crate::models::{
    HumanNotification, NotificationChannel, NotificationDeliveryStatus, NotificationResult,
};
use futures_util::{stream::FuturesUnordered, StreamExt};
use std::time::Duration;

pub const DISPATCH_TIMEOUT: Duration = Duration::from_secs(180);

pub fn compact(value: &str, field: &str, limit: usize) -> Result<String, String> {
    if value.chars().any(|c| c.is_control() && !c.is_whitespace()) {
        return Err(format!("{field} contains unsupported control characters"));
    }
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() || value.chars().count() > limit {
        return Err(format!("{field} must contain 1 to {limit} characters"));
    }
    Ok(value)
}

pub fn identifier(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 128 || value.chars().any(char::is_control) {
        return Err(format!(
            "{field} must contain 1 to 128 characters without controls"
        ));
    }
    Ok(value.to_string())
}

pub fn locator(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 240 || value.chars().any(char::is_control) {
        return Err("locator must contain 1 to 240 characters without controls".into());
    }
    let parsed = reqwest::Url::parse(value);
    let prefix = value.split(':').next().unwrap_or_default();
    if parsed.is_err()
        && (value.contains("://")
            || prefix.eq_ignore_ascii_case("http")
            || prefix.eq_ignore_ascii_case("https"))
    {
        return Err("locator URL is invalid".into());
    }
    if let Ok(url) = parsed {
        if matches!(url.scheme(), "http" | "https") {
            if !url.username().is_empty() || url.password().is_some() {
                return Err("locator URL must not contain credentials".into());
            }
        } else {
            if matches!(
                url.scheme(),
                "javascript"
                    | "vbscript"
                    | "data"
                    | "file"
                    | "mailto"
                    | "about"
                    | "blob"
                    | "ftp"
                    | "ftps"
                    | "ws"
                    | "wss"
            ) {
                return Err("locator URL must use HTTP(S) without credentials".into());
            }
            let (_, suffix) = value.split_once(':').ok_or("locator URL is invalid")?;
            let commit =
                (7..=40).contains(&suffix.len()) && suffix.bytes().all(|c| c.is_ascii_hexdigit());
            let line = !suffix.is_empty()
                && suffix.bytes().all(|c| c.is_ascii_digit())
                && value
                    .split(':')
                    .next()
                    .is_some_and(|path| path.contains('.') || path.contains('/'));
            if !commit && !line {
                return Err("locator URL must use HTTP(S) without credentials".into());
            }
        }
    }
    Ok(value.to_string())
}

/// Preserve every supplied field. Over-budget content fails before any channel mutation.
pub fn render(notification: &HumanNotification) -> Result<String, String> {
    identifier(&notification.notification_id, "notification_id")?;
    let source = compact(&notification.source_agent, "source_agent", 80)?;
    let source = match crate::project::repository_identity(&notification.project) {
        crate::project::RepositoryIdentity::Github(repository) => {
            format!("{source} · {repository}")
        }
        crate::project::RepositoryIdentity::NonRepository => source,
        crate::project::RepositoryIdentity::Unavailable => {
            return Err("canonical GitHub repository identity is unavailable".into())
        }
    };
    let source = compact(&source, "source and repository", 80)?;
    let mut lines = vec![format!("[HIL · {}]", notification.status.as_str()), source];
    if let Some(task_id) = &notification.task_id {
        lines.push(identifier(task_id, "task_id")?);
    }
    lines.push(String::new());
    lines.push(compact(&notification.summary, "summary", 160)?);
    if notification.context.len() > 2 {
        return Err("context supports at most 2 compact fields".into());
    }
    for field in &notification.context {
        let label = compact(&field.label, "context label", 80)?;
        let value = compact(&field.value, "context value", 80)?;
        lines.push(compact(&format!("{label}: {value}"), "context field", 80)?);
    }
    if let Some(value) = &notification.locator {
        lines.push(locator(value)?);
    }
    let text = lines.join("\n");
    if text.chars().count() > imessage::MAX_RENDERED_CHARS {
        return Err("notification exceeds the 700-character compact surface".into());
    }
    Ok(text)
}

pub fn result(
    notification_id: String,
    outcomes: &[(NotificationChannel, bool)],
) -> NotificationResult {
    let sent = outcomes.iter().filter(|(_, sent)| *sent).count();
    NotificationResult {
        notification_id,
        delivery_status: if sent == 0 {
            NotificationDeliveryStatus::Failed
        } else if sent == outcomes.len() {
            NotificationDeliveryStatus::Sent
        } else {
            NotificationDeliveryStatus::Partial
        },
        channel_ids: outcomes.iter().map(|(channel, _)| *channel).collect(),
    }
}

pub async fn dispatch(
    notification: &HumanNotification,
    config: &AppConfig,
    candidates: &[&str],
) -> NotificationResult {
    let Ok(text) = render(notification) else {
        return result(notification.notification_id.clone(), &[]);
    };
    let mut deliveries = FuturesUnordered::new();
    for channel in candidates {
        let text = &text;
        deliveries.push(async move {
            let kind = match *channel {
                "feishu" => NotificationChannel::Feishu,
                "imessage" => NotificationChannel::Imessage,
                _ => return None,
            };
            let sent = tokio::time::timeout(DISPATCH_TIMEOUT, async {
                match kind {
                    NotificationChannel::Feishu => {
                        let Ok(client) =
                            crate::feishu::client::FeishuClient::new(&config.channels.feishu)
                        else {
                            return false;
                        };
                        if client.open_id().is_empty() {
                            return false;
                        }
                        client.send_text(text).await.is_ok_and(|id| !id.is_empty())
                    }
                    NotificationChannel::Imessage => {
                        let channel = &config.channels.imessage;
                        if crate::channels::imessage_worker::required_for(channel.identity_mode) {
                            return crate::channels::imessage_worker::notify(channel, text)
                                .await
                                .is_ok();
                        }
                        let Ok(readiness) = imessage::prepare(channel).await else {
                            return false;
                        };
                        let Ok(boundary) = imessage::pre_send_boundary(&readiness).await else {
                            return false;
                        };
                        let Ok(receipt) = imessage::send(channel, text, None).await else {
                            return false;
                        };
                        let Ok(resolved) = imessage::resolve_after_send(
                            channel, &readiness, &boundary, &receipt, text,
                        )
                        .await
                        else {
                            return false;
                        };
                        imessage::persist_resolved_chat(channel, &resolved).is_ok()
                    }
                }
            })
            .await
            .unwrap_or(false);
            Some((kind, sent))
        });
    }
    let mut outcomes = Vec::new();
    while let Some(outcome) = deliveries.next().await {
        if let Some(outcome) = outcome {
            outcomes.push(outcome);
        }
    }
    outcomes.sort_by_key(|(channel, _)| match channel {
        NotificationChannel::Feishu => 0,
        NotificationChannel::Imessage => 1,
    });
    result(notification.notification_id.clone(), &outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NotificationField, NotificationStatus};

    fn notification() -> HumanNotification {
        HumanNotification {
            notification_id: "notice-1".into(),
            project: String::new(),
            source_agent: "Codex".into(),
            status: NotificationStatus::PassWithLimitations,
            summary: "Completed with one limitation.".into(),
            context: vec![NotificationField {
                label: "Scope".into(),
                value: "Local verification".into(),
            }],
            task_id: Some("TASK-1".into()),
            locator: Some("reports/codex/result.md".into()),
        }
    }

    #[test]
    fn compact_notification_preserves_all_fields_without_a_reply_surface() {
        let text = render(&notification()).unwrap();
        assert_eq!(text, "[HIL · PASS_WITH_LIMITATIONS]\nCodex\nTASK-1\n\nCompleted with one limitation.\nScope: Local verification\nreports/codex/result.md");
        assert!(!text.contains("Reply:"));
        assert!(!text.contains("1  "));
    }

    #[test]
    fn notification_rejects_unsafe_or_oversized_fields_without_truncation() {
        for invalid in ["", "\0", &"界".repeat(161)] {
            let mut notice = notification();
            notice.summary = invalid.into();
            assert!(render(&notice).is_err());
        }
        for invalid in [
            "",
            "https://user:secret@example.org/report",
            "https:user:secret@example.org/report",
            "https:/user:secret@example.org/report",
            "https://user:secret@bad host/report",
            "javascript:alert(1)",
            "javascript:deadbeef",
            "javascript://alert",
            "report\nPASS",
            &"x".repeat(241),
        ] {
            assert!(locator(invalid).is_err());
        }
        assert_eq!(locator("reports/a  b.md").unwrap(), "reports/a  b.md");
        assert!(locator("task/notification:0123456789abcdef").is_ok());
        assert!(locator("master:0123456789abcdef").is_ok());
        assert!(locator("result.md:17").is_ok());
        let mut notice = notification();
        notice.context = vec![notice.context[0].clone(); 3];
        assert!(render(&notice).is_err());
        notice.context.clear();
        notice.summary = "界".repeat(160);
        assert!(render(&notice).unwrap().contains(&notice.summary));
    }

    #[tokio::test]
    async fn no_channel_dispatch_is_immediate_and_creates_no_decision() {
        let registry = crate::daemon::request::RequestRegistry::new();
        let outcome = tokio::time::timeout(
            Duration::from_secs(1),
            dispatch(&notification(), &AppConfig::default(), &[]),
        )
        .await
        .unwrap();
        assert_eq!(outcome.delivery_status, NotificationDeliveryStatus::Failed);
        assert!(outcome.channel_ids.is_empty());
        assert_eq!(registry.active_count(), 0);
    }

    #[test]
    fn dispatch_status_reports_mixed_failures_truthfully() {
        use NotificationChannel::{Feishu, Imessage};
        assert_eq!(
            result("n".into(), &[(Imessage, true)]).delivery_status,
            NotificationDeliveryStatus::Sent
        );
        assert_eq!(
            result("n".into(), &[(Feishu, false), (Imessage, true)]).delivery_status,
            NotificationDeliveryStatus::Partial
        );
        assert_eq!(
            result("n".into(), &[(Imessage, false)]).delivery_status,
            NotificationDeliveryStatus::Failed
        );
    }
}
