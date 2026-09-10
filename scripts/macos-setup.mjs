#!/usr/bin/env node
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { Client } from "./mcp-client.mjs";

const SHARED_BINARY = "/Users/Shared/human-in-loop/bin/human-in-loop";
const RECEIPT_NAME = "setup-qualification.json";
const AUTOLOGIN_PREFERENCE_NAME = "bot-autologin.json";
const CONFIGURED_BOT_USER = "human-in-loop";

export const AUTOLOGIN_STATES = Object.freeze({
  supported: "autologin_supported",
  alreadyEnabledForBot: "autologin_already_enabled_for_bot",
  disabledByUser: "autologin_disabled_by_user",
  unavailableFileVault: "autologin_unavailable_filevault",
  unavailableManagedPolicy: "autologin_unavailable_managed_policy",
  unavailableAccountType: "autologin_unavailable_account_type",
  nativeAuthRequired: "autologin_native_auth_required",
  configurationFailed: "autologin_configuration_failed",
});

export function qualificationFingerprint(snapshot) {
  const material = {
    botUserName: snapshot.botUserName,
    identityMode: snapshot.identityMode,
    recipient: snapshot.recipient.trim().toLowerCase(),
    chatId: snapshot.chatId ?? null,
    chatGuid: snapshot.chatGuid ?? "",
    designatedRequirement: snapshot.designatedRequirement,
  };
  return createHash("sha256").update(JSON.stringify(material)).digest("hex");
}

export function createQualificationReceipt(snapshot) {
  return {
    version: 1,
    fingerprint: qualificationFingerprint(snapshot),
    notificationPresentation: "HUMAN_VERIFIED",
    replyCorrelation: "VERIFIED",
  };
}

export function isUserLoggedIn(roster, username) {
  return roster.split("\n").some((line) => line.trim().split(/\s+/, 1)[0] === username);
}

export function deriveSetupState(snapshot, receipt) {
  const transportReady = ["ready", "bootstrap_required"].includes(snapshot.workerHealth);
  const automationReady = snapshot.automation === "automation_ready";
  const routeReady = snapshot.imessageOnlyRoute
    && ["ready", "bootstrap_required"].includes(snapshot.channelHealth);
  const qualified = receipt?.version === 1
    && receipt.fingerprint === qualificationFingerprint(snapshot)
    && receipt.notificationPresentation === "HUMAN_VERIFIED"
    && receipt.replyCorrelation === "VERIFIED";
  const rows = [
    { label: "Stable runtime identity", status: snapshot.stableRuntimeIdentity ? "READY" : "NOT_READY" },
    { label: "Bot macOS user", status: snapshot.botUser ? "READY" : "NOT_READY" },
    { label: "Bot session", status: snapshot.botSession ? "READY" : "NOT_READY" },
    { label: "Bot Messages / iMessage", status: transportReady ? "READY" : "NOT_READY" },
    { label: "Distinct sender/recipient", status: transportReady ? "READY" : "NOT_READY" },
    { label: "Messages DB / Full Disk Access", status: transportReady ? "READY" : "NOT_READY" },
    { label: "Automation → Messages", status: automationReady ? "READY" : "NOT_READY" },
    { label: "iMessage-only route", status: routeReady ? "READY" : "NOT_READY" },
    { label: "Notification qualification", status: qualified ? "HUMAN_VERIFIED" : "REQUIRED" },
    { label: "Reply/correlation", status: qualified ? "VERIFIED" : "REQUIRED" },
  ];
  let state = "SETUP_COMPLETE";
  if (!snapshot.stableRuntimeIdentity) state = "SETUP_NEEDS_RUNTIME_BOOTSTRAP";
  else if (!snapshot.botUser) state = "SETUP_NEEDS_BOT_USER";
  else if (!snapshot.botSession || snapshot.workerHealth === "BOT_SESSION_LOGIN_REQUIRED") {
    state = "BOT_SESSION_LOGIN_REQUIRED";
  } else if (!automationReady) state = snapshot.automation === "automation_consent_required"
    ? "SETUP_NEEDS_TCC_CONSENT" : "RECOVERY_REQUIRED";
  else if (!transportReady || !routeReady) state = "RECOVERY_REQUIRED";
  else if (!qualified) state = "SETUP_NEEDS_NOTIFICATION_QUALIFICATION";
  return { state, complete: state === "SETUP_COMPLETE", rows };
}

function run(program, args) {
  return spawnSync(program, args, {
    encoding: "utf8",
    env: process.env,
    timeout: 10_000,
    killSignal: "SIGKILL",
  });
}

function output(result) {
  return `${result.stdout ?? ""}\n${result.stderr ?? ""}`.trim();
}

function commandValue(program, args) {
  const result = run(program, args);
  return result.status === 0 ? output(result).split("\n").at(-1)?.trim() ?? "" : "";
}

function receiptPath(configPath) {
  return path.join(path.dirname(configPath), RECEIPT_NAME);
}

function autologinPreferencePath(configPath) {
  return path.join(path.dirname(configPath), AUTOLOGIN_PREFERENCE_NAME);
}

function readReceipt(configPath) {
  const file = receiptPath(configPath);
  try {
    if ((fs.statSync(file).mode & 0o077) !== 0) return undefined;
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return undefined;
  }
}

function writeReceipt(configPath, receipt) {
  writePrivateJson(receiptPath(configPath), receipt);
}

function writePrivateJson(file, value) {
  const temporary = `${file}.next.${process.pid}`;
  try {
    fs.writeFileSync(temporary, `${JSON.stringify(value)}\n`, { mode: 0o600, flag: "wx" });
    fs.renameSync(temporary, file);
    fs.chmodSync(file, 0o600);
  } finally {
    if (fs.existsSync(temporary)) fs.unlinkSync(temporary);
  }
}

function readAutologinPreference(configPath) {
  try {
    const file = autologinPreferencePath(configPath);
    if ((fs.statSync(file).mode & 0o077) !== 0) return undefined;
    const value = JSON.parse(fs.readFileSync(file, "utf8"));
    if (value?.version !== 1 || !["manual", "automatic"].includes(value.mode)) return undefined;
    if (value.botUserName !== CONFIGURED_BOT_USER) return undefined;
    return value;
  } catch {
    return undefined;
  }
}

function writeAutologinPreference(configPath, value) {
  writePrivateJson(autologinPreferencePath(configPath), value);
}

export function classifyAutologin(snapshot, preference) {
  if (!snapshot.configuredBot
    || !snapshot.botExists
    || snapshot.botIsAdmin
    || !snapshot.botIsLocal
    || snapshot.botUserName === "root"
    || snapshot.botUserName === snapshot.currentUserName
    || (snapshot.enabledUser && snapshot.enabledUser !== snapshot.botUserName)) {
    return AUTOLOGIN_STATES.unavailableAccountType;
  }
  if (snapshot.fileVault === "on") return AUTOLOGIN_STATES.unavailableFileVault;
  if (snapshot.managedPolicy) return AUTOLOGIN_STATES.unavailableManagedPolicy;
  if (snapshot.fileVault !== "off") return AUTOLOGIN_STATES.configurationFailed;
  if (preference?.mode === "manual" && preference.botUserName === snapshot.botUserName) {
    return snapshot.enabledUser
      ? AUTOLOGIN_STATES.configurationFailed
      : AUTOLOGIN_STATES.disabledByUser;
  }
  if (snapshot.enabledUser === snapshot.botUserName) return AUTOLOGIN_STATES.alreadyEnabledForBot;
  if (preference?.mode === "automatic" && preference.botUserName === snapshot.botUserName) {
    return AUTOLOGIN_STATES.configurationFailed;
  }
  return AUTOLOGIN_STATES.supported;
}

export async function runAutologinFlow({ snapshot, preference, decide, persist, openSettings }) {
  const state = classifyAutologin(snapshot, preference);
  if (state === AUTOLOGIN_STATES.alreadyEnabledForBot
    || state === AUTOLOGIN_STATES.disabledByUser
    || state.startsWith("autologin_unavailable_")) {
    return { state };
  }
  if (state === AUTOLOGIN_STATES.configurationFailed && preference?.mode !== "automatic") {
    return { state };
  }

  let mode = preference?.mode;
  if (!mode) {
    const choice = await decide();
    if (choice === "manual_bot_login") {
      persist({ version: 1, mode: "manual", botUserName: snapshot.botUserName });
      return { state: AUTOLOGIN_STATES.disabledByUser };
    }
    if (choice !== "enable_bot_autologin") {
      return { state: AUTOLOGIN_STATES.configurationFailed };
    }
    mode = "automatic";
    persist({ version: 1, mode, botUserName: snapshot.botUserName });
  }

  return {
    state: openSettings() ? AUTOLOGIN_STATES.nativeAuthRequired : AUTOLOGIN_STATES.configurationFailed,
  };
}

export function openAutologinSettings(runCommand = run) {
  return runCommand("/usr/bin/open", [
    "/System/Library/PreferencePanes/Accounts.prefPane",
  ]).status === 0;
}

export function collectAutologinSnapshot({ botUserName }, runCommand = run) {
  const value = (program, args) => {
    const result = runCommand(program, args);
    return result.status === 0 ? output(result).split("\n").at(-1)?.trim() ?? "" : "";
  };
  const currentUserName = value("/usr/bin/id", ["-un"]);
  const botIdentity = runCommand("/usr/bin/id", ["-u", botUserName]);
  const botGroups = value("/usr/bin/id", ["-Gn", botUserName]).split(/\s+/).filter(Boolean);
  const localAccount = runCommand("/usr/bin/dscl", [
    "localhost",
    "-read",
    `/Local/Default/Users/${botUserName}`,
    "NFSHomeDirectory",
  ]);
  const botHome = localAccount.status === 0
    ? output(localAccount).split(/\s+/).at(-1) ?? ""
    : "";
  const fileVaultValue = value("/usr/bin/fdesetup", ["isactive"]);
  const managedLoginWindow = runCommand("/usr/bin/defaults", [
    "read",
    "/Library/Managed Preferences/com.apple.loginwindow",
  ]);
  return {
    configuredBot: botUserName === CONFIGURED_BOT_USER,
    botUserName,
    currentUserName,
    botExists: botIdentity.status === 0,
    botIsAdmin: botGroups.includes("admin"),
    botIsLocal: botIdentity.status === 0 && localAccount.status === 0 && botHome.startsWith("/Users/"),
    fileVault: fileVaultValue === "true" ? "on" : fileVaultValue === "false" ? "off" : "unknown",
    managedPolicy: managedLoginWindow.status === 0,
    enabledUser: value("/usr/bin/defaults", [
      "read",
      "/Library/Preferences/com.apple.loginwindow",
      "autoLoginUser",
    ]),
  };
}

async function decideAutologin(options) {
  const client = new Client({ binary: options.binary, cwd: options.repository, env: process.env });
  try {
    await client.initialize();
    const request = client.ask({
      repository_path: options.repository,
      source_agent: "human-in-loop setup",
      question: "Enable automatic login for the dedicated human-in-loop Bot user after Mac restart?",
      detail: "This reduces post-reboot manual work, but anyone with physical access after restart may enter the Bot session. It applies only to the dedicated non-admin Bot user. human-in-loop will not disable FileVault, SIP, or TCC, and never captures macOS or Apple Account credentials.",
      choices: [
        { id: "enable_bot_autologin", label: "Enable automatic login" },
        { id: "manual_bot_login", label: "Keep manual Bot login" },
      ],
      context: "Optional post-reboot Bot session convenience",
    });
    const response = await client.response(request, 15 * 60 * 1000);
    assert(!response.error && response.result?.isError !== true, "automatic-login decision failed");
    return response.result.structuredContent.selected_choice_id;
  } finally {
    await client.close("SIGTERM");
  }
}

async function configureAutologin(options) {
  const configPath = commandValue(options.binary, ["config", "path"]);
  if (!path.isAbsolute(configPath)) throw new Error("canonical configuration path is unavailable");
  const snapshot = collectAutologinSnapshot(options);
  const result = await runAutologinFlow({
    snapshot,
    preference: readAutologinPreference(configPath),
    decide: () => decideAutologin(options),
    persist: (value) => writeAutologinPreference(configPath, value),
    openSettings: () => openAutologinSettings(),
  });
  console.log(`Bot automatic login ${result.state}`);
  if (result.state === AUTOLOGIN_STATES.nativeAuthRequired) {
    console.error("USER_CHECKPOINT: AUTOLOGIN_NATIVE_AUTH_REQUIRED");
    console.error("In System Settings → Users & Groups, select the dedicated human-in-loop Bot for automatic login and enter the required password only in the native macOS interface. Then rerun this command.");
    process.exitCode = 3;
  } else if (result.state === AUTOLOGIN_STATES.configurationFailed) {
    process.exitCode = 1;
  }
  return result;
}

function collectSnapshot({ binary, botUserName }) {
  const configPath = commandValue(binary, ["config", "path"]);
  const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
  const imessage = config.channels?.imessage ?? {};
  const verify = run("/usr/bin/codesign", ["--verify", "--strict", SHARED_BINARY]);
  const requirementResult = run("/usr/bin/codesign", ["-d", "-r-", SHARED_BINARY]);
  const requirement = output(requirementResult).split("\n")
    .find((line) => line.startsWith("designated => "))?.slice("designated => ".length) ?? "";
  const uidResult = run("/usr/bin/id", ["-u", botUserName]);
  const botUid = uidResult.status === 0 ? uidResult.stdout.trim() : "";
  const sessionResult = run("/usr/bin/who", []);
  const sessionRoster = sessionResult.status === 0 ? sessionResult.stdout : "";
  const workerHealth = commandValue(binary, ["imessage-worker", "status"]);
  const automation = commandValue(binary, ["imessage-worker", "automation", "status"]);
  const channelHealth = commandValue(binary, ["channel", "test", "imessage"]);
  return {
    configPath,
    stableRuntimeIdentity: verify.status === 0 && requirement.length > 0 && !requirement.includes("cdhash"),
    botUser: uidResult.status === 0,
    botSession: botUid.length > 0 && isUserLoggedIn(sessionRoster, botUserName),
    workerHealth,
    automation,
    channelHealth,
    imessageOnlyRoute: imessage.identityMode === "distinct_peer",
    botUserName,
    identityMode: imessage.identityMode ?? "",
    recipient: imessage.recipient ?? "",
    chatId: imessage.chatId ?? null,
    chatGuid: imessage.chatGuid ?? "",
    designatedRequirement: requirement,
  };
}

function printState(result) {
  for (const row of result.rows) console.log(`${row.label.padEnd(36)} ${row.status}`);
  console.log(`\nSetup ${result.complete ? "COMPLETE" : result.state}`);
}

function parseArgs(argv) {
  const options = { action: argv[0], binary: SHARED_BINARY, botUserName: "human-in-loop", repository: "" };
  for (let index = 1; index < argv.length; index += 2) {
    const value = argv[index + 1];
    if (!value) throw new Error(`missing value for ${argv[index]}`);
    if (argv[index] === "--binary") options.binary = value;
    else if (argv[index] === "--bot-user") options.botUserName = value;
    else if (argv[index] === "--repository") options.repository = value;
    else throw new Error(`unknown option: ${argv[index]}`);
  }
  if (!["status", "qualify", "autologin"].includes(options.action)) throw new Error("usage: macos-setup.mjs <status|qualify|autologin> [options]");
  if (["qualify", "autologin"].includes(options.action) && !path.isAbsolute(options.repository)) {
    throw new Error(`${options.action} requires an absolute --repository path`);
  }
  return options;
}

async function qualify(options, snapshot) {
  const initial = deriveSetupState(snapshot, readReceipt(snapshot.configPath));
  if (initial.complete) return initial;
  if (initial.rows.slice(0, 8).some((row) => row.status !== "READY")) return initial;

  console.log("Phone qualification required: lock the iPhone or keep Messages out of foreground, then reply using the generated token and option number.");
  const client = new Client({ binary: options.binary, cwd: options.repository, env: process.env });
  try {
    await client.initialize();
    const list = await client.response(client.request("tools/list"));
    assert.deepEqual(list.result.tools.map((tool) => tool.name).sort(), ["ask_human", "notify_human"]);
    const request = client.ask({
      repository_path: options.repository,
      source_agent: "human-in-loop setup",
      question: "Did this setup notification appear correctly on the locked or non-foreground iPhone?",
      choices: [
        { id: "received_correctly", label: "Received correctly" },
        { id: "failed", label: "Failed" },
      ],
      recommended_choice: "received_correctly",
      context: "Initial macOS notification and reply qualification",
    });
    const response = await client.response(request, 15 * 60 * 1000);
    assert(!response.error && !response.result?.isError, "qualification request failed");
    const result = response.result.structuredContent;
    assert.deepEqual(Object.keys(result).sort(), ["request_id", "selected_choice_id", "source_channel_id"]);
    assert.equal(result.selected_choice_id, "received_correctly");
    assert.equal(result.source_channel_id, "imessage");
    assert.equal(typeof result.request_id, "string");
    assert(result.request_id.length > 0);
  } finally {
    await client.close("SIGTERM");
  }
  const current = collectSnapshot(options);
  writeReceipt(current.configPath, createQualificationReceipt(current));
  return deriveSetupState(current, readReceipt(current.configPath));
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.action === "autologin") {
    await configureAutologin(options);
    return;
  }
  const snapshot = collectSnapshot(options);
  const result = options.action === "qualify"
    ? await qualify(options, snapshot)
    : deriveSetupState(snapshot, readReceipt(snapshot.configPath));
  printState(result);
  if (!result.complete) process.exitCode = 3;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(`Setup failed: ${error.message}`);
    process.exitCode = 1;
  });
}
