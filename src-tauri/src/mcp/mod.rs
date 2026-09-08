//! Local STDIO MCP server exposing blocking `ask_human` and one-way `notify_human`.
//!
//! The handler submits the existing structured `ConfirmTask` IPC request and therefore reuses the
//! daemon's canonical coordinator and configured Feishu/iMessage sessions. The input stream is
//! cancellation-aware so a client disconnect reaches the same request-owned cleanup path as an
//! explicit MCP cancellation.

pub(crate) mod ask;
pub(crate) mod human;

use rmcp::{transport::stdio, ServiceExt};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::sync::CancellationToken;

struct CancelOnEof<R> {
    inner: R,
    cancel: CancellationToken,
}

impl<R> CancelOnEof<R> {
    fn new(inner: R, cancel: CancellationToken) -> Self {
        Self { inner, cancel }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for CancelOnEof<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let can_read = buf.remaining() > 0;
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) if can_read && buf.filled().len() == before => {
                self.cancel.cancel();
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => {
                self.cancel.cancel();
                Poll::Ready(Err(error))
            }
            other => other,
        }
    }
}

/// 进入 STDIO MCP server 事件循环（不返回）。
pub fn run() -> ! {
    let code = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt.block_on(serve()),
        Err(_) => 3,
    };
    std::process::exit(code);
}

/// 建 server、握手、等关闭。返回进程退出码。
async fn serve() -> i32 {
    #[cfg(windows)]
    let parent = mcp_parent_process();
    let server = human::AskHumanServer::new();
    let shutdown = server.shutdown_token();
    let (input, output) = stdio();
    match server
        .serve((CancelOnEof::new(input, shutdown), output))
        .await
    {
        Ok(service) => {
            #[cfg(windows)]
            let parent_watcher = parent.map(|parent| {
                let cancellation = service.cancellation_token();
                tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
                    wait_for_parent_exit(&parent).await;
                    // Cancelling the rmcp service also cancels every request child token. The
                    // handler drops its daemon connection, which finalizes channel cleanup.
                    cancellation.cancel();
                }))
            });
            let _ = service.waiting().await;
            #[cfg(windows)]
            drop(parent_watcher);
            0
        }
        // 握手失败（如非 MCP 客户端误启）：直接退出，stdout 不能有杂音。
        Err(_) => 3,
    }
}

#[cfg(windows)]
fn mcp_parent_process() -> Option<crate::agents::detect::ProcessIdentity> {
    let parent_pid = crate::agents::detect::parent_pid(std::process::id())?;
    crate::agents::detect::inspect_process(parent_pid)
}

#[cfg(windows)]
async fn wait_for_parent_exit(parent: &crate::agents::detect::ProcessIdentity) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        if !original_parent_alive(parent) {
            return;
        }
    }
}

#[cfg(windows)]
fn original_parent_alive(expected: &crate::agents::detect::ProcessIdentity) -> bool {
    if !crate::agents::detect::pid_alive(expected.pid) {
        return false;
    }
    let Some(current) = crate::agents::detect::inspect_process(expected.pid) else {
        // Access restrictions are not evidence of process death. Fail open until pid_alive says
        // otherwise rather than cancelling a healthy Agent session.
        return true;
    };
    process_instance_matches(expected, &current)
}

fn process_instance_matches(
    expected: &crate::agents::detect::ProcessIdentity,
    current: &crate::agents::detect::ProcessIdentity,
) -> bool {
    expected.pid == current.pid
        && match (expected.creation_time, current.creation_time) {
            (Some(expected), Some(current)) => expected == current,
            _ => true,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(pid: u32, creation_time: Option<u64>) -> crate::agents::detect::ProcessIdentity {
        crate::agents::detect::ProcessIdentity {
            pid,
            parent_pid: 1,
            executable: None,
            command_line: None,
            session_id: None,
            creation_time,
        }
    }

    #[test]
    fn parent_identity_rejects_pid_reuse_and_accepts_unavailable_creation_time() {
        assert!(process_instance_matches(
            &identity(42, Some(100)),
            &identity(42, Some(100))
        ));
        assert!(!process_instance_matches(
            &identity(42, Some(100)),
            &identity(42, Some(101))
        ));
        assert!(!process_instance_matches(
            &identity(42, Some(100)),
            &identity(43, Some(100))
        ));
        assert!(process_instance_matches(
            &identity(42, None),
            &identity(42, Some(100))
        ));
    }
}
