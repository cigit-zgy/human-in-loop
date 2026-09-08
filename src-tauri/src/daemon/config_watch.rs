//! 监听 `~/.askhuman/config.json` 变更（`notify` + 去抖）。
//!
//! 本模块只做「文件变更 → 去抖脉冲」，与 Daemon 内部状态解耦：调用 `spawn()` 得到一个接收端，
//! 每当 config.json 稳定写入后收到一个 `()`。具体「重载/失效 Router/通知 GUI」由 daemon 处理。
//!
//! 监听的是**配置目录**（非单个文件）：配置用「写临时文件 + rename」原子落盘，rename 会替换 inode，
//! 直接 watch 文件在首次替换后即失效；watch 目录再按文件名过滤最稳。daemon.log 等同目录写入被过滤掉。

use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

/// 启动监听，返回「去抖后的变更脉冲」接收端。
pub fn spawn() -> UnboundedReceiver<()> {
    let (raw_tx, mut raw_rx) = unbounded_channel::<()>();

    // notify 的 watcher 跑在专用线程；回调里按文件名过滤，仅 config.json 触发。
    std::thread::spawn(move || {
        use notify::{RecursiveMode, Watcher};
        let dir = crate::paths::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let mut watcher =
            match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res {
                    let hit = ev
                        .paths
                        .iter()
                        .any(|p| p.file_name().map(|n| n == "config.json").unwrap_or(false));
                    if hit {
                        let _ = raw_tx.send(());
                    }
                }
            }) {
                Ok(w) => w,
                Err(_) => return,
            };
        if watcher.watch(&dir, RecursiveMode::NonRecursive).is_err() {
            return;
        }
        // 阻塞保活：watcher 持有回调（含 raw_tx），park 住本线程直到进程退出。
        loop {
            std::thread::park();
        }
    });

    // 去抖：首个脉冲后等 300ms 静默，再向上层发一次（合并原子写产生的多个 rename/modify 事件）。
    let (out_tx, out_rx) = unbounded_channel::<()>();
    tokio::spawn(async move {
        while raw_rx.recv().await.is_some() {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(300)) => break,
                    m = raw_rx.recv() => {
                        if m.is_none() {
                            return;
                        }
                    }
                }
            }
            if out_tx.send(()).is_err() {
                break;
            }
        }
    });

    out_rx
}
