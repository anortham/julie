---
name: security-audit
description: Run a security audit of the codebase — finds injection sinks, untested security code, and high-exposure risks. Use when the user asks about security, wants a security review, or is concerned about AI-generated code safety.
user-invocable: true
disable-model-invocation: true
allowed-tools: mcp__julie__query_metrics, mcp__julie__deep_dive, mcp__julie__get_context
---

# Security Audit

Analyze the codebase for security risks. Designed to be understandable by someone who isn't a security expert — explain WHY something is risky, not just that it was flagged.

## Arguments

`$ARGUMENTS` is an optional area focus. Empty = full codebase.

## Query Pattern

### Step 1: Find All Security Signals

```
query_metrics(sort_by="security_risk", order="desc", min_risk="low", exclude_tests=true, limit=30)
```

### Step 2: Find Untested Security-Sensitive Code (The Scariest Combination)

```
query_metrics(sort_by="security_risk", order="desc", has_tests=false, min_risk="medium", exclude_tests=true, limit=20)
```

### Step 3: Deep Dive on HIGH Risk Symbols

For every symbol with security_risk label "HIGH", run:

```
deep_dive(symbol="<name>", depth="context")
```

This reveals the actual code, callers (who uses this risky code?), and detailed security signals.

### Step 4: Check High-Exposure Risks

```
query_metrics(sort_by="security_risk", order="desc", min_risk="medium", exclude_tests=true, limit=15)
```

Cross-reference with centrality from the results — high centrality + high security risk = the most dangerous combination.

## Report Format

```markdown
# Security Audit Report
**Scope:** [area or "Full codebase"] | **Date:** [today]

## Executive Summary
- **Overall Risk Level:** [HIGH/MEDIUM/LOW based on findings]
- Security signals found: [total count]
  - HIGH: [count] | MEDIUM: [count] | LOW: [count]
- Untested security-sensitive code: [count]

## Critical Findings

### Injection Risks
[Symbols with sink_calls signals — SQL, command execution, XSS]
For each:
- **Symbol:** name (file:line)
- **What's happening:** [Plain language: "This function builds SQL queries by concatenating user input"]
- **Why it's dangerous:** [Plain language: "An attacker could inject SQL commands through the input parameter"]
- **Who uses it:** [Callers from deep_dive — shows blast radius]
- **Test status:** [Has tests / No tests]
- **Recommendation:** [Use parameterized queries / input sanitization / etc.]

### Authentication & Authorization
[Symbols related to auth with security signals]

### Sensitive Data Handling
[Crypto usage, secrets patterns]

### Other Security Signals
[Remaining findings]

## Untested Security Code
[The scariest section: known security-sensitive code with ZERO tests]
[For each: what it does, why it needs tests, what kind of test to write]

## High-Exposure Risks
[Security issues in high-centrality code — these are the most impactful to fix]
[Sorted by centrality x security_risk score]

## Recommendations (Priority Order)
1. [Most critical: untested HIGH-risk code]
2. [HIGH-risk code with tests but known injection patterns]
3. [MEDIUM-risk code in high-centrality positions]
4. [LOW-risk items worth monitoring]

For each recommendation:
- What to fix and where
- Why it matters (in plain language)
- Suggested approach (parameterized queries, input validation, auth checks, etc.)
```

## Guidelines

- **Explain everything in plain language.** The reader may be a non-technical founder or a junior dev who used AI to write the code.
- Security signals come from static analysis — they indicate PATTERNS that are risky, not confirmed vulnerabilities. Frame findings as "this pattern is risky because..." not "this is a vulnerability."
- The combination of untested + security-sensitive is the highest priority. A tested security-sensitive function is infinitely better than an untested one.
- High centrality amplifies risk — a vulnerable function called by 50 other functions is much worse than an isolated one.
- If NO security signals are found, say so clearly: "No security risk signals detected. This doesn't guarantee the code is secure, but no common risk patterns were found by static analysis."
