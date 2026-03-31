//! Tests for sidecar supervisor configuration, launch config, and utility functions.

#[cfg(test)]
#[cfg(feature = "embeddings-sidecar")]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use crate::embeddings::sidecar_supervisor::{
        INSTALL_MARKER_VERSION, RUNTIME_EDITABLE_REQUIREMENT, SUPPORTED_PYTHON_MINORS,
        build_program_override_launch_config, install_marker_value, is_truthy_env_flag,
        python_version_from_program,
    };

    #[test]
    fn test_install_marker_value_includes_version_and_root() {
        let marker = install_marker_value(Path::new("/tmp/sidecar"));
        assert!(marker.contains(&format!("version={INSTALL_MARKER_VERSION}")));
        assert!(marker.contains("root=/tmp/sidecar"));
    }

    #[test]
    fn test_runtime_editable_requirement_targets_runtime_extras() {
        assert_eq!(RUNTIME_EDITABLE_REQUIREMENT, ".[runtime]");
    }

    #[test]
    fn test_supported_python_minors_covers_pytorch_range() {
        // PyTorch supports 3.10-3.13 as of early 2026.
        assert!(SUPPORTED_PYTHON_MINORS.contains(&10));
        assert!(SUPPORTED_PYTHON_MINORS.contains(&11));
        assert!(SUPPORTED_PYTHON_MINORS.contains(&12));
        assert!(SUPPORTED_PYTHON_MINORS.contains(&13));
        // 3.14+ is not supported yet.
        assert!(!SUPPORTED_PYTHON_MINORS.contains(&14));
    }

    #[test]
    fn test_python_version_parses_current_interpreter() {
        // Smoke-test that python_version_from_program can parse whatever
        // Python is on PATH. We only care that parsing works, not that the
        // version is new enough for PyTorch (that's the bootstrap's job).
        let candidates: &[&str] = if cfg!(target_os = "windows") {
            &["py", "python"]
        } else {
            &["python3", "python"]
        };
        for &name in candidates {
            if let Some((major, minor)) = python_version_from_program(OsStr::new(name)) {
                assert_eq!(major, 3, "Expected Python 3.x");
                assert!(minor >= 6, "Expected Python 3.6+, got 3.{minor}");
                return;
            }
        }
        // No Python found - skip rather than fail (CI might not have Python).
    }

    #[test]
    fn test_program_override_raw_mode_uses_no_implicit_args() {
        let launch = build_program_override_launch_config(
            Path::new("/usr/bin/env").to_path_buf(),
            None,
            "custom.module",
            Path::new("/tmp/sidecar"),
            true,
        )
        .expect("raw override launch should build");

        assert_eq!(launch.program, Path::new("/usr/bin/env"));
        assert!(
            launch.args.is_empty(),
            "raw mode should not add implicit args: {:?}",
            launch.args
        );
        assert!(
            launch.env.is_empty(),
            "raw mode should not inject env vars: {:?}",
            launch.env
        );
    }

    #[test]
    fn test_program_override_without_raw_mode_keeps_python_entrypoint_args() {
        let launch = build_program_override_launch_config(
            Path::new("/usr/bin/env").to_path_buf(),
            None,
            "custom.module",
            Path::new("/tmp/sidecar"),
            false,
        )
        .expect("override launch should build");

        assert_eq!(launch.program, Path::new("/usr/bin/env"));
        assert_eq!(
            launch.args,
            vec!["-m".to_string(), "custom.module".to_string()]
        );
        assert_eq!(launch.env.len(), 1, "expected PYTHONPATH to be injected");
        assert_eq!(launch.env[0].0, "PYTHONPATH");
    }

    #[test]
    fn test_is_truthy_env_flag_accepts_expected_values() {
        for value in ["1", " true ", "TRUE", "on", "On"] {
            assert!(
                is_truthy_env_flag(value),
                "expected value '{value}' to be truthy"
            );
        }
    }

    #[test]
    fn test_is_truthy_env_flag_rejects_non_truthy_values() {
        for value in ["", "0", "false", "off", "yes"] {
            assert!(
                !is_truthy_env_flag(value),
                "expected value '{value}' to be non-truthy"
            );
        }
    }

    #[test]
    fn test_detect_cuda_from_nvidia_smi_is_idempotent() {
        // Result is hardware-dependent, but must be consistent across calls
        let first = crate::embeddings::sidecar_bootstrap::detect_nvidia_cuda();
        let second = crate::embeddings::sidecar_bootstrap::detect_nvidia_cuda();
        assert_eq!(first, second, "CUDA detection should be idempotent");
    }

    #[test]
    fn test_cuda_torch_index_url() {
        let url = crate::embeddings::sidecar_bootstrap::cuda_torch_index_url();
        assert!(url.starts_with("https://download.pytorch.org/whl/cu"));
    }
}
