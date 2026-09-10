// Tests for `workspace::JulieWorkspace` extracted from the implementation module.

use crate::embeddings::{
    DeviceInfo, EmbeddingBackend, EmbeddingProvider, EmbeddingRequestBudget, EmbeddingRuntimeStatus,
    EncoderIdentity,
};
use crate::handler::JulieServerHandler;
use crate::startup::run_primary_workspace_repair;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::tools::workspace::indexing::engine_version::{
    SEMANTIC_INDEX_ENGINE_COMPONENT, SEMANTIC_INDEX_ENGINE_VERSION,
};
use crate::workspace::JulieWorkspace;
use serial_test::serial;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, atomic::AtomicUsize};
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct NoopEmbeddingProvider;

impl EmbeddingProvider for NoopEmbeddingProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.1_f32; 384])
    }

    fn embed_batch(&self, texts: &[String], _budget: &EmbeddingRequestBudget) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1_f32; 384]).collect())
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn encoder_identity(&self) -> anyhow::Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock("noop", 384))
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "pytorch-sidecar".to_string(),
            device: "cpu".to_string(),
            model_name: "noop".to_string(),
            dimensions: 384,
        }
    }
}

#[derive(Default)]
struct BatchMarkerEmbeddingProvider {
    calls: AtomicUsize,
}

impl EmbeddingProvider for BatchMarkerEmbeddingProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.0_f32; 384])
    }

    fn embed_batch(&self, texts: &[String], _budget: &EmbeddingRequestBudget) -> anyhow::Result<Vec<Vec<f32>>> {
        let marker = (self.calls.fetch_add(1, Ordering::SeqCst) + 1) as f32;
        Ok(texts
            .iter()
            .map(|_| {
                let mut vector = vec![0.0_f32; 384];
                vector[0] = marker;
                vector
            })
            .collect())
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn encoder_identity(&self) -> anyhow::Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock("batch-marker", 384))
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "pytorch-sidecar".to_string(),
            device: "cpu".to_string(),
            model_name: "batch-marker".to_string(),
            dimensions: 384,
        }
    }
}

fn extract_text_from_result(result: &impl crate::mcp_compat::AsCallToolResult) -> String {
    crate::mcp_compat::call_tool_result_text(result)
}

async fn wait_for_embedding_tasks_to_finish(handler: &JulieServerHandler) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let tasks = handler.embedding_tasks.lock().await;
        if tasks.is_empty() {
            break;
        }
        drop(tasks);
        assert!(
            Instant::now() < deadline,
            "Embedding task did not complete within 5s"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn embedding_count_for_primary(handler: &JulieServerHandler) -> i64 {
    let workspace = handler
        .get_workspace()
        .await
        .unwrap()
        .expect("workspace should be initialized");
    let db = workspace.db.as_ref().expect("workspace db should exist");
    db.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .embedding_count()
        .unwrap()
}

async fn first_embedding_value_for_symbol(handler: &JulieServerHandler, name: &str) -> f32 {
    let workspace = handler
        .get_workspace()
        .await
        .unwrap()
        .expect("workspace should be initialized");
    let db = workspace.db.as_ref().expect("workspace db should exist");
    let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let symbol = db
        .find_symbols_by_name(name)
        .unwrap()
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("symbol {name} should exist"));
    db.get_embedding(&symbol.id)
        .unwrap()
        .unwrap_or_else(|| panic!("symbol {name} should have an embedding"))
        .first()
        .copied()
        .expect("embedding should not be empty")
}
