#!/usr/bin/env node
/**
 * mcp-test-server.js — 最小 MCP 测试服务器
 * JSON-RPC 2.0 over stdio，供 llm-wiki MCP 客户端集成测试使用。
 *
 * 提供 3 个工具：
 *   echo        — 原样返回输入文本
 *   get_time    — 返回当前服务器时间（ISO 8601）
 *   add         — 对两个数字求和
 */

"use strict";

const readline = require("readline");

const TOOLS = [
  {
    name: "echo",
    description: "原样返回输入的文本，用于测试 MCP 通信是否正常。",
    inputSchema: {
      type: "object",
      properties: {
        text: { type: "string", description: "要回显的文本" },
      },
      required: ["text"],
    },
  },
  {
    name: "get_time",
    description: "返回 MCP 服务器当前时间（ISO 8601 格式），无需参数。",
    inputSchema: {
      type: "object",
      properties: {},
      required: [],
    },
  },
  {
    name: "add",
    description: "对两个数字求和并返回结果。",
    inputSchema: {
      type: "object",
      properties: {
        a: { type: "number", description: "第一个数" },
        b: { type: "number", description: "第二个数" },
      },
      required: ["a", "b"],
    },
  },
];

function respond(id, result) {
  const msg = JSON.stringify({ jsonrpc: "2.0", id, result });
  process.stdout.write(msg + "\n");
}

function respondError(id, code, message) {
  const msg = JSON.stringify({ jsonrpc: "2.0", id, error: { code, message } });
  process.stdout.write(msg + "\n");
}

function callTool(name, args) {
  if (name === "echo") {
    const text = String(args.text ?? "");
    return `[echo] ${text}`;
  }
  if (name === "get_time") {
    return `当前时间：${new Date().toISOString()}`;
  }
  if (name === "add") {
    const a = Number(args.a ?? 0);
    const b = Number(args.b ?? 0);
    return `${a} + ${b} = ${a + b}`;
  }
  return null;
}

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });

rl.on("line", (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;

  let req;
  try {
    req = JSON.parse(trimmed);
  } catch {
    return; // ignore non-JSON lines
  }

  const { id, method, params } = req;

  // Notifications have no id — ignore silently.
  if (id === undefined || id === null) return;

  if (method === "initialize") {
    respond(id, {
      protocolVersion: "2024-11-05",
      capabilities: { tools: {} },
      serverInfo: { name: "mcp-test-server", version: "1.0.0" },
    });
    return;
  }

  if (method === "tools/list") {
    respond(id, { tools: TOOLS });
    return;
  }

  if (method === "tools/call") {
    const toolName = params?.name;
    const toolArgs = params?.arguments ?? {};
    const text = callTool(toolName, toolArgs);
    if (text === null) {
      respondError(id, -32601, `未知工具: ${toolName}`);
    } else {
      respond(id, { content: [{ type: "text", text }] });
    }
    return;
  }

  respondError(id, -32601, `未知方法: ${method}`);
});

// Keep process alive.
rl.on("close", () => process.exit(0));
