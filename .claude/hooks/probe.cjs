#!/usr/bin/env node
// Temporary probe to verify PreToolUse output shapes reach the model.
// Logs all stdin to /tmp/julie-hook-probe.log, then emits JSON additionalContext
// with a unique sentinel string. After running a Bash command, search the next
// system-reminder for SENTINEL_AAA1.
const fs = require("node:fs");

let stdin = "";
process.stdin.setEncoding("utf-8");
process.stdin.on("data", (chunk) => (stdin += chunk));
process.stdin.on("end", () => {
  try {
    fs.appendFileSync(
      "/tmp/julie-hook-probe.log",
      `\n[${new Date().toISOString()}] STDIN: ${stdin}\n`,
    );
  } catch (_) {}

  const SENTINEL = "JULIE_PROBE_2025_05_08_SENTINEL_AAA1";
  const out = {
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      additionalContext: `${SENTINEL} probe-says-additionalContext-works`,
    },
  };
  process.stdout.write(JSON.stringify(out) + "\n");
});
