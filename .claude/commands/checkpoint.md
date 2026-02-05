---
allowed-tools: mcp__julie__checkpoint
argument-hint: [description] [--type decision|learning|observation]
description: Save a development memory checkpoint
---

Save a development memory checkpoint.

If arguments are provided ($ARGUMENTS), use them as the checkpoint description and save immediately. Parse --type flag if present (checkpoint, decision, learning, observation).

If no arguments are provided, analyze the recent conversation context (last 5-10 messages) to determine what was accomplished:
- Create a clear, concise description (1-2 sentences)
- Determine appropriate type (checkpoint/decision/learning/observation)
- Generate 2-4 relevant tags

Then IMMEDIATELY save the checkpoint using mcp__julie__checkpoint - DO NOT ask for confirmation.

After the checkpoint is saved:
1. Get the memory file path from the tool response (e.g., ".memories/2025-11-15/030013_bf3c.md")
2. IMMEDIATELY commit it to git:
   ```
   git add <memory_file_path>
   git commit -m "checkpoint: <brief summary of checkpoint description>"
   ```
3. Confirm with: "✓ Checkpoint saved and committed! Recall it later with /recall"

⚠️ CRITICAL: Memory files are designed to be git-committed. Always commit them immediately after creation.
