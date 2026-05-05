use crate::watcher::events::process_file_system_event_with_pause;
use crate::watcher::filtering;
use crate::watcher::types::FileChangeEvent;
use ignore::gitignore::Gitignore;
use notify::event::ModifyKind;
use notify::{Event, EventKind};
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::Mutex as TokioMutex;

fn rs_extensions() -> HashSet<String> {
    let mut extensions = HashSet::new();
    extensions.insert("rs".to_string());
    extensions
}

fn gitignore_for(path: &std::path::Path) -> Gitignore {
    filtering::build_gitignore_matcher(path).expect("gitignore matcher should build")
}

#[tokio::test]
async fn test_paused_event_ingestion_sets_rescan_without_queueing() {
    let temp_dir = tempfile::tempdir().unwrap();
    let queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));
    let needs_rescan = Arc::new(AtomicBool::new(false));
    let pause_flag = AtomicBool::new(true);
    let paused_event_count = AtomicUsize::new(0);
    let extensions = rs_extensions();
    let gitignore = gitignore_for(temp_dir.path());

    let relevant_file = temp_dir.path().join("paused.rs");
    fs::write(&relevant_file, "fn paused() {}").unwrap();

    process_file_system_event_with_pause(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![relevant_file.clone()],
            attrs: Default::default(),
        },
        &needs_rescan,
        &pause_flag,
        Some(&paused_event_count),
    )
    .await
    .unwrap();

    assert_eq!(
        queue.lock().await.len(),
        0,
        "paused watcher should not enqueue relevant events",
    );
    assert!(
        needs_rescan.load(Ordering::Acquire),
        "paused relevant event should request one repair scan",
    );
    assert_eq!(
        paused_event_count.load(Ordering::Acquire),
        1,
        "paused relevant event should increment paused-event counter",
    );

    // Ignored files should stay ignored during paused ingestion.
    needs_rescan.store(false, Ordering::Release);
    let ignored_file = temp_dir.path().join("pnpm-lock.yaml");
    fs::write(&ignored_file, "lockfileVersion: '9.0'\n").unwrap();

    process_file_system_event_with_pause(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![ignored_file],
            attrs: Default::default(),
        },
        &needs_rescan,
        &pause_flag,
        Some(&paused_event_count),
    )
    .await
    .unwrap();

    assert_eq!(
        queue.lock().await.len(),
        0,
        "ignored paused events should not enqueue",
    );
    assert!(
        !needs_rescan.load(Ordering::Acquire),
        "ignored paused event should not request rescan",
    );
    assert_eq!(
        paused_event_count.load(Ordering::Acquire),
        1,
        "ignored paused event should not increment paused-event counter",
    );
}
