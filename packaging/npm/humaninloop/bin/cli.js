#!/usr/bin/env node
"use strict";

// 全局命令入口（npm i -g humaninloop 后的 `AskHuman`）：
// 解析当前平台二进制并透传 argv 与退出码。
// 下游程序集成请直接使用 require("humaninloop").getBinaryPath()，不必经此 shim。

const { spawn } = require("child_process");
const { getBinaryPath } = require("../index.js");

const bin = getBinaryPath();
if (!bin) {
  process.stderr.write(
    "humaninloop: 未找到当前平台的 AskHuman 二进制。\n" +
      "请确认对应平台子包已安装，或设置环境变量 HUMANINLOOP_BINARY 指向二进制。\n"
  );
  process.exit(1);
}

const child = spawn(bin, process.argv.slice(2), { stdio: "inherit" });

child.on("error", (err) => {
  process.stderr.write(`humaninloop: 启动失败: ${err.message}\n`);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code == null ? 0 : code);
  }
});
