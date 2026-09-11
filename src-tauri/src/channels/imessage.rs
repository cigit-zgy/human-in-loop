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
use tokio::time::{sleep, timeout, Duration, Instant};

pub const MIN_TOKEN_CHARS: usize = 5;
pub const MAX_TOKEN_CHARS: usize = 63;
pub const MAX_SOURCE_PROJECT_CHARS: usize = 80;
pub const MAX_QUESTION_CHARS: usize = 160;
pub const MAX_CONTEXT_FIELDS: usize = 2;
pub const MAX_CONTEXT_LINE_CHARS: usize = 80;
pub const MIN_CHOICES: usize = 2;
pub const MAX_CHOICES: usize = 6;
pub const MAX_CHOICE_LABEL_CHARS: usize = 60;
pub const MAX_NOTIFICATION_RENDERED_CHARS: usize = 700;
pub const ABSOLUTE_DETAIL_MAX_CHARS: usize = 4500;
pub const ABSOLUTE_RENDERED_MAX_CHARS: usize = 5000;
pub const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
pub(crate) const SUPPORTED_IMSG_VERSION: &str = "0.15.1";
pub(crate) const IMSG_EXECUTABLE_ENV: &str = "HUMAN_IN_LOOP_IMSG_EXECUTABLE";
const CHAT_SCAN_LIMIT: usize = 10_000;
const CHAT_SCAN_TIMEOUT: Duration = Duration::from_secs(90);
const POST_SEND_CHAT_LIMIT: usize = 20;
const POST_SEND_HISTORY_LIMIT: usize = 20;
const POST_SEND_RESOLUTION_TIMEOUT: Duration = Duration::from_secs(10);
const POST_SEND_RESOLUTION_RETRY: Duration = Duration::from_millis(100);

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
    DetailTooLong,
    RenderedTextTooLong,
    InvalidDecisionBudget,
    InvalidToken,
    RequiredImageUnsupported,
}

impl UnsupportedReason {
    pub fn diagnostic_reason(&self) -> String {
        match self {
            Self::DetailTooLong => "detail_too_long".into(),
            Self::RenderedTextTooLong => "rendered_text_too_long".into(),
            Self::InvalidDecisionBudget => "invalid_decision_budget".into(),
            other => format!("unsupported: {other:?}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionBudgets {
    detail_max_chars: usize,
    rendered_max_chars: usize,
}

impl DecisionBudgets {
    pub(crate) const fn rendered_max_chars(self) -> usize {
        self.rendered_max_chars
    }
}

pub fn decision_budgets(
    config: &IMessageChannelConfig,
) -> Result<DecisionBudgets, UnsupportedReason> {
    if config.decision_detail_max_chars == 0
        || config.decision_rendered_max_chars == 0
        || config.decision_detail_max_chars > config.decision_rendered_max_chars
        || config.decision_detail_max_chars > ABSOLUTE_DETAIL_MAX_CHARS
        || config.decision_rendered_max_chars > ABSOLUTE_RENDERED_MAX_CHARS
    {
        return Err(UnsupportedReason::InvalidDecisionBudget);
    }
    Ok(DecisionBudgets {
        detail_max_chars: config.decision_detail_max_chars,
        rendered_max_chars: config.decision_rendered_max_chars,
    })
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
    render_confirmation_with_config(
        request,
        token,
        source,
        repository,
        &IMessageChannelConfig::default(),
    )
}

pub fn render_confirmation_with_config(
    request: &ConfirmRequest,
    token: &str,
    source: &str,
    repository: Option<&str>,
    config: &IMessageChannelConfig,
) -> Result<RenderedConfirmation, UnsupportedReason> {
    let budgets = decision_budgets(config)?;
    if !valid_token(token) {
        return Err(UnsupportedReason::InvalidToken);
    }
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
    let mut blocks = vec![format!("[HIL · {token}]"), source_line];
    if !context_lines.is_empty() {
        blocks.push(context_lines.join("\n"));
    }
    let detail = request
        .detail
        .body_md
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string();
    if detail.chars().count() > budgets.detail_max_chars {
        return Err(UnsupportedReason::DetailTooLong);
    }
    if !detail.is_empty() {
        blocks.push(detail);
    }
    blocks.push(question);
    let default = request.presentation.default_action_id();
    for (position, (_, choice)) in choices.iter().enumerate() {
        let recommended = if default == Some(choice.id.as_str()) {
            " [recommended]"
        } else {
            ""
        };
        blocks.push(format!(
            "{}  {}{}",
            position + 1,
            compact_line(&choice.label),
            recommended
        ));
    }
    blocks.push("Reply:".into());
    blocks.push(format!("{token}-1"));
    let text = blocks.join("\n\n");
    if text.chars().count() > budgets.rendered_max_chars {
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

pub(crate) fn valid_token(token: &str) -> bool {
    (MIN_TOKEN_CHARS..=MAX_TOKEN_CHARS).contains(&token.len())
        && token.len() % 2 == 1
        && token.as_bytes()[0].is_ascii_digit()
        && token.as_bytes()[0] != b'0'
        && token.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn parse_reply(text: &str) -> Option<(String, usize)> {
    let (token, option) = text.trim().split_once('-')?;
    if !valid_token(token)
        || option.is_empty()
        || option.as_bytes()[0] == b'0'
        || !option.bytes().all(|byte| byte.is_ascii_digit())
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
            let digest = Sha256::digest(material.as_bytes());
            let prefix = u32::from_be_bytes(digest[..4].try_into().expect("SHA-256 prefix"));
            let mut token = (10_000 + prefix % 90_000).to_string();
            if !self.by_token.contains_key(&token) {
                self.by_token.insert(token.clone(), request_id.into());
                return token;
            }
            for chunk in digest[4..].chunks_exact(2) {
                let suffix = u16::from_be_bytes([chunk[0], chunk[1]]) % 100;
                token.push_str(&format!("{suffix:02}"));
                if !self.by_token.contains_key(&token) {
                    self.by_token.insert(token.clone(), request_id.into());
                    return token;
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
    AutomationReady,
    AutomationConsentRequired,
    AutomationDenied,
    AutomationTargetUnavailable,
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
            Self::AutomationReady => "automation_ready",
            Self::AutomationConsentRequired => "automation_consent_required",
            Self::AutomationDenied => "automation_denied",
            Self::AutomationTargetUnavailable => "automation_target_unavailable",
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

/// Redacted evidence only: this never registers or completes a canonical request.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryDiagnosis {
    pub classification: String,
    pub outgoing_matches: usize,
    pub reply_rows_present: usize,
    pub scanned: usize,
}

pub fn valid_diagnostic_locator(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://github.com/cigit-zgy/human-in-loop/blob/") else {
        return false;
    };
    let Some((revision, path)) = rest.split_once('/') else {
        return false;
    };
    revision.len() == 40
        && revision.bytes().all(|b| b.is_ascii_hexdigit())
        && path.starts_with("reports/chatgpt/")
        && path.ends_with(".md")
        && path.len() < 80
        && !path.contains("..")
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
}

fn diagnose_rows(rows: &[InboundMessage], locator: &str, chat_id: i64) -> HistoryDiagnosis {
    let outgoing: Vec<_> = rows
        .iter()
        .filter(|row| {
            row.chat_id == chat_id
                && row.is_from_me
                && !row.is_reaction
                && !row.has_attachments
                && row
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains(locator) && text.starts_with("[HIL · "))
        })
        .collect();
    let mut result = HistoryDiagnosis {
        classification: "INSUFFICIENT_EVIDENCE".into(),
        outgoing_matches: outgoing.len(),
        reply_rows_present: 0,
        scanned: rows.len(),
    };
    let [sent] = outgoing.as_slice() else {
        return result;
    };
    let text = sent.text.as_deref().unwrap_or_default();
    let Some(token) = text
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("[HIL · "))
        .and_then(|line| line.strip_suffix(']'))
        .filter(|token| valid_token(token))
    else {
        return result;
    };
    let footer = format!("\n\nReply:\n\n{token}-1");
    let Some(body) = text.strip_suffix(&footer) else {
        return result;
    };
    let body_blocks: Vec<_> = body.split("\n\n").collect();
    let Some(choice_count) = (MIN_CHOICES..=MAX_CHOICES).find(|count| {
        body_blocks.len() >= *count
            && body_blocks[body_blocks.len() - *count..]
                .iter()
                .enumerate()
                .all(|(index, block)| {
                    block
                        .strip_prefix(&format!("{}  ", index + 1))
                        .is_some_and(|label| !label.is_empty() && !label.contains('\n'))
                })
    }) else {
        return result;
    };
    // History is a bounded newest-first snapshot containing the identified send row.
    // Every later row in this chat is therefore covered, even if older history is truncated.
    if rows.windows(2).any(|pair| pair[0].id <= pair[1].id) || sent.guid.is_empty() {
        return result;
    }
    result.reply_rows_present =
        rows.iter()
            .filter(|row| {
                row.chat_id == chat_id
                    && row.id > sent.id
                    && !row.is_from_me
                    && !row.is_reaction
                    && !row.has_attachments
                    && row.guid != sent.guid
                    && row
                        .reply_to_guid
                        .as_deref()
                        .is_none_or(|guid| guid == sent.guid)
                    && row.text.as_deref().and_then(parse_reply).is_some_and(
                        |(reply_token, option)| reply_token == token && option < choice_count,
                    )
            })
            .count();
    result.classification = if result.reply_rows_present > 0 {
        "HISTORY_ROW_PRESENT_WATCH_MISSED"
    } else {
        "HISTORY_ROW_STILL_ABSENT"
    }
    .into();
    result
}

pub async fn diagnose_history(
    config: &IMessageChannelConfig,
    locator: &str,
) -> Result<HistoryDiagnosis, HealthState> {
    if !valid_diagnostic_locator(locator) {
        return Err(HealthState::MessagesUnavailable);
    }
    let Readiness::Ready(chat) = prepare(config).await? else {
        return Err(HealthState::BootstrapRequired);
    };
    let rows = history(chat.id, 200).await?;
    Ok(diagnose_rows(&rows, locator, chat.id))
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

enum BootstrapResolutionStep {
    Ready(ResolvedRequest),
    Retry,
    Fail(HealthState),
}

fn bootstrap_resolution_step(
    config: &IMessageChannelConfig,
    boundary: &PreSendBoundary,
    receipt: &SendReceipt,
    text: &str,
    chats: &[ChatRecord],
    messages: &[InboundMessage],
) -> BootstrapResolutionStep {
    match resolve_sent_request(config, boundary, receipt, text, chats, messages) {
        Ok(resolved) => BootstrapResolutionStep::Ready(resolved),
        Err(HealthState::MessagesUnavailable) => BootstrapResolutionStep::Retry,
        Err(state) => BootstrapResolutionStep::Fail(state),
    }
}

async fn resolve_bootstrap_after_send(
    config: &IMessageChannelConfig,
    boundary: &PreSendBoundary,
    receipt: &SendReceipt,
    text: &str,
) -> Result<ResolvedRequest, HealthState> {
    let deadline = Instant::now() + POST_SEND_RESOLUTION_TIMEOUT;
    loop {
        let chats = list_chats(POST_SEND_CHAT_LIMIT, Duration::from_secs(2)).await?;
        let mut messages = Vec::new();
        let mut history_unavailable = false;
        for chat in chats
            .iter()
            .filter(|chat| post_send_chat_matches(config, chat))
        {
            match history(chat.id, POST_SEND_HISTORY_LIMIT).await {
                Ok(mut rows) => messages.append(&mut rows),
                Err(HealthState::MessagesUnavailable) => history_unavailable = true,
                Err(state) => return Err(state),
            }
        }
        if !history_unavailable {
            match bootstrap_resolution_step(config, boundary, receipt, text, &chats, &messages) {
                BootstrapResolutionStep::Ready(resolved) => return Ok(resolved),
                BootstrapResolutionStep::Fail(state) => return Err(state),
                BootstrapResolutionStep::Retry => {}
            }
        }
        if Instant::now() >= deadline {
            return Err(HealthState::MessagesUnavailable);
        }
        sleep(POST_SEND_RESOLUTION_RETRY).await;
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
    if matches!(readiness, Readiness::BootstrapRequired) {
        return resolve_bootstrap_after_send(config, boundary, receipt, text).await;
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

/// Live watch plus a finite catch-up budget. Both paths use the same pending ledger.
/// Lines::next_line preserves partial NDJSON across timer cancellation.
pub async fn wait_correlated_reply(
    reader: &mut BufReader<ChildStdout>,
    pending: &mut PendingReplies,
    chat_id: i64,
    expires_at_ms: u64,
) -> Result<CorrelatedReply, HealthState> {
    let mut lines = reader.lines();
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut scans = 0;
    let remaining = expires_at_ms.saturating_sub(unix_millis(SystemTime::now()));
    let expiry = sleep(Duration::from_millis(remaining));
    tokio::pin!(expiry);
    loop {
        tokio::select! {
            biased;
            _ = &mut expiry => return Err(HealthState::WatchFailed),
            line = lines.next_line() => {
                let line = line.map_err(|_| HealthState::WatchFailed)?.ok_or(HealthState::WatchFailed)?;
                let message = serde_json::from_str(&line).map_err(|_| HealthState::WatchFailed)?;
                if let Some(reply) = pending.resolve(&message, unix_millis(SystemTime::now())) { return Ok(reply); }
            }
            _ = tick.tick(), if scans < 60 => {
                scans += 1;
                let mut rows = history(chat_id, 200).await?;
                rows.sort_by_key(|row| row.id);
                for message in rows {
                    if let Some(reply) = pending.resolve(&message, unix_millis(SystemTime::now())) { return Ok(reply); }
                }
            }
        }
    }
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

    #[test]
    fn history_diagnosis_is_bounded_redacted_and_never_a_canonical_answer() {
        let locator = "https://github.com/cigit-zgy/human-in-loop/blob/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/reports/chatgpt/260910_chatgpt_04.md";
        assert!(valid_diagnostic_locator(locator));
        assert!(!valid_diagnostic_locator("file:///private/chat.db"));
        let rendered = render_confirmation(&request(), "48273", "test", None).unwrap();
        let body = rendered.text.replacen("\n", &format!("\n{locator}\n"), 1);
        let sent = InboundMessage {
            id: 10,
            chat_id: 2,
            guid: "sent".into(),
            reply_to_guid: None,
            created_at: String::new(),
            is_from_me: true,
            text: Some(body),
            is_reaction: false,
            has_attachments: false,
        };
        let mut reply = sent.clone();
        reply.id = 11;
        reply.guid = "reply".into();
        reply.is_from_me = false;
        reply.text = Some("48273-1".into());
        assert_eq!(
            diagnose_rows(std::slice::from_ref(&sent), locator, 2).classification,
            "HISTORY_ROW_STILL_ABSENT"
        );
        let result = diagnose_rows(&[reply.clone(), sent.clone()], locator, 2);
        assert_eq!(result.classification, "HISTORY_ROW_PRESENT_WATCH_MISSED");
        assert_eq!(result.reply_rows_present, 1);
        let encoded = serde_json::to_string(&result).unwrap();
        assert!(!encoded.contains("48273"));
        assert!(!encoded.contains("choice"));
        reply.reply_to_guid = Some("other-send".into());
        assert_eq!(
            diagnose_rows(&[reply.clone(), sent.clone()], locator, 2).reply_rows_present,
            0
        );
        reply.reply_to_guid = None;
        reply.is_reaction = true;
        assert_eq!(
            diagnose_rows(&[reply.clone(), sent.clone()], locator, 2).reply_rows_present,
            0
        );
        assert_eq!(
            diagnose_rows(&[reply], locator, 2).classification,
            "INSUFFICIENT_EVIDENCE"
        );
        assert_eq!(
            diagnose_rows(&[sent.clone(), sent.clone()], locator, 2).classification,
            "INSUFFICIENT_EVIDENCE"
        );

        let mut old_footer = sent.clone();
        old_footer.text = old_footer
            .text
            .map(|text| text.replace("\n\nReply:\n\n48273-1", "\n\nReply: 48273-1"));
        assert_eq!(
            diagnose_rows(&[old_footer], locator, 2).classification,
            "INSUFFICIENT_EVIDENCE"
        );

        let mut adjacent_choices = sent.clone();
        adjacent_choices.text = adjacent_choices
            .text
            .map(|text| text.replace("\n\n2  Stop", "\n2  Stop"));
        assert_eq!(
            diagnose_rows(&[adjacent_choices], locator, 2).classification,
            "INSUFFICIENT_EVIDENCE"
        );
    }

    fn request() -> ConfirmRequest {
        ConfirmRequest {
            id: "request-123".into(),
            title: "Delete generated objects".into(),
            context: vec![],
            detail: ConfirmDetail {
                summary: "Continue?".into(),
                body_md: String::new(),
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
            ..IMessageChannelConfig::default()
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
        let rendered = render_confirmation(&request(), "48273", "Codex", Some("human-in-loop"))
            .expect("supported request");
        assert_eq!(rendered.choice_indices, vec![0, 1]);
        assert_eq!(
            rendered.text,
            "[HIL · 48273]\n\nCodex · human-in-loop\n\nContinue?\n\n1  Continue [recommended]\n\n2  Stop\n\nReply:\n\n48273-1"
        );
        assert_eq!(rendered.text.lines().last(), Some("48273-1"));
        assert!(rendered.text.ends_with("Reply:\n\n48273-1"));
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
            "48273",
            "Codex\nAgent",
            Some("human-in-loop\nproject"),
        )
        .unwrap();
        assert!(rendered
            .text
            .contains("\n\nCodex Agent · human-in-loop project\n\n"));
        assert!(rendered
            .text
            .contains("\n\nRelease status: candidate build\n\n"));
        assert!(rendered
            .text
            .contains("\nDoes this compact layout look correct?\n"));
        assert!(rendered
            .text
            .contains("\n1  Looks correct [recommended]\n\n"));
    }

    #[test]
    fn renderer_preserves_multiline_detail_as_decision_evidence() {
        let mut multiline = request();
        multiline.detail.body_md = "line A\nline B\n\nline C".into();
        let rendered = render_confirmation(&multiline, "48273", "Codex", None).unwrap();

        assert!(rendered
            .text
            .contains("\n\nline A\nline B\n\nline C\n\nContinue?\n"));
        assert!(!rendered.text.contains("line A line B line C"));
    }

    #[test]
    fn renderer_enforces_default_detail_character_budget_without_truncation() {
        for count in [999, 1000] {
            let mut bounded = request();
            bounded.detail.body_md = "界".repeat(count);
            let rendered = render_confirmation(&bounded, "48273", "Codex", None).unwrap();
            assert_eq!(rendered.text.matches('界').count(), count);
        }

        let mut over = request();
        over.detail.body_md = "界".repeat(1001);
        assert_eq!(
            format!(
                "{:?}",
                render_confirmation(&over, "48273", "Codex", None).unwrap_err()
            ),
            "DetailTooLong"
        );
    }

    #[test]
    fn renderer_enforces_the_default_full_rendered_budget_separately() {
        fn boundary_request(detail_chars: usize) -> ConfirmRequest {
            let mut boundary = request();
            boundary.context = vec![
                context("甲", &"界".repeat(77)),
                context("乙", &"文".repeat(77)),
            ];
            boundary.detail.summary = "问".repeat(160);
            boundary.detail.body_md = "证".repeat(detail_chars);
            boundary.presentation = ConfirmPresentation::SingleSelectSubmit {
                input: None,
                submit_label: "Submit".into(),
                default_action_id: None,
            };
            boundary.choices = (0..6)
                .map(|index| ConfirmChoice {
                    id: format!("choice-{index}"),
                    label: "选".repeat(60),
                    description: String::new(),
                    role: ActionRole::Default,
                    variant: None,
                })
                .collect();
            boundary.dismiss_action_id = "choice-5".into();
            boundary
        }

        let exact =
            render_confirmation(&boundary_request(671), "48273", &"S".repeat(80), None).unwrap();
        assert_eq!(exact.text.chars().count(), 1500);
        assert_eq!(
            format!(
                "{:?}",
                render_confirmation(&boundary_request(672), "48273", &"S".repeat(80), None)
                    .unwrap_err()
            ),
            "RenderedTextTooLong"
        );
    }

    #[test]
    fn renderer_accepts_each_supported_choice_count_with_blank_line_separation() {
        for count in MIN_CHOICES..=MAX_CHOICES {
            let mut bounded = request();
            bounded.choices = (0..count)
                .map(|index| ConfirmChoice {
                    id: format!("choice-{index}"),
                    label: format!("Choice {}", index + 1),
                    description: String::new(),
                    role: ActionRole::Default,
                    variant: None,
                })
                .collect();
            bounded.dismiss_action_id = format!("choice-{}", count - 1);

            let rendered = render_confirmation(&bounded, "48273", "Codex", None).unwrap();
            for index in 1..count {
                assert!(rendered.text.contains(&format!(
                    "\n\n{}  Choice {}\n\n{}  Choice {}",
                    index,
                    index,
                    index + 1,
                    index + 1
                )));
            }
        }
    }

    #[test]
    fn configured_decision_budgets_allow_larger_bodies_and_reject_invalid_values() {
        let mut request = request();
        request.detail.body_md = "界".repeat(1500);
        assert_eq!(
            render_confirmation(&request, "48273", "Codex", None),
            Err(UnsupportedReason::DetailTooLong)
        );

        let mut config = IMessageChannelConfig {
            decision_detail_max_chars: 2000,
            decision_rendered_max_chars: 2600,
            ..IMessageChannelConfig::default()
        };
        let rendered =
            render_confirmation_with_config(&request, "48273", "Codex", None, &config).unwrap();
        assert_eq!(rendered.text.matches('界').count(), 1500);

        config.decision_detail_max_chars = 1500;
        config.decision_rendered_max_chars = 1586;
        let exact =
            render_confirmation_with_config(&request, "48273", "Codex", None, &config).unwrap();
        assert_eq!(exact.text.chars().count(), 1586);
        config.decision_rendered_max_chars = 1585;
        assert_eq!(
            render_confirmation_with_config(&request, "48273", "Codex", None, &config),
            Err(UnsupportedReason::RenderedTextTooLong)
        );

        for (detail, rendered) in [
            (0, 1500),
            (1000, 0),
            (1501, 1500),
            (4501, 5000),
            (4500, 5001),
        ] {
            config.decision_detail_max_chars = detail;
            config.decision_rendered_max_chars = rendered;
            assert_eq!(
                render_confirmation_with_config(&request, "48273", "Codex", None, &config,),
                Err(UnsupportedReason::InvalidDecisionBudget)
            );
        }

        config.decision_detail_max_chars = 4500;
        config.decision_rendered_max_chars = 5000;
        request.detail.body_md = "界".repeat(4500);
        assert!(
            render_confirmation_with_config(&request, "48273", "Codex", None, &config,).is_ok()
        );
    }

    #[test]
    fn renderer_emits_zero_one_or_two_required_context_lines() {
        let zero = render_confirmation(&request(), "48273", "Codex", None).unwrap();
        assert_eq!(zero.text.lines().nth(2), Some("Codex"));
        assert!(!zero.text.contains("Release: candidate"));

        let mut one_request = request();
        one_request.context = vec![context("Release", "candidate")];
        let one = render_confirmation(&one_request, "48273", "Codex", None).unwrap();
        assert!(one.text.contains("\nRelease: candidate\n\nContinue?"));

        let mut two_request = one_request.clone();
        two_request.context.push(context("Risk", "renderer only"));
        let two = render_confirmation(&two_request, "48273", "Codex", None).unwrap();
        assert!(two
            .text
            .contains("\nRelease: candidate\nRisk: renderer only\n\nContinue?"));

        let mut three_request = two_request;
        three_request.context.push(context("Owner", "User"));
        assert!(render_confirmation(&three_request, "48273", "Codex", None).is_err());
    }

    #[test]
    fn renderer_enforces_line_and_field_budgets_without_truncation() {
        let mut long_title = request();
        long_title.title = "Title is canonical but not duplicated on the phone. ".repeat(4);
        let rendered = render_confirmation(&long_title, "48273", "Codex", Some("human-in-loop"))
            .expect("the compact renderer does not copy the title");
        assert!(!rendered.text.contains("Title is canonical"));

        let source = "S".repeat(80);
        let rendered = render_confirmation(&request(), "48273", &source, None).unwrap();
        assert_eq!(rendered.text.lines().nth(2), Some(source.as_str()));
        assert!(render_confirmation(&request(), "48273", &"S".repeat(81), None).is_err());

        let mut long_context = request();
        long_context.context = vec![context("L", &"界".repeat(78))];
        assert!(render_confirmation(&long_context, "48273", "Codex", None).is_err());

        let mut long_question = request();
        long_question.detail.summary = "界".repeat(161);
        assert!(render_confirmation(&long_question, "48273", "Codex", None).is_err());

        let mut long_choice = request();
        long_choice.choices[0].label = "界".repeat(61);
        assert!(render_confirmation(&long_choice, "48273", "Codex", None).is_err());
    }

    #[test]
    fn renderer_never_compacts_away_a_repository_label() {
        let repository = "R".repeat(10);
        let source = "S".repeat(67);
        let rendered =
            render_confirmation(&request(), "48273", &source, Some(&repository)).unwrap();
        assert_eq!(rendered.text.lines().nth(2).unwrap().chars().count(), 80);
        assert!(rendered.text.lines().nth(2).unwrap().ends_with(&repository));

        let source = "S".repeat(68);
        assert_eq!(
            render_confirmation(&request(), "48273", &source, Some(&repository)),
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
        let rendered = render_confirmation(&bounded, "48273", &source, None).unwrap();
        assert!(rendered.text.contains(&first_context));
        assert!(rendered.text.contains(&second_context));
        assert!(rendered.text.contains(&question));
        assert_eq!(rendered.text.matches(&label).count(), 3);
        assert!(rendered.text.chars().count() <= 700);

        bounded.choices.push(ConfirmChoice {
            id: "fourth".into(),
            label: label.clone(),
            description: String::new(),
            role: ActionRole::Default,
            variant: None,
        });
        let rendered = render_confirmation(&bounded, "48273", &source, None).unwrap();
        assert_eq!(rendered.text.matches(&label).count(), 4);

        for id in ["fifth", "sixth"] {
            bounded.choices.push(ConfirmChoice {
                id: id.into(),
                label: label.clone(),
                description: String::new(),
                role: ActionRole::Default,
                variant: None,
            });
        }
        bounded.detail.body_md = "证".repeat(1000);
        assert_eq!(
            render_confirmation(&bounded, "48273", &source, None),
            Err(UnsupportedReason::RenderedTextTooLong)
        );
    }

    #[test]
    fn renderer_keeps_choice_count_and_interactive_input_fail_closed() {
        let mut one_choice = request();
        one_choice.choices.truncate(1);
        assert_eq!(
            render_confirmation(&one_choice, "48273", "Codex", None),
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
            render_confirmation(&seven_choices, "48273", "Codex", None),
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
            render_confirmation(&with_input, "48273", "Codex", Some("human-in-loop")),
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
        assert_eq!(parse_reply("48273-2"), Some(("48273".into(), 1)));
        assert_eq!(parse_reply(" \t48273-1\n"), Some(("48273".into(), 0)));
        for invalid in [
            "1",
            "48273",
            "48273 1",
            "48273 - 1",
            "48273--1",
            "48273_1",
            "A7F3-1",
            "04827-1",
            "48273-0",
            "48273-01",
            "48273-two",
            "48273-1 extra",
            "prefix 48273-1",
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
    fn token_registry_keeps_active_tokens_unique_and_releases_only_the_owner() {
        let mut registry = TokenRegistry::default();
        let tokens = (0..1_000)
            .map(|index| registry.allocate(&format!("request-{index}")))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(tokens.len(), 1_000);
        assert!(tokens.iter().all(|token| valid_token(token)));

        let token = registry.allocate("release-fixture");
        registry.release(&token, "different-request");
        assert_eq!(
            registry.by_token.get(&token).map(String::as_str),
            Some("release-fixture")
        );
        registry.release(&token, "release-fixture");
        assert!(!registry.by_token.contains_key(&token));
        assert_eq!(registry.allocate("release-fixture"), token);
    }

    #[test]
    fn token_registry_uses_salted_rehash_after_exhausting_one_digest() {
        let request_id = "request-with-forced-prefix-collisions";
        let digest = Sha256::digest(request_id.as_bytes());
        let prefix = u32::from_be_bytes(digest[..4].try_into().unwrap());
        let mut token = (10_000 + prefix % 90_000).to_string();
        let mut registry = TokenRegistry::default();
        registry.by_token.insert(token.clone(), "occupied-5".into());
        for (index, chunk) in digest[4..].chunks_exact(2).enumerate() {
            let suffix = u16::from_be_bytes([chunk[0], chunk[1]]) % 100;
            token.push_str(&format!("{suffix:02}"));
            registry
                .by_token
                .insert(token.clone(), format!("occupied-{}", 7 + index * 2));
        }

        let allocated = registry.allocate(request_id);
        assert!(valid_token(&allocated));
        assert_ne!(allocated, token);
        assert_eq!(
            registry.by_token.get(&allocated).map(String::as_str),
            Some(request_id)
        );
    }

    #[test]
    fn same_account_accepts_only_a_strictly_post_send_reply() {
        let mut pending = PendingReplies::default();
        pending.register(
            "48273",
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
            text: Some("48273-2".into()),
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
            text: Some("48273-1".into()),
            is_reaction: false,
            has_attachments: false,
        };

        for (token, message) in [
            (
                "48273",
                InboundMessage {
                    id: 100,
                    guid: "REQUEST-GUID".into(),
                    ..candidate.clone()
                },
            ),
            (
                "48273",
                InboundMessage {
                    id: 99,
                    ..candidate.clone()
                },
            ),
            (
                "48273",
                InboundMessage {
                    chat_id: 7,
                    ..candidate.clone()
                },
            ),
            (
                "48273",
                InboundMessage {
                    text: Some("FFFF 1".into()),
                    ..candidate.clone()
                },
            ),
            (
                "48273",
                InboundMessage {
                    text: Some("48273-7".into()),
                    ..candidate.clone()
                },
            ),
            (
                "48273",
                InboundMessage {
                    text: None,
                    has_attachments: true,
                    ..candidate.clone()
                },
            ),
            (
                "48273",
                InboundMessage {
                    is_reaction: true,
                    ..candidate.clone()
                },
            ),
        ] {
            register(&mut pending, token);
            assert_eq!(pending.resolve(&message, 1_500), None);
        }

        register(&mut pending, "48273");
        assert_eq!(pending.resolve(&candidate, 2_001), None, "expired reply");
    }

    #[test]
    fn inline_reply_guid_must_match_the_sent_request() {
        let mut pending = PendingReplies::default();
        pending.register(
            "48273",
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
            text: Some("48273-1".into()),
            is_reaction: false,
            has_attachments: false,
        };
        assert_eq!(pending.resolve(&message, 1_500), None);
    }

    #[test]
    fn distinct_peer_still_rejects_is_from_me() {
        let mut pending = PendingReplies::default();
        pending.register(
            "48273",
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
            text: Some("48273-1".into()),
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
    fn bootstrap_resolution_retries_until_the_new_chat_is_queryable() {
        let mut config = imessage_config();
        config.chat_id = None;
        config.chat_guid.clear();
        let receipt = SendReceipt {
            row_id: 101,
            guid: "REQUEST-GUID".into(),
        };
        let boundary = PreSendBoundary {
            latest_row_id: None,
            started_at_ms: 0,
        };

        assert!(matches!(
            bootstrap_resolution_step(&config, &boundary, &receipt, "rendered request", &[], &[],),
            BootstrapResolutionStep::Retry
        ));

        let chats = vec![ChatRecord {
            id: 42,
            guid: Some("iMessage;-;direct".into()),
            service: "iMessage".into(),
            is_group: false,
            participants: vec![config.recipient.clone()],
        }];
        let messages = vec![InboundMessage {
            id: receipt.row_id,
            chat_id: 42,
            guid: receipt.guid.clone(),
            reply_to_guid: None,
            created_at: "2026-09-07T10:00:00.000Z".into(),
            is_from_me: true,
            text: Some("rendered request".into()),
            is_reaction: false,
            has_attachments: false,
        }];
        assert!(matches!(
            bootstrap_resolution_step(
                &config,
                &boundary,
                &receipt,
                "rendered request",
                &chats,
                &messages,
            ),
            BootstrapResolutionStep::Ready(_)
        ));
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
        assert_eq!(first, "19912", "stable SHA-256 decimal fixture changed");
        assert_eq!(first.len(), 5);
        assert_ne!(first.as_bytes()[0], b'0');
        assert!(first.bytes().all(|byte| byte.is_ascii_digit()));

        let mut collided = TokenRegistry::default();
        collided.by_token.insert(first.clone(), "occupied-5".into());
        let seven = collided.allocate("request-a");
        assert_eq!(seven, "1991262");
        assert_eq!(&seven[..5], first);

        let mut collided_twice = TokenRegistry::default();
        collided_twice
            .by_token
            .insert(first.clone(), "occupied-5".into());
        collided_twice
            .by_token
            .insert(seven.clone(), "occupied-7".into());
        let nine = collided_twice.allocate("request-a");
        assert_eq!(nine, "199126233");
        assert_eq!(&nine[..7], seven);
    }

    #[test]
    fn configured_deadline_helper_uses_wall_clock_only_for_expiry_comparison() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_millis(1_500);
        assert_eq!(unix_millis(now), 1_500);
    }
}
