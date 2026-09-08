import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function read(relativePath) {
  return readFile(new URL(relativePath, root), "utf8");
}

test("production runtime uses the human-in-loop identity", async () => {
  const [cargo, tauri, installer, help, paths, daemon, daemonRuntime, daemonRequest, cli, prompts, mcpVerify, readme] = await Promise.all([
    read("src-tauri/Cargo.toml"),
    read("src-tauri/tauri.conf.json"),
    read("scripts/install.sh"),
    read("src-tauri/src/cli/help.rs"),
    read("src-tauri/src/paths.rs"),
    read("src-tauri/src/daemon/spawn.rs"),
    read("src-tauri/src/daemon/runtime/mod.rs"),
    read("src-tauri/src/daemon/request.rs"),
    read("src-tauri/src/cli/debug_cmd.rs"),
    read("src-tauri/src/prompts.rs"),
    read("scripts/mcp-verify.mjs"),
    read("README.md"),
  ]);

  assert.match(cargo, /default-run = "human-in-loop"/);
  assert.match(cargo, /name = "human-in-loop"/);
  assert.equal(JSON.parse(tauri).productName, "human-in-loop");
  assert.equal(JSON.parse(tauri).identifier, "io.github.cigit-zgy.human-in-loop");

  assert.match(installer, /BIN_PATH="\$TARGET_ROOT\/\$BUILD_PROFILE\/human-in-loop"/);
  assert.match(installer, /INSTALLED_BIN="\$INSTALL_DIR\/human-in-loop"/);
  assert.match(installer, /INSTALL_STATE="\$INSTALL_DIR\/\.human-in-loop-install-state"/);

  assert.match(help, /format!\("human-in-loop v\{\}"/);
  assert.doesNotMatch(help, /AskHuman/);
  assert.match(paths, /HUMAN_IN_LOOP_HOME_ENV/);
  assert.match(paths, /home\(\)\.join\("\.human-in-loop"\)/);
  assert.match(daemon, /io\.github\.cigit-zgy\.human-in-loop\.daemon/);
  for (const source of [daemonRuntime, daemonRequest, cli]) {
    assert.doesNotMatch(source, /AskHuman daemon|askhuman daemon|askhuman-daemon/);
  }
  assert.doesNotMatch(prompts, /AskHuman[^\n]*MCP|MCP[^\n]*AskHuman/);
  assert.match(help, /exposing ask_human and notify_human/);
  assert.match(mcpVerify, /HUMAN_IN_LOOP_HOME: configDir/);
  assert.doesNotMatch(mcpVerify, /ASKHUMAN_HOME:/);
  assert.match(readme, /installed `human-in-loop` executable with the argument `mcp`/);
});
