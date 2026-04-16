//! Tests for sidecar protocol contracts and validation helpers.

#[cfg(test)]
#[cfg(feature = "embeddings-sidecar")]
mod tests {
    use crate::embeddings::sidecar_protocol::{
        DeviceBackendCapabilities, DeviceBackendCapability, DeviceLoadPolicy, EmbedBatchResult,
        EmbedQueryResult, HealthResult, ProtocolError, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA,
        SIDECAR_PROTOCOL_VERSION, validate_batch_response, validate_health_response,
        validate_query_response, validate_response_envelope,
    };

    /// Test dimension value — matches BGE-small default but is just a test constant.
    const TEST_DIMS: usize = 384;

    fn ok_query() -> EmbedQueryResult {
        EmbedQueryResult {
            dims: TEST_DIMS,
            vector: vec![0.0; TEST_DIMS],
        }
    }

    fn ok_batch(count: usize) -> EmbedBatchResult {
        EmbedBatchResult {
            dims: TEST_DIMS,
            vectors: vec![vec![0.0; TEST_DIMS]; count],
        }
    }

    fn ok_health() -> HealthResult {
        HealthResult {
            ready: true,
            dims: Some(TEST_DIMS),
            device: Some("cpu".to_string()),
            runtime: Some("fake-runtime".to_string()),
            model_id: Some("fake-model".to_string()),
            resolved_backend: Some("sidecar".to_string()),
            accelerated: Some(false),
            degraded_reason: None,
            capabilities: Some(DeviceBackendCapabilities {
                cpu: DeviceBackendCapability { available: true },
                cuda: DeviceBackendCapability { available: false },
                directml: DeviceBackendCapability { available: false },
                mps: DeviceBackendCapability { available: false },
            }),
            load_policy: Some(DeviceLoadPolicy {
                requested_device_backend: "cpu".to_string(),
                resolved_device_backend: "cpu".to_string(),
            }),
        }
    }

    fn ok_envelope<T>(request_id: &str, result: T) -> ResponseEnvelope<T> {
        ResponseEnvelope {
            schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
            version: SIDECAR_PROTOCOL_VERSION,
            request_id: request_id.to_string(),
            result: Some(result),
            error: None,
        }
    }

    #[test]
    fn test_validate_query_response_accepts_expected_shape() {
        let resp = ok_query();
        assert!(validate_query_response(&resp, TEST_DIMS).is_ok());
    }

    #[test]
    fn test_validate_query_response_rejects_vector_length_mismatch_with_clear_error() {
        let resp = EmbedQueryResult {
            dims: TEST_DIMS,
            vector: vec![0.0; TEST_DIMS - 1],
        };

        let err = validate_query_response(&resp, TEST_DIMS).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("vector length mismatch") && message.contains("expected"),
            "expected useful query mismatch message, got: {message}"
        );
    }

    #[test]
    fn test_validate_query_response_rejects_dimension_mismatch_with_clear_error() {
        let resp = EmbedQueryResult {
            dims: TEST_DIMS + 1,
            vector: vec![0.0; TEST_DIMS],
        };

        let err = validate_query_response(&resp, TEST_DIMS).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("dimension mismatch") && message.contains("expected"),
            "expected useful query dimension mismatch message, got: {message}"
        );
    }

    #[test]
    fn test_validate_batch_response_accepts_expected_shape() {
        let resp = ok_batch(2);
        assert!(validate_batch_response(&resp, 2, TEST_DIMS).is_ok());
    }

    #[test]
    fn test_embed_batch_response_rejects_dimension_mismatch() {
        let resp = EmbedBatchResult {
            dims: 768,
            vectors: vec![vec![0.0; 768]],
        };

        let err = validate_batch_response(&resp, 1, TEST_DIMS).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("dimension mismatch") && message.contains("expected"),
            "expected useful batch dimension mismatch message, got: {message}"
        );
    }

    #[test]
    fn test_embed_batch_response_rejects_count_mismatch() {
        let resp = EmbedBatchResult {
            dims: TEST_DIMS,
            vectors: vec![],
        };

        let err = validate_batch_response(&resp, 1, TEST_DIMS).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("count mismatch") && message.contains("expected 1"),
            "expected useful batch count mismatch message, got: {message}"
        );
    }

    #[test]
    fn test_validate_batch_response_rejects_vector_length_mismatch_with_clear_error() {
        let resp = EmbedBatchResult {
            dims: TEST_DIMS,
            vectors: vec![vec![0.0; TEST_DIMS - 1]],
        };

        let err = validate_batch_response(&resp, 1, TEST_DIMS).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("vector length mismatch") && message.contains("index 0"),
            "expected useful batch vector-length mismatch message, got: {message}"
        );
    }

    #[test]
    fn test_validate_health_response_accepts_ready_runtime_metadata() {
        let resp = ok_health();
        assert!(validate_health_response(&resp).is_ok());
    }

    #[test]
    fn test_validate_health_response_rejects_ready_health_without_dims() {
        let resp = HealthResult {
            dims: None,
            ..ok_health()
        };

        let err = validate_health_response(&resp).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("dims") && message.contains("ready"),
            "expected clear health validation error, got: {message}"
        );
    }

    #[test]
    fn test_validate_health_response_accepts_degraded_runtime_with_reason() {
        let resp = HealthResult {
            ready: false,
            dims: Some(TEST_DIMS),
            degraded_reason: Some("DirectML unavailable, fell back to CPU".to_string()),
            ..ok_health()
        };

        assert!(validate_health_response(&resp).is_ok());
    }

    #[test]
    fn test_validate_health_response_accepts_structured_capabilities_and_load_policy() {
        let resp = HealthResult {
            capabilities: Some(DeviceBackendCapabilities {
                cpu: DeviceBackendCapability { available: true },
                cuda: DeviceBackendCapability { available: true },
                directml: DeviceBackendCapability { available: true },
                mps: DeviceBackendCapability { available: false },
            }),
            load_policy: Some(DeviceLoadPolicy {
                requested_device_backend: "directml".to_string(),
                resolved_device_backend: "cpu".to_string(),
            }),
            degraded_reason: Some("probe encode failed on directml, fell back to CPU".to_string()),
            ..ok_health()
        };

        assert!(validate_health_response(&resp).is_ok());
    }

    #[test]
    fn test_validate_health_response_rejects_device_backend_fallback_without_reason() {
        let resp = HealthResult {
            load_policy: Some(DeviceLoadPolicy {
                requested_device_backend: "directml".to_string(),
                resolved_device_backend: "cpu".to_string(),
            }),
            degraded_reason: None,
            ..ok_health()
        };

        let err = validate_health_response(&resp).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("degraded_reason")
                && message.contains("requested_device_backend")
                && message.contains("resolved_device_backend"),
            "expected clear load-policy validation error, got: {message}"
        );
    }

    #[test]
    fn test_validate_health_response_rejects_blank_requested_device_backend() {
        let resp = HealthResult {
            load_policy: Some(DeviceLoadPolicy {
                requested_device_backend: "".to_string(),
                resolved_device_backend: "cpu".to_string(),
            }),
            ..ok_health()
        };

        let err = validate_health_response(&resp).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("requested_device_backend"),
            "expected clear requested-backend validation error, got: {message}"
        );
    }

    #[test]
    fn test_validate_response_envelope_accepts_valid_envelope() {
        let env = ok_envelope("req-1", ok_query());
        assert!(validate_response_envelope(&env, "req-1").is_ok());
    }

    #[test]
    fn test_validate_response_envelope_rejects_when_both_result_and_error_set() {
        let env = ResponseEnvelope {
            schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
            version: SIDECAR_PROTOCOL_VERSION,
            request_id: "req-1".to_string(),
            result: Some(ok_query()),
            error: Some(ProtocolError {
                code: "internal".to_string(),
                message: "boom".to_string(),
            }),
        };

        let err = validate_response_envelope(&env, "req-1").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("exactly one")
                && message.contains("result")
                && message.contains("error"),
            "expected clear envelope invariant error, got: {message}"
        );
    }

    #[test]
    fn test_validate_response_envelope_rejects_when_neither_result_nor_error_set() {
        let env = ResponseEnvelope::<EmbedQueryResult> {
            schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
            version: SIDECAR_PROTOCOL_VERSION,
            request_id: "req-1".to_string(),
            result: None,
            error: None,
        };

        let err = validate_response_envelope(&env, "req-1").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("exactly one")
                && message.contains("result")
                && message.contains("error"),
            "expected clear envelope invariant error, got: {message}"
        );
    }

    #[test]
    fn test_validate_response_envelope_rejects_schema_mismatch() {
        let env = ResponseEnvelope {
            schema: "wrong.schema".to_string(),
            version: SIDECAR_PROTOCOL_VERSION,
            request_id: "req-1".to_string(),
            result: Some(ok_query()),
            error: None,
        };

        let err = validate_response_envelope(&env, "req-1").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("schema") && message.contains(SIDECAR_PROTOCOL_SCHEMA),
            "expected clear schema mismatch error, got: {message}"
        );
    }

    #[test]
    fn test_validate_response_envelope_rejects_version_mismatch() {
        let env = ResponseEnvelope {
            schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
            version: SIDECAR_PROTOCOL_VERSION + 1,
            request_id: "req-1".to_string(),
            result: Some(ok_query()),
            error: None,
        };

        let err = validate_response_envelope(&env, "req-1").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("version") && message.contains("expected"),
            "expected clear version mismatch error, got: {message}"
        );
    }

    #[test]
    fn test_validate_response_envelope_rejects_request_id_mismatch() {
        let env = ok_envelope("req-actual", ok_query());

        let err = validate_response_envelope(&env, "req-expected").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("request_id")
                && message.contains("req-expected")
                && message.contains("req-actual"),
            "expected clear request id mismatch error, got: {message}"
        );
    }
}
