#!/usr/bin/env node
// SessionStart hook - remind agents about the narrow-test workflow.
console.log(
  "Tests: prefer `cargo nextest run --lib <name>` or `cargo xtask test changed`. `cargo xtask test dev` is a batch gate, not an edit-loop tool.",
);
