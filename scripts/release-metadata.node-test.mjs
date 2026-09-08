import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);
const require = createRequire(import.meta.url);

async function json(path) {
  return JSON.parse(await readFile(new URL(path, root), "utf8"));
}

test("release manifests share one independent project identity and version", async () => {
  const rootPackage = await json("package.json");
  const tauri = await json("src-tauri/tauri.conf.json");
  const npmPackage = await json("packaging/npm/humaninloop/package.json");
  const platforms = await Promise.all([
    "darwin-arm64",
    "darwin-x64",
    "linux-x64",
    "win32-x64",
  ].map((platform) => json(`packaging/npm/platforms/${platform}/package.json`)));
  const cargo = await readFile(new URL("src-tauri/Cargo.toml", root), "utf8");
  const cargoVersion = cargo.match(/^version = "([^"]+)"$/m)?.[1];

  assert.equal(npmPackage.name, "humaninloop");
  assert.equal(npmPackage.bin["human-in-loop"], "bin/cli.js");
  assert.equal(npmPackage.bin.AskHuman, "bin/cli.js");
  assert.doesNotMatch(String(npmPackage.author), /Naituw/i);
  assert.doesNotMatch(cargo.match(/^authors = \[(.*)\]$/m)?.[1] ?? "", /Naituw/i);
  for (const manifest of [rootPackage, tauri, npmPackage, ...platforms]) {
    assert.equal(manifest.version, cargoVersion);
  }
  for (const manifest of platforms) {
    assert.doesNotMatch(String(manifest.author), /Naituw/i);
    assert.equal(npmPackage.optionalDependencies[manifest.name], cargoVersion);
  }
});

test("npm wrapper prefers human-in-loop while retaining an explicit legacy alias", async () => {
  const metadata = await json("packaging/npm/humaninloop/package.json");
  const readme = await readFile(new URL("packaging/npm/humaninloop/README.md", root), "utf8");
  const wrapper = require("../packaging/npm/humaninloop/index.js");
  const previousPrimary = process.env.HUMANINLOOP_BINARY;
  const previousLegacy = process.env.ASKHUMAN_BINARY;

  try {
    process.env.HUMANINLOOP_BINARY = process.execPath;
    process.env.ASKHUMAN_BINARY = "/nonexistent/legacy-binary";
    assert.equal(wrapper.getBinaryPath(), process.execPath);
    delete process.env.HUMANINLOOP_BINARY;
    process.env.ASKHUMAN_BINARY = process.execPath;
    assert.equal(wrapper.getBinaryPath(), process.execPath);
  } finally {
    if (previousPrimary === undefined) delete process.env.HUMANINLOOP_BINARY;
    else process.env.HUMANINLOOP_BINARY = previousPrimary;
    if (previousLegacy === undefined) delete process.env.ASKHUMAN_BINARY;
    else process.env.ASKHUMAN_BINARY = previousLegacy;
  }

  assert.deepEqual(Object.keys(metadata.bin), ["human-in-loop", "AskHuman"]);
  assert.match(readme, /legacy compatibility alias/);
  assert.match(readme, /source release only/i);
});
