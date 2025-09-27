# Julie Workspace Management System

This document describes Julie's comprehensive workspace management system for handling multiple project indexes within a single MCP session.

## Overview

Julie supports indexing multiple workspaces simultaneously:
- **Primary workspace**: Where MCP started (project-a), owns .julie directory
- **Reference workspaces**: Additional projects (project-b, project-c) for cross-project learning
- All data centralized in primary workspace's .julie directory

## Architecture

Based on analysis of COA CodeSearch implementation with adaptations for Julie's Rust architecture.

### Key Design Decisions

1. **Workspace Registry Architecture**
   - `workspace_registry.json` in `.julie/` directory
   - Centralized tracking of all indexed workspaces
   - Atomic file operations with backup copies
   - Memory caching with 5-second TTL for performance

2. **Workspace ID Generation**
   ```rust
   // Format: workspacename_hash8
   // Example: "project-b_a3f2b8c1"
   - Normalize path (lowercase, consistent separators)
   - SHA256 hash of normalized path
   - Take first 8 chars of hash
   - Combine: safe_workspace_name + "_" + hash8
   ```

3. **Storage Strategy**
   ```
   project-a/.julie/
   ├── workspace_registry.json      # Central registry
   ├── db/
   │   └── symbols.db               # Single DB for ALL workspaces
   ├── index/
   │   └── tantivy/
   │       ├── primary/            # Primary workspace index
   │       └── references/         # Reference workspace indexes
   │           ├── project-b_a3f2b8c1/
   │           └── project-c_d4e5f6a2/
   ```

## Implementation Components

### Phase 1: Workspace Registry System

**1.1 Registry Models** (`workspace/registry.rs`)
```rust
pub struct WorkspaceRegistry {
    pub version: String,
    pub last_updated: DateTime<Utc>,
    pub primary_workspace: Option<WorkspaceEntry>,
    pub reference_workspaces: HashMap<String, WorkspaceEntry>,
    pub orphaned_indexes: HashMap<String, OrphanedIndex>,
    pub config: RegistryConfig,
    pub statistics: RegistryStatistics,
}

pub struct WorkspaceEntry {
    pub id: String,              // workspace_hash
    pub original_path: String,
    pub directory_name: String,  // workspacename_hash
    pub display_name: String,
    pub workspace_type: WorkspaceType,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,  // For TTL
    pub document_count: usize,
    pub index_size_bytes: u64,
    pub status: WorkspaceStatus,
}

pub enum WorkspaceType {
    Primary,    // File watching enabled, never expires
    Reference,  // No file watching, can expire
    Session,    // Temporary, cleared on restart
}
```

**1.2 Registry Service** (`workspace/registry_service.rs`)
- Load/save registry with atomic operations
- Generate workspace IDs using SHA256
- Track last accessed times
- Handle orphan detection and cleanup

### Phase 2: Unified manage_workspace Tool

**Replace index_workspace with manage_workspace:**
```rust
pub enum WorkspaceCommand {
    // Core operations
    Index { path: Option<String>, force: bool },       // Index primary or current
    Add { path: String, name: Option<String> },        // Add reference workspace
    Remove { workspace_id: String },                    // Remove specific workspace
    List,                                               // Show all workspaces

    // Maintenance
    Clean { expired_only: bool },                      // Clean expired/orphaned
    Refresh { workspace_id: String },                  // Re-index workspace
    Stats { workspace_id: Option<String> },            // Show statistics

    // Configuration
    SetTTL { days: u32 },                             // Set expiry (default: 7)
    SetLimit { max_size_mb: u64 },                    // Storage limit
}
```

### Phase 3: Database Schema Updates

```sql
-- Add workspace tracking to all tables
ALTER TABLE symbols ADD COLUMN workspace_id TEXT NOT NULL DEFAULT 'primary';
ALTER TABLE files ADD COLUMN workspace_id TEXT NOT NULL DEFAULT 'primary';
ALTER TABLE relationships ADD COLUMN workspace_id TEXT NOT NULL DEFAULT 'primary';

-- Create indexes for workspace filtering
CREATE INDEX idx_symbols_workspace ON symbols(workspace_id);
CREATE INDEX idx_files_workspace ON files(workspace_id);

-- Track workspace metadata
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    type TEXT CHECK(type IN ('primary', 'reference', 'session')),
    indexed_at INTEGER,
    last_accessed INTEGER,
    expires_at INTEGER,
    file_count INTEGER,
    symbol_count INTEGER
);
```

### Phase 4: Search Integration

**Update fast_search to be workspace-aware:**
```rust
// Optional workspace filter
pub struct FastSearchTool {
    pub query: String,
    pub mode: String,
    pub workspace: Option<String>,  // "all", "primary", or specific ID
    pub include_references: bool,   // Include reference workspaces
}
```

### Phase 5: Eviction Strategies

**5.1 Manual Management**
- `manage_workspace remove project-b_a3f2b8c1`
- `manage_workspace clean` - Remove all expired

**5.2 TTL-Based Expiry (Default: 7 days)**
- Reference workspaces expire after N days of no access
- Update last_accessed on every search/navigation
- Background cleanup on index operations

**5.3 Size-Based LRU**
- Configure max total size (e.g., 500MB)
- Track index sizes in registry
- Evict least recently used when limit exceeded

### Phase 6: File Watching Strategy

**Only watch primary workspace:**
- Primary workspace: Full IncrementalIndexer with Blake3
- Reference workspaces: Snapshot only, no watching
- Manual refresh command for reference updates

## User Experience

### Workflow Examples:
```bash
# Start in project-a (primary workspace)
> manage_workspace index                    # Indexes project-a as primary
> manage_workspace add ../project-b         # Adds project-b as reference
> fast_search "AuthService"                 # Searches both workspaces
> fast_search "AuthService" --workspace=project-b  # Search only project-b
> manage_workspace list                     # Shows all workspaces with stats
> manage_workspace remove project-b         # Removes project-b index
> manage_workspace clean                    # Removes expired workspaces
```

### Command Examples:
```bash
# Basic workspace management
manage_workspace index                      # Index current directory as primary
manage_workspace add /path/to/project-b     # Add reference workspace
manage_workspace add ../project-c --name="Core Utils"  # Add with custom name
manage_workspace list                       # Show all workspaces

# Maintenance operations
manage_workspace clean                      # Remove expired workspaces
manage_workspace clean --expired-only=false # Remove all non-primary workspaces
manage_workspace refresh project-b_a3f2b8c1 # Re-index specific workspace
manage_workspace stats                      # Show overall statistics
manage_workspace stats project-b_a3f2b8c1   # Show specific workspace stats

# Configuration
manage_workspace set-ttl 14                # Set 14-day expiry for references
manage_workspace set-limit 1000            # Set 1GB total size limit

# Search with workspace filtering
fast_search "UserService"                  # Search all workspaces
fast_search "UserService" --workspace=primary  # Search only primary
fast_search "UserService" --workspace=project-b_a3f2b8c1  # Search specific workspace
```

## Implementation Order

1. **Workspace Registry Models & Service** (workspace/registry.rs, workspace/registry_service.rs)
2. **Update Database Schema** (Add workspace_id columns)
3. **Create manage_workspace Tool** (Replace index_workspace)
4. **Update Indexing Process** (Tag with workspace_id)
5. **Update Search Tools** (Add workspace filtering)
6. **Implement Cleanup/Eviction** (TTL and manual)
7. **Add Workspace Statistics** (Track usage metrics)

## Benefits

- **Controlled Growth**: Data doesn't accumulate forever
- **Clear Boundaries**: Know exactly what's indexed
- **Flexible Management**: Manual + automatic cleanup
- **Performance**: Separate indexes for better speed
- **User-Friendly**: Simple commands, clear feedback
- **Scalable**: Handle many reference workspaces efficiently

## Configuration

### Default Settings
- **TTL for reference workspaces**: 7 days
- **Maximum total index size**: 500MB
- **Auto cleanup**: Enabled
- **Cleanup interval**: 1 hour

### Registry Location
- **File**: `<primary_workspace>/.julie/workspace_registry.json`
- **Backup**: `<primary_workspace>/.julie/workspace_registry.json.backup`
- **Format**: JSON with atomic write operations

## Error Handling

- **Missing paths**: Graceful handling with clear error messages
- **Corrupted registry**: Automatic backup restoration
- **Orphaned indexes**: Detection and scheduled cleanup
- **Permission issues**: Clear feedback and recovery suggestions
- **Disk space**: Size limits with LRU eviction

## Security Considerations

- **Path validation**: Prevent directory traversal attacks
- **Workspace isolation**: Each workspace's data properly tagged
- **Access controls**: Respect file system permissions
- **Safe cleanup**: Verify before deleting index data

## Future Enhancements

- **Workspace synchronization**: Share workspaces across Julie instances
- **Remote workspaces**: Index workspaces on network shares
- **Workspace templates**: Pre-configured workspace types
- **Advanced analytics**: Detailed usage tracking and optimization suggestions
- **Integration hooks**: API for external workspace management tools