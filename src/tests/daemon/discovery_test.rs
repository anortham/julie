// Tests for DiscoveryRecord, DiscoveryFile, and DiscoveryState.
//
// All tests use tempfile::TempDir for isolated paths; ~/.julie/ is never
// touched.

mod tests {
    use crate::daemon::discovery::{DiscoveryFile, DiscoveryRecord, DiscoveryState};
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn tmp_dir() -> TempDir {
        tempfile::TempDir::new().expect("TempDir::new failed")
    }

    /// Build a synthetic record for the current process, using the supplied
    /// directory as the location for sibling paths (token_path, log_path).
    fn current_process_record(dir: &TempDir) -> DiscoveryRecord {
        DiscoveryRecord::for_current_process(
            "127.0.0.1",
            1234,
            dir.path().join("daemon.token"),
            dir.path().join("daemon.log"),
        )
    }

    // -----------------------------------------------------------------------
    // test_discovery_record_round_trip
    // -----------------------------------------------------------------------

    /// Serialize then deserialize a DiscoveryRecord; every field must survive
    /// the JSON round-trip exactly.
    #[test]
    fn test_discovery_record_round_trip() {
        let dir = tmp_dir();
        let record = current_process_record(&dir);

        let json = serde_json::to_string_pretty(&record).expect("serialize failed");
        let decoded: DiscoveryRecord = serde_json::from_str(&json).expect("deserialize failed");

        assert_eq!(decoded.pid, record.pid);
        assert_eq!(
            decoded.pid_creation_time_micros,
            record.pid_creation_time_micros
        );
        assert_eq!(decoded.host, record.host);
        assert_eq!(decoded.port, record.port);
        assert_eq!(decoded.token_path, record.token_path);
        assert_eq!(decoded.log_path, record.log_path);
        assert_eq!(decoded.daemon_version, record.daemon_version);
        assert_eq!(decoded.protocol_version, record.protocol_version);
        assert_eq!(decoded.schema_version, record.schema_version);
        assert_eq!(decoded.started_at, record.started_at);
    }

    // -----------------------------------------------------------------------
    // test_discovery_state_live
    // -----------------------------------------------------------------------

    /// Write a record for the current process, then validate — must be Live.
    #[test]
    fn test_discovery_state_live() {
        let dir = tmp_dir();
        let discovery_path = dir.path().join("discovery.json");
        let record = current_process_record(&dir);

        DiscoveryFile::write_atomic(&discovery_path, &record).expect("write_atomic failed");

        let state = DiscoveryFile::read_and_validate(&discovery_path);
        match state {
            DiscoveryState::Live(r) => {
                assert_eq!(r.pid, record.pid);
            }
            other => panic!("expected Live, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // test_discovery_atomic_write_durability
    // -----------------------------------------------------------------------

    /// Simulate a crash after fsync-of-tmp but BEFORE rename:
    /// - write the tmp file, fsync it, then abandon it (no rename)
    /// - the reader must see Missing, not Corrupt
    ///
    /// This verifies the atomic-write contract: an interrupted write never
    /// leaves a half-written discovery.json visible to readers.
    #[test]
    fn test_discovery_atomic_write_durability() {
        let dir = tmp_dir();
        let discovery_path = dir.path().join("discovery.json");
        let tmp_path = dir.path().join("discovery.json.tmp");

        let record = current_process_record(&dir);
        let json = serde_json::to_string_pretty(&record).expect("serialize");

        // Simulate crash: write + fsync tmp but do NOT rename to final path.
        {
            let mut f = fs::File::create(&tmp_path).expect("create tmp");
            f.write_all(json.as_bytes()).expect("write tmp");
            f.sync_all().expect("fsync tmp");
        }

        // The .tmp is there but discovery.json is absent.
        assert!(tmp_path.exists(), "tmp file should exist (sanity)");
        assert!(
            !discovery_path.exists(),
            "discovery.json must not exist yet"
        );

        // Reader must return Missing, not Corrupt, because the canonical
        // path is absent.
        let state = DiscoveryFile::read_and_validate(&discovery_path);
        assert!(
            matches!(state, DiscoveryState::Missing),
            "expected Missing after simulated crash, got {:?}",
            state
        );
    }

    // -----------------------------------------------------------------------
    // test_discovery_pid_reuse_defense
    // -----------------------------------------------------------------------

    /// Write a record containing a PID that is known to be dead (a spawned
    /// child process that has been waited), then validate — must be Stale.
    #[test]
    fn test_discovery_pid_reuse_defense() {
        let dir = tmp_dir();
        let discovery_path = dir.path().join("discovery.json");

        // Spawn a child and immediately wait for it so the PID is dead.
        let child = std::process::Command::new("true")
            .spawn()
            .unwrap_or_else(|_| {
                // Fallback on systems that don't have `true` (Windows)
                std::process::Command::new("cmd")
                    .args(["/C", "exit 0"])
                    .spawn()
                    .expect("spawn fallback")
            });
        let dead_pid = child.id();
        // Wait so the PID is truly reaped.
        let mut child = child;
        let _ = child.wait();

        // Construct a record claiming the dead PID is the daemon.
        let mut record = current_process_record(&dir);
        record.pid = dead_pid;
        record.pid_creation_time_micros = 999_999_999_999; // impossible mismatch

        DiscoveryFile::write_atomic(&discovery_path, &record).expect("write_atomic failed");

        let state = DiscoveryFile::read_and_validate(&discovery_path);
        assert!(
            matches!(state, DiscoveryState::Stale),
            "expected Stale for dead PID, got {:?}",
            state
        );
    }

    // -----------------------------------------------------------------------
    // test_discovery_state_corrupt
    // -----------------------------------------------------------------------

    /// Write garbage bytes at the discovery path; reader must return Corrupt.
    #[test]
    fn test_discovery_state_corrupt() {
        let dir = tmp_dir();
        let discovery_path = dir.path().join("discovery.json");

        fs::write(&discovery_path, b"not-valid-json-at-all{{{").expect("write garbage");

        let state = DiscoveryFile::read_and_validate(&discovery_path);
        assert!(
            matches!(state, DiscoveryState::Corrupt(_)),
            "expected Corrupt for garbage bytes, got {:?}",
            state
        );
    }
}
