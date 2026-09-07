//! Bounded structured confirmations over the external `imsg` CLI.

use crate::config::IMessageChannelConfig;
use crate::models::ConfirmRequest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
use tokio::time::{timeout, Duration};

pub const MIN_TOKEN_CHARS: usize = 4;
pub const MAX_TITLE_CHARS: usize = 60;
pub const MAX_QUESTION_CHARS: usize = 300;
pub const MAX_CONTEXT_FIELDS: usize = 5;
pub const MAX_CONTEXT_VALUE_CHARS: usize = 120;
pub const MIN_CHOICES: usize = 2;
pub const MAX_CHOICES: usize = 6;
pub const MAX_CHOICE_LABEL_CHARS: usize = 80;
pub const MAX_RENDERED_CHARS: usize = 1_200;
pub const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsupportedReason {
    TitleTooLong,
    QuestionTooLong,
    TooManyContextFields,
    ContextValueTooLong,
    ChoiceCount,
    ChoiceLabelTooLong,
    InteractiveInput,
    RenderedTextTooLong,
    RequiredImageUnsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedConfirmation {
    pub token: String,
    pub text: String,
    /// Wire indices into the canonical request choice ledger.
    pub choice_indices: Vec<usize>,
}

pub fn render_confirmation(
    request: &ConfirmRequest,
    token: &str,
    source: &str,
    project: &str,
) -> Result<RenderedConfirmation, UnsupportedReason> {
    if request.title.chars().count() > MAX_TITLE_CHARS {
        return Err(UnsupportedReason::TitleTooLong);
    }
    if request.detail.summary.chars().count() > MAX_QUESTION_CHARS {
        return Err(UnsupportedReason::QuestionTooLong);
    }
    if request.context.len() > MAX_CONTEXT_FIELDS {
        return Err(UnsupportedReason::TooManyContextFields);
    }
    if request
        .context
        .iter()
        .any(|field| field.value.chars().count() > MAX_CONTEXT_VALUE_CHARS)
    {
        return Err(UnsupportedReason::ContextValueTooLong);
    }
    if request.presentation.input().is_some() {
        return Err(UnsupportedReason::InteractiveInput);
    }

    let choices: Vec<_> = request
        .choices
        .iter()
        .enumerate()
        .filter(|(_, choice)| {
            choice
                .variant
                .as_ref()
                .is_none_or(|variant| variant.recommended)
        })
        .collect();
    if !(MIN_CHOICES..=MAX_CHOICES).contains(&choices.len()) {
        return Err(UnsupportedReason::ChoiceCount);
    }
    if choices
        .iter()
        .any(|(_, choice)| choice.label.chars().count() > MAX_CHOICE_LABEL_CHARS)
    {
        return Err(UnsupportedReason::ChoiceLabelTooLong);
    }

    let source = source.trim();
    let source = if source.is_empty() { "Agent" } else { source };
    let mut lines = vec![format!("AskHuman · {source} [{token}]")];
    if !project.trim().is_empty() {
        lines.push(format!("Project: {}", project.trim()));
    }
    lines.push(format!("Action: {}", request.title.trim()));
    if !request.context.is_empty() {
        lines.push(String::new());
        lines.push("Context".into());
        lines.extend(
            request
                .context
                .iter()
                .map(|field| format!("{}: {}", field.label.trim(), field.value.trim())),
        );
    }
    lines.push(String::new());
    lines.push("Question".into());
    lines.push(request.detail.summary.trim().into());
    lines.push(String::new());
    let default = request.presentation.default_action_id();
    for (position, (_, choice)) in choices.iter().enumerate() {
        let recommended = if default == Some(choice.id.as_str()) {
            "  [recommended]"
        } else {
            ""
        };
        lines.push(format!(
            "{}. {}{}",
            position + 1,
            choice.label.trim(),
            recommended
        ));
    }
    lines.push(String::new());
    lines.push(format!("Reply: {token} 1"));
    let text = lines.join("\n");
    if text.chars().count() > MAX_RENDERED_CHARS {
        return Err(UnsupportedReason::RenderedTextTooLong);
    }
    Ok(RenderedConfirmation {
        token: token.to_string(),
        text,
        choice_indices: choices.into_iter().map(|(index, _)| index).collect(),
    })
}

pub fn direct_send_args(recipient: &str, text: &str, image: Option<&Path>) -> Vec<String> {
    let mut args = vec![
        "send".into(),
        "--to".into(),
        recipient.into(),
        "--text".into(),
        text.into(),
        "--service".into(),
        "imessage".into(),
        "--no-sms-fallback".into(),
    ];
    if let Some(image) = image {
        args.push("--file".into());
        args.push(image.to_string_lossy().into_owned());
    }
    args.push("--json".into());
    args
}

pub fn watch_args(chat_id: i64) -> Vec<String> {
    vec![
        "watch".into(),
        "--chat-id".into(),
        chat_id.to_string(),
        "--json".into(),
    ]
}

pub fn parse_reply(text: &str) -> Option<(String, usize)> {
    let mut fields = text.split_whitespace();
    let token = fields.next()?;
    let option = fields.next()?;
    if fields.next().is_some()
        || token.len() < MIN_TOKEN_CHARS
        || token.len() > 64
        || !token
            .chars()
            .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c))
    {
        return None;
    }
    let option = option.parse::<usize>().ok()?.checked_sub(1)?;
    Some((token.to_string(), option))
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct InboundMessage {
    pub chat_id: i64,
    pub is_from_me: bool,
    pub text: Option<String>,
    #[serde(default)]
    pub is_reaction: bool,
    #[serde(
        default,
        rename = "attachments",
        deserialize_with = "attachments_present"
    )]
    pub has_attachments: bool,
}

fn attachments_present<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(!value.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrelatedReply {
    pub request_id: String,
    pub choice_index: usize,
}

struct PendingReply {
    request_id: String,
    chat_id: i64,
    choice_indices: Vec<usize>,
    expires_at_ms: u64,
    terminal: bool,
}

#[derive(Default)]
pub struct PendingReplies {
    by_token: HashMap<String, PendingReply>,
}

impl PendingReplies {
    pub fn register(
        &mut self,
        token: &str,
        request_id: &str,
        chat_id: i64,
        choice_indices: Vec<usize>,
        expires_at_ms: u64,
    ) {
        self.by_token.insert(
            token.to_string(),
            PendingReply {
                request_id: request_id.to_string(),
                chat_id,
                choice_indices,
                expires_at_ms,
                terminal: false,
            },
        );
    }

    pub fn resolve(&mut self, message: &InboundMessage, now_ms: u64) -> Option<CorrelatedReply> {
        if message.is_from_me || message.is_reaction || message.has_attachments {
            return None;
        }
        let (token, option) = parse_reply(message.text.as_deref()?)?;
        let pending = self.by_token.get_mut(&token)?;
        if pending.terminal || now_ms > pending.expires_at_ms || message.chat_id != pending.chat_id
        {
            return None;
        }
        let choice_index = *pending.choice_indices.get(option)?;
        pending.terminal = true;
        Some(CorrelatedReply {
            request_id: pending.request_id.clone(),
            choice_index,
        })
    }
}

#[derive(Default)]
pub struct TokenRegistry {
    by_token: HashMap<String, String>,
}

impl TokenRegistry {
    pub fn allocate(&mut self, request_id: &str) -> String {
        if let Some((token, _)) = self
            .by_token
            .iter()
            .find(|(_, existing)| existing.as_str() == request_id)
        {
            return token.clone();
        }
        for salt in 0_u64.. {
            let material = if salt == 0 {
                request_id.to_string()
            } else {
                format!("{request_id}\0{salt}")
            };
            let digest = format!("{:X}", Sha256::digest(material.as_bytes()));
            for len in (MIN_TOKEN_CHARS..=digest.len()).step_by(2) {
                let token = &digest[..len];
                if !self.by_token.contains_key(token) {
                    self.by_token.insert(token.into(), request_id.into());
                    return token.into();
                }
            }
        }
        unreachable!("the finite active-token set cannot exhaust the SHA-256 namespace")
    }

    pub fn release(&mut self, token: &str, request_id: &str) {
        if self.by_token.get(token).is_some_and(|id| id == request_id) {
            self.by_token.remove(token);
        }
    }
}

pub fn admit_image(
    path: Option<&Path>,
    required_for_decision: bool,
) -> Result<Option<PathBuf>, UnsupportedReason> {
    let Some(path) = path else {
        return Ok(None);
    };
    let supported = (|| {
        let metadata = std::fs::metadata(path).ok()?;
        if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
            return None;
        }
        let mut header = [0_u8; 8];
        let read = std::fs::File::open(path).ok()?.read(&mut header).ok()?;
        let png = read >= 8 && header == *b"\x89PNG\r\n\x1a\n";
        let jpeg = read >= 3 && header[..3] == [0xff, 0xd8, 0xff];
        (png || jpeg).then(|| path.to_path_buf())
    })();
    match (supported, required_for_decision) {
        (Some(path), _) => Ok(Some(path)),
        (None, true) => Err(UnsupportedReason::RequiredImageUnsupported),
        (None, false) => Ok(None),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    NotConfigured,
    ImsgMissing,
    PermissionMissing,
    MessagesUnavailable,
    RecipientNotImessage,
    WatchFailed,
    SendFailed,
    Ready,
}

impl HealthState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::ImsgMissing => "imsg_missing",
            Self::PermissionMissing => "permission_missing",
            Self::MessagesUnavailable => "messages_unavailable",
            Self::RecipientNotImessage => "recipient_not_imessage",
            Self::WatchFailed => "watch_failed",
            Self::SendFailed => "send_failed",
            Self::Ready => "ready",
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatRecord {
    id: i64,
    guid: Option<String>,
    service: String,
    #[serde(default)]
    is_group: bool,
    #[serde(default)]
    participants: Vec<String>,
}

fn configured(config: &IMessageChannelConfig) -> bool {
    config.enabled
        && !config.recipient.trim().is_empty()
        && config.chat_id.is_some()
        && !config.chat_guid.trim().is_empty()
}

fn verified_direct_chat(config: &IMessageChannelConfig, chat: &ChatRecord) -> bool {
    chat.id == config.chat_id.unwrap_or_default()
        && chat.guid.as_deref() == Some(config.chat_guid.trim())
        && !chat.is_group
        && chat.participants.len() == 1
        && chat.participants[0] == config.recipient.trim()
        && chat.service.eq_ignore_ascii_case("imessage")
}

/// Check the documented external CLI and verify that the stored conversation remains a direct
/// iMessage chat with the configured peer. Human-oriented output is never parsed.
pub async fn health(config: &IMessageChannelConfig) -> HealthState {
    if !configured(config) {
        return HealthState::NotConfigured;
    }
    let version = timeout(
        Duration::from_secs(5),
        Command::new("imsg").arg("--version").output(),
    )
    .await;
    match version {
        Ok(Ok(output)) if output.status.success() => {}
        Ok(Ok(output)) => {
            return classify_failure(
                output.status.code(),
                &String::from_utf8_lossy(&output.stderr),
            );
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return HealthState::ImsgMissing;
        }
        Ok(Err(_)) | Err(_) => return HealthState::ImsgMissing,
    }

    let output = match timeout(
        Duration::from_secs(10),
        Command::new("imsg")
            .args(["chats", "--limit", "1000", "--json"])
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return HealthState::ImsgMissing;
        }
        Ok(Err(_)) | Err(_) => return HealthState::MessagesUnavailable,
    };
    if !output.status.success() {
        let classified = classify_failure(
            output.status.code(),
            &String::from_utf8_lossy(&output.stderr),
        );
        return match classified {
            HealthState::SendFailed => HealthState::MessagesUnavailable,
            other => other,
        };
    }
    let wanted_id = config.chat_id.expect("configured chat id");
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(chat) = serde_json::from_str::<ChatRecord>(line) else {
            continue;
        };
        if chat.id != wanted_id {
            continue;
        }
        return if verified_direct_chat(config, &chat) {
            HealthState::Ready
        } else {
            HealthState::RecipientNotImessage
        };
    }
    HealthState::RecipientNotImessage
}

/// Send once through the explicit iMessage-only direct-recipient path. Mutation failures are
/// returned without retry because their delivery disposition may be uncertain.
pub async fn send(
    config: &IMessageChannelConfig,
    text: &str,
    image: Option<&Path>,
) -> Result<String, HealthState> {
    let output = timeout(
        Duration::from_secs(60),
        Command::new("imsg")
            .args(direct_send_args(config.recipient.trim(), text, image))
            .output(),
    )
    .await
    .map_err(|_| HealthState::SendFailed)?
    .map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            HealthState::ImsgMissing
        } else {
            HealthState::SendFailed
        }
    })?;
    if !output.status.success() {
        return Err(classify_failure(
            output.status.code(),
            &String::from_utf8_lossy(&output.stderr),
        ));
    }
    let sent = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| value.get("status").and_then(|v| v.as_str()) == Some("sent"))
        .ok_or(HealthState::SendFailed)?;
    Ok(sent
        .get("message_id")
        .or_else(|| sent.get("guid"))
        .or_else(|| sent.get("id"))
        .map(|value| value.to_string())
        .unwrap_or_else(|| "sent".into()))
}

/// Start a single chat-scoped NDJSON watcher. `kill_on_drop` is defense in depth; callers still
/// explicitly kill and reap the child on every terminal path.
pub fn spawn_watch(chat_id: i64) -> Result<(Child, BufReader<ChildStdout>), HealthState> {
    let mut child = Command::new("imsg")
        .args(watch_args(chat_id))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                HealthState::ImsgMissing
            } else {
                HealthState::WatchFailed
            }
        })?;
    let stdout = child.stdout.take().ok_or(HealthState::WatchFailed)?;
    Ok((child, BufReader::new(stdout)))
}

pub async fn read_inbound_line(
    reader: &mut BufReader<ChildStdout>,
) -> Result<Option<InboundMessage>, HealthState> {
    let mut line = String::new();
    match reader.read_line(&mut line).await {
        Ok(0) => Err(HealthState::WatchFailed),
        Ok(_) => Ok(serde_json::from_str(line.trim()).ok()),
        Err(_) => Err(HealthState::WatchFailed),
    }
}

pub fn classify_failure(exit_code: Option<i32>, stderr: &str) -> HealthState {
    if exit_code.is_none() {
        return HealthState::ImsgMissing;
    }
    let message = stderr.to_ascii_lowercase();
    if message.contains("not available via imessage")
        || message.contains("not reachable via imessage")
    {
        HealthState::RecipientNotImessage
    } else if message.contains("operation not permitted")
        || message.contains("full disk access")
        || message.contains("permission")
    {
        HealthState::PermissionMissing
    } else if message.contains("messages.app") && message.contains("unavailable") {
        HealthState::MessagesUnavailable
    } else {
        HealthState::SendFailed
    }
}

pub fn unix_millis(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confirm::ActionRole;
    use crate::models::{ConfirmChoice, ConfirmDetail, ConfirmPresentation, ConfirmRequest};
    use std::time::{Duration, SystemTime};

    fn request() -> ConfirmRequest {
        ConfirmRequest {
            id: "request-123".into(),
            title: "Delete generated objects".into(),
            context: vec![],
            detail: ConfirmDetail {
                summary: "Continue?".into(),
                body_md: "Optional explanation".into(),
            },
            choices: vec![
                ConfirmChoice {
                    id: "continue".into(),
                    label: "Continue".into(),
                    description: String::new(),
                    role: ActionRole::Primary,
                    variant: None,
                },
                ConfirmChoice {
                    id: "stop".into(),
                    label: "Stop".into(),
                    description: String::new(),
                    role: ActionRole::Destructive,
                    variant: None,
                },
            ],
            presentation: ConfirmPresentation::SingleSelectSubmit {
                input: None,
                submit_label: "Submit".into(),
                default_action_id: Some("continue".into()),
            },
            dismiss_action_id: "stop".into(),
            decision_image: None,
            created_at_ms: 1_000,
            expires_at_ms: 2_000,
        }
    }

    fn imessage_config() -> IMessageChannelConfig {
        IMessageChannelConfig {
            enabled: true,
            recipient: "+15551234567".into(),
            chat_id: Some(42),
            chat_guid: "iMessage;-;+15551234567".into(),
        }
    }

    #[test]
    fn stored_chat_identity_must_be_direct_exact_peer_and_imessage() {
        let mut chat = ChatRecord {
            id: 42,
            guid: Some("iMessage;-;+15551234567".into()),
            service: "iMessage".into(),
            is_group: false,
            participants: vec!["+15551234567".into()],
        };
        assert!(verified_direct_chat(&imessage_config(), &chat));
        chat.service = "SMS".into();
        assert!(!verified_direct_chat(&imessage_config(), &chat));
        chat.service = "iMessage".into();
        chat.is_group = true;
        assert!(!verified_direct_chat(&imessage_config(), &chat));
        chat.is_group = false;
        chat.participants = vec!["+15550000000".into()];
        assert!(!verified_direct_chat(&imessage_config(), &chat));
    }

    #[test]
    fn renderer_preserves_stable_choice_mapping_and_reply_grammar() {
        let rendered = render_confirmation(&request(), "7F32", "Codex", "human-in-loop")
            .expect("supported request");
        assert_eq!(rendered.choice_indices, vec![0, 1]);
        assert!(rendered.text.contains("1. Continue  [recommended]"));
        assert!(rendered.text.ends_with("Reply: 7F32 1"));
        assert!(rendered.text.chars().count() <= MAX_RENDERED_CHARS);
    }

    #[test]
    fn renderer_declines_over_budget_or_input_requests_without_truncating() {
        let mut overlong = request();
        overlong.title = "界".repeat(MAX_TITLE_CHARS + 1);
        assert_eq!(
            render_confirmation(&overlong, "7F32", "Codex", "human-in-loop"),
            Err(UnsupportedReason::TitleTooLong)
        );

        let mut with_input = request();
        with_input.presentation = ConfirmPresentation::SingleSelectSubmit {
            input: Some(crate::models::ConfirmInput {
                id: "comment".into(),
                visible_when_action_id: "continue".into(),
                always_visible: true,
                required: false,
                prefix_chars_by_action_id: Default::default(),
                label: "Comment".into(),
                placeholder: String::new(),
                max_chars: 100,
            }),
            submit_label: "Submit".into(),
            default_action_id: None,
        };
        assert_eq!(
            render_confirmation(&with_input, "7F32", "Codex", "human-in-loop"),
            Err(UnsupportedReason::InteractiveInput)
        );
    }

    #[test]
    fn direct_send_is_always_explicit_imessage_without_sms_fallback() {
        let args = direct_send_args(
            "person@example.com",
            "hello",
            Some(std::path::Path::new("/tmp/evidence.png")),
        );
        assert_eq!(
            args,
            vec![
                "send",
                "--to",
                "person@example.com",
                "--text",
                "hello",
                "--service",
                "imessage",
                "--no-sms-fallback",
                "--file",
                "/tmp/evidence.png",
                "--json",
            ]
        );
        assert!(!args.iter().any(|arg| *arg == "auto" || *arg == "sms"));
    }

    #[test]
    fn reply_parser_requires_exact_token_and_one_based_option() {
        assert_eq!(parse_reply("7F32 2"), Some(("7F32".into(), 1)));
        for invalid in ["2", "7F32", "7F32 0", "7F32 two", "7F32 2 extra", "7f32 2"] {
            assert_eq!(parse_reply(invalid), None, "accepted {invalid:?}");
        }
    }

    #[test]
    fn token_registry_extends_or_rehashes_instead_of_reusing_an_active_token() {
        let request_id = "request-with-forced-prefix-collisions";
        let digest = format!("{:X}", Sha256::digest(request_id.as_bytes()));
        let mut registry = TokenRegistry::default();
        for len in (MIN_TOKEN_CHARS..=digest.len()).step_by(2) {
            registry
                .by_token
                .insert(digest[..len].into(), format!("occupied-{len}"));
        }

        let token = registry.allocate(request_id);
        assert_ne!(token, digest);
        assert_eq!(
            registry.by_token.get(&token).map(String::as_str),
            Some(request_id)
        );
    }

    #[test]
    fn correlation_rejects_wrong_self_stale_late_and_non_text_messages() {
        let mut pending = PendingReplies::default();
        pending.register("7F32", "request-123", 42, vec![0, 1], 2_000);
        let valid = InboundMessage {
            chat_id: 42,
            is_from_me: false,
            text: Some("7F32 2".into()),
            is_reaction: false,
            has_attachments: false,
        };
        assert_eq!(
            pending.resolve(&valid, 1_500),
            Some(CorrelatedReply {
                request_id: "request-123".into(),
                choice_index: 1,
            })
        );
        assert_eq!(pending.resolve(&valid, 1_500), None, "late duplicate");

        for message in [
            InboundMessage {
                chat_id: 7,
                ..valid.clone()
            },
            InboundMessage {
                is_from_me: true,
                ..valid.clone()
            },
            InboundMessage {
                text: None,
                has_attachments: true,
                ..valid.clone()
            },
            InboundMessage {
                is_reaction: true,
                ..valid.clone()
            },
        ] {
            assert_eq!(pending.resolve(&message, 1_500), None);
        }

        pending.register("9ABC", "stale", 42, vec![0, 1], 2_000);
        assert_eq!(
            pending.resolve(
                &InboundMessage {
                    text: Some("9ABC 1".into()),
                    ..valid
                },
                2_001
            ),
            None
        );
    }

    #[test]
    fn image_admission_accepts_one_png_or_jpeg_up_to_five_mib() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("evidence.png");
        std::fs::write(&png, [b"\x89PNG\r\n\x1a\n".as_slice(), &[0; 16]].concat()).unwrap();
        assert_eq!(admit_image(Some(&png), true), Ok(Some(png.clone())));

        let bad = dir.path().join("evidence.gif");
        std::fs::write(&bad, b"GIF89a").unwrap();
        assert_eq!(
            admit_image(Some(&bad), true),
            Err(UnsupportedReason::RequiredImageUnsupported)
        );
        assert_eq!(admit_image(Some(&bad), false), Ok(None));
    }

    #[test]
    fn health_classification_keeps_recipient_boundary_distinct() {
        assert_eq!(classify_failure(None, ""), HealthState::ImsgMissing);
        assert_eq!(
            classify_failure(Some(1), "recipient is not available via iMessage"),
            HealthState::RecipientNotImessage
        );
        assert_eq!(
            classify_failure(Some(1), "operation not permitted for chat.db"),
            HealthState::PermissionMissing
        );
    }

    #[test]
    fn token_is_derived_and_collision_safe_for_active_requests() {
        let mut registry = TokenRegistry::default();
        let first = registry.allocate("request-a");
        let second = registry.allocate("request-a");
        assert_eq!(first, second);
        assert!(first
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));
        assert!(first.len() >= MIN_TOKEN_CHARS);
    }

    #[test]
    fn configured_deadline_helper_uses_wall_clock_only_for_expiry_comparison() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_millis(1_500);
        assert_eq!(unix_millis(now), 1_500);
    }
}
