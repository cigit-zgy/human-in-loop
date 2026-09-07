#!/usr/bin/env node
// Finite, synthetic imsg boundary for mcp-verify.mjs. Never accesses Apple Messages.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const root = process.env.ASKHUMAN_MCP_VERIFY_DIR;
assert(root, 'This synthetic executable requires the verification harness');
const args = process.argv.slice(2);
const command = args[0];
const value = (flag) => args[args.indexOf(flag) + 1];
const read = (file) => fs.existsSync(path.join(root, file))
  ? fs.readFileSync(path.join(root, file), 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse)
  : [];
const write = (data) => process.stdout.write(`${JSON.stringify(data)}\n`);
const event = (data) => fs.appendFileSync(path.join(root, 'events.jsonl'), `${JSON.stringify(data)}\n`);
const control = () => JSON.parse(fs.readFileSync(path.join(root, 'control.json'), 'utf8'));
event({ kind: 'spawn', pid: process.pid, command, scoped: args.includes('--chat-id') });
process.on('exit', () => event({ kind: 'exit', pid: process.pid, command }));

if (command === '--version') {
  if (control().mode === 'slow_version') await new Promise((resolve) => setTimeout(resolve, 2000));
  process.stdout.write('0.15.1\n');
} else if (command === 'chats') {
  write({ id: 42, guid: 'synthetic-direct-chat', service: 'iMessage', is_group: false, participants: ['synthetic@example.invalid'] });
} else if (command === 'history') {
  const sent = read('sent.jsonl');
  write(sent.at(-1) ?? { id: 1, chat_id: 42, guid: 'synthetic-history', created_at: new Date(0).toISOString(), is_from_me: true, text: '' });
} else if (command === 'send') {
  assert.equal(value('--service'), 'imessage');
  assert(args.includes('--no-sms-fallback'));
  assert.equal(value('--to'), 'synthetic@example.invalid');
  assert(!args.some((arg) => ['auto', 'sms', '--file'].includes(arg)));
  event({ kind: 'send', pid: process.pid, service: 'imessage', no_sms_fallback: true });
  if (control().mode === 'slow_send') await new Promise((resolve) => setTimeout(resolve, 2000));
  if (control().mode === 'send_fail') {
    process.stderr.write('synthetic may_have_completed\n');
    process.exitCode = 1;
  } else {
    // A short filesystem lock gives concurrent synthetic sends deterministic unique rows.
    const lock = path.join(root, 'sequence.lock');
    const deadline = Date.now() + 2000;
    while (true) {
      try { fs.mkdirSync(lock); break; } catch (error) {
        if (error.code !== 'EEXIST' || Date.now() > deadline) throw error;
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 5);
      }
    }
    let record;
    try {
      const previous = read('sent.jsonl').at(-1);
      record = { id: (previous?.id ?? 100) + 100, chat_id: 42,
        guid: `synthetic-sent-${process.pid}`, created_at: new Date().toISOString(),
        is_from_me: true, text: value('--text') };
      fs.appendFileSync(path.join(root, 'sent.jsonl'), `${JSON.stringify(record)}\n`);
    } finally { fs.rmdirSync(lock); }
    write({ status: 'sent', id: record.id, guid: record.guid });
  }
} else if (command === 'watch') {
  const since = Number(value('--since-rowid'));
  if (!args.includes('--chat-id')) {
    const outgoing = read('sent.jsonl').find((record) => record.id === since + 1);
    assert(outgoing, 'sent-row resolution must use the actual receipt');
    write(outgoing);
    setInterval(() => {}, 1000);
  } else if (control().mode === 'watch_eof') {
    process.exitCode = 0;
  } else {
    assert.equal(Number(value('--chat-id')), 42);
    event({ kind: 'ready', pid: process.pid, since });
    let offset = 0;
    setInterval(() => {
      const replies = read('replies.jsonl');
      for (const row of replies.slice(offset)) write(row);
      offset = replies.length;
    }, 25);
  }
} else {
  throw new Error('Unsupported synthetic command');
}
