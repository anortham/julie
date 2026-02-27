//! Tests for EmbeddingProvider trait and OrtEmbeddingProvider implementation.

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use crate::embeddings::{
        EmbeddingConfig, EmbeddingProvider, EmbeddingProviderFactory, OrtEmbeddingProvider,
    };

    /// Helper: create an OrtEmbeddingProvider with a stable cache path.
    fn create_test_provider() -> OrtEmbeddingProvider {
        let cache_dir =
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
                .join(".cache")
                .join("fastembed");

        OrtEmbeddingProvider::try_new(Some(cache_dir))
            .expect("OrtEmbeddingProvider should initialize")
    }

    #[test]
    #[serial(fastembed)]
    fn test_try_new_succeeds() {
        let provider = create_test_provider();
        assert_eq!(provider.dimensions(), 384);
    }

    #[test]
    #[serial(fastembed)]
    fn test_embed_query_returns_correct_dimensions() {
        let provider = create_test_provider();
        let embedding = provider
            .embed_query("function to handle authentication")
            .expect("embed_query should succeed");

        assert_eq!(embedding.len(), 384);

        // Should be unit-normalized
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "Embedding should be unit-normalized, got {norm}"
        );
    }

    #[test]
    #[serial(fastembed)]
    fn test_embed_batch_returns_correct_count() {
        let provider = create_test_provider();
        let texts = vec![
            "class UserService".to_string(),
            "function parseJSON".to_string(),
            "struct DatabaseConnection".to_string(),
        ];

        let embeddings = provider
            .embed_batch(&texts)
            .expect("embed_batch should succeed");

        assert_eq!(embeddings.len(), 3);
        for (i, emb) in embeddings.iter().enumerate() {
            assert_eq!(emb.len(), 384, "Embedding {i} should be 384-dim");
        }
    }

    #[test]
    #[serial(fastembed)]
    fn test_embed_batch_empty_input() {
        let provider = create_test_provider();
        let embeddings = provider
            .embed_batch(&[])
            .expect("empty batch should succeed");

        assert!(embeddings.is_empty());
    }

    #[test]
    #[serial(fastembed)]
    fn test_device_info() {
        let provider = create_test_provider();
        let info = provider.device_info();

        assert!(info.runtime.contains("ort"), "Runtime should mention ort");
        assert_eq!(info.dimensions, 384);
        assert!(
            info.model_name.contains("BGE"),
            "Model name should mention BGE"
        );
    }

    #[test]
    #[serial(fastembed)]
    fn test_semantic_similarity_sanity_check() {
        let provider = create_test_provider();

        let error_handling = provider
            .embed_query("error handling and exception management")
            .unwrap();
        let try_catch = provider
            .embed_query("try catch block for failures")
            .unwrap();
        let database_query = provider
            .embed_query("SQL database query optimization")
            .unwrap();

        // Cosine similarity: dot product of unit vectors
        let sim_related: f32 = error_handling
            .iter()
            .zip(try_catch.iter())
            .map(|(a, b)| a * b)
            .sum();
        let sim_unrelated: f32 = error_handling
            .iter()
            .zip(database_query.iter())
            .map(|(a, b)| a * b)
            .sum();

        // Semantically related texts should have higher similarity
        assert!(
            sim_related > sim_unrelated,
            "Related texts should be more similar: related={sim_related:.4} vs unrelated={sim_unrelated:.4}"
        );
    }

    #[test]
    #[serial(fastembed)]
    fn test_provider_factory_creates_ort_provider() {
        let cache_dir =
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
                .join(".cache")
                .join("fastembed");

        let config = EmbeddingConfig {
            provider: "ort".to_string(),
            cache_dir: Some(cache_dir),
        };

        let provider = EmbeddingProviderFactory::create(&config).unwrap();
        assert_eq!(provider.dimensions(), 384);
    }

    #[test]
    fn test_provider_factory_rejects_unknown_provider() {
        let config = EmbeddingConfig {
            provider: "not-a-real-provider".to_string(),
            cache_dir: None,
        };

        let err = match EmbeddingProviderFactory::create(&config) {
            Ok(_) => panic!("Factory should reject unknown provider"),
            Err(err) => err,
        };
        assert!(
            err.to_string().contains("Unknown embedding provider"),
            "Expected unknown provider error, got: {err}"
        );
    }
}
