use std::time::Duration;

use julie_core::embeddings_contract::{EmbeddingProvider, EmbeddingRequestBudget};

use crate::embeddings::native::child::SidecarChild;

const FAKE_SIDECAR: &str = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"method":"health"'*)
      printf '{"schema":"julie.embedding.sidecar","version":1,"request_id":"%s","result":{"ready":true,"model_id":"fake","model_sha256":"%s","dims":3,"pooling":"cls","normalization":"l2","instruction_policy_version":1,"llama_cpp_build":"fake-build","device":"cpu","runtime":"fake","accelerated":false}}\n' "$id" "$(printf 'a%.0s' $(seq 64))" ;;
    *'"method":"embed_query"'*)
      printf '{"schema":"julie.embedding.sidecar","version":1,"request_id":"%s","result":{"vector":[1.0,0.0,0.0],"dims":3}}\n' "$id" ;;
    *'"method":"embed_batch"'*)
      printf '{"schema":"julie.embedding.sidecar","version":1,"request_id":"%s","result":{"vectors":[[1.0,0.0,0.0]],"dims":3}}\n' "$id" ;;
    *'"method":"shutdown"'*) exit 0 ;;
    *'"method":"die"'*) exit 7 ;;
  esac
done
"#;

fn fake_sidecar(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("fake-sidecar.sh");
    std::fs::write(&path, FAKE_SIDECAR).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn child_answers_health_and_embeds_over_stdio() {
    let dir = tempfile::tempdir().unwrap();
    let exe = fake_sidecar(dir.path());
    let mut child = SidecarChild::spawn(&exe, "fake").unwrap();
    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(5));
    let health = child.health(&budget).unwrap();
    assert_eq!(health.dims, Some(3));
    let vector: Vec<f32> = child.embed_query("hello", &budget).unwrap();
    assert_eq!(vector, vec![1.0, 0.0, 0.0]);
}

#[cfg(unix)]
#[test]
fn provider_respawns_the_child_after_it_exits() {
    let dir = tempfile::tempdir().unwrap();
    let exe = fake_sidecar(dir.path());
    let provider = crate::embeddings::native::NativeEmbeddingProvider::try_new(
        &crate::embeddings::EmbeddingConfig {
            provider: "native".into(),
            cache_dir: Some(dir.path().to_path_buf()),
            native_program: Some(exe),
            native_model: Some("fake".into()),
        },
    )
    .unwrap();
    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(5));
    let first_pid = provider.child_pid().unwrap();
    provider.kill_child_for_test();
    let vector = provider.embed_query("again", &budget).unwrap();
    assert_eq!(vector.len(), 3);
    assert_ne!(provider.child_pid().unwrap(), first_pid);
}

#[cfg(unix)]
#[test]
fn child_request_respects_the_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sleepy.sh");
    std::fs::write(&path, "#!/bin/sh\nsleep 30\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = SidecarChild::spawn(&path, "fake").unwrap();
    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(200));
    let started = std::time::Instant::now();
    assert!(child.health(&budget).is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
}
