//! Minimal MCP boundary for the canonical structured-confirmation runtime.

use crate::confirm::ActionRole;
use crate::ipc::{ConfirmTask, ConfirmTaskOrigin};
use crate::models::{
    ConfirmChoice, ConfirmDetail, ConfirmField, ConfirmFieldKind, ConfirmPresentation,
    ConfirmResult, ConfirmSpec, HumanNotification, NotificationField, NotificationResult,
    NotificationStatus,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, Json, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
#[cfg(test)]
use std::{future::Future, pin::Pin, sync::Arc};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
type TestSubmitter = Arc<
    dyn Fn(
            ConfirmTask,
            CancellationToken,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<ConfirmResult, crate::client::ConfirmClientError>>
                    + Send,
            >,
        > + Send
        + Sync,
>;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AskHumanChoice {
    /// Stable semantic identifier returned unchanged when this choice wins.
    pub id: String,
    /// Compact human-visible label.
    pub label: String,
}

#[cfg(test)]
type TestNotifier = Arc<
    dyn Fn(
            HumanNotification,
            CancellationToken,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<NotificationResult, crate::client::NotificationClientError>,
                    > + Send,
            >,
        > + Send
        + Sync,
>;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NotifyHumanParams {
    /// Local path inside the associated GitHub repository. Omit only for non-repository work.
    #[serde(default)]
    pub repository_path: Option<String>,
    /// Short identity of the calling agent/source.
    pub source_agent: String,
    /// Established task verdict, independent of notification delivery.
    pub status: NotificationStatus,
    /// Compact summary (at most 160 Unicode characters).
    pub summary: String,
    /// At most two compact label/value fields; each rendered line is at most 80 characters.
    #[serde(default)]
    #[schemars(length(max = 2))]
    pub context: Vec<NotificationField>,
    /// Optional durable task identity (at most 128 characters).
    #[serde(default)]
    pub task_id: Option<String>,
    /// Safe evidence URL, path or branch/commit locator (at most 240 characters); never opened.
    #[serde(default)]
    pub locator: Option<String>,
    /// Optional notification identity (at most 128 characters). Generated when omitted.
    #[serde(default)]
    pub notification_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AskHumanParams {
    /// Local path inside the associated GitHub repository. Omit only for a genuinely
    /// non-repository decision. The displayed repository name is resolved from Git origin.
    #[serde(default)]
    pub repository_path: Option<String>,
    /// Short identity of the calling agent or source, for example `Codex`.
    pub source_agent: String,
    /// Compact decision question shown to the human.
    pub question: String,
    /// Optional multiline evidence needed to make the decision.
    #[serde(default)]
    pub detail: Option<String>,
    /// Two to six choices with stable semantic identifiers.
    #[schemars(length(min = 2, max = 6))]
    pub choices: Vec<AskHumanChoice>,
    /// Optional compact decision context.
    #[serde(default)]
    pub context: Option<String>,
    /// Optional stable choice id to render as recommended.
    #[serde(default)]
    pub recommended_choice: Option<String>,
    /// Optional caller-provided request id. A local UUID is generated when omitted.
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AskHumanResult {
    pub request_id: String,
    pub selected_choice_id: String,
    pub source_channel_id: String,
}

#[derive(Clone)]
pub struct AskHumanServer {
    tool_router: ToolRouter<Self>,
    shutdown: CancellationToken,
    #[cfg(test)]
    submitter: Option<TestSubmitter>,
    #[cfg(test)]
    notifier: Option<TestNotifier>,
}

#[tool_router(router = tool_router)]
impl AskHumanServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            shutdown: CancellationToken::new(),
            #[cfg(test)]
            submitter: None,
            #[cfg(test)]
            notifier: None,
        }
    }

    #[cfg(test)]
    fn with_submitter(submitter: TestSubmitter) -> Self {
        Self {
            tool_router: Self::tool_router(),
            shutdown: CancellationToken::new(),
            submitter: Some(submitter),
            notifier: None,
        }
    }

    pub(super) fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// Ask the human to select one stable choice through the configured Human in Loop channels.
    #[tool(
        name = "ask_human",
        description = "Ask the human one bounded multiple-choice question and block until exactly one configured Human in Loop channel returns a canonical choice. Repository-associated requests must provide `repository_path`; the server resolves the GitHub repository name locally. Recipient and channel configuration are never accepted by this tool.",
        annotations(destructive_hint = false, open_world_hint = true)
    )]
    async fn ask_human(
        &self,
        Parameters(params): Parameters<AskHumanParams>,
        cancel: CancellationToken,
    ) -> Result<Json<AskHumanResult>, McpError> {
        let task = build_confirm_task(params)
            .map_err(|message| McpError::invalid_params(message, None))?;
        let request_id = task.request_id.clone().expect("mapper assigns request id");
        let submit_cancel = CancellationToken::new();
        #[cfg(test)]
        let submission = async {
            match &self.submitter {
                Some(submitter) => submitter(task, submit_cancel.clone()).await,
                None => crate::client::run_confirm_async(task, submit_cancel.clone()).await,
            }
        };
        #[cfg(not(test))]
        let submission = crate::client::run_confirm_async(task, submit_cancel.clone());
        tokio::pin!(submission);
        let result = tokio::select! {
            result = &mut submission => result,
            _ = cancel.cancelled() => {
                submit_cancel.cancel();
                submission.await
            }
            _ = self.shutdown.cancelled() => {
                submit_cancel.cancel();
                submission.await
            }
        };
        let result = result.map_err(|error| McpError::internal_error(error.to_string(), None))?;
        Ok(Json(map_result(&request_id, result)))
    }

    #[tool(
        name = "notify_human",
        description = "Send a compact informational task notification through configured Human in Loop channels. Returns after bounded dispatch, creates no pending decision and never waits for a reply. Repository-associated notifications require repository_path. Delivery status is SENT, PARTIAL or FAILED and does not change the supplied task verdict.",
        annotations(destructive_hint = false, open_world_hint = true)
    )]
    async fn notify_human(
        &self,
        Parameters(params): Parameters<NotifyHumanParams>,
        cancel: CancellationToken,
    ) -> Result<Json<NotificationResult>, McpError> {
        let notification = build_notification(params)
            .map_err(|message| McpError::invalid_params(message, None))?;
        let dispatch_cancel = CancellationToken::new();
        #[cfg(test)]
        let dispatch = async {
            match &self.notifier {
                Some(notifier) => notifier(notification, dispatch_cancel.clone()).await,
                None => {
                    crate::client::run_notification_async(notification, dispatch_cancel.clone())
                        .await
                }
            }
        };
        #[cfg(not(test))]
        let dispatch = crate::client::run_notification_async(notification, dispatch_cancel.clone());
        tokio::pin!(dispatch);
        let result = tokio::select! {
            result = &mut dispatch => result,
            _ = cancel.cancelled() => {
                dispatch_cancel.cancel();
                dispatch.await
            }
            _ = self.shutdown.cancelled() => {
                dispatch_cancel.cancel();
                dispatch.await
            }
        };
        result
            .map(Json)
            .map_err(|error| McpError::internal_error(error.to_string(), None))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AskHumanServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Human in Loop exposes ask_human for one blocking correlated choice and notify_human for bounded informational dispatch without a reply. Neither accepts transport credentials or recipient identity.",
            );
        let mut implementation = Implementation::from_build_env();
        implementation.name = "human-in-loop".to_string();
        implementation.version = env!("CARGO_PKG_VERSION").to_string();
        info.server_info = implementation;
        info
    }
}

fn build_confirm_task(params: AskHumanParams) -> Result<ConfirmTask, String> {
    let source = required_compact(&params.source_agent, "source_agent")?;
    let question = required_compact(&params.question, "question")?;
    if !(2..=6).contains(&params.choices.len()) {
        return Err("ask_human requires 2 to 6 choices".to_string());
    }

    let mut ids = std::collections::HashSet::new();
    let choices: Vec<ConfirmChoice> = params
        .choices
        .into_iter()
        .map(|choice| {
            let id = required_identifier(&choice.id, "choice id")?;
            let label = required_compact(&choice.label, "choice label")?;
            if !ids.insert(id.clone()) {
                return Err(format!("duplicate choice id: {id}"));
            }
            Ok(ConfirmChoice {
                role: if params.recommended_choice.as_deref() == Some(id.as_str()) {
                    ActionRole::Primary
                } else {
                    ActionRole::Default
                },
                id,
                label,
                description: String::new(),
                variant: None,
            })
        })
        .collect::<Result<_, String>>()?;
    if params
        .recommended_choice
        .as_deref()
        .is_some_and(|id| !ids.contains(id))
    {
        return Err("recommended_choice must reference a choice id".to_string());
    }

    let project = resolve_project(params.repository_path.as_deref())?;
    let context = params
        .context
        .map(|value| required_compact(&value, "context"))
        .transpose()?
        .map(|value| {
            vec![ConfirmField {
                id: "context".into(),
                label: "Context".into(),
                value,
                kind: ConfirmFieldKind::Text,
            }]
        })
        .unwrap_or_default();
    let request_id = match params.request_id {
        Some(value) => required_identifier(&value, "request_id")?,
        None => uuid::Uuid::new_v4().to_string(),
    };
    let dismiss_action_id = choices.last().expect("choice bound checked").id.clone();
    let default_action_id = params.recommended_choice;
    let task = ConfirmTask {
        origin: ConfirmTaskOrigin::Mcp,
        request_id: Some(request_id.clone()),
        spec: ConfirmSpec {
            title: question.clone(),
            context,
            detail: ConfirmDetail {
                summary: question,
                body_md: params
                    .detail
                    .map(|value| normalize_detail(&value))
                    .unwrap_or_default(),
            },
            choices,
            presentation: ConfirmPresentation::SingleSelectSubmit {
                input: None,
                submit_label: "Submit decision".into(),
                default_action_id,
            },
            dismiss_action_id,
            decision_image: None,
        },
        popup_edit: None,
        source: source.clone(),
        lang: "en".into(),
        project,
        agent_kind: source,
        agent_session_id: request_id,
        caller_pid: std::process::id(),
        memory: None,
    };
    task.spec.validate()?;
    Ok(task)
}

fn resolve_project(repository_path: Option<&str>) -> Result<String, String> {
    Ok(match repository_path {
        Some(raw) if raw.trim().is_empty() => {
            return Err("repository_path must not be empty".to_string())
        }
        Some(raw) => {
            let path = Path::new(raw)
                .canonicalize()
                .map_err(|_| "repository_path must identify an existing directory".to_string())?;
            if !path.is_dir() {
                return Err("repository_path must identify an existing directory".to_string());
            }
            let project = crate::project::detect_from(&path);
            match crate::project::repository_identity(&project) {
                crate::project::RepositoryIdentity::Github(_) => project,
                crate::project::RepositoryIdentity::NonRepository => {
                    return Err("repository_path must identify a GitHub repository".to_string())
                }
                crate::project::RepositoryIdentity::Unavailable => {
                    return Err("canonical GitHub repository identity is unavailable".to_string())
                }
            }
        }
        None => String::new(),
    })
}

fn build_notification(params: NotifyHumanParams) -> Result<HumanNotification, String> {
    use crate::channels::notify;
    let notification = HumanNotification {
        notification_id: params
            .notification_id
            .map(|id| notify::identifier(&id, "notification_id"))
            .transpose()?
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        project: resolve_project(params.repository_path.as_deref())?,
        source_agent: params.source_agent,
        status: params.status,
        summary: params.summary,
        context: params.context,
        task_id: params.task_id,
        locator: params.locator,
    };
    notify::render(&notification)?;
    Ok(notification)
}

fn required_compact(value: &str, field: &str) -> Result<String, String> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(value)
    }
}

fn normalize_detail(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}

fn required_identifier(value: &str, field: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(value.to_string())
    }
}

fn map_result(request_id: &str, result: ConfirmResult) -> AskHumanResult {
    AskHumanResult {
        request_id: request_id.to_string(),
        selected_choice_id: result.action_id,
        source_channel_id: result.source_channel_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ConfirmResult;
    use rmcp::ServiceExt;
    use serde_json::json;
    use std::fs;
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tempfile::tempdir;
    use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
    use tokio::sync::Notify;

    async fn send_json(writer: &mut (impl AsyncWrite + Unpin), value: serde_json::Value) {
        writer
            .write_all(value.to_string().as_bytes())
            .await
            .unwrap();
        writer.write_all(b"\n").await.unwrap();
        writer.flush().await.unwrap();
    }

    async fn read_response(reader: &mut (impl AsyncBufRead + Unpin), id: i64) -> serde_json::Value {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).await.unwrap() > 0);
                let value: serde_json::Value = serde_json::from_str(&line).unwrap();
                if value.get("id") == Some(&json!(id)) {
                    break value;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("MCP response timeout for protocol id {id}"))
    }

    async fn initialize(
        writer: &mut (impl AsyncWrite + Unpin),
        reader: &mut (impl AsyncBufRead + Unpin),
    ) {
        send_json(
            writer,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "human-in-loop-test", "version": "0" }
                }
            }),
        )
        .await;
        let response = read_response(reader, 1).await;
        assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(response["result"]["serverInfo"]["name"], "human-in-loop");
        assert!(response["result"]["capabilities"].get("tools").is_some());
        send_json(
            writer,
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        )
        .await;
    }

    fn valid_arguments() -> serde_json::Value {
        json!({
            "source_agent": "Codex",
            "question": "Did it arrive?",
            "choices": [
                { "id": "received", "label": "Received" },
                { "id": "failed", "label": "Failed" }
            ],
            "request_id": "request-1"
        })
    }

    fn choice(id: &str, label: &str) -> AskHumanChoice {
        AskHumanChoice {
            id: id.into(),
            label: label.into(),
        }
    }

    fn params(repository_path: Option<String>) -> AskHumanParams {
        AskHumanParams {
            repository_path,
            source_agent: "Codex".into(),
            question: "Did it arrive?".into(),
            detail: None,
            choices: vec![choice("received", "Received"), choice("failed", "Failed")],
            context: Some("MCP release candidate".into()),
            recommended_choice: Some("received".into()),
            request_id: Some("request-1".into()),
        }
    }

    async fn protocol_session(
        submitter: TestSubmitter,
    ) -> (
        BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
        tokio::io::WriteHalf<tokio::io::DuplexStream>,
        tokio::task::JoinHandle<()>,
    ) {
        protocol_with_server(AskHumanServer::with_submitter(submitter)).await
    }

    async fn protocol_with_server(
        server: AskHumanServer,
    ) -> (
        BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
        tokio::io::WriteHalf<tokio::io::DuplexStream>,
        tokio::task::JoinHandle<()>,
    ) {
        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let server_task = tokio::spawn(async move {
            let shutdown = server.shutdown_token();
            let (read, write) = tokio::io::split(server_transport);
            server
                .serve((super::super::CancelOnEof::new(read, shutdown), write))
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let (read, mut write) = tokio::io::split(client_transport);
        let mut reader = BufReader::new(read);
        initialize(&mut write, &mut reader).await;
        (reader, write, server_task)
    }

    fn call(id: i64, arguments: serde_json::Value) -> serde_json::Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": "ask_human", "arguments": arguments } })
    }

    fn notification_arguments() -> serde_json::Value {
        json!({
            "source_agent": "Codex", "status": "PASS", "summary": "Verification complete.",
            "task_id": "TASK-1", "locator": "reports/codex/result.md", "notification_id": "notice-1",
            "context": [{"label": "Scope", "value": "Local"}]
        })
    }

    fn notification_call(id: i64, arguments: serde_json::Value) -> serde_json::Value {
        json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "notify_human", "arguments": arguments}})
    }

    #[tokio::test]
    async fn notification_protocol_is_closed_validates_fields_and_never_submits_a_decision() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut server = AskHumanServer::with_submitter(Arc::new(|_, _| {
            panic!("notification created a confirmation")
        }));
        server.notifier = Some({
            let calls = calls.clone();
            Arc::new(move |notice, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(notice.task_id.as_deref(), Some("TASK-1"));
                assert_eq!(notice.locator.as_deref(), Some("reports/codex/result.md"));
                assert_eq!(notice.context[0].value, "Local");
                Box::pin(async move {
                    Ok(crate::channels::notify::result(
                        notice.notification_id,
                        &[(crate::models::NotificationChannel::Imessage, true)],
                    ))
                })
            })
        });
        let (mut reader, mut write, server_task) = protocol_with_server(server).await;
        send_json(
            &mut write,
            json!({"jsonrpc":"2.0", "id":2, "method":"tools/list"}),
        )
        .await;
        let response = read_response(&mut reader, 2).await;
        let tool = &response["result"]["tools"][1];
        assert_eq!(tool["name"], "notify_human");
        let schema = &tool["inputSchema"];
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["$defs"]["NotificationField"]["additionalProperties"],
            false
        );
        let keys = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            [
                "context",
                "locator",
                "notification_id",
                "repository_path",
                "source_agent",
                "status",
                "summary",
                "task_id"
            ]
        );
        assert_eq!(tool["outputSchema"]["additionalProperties"], false);

        let mut invalid = vec![json!({}), json!("invalid")];
        for field in ["source_agent", "status", "summary"] {
            let mut value = notification_arguments();
            value.as_object_mut().unwrap().remove(field);
            invalid.push(value);
        }
        for field in [
            "source_agent",
            "summary",
            "notification_id",
            "task_id",
            "locator",
            "repository_path",
        ] {
            let mut value = notification_arguments();
            value[field] = json!("");
            invalid.push(value);
        }
        for (field, value) in [
            ("status", json!("DONE")),
            ("choices", json!([{"id":"acknowledge", "label":"OK"}])),
            ("recipient", json!("private-sentinel")),
            ("shell", json!("private-sentinel")),
            ("summary", json!("界".repeat(161))),
            ("notification_id", json!("x".repeat(129))),
            (
                "context",
                json!([{"label":"Scope", "value":"Local", "recipient":"private-sentinel"}]),
            ),
        ] {
            let mut arguments = notification_arguments();
            arguments[field] = value;
            invalid.push(arguments);
        }
        for (index, arguments) in invalid.into_iter().enumerate() {
            let id = 10 + index as i64;
            send_json(&mut write, notification_call(id, arguments)).await;
            let response = read_response(&mut reader, id).await;
            assert!(
                response.get("error").is_some() || response["result"]["isError"] == true,
                "{response}"
            );
            assert!(!response.to_string().contains("private-sentinel"));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        for (index, status) in ["PASS", "PASS_WITH_LIMITATIONS", "BLOCKED", "FAIL"]
            .iter()
            .enumerate()
        {
            let id = 100 + index as i64;
            let mut arguments = notification_arguments();
            arguments["status"] = json!(status);
            send_json(&mut write, notification_call(id, arguments)).await;
            let response = read_response(&mut reader, id).await;
            assert_eq!(
                response["result"]["structuredContent"],
                json!({"notification_id":"notice-1", "delivery_status":"SENT", "channel_ids":["imessage"]})
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        drop(write);
        drop(reader);
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn notification_cancellation_and_eof_cancel_inflight_dispatch() {
        for eof in [false, true] {
            let started = Arc::new(Notify::new());
            let cleaned = Arc::new(Notify::new());
            let mut server = AskHumanServer::new();
            server.notifier = Some({
                let started = started.clone();
                let cleaned = cleaned.clone();
                Arc::new(move |_, cancel| {
                    let started = started.clone();
                    let cleaned = cleaned.clone();
                    Box::pin(async move {
                        started.notify_one();
                        cancel.cancelled().await;
                        cleaned.notify_one();
                        Err(crate::client::NotificationClientError::Cancelled)
                    })
                })
            });
            let (reader, write, server_task) = protocol_with_server(server).await;
            let mut reader = Some(reader);
            let mut write = Some(write);
            send_json(
                write.as_mut().unwrap(),
                notification_call(2, notification_arguments()),
            )
            .await;
            started.notified().await;
            if eof {
                drop(write.take());
                drop(reader.take());
            } else {
                send_json(write.as_mut().unwrap(), json!({"jsonrpc":"2.0", "method":"notifications/cancelled", "params":{"requestId":2}})).await;
            }
            tokio::time::timeout(std::time::Duration::from_secs(2), cleaned.notified())
                .await
                .expect("notification dispatch cleanup");
            drop(write);
            drop(reader);
            server_task.await.unwrap();
        }
    }

    #[tokio::test]
    async fn protocol_schema_rejects_invalid_calls_and_recovers_without_submission() {
        let calls = Arc::new(AtomicUsize::new(0));
        let submitter: TestSubmitter = {
            let calls = calls.clone();
            Arc::new(move |task, _cancel| {
                calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(task.spec.presentation.default_action_id(), Some("received"));
                assert_eq!(task.spec.choices[0].role, ActionRole::Primary);
                Box::pin(async {
                    Ok(ConfirmResult {
                        action_id: "received".into(),
                        comment: None,
                        source_channel_id: "imessage".into(),
                    })
                })
            })
        };
        let (mut reader, mut write, server_task) = protocol_session(submitter).await;
        send_json(
            &mut write,
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        )
        .await;
        let response = read_response(&mut reader, 2).await;
        let tools = response["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["name"], "ask_human");
        assert_eq!(tools[1]["name"], "notify_human");
        let schema = &tools[0]["inputSchema"];
        let mut properties = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        properties.sort_unstable();
        assert_eq!(
            properties,
            [
                "choices",
                "context",
                "detail",
                "question",
                "recommended_choice",
                "repository_path",
                "request_id",
                "source_agent"
            ]
        );
        assert_eq!(schema["additionalProperties"], false);
        let mut required = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        required.sort_unstable();
        assert_eq!(required, ["choices", "question", "source_agent"]);
        assert_eq!(schema["properties"]["choices"]["minItems"], 2);
        assert_eq!(schema["properties"]["choices"]["maxItems"], 6);
        let mut result_keys = tools[0]["outputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        result_keys.sort_unstable();
        assert_eq!(
            result_keys,
            ["request_id", "selected_choice_id", "source_channel_id"]
        );

        let mut invalid = vec![json!({}), json!("not an object")];
        for missing in ["source_agent", "question", "choices"] {
            let mut arguments = valid_arguments();
            arguments.as_object_mut().unwrap().remove(missing);
            invalid.push(arguments);
        }
        for count in [0, 1, 7] {
            let mut arguments = valid_arguments();
            arguments["choices"] = json!((0..count)
                .map(|i| json!({"id": format!("choice-{i}"), "label": "Choice"}))
                .collect::<Vec<_>>());
            invalid.push(arguments);
        }
        for forbidden in [
            "recipient",
            "phone",
            "email",
            "chat_id",
            "chat_guid",
            "imsg",
            "sms",
            "shell",
            "read_file",
            "credential",
            "project_name",
        ] {
            let mut arguments = valid_arguments();
            arguments[forbidden] = json!("forbidden-runtime-value");
            invalid.push(arguments);
        }
        let mut duplicate = valid_arguments();
        duplicate["choices"][1]["id"] = json!("received");
        invalid.push(duplicate);
        let mut blank_id = valid_arguments();
        blank_id["choices"][0]["id"] = json!(" \t");
        invalid.push(blank_id);
        let mut unknown_choice_field = valid_arguments();
        unknown_choice_field["choices"][0]["command"] = json!("forbidden-runtime-value");
        invalid.push(unknown_choice_field);
        let mut recommendation = valid_arguments();
        recommendation["recommended_choice"] = json!("missing");
        invalid.push(recommendation);
        for (i, arguments) in invalid.into_iter().enumerate() {
            let id = i as i64 + 10;
            send_json(&mut write, call(id, arguments)).await;
            let response = read_response(&mut reader, id).await;
            assert!(
                response.get("error").is_some() || response["result"]["isError"] == true,
                "{response}"
            );
            assert!(response.pointer("/result/structuredContent").is_none());
            assert!(!response.to_string().contains("forbidden-runtime-value"));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        send_json(&mut write, json!({ "jsonrpc": "2.0", "id": 50, "method": "tools/call", "params": { "name": "shell", "arguments": {} } })).await;
        assert!(read_response(&mut reader, 50).await.get("error").is_some());
        send_json(
            &mut write,
            json!({ "jsonrpc": "2.0", "id": 51, "method": "unknown/method" }),
        )
        .await;
        assert!(read_response(&mut reader, 51).await.get("error").is_some());
        write
            .write_all(b"{malformed json\n{\"jsonrpc\":\"2.0\",\"id\":52,\"method\":7}\n")
            .await
            .unwrap();
        write.flush().await.unwrap();
        for count in [2, 6] {
            let mut arguments = valid_arguments();
            arguments["recommended_choice"] = json!("received");
            for i in 2..count {
                arguments["choices"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id": format!("choice-{i}"), "label": "Choice"}));
            }
            send_json(&mut write, call(60 + count, arguments)).await;
            assert_eq!(
                read_response(&mut reader, 60 + count).await["result"]["structuredContent"]
                    ["selected_choice_id"],
                "received"
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        drop(write);
        drop(reader);
        server_task.await.unwrap();
    }

    #[test]
    fn builds_canonical_confirmation_with_stable_choices_and_recommendation() {
        let dir = tempdir().unwrap();
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "git@github.com:cigit-zgy/human-in-loop.git",
            ])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success());
        let task = build_confirm_task(params(Some(dir.path().display().to_string()))).unwrap();
        assert_eq!(task.request_id.as_deref(), Some("request-1"));
        assert_eq!(task.source, "Codex");
        assert_eq!(task.spec.detail.summary, "Did it arrive?");
        assert_eq!(task.spec.choices[0].id, "received");
        assert_eq!(task.spec.choices[1].id, "failed");
        assert_eq!(task.spec.presentation.default_action_id(), Some("received"));
        assert_eq!(
            crate::project::repository_identity(&task.project),
            crate::project::RepositoryIdentity::Github("human-in-loop".into())
        );
        let rendered = crate::channels::imessage::render_confirmation(
            &task
                .spec
                .clone()
                .into_request("request-1".into(), 1, 2)
                .unwrap(),
            "48273",
            &task.source,
            Some("human-in-loop"),
        )
        .unwrap();
        assert_eq!(rendered.text.lines().nth(1), Some("Codex · human-in-loop"));
        assert!(rendered.text.contains("1  Received [recommended]"));
    }

    #[test]
    fn multiline_detail_maps_to_the_canonical_body_without_flattening() {
        let mut arguments = valid_arguments();
        arguments["detail"] = json!("  line A\r\nline B\r\rline C  ");
        let params: AskHumanParams = serde_json::from_value(arguments).unwrap();
        let task = build_confirm_task(params).unwrap();

        assert_eq!(task.spec.detail.summary, "Did it arrive?");
        assert_eq!(task.spec.detail.body_md, "line A\nline B\n\nline C");
    }

    #[test]
    fn synthetic_wme_sized_multiline_detail_survives_mapping_and_imessage_rendering() {
        const DETAIL: &str = concat!(
            "进度\n当前进入合成审阅阶段，所有材料均为本测试专门编写，不对应任何真实论文、作者、数据集或生产结论。请根据下面的结构化证据判断卡片是否完整、清楚且适合在手机上阅读。\n\n",
            "中文题目\n面向虚构循环水系统的多阶段模型审阅示例。本题目只用于验证长正文、中文字符、段落边界和五个稳定选项的传递，不承载科学主张。\n\n",
            "研究对象\n对象是一个完全虚构的实验性水处理流程，包含进水区、反应区和回流区。设定的变量、采样频率与运行条件都是合成描述，目的是提供接近真实审阅卡的阅读密度，同时避免引入受许可限制的来源内容。\n\n",
            "模型或计算方法\n使用一个假想的分区质量平衡框架，并以确定性的参数表和规则化计算步骤生成比较结果。这里不调用外部模型，不引用真实文献，也不声称数值具备工程意义；只检查正文能否跨行、跨段保持原样。\n\n",
            "模型承担的作用\n模型在这个示例中仅负责组织观察量、候选解释和判定依据，使审阅者能够区分数据描述、计算假设与最终选择。任何选项都不会触发删除、发布、付款或真实环境变更。\n\n",
            "与模型直接相关的关键结果\n合成结果显示三个阶段可被分别描述，段落标题仍位于独立行，中文标点与数字不会被压成一个长行；五个候选选择保持稳定语义编号，返回值应是稳定标识而不是手机上的位置数字。\n\n",
            "判据提醒\n请只判断这条合成卡片的正文是否完整显示、换行是否保留、问题与选项是否清楚。不要据此推断任何真实科研结论；若正文缺失、被截断或段落合并，应选择需要修订或阻止继续。"
        );
        let count = DETAIL.chars().count();
        assert!((600..=800).contains(&count), "fixture has {count} chars");

        let mut arguments = valid_arguments();
        arguments["detail"] = json!(DETAIL);
        arguments["context"] = json!("合成审阅回归");
        arguments["choices"] = json!([
            {"id":"approve","label":"内容完整"},
            {"id":"minor_revision","label":"小幅修订"},
            {"id":"major_revision","label":"大幅修订"},
            {"id":"insufficient_evidence","label":"证据不足"},
            {"id":"stop","label":"停止继续"}
        ]);
        let params: AskHumanParams = serde_json::from_value(arguments).unwrap();
        let task = build_confirm_task(params).unwrap();
        assert_eq!(task.spec.detail.body_md, DETAIL);

        let request = task
            .spec
            .into_request("synthetic-wme".into(), 1, 2)
            .unwrap();
        let rendered =
            crate::channels::imessage::render_confirmation(&request, "48273", &task.source, None)
                .unwrap();
        assert!(rendered.text.contains(DETAIL));
        assert_eq!(rendered.choice_indices, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn validation_enforces_choice_bounds_ids_and_canonical_repository() {
        let mut one = params(None);
        one.choices.truncate(1);
        assert!(build_confirm_task(one).is_err());

        let mut seven = params(None);
        seven.choices = (0..7)
            .map(|index| choice(&format!("id-{index}"), "Choice"))
            .collect();
        assert!(build_confirm_task(seven).is_err());

        let mut duplicate = params(None);
        duplicate.choices[1].id = duplicate.choices[0].id.clone();
        assert!(build_confirm_task(duplicate).is_err());

        let mut unknown_recommendation = params(None);
        unknown_recommendation.recommended_choice = Some("missing".into());
        assert!(build_confirm_task(unknown_recommendation).is_err());

        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        assert!(build_confirm_task(params(Some(dir.path().display().to_string()))).is_err());
    }

    #[test]
    fn opaque_ids_are_preserved_and_unknown_transport_fields_are_rejected() {
        let mut input = params(None);
        input.choices[0].id = "  stable\tchoice  ".into();
        input.recommended_choice = Some(input.choices[0].id.clone());
        input.request_id = Some("  caller\trequest  ".into());
        let task = build_confirm_task(input).unwrap();
        assert_eq!(task.spec.choices[0].id, "  stable\tchoice  ");
        assert_eq!(
            task.spec.presentation.default_action_id(),
            Some("  stable\tchoice  ")
        );
        assert_eq!(task.request_id.as_deref(), Some("  caller\trequest  "));

        let mut arguments = valid_arguments();
        arguments["recipient"] = json!("not-runtime-data");
        assert!(serde_json::from_value::<AskHumanParams>(arguments).is_err());
        let mut arguments = valid_arguments();
        arguments["choices"][0]["command"] = json!("not-a-capability");
        assert!(serde_json::from_value::<AskHumanParams>(arguments).is_err());
    }

    #[test]
    fn repository_input_requires_existing_directory_and_canonical_github_origin() {
        let dir = tempdir().unwrap();
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/cigit-zgy/human-in-loop.git"
            ])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success());
        let path = dir.path().join("deleted-directory");
        assert!(build_confirm_task(params(Some(path.display().to_string()))).is_err());
        fs::write(&path, "file, not project directory").unwrap();
        assert!(build_confirm_task(params(Some(path.display().to_string()))).is_err());
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        let task = build_confirm_task(params(Some(path.display().to_string()))).unwrap();
        assert_eq!(
            crate::project::repository_identity(&task.project),
            crate::project::RepositoryIdentity::Github("human-in-loop".into())
        );
        assert!(Command::new("git")
            .args([
                "remote",
                "set-url",
                "origin",
                "https://example.invalid/owner/repository.git"
            ])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success());
        assert!(build_confirm_task(params(Some(path.display().to_string()))).is_err());
        // The task-local temp directory itself is inside this repository, so use the filesystem
        // root for the read-only non-repository case rather than pretending nested scratch is one.
        let filesystem_root = dir.path().ancestors().last().unwrap();
        assert!(build_confirm_task(params(Some(filesystem_root.display().to_string()))).is_err());
        #[cfg(unix)]
        {
            let link = dir.path().join("outside-repository");
            std::os::unix::fs::symlink(filesystem_root, &link).unwrap();
            assert!(build_confirm_task(params(Some(link.display().to_string()))).is_err());
        }
        assert!(build_confirm_task(params(Some("\0".into()))).is_err());
    }

    #[test]
    fn unicode_and_long_values_preserve_semantics_and_empty_fields_fail() {
        let mut input = params(None);
        input.question = "是否\n\t收到确认？".into();
        input.context = Some("当前  决策上下文 🧪".into());
        input.choices[0] = choice("已收到", "已收到 ✓");
        input.recommended_choice = Some("已收到".into());
        input.request_id = None;
        let task = build_confirm_task(input).unwrap();
        assert_eq!(task.spec.title, "是否 收到确认？");
        assert_eq!(task.spec.context[0].value, "当前 决策上下文 🧪");
        assert_eq!(task.spec.choices[0].id, "已收到");
        assert_eq!(task.spec.choices[0].label, "已收到 ✓");
        assert!(uuid::Uuid::parse_str(task.request_id.as_deref().unwrap()).is_ok());

        let mut input = params(None);
        input.question = "valid ".repeat(200);
        input.context = Some("context ".repeat(200));
        input.choices[0].label = "label ".repeat(200);
        let task = build_confirm_task(input).unwrap();
        let request = task.spec.into_request("long-input".into(), 1, 2).unwrap();
        assert!(crate::channels::imessage::render_confirmation(
            &request,
            "48273",
            &task.source,
            None
        )
        .is_err());
        for field in [
            "source_agent",
            "question",
            "context",
            "request_id",
            "id",
            "label",
        ] {
            let mut input = params(None);
            match field {
                "source_agent" => input.source_agent = " \t\n".into(),
                "question" => input.question = " \t\n".into(),
                "context" => input.context = Some(" \t\n".into()),
                "request_id" => input.request_id = Some(" \t\n".into()),
                "id" => input.choices[0].id = " \t\n".into(),
                "label" => input.choices[0].label = " \t\n".into(),
                _ => unreachable!(),
            }
            assert!(build_confirm_task(input).is_err(), "{field}");
        }
    }

    #[test]
    fn maps_only_transport_independent_result_fields() {
        let value = map_result(
            "request-1",
            ConfirmResult {
                action_id: "received".into(),
                comment: Some("must not escape".into()),
                source_channel_id: "imessage".into(),
            },
        );
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            json!({
                "request_id": "request-1",
                "selected_choice_id": "received",
                "source_channel_id": "imessage"
            })
        );
    }

    #[test]
    fn discovery_exposes_only_ask_and_notify_without_transport_capabilities() {
        let tools = AskHumanServer::new().tool_router.list_all();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "ask_human");
        assert_eq!(tools[1].name, "notify_human");
        let schema = serde_json::to_value(&tools[0].input_schema).unwrap();
        let properties = schema["properties"].as_object().unwrap();
        for forbidden in [
            "recipient",
            "phone",
            "chat_id",
            "guid",
            "credential",
            "shell",
            "file_read",
            "imsg",
            "sms",
            "mms",
            "rcs",
        ] {
            assert!(!properties.contains_key(forbidden), "{forbidden}");
        }
        assert!(tools[0].output_schema.is_some());
    }

    async fn cancellation_case(shutdown: bool) {
        let started = Arc::new(Notify::new());
        let cleaned = Arc::new(AtomicBool::new(false));
        let submitter: TestSubmitter = {
            let started = started.clone();
            let cleaned = cleaned.clone();
            Arc::new(move |_task, cancel| {
                let started = started.clone();
                let cleaned = cleaned.clone();
                Box::pin(async move {
                    started.notify_one();
                    cancel.cancelled().await;
                    cleaned.store(true, Ordering::SeqCst);
                    Err(crate::client::ConfirmClientError::Cancelled)
                })
            })
        };
        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let server_task = tokio::spawn(async move {
            let server = AskHumanServer::with_submitter(submitter);
            let shutdown = server.shutdown_token();
            let (read, write) = tokio::io::split(server_transport);
            server
                .serve((super::super::CancelOnEof::new(read, shutdown), write))
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let (read, write) = tokio::io::split(client_transport);
        let mut write = Some(write);
        let mut reader = Some(BufReader::new(read));
        initialize(write.as_mut().unwrap(), reader.as_mut().unwrap()).await;
        send_json(
            write.as_mut().unwrap(),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "ask_human", "arguments": valid_arguments() }
            }),
        )
        .await;
        started.notified().await;
        if shutdown {
            drop(write.take());
            drop(reader.take());
        } else {
            send_json(
                write.as_mut().unwrap(),
                json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/cancelled",
                    "params": { "requestId": 2 }
                }),
            )
            .await;
        }
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !cleaned.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("request cleanup");
        drop(write);
        drop(reader);
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn client_cancellation_reaches_active_request_cleanup() {
        cancellation_case(false).await;
    }

    #[tokio::test]
    async fn server_shutdown_reaches_active_request_cleanup() {
        cancellation_case(true).await;
    }

    #[tokio::test]
    async fn protocol_call_returns_one_canonical_structured_result() {
        let calls = Arc::new(AtomicUsize::new(0));
        let submitter: TestSubmitter = {
            let calls = calls.clone();
            Arc::new(move |_task, _cancel| {
                calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {
                    Ok(ConfirmResult {
                        action_id: "received".into(),
                        comment: Some("private transport detail".into()),
                        source_channel_id: "imessage".into(),
                    })
                })
            })
        };
        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let server_task = tokio::spawn(async move {
            let server = AskHumanServer::with_submitter(submitter);
            let shutdown = server.shutdown_token();
            let (read, write) = tokio::io::split(server_transport);
            server
                .serve((super::super::CancelOnEof::new(read, shutdown), write))
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let (read, mut write) = tokio::io::split(client_transport);
        let mut reader = BufReader::new(read);
        initialize(&mut write, &mut reader).await;
        send_json(
            &mut write,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "ask_human", "arguments": valid_arguments() }
            }),
        )
        .await;
        let response = read_response(&mut reader, 2).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            response.pointer("/result/structuredContent"),
            Some(&json!({
                "request_id": "request-1",
                "selected_choice_id": "received",
                "source_channel_id": "imessage"
            }))
        );
        let encoded = response.to_string();
        assert!(!encoded.contains("private transport detail"));
        assert!(!encoded.contains("recipient"));
        drop(write);
        drop(reader);
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn protocol_coordinator_failures_cancellation_races_and_disconnect_are_isolated() {
        use crate::app::confirm_coordinator::ConfirmOutcome;
        use crate::client::ConfirmClientError;
        use crate::models::ConfirmFallbackReason;

        let registry = Arc::new(crate::daemon::request::RequestRegistry::new());
        let (entry_tx, mut entries) = tokio::sync::mpsc::unbounded_channel();
        let submitter: TestSubmitter = {
            let registry = registry.clone();
            Arc::new(move |task, cancel| {
                let registry = registry.clone();
                let entry_tx = entry_tx.clone();
                Box::pin(async move {
                    let (entry, mut rx) = registry.create_confirm(task, None).unwrap();
                    entry_tx.send(entry.clone()).unwrap();
                    let outcome = tokio::select! {
                        _ = cancel.cancelled() => {
                            assert!(entry.coordinator.cancel());
                            entry.cancel.notify_waiters();
                            Err(ConfirmClientError::Cancelled)
                        }
                        result = rx.recv() => match result.unwrap() {
                            ConfirmOutcome::Final(answer) => Ok(answer),
                            ConfirmOutcome::Fallback(reason) => Err(ConfirmClientError::Fallback(reason)),
                        }
                    };
                    registry.remove_confirm(&entry.request_id);
                    outcome
                })
            })
        };
        let (mut reader, mut write, server_task) = protocol_session(submitter).await;
        // A channel failure and an expiry both return errors without poisoning later calls.
        for (id, reason) in [
            (2, ConfirmFallbackReason::NoAvailableChannel),
            (3, ConfirmFallbackReason::Expired),
        ] {
            let mut arguments = valid_arguments();
            arguments["request_id"] = json!(format!("request-{id}"));
            send_json(&mut write, call(id, arguments)).await;
            let entry = entries.recv().await.unwrap();
            assert!(entry.coordinator.fallback(reason));
            let response = read_response(&mut reader, id).await;
            assert!(response.get("error").is_some());
            assert!(response.pointer("/result/structuredContent").is_none());
            assert!(!entry.coordinator.submit_wire(0, None, "imessage").unwrap());
            assert_eq!(registry.active_count(), 0);
        }
        let mut pending = Vec::new();
        for id in [4, 5] {
            let mut arguments = valid_arguments();
            arguments["request_id"] = json!(format!("request-{id}"));
            send_json(&mut write, call(id, arguments)).await;
            pending.push(entries.recv().await.unwrap());
        }
        assert_ne!(pending[0].token, pending[1].token);
        assert_eq!(registry.active_count(), 2);
        send_json(&mut write, json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 4}})).await;
        // MCP cancellation suppresses the cancelled response rather than returning a choice.
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while registry.active_count() != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        send_json(
            &mut write,
            json!({"jsonrpc": "2.0", "id": 45, "method": "tools/list"}),
        )
        .await;
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).await.unwrap() > 0);
                let response: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_ne!(response.get("id"), Some(&json!(4)));
                if response.get("id") == Some(&json!(45)) {
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(registry.active_count(), 1);
        assert!(!pending[0]
            .coordinator
            .submit_wire(0, None, "imessage")
            .unwrap());
        assert!(!pending[1].coordinator.is_terminal());

        // Multiple channel candidates compete at the existing coordinator's atomic terminal gate.
        let barrier = Arc::new(tokio::sync::Barrier::new(8));
        let mut candidates = Vec::new();
        for _ in 0..8 {
            let barrier = barrier.clone();
            let coordinator = pending[1].coordinator.clone();
            candidates.push(tokio::spawn(async move {
                barrier.wait().await;
                coordinator.submit_wire(0, None, "imessage").unwrap()
            }));
        }
        let mut winners = 0;
        for candidate in candidates {
            winners += usize::from(candidate.await.unwrap());
        }
        assert_eq!(winners, 1);
        assert_eq!(
            read_response(&mut reader, 5).await["result"]["structuredContent"],
            json!({"request_id": "request-5", "selected_choice_id": "received", "source_channel_id": "imessage"})
        );
        assert_eq!(registry.active_count(), 0);
        assert!(!pending[1]
            .coordinator
            .submit_wire(1, None, "imessage")
            .unwrap());

        // EOF must reach every pending canonical request, not just the most recent one.
        let mut disconnected = Vec::new();
        for id in [6, 7] {
            let mut arguments = valid_arguments();
            arguments["request_id"] = json!(format!("request-{id}"));
            send_json(&mut write, call(id, arguments)).await;
            disconnected.push(entries.recv().await.unwrap());
        }
        drop(write);
        drop(reader);
        tokio::time::timeout(std::time::Duration::from_secs(2), server_task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(registry.active_count(), 0);
        for entry in disconnected {
            assert!(entry.coordinator.is_terminal());
            assert!(!entry.coordinator.submit_wire(0, None, "imessage").unwrap());
            assert!(entry.coordinator.winner_channel_id().is_none());
        }
    }

    #[tokio::test]
    async fn mcp_task_uses_existing_coordinator_and_request_id_exactly_once() {
        let task = build_confirm_task(params(None)).unwrap();
        let duplicate = task.clone();
        let registry = crate::daemon::request::RequestRegistry::new();
        let (entry, mut result) = registry.create_confirm(task, None).unwrap();
        assert_eq!(entry.request_id, "request-1");
        assert!(entry.coordinator.submit_wire(0, None, "imessage").unwrap());
        assert!(!entry.coordinator.submit_wire(1, None, "imessage").unwrap());
        let crate::app::confirm_coordinator::ConfirmOutcome::Final(answer) =
            result.recv().await.unwrap()
        else {
            panic!("expected canonical result")
        };
        assert_eq!(answer.action_id, "received");
        assert_eq!(answer.source_channel_id, "imessage");
        assert!(result.try_recv().is_err());
        assert!(registry.create_confirm(duplicate, None).is_err());
    }
}
