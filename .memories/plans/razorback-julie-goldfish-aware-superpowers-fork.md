---
id: razorback-julie-goldfish-aware-superpowers-fork
title: "Razorback: Julie/Goldfish-Aware Superpowers Fork"
status: active
created: 2026-03-02T22:17:24.583Z
updated: 2026-03-02T22:22:04.784Z
tags:
  - razorback
  - superpowers
  - julie
  - goldfish
  - plugin
---

# Razorback Implementation Plan

## Goal
Fork Superpowers v4.3.1, add Julie + Goldfish tool awareness at injection points throughout all 14 skills and 3 subagent prompt templates. Same process, better tools.

## Phase 1: Project Setup
- [x] Create ~/source/razorback repository
- [x] Write and commit design document
- [x] Write and commit implementation plan (16 tasks)
- [ ] Set up .claude-plugin/plugin.json manifest (Task 1)
- [ ] Copy and modify hook infrastructure (Task 2)

## Phase 2: Fork Skills (Highest Impact First)
- [ ] Create using-razorback entry point with toolchain section (Task 3)
- [ ] Fork implementer prompt — add Julie orientation block (Task 4)
- [ ] Fork reviewer prompts — add Julie-aware review (Task 5)
- [ ] Fork subagent-driven-development SKILL.md (Task 6)
- [ ] Fork brainstorming — add get_context + recall (Task 7)
- [ ] Fork writing-plans — add Goldfish plan persistence (Task 8)

## Phase 3: Fork Remaining Skills
- [ ] Fork systematic-debugging + sub-techniques (Task 9)
- [ ] Fork requesting-code-review + code-reviewer.md (Task 10)
- [ ] Fork verification-before-completion (Task 11)
- [ ] Fork executing-plans + finishing-a-development-branch (Task 12)
- [ ] Fork receiving-code-review + test-driven-development (Task 13)
- [ ] Fork low/no-change skills (Task 14)

## Phase 4: Agents, Commands, Docs
- [ ] Fork agents/code-reviewer.md + commands/ (Task 15)
- [ ] Write README.md (Task 16)

## Key Decisions
- Faithful fork (same 14 skills, same process flow)
- Injection points strategy (add tool calls, don't rewrite)
- Goldfish plans primary (not file-based)
- Hard dependencies on Julie + Goldfish
- General plugin (not personal-specific)
- Explicit fork with MIT license credit
