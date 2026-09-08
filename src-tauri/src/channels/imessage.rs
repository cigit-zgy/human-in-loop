//! Bounded structured confirmations over the external `imsg` CLI.

use crate::config::{AppConfig, IMessageChannelConfig, IMessageIdentityMode};
use crate::models::ConfirmRequest;
use serde::de::DeserializeOwned;
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
pub const MAX_SOURCE_PROJECT_CHARS: usize = 80;
pub const MAX_QUESTION_CHARS: usize = 160;
pub const MAX_CONTEXT_FIELDS: usize = 2;
pub const MAX_CONTEXT_LINE_CHARS: usize = 80;
pub const MIN_CHOICES: usize = 2;
pub const MAX_CHOICES: usize = 6;
pub const MAX_CHOICE_LABEL_CHARS: usize = 60;
pub const MAX_RENDERED_CHARS: usize = 700;
pub const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
pub(crate) const SUPPORTED_IMSG_VERSION: &str = "0.15.1";
pub(crate) const IMSG_EXECUTABLE_ENV: &str = "HUMAN_IN_LOOP_IMSG_EXECUTABLE";
const CHAT_SCAN_LIMIT: usize = 10_000;
const CHAT_SCAN_TIMEOUT: Duration = Duration::from_secs(90);
const POST_SEND_CHAT_LIMIT: usize = 20;

fn imsg_program_from_override(value: Option<std::ffi::OsString>) -> std::ffi::OsString {
    value
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| std::ffi::OsString::from("imsg"))
}

fn imsg_program() -> std::ffi::OsString {
    imsg_program_from_override(std::env::var_os(IMSG_EXECUTABLE_ENV))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsupportedReason {
    SourceProjectLineTooLong,
    QuestionTooLong,
    TooManyContextFields,
    ContextLineTooLong,
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

fn compact_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn render_confirmation(
    request: &ConfirmRequest,
    token: &str,
    source: &str,
    repository: Option<&str>,
) -> Result<RenderedConfirmation, UnsupportedReason> {
    let question = compact_line(&request.detail.summary);
    if question.chars().count() > MAX_QUESTION_CHARS {
        return Err(UnsupportedReason::QuestionTooLong);
    }
    if request.context.len() > MAX_CONTEXT_FIELDS {
        return Err(UnsupportedReason::TooManyContextFields);
    }
    let context_lines: Vec<_> = request
        .context
        .iter()
        .map(|field| {
            format!(
                "{}: {}",
                compact_line(&field.label),
                compact_line(&field.value)
            )
        })
        .collect();
    if context_lines
        .iter()
        .any(|line| line.chars().count() > MAX_CONTEXT_LINE_CHARS)
    {
        return Err(UnsupportedReason::ContextLineTooLong);
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
        .any(|(_, choice)| compact_line(&choice.label).chars().count() > MAX_CHOICE_LABEL_CHARS)
    {
        return Err(UnsupportedReason::ChoiceLabelTooLong);
    }

    let source = compact_line(source);
    let source = if source.is_empty() {
        "Agent".to_string()
    } else {
        source
    };
    if source.chars().count() > MAX_SOURCE_PROJECT_CHARS {
        return Err(UnsupportedReason::SourceProjectLineTooLong);
    }
    let repository = repository.map(compact_line).filter(|name| !name.is_empty());
    let source_line = match repository {
        Some(repository) => {
            let combined = format!("{source} · {repository}");
            if combined.chars().count() > MAX_SOURCE_PROJECT_CHARS {
                return Err(UnsupportedReason::SourceProjectLineTooLong);
            }
            combined
        }
        None => source,
    };
    let mut lines = vec![format!("[HIL · {token}]"), source_line];
    lines.extend(context_lines);
    lines.push(String::new());
    lines.push(question);
    lines.push(String::new());
    let default = request.presentation.default_action_id();
    for (position, (_, choice)) in choices.iter().enumerate() {
        let recommended = if default == Some(choice.id.as_str()) {
            " [recommended]"
        } else {
            ""
        };
        lines.push(format!(
            "{}  {}{}",
            position + 1,
            compact_line(&choice.label),
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

pub fn watch_args(chat_id: i64, since_row_id: i64) -> Vec<String> {
    vec![
        "watch".into(),
        "--chat-id".into(),
        chat_id.to_string(),
        "--since-rowid".into(),
        since_row_id.to_string(),
        "--json".into(),
    ]
}

pub fn parse_reply(text: &str) -> Option<(String, usize)> {
    let (token, option) = text.split_once(' ')?;
    if option.is_empty()
        || option.contains(char::is_whitespace)
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
    pub id: i64,
    pub chat_id: i64,
    #[serde(default)]
    pub guid: String,
    #[serde(default)]
    pub reply_to_guid: Option<String>,
    #[serde(default)]
    pub created_at: String,
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
    boundary: RequestBoundary,
    choice_indices: Vec<usize>,
    expires_at_ms: u64,
    terminal: bool,
}

#[derive(Default)]
pub struct PendingReplies {
    by_token: HashMap<String, PendingReply>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestBoundary {
    pub identity_mode: IMessageIdentityMode,
    pub chat_id: i64,
    pub sent_row_id: i64,
    pub sent_guid: String,
}

impl PendingReplies {
    pub fn register(
        &mut self,
        token: &str,
        request_id: &str,
        boundary: RequestBoundary,
        choice_indices: Vec<usize>,
        expires_at_ms: u64,
    ) {
        self.by_token.insert(
            token.to_string(),
            PendingReply {
                request_id: request_id.to_string(),
                boundary,
                choice_indices,
                expires_at_ms,
                terminal: false,
            },
        );
    }

    pub fn resolve(&mut self, message: &InboundMessage, now_ms: u64) -> Option<CorrelatedReply> {
        if message.is_reaction || message.has_attachments {
            return None;
        }
        let (token, option) = parse_reply(message.text.as_deref()?)?;
        let pending = self.by_token.get_mut(&token)?;
        if pending.terminal
            || now_ms > pending.expires_at_ms
            || message.chat_id != pending.boundary.chat_id
            || message.id <= pending.boundary.sent_row_id
            || (pending.boundary.identity_mode == IMessageIdentityMode::DistinctPeer
                && message.is_from_me)
            || (!message.guid.is_empty()
                && !pending.boundary.sent_guid.is_empty()
                && message.guid == pending.boundary.sent_guid)
            || message.reply_to_guid.as_deref().is_some_and(|guid| {
                pending.boundary.sent_guid.is_empty() || guid != pending.boundary.sent_guid
            })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    NotConfigured,
    BotSessionLoginRequired,
    BotMessagesAccountUnavailable,
    BotSenderIdentityUnverified,
    SelfMessageUnsupported,
    ImsgMissing,
    PermissionMissing,
    MessagesUnavailable,
    ImsgIncompatible,
    BootstrapRequired,
    RecipientNotImessage,
    AmbiguousChat,
    WatchFailed,
    SendFailed,
    Ready,
}

impl HealthState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::BotSessionLoginRequired => "BOT_SESSION_LOGIN_REQUIRED",
            Self::BotMessagesAccountUnavailable => "BOT_MESSAGES_ACCOUNT_UNAVAILABLE",
            Self::BotSenderIdentityUnverified => "BOT_SENDER_IDENTITY_UNVERIFIED",
            Self::SelfMessageUnsupported => "SELF_MESSAGE_UNSUPPORTED",
            Self::ImsgMissing => "imsg_missing",
            Self::PermissionMissing => "permission_missing",
            Self::MessagesUnavailable => "messages_unavailable",
            Self::ImsgIncompatible => "imsg_incompatible",
            Self::BootstrapRequired => "bootstrap_required",
            Self::RecipientNotImessage => "recipient_not_imessage",
            Self::AmbiguousChat => "ambiguous_chat",
            Self::WatchFailed => "watch_failed",
            Self::SendFailed => "send_failed",
            Self::Ready => "ready",
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
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
    config.enabled && !config.recipient.trim().is_empty()
}

fn verified_direct_chat(config: &IMessageChannelConfig, chat: &ChatRecord) -> bool {
    chat.id == config.chat_id.unwrap_or_default()
        && chat.guid.as_deref() == Some(config.chat_guid.trim())
        && !chat.is_group
        && chat.service.eq_ignore_ascii_case("imessage")
        && valid_participants(config, chat)
}

fn matches_recipient(config: &IMessageChannelConfig, chat: &ChatRecord) -> bool {
    !chat.is_group
        && chat.participants.len() == 1
        && chat.participants[0] == config.recipient.trim()
        && chat.service.eq_ignore_ascii_case("imessage")
        && chat.guid.as_deref().is_some_and(|guid| !guid.is_empty())
}

fn valid_participants(config: &IMessageChannelConfig, chat: &ChatRecord) -> bool {
    match config.identity_mode {
        IMessageIdentityMode::DistinctPeer => {
            chat.participants.len() == 1 && chat.participants[0] == config.recipient.trim()
        }
        IMessageIdentityMode::SameAccount => {
            chat.participants.is_empty()
                || (chat.participants.len() == 1 && chat.participants[0] == config.recipient.trim())
        }
    }
}

fn post_send_chat_matches(config: &IMessageChannelConfig, chat: &ChatRecord) -> bool {
    !chat.is_group
        && chat.service.eq_ignore_ascii_case("imessage")
        && chat.guid.as_deref().is_some_and(|guid| !guid.is_empty())
        && valid_participants(config, chat)
}

fn parse_ndjson<T: DeserializeOwned>(stdout: &[u8]) -> Result<Vec<T>, HealthState> {
    let text = std::str::from_utf8(stdout).map_err(|_| HealthState::MessagesUnavailable)?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|_| HealthState::MessagesUnavailable))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatIdentity {
    pub id: i64,
    pub guid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readiness {
    Ready(ChatIdentity),
    BootstrapRequired,
}

fn readiness_from_chats(
    config: &IMessageChannelConfig,
    chats: &[ChatRecord],
) -> Result<Readiness, HealthState> {
    match (config.chat_id, config.chat_guid.trim().is_empty()) {
        (Some(_), false) => chats
            .iter()
            .find(|chat| verified_direct_chat(config, chat))
            .map(|chat| {
                Readiness::Ready(ChatIdentity {
                    id: chat.id,
                    guid: chat.guid.clone().unwrap_or_default(),
                })
            })
            .ok_or(HealthState::RecipientNotImessage),
        (None, true) => {
            let matches: Vec<_> = chats
                .iter()
                .filter(|chat| matches_recipient(config, chat))
                .collect();
            match matches.as_slice() {
                [] => Ok(Readiness::BootstrapRequired),
                [chat] => Ok(Readiness::Ready(ChatIdentity {
                    id: chat.id,
                    guid: chat.guid.clone().unwrap_or_default(),
                })),
                _ => Err(HealthState::AmbiguousChat),
            }
        }
        _ => Err(HealthState::NotConfigured),
    }
}

async fn list_chats(limit: usize, deadline: Duration) -> Result<Vec<ChatRecord>, HealthState> {
    let output = timeout(
        deadline,
        Command::new(imsg_program())
            .kill_on_drop(true)
            .args(["chats", "--limit", &limit.to_string(), "--json"])
            .output(),
    )
    .await
    .map_err(|_| HealthState::MessagesUnavailable)?
    .map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            HealthState::ImsgMissing
        } else {
            HealthState::MessagesUnavailable
        }
    })?;
    if !output.status.success() {
        return Err(
            match classify_failure(
                output.status.code(),
                &String::from_utf8_lossy(&output.stderr),
            ) {
                HealthState::SendFailed => HealthState::MessagesUnavailable,
                other => other,
            },
        );
    }
    parse_ndjson(&output.stdout)
}

pub async fn prepare(config: &IMessageChannelConfig) -> Result<Readiness, HealthState> {
    if !configured(config) {
        return Err(HealthState::NotConfigured);
    }
    let version = timeout(
        Duration::from_secs(5),
        Command::new(imsg_program())
            .kill_on_drop(true)
            .arg("--version")
            .output(),
    )
    .await;
    match version {
        Ok(Ok(output)) if output.status.success() && compatible_version(&output.stdout) => {}
        Ok(Ok(output)) if output.status.success() => {
            return Err(HealthState::ImsgIncompatible);
        }
        Ok(Ok(output)) => {
            return Err(classify_failure(
                output.status.code(),
                &String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(HealthState::ImsgMissing);
        }
        Ok(Err(_)) | Err(_) => return Err(HealthState::ImsgMissing),
    }
    readiness_from_chats(
        config,
        &list_chats(CHAT_SCAN_LIMIT, CHAT_SCAN_TIMEOUT).await?,
    )
}

fn compatible_version(stdout: &[u8]) -> bool {
    String::from_utf8_lossy(stdout).trim() == SUPPORTED_IMSG_VERSION
}

/// Check the documented external CLI and verify that the stored conversation remains a direct
/// iMessage chat with the configured peer. Human-oriented output is never parsed.
pub(crate) async fn health_local(config: &IMessageChannelConfig) -> HealthState {
    match prepare(config).await {
        Ok(Readiness::Ready(_)) => HealthState::Ready,
        Ok(Readiness::BootstrapRequired) => HealthState::BootstrapRequired,
        Err(state) => state,
    }
}

pub async fn health(config: &IMessageChannelConfig) -> HealthState {
    if super::imessage_worker::required_for(config.identity_mode) {
        super::imessage_worker::health(config).await
    } else {
        health_local(config).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReceipt {
    pub row_id: i64,
    pub guid: String,
}

fn parse_send_receipt(stdout: &[u8]) -> Option<SendReceipt> {
    parse_ndjson::<serde_json::Value>(stdout)
        .ok()?
        .into_iter()
        .find_map(|value| {
            if value.get("status").and_then(|v| v.as_str()) != Some("sent") {
                return None;
            }
            Some(SendReceipt {
                row_id: value.get("id")?.as_i64()?,
                guid: value.get("guid")?.as_str()?.to_string(),
            })
        })
}

/// Send once through the explicit iMessage-only direct-recipient path. Mutation failures are
/// returned without retry because their delivery disposition may be uncertain.
pub async fn send(
    config: &IMessageChannelConfig,
    text: &str,
    image: Option<&Path>,
) -> Result<SendReceipt, HealthState> {
    let output = timeout(
        Duration::from_secs(60),
        Command::new(imsg_program())
            .kill_on_drop(true)
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
    parse_send_receipt(&output.stdout).ok_or(HealthState::SendFailed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreSendBoundary {
    pub latest_row_id: Option<i64>,
    pub started_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRequest {
    pub chat: ChatIdentity,
    pub sent: SendReceipt,
}

async fn history(chat_id: i64, limit: usize) -> Result<Vec<InboundMessage>, HealthState> {
    let output = timeout(
        Duration::from_secs(10),
        Command::new(imsg_program())
            .kill_on_drop(true)
            .args([
                "history",
                "--chat-id",
                &chat_id.to_string(),
                "--limit",
                &limit.to_string(),
                "--json",
            ])
            .output(),
    )
    .await
    .map_err(|_| HealthState::MessagesUnavailable)?
    .map_err(|_| HealthState::MessagesUnavailable)?;
    if !output.status.success() {
        return Err(HealthState::MessagesUnavailable);
    }
    parse_ndjson(&output.stdout)
}

pub async fn pre_send_boundary(readiness: &Readiness) -> Result<PreSendBoundary, HealthState> {
    let latest_row_id = match readiness {
        Readiness::Ready(chat) => history(chat.id, 1).await?.first().map(|message| message.id),
        Readiness::BootstrapRequired => None,
    };
    Ok(PreSendBoundary {
        latest_row_id,
        started_at_ms: chrono::Utc::now().timestamp_millis(),
    })
}

fn resolve_sent_request(
    config: &IMessageChannelConfig,
    boundary: &PreSendBoundary,
    receipt: &SendReceipt,
    text: &str,
    chats: &[ChatRecord],
    messages: &[InboundMessage],
) -> Result<ResolvedRequest, HealthState> {
    let mut matches = Vec::new();
    for chat in chats
        .iter()
        .filter(|chat| post_send_chat_matches(config, chat))
    {
        for message in messages.iter().filter(|message| {
            message.chat_id == chat.id
                && message.id == receipt.row_id
                && message.guid == receipt.guid
                && message.is_from_me
                && message.text.as_deref() == Some(text)
                && chrono::DateTime::parse_from_rfc3339(&message.created_at)
                    .is_ok_and(|created| created.timestamp_millis() >= boundary.started_at_ms)
        }) {
            let _ = message;
            matches.push(ResolvedRequest {
                chat: ChatIdentity {
                    id: chat.id,
                    guid: chat.guid.clone().unwrap_or_default(),
                },
                sent: receipt.clone(),
            });
        }
    }
    match matches.as_slice() {
        [resolved] => Ok(resolved.clone()),
        [] => Err(HealthState::MessagesUnavailable),
        _ => Err(HealthState::AmbiguousChat),
    }
}

pub async fn resolve_after_send(
    config: &IMessageChannelConfig,
    readiness: &Readiness,
    boundary: &PreSendBoundary,
    receipt: &SendReceipt,
    text: &str,
) -> Result<ResolvedRequest, HealthState> {
    if boundary
        .latest_row_id
        .is_some_and(|row_id| receipt.row_id <= row_id)
    {
        return Err(HealthState::MessagesUnavailable);
    }
    let message = read_sent_event(receipt, boundary, text).await?;
    let chats = list_chats(POST_SEND_CHAT_LIMIT, Duration::from_secs(10)).await?;
    let candidates: Vec<_> = chats
        .into_iter()
        .filter(|chat| {
            chat.id == message.chat_id
                && match readiness {
                    Readiness::Ready(expected) => {
                        chat.id == expected.id
                            && chat.guid.as_deref() == Some(expected.guid.as_str())
                    }
                    Readiness::BootstrapRequired => true,
                }
        })
        .collect();
    resolve_sent_request(config, boundary, receipt, text, &candidates, &[message])
}

async fn read_sent_event(
    receipt: &SendReceipt,
    boundary: &PreSendBoundary,
    text: &str,
) -> Result<InboundMessage, HealthState> {
    let since_row_id = receipt
        .row_id
        .checked_sub(1)
        .ok_or(HealthState::MessagesUnavailable)?;
    let (mut child, mut reader) = spawn_watch_process(None, since_row_id)?;
    let result = timeout(Duration::from_secs(10), async {
        loop {
            let message = read_inbound_line(&mut reader)
                .await?
                .ok_or(HealthState::WatchFailed)?;
            if message.id < receipt.row_id {
                continue;
            }
            if message.id != receipt.row_id
                || message.guid != receipt.guid
                || !message.is_from_me
                || message.text.as_deref() != Some(text)
                || chrono::DateTime::parse_from_rfc3339(&message.created_at)
                    .map_or(true, |created| {
                        created.timestamp_millis() < boundary.started_at_ms
                    })
            {
                return Err(HealthState::MessagesUnavailable);
            }
            return Ok(message);
        }
    })
    .await
    .unwrap_or(Err(HealthState::MessagesUnavailable));
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

pub fn persist_resolved_chat(
    original: &IMessageChannelConfig,
    resolved: &ResolvedRequest,
) -> Result<(), HealthState> {
    if original.chat_id == Some(resolved.chat.id) && original.chat_guid.trim() == resolved.chat.guid
    {
        return Ok(());
    }
    let mut config = AppConfig::load_without_secrets();
    let channel = &mut config.channels.imessage;
    if !channel.enabled
        || channel.recipient.trim() != original.recipient.trim()
        || channel.identity_mode != original.identity_mode
        || channel.chat_id.is_some()
        || !channel.chat_guid.trim().is_empty()
    {
        return Err(HealthState::MessagesUnavailable);
    }
    channel.chat_id = Some(resolved.chat.id);
    channel.chat_guid = resolved.chat.guid.clone();
    config.save().map_err(|_| HealthState::MessagesUnavailable)
}

/// Start a single chat-scoped NDJSON watcher. `kill_on_drop` is defense in depth; callers still
/// explicitly kill and reap the child on every terminal path.
pub fn spawn_watch(
    chat_id: i64,
    since_row_id: i64,
) -> Result<(Child, BufReader<ChildStdout>), HealthState> {
    spawn_watch_process(Some(chat_id), since_row_id)
}

fn spawn_watch_process(
    chat_id: Option<i64>,
    since_row_id: i64,
) -> Result<(Child, BufReader<ChildStdout>), HealthState> {
    let mut args = vec!["watch".to_string()];
    if let Some(chat_id) = chat_id {
        args.extend(["--chat-id".into(), chat_id.to_string()]);
    }
    args.extend([
        "--since-rowid".into(),
        since_row_id.to_string(),
        "--json".into(),
    ]);
    let mut child = Command::new(imsg_program())
        .args(args)
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
        Ok(_) => serde_json::from_str(line.trim())
            .map(Some)
            .map_err(|_| HealthState::WatchFailed),
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
        || message.contains("authorization denied")
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
    use crate::models::{
        ConfirmChoice, ConfirmDetail, ConfirmField, ConfirmFieldKind, ConfirmPresentation,
        ConfirmRequest,
    };
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
            identity_mode: crate::config::IMessageIdentityMode::DistinctPeer,
            chat_id: Some(42),
            chat_guid: "iMessage;-;+15551234567".into(),
        }
    }

    fn context(label: &str, value: &str) -> ConfirmField {
        ConfirmField {
            id: label.to_ascii_lowercase().replace(' ', "-"),
            label: label.into(),
            value: value.into(),
            kind: ConfirmFieldKind::Text,
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
    fn renderer_uses_the_preferred_compact_shape_without_redundant_headings() {
        let rendered = render_confirmation(&request(), "7F32", "Codex", Some("human-in-loop"))
            .expect("supported request");
        assert_eq!(rendered.choice_indices, vec![0, 1]);
        assert_eq!(
            rendered.text,
            "[HIL · 7F32]\nCodex · human-in-loop\n\nContinue?\n\n1  Continue [recommended]\n2  Stop\n\nReply: 7F32 1"
        );
        for redundant in ["Context", "Question", "Action", "Delete generated objects"] {
            assert!(!rendered.text.contains(redundant));
        }
        assert!(rendered.text.chars().count() <= 700);
    }

    #[test]
    fn renderer_compacts_fields_to_one_line_without_dropping_words() {
        let mut multiline = request();
        multiline.context = vec![context("Release\nstatus", "candidate\tbuild")];
        multiline.detail.summary = "Does this\ncompact layout look correct?".into();
        multiline.choices[0].label = "Looks\ncorrect".into();
        let rendered = render_confirmation(
            &multiline,
            "7F32",
            "Codex\nAgent",
            Some("human-in-loop\nproject"),
        )
        .unwrap();
        assert!(rendered
            .text
            .contains("\nCodex Agent · human-in-loop project\n"));
        assert!(rendered
            .text
            .contains("\nRelease status: candidate build\n"));
        assert!(rendered
            .text
            .contains("\nDoes this compact layout look correct?\n"));
        assert!(rendered.text.contains("\n1  Looks correct [recommended]\n"));
    }

    #[test]
    fn renderer_emits_zero_one_or_two_required_context_lines() {
        let zero = render_confirmation(&request(), "7F32", "Codex", None).unwrap();
        assert_eq!(zero.text.lines().nth(1), Some("Codex"));
        assert!(!zero.text.contains("Release: candidate"));

        let mut one_request = request();
        one_request.context = vec![context("Release", "candidate")];
        let one = render_confirmation(&one_request, "7F32", "Codex", None).unwrap();
        assert!(one.text.contains("\nRelease: candidate\n\nContinue?"));

        let mut two_request = one_request.clone();
        two_request.context.push(context("Risk", "renderer only"));
        let two = render_confirmation(&two_request, "7F32", "Codex", None).unwrap();
        assert!(two
            .text
            .contains("\nRelease: candidate\nRisk: renderer only\n\nContinue?"));

        let mut three_request = two_request;
        three_request.context.push(context("Owner", "User"));
        assert!(render_confirmation(&three_request, "7F32", "Codex", None).is_err());
    }

    #[test]
    fn renderer_enforces_line_and_field_budgets_without_truncation() {
        let mut long_title = request();
        long_title.title = "Title is canonical but not duplicated on the phone. ".repeat(4);
        let rendered = render_confirmation(&long_title, "7F32", "Codex", Some("human-in-loop"))
            .expect("the compact renderer does not copy the title");
        assert!(!rendered.text.contains("Title is canonical"));

        let source = "S".repeat(80);
        let rendered = render_confirmation(&request(), "7F32", &source, None).unwrap();
        assert_eq!(rendered.text.lines().nth(1), Some(source.as_str()));
        assert!(render_confirmation(&request(), "7F32", &"S".repeat(81), None).is_err());

        let mut long_context = request();
        long_context.context = vec![context("L", &"界".repeat(78))];
        assert!(render_confirmation(&long_context, "7F32", "Codex", None).is_err());

        let mut long_question = request();
        long_question.detail.summary = "界".repeat(161);
        assert!(render_confirmation(&long_question, "7F32", "Codex", None).is_err());

        let mut long_choice = request();
        long_choice.choices[0].label = "界".repeat(61);
        assert!(render_confirmation(&long_choice, "7F32", "Codex", None).is_err());
    }

    #[test]
    fn renderer_never_compacts_away_a_repository_label() {
        let repository = "R".repeat(10);
        let source = "S".repeat(67);
        let rendered = render_confirmation(&request(), "7F32", &source, Some(&repository)).unwrap();
        assert_eq!(rendered.text.lines().nth(1).unwrap().chars().count(), 80);
        assert!(rendered.text.lines().nth(1).unwrap().ends_with(&repository));

        let source = "S".repeat(68);
        assert_eq!(
            render_confirmation(&request(), "7F32", &source, Some(&repository)),
            Err(UnsupportedReason::SourceProjectLineTooLong)
        );
    }

    #[test]
    fn renderer_preserves_critical_unicode_at_the_hard_limit_or_sends_nothing() {
        let source = "源".repeat(80);
        let first_context = "甲: ".to_string() + &"界".repeat(77);
        let second_context = "乙: ".to_string() + &"文".repeat(77);
        let question = "问".repeat(160);
        let label = "选".repeat(60);
        let mut bounded = request();
        bounded.context = vec![
            context("甲", &"界".repeat(77)),
            context("乙", &"文".repeat(77)),
        ];
        bounded.detail.summary = question.clone();
        bounded.choices[0].label = label.clone();
        bounded.choices[1].label = label.clone();
        bounded.choices.push(ConfirmChoice {
            id: "third".into(),
            label: label.clone(),
            description: String::new(),
            role: ActionRole::Default,
            variant: None,
        });
        let rendered = render_confirmation(&bounded, "7F32", &source, None).unwrap();
        assert!(rendered.text.contains(&first_context));
        assert!(rendered.text.contains(&second_context));
        assert!(rendered.text.contains(&question));
        assert_eq!(rendered.text.matches(&label).count(), 3);
        assert!(rendered.text.chars().count() <= 700);

        bounded.choices.push(ConfirmChoice {
            id: "fourth".into(),
            label,
            description: String::new(),
            role: ActionRole::Default,
            variant: None,
        });
        assert_eq!(
            render_confirmation(&bounded, "7F32", &source, None),
            Err(UnsupportedReason::RenderedTextTooLong)
        );
    }

    #[test]
    fn renderer_keeps_choice_count_and_interactive_input_fail_closed() {
        let mut one_choice = request();
        one_choice.choices.truncate(1);
        assert_eq!(
            render_confirmation(&one_choice, "7F32", "Codex", None),
            Err(UnsupportedReason::ChoiceCount)
        );

        let mut seven_choices = request();
        for index in 3..=7 {
            seven_choices.choices.push(ConfirmChoice {
                id: format!("choice-{index}"),
                label: format!("Choice {index}"),
                description: String::new(),
                role: ActionRole::Default,
                variant: None,
            });
        }
        assert_eq!(
            render_confirmation(&seven_choices, "7F32", "Codex", None),
            Err(UnsupportedReason::ChoiceCount)
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
            render_confirmation(&with_input, "7F32", "Codex", Some("human-in-loop")),
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
    fn watcher_resumes_strictly_after_the_sent_row() {
        assert_eq!(
            watch_args(42, 9000),
            vec![
                "watch",
                "--chat-id",
                "42",
                "--since-rowid",
                "9000",
                "--json"
            ]
        );
    }

    #[test]
    fn reply_parser_requires_exact_token_and_one_based_option() {
        assert_eq!(parse_reply("7F32 2"), Some(("7F32".into(), 1)));
        for invalid in [
            "2",
            "7F32",
            "7F32 0",
            "7F32 two",
            "7F32 2 extra",
            "7f32 2",
            " 7F32 2",
            "7F32  2",
            "7F32\t2",
            "7F32 2\n",
        ] {
            assert_eq!(parse_reply(invalid), None, "accepted {invalid:?}");
        }
    }

    #[test]
    fn compatible_imsg_profile_is_exact() {
        assert!(compatible_version(b"0.15.1\n"));
        assert!(!compatible_version(b"0.15.0\n"));
        assert!(!compatible_version(b"0.16.0\n"));
    }

    #[test]
    fn imsg_program_uses_a_pinned_worker_override_or_the_normal_path_lookup() {
        assert_eq!(
            imsg_program_from_override(None),
            std::ffi::OsString::from("imsg")
        );
        assert_eq!(
            imsg_program_from_override(Some(std::ffi::OsString::new())),
            std::ffi::OsString::from("imsg")
        );
        assert_eq!(
            imsg_program_from_override(Some(std::ffi::OsString::from(
                "/Users/Shared/human-in-loop/bin/imsg"
            ))),
            std::ffi::OsString::from("/Users/Shared/human-in-loop/bin/imsg")
        );
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
    fn same_account_accepts_only_a_strictly_post_send_reply() {
        let mut pending = PendingReplies::default();
        pending.register(
            "7F32",
            "request-123",
            RequestBoundary {
                identity_mode: crate::config::IMessageIdentityMode::SameAccount,
                chat_id: 42,
                sent_row_id: 100,
                sent_guid: "REQUEST-GUID".into(),
            },
            vec![0, 1],
            2_000,
        );
        let valid = InboundMessage {
            id: 101,
            chat_id: 42,
            guid: "REPLY-GUID".into(),
            reply_to_guid: None,
            created_at: "2026-09-07T10:00:01.000Z".into(),
            is_from_me: true,
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
    }

    #[test]
    fn same_account_rejects_outgoing_stale_wrong_chat_and_non_text_rows() {
        let mut pending = PendingReplies::default();
        let register = |pending: &mut PendingReplies, token: &str| {
            pending.register(
                token,
                "request-123",
                RequestBoundary {
                    identity_mode: crate::config::IMessageIdentityMode::SameAccount,
                    chat_id: 42,
                    sent_row_id: 100,
                    sent_guid: "REQUEST-GUID".into(),
                },
                vec![0, 1],
                2_000,
            );
        };
        let candidate = InboundMessage {
            id: 101,
            chat_id: 42,
            guid: "REPLY-GUID".into(),
            reply_to_guid: None,
            created_at: "2026-09-07T10:00:01.000Z".into(),
            is_from_me: true,
            text: Some("7F32 1".into()),
            is_reaction: false,
            has_attachments: false,
        };

        for (token, message) in [
            (
                "7F32",
                InboundMessage {
                    id: 100,
                    guid: "REQUEST-GUID".into(),
                    ..candidate.clone()
                },
            ),
            (
                "7F32",
                InboundMessage {
                    id: 99,
                    ..candidate.clone()
                },
            ),
            (
                "7F32",
                InboundMessage {
                    chat_id: 7,
                    ..candidate.clone()
                },
            ),
            (
                "7F32",
                InboundMessage {
                    text: Some("FFFF 1".into()),
                    ..candidate.clone()
                },
            ),
            (
                "7F32",
                InboundMessage {
                    text: Some("7F32 3".into()),
                    ..candidate.clone()
                },
            ),
            (
                "7F32",
                InboundMessage {
                    text: None,
                    has_attachments: true,
                    ..candidate.clone()
                },
            ),
            (
                "7F32",
                InboundMessage {
                    is_reaction: true,
                    ..candidate.clone()
                },
            ),
        ] {
            register(&mut pending, token);
            assert_eq!(pending.resolve(&message, 1_500), None);
        }

        register(&mut pending, "7F32");
        assert_eq!(pending.resolve(&candidate, 2_001), None, "expired reply");
    }

    #[test]
    fn inline_reply_guid_must_match_the_sent_request() {
        let mut pending = PendingReplies::default();
        pending.register(
            "7F32",
            "request-123",
            RequestBoundary {
                identity_mode: crate::config::IMessageIdentityMode::SameAccount,
                chat_id: 42,
                sent_row_id: 100,
                sent_guid: "REQUEST-GUID".into(),
            },
            vec![0, 1],
            2_000,
        );
        let message = InboundMessage {
            id: 101,
            chat_id: 42,
            guid: "REPLY-GUID".into(),
            reply_to_guid: Some("OTHER-GUID".into()),
            created_at: "2026-09-07T10:00:01.000Z".into(),
            is_from_me: true,
            text: Some("7F32 1".into()),
            is_reaction: false,
            has_attachments: false,
        };
        assert_eq!(pending.resolve(&message, 1_500), None);
    }

    #[test]
    fn distinct_peer_still_rejects_is_from_me() {
        let mut pending = PendingReplies::default();
        pending.register(
            "7F32",
            "request-123",
            RequestBoundary {
                identity_mode: crate::config::IMessageIdentityMode::DistinctPeer,
                chat_id: 42,
                sent_row_id: 100,
                sent_guid: "REQUEST-GUID".into(),
            },
            vec![0, 1],
            2_000,
        );
        let message = InboundMessage {
            id: 101,
            chat_id: 42,
            guid: "REPLY-GUID".into(),
            reply_to_guid: None,
            created_at: "2026-09-07T10:00:01.000Z".into(),
            is_from_me: true,
            text: Some("7F32 1".into()),
            is_reaction: false,
            has_attachments: false,
        };
        assert_eq!(pending.resolve(&message, 1_500), None);
    }

    #[test]
    fn recipient_without_an_existing_chat_is_bootstrap_required() {
        let mut config = imessage_config();
        config.identity_mode = crate::config::IMessageIdentityMode::SameAccount;
        config.chat_id = None;
        config.chat_guid.clear();
        assert_eq!(
            readiness_from_chats(&config, &[]),
            Ok(Readiness::BootstrapRequired)
        );
    }

    #[test]
    fn ambiguous_post_bootstrap_resolution_fails_closed() {
        let mut config = imessage_config();
        config.identity_mode = crate::config::IMessageIdentityMode::SameAccount;
        config.chat_id = None;
        config.chat_guid.clear();
        let chats = vec![
            ChatRecord {
                id: 42,
                guid: Some("iMessage;-;first".into()),
                service: "iMessage".into(),
                is_group: false,
                participants: vec![config.recipient.clone()],
            },
            ChatRecord {
                id: 43,
                guid: Some("iMessage;-;second".into()),
                service: "iMessage".into(),
                is_group: false,
                participants: vec![config.recipient.clone()],
            },
        ];
        let receipt = SendReceipt {
            row_id: 101,
            guid: "REQUEST-GUID".into(),
        };
        let boundary = PreSendBoundary {
            latest_row_id: None,
            started_at_ms: 0,
        };
        let messages = vec![
            InboundMessage {
                id: 101,
                chat_id: 42,
                guid: receipt.guid.clone(),
                reply_to_guid: None,
                created_at: "2026-09-07T10:00:00.000Z".into(),
                is_from_me: true,
                text: Some("rendered request".into()),
                is_reaction: false,
                has_attachments: false,
            },
            InboundMessage {
                chat_id: 43,
                ..InboundMessage {
                    id: 101,
                    chat_id: 42,
                    guid: receipt.guid.clone(),
                    reply_to_guid: None,
                    created_at: "2026-09-07T10:00:00.000Z".into(),
                    is_from_me: true,
                    text: Some("rendered request".into()),
                    is_reaction: false,
                    has_attachments: false,
                }
            },
        ];
        assert_eq!(
            resolve_sent_request(
                &config,
                &boundary,
                &receipt,
                "rendered request",
                &chats,
                &messages
            ),
            Err(HealthState::AmbiguousChat)
        );
    }

    #[test]
    fn post_send_resolution_rejects_a_row_before_the_time_boundary() {
        let mut config = imessage_config();
        config.chat_id = None;
        config.chat_guid.clear();
        let chats = vec![ChatRecord {
            id: 42,
            guid: Some("iMessage;-;direct".into()),
            service: "iMessage".into(),
            is_group: false,
            participants: vec![config.recipient.clone()],
        }];
        let receipt = SendReceipt {
            row_id: 101,
            guid: "REQUEST-GUID".into(),
        };
        let messages = vec![InboundMessage {
            id: 101,
            chat_id: 42,
            guid: receipt.guid.clone(),
            reply_to_guid: None,
            created_at: "2026-09-07T10:00:00.000Z".into(),
            is_from_me: true,
            text: Some("rendered request".into()),
            is_reaction: false,
            has_attachments: false,
        }];
        let boundary = PreSendBoundary {
            latest_row_id: None,
            started_at_ms: chrono::DateTime::parse_from_rfc3339("2026-09-07T10:00:01.000Z")
                .unwrap()
                .timestamp_millis(),
        };

        assert_eq!(
            resolve_sent_request(
                &config,
                &boundary,
                &receipt,
                "rendered request",
                &chats,
                &messages
            ),
            Err(HealthState::MessagesUnavailable)
        );
    }

    #[test]
    fn same_account_post_send_resolution_allows_local_implicit_participants() {
        let mut config = imessage_config();
        config.identity_mode = crate::config::IMessageIdentityMode::SameAccount;
        config.chat_id = None;
        config.chat_guid.clear();
        let chats = vec![ChatRecord {
            id: 42,
            guid: Some("iMessage;-;self".into()),
            service: "iMessage".into(),
            is_group: false,
            participants: vec![],
        }];
        let receipt = SendReceipt {
            row_id: 101,
            guid: "REQUEST-GUID".into(),
        };
        let boundary = PreSendBoundary {
            latest_row_id: None,
            started_at_ms: 0,
        };
        let messages = vec![InboundMessage {
            id: 101,
            chat_id: 42,
            guid: receipt.guid.clone(),
            reply_to_guid: None,
            created_at: "2026-09-07T10:00:00.000Z".into(),
            is_from_me: true,
            text: Some("rendered request".into()),
            is_reaction: false,
            has_attachments: false,
        }];

        assert_eq!(
            resolve_sent_request(
                &config,
                &boundary,
                &receipt,
                "rendered request",
                &chats,
                &messages
            ),
            Ok(ResolvedRequest {
                chat: ChatIdentity {
                    id: 42,
                    guid: "iMessage;-;self".into(),
                },
                sent: receipt,
            })
        );
    }

    #[test]
    fn distinct_peer_post_send_resolution_keeps_exact_participant_check() {
        let config = imessage_config();
        let chats = vec![ChatRecord {
            id: 42,
            guid: Some("iMessage;-;peer".into()),
            service: "iMessage".into(),
            is_group: false,
            participants: vec![],
        }];
        let receipt = SendReceipt {
            row_id: 101,
            guid: "REQUEST-GUID".into(),
        };
        let boundary = PreSendBoundary {
            latest_row_id: None,
            started_at_ms: 0,
        };
        let messages = vec![InboundMessage {
            id: 101,
            chat_id: 42,
            guid: receipt.guid.clone(),
            reply_to_guid: None,
            created_at: "2026-09-07T10:00:00.000Z".into(),
            is_from_me: true,
            text: Some("rendered request".into()),
            is_reaction: false,
            has_attachments: false,
        }];

        assert_eq!(
            resolve_sent_request(
                &config,
                &boundary,
                &receipt,
                "rendered request",
                &chats,
                &messages
            ),
            Err(HealthState::MessagesUnavailable)
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
    fn production_bot_health_states_are_distinct_and_stable() {
        assert_eq!(
            HealthState::BotSessionLoginRequired.as_str(),
            "BOT_SESSION_LOGIN_REQUIRED"
        );
        assert_eq!(
            HealthState::BotMessagesAccountUnavailable.as_str(),
            "BOT_MESSAGES_ACCOUNT_UNAVAILABLE"
        );
        assert_eq!(
            HealthState::BotSenderIdentityUnverified.as_str(),
            "BOT_SENDER_IDENTITY_UNVERIFIED"
        );
        assert_eq!(
            HealthState::SelfMessageUnsupported.as_str(),
            "SELF_MESSAGE_UNSUPPORTED"
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
