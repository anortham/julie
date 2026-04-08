#!/usr/bin/env node
// PreToolUse:Agent hook - remind to include Julie instructions for subagents
console.log("Subagents don't receive Julie's session guidance. If this subagent will explore or modify code, include Julie tool instructions in the prompt: fast_search (not Grep), get_symbols (before Read), deep_dive (before modifying), fast_refs (before changing), edit_file/edit_symbol (not Read+Edit).");
