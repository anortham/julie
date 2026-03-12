# AGENTS.md

This file is for AI agents that are not Claude Code (e.g., Copilot, Cursor, Windsurf, Cody, aider, etc.).

**You MUST read [CLAUDE.md](./CLAUDE.md) before making any changes to this codebase.** It contains critical project guidelines including:

- Project architecture and design decisions
- **Language-agnostic design requirements** — Julie supports 31 languages; all heuristics must work across ALL project layouts, not just Rust
- TDD methodology (non-negotiable)
- File size limits (500 lines max for implementation files)
- Test organization standards
- Test running strategy (don't run the full suite after every change)

## Key Principles

1. **Language-agnostic everything.** Never hardcode paths like `src/tests/` or `src/` — they only match one project layout. Use generic heuristics that work for Rust, C#, Python, Java, Go, TypeScript, Ruby, Swift, and every other language Julie supports.

2. **TDD is mandatory.** Write a failing test first. No exceptions.

3. **Dogfood Julie's own tools.** Use Julie's MCP tools (fast_search, deep_dive, fast_refs, get_symbols, get_context) to understand the codebase before modifying it. Don't grep when Julie can search. Don't read entire files when get_symbols gives you the structure.

4. **Default to `cargo xtask test dev`.** That is the canonical fast local tier now. Use raw `cargo test --lib <filter>` only to narrow a known failure after the xtask run points you at a bucket. See `CLAUDE.md` for the full tiering strategy, including `smoke`, `system`, `dogfood`, and `full`.

5. **Check references before changing symbols.** Use `fast_refs` to see all callers before modifying any function, struct, or interface.
