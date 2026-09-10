//! `human-in-loop channel` configuration for the maintained iMessage delivery channel.

use super::cfgio;
use crate::config::{AppConfig, IMessageIdentityMode};
use crate::i18n::{err_prefix, Lang};
use std::collections::HashMap;
use std::process::exit;

pub(crate) const CHANNELS: [&str; 1] = ["imessage"];

pub fn dispatch(args: &[String], lang: Lang) {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let rest = &args[args.len().min(1)..];
    let result = match sub {
        "list" | "ls" => list(rest, lang),
        "set" => set(rest, lang),
        "enable" => toggle(rest, true, lang),
        "disable" => toggle(rest, false, lang),
        "test" => test(rest, lang),
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
        let value = args.get(index + 1).ok_or_else(|| {
            cfgio::t(
                lang,
                &format!("{flag} needs a value"),
                &format!("{flag} 需要参数值"),
            )
        })?;
        parsed.values.insert(name.into(), value.clone());
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
    if parsed.enabled.is_none() && parsed.values.is_empty() {
        return Err(cfgio::t(lang, "no settings supplied", "未提供设置项"));
    }
    let mut config = AppConfig::load_without_secrets();
    match name {
        "imessage" => {
            let channel = &mut config.channels.imessage;
            if let Some(enabled) = parsed.enabled {
                channel.enabled = enabled;
            }
            if let Some(value) = parsed.values.remove("recipient") {
                channel.recipient = value;
            }
            if let Some(value) = parsed.values.remove("identity-mode") {
                channel.identity_mode = parse_identity_mode(&value).ok_or_else(|| {
                    "identity-mode must be distinct_peer or same_account".to_string()
                })?;
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
    let mut leftovers: Vec<_> = parsed.values.into_keys().collect();
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
    let config = AppConfig::load_without_secrets();
    match name {
        "imessage" => {
            let health =
                cfgio::block_on(crate::channels::imessage::health(&config.channels.imessage));
            if matches!(
                health,
                crate::channels::imessage::HealthState::Ready
                    | crate::channels::imessage::HealthState::BootstrapRequired
            ) {
                print_line(health.as_str());
                Ok(())
            } else {
                Err(health.as_str().to_string())
            }
        }
        _ => unreachable!(),
    }
}

fn canon(name: &str, lang: Lang) -> Result<&'static str, String> {
    match name.trim().to_ascii_lowercase().as_str() {
        "imessage" | "messages" => Ok("imessage"),
        other => Err(cfgio::t(
            lang,
            &format!("unknown channel: {other} (expected imessage)"),
            &format!("未知渠道: {other}（应为 imessage）"),
        )),
    }
}

pub(crate) fn conn_name(name: &str) -> &'static str {
    match name {
        "imessage" => "imessage",
        _ => "",
    }
}

pub(crate) fn is_enabled(config: &AppConfig, name: &str) -> bool {
    match name {
        "imessage" => config.channels.imessage.enabled,
        _ => false,
    }
}

pub(crate) fn is_configured(config: &AppConfig, name: &str) -> bool {
    match name {
        "imessage" => !config.channels.imessage.recipient.trim().is_empty(),
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
        "human-in-loop channel — maintained channel: imessage\n\n  channel list [--json]\n  channel set imessage [--enable|--disable] --recipient <handle> --identity-mode <distinct_peer|same_account> [--chat-id <id> --chat-guid <guid>]\n  channel enable|disable imessage\n  channel test imessage\n\nApple Messages always uses explicit iMessage service with SMS fallback disabled.",
        "human-in-loop channel —— 受维护渠道：imessage\n\n  channel list [--json]\n  channel set imessage [--enable|--disable] --recipient <handle> --identity-mode <distinct_peer|same_account> [--chat-id <id> --chat-guid <guid>]\n  channel enable|disable imessage\n  channel test imessage\n\nApple 信息始终显式使用 iMessage 服务并关闭 SMS 回退。",
    )
}

fn parse_identity_mode(value: &str) -> Option<IMessageIdentityMode> {
    match value {
        "distinct_peer" => Some(IMessageIdentityMode::DistinctPeer),
        "same_account" => Some(IMessageIdentityMode::SameAccount),
        _ => None,
    }
}

fn print_line(value: &str) {
    println!("{value}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintained_channel_registry_is_imessage_only() {
        assert_eq!(CHANNELS, ["imessage"]);
    }

    #[test]
    fn imessage_is_configured_for_bootstrap_without_a_chat() {
        let mut config = AppConfig::default();
        config.channels.imessage.enabled = true;
        config.channels.imessage.recipient = "person@example.com".into();
        config.channels.imessage.identity_mode = IMessageIdentityMode::SameAccount;

        assert!(is_configured(&config, "imessage"));
    }

    #[test]
    fn identity_mode_parser_accepts_only_the_two_supported_modes() {
        assert_eq!(
            parse_identity_mode("distinct_peer"),
            Some(IMessageIdentityMode::DistinctPeer)
        );
        assert_eq!(
            parse_identity_mode("same_account"),
            Some(IMessageIdentityMode::SameAccount)
        );
        assert_eq!(parse_identity_mode("auto"), None);
    }
}
