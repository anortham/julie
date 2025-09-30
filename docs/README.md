# Julie Documentation

This directory contains active documentation for the Julie project.

## Current Documentation

- **`workspace_management.md`** - Julie's workspace registry system for multi-project support
- **`future/`** - Future ideas and enhancements

## Historical Documentation

The `historical/` directory contains archived documents from Julie's development:

- **`THE_PLAN.md`** - Original vision document explaining Julie's mission and architecture decisions
- **`julie.md`** - Technology comparison (Rust vs Go) and dependency analysis that informed the Rust choice
- **`julie-implementation-checklist.md`** - Phase-by-phase implementation tracking (now complete)

These historical docs provide context on design decisions but may reference features or issues that have since been resolved.

## Primary Documentation

The primary source of truth for Julie's current state and guidelines is:
- **`/CLAUDE.md`** - Project development guidelines and TDD methodology
- **`/ARCHITECTURE_DEBT.md`** - Technical debt tracking and resolved issues
- **`/REALITY_CHECK.md`** - Honest assessment of what works vs. what's claimed
- **`/TODO.md`** - Observations and ideas from current work

## Removed Documentation

The following outdated documents were deleted during 2025-09-30 cleanup:
- `coder.md` - Sept 29 review claiming issues that were subsequently fixed
- `review.md` - Sept 27 review with outdated findings

These reviews pre-dated major fixes including:
- ✅ Semantic search implementation (HNSW)
- ✅ Database schema completion
- ✅ Refactoring safety verification
- ✅ Performance optimizations

---

**Last Updated**: 2025-09-30
**Status**: Documentation synchronized with actual implementation