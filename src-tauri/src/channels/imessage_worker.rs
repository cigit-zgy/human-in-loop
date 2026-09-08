//! Narrow cross-user boundary for the Bot-owned Apple Messages transport.

use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const WORKER_SOCKET_PATH: &str = "/Users/Shared/human-in-loop-imessage-worker.sock";
const WORKER_LABEL: &str = "io.github.cigit-zgy.human-in-loop.imessage-worker";
const MAX_FRAME_BYTES: usize = 16 * 1024;
const SHARED_BINARY: &str = "/Users/Shared/human-in-loop/bin/human-in-loop";
const SHARED_IMSG: &str = "/Users/Shared/human-in-loop/bin/imsg";
const BOT_SENDER_ENV: &str = "HUMAN_IN_LOOP_BOT_SENDER";
const RECIPIENT_ENV: &str = "HUMAN_IN_LOOP_IMESSAGE_RECIPIENT";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerConfig {
    coordinator_uid: u32,
    coordinator_gid: u32,
    bot_sender: String,
}

impl WorkerConfig {
    pub fn new(
        coordinator_uid: u32,
        coordinator_gid: u32,
        bot_sender: &str,
        recipient: &str,
    ) -> Result<Self, String> {
        let bot_sender = bot_sender.trim();
        let recipient = recipient.trim();
        if coordinator_uid == 0 {
            return Err("coordinator user must not be root".into());
        }
        if bot_sender.is_empty() {
            return Err("Bot sender identity is required".into());
        }
        if recipient.is_empty() {
            return Err("recipient identity is required".into());
        }
        if bot_sender.eq_ignore_ascii_case(recipient) {
            return Err("SELF_MESSAGE_UNSUPPORTED".into());
        }
        Ok(Self {
            coordinator_uid,
            coordinator_gid,
            bot_sender: bot_sender.to_string(),
        })
    }
}

fn save_worker_config(path: &Path, config: &WorkerConfig) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(config).map_err(std::io::Error::other)?;
    crate::integrations::hook_edit::atomic_write_private(path, &bytes)
        .map_err(std::io::Error::other)
}

fn worker_config_path() -> std::path::PathBuf {
    crate::paths::config_dir().join("imessage-worker.json")
}

fn load_worker_config(path: &Path) -> Result<WorkerConfig, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = std::fs::metadata(path).map_err(|_| "Bot worker is not configured")?;
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o777 != 0o600
        {
            return Err("Bot worker configuration ownership is invalid".into());
        }
    }
    let bytes = std::fs::read(path).map_err(|_| "Bot worker is not configured")?;
    let config: WorkerConfig =
        serde_json::from_slice(&bytes).map_err(|_| "Bot worker configuration is invalid")?;
    if config.coordinator_uid == 0 || config.bot_sender.trim().is_empty() {
        return Err("Bot worker configuration is invalid".into());
    }
    Ok(config)
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn launch_agent_plist(executable: &str, home: &str) -> String {
    let bin_dir = Path::new(executable)
        .parent()
        .unwrap_or_else(|| Path::new("/usr/local/bin"))
        .display()
        .to_string();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{WORKER_LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>imessage-worker</string><string>run</string></array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>HUMAN_IN_LOOP_HOME</key><string>{}</string>
    <key>PATH</key><string>{}:/usr/bin:/bin:/usr/sbin:/sbin</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Interactive</string>
  <key>StandardOutPath</key><string>/dev/null</string>
  <key>StandardErrorPath</key><string>/dev/null</string>
</dict>
</plist>
"#,
        xml(executable),
        xml(home),
        xml(&bin_dir),
    )
}

fn worker_runtime_path() -> String {
    format!(
        "{}:/usr/bin:/bin:/usr/sbin:/sbin",
        Path::new(SHARED_IMSG)
            .parent()
            .unwrap_or_else(|| Path::new("/Users/Shared/human-in-loop/bin"))
            .display()
    )
}

fn parse_install_args(args: &[String]) -> Result<String, String> {
    if args.len() != 2 || args[0] != "--coordinator-user" {
        return Err("usage: imessage-worker install --coordinator-user <short-name>".into());
    }
    let username = args[1].trim();
    if username.is_empty()
        || username.len() > 64
        || !username
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err("coordinator user short name is invalid".into());
    }
    Ok(username.to_string())
}

fn parse_uid(stdout: &[u8]) -> Result<u32, String> {
    let value = std::str::from_utf8(stdout)
        .map_err(|_| "user id is invalid")?
        .trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("user id is invalid".into());
    }
    let uid = value.parse::<u32>().map_err(|_| "user id is invalid")?;
    if uid == 0 {
        return Err("root is not an allowed worker peer".into());
    }
    Ok(uid)
}

fn lookup_uid(username: &str) -> Result<u32, String> {
    let output = std::process::Command::new("/usr/bin/id")
        .args(["-u", username])
        .output()
        .map_err(|_| "cannot resolve macOS user")?;
    if !output.status.success() {
        return Err("macOS user is unavailable".into());
    }
    parse_uid(&output.stdout)
}

fn lookup_gid(username: &str) -> Result<u32, String> {
    let output = std::process::Command::new("/usr/bin/id")
        .args(["-g", username])
        .output()
        .map_err(|_| "cannot resolve macOS user group")?;
    if !output.status.success() {
        return Err("macOS user group is unavailable".into());
    }
    parse_uid(&output.stdout).map_err(|_| "user group id is invalid".into())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WorkerRequest {
    Health {
        recipient: String,
    },
    Confirm {
        recipient: String,
        request_id: String,
        token: String,
        text: String,
        choice_indices: Vec<usize>,
        expires_at_ms: u64,
    },
    Notify {
        recipient: String,
        text: String,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "event",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WorkerResponse {
    Health { state: String },
    Ready,
    Answer { choice_index: usize },
    Sent,
    Error { state: String },
}

pub const fn peer_allowed(expected_uid: u32, actual_uid: u32) -> bool {
    expected_uid == actual_uid
}

pub const fn required_for(mode: crate::config::IMessageIdentityMode) -> bool {
    matches!(mode, crate::config::IMessageIdentityMode::DistinctPeer)
}

pub fn admit_cross_user_image(
    has_image: bool,
    required_for_decision: bool,
) -> Result<(), crate::channels::imessage::UnsupportedReason> {
    if has_image && required_for_decision {
        Err(crate::channels::imessage::UnsupportedReason::RequiredImageUnsupported)
    } else {
        Ok(())
    }
}

fn request_recipient(request: &WorkerRequest) -> &str {
    match request {
        WorkerRequest::Health { recipient }
        | WorkerRequest::Confirm { recipient, .. }
        | WorkerRequest::Notify { recipient, .. } => recipient,
    }
}

fn validate_request(
    worker: &WorkerConfig,
    channel: &crate::config::IMessageChannelConfig,
    request: &WorkerRequest,
) -> Result<(), crate::channels::imessage::HealthState> {
    use crate::channels::imessage::HealthState;
    use crate::config::IMessageIdentityMode;

    if !channel.enabled
        || channel.identity_mode != IMessageIdentityMode::DistinctPeer
        || channel.recipient.trim().is_empty()
    {
        return Err(HealthState::NotConfigured);
    }
    if worker.bot_sender.trim().is_empty() {
        return Err(HealthState::BotSenderIdentityUnverified);
    }
    if worker
        .bot_sender
        .trim()
        .eq_ignore_ascii_case(channel.recipient.trim())
    {
        return Err(HealthState::SelfMessageUnsupported);
    }
    if request_recipient(request).trim() != channel.recipient.trim() {
        return Err(HealthState::RecipientNotImessage);
    }

    match request {
        WorkerRequest::Health { .. } => Ok(()),
        WorkerRequest::Notify { text, .. } => {
            if text.is_empty() || text.chars().count() > super::imessage::MAX_RENDERED_CHARS {
                Err(HealthState::MessagesUnavailable)
            } else {
                Ok(())
            }
        }
        WorkerRequest::Confirm {
            request_id,
            token,
            text,
            choice_indices,
            expires_at_ms,
            ..
        } => {
            let unique = choice_indices
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>();
            if request_id.trim().is_empty()
                || request_id.chars().count() > 128
                || !(4..=64).contains(&token.len())
                || !token
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_lowercase())
                || text.is_empty()
                || text.chars().count() > super::imessage::MAX_RENDERED_CHARS
                || !(2..=6).contains(&choice_indices.len())
                || unique.len() != choice_indices.len()
                || *expires_at_ms == 0
            {
                Err(HealthState::MessagesUnavailable)
            } else {
                Ok(())
            }
        }
    }
}

async fn read_frame<R, T>(reader: &mut R) -> Result<T, String>
where
    R: AsyncBufRead + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut bytes = Vec::new();
    let read = reader
        .take((MAX_FRAME_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .await
        .map_err(|_| "worker IPC read failed".to_string())?;
    if read == 0 || read > MAX_FRAME_BYTES || bytes.last() != Some(&b'\n') {
        return Err("worker IPC frame is missing or oversized".into());
    }
    bytes.pop();
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    serde_json::from_slice(&bytes).map_err(|_| "worker IPC frame is invalid".into())
}

async fn write_frame<W, T>(writer: &mut W, value: &T) -> Result<(), String>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let bytes = serde_json::to_vec(value).map_err(|_| "worker IPC encode failed".to_string())?;
    if bytes.len() + 1 > MAX_FRAME_BYTES {
        return Err("worker IPC frame is oversized".into());
    }
    writer
        .write_all(&bytes)
        .await
        .map_err(|_| "worker IPC write failed".to_string())?;
    writer
        .write_all(b"\n")
        .await
        .map_err(|_| "worker IPC write failed".to_string())?;
    writer
        .flush()
        .await
        .map_err(|_| "worker IPC write failed".to_string())
}

#[cfg(target_os = "macos")]
fn peer_uid(stream: &tokio::net::UnixStream) -> std::io::Result<u32> {
    use std::os::fd::AsRawFd;

    let mut uid = 0;
    let mut gid = 0;
    let result = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    if result == 0 {
        Ok(uid)
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
pub struct WorkerClient {
    reader: tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

#[cfg(target_os = "macos")]
impl WorkerClient {
    async fn connect() -> Result<Self, crate::channels::imessage::HealthState> {
        let uid = lookup_uid("human-in-loop")
            .map_err(|_| crate::channels::imessage::HealthState::BotSessionLoginRequired)?;
        Self::connect_at(Path::new(WORKER_SOCKET_PATH), uid)
            .await
            .map_err(|_| crate::channels::imessage::HealthState::BotSessionLoginRequired)
    }

    async fn connect_at(path: &Path, expected_uid: u32) -> Result<Self, String> {
        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            tokio::net::UnixStream::connect(path),
        )
        .await
        .map_err(|_| "Bot worker connection timed out".to_string())?
        .map_err(|_| "Bot worker is unavailable".to_string())?;
        let actual_uid = peer_uid(&stream).map_err(|_| "Bot worker identity is unavailable")?;
        if !peer_allowed(expected_uid, actual_uid) {
            return Err("Bot worker peer identity mismatch".into());
        }
        let (read, write) = stream.into_split();
        Ok(Self {
            reader: tokio::io::BufReader::new(read),
            writer: write,
        })
    }

    pub async fn send(&mut self, request: &WorkerRequest) -> Result<(), String> {
        write_frame(&mut self.writer, request).await
    }

    pub async fn next(&mut self) -> Result<WorkerResponse, String> {
        read_frame(&mut self.reader).await
    }
}

fn health_state(value: &str) -> crate::channels::imessage::HealthState {
    use crate::channels::imessage::HealthState;
    [
        HealthState::NotConfigured,
        HealthState::BotSessionLoginRequired,
        HealthState::BotMessagesAccountUnavailable,
        HealthState::BotSenderIdentityUnverified,
        HealthState::SelfMessageUnsupported,
        HealthState::ImsgMissing,
        HealthState::PermissionMissing,
        HealthState::MessagesUnavailable,
        HealthState::ImsgIncompatible,
        HealthState::BootstrapRequired,
        HealthState::RecipientNotImessage,
        HealthState::AmbiguousChat,
        HealthState::WatchFailed,
        HealthState::SendFailed,
        HealthState::Ready,
    ]
    .into_iter()
    .find(|state| state.as_str() == value)
    .unwrap_or(HealthState::MessagesUnavailable)
}

#[cfg(target_os = "macos")]
pub async fn health(
    config: &crate::config::IMessageChannelConfig,
) -> crate::channels::imessage::HealthState {
    let mut client = match WorkerClient::connect().await {
        Ok(client) => client,
        Err(state) => return state,
    };
    if client
        .send(&WorkerRequest::Health {
            recipient: config.recipient.clone(),
        })
        .await
        .is_err()
    {
        return crate::channels::imessage::HealthState::BotSessionLoginRequired;
    }
    match client.next().await {
        Ok(WorkerResponse::Health { state }) | Ok(WorkerResponse::Error { state }) => {
            health_state(&state)
        }
        _ => crate::channels::imessage::HealthState::MessagesUnavailable,
    }
}

#[cfg(not(target_os = "macos"))]
pub async fn health(
    _config: &crate::config::IMessageChannelConfig,
) -> crate::channels::imessage::HealthState {
    crate::channels::imessage::HealthState::BotSessionLoginRequired
}

#[cfg(target_os = "macos")]
pub async fn start_confirm(
    config: &crate::config::IMessageChannelConfig,
    request_id: &str,
    token: &str,
    text: &str,
    choice_indices: Vec<usize>,
    expires_at_ms: u64,
) -> Result<WorkerClient, crate::channels::imessage::HealthState> {
    let mut client = WorkerClient::connect().await?;
    client
        .send(&WorkerRequest::Confirm {
            recipient: config.recipient.clone(),
            request_id: request_id.to_string(),
            token: token.to_string(),
            text: text.to_string(),
            choice_indices,
            expires_at_ms,
        })
        .await
        .map_err(|_| crate::channels::imessage::HealthState::BotSessionLoginRequired)?;
    match client.next().await {
        Ok(WorkerResponse::Ready) => Ok(client),
        Ok(WorkerResponse::Error { state }) => Err(health_state(&state)),
        _ => Err(crate::channels::imessage::HealthState::MessagesUnavailable),
    }
}

#[cfg(not(target_os = "macos"))]
pub async fn start_confirm(
    _config: &crate::config::IMessageChannelConfig,
    _request_id: &str,
    _token: &str,
    _text: &str,
    _choice_indices: Vec<usize>,
    _expires_at_ms: u64,
) -> Result<(), crate::channels::imessage::HealthState> {
    Err(crate::channels::imessage::HealthState::BotSessionLoginRequired)
}

#[cfg(target_os = "macos")]
pub async fn notify(
    config: &crate::config::IMessageChannelConfig,
    text: &str,
) -> Result<(), crate::channels::imessage::HealthState> {
    let mut client = WorkerClient::connect().await?;
    client
        .send(&WorkerRequest::Notify {
            recipient: config.recipient.clone(),
            text: text.to_string(),
        })
        .await
        .map_err(|_| crate::channels::imessage::HealthState::BotSessionLoginRequired)?;
    match client.next().await {
        Ok(WorkerResponse::Sent) => Ok(()),
        Ok(WorkerResponse::Error { state }) => Err(health_state(&state)),
        _ => Err(crate::channels::imessage::HealthState::MessagesUnavailable),
    }
}

#[cfg(not(target_os = "macos"))]
pub async fn notify(
    _config: &crate::config::IMessageChannelConfig,
    _text: &str,
) -> Result<(), crate::channels::imessage::HealthState> {
    Err(crate::channels::imessage::HealthState::BotSessionLoginRequired)
}

#[cfg(target_os = "macos")]
async fn connection_closed(reader: &mut tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>) {
    let mut byte = [0_u8; 1];
    loop {
        match reader.read(&mut byte).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
    }
}

#[cfg(target_os = "macos")]
async fn prepare_confirmation(
    config: &crate::config::IMessageChannelConfig,
    text: &str,
) -> Result<
    (
        crate::channels::imessage::ResolvedRequest,
        tokio::process::Child,
        tokio::io::BufReader<tokio::process::ChildStdout>,
    ),
    crate::channels::imessage::HealthState,
> {
    use crate::channels::imessage;

    let readiness = imessage::prepare(config).await?;
    let boundary = imessage::pre_send_boundary(&readiness).await?;
    let receipt = imessage::send(config, text, None).await?;
    let resolved =
        imessage::resolve_after_send(config, &readiness, &boundary, &receipt, text).await?;
    imessage::persist_resolved_chat(config, &resolved)?;
    let (child, reader) = imessage::spawn_watch(resolved.chat.id, resolved.sent.row_id)?;
    Ok((resolved, child, reader))
}

#[cfg(target_os = "macos")]
async fn send_notification(
    config: &crate::config::IMessageChannelConfig,
    text: &str,
) -> Result<(), crate::channels::imessage::HealthState> {
    use crate::channels::imessage;

    let readiness = imessage::prepare(config).await?;
    let boundary = imessage::pre_send_boundary(&readiness).await?;
    let receipt = imessage::send(config, text, None).await?;
    let resolved =
        imessage::resolve_after_send(config, &readiness, &boundary, &receipt, text).await?;
    imessage::persist_resolved_chat(config, &resolved)
}

#[cfg(target_os = "macos")]
async fn write_error(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    state: crate::channels::imessage::HealthState,
) {
    let _ = write_frame(
        writer,
        &WorkerResponse::Error {
            state: state.as_str().to_string(),
        },
    )
    .await;
}

#[cfg(target_os = "macos")]
async fn handle_connection(
    stream: tokio::net::UnixStream,
    worker: WorkerConfig,
    channel: crate::config::IMessageChannelConfig,
) {
    use crate::channels::imessage::{self, HealthState};

    let Ok(uid) = peer_uid(&stream) else {
        return;
    };
    if !peer_allowed(worker.coordinator_uid, uid) {
        return;
    }
    let (read, mut write) = stream.into_split();
    let mut read = tokio::io::BufReader::new(read);
    let Ok(request) = read_frame::<_, WorkerRequest>(&mut read).await else {
        return;
    };
    if let Err(state) = validate_request(&worker, &channel, &request) {
        write_error(&mut write, state).await;
        return;
    }

    match request {
        WorkerRequest::Health { .. } => {
            let state = match imessage::health_local(&channel).await {
                HealthState::MessagesUnavailable => HealthState::BotMessagesAccountUnavailable,
                state => state,
            };
            let _ = write_frame(
                &mut write,
                &WorkerResponse::Health {
                    state: state.as_str().to_string(),
                },
            )
            .await;
        }
        WorkerRequest::Notify { text, .. } => {
            let outcome = tokio::select! {
                _ = connection_closed(&mut read) => return,
                outcome = send_notification(&channel, &text) => outcome,
            };
            match outcome {
                Ok(()) => {
                    let _ = write_frame(&mut write, &WorkerResponse::Sent).await;
                }
                Err(state) => write_error(&mut write, state).await,
            }
        }
        WorkerRequest::Confirm {
            request_id,
            token,
            text,
            choice_indices,
            expires_at_ms,
            ..
        } => {
            let prepared = tokio::select! {
                _ = connection_closed(&mut read) => return,
                outcome = prepare_confirmation(&channel, &text) => outcome,
            };
            let (resolved, mut child, mut inbound) = match prepared {
                Ok(prepared) => prepared,
                Err(state) => {
                    write_error(&mut write, state).await;
                    return;
                }
            };
            if write_frame(&mut write, &WorkerResponse::Ready)
                .await
                .is_err()
            {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return;
            }

            let mut pending = imessage::PendingReplies::default();
            pending.register(
                &token,
                &request_id,
                imessage::RequestBoundary {
                    identity_mode: crate::config::IMessageIdentityMode::DistinctPeer,
                    chat_id: resolved.chat.id,
                    sent_row_id: resolved.sent.row_id,
                    sent_guid: resolved.sent.guid,
                },
                choice_indices,
                expires_at_ms,
            );
            let mut failed = None;
            loop {
                tokio::select! {
                    _ = connection_closed(&mut read) => break,
                    message = imessage::read_inbound_line(&mut inbound) => match message {
                        Ok(Some(message)) => {
                            let Some(reply) = pending.resolve(
                                &message,
                                imessage::unix_millis(std::time::SystemTime::now()),
                            ) else {
                                continue;
                            };
                            if reply.request_id == request_id {
                                let _ = write_frame(
                                    &mut write,
                                    &WorkerResponse::Answer {
                                        choice_index: reply.choice_index,
                                    },
                                )
                                .await;
                                break;
                            }
                        }
                        Ok(None) => {}
                        Err(_) => {
                            failed = Some(HealthState::WatchFailed);
                            break;
                        }
                    }
                }
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
            if let Some(state) = failed {
                write_error(&mut write, state).await;
            }
        }
    }
}

#[cfg(target_os = "macos")]
async fn serve() -> Result<(), String> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    let bot_uid = lookup_uid("human-in-loop")?;
    let current_uid = unsafe { libc::geteuid() };
    if current_uid != bot_uid {
        return Err("imessage worker must run as macOS user human-in-loop".into());
    }
    let worker = load_worker_config(&worker_config_path())?;
    let channel = crate::config::AppConfig::load_without_secrets()
        .channels
        .imessage;
    let path = Path::new(WORKER_SOCKET_PATH);
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.file_type().is_socket() || metadata.uid() != current_uid {
            return Err("worker socket path is owned by another identity".into());
        }
        std::fs::remove_file(path).map_err(|_| "cannot remove stale worker socket")?;
    }
    let listener =
        tokio::net::UnixListener::bind(path).map_err(|_| "cannot bind the Bot worker socket")?;
    secure_worker_socket(path, worker.coordinator_gid)?;
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|_| "Bot worker accept failed")?;
        tokio::spawn(handle_connection(stream, worker.clone(), channel.clone()));
    }
}

#[cfg(target_os = "macos")]
fn secure_worker_socket(path: &Path, coordinator_gid: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    std::os::unix::fs::chown(path, None, Some(coordinator_gid))
        .map_err(|_| "cannot assign the Bot worker socket group")?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))
        .map_err(|_| "cannot secure the Bot worker socket".to_string())
}

#[cfg(target_os = "macos")]
fn install(args: &[String]) -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;

    let coordinator = parse_install_args(args)?;
    let bot_uid = lookup_uid("human-in-loop")?;
    if unsafe { libc::geteuid() } != bot_uid {
        return Err("run worker installation while logged in as macOS user human-in-loop".into());
    }
    let executable = std::env::current_exe().map_err(|_| "cannot locate worker executable")?;
    if executable != Path::new(SHARED_BINARY) {
        return Err(format!("worker installation must use {SHARED_BINARY}"));
    }
    let version = std::process::Command::new(SHARED_IMSG)
        .arg("--version")
        .output()
        .map_err(|_| "shared imsg 0.15.1 is unavailable")?;
    if !version.status.success()
        || String::from_utf8_lossy(&version.stdout).trim()
            != crate::channels::imessage::SUPPORTED_IMSG_VERSION
    {
        return Err("shared imsg version is incompatible".into());
    }

    let sender = std::env::var(BOT_SENDER_ENV)
        .map_err(|_| format!("{BOT_SENDER_ENV} must be supplied privately"))?;
    let recipient = std::env::var(RECIPIENT_ENV)
        .map_err(|_| format!("{RECIPIENT_ENV} must be supplied privately"))?;
    let worker = WorkerConfig::new(
        lookup_uid(&coordinator)?,
        lookup_gid(&coordinator)?,
        &sender,
        &recipient,
    )?;

    let mut app = crate::config::AppConfig::load_without_secrets();
    app.channels.imessage.enabled = true;
    app.channels.imessage.recipient = recipient;
    app.channels.imessage.identity_mode = crate::config::IMessageIdentityMode::DistinctPeer;
    app.channels.imessage.chat_id = None;
    app.channels.imessage.chat_guid.clear();
    app.save()
        .map_err(|_| "cannot save Bot channel configuration")?;
    save_worker_config(&worker_config_path(), &worker)
        .map_err(|_| "cannot save Bot worker configuration")?;

    let launch_agents = crate::paths::home().join("Library/LaunchAgents");
    std::fs::create_dir_all(&launch_agents)
        .map_err(|_| "cannot create Bot LaunchAgents directory")?;
    let plist_path = launch_agents.join(format!("{WORKER_LABEL}.plist"));
    let plist = launch_agent_plist(
        SHARED_BINARY,
        &crate::paths::config_dir().display().to_string(),
    );
    crate::integrations::hook_edit::atomic_write(&plist_path, plist.as_bytes())
        .map_err(|_| "cannot write Bot worker LaunchAgent")?;
    std::fs::set_permissions(&plist_path, std::fs::Permissions::from_mode(0o644))
        .map_err(|_| "cannot secure Bot worker LaunchAgent")?;

    let domain = format!("gui/{bot_uid}");
    let service = format!("{domain}/{WORKER_LABEL}");
    let _ = std::process::Command::new("/bin/launchctl")
        .args(["bootout", &service])
        .status();
    let status = std::process::Command::new("/bin/launchctl")
        .args(["bootstrap", &domain, &plist_path.display().to_string()])
        .status()
        .map_err(|_| "cannot start Bot worker LaunchAgent")?;
    if !status.success() {
        return Err("Bot worker LaunchAgent failed to start".into());
    }
    Ok("Bot iMessage worker installed".into())
}

#[cfg(target_os = "macos")]
pub fn dispatch(args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("install") => install(&args[1..]),
        Some("run") if args.len() == 1 => {
            std::env::set_var("PATH", worker_runtime_path());
            crate::cli::cfgio::block_on(serve())?;
            Ok(String::new())
        }
        Some("status") if args.len() == 1 => {
            let config = crate::config::AppConfig::load_without_secrets();
            let state = crate::cli::cfgio::block_on(health(&config.channels.imessage));
            Ok(state.as_str().to_string())
        }
        _ => Err(
            "usage: imessage-worker <install --coordinator-user <short-name>|run|status>".into(),
        ),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn dispatch(_args: &[String]) -> Result<String, String> {
    Err("the iMessage worker is supported only on macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{IMessageChannelConfig, IMessageIdentityMode};

    #[test]
    fn worker_config_requires_distinct_pinned_identities() {
        assert!(WorkerConfig::new(501, 20, "bot@example.com", "person@example.com").is_ok());
        assert!(WorkerConfig::new(501, 20, "", "person@example.com").is_err());
        assert!(WorkerConfig::new(501, 20, "same@example.com", " SAME@example.com ").is_err());
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn worker_socket_is_group_accessible_to_the_coordinator() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("worker.sock");
        let _listener = tokio::net::UnixListener::bind(&path).unwrap();
        let coordinator_gid = unsafe { libc::getegid() };
        secure_worker_socket(&path, coordinator_gid).unwrap();
        let metadata = std::fs::metadata(path).unwrap();
        assert_eq!(metadata.gid(), coordinator_gid);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o660);
    }

    #[cfg(unix)]
    #[test]
    fn worker_config_is_written_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("imessage-worker.json");
        let config = WorkerConfig::new(501, 20, "bot@example.com", "person@example.com").unwrap();
        save_worker_config(&path, &config).unwrap();
        let loaded = load_worker_config(&path).unwrap();
        assert_eq!(loaded.coordinator_uid, 501);
        assert_eq!(loaded.coordinator_gid, 20);
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn launch_agent_contains_only_fixed_runtime_metadata() {
        let plist = launch_agent_plist(
            "/Users/Shared/human-in-loop/bin/human-in-loop",
            "/Users/human-in-loop/.human-in-loop",
        );
        assert!(plist.contains("imessage-worker"));
        assert!(plist.contains("<string>run</string>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("/Users/Shared/human-in-loop/bin"));
        assert!(!plist.contains("bot@example.com"));
        assert!(!plist.contains("person@example.com"));
    }

    #[test]
    fn worker_runtime_path_uses_only_the_shared_install_and_system_bins() {
        assert_eq!(
            worker_runtime_path(),
            "/Users/Shared/human-in-loop/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        );
    }

    #[test]
    fn protocol_accepts_only_bounded_confirmation_and_notification_shapes() {
        let confirm: WorkerRequest = serde_json::from_str(
            r#"{"operation":"confirm","recipient":"person@example.com","requestId":"r1","token":"7F32","text":"Question","choiceIndices":[0,1],"expiresAtMs":2000}"#,
        )
        .unwrap();
        assert!(matches!(confirm, WorkerRequest::Confirm { .. }));
        let notify: WorkerRequest = serde_json::from_str(
            r#"{"operation":"notify","recipient":"person@example.com","text":"Done"}"#,
        )
        .unwrap();
        assert!(matches!(notify, WorkerRequest::Notify { .. }));
        assert!(serde_json::from_str::<WorkerRequest>(
            r#"{"operation":"confirm","recipient":"person@example.com","requestId":"r1","token":"7F32","text":"Question","choiceIndices":[0,1],"expiresAtMs":2000,"file":"/tmp/a"}"#
        )
        .is_err());
    }

    #[tokio::test]
    async fn framing_rejects_oversized_or_unterminated_input() {
        let oversized = vec![b'x'; MAX_FRAME_BYTES + 1];
        let mut reader = tokio::io::BufReader::new(oversized.as_slice());
        assert!(read_frame::<_, WorkerRequest>(&mut reader).await.is_err());

        let mut reader = tokio::io::BufReader::new(
            br#"{"operation":"health","recipient":"person@example.com"}"#.as_slice(),
        );
        assert!(read_frame::<_, WorkerRequest>(&mut reader).await.is_err());
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn unix_peer_credentials_come_from_the_kernel() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("worker.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let expected = unsafe { libc::geteuid() };
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            assert_eq!(peer_uid(&stream).unwrap(), expected);
        });
        let client = tokio::net::UnixStream::connect(path).await.unwrap();
        assert_eq!(peer_uid(&client).unwrap(), expected);
        server.await.unwrap();
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn authenticated_client_round_trips_one_narrow_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("worker.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let uid = unsafe { libc::geteuid() };
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            assert_eq!(peer_uid(&stream).unwrap(), uid);
            let (read, mut write) = stream.into_split();
            let request: WorkerRequest = read_frame(&mut tokio::io::BufReader::new(read))
                .await
                .unwrap();
            assert!(matches!(request, WorkerRequest::Health { .. }));
            write_frame(
                &mut write,
                &WorkerResponse::Health {
                    state: "ready".into(),
                },
            )
            .await
            .unwrap();
        });

        let mut client = WorkerClient::connect_at(&path, uid).await.unwrap();
        client
            .send(&WorkerRequest::Health {
                recipient: "person@example.com".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            client.next().await.unwrap(),
            WorkerResponse::Health {
                state: "ready".into()
            }
        );
        server.await.unwrap();
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn authenticated_client_rejects_the_wrong_server_uid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("worker.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let _ = listener.accept().await.unwrap();
        });
        let uid = unsafe { libc::geteuid() };
        assert!(WorkerClient::connect_at(&path, uid.saturating_add(1))
            .await
            .is_err());
        server.await.unwrap();
    }

    fn channel(recipient: &str) -> IMessageChannelConfig {
        IMessageChannelConfig {
            enabled: true,
            recipient: recipient.into(),
            identity_mode: IMessageIdentityMode::DistinctPeer,
            chat_id: None,
            chat_guid: String::new(),
        }
    }

    #[test]
    fn worker_rejects_unpinned_or_self_addressed_requests_before_transport() {
        let config = WorkerConfig::new(501, 20, "bot@example.com", "person@example.com").unwrap();
        let approved = WorkerRequest::Health {
            recipient: "person@example.com".into(),
        };
        assert!(validate_request(&config, &channel("person@example.com"), &approved).is_ok());

        let unpinned = WorkerRequest::Health {
            recipient: "other@example.com".into(),
        };
        assert_eq!(
            validate_request(&config, &channel("person@example.com"), &unpinned),
            Err(crate::channels::imessage::HealthState::RecipientNotImessage)
        );

        let same = WorkerConfig {
            coordinator_uid: 501,
            coordinator_gid: 20,
            bot_sender: "person@example.com".into(),
        };
        assert_eq!(
            validate_request(&same, &channel("person@example.com"), &approved),
            Err(crate::channels::imessage::HealthState::SelfMessageUnsupported)
        );
    }

    #[test]
    fn worker_rejects_malformed_confirm_without_sending() {
        let config = WorkerConfig::new(501, 20, "bot@example.com", "person@example.com").unwrap();
        let mut request = WorkerRequest::Confirm {
            recipient: "person@example.com".into(),
            request_id: "request-1".into(),
            token: "7F32".into(),
            text: "Question".into(),
            choice_indices: vec![0, 1],
            expires_at_ms: 2_000,
        };
        assert!(validate_request(&config, &channel("person@example.com"), &request).is_ok());
        if let WorkerRequest::Confirm { choice_indices, .. } = &mut request {
            *choice_indices = vec![0];
        }
        assert_eq!(
            validate_request(&config, &channel("person@example.com"), &request),
            Err(crate::channels::imessage::HealthState::MessagesUnavailable)
        );
    }

    #[test]
    fn install_cli_accepts_only_the_coordinator_username() {
        assert_eq!(
            parse_install_args(&["--coordinator-user".into(), "wenv".into()]).unwrap(),
            "wenv"
        );
        assert!(parse_install_args(&["--recipient".into(), "private@example.com".into()]).is_err());
        assert!(parse_install_args(&["--coordinator-user".into(), "../wenv".into()]).is_err());
        assert_eq!(parse_uid(b"501\n").unwrap(), 501);
        assert!(parse_uid(b"0\n").is_err());
        assert!(parse_uid(b"501 extra\n").is_err());
    }

    #[test]
    fn production_distinct_peer_routes_only_through_the_worker() {
        assert!(required_for(IMessageIdentityMode::DistinctPeer));
        assert!(!required_for(IMessageIdentityMode::SameAccount));
        assert_eq!(
            admit_cross_user_image(true, true),
            Err(crate::channels::imessage::UnsupportedReason::RequiredImageUnsupported)
        );
        assert_eq!(admit_cross_user_image(true, false), Ok(()));
        assert_eq!(admit_cross_user_image(false, true), Ok(()));
    }
}
