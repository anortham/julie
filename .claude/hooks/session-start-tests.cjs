#!/usr/bin/env node
// SessionStart hook - remind agents about the narrow-test workflow.
console.log(
  "Tests: run `cargo nextest run --lib <name>` in the edit loop. `cargo xtask test dev` is a batch gate, not an edit-loop tool.",
);
