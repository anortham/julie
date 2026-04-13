//! Tests for daemon IPC header parsing compatibility.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[cfg(unix)]
    use tokio::io::AsyncWriteExt;

    use crate::daemon::{PrefixedIpcStream, parse_ipc_headers_block, read_ipc_headers};
    use crate::workspace::startup_hint::WorkspaceStartupSource;

    #[test]
    fn test_parse_ipc_headers_block_accepts_legacy_version_header() {
        let headers = parse_ipc_headers_block("WORKSPACE:/tmp/workspace\nVERSION:6.7.0\n")
            .expect("legacy headers should parse");

        assert_eq!(headers.workspace, PathBuf::from("/tmp/workspace"));
        assert_eq!(headers.workspace_source, None);
        assert_eq!(headers.version.as_deref(), Some("6.7.0"));
    }

    #[test]
    fn test_parse_ipc_headers_block_preserves_missing_workspace_source_in_startup_hint() {
        let headers = parse_ipc_headers_block("WORKSPACE:/tmp/workspace\nVERSION:6.7.0\n")
            .expect("legacy headers should parse");

        assert_eq!(headers.workspace_startup_hint().source, None);
    }

    #[test]
    fn test_parse_ipc_headers_block_accepts_workspace_source_header() {
        let headers = parse_ipc_headers_block(
            "WORKSPACE:/tmp/workspace\nWORKSPACE_SOURCE:env\nVERSION:6.7.0\n",
        )
        .expect("headers with workspace source should parse");

        assert_eq!(headers.workspace, PathBuf::from("/tmp/workspace"));
        assert_eq!(headers.workspace_source, Some(WorkspaceStartupSource::Env));
        assert_eq!(headers.version.as_deref(), Some("6.7.0"));
    }

    #[test]
    fn test_parse_ipc_headers_block_accepts_version_before_workspace_source_header() {
        let headers = parse_ipc_headers_block(
            "WORKSPACE:/tmp/workspace\nVERSION:6.7.0\nWORKSPACE_SOURCE:env\n\n",
        )
        .expect("headers with version before workspace source should parse");

        assert_eq!(headers.workspace, PathBuf::from("/tmp/workspace"));
        assert_eq!(headers.workspace_source, Some(WorkspaceStartupSource::Env));
        assert_eq!(headers.version.as_deref(), Some("6.7.0"));
    }

    #[test]
    fn test_parse_ipc_headers_block_accepts_workspace_source_without_version() {
        let headers = parse_ipc_headers_block(
            "WORKSPACE:/tmp/workspace\nWORKSPACE_SOURCE:cwd\n{\"jsonrpc\":\"2.0\"}\n",
        )
        .expect("headers with workspace source and no version should parse");

        assert_eq!(headers.workspace, PathBuf::from("/tmp/workspace"));
        assert_eq!(headers.workspace_source, Some(WorkspaceStartupSource::Cwd));
        assert_eq!(headers.version, None);
    }

    #[test]
    fn test_parse_ipc_headers_block_rejects_unknown_workspace_source() {
        let error = parse_ipc_headers_block(
            "WORKSPACE:/tmp/workspace\nWORKSPACE_SOURCE:nope\nVERSION:6.7.0\n",
        )
        .expect_err("unknown workspace source should fail parsing");

        assert!(
            error
                .to_string()
                .contains("Invalid WORKSPACE_SOURCE header")
        );
    }

    #[test]
    fn test_parse_ipc_headers_block_rejects_internal_unknown_workspace_source_wire_value() {
        let error = parse_ipc_headers_block(
            "WORKSPACE:/tmp/workspace\nWORKSPACE_SOURCE:unknown\nVERSION:6.7.0\n",
        )
        .expect_err("internal unknown workspace source should not parse from wire");

        assert!(
            error
                .to_string()
                .contains("Invalid WORKSPACE_SOURCE header")
        );
    }

    #[test]
    fn test_parse_ipc_headers_block_accepts_future_startup_header_after_version_in_blank_line_mode()
    {
        let headers = parse_ipc_headers_block(
            "WORKSPACE:/tmp/workspace\nWORKSPACE_SOURCE:env\nVERSION:6.7.0\nWORKSPACE_ROOT_HINT:future\n\n",
        )
        .expect("future startup headers in blank-line mode should parse");

        assert_eq!(headers.workspace, PathBuf::from("/tmp/workspace"));
        assert_eq!(headers.workspace_source, Some(WorkspaceStartupSource::Env));
        assert_eq!(headers.version.as_deref(), Some("6.7.0"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_read_ipc_headers_preserves_mcp_bytes_after_workspace_source_header() {
        assert_read_ipc_headers_preserves_mcp_bytes(
            b"WORKSPACE:/tmp/workspace\nWORKSPACE_SOURCE:cwd\n\nContent-Length: 20\r\n\r\n",
            Some(WorkspaceStartupSource::Cwd),
            None,
            b"Content-Length: 20\r\n\r\n",
        )
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_read_ipc_headers_preserves_legacy_mcp_bytes_without_blank_line() {
        assert_read_ipc_headers_preserves_mcp_bytes(
            b"WORKSPACE:/tmp/workspace\nWORKSPACE_SOURCE:cwd\nVERSION:6.7.0\nContent-Length: 20\r\n\r\n",
            Some(WorkspaceStartupSource::Cwd),
            Some("6.7.0"),
            b"Content-Length: 20\r\n\r\n",
        )
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_read_ipc_headers_preserves_legacy_mcp_bytes_without_blank_line_or_optional_headers()
     {
        assert_read_ipc_headers_preserves_mcp_bytes(
            b"WORKSPACE:/tmp/workspace\nContent-Length: 20\r\n\r\n",
            None,
            None,
            b"Content-Length: 20\r\n\r\n",
        )
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_read_ipc_headers_completes_for_blank_line_terminated_header_block() {
        assert_read_ipc_headers_completes_without_mcp_bytes(
            b"WORKSPACE:/tmp/workspace\nWORKSPACE_SOURCE:cwd\nVERSION:6.7.0\n\n",
            Some(WorkspaceStartupSource::Cwd),
            Some("6.7.0"),
        )
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_read_ipc_headers_completes_for_version_then_workspace_source_blank_line_block() {
        assert_read_ipc_headers_completes_without_mcp_bytes(
            b"WORKSPACE:/tmp/workspace\nVERSION:6.7.0\nWORKSPACE_SOURCE:cwd\n\n",
            Some(WorkspaceStartupSource::Cwd),
            Some("6.7.0"),
        )
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_read_ipc_headers_consumes_blank_line_before_mcp_bytes() {
        assert_read_ipc_headers_preserves_mcp_bytes(
            b"WORKSPACE:/tmp/workspace\nWORKSPACE_SOURCE:cwd\nVERSION:6.7.0\n\nContent-Length: 20\r\n\r\n",
            Some(WorkspaceStartupSource::Cwd),
            Some("6.7.0"),
            b"Content-Length: 20\r\n\r\n",
        )
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_read_ipc_headers_accepts_future_startup_header_after_version_before_blank_line() {
        assert_read_ipc_headers_preserves_mcp_bytes(
            b"WORKSPACE:/tmp/workspace\nWORKSPACE_SOURCE:cwd\nVERSION:6.7.0\nWORKSPACE_ROOT_HINT:future\n\nContent-Length: 20\r\n\r\n",
            Some(WorkspaceStartupSource::Cwd),
            Some("6.7.0"),
            b"Content-Length: 20\r\n\r\n",
        )
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_read_ipc_headers_preserves_mcp_bytes_after_version_header() {
        assert_read_ipc_headers_preserves_mcp_bytes(
            b"WORKSPACE:/tmp/workspace\nVERSION:6.7.0\n\nContent-Length: 20\r\n\r\n",
            None,
            Some("6.7.0"),
            b"Content-Length: 20\r\n\r\n",
        )
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_read_ipc_headers_preserves_mcp_bytes_after_version_then_workspace_source_header()
    {
        assert_read_ipc_headers_preserves_mcp_bytes(
            b"WORKSPACE:/tmp/workspace\nVERSION:6.7.0\nWORKSPACE_SOURCE:cwd\n\nContent-Length: 20\r\n\r\n",
            Some(WorkspaceStartupSource::Cwd),
            Some("6.7.0"),
            b"Content-Length: 20\r\n\r\n",
        )
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_read_ipc_headers_handles_split_write_blank_line_block() {
        let (mut client, mut server) = tokio::net::UnixStream::pair().unwrap();

        let writer = tokio::spawn(async move {
            client
                .write_all(b"WORKSPACE:/tmp/workspace\nVERSION:6.7.0\n")
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            client
                .write_all(b"WORKSPACE_SOURCE:cwd\n\nContent-Length: 20\r\n\r\n")
                .await
                .unwrap();
        });

        let parsed = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            read_ipc_headers(&mut server),
        )
        .await
        .expect("header parsing should wait for the rest of the blank-line block")
        .unwrap();

        writer.await.unwrap();

        assert_eq!(parsed.headers.workspace, PathBuf::from("/tmp/workspace"));
        assert_eq!(
            parsed.headers.workspace_source,
            Some(WorkspaceStartupSource::Cwd)
        );
        assert_eq!(parsed.headers.version.as_deref(), Some("6.7.0"));

        let mut stream = PrefixedIpcStream::new(server, parsed.buffered_bytes);
        let mut actual = vec![0; b"Content-Length: 20\r\n\r\n".len()];
        tokio::io::AsyncReadExt::read_exact(&mut stream, &mut actual)
            .await
            .unwrap();
        assert_eq!(actual, b"Content-Length: 20\r\n\r\n");
    }

    #[cfg(unix)]
    async fn assert_read_ipc_headers_preserves_mcp_bytes(
        input: &[u8],
        expected_source: Option<WorkspaceStartupSource>,
        expected_version: Option<&str>,
        expected_mcp_prefix: &[u8],
    ) {
        let (mut client, mut server) = tokio::net::UnixStream::pair().unwrap();

        client.write_all(input).await.unwrap();

        let parsed = read_ipc_headers(&mut server).await.unwrap();
        assert_eq!(parsed.headers.workspace, PathBuf::from("/tmp/workspace"));
        assert_eq!(parsed.headers.workspace_source, expected_source);
        assert_eq!(parsed.headers.version.as_deref(), expected_version);

        let mut stream = PrefixedIpcStream::new(server, parsed.buffered_bytes);
        let mut actual = vec![0; expected_mcp_prefix.len()];
        tokio::io::AsyncReadExt::read_exact(&mut stream, &mut actual)
            .await
            .unwrap();
        assert_eq!(actual, expected_mcp_prefix);
    }

    #[cfg(unix)]
    async fn assert_read_ipc_headers_completes_without_mcp_bytes(
        input: &[u8],
        expected_source: Option<WorkspaceStartupSource>,
        expected_version: Option<&str>,
    ) {
        let (mut client, mut server) = tokio::net::UnixStream::pair().unwrap();

        client.write_all(input).await.unwrap();

        let parsed = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            read_ipc_headers(&mut server),
        )
        .await
        .expect("header parsing should complete without waiting for MCP bytes")
        .unwrap();

        assert_eq!(parsed.headers.workspace, PathBuf::from("/tmp/workspace"));
        assert_eq!(parsed.headers.workspace_source, expected_source);
        assert_eq!(parsed.headers.version.as_deref(), expected_version);
        assert!(parsed.buffered_bytes.is_empty());
    }
}
