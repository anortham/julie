//! Tests for adapter IPC handshake header construction.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::adapter::build_ipc_header;
    use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

    #[test]
    fn test_build_ipc_header_includes_workspace_source() {
        let hint = WorkspaceStartupHint {
            path: PathBuf::from("/tmp/workspace"),
            source: Some(WorkspaceStartupSource::Env),
        };

        let header = build_ipc_header(&hint);

        assert!(header.contains("WORKSPACE:/tmp/workspace\n"));
        assert!(header.contains(&format!("VERSION:{}\n", env!("CARGO_PKG_VERSION"))));
        assert!(header.contains("WORKSPACE_SOURCE:env\n"));
        assert!(header.ends_with("\n\n"));

        let expected = format!(
            "WORKSPACE:/tmp/workspace\nVERSION:{}\nWORKSPACE_SOURCE:env\n\n",
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(header, expected);
    }

    #[test]
    fn test_build_ipc_header_omits_internal_unknown_workspace_source() {
        let hint = WorkspaceStartupHint {
            path: PathBuf::from("/tmp/workspace"),
            source: None,
        };

        let header = build_ipc_header(&hint);

        assert!(header.contains("WORKSPACE:/tmp/workspace\n"));
        assert!(header.contains(&format!("VERSION:{}\n", env!("CARGO_PKG_VERSION"))));
        assert!(!header.contains("WORKSPACE_SOURCE:"));
        assert!(header.ends_with("\n\n"));

        let expected = format!(
            "WORKSPACE:/tmp/workspace\nVERSION:{}\n\n",
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(header, expected);
    }
}
