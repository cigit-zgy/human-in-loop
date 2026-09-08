import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function read(path) {
  return readFile(new URL(path, root), "utf8");
}

test("macOS production install is stable-signed and identity guarded", async () => {
  const installer = await read("scripts/install.sh");

  assert.match(installer, /human-in-loop Local Code Signing/);
  assert.match(installer, /RUNTIME_IDENTITY_MIGRATION_REQUIRED/);
  assert.match(installer, /codesign --verify --strict/);
  assert.match(installer, /codesign --verify -R=/);
  assert.match(installer, /\.human-in-loop-designated-requirement/);
  assert.match(installer, /\.human-in-loop\.next\./);
  assert.doesNotMatch(installer, /IDENTITY="-"/);
  assert.doesNotMatch(installer, /签名 \(ad-hoc/);
});

test("bounded bootstrap has one sudo authentication and a fixed operation set", async () => {
  const bootstrap = await read("scripts/macos-bootstrap.sh");

  assert.match(bootstrap, /sudo -v/);
  assert.equal(bootstrap.match(/sudo -v/g)?.length, 1);
  assert.doesNotMatch(bootstrap, /sudo -(i|s)/);
  assert.doesNotMatch(bootstrap, /eval /);
  assert.doesNotMatch(bootstrap, /sh -c/);
  assert.doesNotMatch(bootstrap, /bash -c/);
  assert.doesNotMatch(bootstrap, /\$\{@\}/);
  assert.match(bootstrap, /\/Users\/Shared\/human-in-loop\/bin\/human-in-loop/);
  assert.match(bootstrap, /HUMAN_IN_LOOP_ALLOW_IDENTITY_MIGRATION=1/);
  assert.match(bootstrap, /security import/);
  assert.match(bootstrap, /extendedKeyUsage=codeSigning/);
  assert.match(bootstrap, /imessage-worker install/);
});

test("an already prepared shared runtime updates without administrator authentication", async () => {
  const bootstrap = await read("scripts/macos-bootstrap.sh");

  assert.match(bootstrap, /shared_runtime_is_prepared/);
  assert.match(bootstrap, /ROUTINE_UPDATE_WITHOUT_SUDO/);
  assert.match(bootstrap, /if shared_runtime_is_prepared; then/);
  assert.match(bootstrap, /\/usr\/bin\/install -m 0755[\s\S]*\$SHARED_BINARY\.next/);
  assert.match(bootstrap, /\/usr\/bin\/who/);
  assert.doesNotMatch(bootstrap, /launchctl print "gui\/\$BOT_UID"/);
  assert.match(bootstrap, /imessage-worker restart/);
  assert.doesNotMatch(bootstrap, /launchctl kickstart -k/);
});

test("Bot worker launch metadata never receives a repository path", async () => {
  const worker = await read("src-tauri/src/channels/imessage_worker.rs");

  assert.match(worker, /const SHARED_BINARY: &str = "\/Users\/Shared\/human-in-loop\/bin\/human-in-loop"/);
  assert.doesNotMatch(worker, /WorkerRequest[\s\S]*repository_path/);
  assert.doesNotMatch(worker, /launch_agent_plist[\s\S]*Documents/);
});

test("the production binary declares its bounded Messages automation purpose", async () => {
  const plist = await read("src-tauri/Info.plist");

  assert.match(plist, /<key>NSAppleEventsUsageDescription<\/key>/);
  assert.match(plist, /Messages/);
});
