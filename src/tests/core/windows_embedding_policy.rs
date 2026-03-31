//! Focused tests for Windows embedding backend policy.

#[cfg(test)]
mod tests {
    use crate::embeddings::{
        BackendResolverCapabilities, EmbeddingBackend, resolve_backend_preference,
    };

    #[test]
    fn test_windows_auto_resolves_to_sidecar() {
        let capabilities = BackendResolverCapabilities {
            sidecar_available: true,
            target_os: "windows",
            target_arch: "x86_64",
        };

        let resolved = resolve_backend_preference(EmbeddingBackend::Auto, &capabilities)
            .expect("windows auto should resolve cleanly");

        assert_eq!(resolved, EmbeddingBackend::Sidecar);
    }

    #[test]
    fn test_windows_explicit_sidecar_is_allowed() {
        let capabilities = BackendResolverCapabilities {
            sidecar_available: true,
            target_os: "windows",
            target_arch: "x86_64",
        };

        let resolved = resolve_backend_preference(EmbeddingBackend::Sidecar, &capabilities)
            .expect("windows explicit sidecar should be allowed");

        assert_eq!(resolved, EmbeddingBackend::Sidecar);
    }

    #[test]
    fn test_windows_auto_fails_gracefully_when_no_backend_compiled() {
        let capabilities = BackendResolverCapabilities {
            sidecar_available: false,
            target_os: "windows",
            target_arch: "x86_64",
        };

        let err = resolve_backend_preference(EmbeddingBackend::Auto, &capabilities).unwrap_err();
        assert!(
            err.to_string().contains("No embedding backend available"),
            "expected no-backend error, got: {err}"
        );
    }
}
