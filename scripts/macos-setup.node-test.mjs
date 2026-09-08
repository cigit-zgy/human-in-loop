import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  createQualificationReceipt,
  deriveSetupState,
  isUserLoggedIn,
  qualificationFingerprint,
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
  assert.match(result.stdout, /default Bot user: human-in-loop/);
  assert.doesNotMatch(result.stdout, /--coordinator-user <short-name>/);
});
