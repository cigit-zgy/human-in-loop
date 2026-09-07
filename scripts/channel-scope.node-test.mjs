import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");
const legacy = ["telegram", "slack", "dingding", "dingtalk"];

test("settings and CLI expose only Feishu and iMessage", async () => {
  const [settings, search, channelCli, packageReadme, packageMetadata, en, zh] = await Promise.all([
    read("src/views/settings/ChannelsTab.vue"),
    read("src/views/settings/useSearch.ts"),
    read("src-tauri/src/cli/channel_cmd.rs"),
    read("packaging/npm/humaninloop/README.md"),
    read("packaging/npm/humaninloop/package.json"),
    read("src/i18n/en.ts"),
    read("src/i18n/zh.ts"),
  ]);
  for (const source of [settings, search, channelCli, packageReadme, packageMetadata]) {
    for (const name of legacy) {
      assert.equal(source.toLowerCase().includes(name), false, `${name} leaked into a maintained channel surface`);
    }
  }
  assert.equal(settings.toLowerCase().includes("popup"), false);
  assert.match(settings, /channels\.feishu/);
  assert.match(settings, /channels\.imessage/);
  assert.match(channelCli, /\["feishu", "imessage"\]/);
  for (const locale of [en, zh]) {
    const start = locale.indexOf("    channels: {");
    const end = locale.indexOf("    history: {", start);
    const channelCopy = locale.slice(start, end).toLowerCase();
    for (const name of legacy) assert.equal(channelCopy.includes(name), false);
  }
});

test("desktop command registry does not expose legacy channel operations", async () => {
  const invoke = await read("src-tauri/src/app/invoke.rs");
  const start = invoke.indexOf("fn channel(");
  const end = invoke.indexOf("fn history(", start);
  const registry = invoke.slice(start, end).toLowerCase();
  assert.match(registry, /feishu_test/);
  for (const name of legacy) assert.equal(registry.includes(name), false);
});

test("confirmation registry selects only maintained channels", async () => {
  const runtime = await read("src-tauri/src/daemon/runtime/mod.rs");
  const start = runtime.indexOf("fn available_im_channels");
  const end = runtime.indexOf("fn select_im_delivery_candidates", start);
  const registry = runtime.slice(start, end).toLowerCase();
  assert.match(registry, /feishu/);
  assert.match(registry, /imessage/);
  for (const name of legacy) assert.equal(registry.includes(name), false);
});

test("iMessage send builder is explicit and fail-closed", async () => {
  const adapter = await read("src-tauri/src/channels/imessage.rs");
  const start = adapter.indexOf("pub fn direct_send_args");
  const end = adapter.indexOf("pub fn watch_args", start);
  const builder = adapter.slice(start, end).toLowerCase();
  assert.match(builder, /"--service"/);
  assert.match(builder, /"imessage"/);
  assert.match(builder, /"--no-sms-fallback"/);
  assert.equal(builder.includes('"auto"'), false);
  assert.equal(builder.includes('"sms"'), false);
});
