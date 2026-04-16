use super::ProjectionFreshness;
use super::{HealthLevel, ProjectionState, SystemHealthSnapshot, SystemStatus};

impl SystemHealthSnapshot {
    pub fn render_report(&self, detailed: bool) -> String {
        let mut report = String::from("JULIE SYSTEM HEALTH REPORT\n\n");

        report.push_str("Overall Status\n");
        report.push_str(&format!("Overall Level: {}\n", self.overall.label()));
        report.push_str(&format!(
            "System Readiness: {}\n\n",
            readiness_label(&self.readiness)
        ));

        report.push_str("Control Plane\n");
        report.push_str(&format!(
            "Control Plane Level: {}\n",
            self.control_plane.level.label()
        ));
        report.push_str(&format!(
            "Daemon Status: {}\n",
            self.control_plane.daemon_state.label()
        ));
        report.push_str(&format!(
            "Primary Workspace: {}\n",
            self.control_plane
                .primary_workspace_id
                .as_deref()
                .unwrap_or("unbound")
        ));
        report.push_str(&format!(
            "Watcher Status: {}\n",
            self.control_plane.watcher_state.label()
        ));
        if let Some(ref_count) = self.control_plane.watcher_ref_count {
            report.push_str(&format!("Watcher Ref Count: {}\n", ref_count));
        }
        if detailed || self.control_plane.watcher_grace_active {
            report.push_str(&format!(
                "Watcher Grace Active: {}\n",
                self.control_plane.watcher_grace_active
            ));
        }
        report.push_str(&format!("Detail: {}\n\n", self.control_plane.detail));

        report.push_str("Data Plane\n");
        report.push_str(&format!(
            "Data Plane Level: {}\n",
            self.data_plane.level.label()
        ));
        report.push_str(&format!(
            "SQLite Status: {}\n",
            self.data_plane.canonical_store.sqlite_status_label()
        ));

        if self.data_plane.canonical_store.level == HealthLevel::Unavailable {
            report.push_str("Database not initialized\n");
        } else {
            let symbols_per_file = if self.data_plane.canonical_store.file_count > 0 {
                self.data_plane.canonical_store.symbol_count as f64
                    / self.data_plane.canonical_store.file_count as f64
            } else {
                0.0
            };

            report.push_str("Data Summary:\n");
            report.push_str(&format!(
                "• {} symbols across {} files\n",
                self.data_plane.canonical_store.symbol_count,
                self.data_plane.canonical_store.file_count
            ));
            report.push_str(&format!(
                "• {} relationships tracked\n",
                self.data_plane.canonical_store.relationship_count
            ));
            report.push_str(&format!(
                "• {} languages supported: {}\n",
                self.data_plane.canonical_store.languages.len(),
                display_languages(&self.data_plane.canonical_store.languages)
            ));
            report.push_str(&format!(
                "• {:.1} symbols per file average\n",
                symbols_per_file
            ));
            report.push_str(&format!(
                "Storage: {:.2} MB on disk\n",
                self.data_plane.canonical_store.db_size_mb
            ));
            if self.data_plane.canonical_store.embedding_count > 0 {
                report.push_str(&format!(
                    "Embeddings: {} vectors\n",
                    self.data_plane.canonical_store.embedding_count
                ));
            } else {
                report.push_str("Embeddings: None\n");
            }
        }

        report.push_str(&format!(
            "Projection Status: {}\n",
            self.data_plane.search_projection.state.label()
        ));
        report.push_str(&format!(
            "Projection Freshness: {}\n",
            self.data_plane.search_projection.freshness.label()
        ));
        report.push_str(&format!(
            "Projection Workspace: {}\n",
            self.data_plane
                .search_projection
                .workspace_id
                .as_deref()
                .unwrap_or("primary")
        ));
        report.push_str(&format!(
            "Canonical Revision: {}\n",
            self.data_plane
                .search_projection
                .canonical_revision
                .map(|revision| revision.to_string())
                .unwrap_or_else(|| "none".to_string())
        ));
        report.push_str(&format!(
            "Projected Revision: {}\n",
            self.data_plane
                .search_projection
                .projected_revision
                .map(|revision| revision.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
        report.push_str(&format!(
            "Projection Revision Lag: {}\n",
            self.data_plane
                .search_projection
                .revision_lag
                .map(|lag| lag.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
        report.push_str(&format!(
            "Projection Repair Needed: {}\n",
            self.data_plane.search_projection.repair_needed
        ));
        report.push_str(&format!(
            "Projection Detail: {}\n",
            self.data_plane.search_projection.detail
        ));
        report.push_str(&format!(
            "Indexing Status: {}\n",
            self.data_plane.indexing.level.label()
        ));
        report.push_str(&format!(
            "Indexing Operation: {}\n",
            self.data_plane
                .indexing
                .active_operation
                .as_deref()
                .unwrap_or("idle")
                .to_ascii_uppercase()
        ));
        report.push_str(&format!(
            "Indexing Stage: {}\n",
            self.data_plane
                .indexing
                .stage
                .as_deref()
                .unwrap_or("idle")
                .to_ascii_uppercase()
        ));
        report.push_str(&format!(
            "Catch-Up Active: {}\n",
            self.data_plane.indexing.catchup_active
        ));
        report.push_str(&format!(
            "Watcher Paused: {}\n",
            self.data_plane.indexing.watcher_paused
        ));
        report.push_str(&format!(
            "Watcher Rescan Pending: {}\n",
            self.data_plane.indexing.watcher_rescan_pending
        ));
        report.push_str(&format!(
            "Dirty Projection Entries: {}\n",
            self.data_plane.indexing.dirty_projection_count
        ));
        report.push_str(&format!(
            "Repair Needed: {}\n",
            self.data_plane.indexing.repair_needed
        ));
        report.push_str(&format!(
            "Repair Reasons: {}\n",
            if self.data_plane.indexing.repair_reasons.is_empty() {
                "none".to_string()
            } else {
                self.data_plane.indexing.repair_reasons.join(", ")
            }
        ));
        report.push_str(&format!(
            "Indexing Detail: {}\n",
            self.data_plane.indexing.detail
        ));

        if detailed {
            report.push_str(&format!(
                "📊 Database: {} symbols, {} files, {} relationships\n",
                self.data_plane.canonical_store.symbol_count,
                self.data_plane.canonical_store.file_count,
                self.data_plane.canonical_store.relationship_count
            ));
            report.push_str(&format!(
                "{} Tantivy search {}\n",
                if self.data_plane.search_projection.state == ProjectionState::Ready {
                    "✅"
                } else {
                    "❌"
                },
                match self.data_plane.search_projection.freshness {
                    ProjectionFreshness::Current => "ready",
                    ProjectionFreshness::Lagging => "lagging behind canonical revision",
                    ProjectionFreshness::RebuildRequired => "repair required",
                    ProjectionFreshness::Unavailable => "index not initialized",
                }
            ));
        }

        report.push('\n');
        report.push_str("Runtime Plane\n");
        report.push_str(&format!(
            "Runtime Plane Level: {}\n",
            self.runtime_plane.level.label()
        ));
        report.push_str("Embedding Runtime\n");
        report.push_str(&format!(
            "Embedding Status: {}\n",
            self.runtime_plane.embeddings.state.label()
        ));
        report.push_str(&format!(
            "Runtime: {}\n",
            self.runtime_plane.embeddings.runtime
        ));
        report.push_str(&format!(
            "Requested Backend: {}\n",
            self.runtime_plane.embeddings.requested_backend
        ));
        report.push_str(&format!(
            "Backend: {}\n",
            self.runtime_plane.embeddings.backend
        ));
        report.push_str(&format!(
            "Device: {}\n",
            self.runtime_plane.embeddings.device
        ));
        report.push_str(&format!(
            "Accelerated: {}\n",
            self.runtime_plane.embeddings.accelerated
        ));
        report.push_str(&format!(
            "Degraded: {}\n",
            self.runtime_plane.embeddings.detail
        ));
        report.push_str(&format!(
            "Query Fallback: {}\n",
            self.runtime_plane.embeddings.query_fallback
        ));

        report
    }
}

fn readiness_label(readiness: &SystemStatus) -> &'static str {
    match readiness {
        SystemStatus::NotReady => "NOT READY",
        SystemStatus::SqliteOnly { .. } => "SQLITE ONLY",
        SystemStatus::FullyReady { .. } => "FULLY READY",
    }
}

fn display_languages(languages: &[String]) -> String {
    if languages.is_empty() {
        "none".to_string()
    } else {
        languages.join(", ")
    }
}
