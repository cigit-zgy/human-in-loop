#!/usr/bin/env node
// Actual stdio MCP client → installed binary → production daemon/coordinator → synthetic imsg.
// Usage: node scripts/mcp-verify.mjs /absolute/path/to/installed/human-in-loop /absolute/empty/task/scratch
// No test path can reach the real imsg executable or the user's channel configuration.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Client } from './mcp-client.mjs';

const repo = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
assert(process.argv[2] && process.argv[3], 'Provide the production binary and task scratch directory');
assert(path.isAbsolute(process.argv[2]) && path.isAbsolute(process.argv[3]), 'Use absolute binary and scratch paths');
const binary = fs.realpathSync(process.argv[2]);
const scratch = process.argv[3];
fs.mkdirSync(scratch, { recursive: true });
assert.equal(fs.readdirSync(scratch).length, 0, 'Task scratch must be empty');
const root = scratch;
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
const env = { ...process.env, HUMAN_IN_LOOP_HOME: configDir, ASKHUMAN_NO_KEYCHAIN: '1',
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
const base = (requestId, count = 2) => ({ repository_path: repo, source_agent: 'Codex', question: 'Synthetic MCP confirmation?',
  choices: Array.from({ length: count }, (_, index) => ({ id: index ? `other_${index}` : 'received_correctly', label: index ? `Other ${index}` : 'Received correctly' })),
  request_id: requestId });
const richDetail = [
  ['进度', '这是完全合成的审阅进度说明，用于确认多行正文在协议、协调器与手机文本渲染之间保持一致。'.repeat(3)],
  ['中文题目', '虚构循环水系统的结构化决策卡，不对应任何真实论文、作者、数据或生产结论。'.repeat(3)],
  ['研究对象', '对象仅为测试用的进水区、反应区和回流区，所有条件与观察量均为人工编写。'.repeat(3)],
  ['模型或计算方法', '采用假想的分区平衡步骤组织证据，只验证字符预算与段落边界，不产生科学主张。'.repeat(3)],
  ['模型承担的作用', '模型只负责排列合成证据、候选解释和判据，使审阅者能看到完整决策上下文。'.repeat(3)],
  ['与模型直接相关的关键结果', '段落标题应独立成行，五个稳定选择应完整出现，返回值仍是语义标识。'.repeat(3)],
  ['判据提醒', '只判断正文、换行、问题与选项是否完整；若发生截断或合并，应阻止继续。'.repeat(2)],
].map(([heading, body]) => `${heading}\n${body}`).join('\n\n');
assert([...richDetail].length >= 600 && [...richDetail].length <= 800, 'rich detail fixture must stay WME-sized');
const notification = (notificationId, status = 'PASS') => ({ repository_path: repo, source_agent: 'Codex', status,
  summary: 'Synthetic terminal verification 完成', task_id: 'SYNTHETIC-VERIFICATION',
  context: [{ label: 'Checks', value: 'Passed' }], locator: 'reports/codex/synthetic.md', notification_id: notificationId });
const rejected = (response) => Boolean(response.error || response.result?.isError);
const result = (response, requestId, selected = 'received_correctly') => {
  assert(!rejected(response), 'expected canonical result');
  assert.deepEqual(response.result.structuredContent, { request_id: requestId, selected_choice_id: selected, source_channel_id: 'imessage' });
  assert.deepEqual(JSON.parse(response.result.content[0].text), response.result.structuredContent);
};
const notificationResult = (response, notificationId, deliveryStatus = 'SENT') => {
  assert(!rejected(response), 'expected bounded notification dispatch result');
  const value = response.result.structuredContent;
  assert.deepEqual(Object.keys(value).sort(), ['channel_ids', 'delivery_status', 'notification_id']);
  assert.equal(typeof value.notification_id, 'string');
  assert(value.notification_id.length > 0);
  if (notificationId !== undefined) assert.equal(value.notification_id, notificationId);
  assert.equal(value.delivery_status, deliveryStatus);
  assert.deepEqual(value.channel_ids, ['imessage']);
  assert.deepEqual(JSON.parse(response.result.content[0].text), value);
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
const daemons = [];
let daemon;
let passed = false;
let summary;
async function startDaemon() {
  daemon = spawn(binary, ['daemon', 'run'], { cwd: root, env, stdio: ['ignore', 'pipe', 'pipe'] });
  daemons.push(daemon);
  daemon.stdout.resume(); daemon.stderr.resume();
  await until(async () => { try { return (await status()).pid === daemon.pid; } catch { return false; } }, 'production daemon ready');
}
try {
  setMode('wait');
  await startDaemon();
  const client = new Client({ binary, cwd: root, env }); clients.push(client);
  await client.initialize();
  const list = (await client.response(client.request('tools/list'))).result;
  assert.deepEqual(list.tools.map((tool) => tool.name).sort(), ['ask_human', 'notify_human']);
  const askTool = list.tools.find((tool) => tool.name === 'ask_human');
  const notifyTool = list.tools.find((tool) => tool.name === 'notify_human');
  const schema = askTool.inputSchema;
  assert.deepEqual(Object.keys(schema.properties).sort(), ['choices', 'context', 'detail', 'question', 'recommended_choice', 'repository_path', 'request_id', 'source_agent']);
  assert.deepEqual(schema.required.toSorted(), ['choices', 'question', 'source_agent']);
  assert.equal(schema.properties.choices.minItems, 2);
  assert.equal(schema.properties.choices.maxItems, 6);
  assert.equal(schema.additionalProperties, false);
  assert.deepEqual(Object.keys(askTool.outputSchema.properties).sort(), ['request_id', 'selected_choice_id', 'source_channel_id']);
  const notifySchema = notifyTool.inputSchema;
  assert.deepEqual(Object.keys(notifySchema.properties).sort(), ['context', 'locator', 'notification_id', 'repository_path', 'source_agent', 'status', 'summary', 'task_id']);
  assert.deepEqual(notifySchema.required.toSorted(), ['source_agent', 'status', 'summary']);
  assert.equal(notifySchema.additionalProperties, false);
  const statusSchema = notifySchema.properties.status;
  const statusType = statusSchema.$ref ? notifySchema.$defs[statusSchema.$ref.split('/').at(-1)] : statusSchema;
  assert.deepEqual(statusType.enum.toSorted(), ['BLOCKED', 'FAIL', 'PASS', 'PASS_WITH_LIMITATIONS']);
  assert.deepEqual(Object.keys(notifyTool.outputSchema.properties).sort(), ['channel_ids', 'delivery_status', 'notification_id']);
  check('initialize, protocol negotiation, exactly two public tools and closed input schemas');

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
  const invalidNotifications = [
    null, {},
    ...['source_agent', 'status', 'summary'].map((field) => ({ ...notification(`missing-${field}`), [field]: undefined })),
    ...['source_agent', 'summary', 'task_id', 'locator', 'notification_id', 'repository_path'].map((field) => ({ ...notification(`empty-${field}`), [field]: ' ' })),
    ...['source_agent', 'summary', 'task_id', 'locator', 'notification_id'].map((field) => ({ ...notification(`long-${field}`), [field]: 'x'.repeat(10000) })),
    ...['', 'pass', 'ACK', 'UNKNOWN'].map((status) => notification('invalid-status', status)),
    ...['https://user:secret@example.invalid/report', 'https:user:secret@example.invalid/report', 'javascript:alert(1)', 'data:text/plain,private', 'file:/private/report'].map((locator) => ({ ...notification('unsafe-locator'), locator })),
    { ...notification('missing-path'), repository_path: path.join(root, 'does-not-exist') },
    { ...notification('not-repository'), repository_path: '/' },
    ...['choices', 'recipient', 'chat_id', 'credential', 'command', 'files', 'arbitrary'].map((field) => ({ ...notification(`private-${field}`), [field]: 'forbidden' })),
    { ...notification('context-type'), context: 'arbitrary text' },
    { ...notification('context-empty-label'), context: [{ label: ' ', value: 'Value' }] },
    { ...notification('context-empty-value'), context: [{ label: 'Label', value: ' ' }] },
    { ...notification('context-unknown-field'), context: [{ label: 'Label', value: 'Value', command: 'forbidden' }] },
  ];
  for (const input of invalidNotifications) assert(rejected(await client.response(client.notify(input))), 'invalid notification payload was accepted');
  assert.equal(sends(), 0);
  assert(rejected(await client.response(client.request('tools/call', { name: 'shell', arguments: {} }))));
  assert((await client.response(client.request('unknown/method'))).error);
  check('malformed/missing/boundary/unknown-field payloads rejected before any channel send', { rejected_payloads: invalid.length + invalidNotifications.length, application_sends: 0 });

  const privatePayloadMarker = 'PRIVATE-PAYLOAD-MARKER-DO-NOT-LOG';
  const oversized = { ...base('detail-too-long'), detail: privatePayloadMarker + '界'.repeat(1001) };
  const beforeOversized = sends();
  assert(rejected(await client.response(client.ask(oversized))));
  await quiescent();
  assert.equal(sends(), beforeOversized, 'oversized detail must fail before mutation');
  const daemonLog = fs.readFileSync(path.join(configDir, 'daemon.log'), 'utf8');
  assert(daemonLog.split('\n').some((line) => line.includes('"event":"no_available_channel"') && line.includes('"imessage":"detail_too_long"')));
  assert(!daemonLog.includes(privatePayloadMarker), 'payload content leaked into diagnostics');
  check('oversized decision body fails before send with a fixed redacted channel reason', { application_sends: 0 });

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

  const richArgs = {
    ...base('rich-detail', 5),
    context: '合成审阅回归',
    detail: richDetail,
    choices: [
      { id: 'approve', label: '内容完整' },
      { id: 'minor_revision', label: '小幅修订' },
      { id: 'major_revision', label: '大幅修订' },
      { id: 'insufficient_evidence', label: '证据不足' },
      { id: 'stop', label: '停止继续' },
    ],
  };
  const beforeRich = sends();
  const rich = await waiting(client, richArgs);
  assert(rich.record.text.includes(richDetail));
  assert(rich.record.text.includes('\n\n判据提醒\n'));
  for (const choice of richArgs.choices) assert(rich.record.text.includes(choice.label));
  emit(reply(rich));
  result(await client.response(rich.request), 'rich-detail', 'approve');
  await quiescent();
  assert.equal(sends(), beforeRich + 1);
  check('WME-sized synthetic multiline detail crosses real MCP, daemon, coordinator and production renderer', { detail_chars: [...richDetail].length, choices: 5, application_sends: 1, canonical_results: 1 });

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
    const pendingClient = new Client({ binary, cwd: root, env }); clients.push(pendingClient);
    await pendingClient.initialize();
    await waiting(pendingClient, base(`disconnect-${signal ?? 'eof'}-a`));
    await waiting(pendingClient, base(`disconnect-${signal ?? 'eof'}-b`));
    await pendingClient.close(signal);
    await quiescent();
  }
  check('stdio EOF and process termination clean all owned concurrent pending requests/watchers', { requests: 4, application_sends: 4, canonical_results: 0 });

  for (const terminalStatus of ['PASS', 'PASS_WITH_LIMITATIONS', 'BLOCKED', 'FAIL']) {
    const args = notification(terminalStatus === 'FAIL' ? undefined : `notify-${terminalStatus}`, terminalStatus);
    const before = sends();
    const watcherCount = ready().length;
    const repliesBefore = lines('replies.jsonl').length;
    setMode('slow_send');
    const request = client.notify(args);
    await until(() => sends() === before + 1, 'notification dispatch started');
    assert.equal((await status()).activeRequests, 0, 'notification must not create a pending decision during dispatch');
    notificationResult(await client.response(request), args.notification_id);
    await quiescent();
    const rendered = lines('sent.jsonl').at(-1).text;
    for (const value of [terminalStatus, 'Codex', 'human-in-loop', args.summary, args.task_id, args.locator, 'Checks: Passed']) {
      assert(rendered.includes(value), 'notification must preserve compact status, identity, summary, task, locator, and context');
    }
    assert(rendered.includes(`Report: ${args.locator}`), 'locator must stay in the same application message with its Report label');
    assert.equal(ready().length, watcherCount, 'notification must not start a decision reply watcher');
    assert.equal(lines('replies.jsonl').length, repliesBefore, 'notification completes without synthetic acknowledgement');
    assert.equal(sends(), before + 1);
    assert.equal(client.responses.filter((row) => row.id === request.id).length, 1);
  }
  check('all terminal statuses preserve compact fields and return without pending decision or acknowledgement', { notifications: 4, application_sends: 4 });

  setMode('wait');
  const standalone = { source_agent: 'Codex', status: 'PASS', summary: 'Synthetic non-repository terminal notification' };
  notificationResult(await client.response(client.notify(standalone)));
  await quiescent();
  check('non-repository notification and generated notification id', { notifications: 1, application_sends: 1 });

  setMode('send_fail');
  const beforeFailedNotification = sends();
  notificationResult(await client.response(client.notify(notification('notify-send-fail'))), 'notify-send-fail', 'FAILED');
  await quiescent();
  await pause(250);
  assert.equal(sends(), beforeFailedNotification + 1, 'uncertain notification mutation must not retry');
  check('notification transport failure returns bounded FAILED status without decision or retry', { notifications: 1, application_sends: 1 });

  for (const mode of ['slow_version', 'slow_send']) {
    setMode(mode);
    const before = sends();
    const eventsBefore = lines('events.jsonl').length;
    const request = client.notify(notification(`notify-cancel-${mode}`));
    await until(() => lines('events.jsonl').slice(eventsBefore).some((event) => mode === 'slow_send'
      ? event.kind === 'send' : event.command === '--version' && event.kind === 'spawn'), 'notification finite child started');
    assert.equal((await status()).activeRequests, 0);
    client.cancel(request.id);
    await quiescent();
    await pause(2100);
    assert.equal(sends(), before + Number(mode === 'slow_send'), 'cancelled notification must not send later or retry');
    assert(!client.responses.some((row) => row.id === request.id && row.result?.structuredContent?.delivery_status === 'SENT'));
  }
  check('notification cancellation reaps preparation/send child without a delayed send or retry', { notifications: 2, application_sends: 1 });

  for (const signal of [undefined, 'SIGTERM']) {
    setMode('slow_send');
    const pendingClient = new Client({ binary, cwd: root, env }); clients.push(pendingClient);
    await pendingClient.initialize();
    const before = sends();
    pendingClient.notify(notification(`notify-disconnect-${signal ?? 'eof'}`));
    await until(() => sends() === before + 1, 'notification send child started');
    assert.equal((await status()).activeRequests, 0);
    await pendingClient.close(signal);
    await quiescent();
    await pause(2100);
    assert.equal(sends(), before + 1);
  }
  check('notification stdio EOF and termination reap finite transport processes', { notifications: 2, application_sends: 2 });

  let forcedStopResults = 0;
  for (const force of [false, true]) {
    for (const mode of ['slow_version', 'slow_send', 'slow_receipt']) {
      setMode(mode);
      const before = sends();
      const recordsBefore = lines('sent.jsonl').length;
      const eventsBefore = lines('events.jsonl').length;
      const notificationId = `notify-daemon-${force ? 'forced' : 'graceful'}-${mode}`;
      const request = client.notify(notification(notificationId));
      await until(() => lines('events.jsonl').slice(eventsBefore).some((event) => {
        if (mode === 'slow_send') return event.kind === 'send';
        if (mode === 'slow_receipt') return event.kind === 'spawn' && event.command === 'watch' && !event.scoped;
        return event.kind === 'spawn' && event.command === '--version';
      }), 'notification transport stage started before daemon stop');
      assert.equal((await status()).activeRequests, 0);
      const stopped = spawnSync(binary, ['daemon', 'stop', ...(force ? ['--force'] : [])], { cwd: root, env, stdio: 'ignore', timeout: 10000 });
      assert.equal(stopped.status, 0, 'daemon stop command must complete');
      const response = await client.response(request);
      if (force) {
        if (!rejected(response)) {
          notificationResult(response, notificationId, 'FAILED');
          forcedStopResults += 1;
        }
      } else {
        notificationResult(response, notificationId);
      }
      await until(() => !alive(daemon.pid), 'daemon stopped');
      assert.deepEqual(liveFakePids(), [], 'daemon must reap owned transport processes before exit');
      const expectedSends = before + Number(!force || mode !== 'slow_version');
      assert.equal(sends(), expectedSends);
      assert.equal(lines('sent.jsonl').length, recordsBefore + Number(!force || mode === 'slow_receipt'));
      await pause(2100);
      assert.equal(sends(), expectedSends, 'stopped daemon must not cause a delayed send or retry');
      assert.deepEqual(liveFakePids(), []);
      assert.equal(client.responses.filter((row) => row.id === request.id).length, 1, 'daemon stop must resolve the MCP call once');
      setMode('wait');
      await startDaemon();
      await quiescent();
    }
  }
  check('daemon graceful drain and forced stop finish MCP calls and reap preparation/send/receipt processes', { notifications: 6, application_sends: 5, graceful_completions: 3, forced_failures: 3 });

  await client.close();
  await quiescent();
  assert(clients.every((item) => !alive(item.child.pid)));
  const publicOutput = clients.map((item) => `${JSON.stringify(item.responses)}\n${item.stderr}`).join('\n');
  for (const privateValue of ['synthetic@example.invalid', 'synthetic-direct-chat', 'synthetic may_have_completed']) {
    assert(!publicOutput.includes(privateValue), 'private transport data leaked into MCP output or logs');
  }
  check('all MCP clients/servers terminated; repeat registry/process cleanup check');
  const structuredResults = clients.flatMap((item) => item.responses).flatMap((response) => response.result?.structuredContent ?? []);
  summary = { verdict: 'PASS', checks, synthetic: true, real_message_sends: 0,
    synthetic_application_sends: sends(), canonical_results: structuredResults.filter((value) => value.selected_choice_id).length,
    notification_results: structuredResults.filter((value) => value.delivery_status).length,
    active_requests: (await status()).activeRequests, synthetic_processes_alive: liveFakePids().length };
  assert.equal(summary.synthetic_application_sends, 30);
  assert.equal(summary.canonical_results, 7);
  assert.equal(summary.notification_results, 9 + forcedStopResults);
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
    summary.daemon_and_mcp_cleanup = clients.every((client) => !alive(client.child.pid)) && daemons.every((item) => !alive(item.pid));
    assert(summary.daemon_and_mcp_cleanup);
    fs.writeFileSync(path.join(root, 'result.json'), JSON.stringify(summary, null, 2));
    console.log(JSON.stringify({ verdict: 'PASS', checks: checks.length, synthetic_application_sends: sends(), real_message_sends: 0 }));
  }
  // The task owner consumes the aggregate evidence, then removes this owned scratch directory.
  console.log(`Synthetic verification state: ${path.relative(repo, root)}`);
}
