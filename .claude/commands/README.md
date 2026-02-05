# Julie Memory Commands

Custom slash commands for Julie's development memory system.

## /checkpoint

Save development memories (checkpoints, decisions, learnings).

**Interactive mode** (no arguments):
```
/checkpoint
```
Claude analyzes recent work and suggests checkpoint details for confirmation.

**Direct mode** (with description):
```
/checkpoint Fixed race condition in auth flow
/checkpoint --type decision Chose SQLite for simplicity
/checkpoint --type learning Always validate user input on both client and server
```

**Memory types**: checkpoint (default), decision, learning, observation

---

## /recall

Retrieve saved memories.

**Time-based**:
```
/recall           # Last 10 memories
/recall 30m       # Last 30 minutes
/recall 1hr       # Last hour
/recall 3h        # Last 3 hours
/recall 1d        # Last day
/recall 3d        # Last 3 days
```

**Topic-based** (hybrid search):
```
/recall db path bug
/recall auth implementation
/recall performance optimization
```

**Filtered**:
```
/recall --type decision
/recall --type learning
/recall --since 1d database
```

---

## Quick Reference

| Command | Purpose | Example |
|---------|---------|---------|
| `/checkpoint` | Interactive save | `/checkpoint` |
| `/checkpoint [desc]` | Direct save | `/checkpoint Fixed login bug` |
| `/checkpoint --type [type] [desc]` | Typed save | `/checkpoint --type decision Use Postgres` |
| `/recall` | Last 10 | `/recall` |
| `/recall [time]` | Time-based | `/recall 30m` or `/recall 3h` |
| `/recall [topic]` | Topic search | `/recall auth bug` |
| `/recall --type [type]` | Filter by type | `/recall --type decision` |

---

## Tips

**Good checkpoint descriptions**:
- ✅ "Fixed race condition in auth flow by adding mutex"
- ✅ "Implemented dark mode toggle in settings"
- ✅ "Chose SQLite over Postgres for simplicity and fewer dependencies"
- ❌ "Fixed bug" (too vague)
- ❌ "Updated code" (not actionable)

**Effective recall queries**:
- ✅ `/recall auth bug` → finds "authentication error", "login failure", etc.
- ✅ `/recall --since 3h database` → recent database work
- ✅ `/recall --type decision` → all architectural decisions
- ❌ `/recall stuff` → too generic, poor results

---

See `docs/JULIE_2_PLAN.md` for memory system architecture details.
