use std::path::{Path, PathBuf};

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

/// Writes the fake sidecar executable script into `dir` and makes it executable.
pub fn write(dir: &Path) -> PathBuf {
    let path = dir.join("fake-sidecar.sh");
    std::fs::write(&path, FAKE_SIDECAR).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}
