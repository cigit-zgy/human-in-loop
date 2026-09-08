// Small STDIO protocol client shared by synthetic and production MCP qualification.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import readline from 'node:readline';

export class Client {
  constructor({ binary, cwd, env }) {
    this.child = spawn(binary, ['mcp'], { cwd, env, stdio: ['pipe', 'pipe', 'pipe'] });
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
  notify(arguments_) { return this.request('tools/call', { name: 'notify_human', arguments: arguments_ }); }
  cancel(id) { this.send({ jsonrpc: '2.0', method: 'notifications/cancelled', params: { requestId: id, reason: 'verification cancellation' } }); }
  async close(signal) {
    if (signal) this.child.kill(signal); else this.child.stdin.end();
    return this.response({ promise: this.closed });
  }
}
