//! `AskHuman channel` configuration for the two maintained remote delivery channels.

use super::cfgio::{self, SecretSource};
use crate::config::AppConfig;
use crate::i18n::{err_prefix, Lang};
use std::collections::HashMap;
use std::process::exit;

pub(crate) const CHANNELS: [&str; 2] = ["feishu", "imessage"];

pub fn dispatch(args: &[String], lang: Lang) {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let rest = &args[args.len().min(1)..];
    let result = match sub {
        "list" | "ls" => list(rest, lang),
        "set" => set(rest, lang),
        "enable" => toggle(rest, true, lang),
        "disable" => toggle(rest, false, lang),
        "test" => test(rest, lang),
        "detect" => detect(rest, lang),
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
    if let Err(error) = result {
        eprintln!("{}{}", err_prefix(lang), error);
        exit(1);
    }
}

fn list(args: &[String], lang: Lang) -> Result<(), String> {
    let config = AppConfig::load_without_secrets();
    if args.iter().any(|arg| arg == "--json") {
        let value: Vec<_> = CHANNELS
            .iter()
            .map(|name| {
                serde_json::json!({
                    "name": name,
                    "enabled": is_enabled(&config, name),
                    "configured": is_configured(&config, name),
                })
            })
            .collect();
        print_line(&serde_json::to_string_pretty(&value).unwrap_or_default());
        return Ok(());
    }
    print_line(&cfgio::t(
        lang,
        "channel    enabled  configured",
        "渠道        已启用   配置齐全",
    ));
    for name in CHANNELS {
        print_line(&format!(
            "{name:<10} {:<8} {}",
            yes_no_word(is_enabled(&config, name), lang),
            yes_no_word(is_configured(&config, name), lang),
        ));
    }
    Ok(())
}

#[derive(Default)]
struct ParsedFlags {
    enabled: Option<bool>,
    values: HashMap<String, String>,
    secrets: HashMap<String, SecretSource>,
}

fn parse_flags(args: &[String], lang: Lang) -> Result<ParsedFlags, String> {
    let mut parsed = ParsedFlags::default();
    let mut index = 0;
    while index < args.len() {
        let flag = &args[index];
        if flag == "--enable" || flag == "--disable" {
            parsed.enabled = Some(flag == "--enable");
            index += 1;
            continue;
        }
        let name = flag.strip_prefix("--").ok_or_else(|| {
            cfgio::t(
                lang,
                &format!("unexpected argument: {flag}"),
                &format!("非预期参数: {flag}"),
            )
        })?;
        if let Some(field) = name.strip_suffix("-stdin") {
            parsed.secrets.insert(field.into(), SecretSource::Stdin);
            index += 1;
            continue;
        }
        let value = args.get(index + 1).ok_or_else(|| {
            cfgio::t(
                lang,
                &format!("{flag} needs a value"),
                &format!("{flag} 需要参数值"),
            )
        })?;
        if let Some(field) = name.strip_suffix("-env") {
            parsed
                .secrets
                .insert(field.into(), SecretSource::Env(value.clone()));
        } else if let Some(field) = name.strip_suffix("-file") {
            parsed
                .secrets
                .insert(field.into(), SecretSource::File(value.clone()));
        } else {
            parsed.values.insert(name.into(), value.clone());
        }
        index += 2;
    }
    Ok(parsed)
}

fn set(args: &[String], lang: Lang) -> Result<(), String> {
    let name = canon(
        args.first()
            .ok_or_else(|| "usage: channel set <name> [flags]".to_string())?,
        lang,
    )?;
    let mut parsed = parse_flags(&args[1..], lang)?;
    if parsed.enabled.is_none() && parsed.values.is_empty() && parsed.secrets.is_empty() {
        return Err(cfgio::t(lang, "no settings supplied", "未提供设置项"));
    }
    let mut config = AppConfig::load_without_secrets();
    match name {
        "feishu" => {
            let channel = &mut config.channels.feishu;
            if let Some(enabled) = parsed.enabled {
                channel.enabled = enabled;
            }
            if let Some(value) = parsed.values.remove("app-id") {
                channel.app_id = value;
            }
            if let Some(value) = parsed.values.remove("open-id") {
                channel.open_id = value;
            }
            if let Some(value) = parsed.values.remove("base-url") {
                channel.base_url = value;
            }
            if let Some(source) = parsed.secrets.remove("app-secret") {
                channel.app_secret = cfgio::read_secret(&source, lang)?;
            }
        }
        "imessage" => {
            let channel = &mut config.channels.imessage;
            if let Some(enabled) = parsed.enabled {
                channel.enabled = enabled;
            }
            if let Some(value) = parsed.values.remove("recipient") {
                channel.recipient = value;
            }
            if let Some(value) = parsed.values.remove("chat-guid") {
                channel.chat_guid = value;
            }
            if let Some(value) = parsed.values.remove("chat-id") {
                channel.chat_id = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| "chat-id must be an integer".to_string())?,
                );
            }
        }
        _ => unreachable!(),
    }
    let mut leftovers: Vec<_> = parsed
        .values
        .into_keys()
        .chain(parsed.secrets.into_keys())
        .collect();
    leftovers.sort();
    if !leftovers.is_empty() {
        return Err(format!(
            "unknown option(s) for {name}: {}",
            leftovers.join(", ")
        ));
    }
    config.save().map_err(|error| error.to_string())?;
    print_line(&cfgio::t(
        lang,
        &format!("{name} updated"),
        &format!("{name} 已更新"),
    ));
    Ok(())
}

fn toggle(args: &[String], enabled: bool, lang: Lang) -> Result<(), String> {
    let name = canon(
        args.first()
            .ok_or_else(|| "usage: channel enable|disable <name>".to_string())?,
        lang,
    )?;
    let mut config = AppConfig::load_without_secrets();
    match name {
        "feishu" => config.channels.feishu.enabled = enabled,
        "imessage" => config.channels.imessage.enabled = enabled,
        _ => unreachable!(),
    }
    config.save().map_err(|error| error.to_string())
}

fn test(args: &[String], lang: Lang) -> Result<(), String> {
    let name = canon(
        args.first()
            .ok_or_else(|| "usage: channel test <name>".to_string())?,
        lang,
    )?;
    let config = AppConfig::load();
    match name {
        "feishu" => cfgio::block_on(crate::commands::feishu_test(
            crate::commands::FeishuTestArgs {
                app_id: config.channels.feishu.app_id,
                app_secret: String::new(),
                open_id: config.channels.feishu.open_id,
                base_url: config.channels.feishu.base_url,
            },
        ))
        .map(|message| print_line(&message)),
        "imessage" => {
            let health =
                cfgio::block_on(crate::channels::imessage::health(&config.channels.imessage));
            if health == crate::channels::imessage::HealthState::Ready {
                print_line(health.as_str());
                Ok(())
            } else {
                Err(health.as_str().to_string())
            }
        }
        _ => unreachable!(),
    }
}

fn detect(args: &[String], lang: Lang) -> Result<(), String> {
    let name = canon(
        args.first()
            .ok_or_else(|| "usage: channel detect feishu [--save]".to_string())?,
        lang,
    )?;
    if name == "imessage" {
        return Err(cfgio::t(
            lang,
            "Select an existing direct peer with `imsg chats --json`, then save recipient, chat-id, and chat-guid.",
            "请用 `imsg chats --json` 选择已有单聊，再保存 recipient、chat-id 和 chat-guid。",
        ));
    }
    let config = AppConfig::load();
    let code = cfgio::block_on(crate::commands::feishu_detect_prepare(
        crate::commands::FeishuDetectArgs {
            app_id: config.channels.feishu.app_id.clone(),
            app_secret: String::new(),
            base_url: config.channels.feishu.base_url.clone(),
        },
    ))?;
    eprintln!(
        "{}",
        cfgio::t(
            lang,
            &format!("Send this code to your bot within 120s: {code}"),
            &format!("请在 120 秒内把识别码发给机器人: {code}")
        )
    );
    let open_id = cfgio::block_on(crate::commands::feishu_detect_wait(
        crate::commands::FeishuWaitArgs {
            app_id: config.channels.feishu.app_id,
            app_secret: String::new(),
            base_url: config.channels.feishu.base_url,
            code,
        },
    ))?;
    print_line(&open_id);
    if args.iter().any(|arg| arg == "--save") {
        let mut saved = AppConfig::load_without_secrets();
        saved.channels.feishu.open_id = open_id;
        saved.save().map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn canon(name: &str, lang: Lang) -> Result<&'static str, String> {
    match name.trim().to_ascii_lowercase().as_str() {
        "feishu" | "lark" => Ok("feishu"),
        "imessage" | "messages" => Ok("imessage"),
        other => Err(cfgio::t(
            lang,
            &format!("unknown channel: {other} (expected feishu|imessage)"),
            &format!("未知渠道: {other}（应为 feishu|imessage）"),
        )),
    }
}

pub(crate) fn conn_name(name: &str) -> &'static str {
    match name {
        "feishu" => "feishu",
        "imessage" => "imessage",
        _ => "",
    }
}

pub(crate) fn is_enabled(config: &AppConfig, name: &str) -> bool {
    match name {
        "feishu" => config.channels.feishu.enabled,
        "imessage" => config.channels.imessage.enabled,
        _ => false,
    }
}

pub(crate) fn is_configured(config: &AppConfig, name: &str) -> bool {
    match name {
        "feishu" => {
            !config.channels.feishu.app_id.trim().is_empty()
                && !config.channels.feishu.open_id.trim().is_empty()
                && cfgio::secret_is_set(crate::secrets::ACCOUNT_FEISHU_SECRET)
        }
        "imessage" => {
            !config.channels.imessage.recipient.trim().is_empty()
                && config.channels.imessage.chat_id.is_some()
                && !config.channels.imessage.chat_guid.trim().is_empty()
        }
        _ => false,
    }
}

fn yes_no_word(value: bool, lang: Lang) -> String {
    if value {
        cfgio::t(lang, "yes", "是")
    } else {
        cfgio::t(lang, "no", "否")
    }
}

fn help(lang: Lang) -> String {
    cfgio::t(
        lang,
        "AskHuman channel — maintained channels: feishu | imessage\n\n  channel list [--json]\n  channel set feishu [--enable|--disable] --app-id <id> --open-id <id> --base-url <url> --app-secret-{env|file|stdin}\n  channel set imessage [--enable|--disable] --recipient <handle> --chat-id <id> --chat-guid <guid>\n  channel enable|disable <name>\n  channel test <name>\n  channel detect feishu [--save]\n\nApple Messages always uses explicit iMessage service with SMS fallback disabled.",
        "AskHuman channel —— 受维护渠道：feishu | imessage\n\n  channel list [--json]\n  channel set feishu [--enable|--disable] --app-id <id> --open-id <id> --base-url <url> --app-secret-{env|file|stdin}\n  channel set imessage [--enable|--disable] --recipient <handle> --chat-id <id> --chat-guid <guid>\n  channel enable|disable <渠道>\n  channel test <渠道>\n  channel detect feishu [--save]\n\nApple 信息始终显式使用 iMessage 服务并关闭 SMS 回退。",
    )
}

fn print_line(value: &str) {
    println!("{value}");
}
