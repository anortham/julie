use crate::dashboard::error_buffer::ErrorBuffer;
use tracing::{error, info, warn};
use tracing_subscriber::layer::SubscriberExt;

#[test]
fn test_error_buffer_captures_warn_and_error() {
    let buffer = ErrorBuffer::new(50);
    let layer = buffer.layer();

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        info!("this should not be captured");
        warn!("something looks off");
        error!("something broke");
    });

    let entries = buffer.recent_entries();
    assert_eq!(
        entries.len(),
        2,
        "should have captured 2 entries (warn + error)"
    );

    assert_eq!(entries[0].level, "WARN");
    assert_eq!(entries[0].message, "something looks off");

    assert_eq!(entries[1].level, "ERROR");
    assert_eq!(entries[1].message, "something broke");
}

#[test]
fn test_error_buffer_respects_capacity() {
    let buffer = ErrorBuffer::new(3);
    let layer = buffer.layer();

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        warn!("warning 1");
        warn!("warning 2");
        warn!("warning 3");
        warn!("warning 4");
        warn!("warning 5");
    });

    let entries = buffer.recent_entries();
    assert_eq!(entries.len(), 3, "should only keep last 3 entries");

    // Oldest in deque should be entry 3 (entries 1 and 2 were evicted)
    assert_eq!(entries[0].message, "warning 3");
    assert_eq!(entries[1].message, "warning 4");
    assert_eq!(entries[2].message, "warning 5");
}
