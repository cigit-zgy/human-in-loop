//! Minimal MCP boundary for the canonical structured-confirmation runtime.

use crate::confirm::ActionRole;
use crate::ipc::{ConfirmTask, ConfirmTaskOrigin};
use crate::models::{
    ConfirmChoice, ConfirmDetail, ConfirmField, ConfirmFieldKind, ConfirmPresentation,
    ConfirmResult, ConfirmSpec,
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
pub struct AskHumanChoice {
    /// Stable semantic identifier returned unchanged when this choice wins.
    pub id: String,
    /// Compact human-visible label.
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AskHumanParams {
    /// Local path inside the associated GitHub repository. Omit only for a genuinely
    /// non-repository decision. The displayed repository name is resolved from Git origin.
    #[serde(default)]
    pub repository_path: Option<String>,
    /// Short identity of the calling agent or source, for example `Codex`.
    pub source_agent: String,
    /// Compact decision question shown to the human.
    pub question: String,
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
}

#[tool_router(router = tool_router)]
impl AskHumanServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            shutdown: CancellationToken::new(),
            #[cfg(test)]
            submitter: None,
        }
    }

    #[cfg(test)]
    fn with_submitter(submitter: TestSubmitter) -> Self {
        Self {
            tool_router: Self::tool_router(),
            shutdown: CancellationToken::new(),
            submitter: Some(submitter),
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
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AskHumanServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Human in Loop exposes one blocking mutation tool: `ask_human`. It returns a stable canonical choice id and never accepts transport credentials or recipient identity.",
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
            let id = required_compact(&choice.id, "choice id")?;
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

    let project = match params.repository_path.as_deref() {
        Some(raw) if raw.trim().is_empty() => {
            return Err("repository_path must not be empty".to_string())
        }
        Some(raw) => {
            let project = crate::project::detect_from(Path::new(raw));
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
    };
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
        Some(value) => required_compact(&value, "request_id")?,
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
                body_md: String::new(),
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

fn required_compact(value: &str, field: &str) -> Result<String, String> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(value)
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
        .expect("MCP response timeout")
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
        assert!(read_response(reader, 1).await.get("result").is_some());
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
            choices: vec![choice("received", "Received"), choice("failed", "Failed")],
            context: Some("MCP release candidate".into()),
            recommended_choice: Some("received".into()),
            request_id: Some("request-1".into()),
        }
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
            "7F32",
            &task.source,
            Some("human-in-loop"),
        )
        .unwrap();
        assert_eq!(rendered.text.lines().nth(1), Some("Codex · human-in-loop"));
        assert!(rendered.text.contains("1  Received [recommended]"));
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
    fn discovery_exposes_only_ask_human_and_no_transport_capability() {
        let tools = AskHumanServer::new().tool_router.list_all();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "ask_human");
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
    async fn mcp_task_uses_existing_coordinator_and_request_id_exactly_once() {
        let task = build_confirm_task(params(None)).unwrap();
        let duplicate = task.clone();
        let registry = crate::daemon::request::RequestRegistry::new();
        let (entry, mut result) = registry.create_confirm(task, None).unwrap();
        assert_eq!(entry.request_id, "request-1");
        assert!(entry.coordinator.submit_wire(0, None, "imessage").unwrap());
        assert!(!entry.coordinator.submit_wire(1, None, "feishu").unwrap());
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
