use std::collections::BTreeSet;
use std::fmt;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndexingStage {
    Queued,
    Grouped,
    Extracting,
    Persisting,
    Projecting,
    Resolving,
    Analyzing,
    Completed,
}

impl IndexingStage {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Grouped => "grouped",
            Self::Extracting => "extracting",
            Self::Persisting => "persisting",
            Self::Projecting => "projecting",
            Self::Resolving => "resolving",
            Self::Analyzing => "analyzing",
            Self::Completed => "completed",
        }
    }
}

impl fmt::Display for IndexingStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum IndexingRepairReason {
    EmptyDatabase,
    StaleFiles,
    NewFiles,
    DeletedFiles,
    ExtractorFailure,
    ProjectionFailure,
    WatcherOverflow,
    TantivyDirty,
}

impl IndexingRepairReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::EmptyDatabase => "empty_database",
            Self::StaleFiles => "stale_files",
            Self::NewFiles => "new_files",
            Self::DeletedFiles => "deleted_files",
            Self::ExtractorFailure => "extractor_failure",
            Self::ProjectionFailure => "projection_failure",
            Self::WatcherOverflow => "watcher_overflow",
            Self::TantivyDirty => "tantivy_dirty",
        }
    }

    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "empty_database" => Some(Self::EmptyDatabase),
            "stale_files" => Some(Self::StaleFiles),
            "new_files" => Some(Self::NewFiles),
            "deleted_files" => Some(Self::DeletedFiles),
            "extractor_failure" => Some(Self::ExtractorFailure),
            "projection_failure" => Some(Self::ProjectionFailure),
            "watcher_overflow" => Some(Self::WatcherOverflow),
            "tantivy_dirty" => Some(Self::TantivyDirty),
            _ => None,
        }
    }
}

impl fmt::Display for IndexingRepairReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndexingOperation {
    Full,
    Incremental,
    CatchUp,
    WatcherRepair,
}

impl IndexingOperation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Incremental => "incremental",
            Self::CatchUp => "catch_up",
            Self::WatcherRepair => "watcher_repair",
        }
    }
}

impl fmt::Display for IndexingOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub(crate) type SharedIndexingRuntime = Arc<RwLock<IndexingRuntimeState>>;

#[derive(Debug, Clone)]
pub(crate) struct IndexingRuntimeSnapshot {
    pub active_operation: Option<IndexingOperation>,
    pub stage: Option<IndexingStage>,
    pub catchup_active: bool,
    pub watcher_paused: bool,
    pub watcher_rescan_pending: bool,
    pub dirty_projection_count: usize,
    pub repair_reasons: Vec<IndexingRepairReason>,
    pub repair_details: Vec<String>,
}

impl IndexingRuntimeSnapshot {
    pub(crate) fn repair_needed(&self) -> bool {
        !self.repair_reasons.is_empty() || !self.repair_details.is_empty()
    }

    pub(crate) fn repair_issue_count(&self) -> usize {
        self.repair_reasons.len() + self.repair_details.len()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct IndexingRuntimeState {
    active_operation: Option<IndexingOperation>,
    stage: Option<IndexingStage>,
    catchup_active: bool,
    watcher_paused: bool,
    watcher_rescan_pending: bool,
    dirty_projection_count: usize,
    repair_reasons: BTreeSet<IndexingRepairReason>,
    repair_details: Vec<String>,
}

impl Default for IndexingRuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexingRuntimeState {
    pub(crate) fn new() -> Self {
        Self {
            active_operation: None,
            stage: None,
            catchup_active: false,
            watcher_paused: false,
            watcher_rescan_pending: false,
            dirty_projection_count: 0,
            repair_reasons: BTreeSet::new(),
            repair_details: Vec::new(),
        }
    }

    pub(crate) fn shared() -> SharedIndexingRuntime {
        Arc::new(RwLock::new(Self::new()))
    }

    pub(crate) fn snapshot(&self) -> IndexingRuntimeSnapshot {
        IndexingRuntimeSnapshot {
            active_operation: self.active_operation,
            stage: self.stage,
            catchup_active: self.catchup_active,
            watcher_paused: self.watcher_paused,
            watcher_rescan_pending: self.watcher_rescan_pending,
            dirty_projection_count: self.dirty_projection_count,
            repair_reasons: self.repair_reasons.iter().copied().collect(),
            repair_details: self.repair_details.clone(),
        }
    }

    pub(crate) fn begin_operation(&mut self, operation: IndexingOperation) {
        self.active_operation = Some(operation);
        self.stage = Some(IndexingStage::Queued);
        self.repair_details.clear();
        self.repair_reasons
            .remove(&IndexingRepairReason::ExtractorFailure);
        self.repair_reasons
            .remove(&IndexingRepairReason::ProjectionFailure);
    }

    pub(crate) fn transition_stage(&mut self, stage: IndexingStage) {
        self.stage = Some(stage);
    }

    pub(crate) fn finish_operation(&mut self) {
        self.active_operation = None;
    }

    pub(crate) fn set_catchup_active(&mut self, active: bool) {
        self.catchup_active = active;
        if active {
            self.active_operation = Some(IndexingOperation::CatchUp);
            self.stage.get_or_insert(IndexingStage::Queued);
        } else if self.active_operation == Some(IndexingOperation::CatchUp) {
            self.active_operation = None;
        }
    }

    pub(crate) fn set_watcher_paused(&mut self, paused: bool) {
        self.watcher_paused = paused;
    }

    pub(crate) fn set_dirty_projection_count(&mut self, count: usize) {
        self.dirty_projection_count = count;
        if count > 0 {
            self.repair_reasons
                .insert(IndexingRepairReason::TantivyDirty);
        } else {
            self.repair_reasons
                .remove(&IndexingRepairReason::TantivyDirty);
            self.repair_reasons
                .remove(&IndexingRepairReason::ProjectionFailure);
        }
    }

    pub(crate) fn set_watcher_rescan_pending(&mut self, pending: bool) {
        self.watcher_rescan_pending = pending;
        if pending {
            self.repair_reasons
                .insert(IndexingRepairReason::WatcherOverflow);
        } else {
            self.repair_reasons
                .remove(&IndexingRepairReason::WatcherOverflow);
        }
    }

    pub(crate) fn record_repair_reason(&mut self, reason: IndexingRepairReason) {
        self.repair_reasons.insert(reason);
    }

    pub(crate) fn clear_repair_reason(&mut self, reason: IndexingRepairReason) {
        self.repair_reasons.remove(&reason);
    }

    pub(crate) fn replace_repair_details(&mut self, details: Vec<String>) {
        self.repair_details = details;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IndexedFileDisposition {
    Parsed,
    TextOnly,
    RepairNeeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexedFileState {
    pub relative_path: String,
    pub language: String,
    pub disposition: IndexedFileDisposition,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct IndexingBatchState {
    pub workspace_id: String,
    pub current_stage: IndexingStage,
    pub stage_history: Vec<IndexingStage>,
    pub file_states: Vec<IndexedFileState>,
    repair_needed: bool,
    repair_issues: Vec<String>,
}

impl IndexingBatchState {
    pub(crate) fn new(workspace_id: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            current_stage: IndexingStage::Queued,
            stage_history: vec![IndexingStage::Queued],
            file_states: Vec::new(),
            repair_needed: false,
            repair_issues: Vec::new(),
        }
    }

    pub(crate) fn transition_to(&mut self, stage: IndexingStage) {
        if self.current_stage == stage {
            return;
        }

        self.current_stage = stage;
        self.stage_history.push(stage);
    }

    pub(crate) fn record_file(
        &mut self,
        relative_path: impl Into<String>,
        language: impl Into<String>,
        disposition: IndexedFileDisposition,
        detail: Option<String>,
    ) {
        if disposition == IndexedFileDisposition::RepairNeeded {
            self.repair_needed = true;
        }

        self.file_states.push(IndexedFileState {
            relative_path: relative_path.into(),
            language: language.into(),
            disposition,
            detail,
        });
    }

    pub(crate) fn repair_needed(&self) -> bool {
        self.repair_needed
    }

    pub(crate) fn mark_repair_needed(&mut self, detail: impl Into<String>) {
        self.repair_needed = true;
        self.repair_issues.push(detail.into());
    }

    pub(crate) fn repair_issue_count(&self) -> usize {
        self.repair_issues.len()
    }

    pub(crate) fn repair_issues(&self) -> &[String] {
        &self.repair_issues
    }

    pub(crate) fn parsed_file_count(&self) -> usize {
        self.count_files(IndexedFileDisposition::Parsed)
    }

    pub(crate) fn text_only_file_count(&self) -> usize {
        self.count_files(IndexedFileDisposition::TextOnly)
    }

    pub(crate) fn repair_file_count(&self) -> usize {
        self.count_files(IndexedFileDisposition::RepairNeeded)
    }

    fn count_files(&self, disposition: IndexedFileDisposition) -> usize {
        self.file_states
            .iter()
            .filter(|file| file.disposition == disposition)
            .count()
    }
}
