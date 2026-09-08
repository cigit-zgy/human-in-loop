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
  const file = receiptPath(configPath);
  const temporary = `${file}.next.${process.pid}`;
  try {
    fs.writeFileSync(temporary, `${JSON.stringify(receipt)}\n`, { mode: 0o600, flag: "wx" });
    fs.renameSync(temporary, file);
    fs.chmodSync(file, 0o600);
  } finally {
    if (fs.existsSync(temporary)) fs.unlinkSync(temporary);
  }
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
  if (!["status", "qualify"].includes(options.action)) throw new Error("usage: macos-setup.mjs <status|qualify> [options]");
  if (options.action === "qualify" && !path.isAbsolute(options.repository)) {
    throw new Error("qualification requires an absolute --repository path");
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
