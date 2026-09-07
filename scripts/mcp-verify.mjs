#!/usr/bin/env node
// Actual stdio MCP client → installed binary → production daemon/coordinator → synthetic imsg.
// Usage: node scripts/mcp-verify.mjs /absolute/path/to/installed/AskHuman
// No test path can reach the real imsg executable or the user's channel configuration.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import readline from 'node:readline';
import { fileURLToPath } from 'node:url';

const repo = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const binary = fs.realpathSync(process.argv[2] ?? '');
const scratch = path.join(repo, 'tmp', 'HUMAN_IN_LOOP_MCP_05');
fs.mkdirSync(scratch, { recursive: true });
const root = fs.mkdtempSync(path.join(scratch, 'protocol-'));
const configDir = path.join(root, 'c');
const binDir = path.join(root, 'bin');
const tempDir = path.join(root, 'temp');
for (const dir of [configDir, binDir, tempDir]) fs.mkdirSync(dir);
const helper = path.join(repo, 'scripts', 'mcp-synthetic-imsg.mjs');
fs.copyFileSync(helper, path.join(binDir, 'imsg'));
fs.chmodSync(path.join(binDir, 'imsg'), 0o755);
fs.writeFileSync(path.join(configDir, 'config.json'), JSON.stringify({
  general: { language: 'en', menuBarIcon: 'off', popupPrewarm: false, historyLimit: 0 },
  channels: { autoActivation: false, feishu: { enabled: false },
    imessage: { enabled: true, recipient: 'synthetic@example.invalid', identityMode: 'same_account', chatId: 42, chatGuid: 'synthetic-direct-chat' } },
}));
const env = { ...process.env, ASKHUMAN_HOME: configDir, ASKHUMAN_NO_KEYCHAIN: '1',
  ASKHUMAN_MCP_VERIFY_DIR: root, TMPDIR: tempDir,
  PATH: `${binDir}:${path.dirname(process.execPath)}:/usr/bin:/bin:/usr/sbin:/sbin` };
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const lines = (file) => fs.existsSync(path.join(root, file))
  ? fs.readFileSync(path.join(root, file), 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
const setMode = (mode) => fs.writeFileSync(path.join(root, 'control.json'), JSON.stringify({ mode }));
const alive = (pid) => { try { process.kill(pid, 0); return true; } catch { return false; } };
const sends = () => lines('events.jsonl').filter((event) => event.kind === 'send').length;
const ready = () => lines('events.jsonl').filter((event) => event.kind === 'ready');
const fakePids = () => [...new Set(lines('events.jsonl').filter((event) => event.kind === 'spawn').map((event) => event.pid))];
const liveFakePids = () => fakePids().filter(alive);
const checks = [];
const check = (name, evidence = {}) => { checks.push({ name, ...evidence }); console.log(`PASS ${name}`); };
async function until(predicate, label, ms = 10000) {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (await predicate()) return; await pause(25); }
  throw new Error(`Timed out: ${label}`);
}
function status() {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection(path.join(configDir, 'daemon.sock'));
    socket.setTimeout(1000, () => socket.destroy(new Error('status timeout')));
    socket.once('error', reject);
    socket.once('connect', () => socket.write('{"type":"status"}\n'));
    let data = '';
    socket.on('data', (chunk) => {
      data += chunk;
      if (data.includes('\n')) { socket.end(); resolve(JSON.parse(data.split('\n')[0])); }
    });
  });
}
async function quiescent() {
  await until(async () => (await status()).activeRequests === 0 && liveFakePids().length === 0, 'no active request or synthetic process');
  await pause(100);
  assert.equal((await status()).activeRequests, 0);
  assert.deepEqual(liveFakePids(), []);
}
class Client {
  constructor() {
    this.child = spawn(binary, ['mcp'], { cwd: root, env, stdio: ['pipe', 'pipe', 'pipe'] });
    this.pending = new Map();
    this.responses = [];
    this.stderr = '';
    this.nextId = 0;
    this.closed = new Promise((resolve) => this.child.once('exit', (code, signal) => resolve({ code, signal })));
    this.child.stderr.on('data', (chunk) => { this.stderr += chunk; });
    readline.createInterface({ input: this.child.stdout }).on('line', (line) => {
      let message;
      try { message = JSON.parse(line); } catch {
        for (const resolve of this.pending.values()) resolve({ error: { message: 'Non-protocol stdout' } });
        this.pending.clear();
        return;
      }
      this.responses.push(message);
      const pending = this.pending.get(message.id);
      if (pending) { this.pending.delete(message.id); pending(message); }
    });
    this.child.stdin.on('error', () => {});
  }
  send(message) { this.child.stdin.write(`${JSON.stringify(message)}\n`); }
  request(method, params) {
    const id = ++this.nextId;
    const promise = new Promise((resolve) => this.pending.set(id, resolve));
    this.send({ jsonrpc: '2.0', id, method, ...(params === undefined ? {} : { params }) });
    return { id, promise };
  }
  async response(request, ms = 10000) {
    let timer;
    try { return await Promise.race([request.promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('MCP response timeout')), ms);
    })]); } finally { clearTimeout(timer); }
  }
  async initialize() {
    const response = await this.response(this.request('initialize', {
      protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'human-in-loop-release-verifier', version: '1' },
    }));
    assert.equal(response.result.protocolVersion, '2025-11-25');
    assert.equal(response.result.serverInfo.name, 'human-in-loop');
    assert.equal(typeof response.result.capabilities.tools, 'object');
    this.send({ jsonrpc: '2.0', method: 'notifications/initialized' });
  }
  ask(arguments_) { return this.request('tools/call', { name: 'ask_human', arguments: arguments_ }); }
  cancel(id) { this.send({ jsonrpc: '2.0', method: 'notifications/cancelled', params: { requestId: id, reason: 'verification cancellation' } }); }
  async close(signal) {
    if (signal) this.child.kill(signal); else this.child.stdin.end();
    await until(() => this.child.exitCode !== null || this.child.signalCode !== null, 'MCP server termination');
    return this.closed;
  }
}
const base = (requestId, count = 2) => ({ repository_path: repo, source_agent: 'Codex', question: 'Synthetic MCP confirmation?',
  choices: Array.from({ length: count }, (_, index) => ({ id: index ? `other_${index}` : 'received_correctly', label: index ? `Other ${index}` : 'Received correctly' })),
  request_id: requestId });
const rejected = (response) => Boolean(response.error || response.result?.isError);
const result = (response, requestId, selected = 'received_correctly') => {
  assert(!rejected(response), 'expected canonical result');
  assert.deepEqual(response.result.structuredContent, { request_id: requestId, selected_choice_id: selected, source_channel_id: 'imessage' });
  assert.deepEqual(JSON.parse(response.result.content[0].text), response.result.structuredContent);
};
async function waiting(client, args) {
  const count = ready().length;
  const request = client.ask(args);
  await until(() => ready().length > count, 'request watcher started');
  const record = lines('sent.jsonl').at(-1);
  const token = /^\[HIL · ([A-F0-9]+)\]/u.exec(record.text)?.[1];
  assert(token, 'production renderer token');
  assert(record.text.split('\n').includes('Codex · human-in-loop'));
  return { request, record, token };
}
function reply(pending, overrides = {}) {
  return { id: pending.record.id + 1, chat_id: 42, guid: `synthetic-reply-${pending.record.id}`,
    created_at: new Date().toISOString(), is_from_me: true, text: `${pending.token} 1`, ...overrides };
}
function emit(...records) { fs.appendFileSync(path.join(root, 'replies.jsonl'), `${records.map(JSON.stringify).join('\n')}\n`); }
const clients = [];
let daemon;
let passed = false;
let summary;
try {
  setMode('wait');
  daemon = spawn(binary, ['daemon', 'run'], { cwd: root, env, stdio: ['ignore', 'pipe', 'pipe'] });
  daemon.stdout.resume(); daemon.stderr.resume();
  await until(async () => { try { return Boolean(await status()); } catch { return false; } }, 'production daemon ready');
  const client = new Client(); clients.push(client);
  await client.initialize();
  const list = (await client.response(client.request('tools/list'))).result;
  assert.deepEqual(list.tools.map((tool) => tool.name), ['ask_human']);
  const schema = list.tools[0].inputSchema;
  assert.deepEqual(Object.keys(schema.properties).sort(), ['choices', 'context', 'question', 'recommended_choice', 'repository_path', 'request_id', 'source_agent']);
  assert.deepEqual(schema.required.toSorted(), ['choices', 'question', 'source_agent']);
  assert.equal(schema.properties.choices.minItems, 2);
  assert.equal(schema.properties.choices.maxItems, 6);
  assert.equal(schema.additionalProperties, false);
  assert.deepEqual(Object.keys(list.tools[0].outputSchema.properties).sort(), ['request_id', 'selected_choice_id', 'source_channel_id']);
  check('initialize, protocol negotiation, exact public tool and input/output schemas');

  const invalid = [
    null, {}, { ...base('missing-source'), source_agent: undefined }, { ...base('missing-question'), question: undefined },
    ...[0, 1, 7].map((n) => base(`choices-${n}`, n)),
    { ...base('duplicate'), choices: [{ id: 'same', label: 'First' }, { id: 'same', label: 'Second' }] },
    ...['source_agent', 'question', 'request_id', 'context', 'repository_path'].map((field) => ({ ...base(`empty-${field}`), [field]: ' ' })),
    { ...base('empty-id'), choices: [{ id: '', label: 'First' }, { id: 'next', label: 'Second' }] },
    { ...base('empty-label'), choices: [{ id: 'first', label: ' ' }, { id: 'next', label: 'Second' }] },
    { ...base('bad-recommendation'), recommended_choice: 'not-a-choice' },
    { ...base('missing-path'), repository_path: path.join(root, 'does-not-exist') },
    { ...base('not-repository'), repository_path: '/' },
    { ...base('private-field'), recipient: 'forbidden' },
    { ...base('unknown-field'), arbitrary: true },
    { ...base('unknown-choice-field'), choices: [{ id: 'first', label: 'First', command: 'forbidden' }, { id: 'next', label: 'Second' }] },
  ];
  for (const input of invalid) assert(rejected(await client.response(client.ask(input))), 'invalid MCP payload was accepted');
  assert.equal(sends(), 0);
  assert(rejected(await client.response(client.request('tools/call', { name: 'shell', arguments: {} }))));
  assert((await client.response(client.request('unknown/method'))).error);
  check('malformed/missing/boundary/unknown-field payloads rejected before any channel send', { rejected_payloads: invalid.length, application_sends: 0 });

  client.child.stdin.write('{malformed JSON\n');
  client.send({ jsonrpc: '2.0', id: 'malformed-request', params: {} });
  assert((await client.response(client.request('ping'))).result);
  check('malformed JSON and malformed protocol request recover without poisoning next request');

  for (const count of [2, 6]) {
    const args = { ...base(`valid-${count}`, count), context: 'Unicode 上下文', question: '继续验证？', recommended_choice: 'received_correctly' };
    args.choices[0].label = '正确收到 ✓';
    const before = sends();
    const pending = await waiting(client, args);
    assert(pending.record.text.includes('[recommended]'));
    emit(reply(pending), reply(pending, { id: pending.record.id + 2, guid: 'synthetic-duplicate' }));
    result(await client.response(pending.request), args.request_id);
    await quiescent();
    assert.equal(sends(), before + 1);
    assert.equal(client.responses.filter((row) => row.id === pending.request.id).length, 1);
  }
  check('2 and 6 choices, Unicode, recommendation, production rendering, stable result, one send/terminal', { requests: 2, application_sends: 2, canonical_results: 2 });

  const correlated = await waiting(client, base('strict-correlation'));
  const wrong = correlated.token === 'FFFF' ? 'EEEE' : 'FFFF';
  const stale = /^\[HIL · ([A-F0-9]+)\]/u.exec(lines('sent.jsonl').at(-2).text)[1];
  emit(reply(correlated, { text: '1' }), reply(correlated, { text: `${wrong} 1` }),
    reply(correlated, { text: `${stale} 1` }), reply(correlated, { text: `${correlated.token} 0` }),
    reply(correlated, { text: `${correlated.token} 3` }), reply(correlated, { chat_id: 43 }),
    reply(correlated, { id: correlated.record.id }), reply(correlated, { guid: correlated.record.guid }),
    reply(correlated, { is_reaction: true }), reply(correlated, { attachments: [{}] }),
    reply(correlated, { reply_to_guid: 'other-request' }), correlated.record);
  await pause(250);
  assert.equal((await status()).activeRequests, 1);
  assert(!client.responses.some((row) => row.id === correlated.request.id));
  emit(reply(correlated));
  result(await client.response(correlated.request), 'strict-correlation');
  await quiescent();
  check('same-account strict row/GUID/token/chat/option/reaction/attachment/outgoing rejection', { requests: 1, application_sends: 1, canonical_results: 1 });

  for (const mode of ['send_fail', 'watch_eof']) {
    setMode(mode);
    const before = sends();
    assert(rejected(await client.response(client.ask(base(mode)))));
    await quiescent();
    await pause(250);
    assert.equal(sends(), before + 1, 'uncertain/failing mutation must not retry');
    setMode('wait');
    const recovery = await waiting(client, base(`recover-${mode}`));
    emit(reply(recovery)); result(await client.response(recovery.request), `recover-${mode}`);
    await quiescent();
  }
  check('uncertain send/no retry, watch EOF failure, recovery through same MCP client', { requests: 4, application_sends: 4, failures: 2, canonical_results: 2 });

  const cancelled = await waiting(client, base('explicit-cancel'));
  client.cancel(cancelled.request.id);
  await quiescent();
  emit(reply(cancelled));
  await pause(150);
  assert(!client.responses.some((row) => row.id === cancelled.request.id && !rejected(row)));
  check('explicit MCP cancellation reaps watcher and ignores late reply', { requests: 1, application_sends: 1, canonical_results: 0 });

  setMode('slow_version');
  const beforeSlow = sends();
  const eventsBefore = lines('events.jsonl').length;
  const slow = client.ask(base('cancel-during-prepare'));
  await until(() => lines('events.jsonl').slice(eventsBefore).some((event) => event.command === '--version' && event.kind === 'spawn'), 'slow finite child started');
  client.cancel(slow.id);
  await quiescent();
  await pause(2100);
  assert.equal(sends(), beforeSlow, 'cancelled setup must never send later');
  setMode('wait');
  check('cancellation during finite preparation reaps child and prevents a later send', { requests: 1, application_sends: 0, canonical_results: 0 });

  setMode('slow_send');
  const beforeMutation = sends();
  const mutation = client.ask(base('cancel-during-send'));
  await until(() => sends() === beforeMutation + 1, 'slow send child started');
  client.cancel(mutation.id);
  await quiescent();
  await pause(2100);
  assert.equal(sends(), beforeMutation + 1, 'uncertain cancelled send must not retry');
  setMode('wait');
  check('cancellation during uncertain mutation reaps finite child without retry', { requests: 1, application_sends: 1, canonical_results: 0 });

  const first = await waiting(client, base('concurrent-a'));
  const beforeDuplicate = sends();
  assert(rejected(await client.response(client.ask(base('concurrent-a')))));
  assert.equal(sends(), beforeDuplicate);
  const second = await waiting(client, base('concurrent-b'));
  assert.notEqual(first.token, second.token);
  assert.equal((await status()).activeRequests, 2);
  client.cancel(first.request.id);
  await until(async () => (await status()).activeRequests === 1, 'cancel only request A');
  emit(reply(first));
  await pause(150);
  assert.equal((await status()).activeRequests, 1);
  emit(reply(second), reply(second, { id: second.record.id + 2, guid: 'synthetic-second-candidate' }));
  result(await client.response(second.request), 'concurrent-b');
  await quiescent();
  check('concurrent tokens isolated; cancel/reply A cannot resolve B; competing candidates win once', { requests: 2, duplicate_ids_rejected: 1, application_sends: 2, canonical_results: 1 });

  for (const signal of [undefined, 'SIGTERM']) {
    const pendingClient = new Client(); clients.push(pendingClient);
    await pendingClient.initialize();
    await waiting(pendingClient, base(`disconnect-${signal ?? 'eof'}-a`));
    await waiting(pendingClient, base(`disconnect-${signal ?? 'eof'}-b`));
    await pendingClient.close(signal);
    await quiescent();
  }
  check('stdio EOF and process termination clean all owned concurrent pending requests/watchers', { requests: 4, application_sends: 4, canonical_results: 0 });
  await client.close();
  await quiescent();
  assert(clients.every((item) => !alive(item.child.pid)));
  check('all MCP clients/servers terminated; repeat registry/process cleanup check');
  summary = { verdict: 'PASS', checks, synthetic: true, real_message_sends: 0,
    synthetic_application_sends: sends(), canonical_results: clients.flatMap((item) => item.responses).filter((response) => response.result?.structuredContent).length,
    active_requests: (await status()).activeRequests, synthetic_processes_alive: liveFakePids().length };
  assert.equal(summary.synthetic_application_sends, 15);
  assert.equal(summary.canonical_results, 6);
  passed = true;
} finally {
  for (const client of clients) if (alive(client.child.pid)) client.child.kill('SIGKILL');
  if (daemon && alive(daemon.pid)) {
    spawnSync(binary, ['daemon', 'stop', '--force'], { cwd: root, env, stdio: 'ignore', timeout: 10000 });
    if (alive(daemon.pid)) daemon.kill('SIGKILL');
  }
  for (const pid of liveFakePids()) { try { process.kill(pid, 'SIGKILL'); } catch {} }
  await until(() => liveFakePids().length === 0 && (!daemon || !alive(daemon.pid)), 'owned process cleanup');
  if (passed) {
    summary.daemon_and_mcp_cleanup = clients.every((client) => !alive(client.child.pid));
    assert(summary.daemon_and_mcp_cleanup);
    fs.writeFileSync(path.join(root, 'result.json'), JSON.stringify(summary, null, 2));
    console.log(JSON.stringify({ verdict: 'PASS', checks: checks.length, synthetic_application_sends: sends(), real_message_sends: 0 }));
  }
  // The task owner consumes the aggregate evidence, then removes this owned scratch directory.
  console.log(`Synthetic verification state: ${path.relative(repo, root)}`);
}
