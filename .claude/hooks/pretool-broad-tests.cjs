#!/usr/bin/env node
// PreToolUse:Bash hook - block broad test runs, nudge toward narrow tests.
//
// Blocks:
//   cargo xtask test (dev|full|system|dogfood|reliability|benchmark)
//   cargo (test|nextest run) --lib  (without a test-name filter)
//
// Allows (passes through):
//   cargo nextest run --lib test_foo
//   cargo xtask test changed
//   cargo xtask test smoke  (smoke is narrow by design)
//   cargo xtask test cli    (single bucket, fast)
//   cargo test -p xtask
//
// Bypass: set CLAUDE_ALLOW_BROAD_TESTS=1 before the call.

let input = "";
process.stdin.on("data", (chunk) => {
  input += chunk;
});
process.stdin.on("end", () => {
  if (process.env.CLAUDE_ALLOW_BROAD_TESTS === "1") {
    process.exit(0);
  }

  let cmd = "";
  try {
    const data = JSON.parse(input);
    cmd = (data && data.tool_input && data.tool_input.command) || "";
  } catch {
    // Fail open on parse errors; don't block the user's work because the
    // framework changed its input shape.
    process.exit(0);
  }

  const broadTier =
    /\bcargo\s+xtask\s+test\s+(dev|full|system|dogfood|reliability|benchmark)\b/.test(
      cmd,
    );

  // `--lib` is "unfiltered" when the token that follows it is not an
  // identifier-like test name (blocks `--lib`, `--lib -- --skip foo`,
  // `--lib 2>&1`, etc.; allows `--lib test_foo`, `--lib tests::tools::x`).
  const unfilteredLib =
    /\bcargo\s+(?:nextest\s+run|test)\s+--lib(?!\s+[A-Za-z_:])/.test(cmd);

  if (broadTier || unfilteredLib) {
    console.error(
      "Blocked: broad test run. Prefer:\n" +
        "  cargo nextest run --lib <exact_test_name>\n" +
        "  cargo xtask test changed\n" +
        "Set CLAUDE_ALLOW_BROAD_TESTS=1 to bypass when you actually want the broad run.",
    );
    process.exit(2);
  }

  process.exit(0);
});
