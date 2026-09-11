#!/usr/bin/env node
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const ROUTING_BLOCK_FILE = 'julie-routing-block.md';
const MAX_STDIN_BYTES = 65536;

const EVENT_CONFIG = {
  'session-start': { hookEventName: 'SessionStart' },
  'subagent-start': { hookEventName: 'SubagentStart' },
};

function sessionHooksDisabled() {
  const flag = (process.env.JULIE_SESSION_HOOKS ?? '').trim().toLowerCase();
  return flag === '0' || flag === 'false';
}

function readRoutingBlock() {
  const blockPath = path.join(__dirname, ROUTING_BLOCK_FILE);
  if (!fs.existsSync(blockPath)) return null;
  const content = fs.readFileSync(blockPath, 'utf8').replaceAll('\r\n', '\n').trim();
  return content.length > 0 ? content : null;
}

function readBoundedStdin(maxBytes = MAX_STDIN_BYTES) {
  if (process.stdin.isTTY) return '';
  try {
    const buf = Buffer.alloc(maxBytes);
    let bytesRead = 0;
    try {
      bytesRead = fs.readSync(0, buf, 0, maxBytes, null);
    } catch {
      return '';
    }
    if (bytesRead <= 0) return '';
    return buf.toString('utf8', 0, bytesRead);
  } catch {
    return '';
  }
}

function main() {
  if (sessionHooksDisabled()) return;

  const eventArg = process.argv[2];
  const config = EVENT_CONFIG[eventArg];
  if (!config) return;

  const routingBlock = readRoutingBlock();
  if (!routingBlock) return;

  readBoundedStdin();

  process.stdout.write(
    JSON.stringify({
      hookSpecificOutput: {
        hookEventName: config.hookEventName,
        additionalContext: routingBlock,
      },
    }),
  );
}

try {
  main();
} catch {
  // Fail open: guidance is an optimization, so a hook fault must never disturb the host session.
}
