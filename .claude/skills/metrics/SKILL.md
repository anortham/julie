---
name: metrics
description: Show Julie operational metrics -- session stats, tool usage, context efficiency, and historical trends. Use when the user asks about Julie's performance, how it's doing, how much context was saved, or wants a metrics/stats report.
user-invocable: true
disable-model-invocation: true
allowed-tools: mcp__julie__query_metrics
---

# Julie Metrics Report

Show the user how Julie is performing and how much context it's saving them.

## Process

1. Call `query_metrics` with `category: "session"` to get current session stats.
2. Present the results. Lead with the "NOT injected into context" headline number -- this is the key value metric.
3. If the user asks for history or trends, also call `query_metrics` with `category: "history"` and present both.

## Arguments

`$ARGUMENTS` is optional. If the user says "history" or "trends", include the history call. Otherwise, show session only.

## Formatting

Present the raw tool output directly -- it's already formatted for readability. Add a brief intro line like:

"Here's how Julie has been performing this session:"

Then the tool output.

## Rules

- Do NOT editorialize or make value claims beyond what the numbers show
- Do NOT say "Julie saved you X" -- instead present "X was NOT injected into context" (factual arithmetic)
- Present the data and let the user draw conclusions
- If session has zero calls, say so briefly rather than showing empty tables
