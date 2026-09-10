import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  AUTOLOGIN_STATES,
  classifyAutologin,
  collectAutologinSnapshot,
  createQualificationReceipt,
  deriveSetupState,
  isUserLoggedIn,
  qualificationFingerprint,
  openAutologinSettings,
  runAutologinFlow,
} from "./macos-setup.mjs";

const scripts = fileURLToPath(new URL("./", import.meta.url));

const ready = {
  stableRuntimeIdentity: true,
  botUser: true,
  botSession: true,
  workerHealth: "ready",
  automation: "automation_ready",
  channelHealth: "ready",
  imessageOnlyRoute: true,
  botUserName: "human-in-loop",
  identityMode: "distinct_peer",
  recipient: "person@example.invalid",
  chatId: 42,
  chatGuid: "iMessage;-;person@example.invalid",
  designatedRequirement: 'identifier "io.github.cigit-zgy.human-in-loop" and anchor trusted',
};

test("setup is complete only while current health matches a verified qualification", () => {
  const receipt = createQualificationReceipt(ready);
  const result = deriveSetupState(ready, receipt);

  assert.equal(result.state, "SETUP_COMPLETE");
  assert.equal(result.complete, true);
  assert.deepEqual(
    result.rows.map(({ label, status }) => [label, status]),
    [
      ["Stable runtime identity", "READY"],
      ["Bot macOS user", "READY"],
      ["Bot session", "READY"],
      ["Bot Messages / iMessage", "READY"],
      ["Distinct sender/recipient", "READY"],
      ["Messages DB / Full Disk Access", "READY"],
      ["Automation → Messages", "READY"],
      ["iMessage-only route", "READY"],
      ["Notification qualification", "HUMAN_VERIFIED"],
      ["Reply/correlation", "VERIFIED"],
    ],
  );
});

test("a stale or missing qualification cannot become setup complete", () => {
  const receipt = createQualificationReceipt(ready);
  const changed = { ...ready, recipient: "changed@example.invalid" };

  assert.equal(deriveSetupState(ready, undefined).state, "SETUP_NEEDS_NOTIFICATION_QUALIFICATION");
  assert.equal(deriveSetupState(changed, receipt).state, "SETUP_NEEDS_NOTIFICATION_QUALIFICATION");
  assert.notEqual(qualificationFingerprint(ready), qualificationFingerprint(changed));
});

test("current host regression selects one owning recovery state", () => {
  assert.equal(
    deriveSetupState({ ...ready, botSession: false }, undefined).state,
    "BOT_SESSION_LOGIN_REQUIRED",
  );
  assert.equal(
    deriveSetupState({ ...ready, automation: "automation_consent_required" }, undefined).state,
    "SETUP_NEEDS_TCC_CONSENT",
  );
  assert.equal(
    deriveSetupState({ ...ready, workerHealth: "SELF_MESSAGE_UNSUPPORTED" }, undefined).state,
    "RECOVERY_REQUIRED",
  );
});

test("Bot login detection uses the session roster, not cross-user launchctl access", () => {
  const roster = "primary console Sep 8 09:00\nhuman-in-loop console Sep 8 14:03\n";

  assert.equal(isUserLoggedIn(roster, "human-in-loop"), true);
  assert.equal(isUserLoggedIn(roster, "other-bot"), false);
  assert.equal(isUserLoggedIn("human-in-loop-extra console Sep 8 14:03\n", "human-in-loop"), false);
});

test("the qualification receipt persists only redacted structural evidence", () => {
  const receipt = createQualificationReceipt(ready);
  const serialized = JSON.stringify(receipt);

  assert.deepEqual(Object.keys(receipt).sort(), [
    "fingerprint",
    "notificationPresentation",
    "replyCorrelation",
    "version",
  ]);
  assert.equal(receipt.notificationPresentation, "HUMAN_VERIFIED");
  assert.equal(receipt.replyCorrelation, "VERIFIED");
  assert(!serialized.includes(ready.recipient));
  assert(!serialized.includes(ready.chatGuid));
  assert(!serialized.includes(ready.designatedRequirement));
});

test("the public bootstrap defaults to the current primary user", () => {
  const result = spawnSync("bash", [`${scripts}/macos-bootstrap.sh`, "--help"], {
    encoding: "utf8",
  });

  assert.equal(result.status, 0);
  assert.match(result.stdout, /Usage: .*macos-bootstrap\.sh/);
  assert.match(result.stdout, /dedicated standard Bot user \(default: human-in-loop\)/);
  assert.match(result.stdout, /one setup flow/i);
  assert.doesNotMatch(result.stdout, /coordinator-user|MCP HOME|worker socket|LaunchAgent|chat GUID|imsg flags/);
});

const autologinReady = {
  configuredBot: true,
  botUserName: "human-in-loop",
  currentUserName: "wenv",
  botExists: true,
  botIsAdmin: false,
  botIsLocal: true,
  fileVault: "off",
  managedPolicy: false,
  enabledUser: "",
};

test("automatic-login preflight classifies every required host state without mutation", () => {
  assert.equal(classifyAutologin(autologinReady), AUTOLOGIN_STATES.supported);
  assert.equal(
    classifyAutologin({ ...autologinReady, enabledUser: "human-in-loop" }),
    AUTOLOGIN_STATES.alreadyEnabledForBot,
  );
  assert.equal(
    classifyAutologin(autologinReady, { version: 1, mode: "manual", botUserName: "human-in-loop" }),
    AUTOLOGIN_STATES.disabledByUser,
  );
  assert.equal(
    classifyAutologin({ ...autologinReady, fileVault: "on" }),
    AUTOLOGIN_STATES.unavailableFileVault,
  );
  assert.equal(
    classifyAutologin({ ...autologinReady, managedPolicy: true }),
    AUTOLOGIN_STATES.unavailableManagedPolicy,
  );
  for (const snapshot of [
    { ...autologinReady, configuredBot: false },
    { ...autologinReady, botIsAdmin: true },
    { ...autologinReady, botIsLocal: false },
    { ...autologinReady, botUserName: "root" },
    { ...autologinReady, botUserName: "wenv" },
  ]) {
    assert.equal(classifyAutologin(snapshot), AUTOLOGIN_STATES.unavailableAccountType);
  }
  assert.equal(
    classifyAutologin(
      autologinReady,
      { version: 1, mode: "automatic", botUserName: "human-in-loop" },
    ),
    AUTOLOGIN_STATES.configurationFailed,
  );
  assert.equal(
    classifyAutologin(
      { ...autologinReady, enabledUser: "human-in-loop" },
      { version: 1, mode: "manual", botUserName: "human-in-loop" },
    ),
    AUTOLOGIN_STATES.configurationFailed,
  );
});

test("unsupported hosts never ask, open settings, persist a choice, or weaken FileVault", async () => {
  const calls = [];
  const result = await runAutologinFlow({
    snapshot: { ...autologinReady, fileVault: "on" },
    preference: undefined,
    decide: async () => { calls.push("decide"); return "enable_bot_autologin"; },
    persist: () => calls.push("persist"),
    openSettings: () => { calls.push("open"); return true; },
  });

  assert.equal(result.state, AUTOLOGIN_STATES.unavailableFileVault);
  assert.deepEqual(calls, []);
});

test("manual choice is persisted once and leaves post-reboot login manual", async () => {
  const persisted = [];
  const result = await runAutologinFlow({
    snapshot: autologinReady,
    preference: undefined,
    decide: async () => "manual_bot_login",
    persist: (value) => persisted.push(value),
    openSettings: () => assert.fail("manual choice must not open System Settings"),
  });

  assert.equal(result.state, AUTOLOGIN_STATES.disabledByUser);
  assert.deepEqual(persisted, [{ version: 1, mode: "manual", botUserName: "human-in-loop" }]);
});

test("enable choice uses only native System Settings and is not asked again after persistence", async () => {
  const persisted = [];
  let asks = 0;
  let opens = 0;
  const first = await runAutologinFlow({
    snapshot: autologinReady,
    preference: undefined,
    decide: async () => { asks += 1; return "enable_bot_autologin"; },
    persist: (value) => persisted.push(value),
    openSettings: () => { opens += 1; return true; },
  });
  const second = await runAutologinFlow({
    snapshot: autologinReady,
    preference: persisted[0],
    decide: async () => { asks += 1; return "manual_bot_login"; },
    persist: (value) => persisted.push(value),
    openSettings: () => { opens += 1; return true; },
  });

  assert.equal(first.state, AUTOLOGIN_STATES.nativeAuthRequired);
  assert.equal(second.state, AUTOLOGIN_STATES.nativeAuthRequired);
  assert.equal(asks, 1);
  assert.equal(opens, 2);
  assert.deepEqual(persisted[0], { version: 1, mode: "automatic", botUserName: "human-in-loop" });
});

test("verified Bot automatic login is idempotent and needs no decision or UI", async () => {
  const calls = [];
  const result = await runAutologinFlow({
    snapshot: { ...autologinReady, enabledUser: "human-in-loop" },
    preference: { version: 1, mode: "automatic", botUserName: "human-in-loop" },
    decide: async () => { calls.push("decide"); return "enable_bot_autologin"; },
    persist: () => calls.push("persist"),
    openSettings: () => { calls.push("open"); return true; },
  });

  assert.equal(result.state, AUTOLOGIN_STATES.alreadyEnabledForBot);
  assert.deepEqual(calls, []);
});

test("native authentication navigation opens only the public Users & Groups preference pane", () => {
  const calls = [];
  const opened = openAutologinSettings((program, args) => {
    calls.push([program, args]);
    return { status: 0 };
  });

  assert.equal(opened, true);
  assert.deepEqual(calls, [[
    "/usr/bin/open",
    ["/System/Library/PreferencePanes/Accounts.prefPane"],
  ]]);
});

test("local Bot detection uses the local directory node instead of a missing-attribute exit code", () => {
  const fakeRun = (program, args) => {
    const key = `${program} ${args.join(" ")}`;
    const values = new Map([
      ["/usr/bin/id -un", { status: 0, stdout: "wenv\n" }],
      ["/usr/bin/id -u human-in-loop", { status: 0, stdout: "502\n" }],
      ["/usr/bin/id -Gn human-in-loop", { status: 0, stdout: "staff everyone localaccounts\n" }],
      ["/usr/bin/dscl localhost -read /Local/Default/Users/human-in-loop NFSHomeDirectory", { status: 0, stdout: "NFSHomeDirectory: /Users/human-in-loop\n" }],
      ["/usr/bin/fdesetup isactive", { status: 0, stdout: "true\n" }],
      ["/usr/bin/defaults read /Library/Managed Preferences/com.apple.loginwindow", { status: 1, stdout: "" }],
      ["/usr/bin/defaults read /Library/Preferences/com.apple.loginwindow autoLoginUser", { status: 1, stdout: "" }],
    ]);
    return { stdout: "", stderr: "", ...values.get(key) };
  };

  const snapshot = collectAutologinSnapshot({ botUserName: "human-in-loop" }, fakeRun);

  assert.equal(snapshot.botIsLocal, true);
  assert.equal(classifyAutologin(snapshot), AUTOLOGIN_STATES.unavailableFileVault);
});
