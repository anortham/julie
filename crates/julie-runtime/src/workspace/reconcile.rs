use anyhow::Result;
use julie_core::file_policy::detect_language_for_indexing_with_content;
use julie_core::indexing_state::IndexingOperation;
use julie_index::checkout_store::{CheckoutStore, PathChange};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::warn;

pub struct ScannedFile {
    pub path: String,
    pub bytes: Vec<u8>,
    pub hash: String,
    pub language: String,
}

pub struct Reconciliation {
    pub changes: Vec<PathChange>,
    pub unreadable_paths: Vec<String>,
}

pub fn scan_indexable_files(root: &Path, files: &[PathBuf]) -> Result<Vec<ScannedFile>> {
    let mut scanned = Vec::with_capacity(files.len());
    for file_path in files {
        let bytes = std::fs::read(file_path)?;
        let path = relative_path(file_path, root);
        let hash = blake3::hash(&bytes).to_hex().to_string();
        let language = detect_language_for_indexing_with_content(
            Path::new(&path),
            &String::from_utf8_lossy(&bytes),
        );
        scanned.push(ScannedFile {
            path,
            bytes,
            hash,
            language,
        });
    }
    Ok(scanned)
}

pub fn existing_path_hashes(store: &CheckoutStore) -> Result<HashMap<String, String>> {
    let facts = store.current().facts()?;
    let mut map = HashMap::new();
    for row in facts.reader().paths()? {
        map.insert(row.path, row.blob_hash);
    }
    Ok(map)
}

pub fn path_changes(
    scanned: &[ScannedFile],
    existing: &HashMap<String, String>,
    operation: IndexingOperation,
) -> Vec<PathChange> {
    let scanned_paths: HashSet<&str> = scanned.iter().map(|file| file.path.as_str()).collect();
    let mut changes = Vec::new();
    let remove_missing = matches!(
        operation,
        IndexingOperation::Full | IndexingOperation::Incremental | IndexingOperation::CatchUp
    );
    if remove_missing {
        for path in existing.keys() {
            if !scanned_paths.contains(path.as_str()) {
                changes.push(PathChange::Remove { path: path.clone() });
            }
        }
    }
    let skip_unchanged = matches!(
        operation,
        IndexingOperation::Incremental | IndexingOperation::CatchUp
    );
    for file in scanned {
        if skip_unchanged && existing.get(&file.path) == Some(&file.hash) {
            continue;
        }
        changes.push(PathChange::Upsert {
            path: file.path.clone(),
            bytes: file.bytes.clone(),
            language: file.language.clone(),
        });
    }
    changes
}

pub fn reconcile_workspace(store: &CheckoutStore, root: &Path) -> Result<Reconciliation> {
    reconcile_workspace_with(store, root, |path| std::fs::read(path))
}

pub(crate) fn reconcile_workspace_with(
    store: &CheckoutStore,
    root: &Path,
    read: impl Fn(&Path) -> std::io::Result<Vec<u8>>,
) -> Result<Reconciliation> {
    let mut discovered: Vec<_> = julie_core::workspace_scan::scan_workspace_files(root)?
        .into_iter()
        .collect();
    discovered.sort();
    let mut changes = Vec::with_capacity(discovered.len());
    let mut unreadable_paths = Vec::new();
    for path in &discovered {
        match read(&root.join(path)) {
            Ok(bytes) => changes.push(PathChange::Upsert {
                path: path.clone(),
                language: detect_language_for_indexing_with_content(
                    Path::new(path),
                    &String::from_utf8_lossy(&bytes),
                ),
                bytes,
            }),
            Err(err) => {
                warn!(path, error = %err, "retaining unreadable file during reconciliation");
                unreadable_paths.push(path.clone());
            }
        }
    }

    let discovered: HashSet<_> = discovered.into_iter().collect();
    let snapshot = store.current();
    let mut previous: HashSet<String> = existing_path_hashes(store)?.into_keys().collect();
    previous.extend(snapshot.graph().paths().iter().cloned());
    for path in previous {
        if !discovered.contains(&path) {
            changes.push(PathChange::Remove { path });
        }
    }

    Ok(Reconciliation {
        changes,
        unreadable_paths,
    })
}

pub fn relative_path(file_path: &Path, root: &Path) -> String {
    if file_path.is_absolute() {
        julie_core::paths::to_relative_unix_style(file_path, root).unwrap_or_else(|_| {
            file_path
                .to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches('/')
                .to_string()
        })
    } else {
        file_path.to_string_lossy().replace('\\', "/")
    }
}
