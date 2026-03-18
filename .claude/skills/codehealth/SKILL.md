---
name: codehealth
description: Generate a codebase health report — risk hotspots, test gaps, dead code candidates, and prioritized recommendations. Use when the user asks about code quality, wants to find risky code, or asks "what should we fix first?"
user-invocable: true
disable-model-invocation: true
allowed-tools: mcp__julie__query_metrics, mcp__julie__deep_dive, mcp__julie__get_context
---

# Codebase Health Report

Generate a comprehensive health report for the codebase (or a focused area if specified).

## Arguments

`$ARGUMENTS` is an optional area focus — a search query like "authentication", a file pattern like "src/tools/", or empty for the whole codebase.

## Query Pattern

### Step 1: Scope (if area specified)

If `$ARGUMENTS` is not empty, first orient on the area:

```
get_context(query="$ARGUMENTS")
```

Note the file paths of the pivots — use these to build a `file_pattern` for subsequent queries.

### Step 2: Gather Data

Run these queries (adjust `file_pattern` if scoped to an area):

1. **Risk Hotspots:**
```
query_metrics(sort_by="change_risk", order="desc", exclude_tests=true, limit=10)
```

2. **Test Gaps:**
```
query_metrics(sort_by="centrality", order="desc", has_tests=false, exclude_tests=true, limit=10)
```

3. **Dead Code Candidates:**
```
query_metrics(sort_by="centrality", order="asc", exclude_tests=true, limit=10)
```

### Step 3: Deep Dive on Worst Offenders

For the top 3-5 most concerning symbols from the risk hotspots and test gaps queries:

```
deep_dive(symbol="<name>", depth="overview")
```

This reveals callers, callees, and detailed metadata.

### Step 4: Security Quick Check

```
query_metrics(sort_by="security_risk", order="desc", min_risk="medium", exclude_tests=true, limit=5)
```

## Report Format

Present the report in this structure:

```markdown
# Codebase Health Report
**Scope:** [area or "Full codebase"] | **Date:** [today]

## Summary
- Total symbols analyzed: [from query results]
- Risk hotspots found: [count of HIGH/MEDIUM change_risk]
- Untested high-centrality code: [count from test gaps query]
- Security signals: [count from security check]

## Risk Hotspots
[Top 10 by change_risk, showing name, file:line, risk level, test coverage, centrality]
[For top 3-5, include a one-sentence explanation of why it's risky based on deep_dive]

## Test Gaps
[High-centrality symbols with no test coverage]
[Explain why each matters: "This function is called by N other functions but has no tests"]

## Dead Code Candidates
[Zero-centrality symbols, excluding obvious entry points like main/run/setup/new]
[Note: zero centrality may mean the symbol is an entry point not yet detected — flag uncertain cases]

## Security Signals
[Any medium+ security risk symbols, with brief explanation]

## Recommendations
[Prioritized list: what to fix first and why, based on risk x centrality x test coverage]
[Focus on actionable items, not exhaustive lists]
```

## Guidelines

- Keep the report concise — this is an executive summary, not a line-by-line audit
- Explain findings in plain language — assume the reader may not be deeply familiar with the code
- When centrality = 0 for a public function named like an entry point (main, run, setup, start, handler, index), note it's likely a legitimate entry point, not dead code
- If no concerning findings in a category, say so — "No high-risk items found" is useful information
