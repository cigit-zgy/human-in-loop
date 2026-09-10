//! CLI 配置命令公共工具：点号路径读写、类型强制、交互输入与异步运行时助手。

use crate::config::AppConfig;
use crate::i18n::Lang;
use serde_json::Value;
use std::io::Write;

/// 本地化：按语言选英 / 中。CLI 配置命令专属文案用它（既有错误仍走 `i18n::tr`）。
pub fn t(lang: Lang, en: &str, zh: &str) -> String {
    match lang {
        Lang::Zh => zh.to_string(),
        Lang::En => en.to_string(),
    }
}

/// 创建一次性当前线程 tokio 运行时并 block_on（与 `client::run_ask` 同款）。
pub fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(fut)
}

/// Return daemon status on every supported desktop platform.
pub fn daemon_status() -> Option<crate::ipc::StatusInfo> {
    block_on(crate::client::request_status())
}

// ——— 点号路径读写（基于 serde_json::Value，camelCase）———

/// 取点号路径的引用（任一段缺失返回 None）。
pub fn get_path<'a>(root: &'a Value, key: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in key.split('.') {
        cur = cur.get(seg)?;
    }
    Some(cur)
}

/// 写点号路径；中间 / 末段缺失即报「未知键」（配置 schema 固定，不自动建节点）。
pub fn set_path(root: &mut Value, key: &str, val: Value) -> Result<(), String> {
    let segs: Vec<&str> = key.split('.').collect();
    let mut cur = root;
    for seg in &segs[..segs.len() - 1] {
        cur = cur.get_mut(*seg).ok_or_else(|| unknown_key(key))?;
    }
    let last = segs[segs.len() - 1];
    let obj = cur.as_object_mut().ok_or_else(|| unknown_key(key))?;
    if !obj.contains_key(last) {
        return Err(unknown_key(key));
    }
    obj.insert(last.to_string(), val);
    Ok(())
}

fn unknown_key(key: &str) -> String {
    format!("unknown config key: {key}")
}

/// 把字符串输入按「该键现有值的 JSON 类型」转成 `Value`（无 schema 的类型化写入）。
pub fn coerce_to_type(existing: &Value, input: &str) -> Result<Value, String> {
    match existing {
        Value::Bool(_) => parse_bool(input).map(Value::Bool),
        Value::Number(n) => {
            if n.is_f64() && !n.is_i64() && !n.is_u64() {
                input
                    .trim()
                    .parse::<f64>()
                    .map(Value::from)
                    .map_err(|_| format!("expected a number, got: {input}"))
            } else {
                input
                    .trim()
                    .parse::<i64>()
                    .map(Value::from)
                    .map_err(|_| format!("expected an integer, got: {input}"))
            }
        }
        Value::String(_) => Ok(Value::String(input.to_string())),
        _ => Err("this key is not a simple scalar and cannot be set via the CLI".to_string()),
    }
}

/// 宽松布尔解析。
pub fn parse_bool(s: &str) -> Result<bool, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" | "y" => Ok(true),
        "false" | "0" | "no" | "off" | "n" => Ok(false),
        _ => Err(format!("expected a boolean (true/false), got: {s}")),
    }
}

/// Return the maintained local configuration as JSON.
pub fn redacted_value() -> Value {
    let cfg = AppConfig::load_without_secrets();
    serde_json::to_value(&cfg).unwrap_or(Value::Null)
}

/// Expand a leading tilde using the platform home-directory provider.
pub fn expand_tilde(p: &str) -> std::path::PathBuf {
    crate::cli::file_attachment::expand_tilde(p, &crate::paths::home())
}

// ——— 交互输入 ———

/// 普通行输入（显示当前值，回车保留）。提示走 stderr，保持 stdout 洁净。
pub fn prompt_line(label: &str, current: &str) -> Result<String, String> {
    eprint!("{label}");
    if !current.is_empty() {
        eprint!(" [{current}]");
    }
    eprint!(": ");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let v = line.trim_end_matches(['\n', '\r']).to_string();
    Ok(if v.is_empty() { current.to_string() } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_parse_variants() {
        assert!(parse_bool("true").unwrap());
        assert!(parse_bool("YES").unwrap());
        assert!(!parse_bool("off").unwrap());
        assert!(parse_bool("nope").is_err());
    }

    #[test]
    fn path_get_set_on_config() {
        let cfg = AppConfig::default();
        let mut v = serde_json::to_value(&cfg).unwrap();
        assert_eq!(
            get_path(&v, "channels.imessage.identityMode")
                .unwrap()
                .as_str(),
            Some("distinct_peer")
        );
        set_path(&mut v, "channels.autoActivation", Value::Bool(true)).unwrap();
        assert_eq!(
            get_path(&v, "channels.autoActivation").unwrap().as_bool(),
            Some(true)
        );
        assert!(set_path(&mut v, "channels.nope", Value::Bool(true)).is_err());
    }

    #[test]
    fn coerce_uses_existing_type() {
        assert_eq!(
            coerce_to_type(&Value::Bool(false), "true").unwrap(),
            Value::Bool(true)
        );
        assert!(coerce_to_type(&Value::from(1i64), "abc").is_err());
        assert_eq!(
            coerce_to_type(&Value::String(String::new()), "hi").unwrap(),
            Value::String("hi".to_string())
        );
    }

    #[test]
    fn config_paths_expand_all_home_forms() {
        let home = crate::paths::home();
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("~/secret.txt"), home.join("secret.txt"));
        assert_eq!(expand_tilde("~\\secret.txt"), home.join("secret.txt"));
    }
}
