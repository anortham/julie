use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tracing::warn;

use crate::database::SymbolDatabase;
use crate::handler::JulieServerHandler;
use crate::search::{SearchIndex, SearchProjection};
use crate::tools::workspace::indexing::state::SharedIndexingRuntime;
use crate::workspace::JulieWorkspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndexRouteRepairReason {
    PrimaryBindingUnavailable,
    WorkspaceIdentityUnavailable,
    StorageAnchorUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexRouteError {
    pub reason: IndexRouteRepairReason,
    detail: String,
}

impl IndexRouteError {
    fn new(reason: IndexRouteRepairReason, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for IndexRouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.detail)
    }
}

impl std::error::Error for IndexRouteError {}

pub(crate) struct IndexRoute {
    pub workspace_id: String,
    pub workspace_root: PathBuf,
    pub db_path: PathBuf,
    pub tantivy_path: PathBuf,
    pub is_primary: bool,
    pub database: Option<Arc<std::sync::Mutex<SymbolDatabase>>>,
    pub search_index: Option<Arc<std::sync::Mutex<SearchIndex>>>,
    pub indexing_runtime: Option<SharedIndexingRuntime>,
}

impl fmt::Debug for IndexRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IndexRoute")
            .field("workspace_id", &self.workspace_id)
            .field("workspace_root", &self.workspace_root)
            .field("db_path", &self.db_path)
            .field("tantivy_path", &self.tantivy_path)
            .field("is_primary", &self.is_primary)
            .finish()
    }
}

impl IndexRoute {
    async fn path_backed_route(
        handler: &JulieServerHandler,
        workspace_id: String,
        workspace_root: PathBuf,
        is_primary: bool,
    ) -> std::result::Result<Self, IndexRouteError> {
        let db_path = handler
            .workspace_db_file_path_for(&workspace_id)
            .await
            .map_err(|err| {
                IndexRouteError::new(
                    IndexRouteRepairReason::StorageAnchorUnavailable,
                    format!(
                        "failed to resolve database path for workspace '{}': {err}",
                        workspace_id
                    ),
                )
            })?;
        let tantivy_path = handler
            .workspace_tantivy_dir_for(&workspace_id)
            .await
            .map_err(|err| {
                IndexRouteError::new(
                    IndexRouteRepairReason::StorageAnchorUnavailable,
                    format!(
                        "failed to resolve Tantivy path for workspace '{}': {err}",
                        workspace_id
                    ),
                )
            })?;

        Ok(Self {
            workspace_id,
            workspace_root,
            db_path,
            tantivy_path,
            is_primary,
            database: None,
            search_index: None,
            indexing_runtime: None,
        })
    }

    fn workspace_backed_route(
        workspace_id: String,
        workspace_root: PathBuf,
        is_primary: bool,
        workspace: &Arc<JulieWorkspace>,
    ) -> Self {
        Self {
            db_path: workspace.workspace_db_path(&workspace_id),
            tantivy_path: workspace.workspace_tantivy_path(&workspace_id),
            workspace_id,
            workspace_root,
            is_primary,
            database: workspace.db.as_ref().cloned(),
            search_index: workspace.search_index.as_ref().cloned(),
            indexing_runtime: Some(Arc::clone(&workspace.indexing_runtime)),
        }
    }

    async fn open_database_from_path(
        &self,
    ) -> Result<Option<Arc<std::sync::Mutex<SymbolDatabase>>>> {
        if !self.db_path.exists() {
            return Ok(None);
        }

        let db_path = self.db_path.clone();
        let database = tokio::task::spawn_blocking(move || {
            let db = SymbolDatabase::new(db_path)?;
            Ok::<_, anyhow::Error>(Arc::new(std::sync::Mutex::new(db)))
        })
        .await??;

        Ok(Some(database))
    }

    async fn open_search_index_from_path(
        &self,
        create_if_missing: bool,
    ) -> Result<Option<Arc<std::sync::Mutex<SearchIndex>>>> {
        let meta_path = self.tantivy_path.join("meta.json");
        if !create_if_missing && !meta_path.exists() {
            return Ok(None);
        }

        let tantivy_path = self.tantivy_path.clone();
        let db_path = self.db_path.clone();
        let workspace_id = self.workspace_id.clone();
        let search_index = tokio::task::spawn_blocking(move || {
            if create_if_missing {
                std::fs::create_dir_all(&tantivy_path)?;
            } else if !tantivy_path.join("meta.json").exists() {
                return Ok::<_, anyhow::Error>(None);
            }

            let configs = crate::search::LanguageConfigs::load_embedded();
            let open_outcome = if create_if_missing {
                SearchIndex::open_or_create_with_language_configs_outcome(&tantivy_path, &configs)?
            } else {
                SearchIndex::open_with_language_configs_outcome(&tantivy_path, &configs)?
            };

            let repair_required = open_outcome.repair_required();
            let index = open_outcome.into_index();

            if repair_required {
                warn!(
                    "Tantivy index for workspace route '{}' at {} was recreated empty during open; rebuilding projection from canonical SQLite state",
                    workspace_id,
                    tantivy_path.display()
                );

                let mut db = SymbolDatabase::new(&db_path)?;
                let projection = SearchProjection::tantivy(workspace_id.clone());
                projection
                    .repair_recreated_open_if_needed(&mut db, &index, repair_required, None)?;
            }

            Ok::<_, anyhow::Error>(Some(Arc::new(std::sync::Mutex::new(index))))
        })
        .await??;

        Ok(search_index)
    }

    pub(crate) async fn for_current_primary(
        handler: &JulieServerHandler,
    ) -> std::result::Result<Self, IndexRouteError> {
        let binding = handler.require_primary_workspace_binding().map_err(|err| {
            IndexRouteError::new(
                IndexRouteRepairReason::PrimaryBindingUnavailable,
                format!("current primary binding unavailable: {err}"),
            )
        })?;

        match handler.primary_workspace_snapshot().await {
            Ok(snapshot) => {
                let snapshot_binding = snapshot.binding.clone();
                let db_path = {
                    let db = snapshot.database.lock().map_err(|poisoned| {
                        IndexRouteError::new(
                            IndexRouteRepairReason::StorageAnchorUnavailable,
                            format!(
                                "failed to inspect database path for current primary '{}': {}",
                                snapshot_binding.workspace_id,
                                poisoned
                            ),
                        )
                    })?;
                    db.file_path.clone()
                };
                let tantivy_path = db_path
                    .parent()
                    .and_then(|path| path.parent())
                    .map(|workspace_dir| workspace_dir.join("tantivy"))
                    .ok_or_else(|| {
                        IndexRouteError::new(
                            IndexRouteRepairReason::StorageAnchorUnavailable,
                            format!(
                                "failed to derive Tantivy path for current primary '{}' from {}",
                                snapshot_binding.workspace_id,
                                db_path.display()
                            ),
                        )
                    })?;

                Ok(Self {
                    workspace_id: snapshot_binding.workspace_id,
                    workspace_root: snapshot_binding.workspace_root,
                    db_path,
                    tantivy_path,
                    is_primary: true,
                    database: Some(snapshot.database),
                    search_index: snapshot.search_index,
                    indexing_runtime: snapshot.indexing_runtime,
                })
            }
            Err(err) => {
                Self::path_backed_route(
                    handler,
                    binding.workspace_id.clone(),
                    binding.workspace_root.clone(),
                    true,
                )
                .await
                .map_err(|fallback_err| {
                    IndexRouteError::new(
                        IndexRouteRepairReason::StorageAnchorUnavailable,
                        format!(
                            "current primary route unavailable for '{}': {err}; fallback failed: {fallback_err}",
                            binding.workspace_id
                        ),
                    )
                })
            }
        }
    }

    pub(crate) async fn for_workspace_path(
        handler: &JulieServerHandler,
        workspace_path: &Path,
    ) -> std::result::Result<Self, IndexRouteError> {
        let workspace_root = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.to_path_buf());
        let workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace_root.to_string_lossy())
                .map_err(|err| {
                    IndexRouteError::new(
                        IndexRouteRepairReason::WorkspaceIdentityUnavailable,
                        format!(
                            "failed to resolve workspace identity for '{}': {err}",
                            workspace_root.display()
                        ),
                    )
                })?;

        if handler.current_workspace_id().as_deref() == Some(workspace_id.as_str()) {
            return Self::for_current_primary(handler).await;
        }

        if let Some(pool) = handler.workspace_pool.as_ref() {
            if let Some(workspace) = pool.get(&workspace_id).await {
                return Ok(Self::workspace_backed_route(
                    workspace_id,
                    workspace_root,
                    false,
                    &workspace,
                ));
            }
        }

        Self::path_backed_route(handler, workspace_id, workspace_root, false).await
    }

    pub(crate) async fn database_for_read(
        &self,
        _handler: &JulieServerHandler,
    ) -> Result<Option<Arc<std::sync::Mutex<SymbolDatabase>>>> {
        if let Some(database) = &self.database {
            return Ok(Some(Arc::clone(database)));
        }

        self.open_database_from_path().await
    }

    pub(crate) async fn database_for_write(
        &self,
        _handler: &JulieServerHandler,
    ) -> Result<Option<Arc<std::sync::Mutex<SymbolDatabase>>>> {
        if let Some(database) = &self.database {
            return Ok(Some(Arc::clone(database)));
        }

        if self.db_path.exists() {
            return self.open_database_from_path().await;
        }

        if let Some(parent_dir) = self.db_path.parent() {
            std::fs::create_dir_all(parent_dir)?;
        }

        let db_path = self.db_path.clone();
        let database = tokio::task::spawn_blocking(move || {
            let db = SymbolDatabase::new(db_path)?;
            Ok::<_, anyhow::Error>(Arc::new(std::sync::Mutex::new(db)))
        })
        .await??;

        Ok(Some(database))
    }

    pub(crate) async fn search_index_for_write(
        &self,
    ) -> Result<Option<Arc<std::sync::Mutex<SearchIndex>>>> {
        if let Some(search_index) = &self.search_index {
            return Ok(Some(Arc::clone(search_index)));
        }

        self.open_search_index_from_path(true).await
    }
}
