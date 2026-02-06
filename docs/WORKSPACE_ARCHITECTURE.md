# Workspace Architecture

**Last Updated:** 2025-11-07
**Status:** Production

This document provides detailed information about Julie's workspace architecture, routing, and storage.

## How Workspace Isolation Works

Each workspace has its own PHYSICAL database and Tantivy index files:

```
.julie/indexes/
├── julie_316c0b08/              ← PRIMARY workspace
│   ├── db/symbols.db            ← SQLite database
│   └── tantivy/                 ← Tantivy search index
│
└── coa-mcp-framework_c77f81e4/  ← REFERENCE workspace
    ├── db/symbols.db            ← SQLite database
    └── tantivy/                 ← Tantivy search index
```

## Workspace Routing

Workspace selection happens when opening the DB connection:
1. Tool → Handler → Workspace Registry → Open DB File
2. Once DB is open, you're locked to that workspace
3. Can't query other workspaces from that connection

**Tool-level `workspace` parameters are ESSENTIAL** - they route to the correct DB file.

## Primary vs Reference Workspaces

**Primary Workspace:**
- Where you're actively developing (where you run Julie)
- Has its own `.julie/` directory at workspace root
- Stores indexes for ITSELF and ALL reference workspaces
- Full `JulieWorkspace` object with complete machinery

**Reference Workspaces:**
- Other codebases you want to search/reference
- Do NOT have their own `.julie/` directories
- Indexes stored in primary workspace's `.julie/indexes/{workspace_id}/`
- Just indexed data - not independent workspace objects

## Storage Location

Julie stores workspace data at **project level**, not user home:
- Primary workspace data: `<project>/.julie/`
- **NOT** at `~/.julie/` (common mistake!)

## Log Location

Logs are PROJECT-LEVEL, not user-level:

```bash
# CORRECT
/Users/murphy/source/julie/.julie/logs/julie.log.2025-11-07

# WRONG
~/.julie/logs/  ← THIS DOES NOT EXIST!
```

## Key Benefits

- ✅ Complete workspace isolation - Each workspace has own db/tantivy index
- ✅ Centralized storage - All indexes in one location (primary workspace)
- ✅ Trivial deletion - `rm -rf indexes/{workspace_id}/` removes everything
- ✅ Smaller, faster indexes - Simple single-tier architecture
