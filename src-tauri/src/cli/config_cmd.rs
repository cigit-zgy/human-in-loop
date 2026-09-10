//! `human-in-loop config <show|get|set|unset|path|help>` —— 通用键值兜底（点号 camelCase 键）。

use super::cfgio;
use crate::config::AppConfig;
use crate::i18n::{err_prefix, Lang};
use serde_json::Value;
use std::process::exit;

pub fn dispatch(args: &[String], lang: Lang) {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("help");
    let rest = &args[args.len().min(1)..];
    let r = match sub {
        "show" | "list" => show(rest, lang),
        "get" => get(rest, lang),
        "set" => set(rest, lang),
        "unset" => unset(rest, lang),
        "path" => {
            print_line(&crate::paths::config_file().display().to_string());
            Ok(())
        }
        "help" | "-h" | "--help" => {
            print_line(&help(lang));
            Ok(())
        }
        other => Err(cfgio::t(
            lang,
            &format!("unknown subcommand: {other}\n\n{}", help(lang)),
            &format!("未知子命令: {other}\n\n{}", help(lang)),
        )),
    };
    if let Err(e) = r {
        eprintln!("{}{}", err_prefix(lang), e);
        exit(1);
    }
}

fn show(args: &[String], _lang: Lang) -> Result<(), String> {
    let json = args.iter().any(|a| a == "--json");
    let v = cfgio::redacted_value();
    if json {
        print_line(&serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        let mut lines = Vec::new();
        flatten(&v, "", &mut lines);
        for (k, val) in lines {
            print_line(&format!("{k} = {val}"));
        }
    }
    Ok(())
}

fn get(args: &[String], lang: Lang) -> Result<(), String> {
    let key = args
        .first()
        .ok_or_else(|| cfgio::t(lang, "usage: config get <key>", "用法: config get <键>"))?;
    let v = cfgio::redacted_value();
    match cfgio::get_path(&v, key) {
        Some(val) => {
            print_line(&value_to_plain(val));
            Ok(())
        }
        None => Err(cfgio::t(
            lang,
            &format!("unknown config key: {key}"),
            &format!("未知配置键: {key}"),
        )),
    }
}

fn set(args: &[String], lang: Lang) -> Result<(), String> {
    let key = args
        .first()
        .ok_or_else(|| {
            cfgio::t(
                lang,
                "usage: config set <key> <value>",
                "用法: config set <键> <值>",
            )
        })?
        .clone();

    let mut cfg = AppConfig::load_without_secrets();

    // All maintained configuration is ordinary local configuration.
    let value_str = args.get(1).ok_or_else(|| {
        cfgio::t(
            lang,
            "usage: config set <key> <value>",
            "用法: config set <键> <值>",
        )
    })?;
    let mut v = serde_json::to_value(&cfg).unwrap_or(Value::Null);
    let existing = cfgio::get_path(&v, &key)
        .ok_or_else(|| {
            cfgio::t(
                lang,
                &format!("unknown config key: {key}"),
                &format!("未知配置键: {key}"),
            )
        })?
        .clone();
    let coerced = cfgio::coerce_to_type(&existing, value_str)?;
    cfgio::set_path(&mut v, &key, coerced)?;
    cfg = serde_json::from_value(v).map_err(|e| {
        cfgio::t(
            lang,
            &format!("invalid value for {key}: {e}"),
            &format!("{key} 的值非法: {e}"),
        )
    })?;
    cfg.save().map_err(|e| e.to_string())?;
    print_line(&cfgio::t(
        lang,
        &format!("{key} updated"),
        &format!("{key} 已更新"),
    ));
    Ok(())
}

fn unset(args: &[String], lang: Lang) -> Result<(), String> {
    let key = args
        .first()
        .ok_or_else(|| cfgio::t(lang, "usage: config unset <key>", "用法: config unset <键>"))?;
    // Reset to the schema default.
    let defaults = serde_json::to_value(AppConfig::default()).unwrap_or(Value::Null);
    let def = cfgio::get_path(&defaults, key)
        .ok_or_else(|| {
            cfgio::t(
                lang,
                &format!("unknown config key: {key}"),
                &format!("未知配置键: {key}"),
            )
        })?
        .clone();
    let mut cfg = AppConfig::load_without_secrets();
    let mut v = serde_json::to_value(&cfg).unwrap_or(Value::Null);
    cfgio::set_path(&mut v, key, def)?;
    cfg = serde_json::from_value(v).map_err(|e| e.to_string())?;
    cfg.save().map_err(|e| e.to_string())?;
    print_line(&cfgio::t(
        lang,
        &format!("{key} reset to default"),
        &format!("{key} 已重置为默认值"),
    ));
    Ok(())
}

/// 把嵌套 `Value` 扁平成 `a.b.c = v` 行（仅标量；对象递归）。
fn flatten(v: &Value, prefix: &str, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(val, &key, out);
            }
        }
        other => out.push((prefix.to_string(), value_to_plain(other))),
    }
}

fn value_to_plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn help(lang: Lang) -> String {
    cfgio::t(
        lang,
        "human-in-loop config — generic key/value over ~/.human-in-loop/config.json (fallback; prefer 'channel' for IM setup)\n\
\n\
  config show [--json]            Print effective config\n\
  config get <key>                Print one value (dotted camelCase key)\n\
  config set <key> <value>        Set a key (e.g. general.language zh)\n\
  config unset <key>              Reset a key to its default\n\
  config path                     Print the config file path\n\
\n\
  Keys: general.* / channels.imessage.* / channels.autoActivation / experimental.enabled",
        "human-in-loop config —— 对 ~/.human-in-loop/config.json 的通用键值（兜底；渠道配置优先用 'channel'）\n\
\n\
  config show [--json]            打印生效配置\n\
  config get <键>                 打印某个值（点号小驼峰键）\n\
  config set <键> <值>            设置键（如 general.language zh）\n\
  config unset <键>               重置为默认\n\
  config path                     打印配置文件路径\n\
\n\
  键：general.* / channels.imessage.* / channels.autoActivation / experimental.enabled",
    )
}

/// stdout 输出一行（BrokenPipe 静默；见 `cli::print_line`）。
fn print_line(s: &str) {
    super::print_line(s);
}
