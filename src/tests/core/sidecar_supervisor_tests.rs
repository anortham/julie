//! Tests for sidecar supervisor configuration, launch config, and utility functions.

#[cfg(test)]
#[cfg(feature = "embeddings-sidecar")]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::Path;

    use crate::embeddings::sidecar_supervisor::{
        INSTALL_MARKER_VERSION, RUNTIME_EDITABLE_REQUIREMENT, SIDECAR_ROOT_ENV,
        SUPPORTED_PYTHON_MINORS, build_program_override_launch_config, build_sidecar_launch_config,
        install_marker_value, is_truthy_env_flag, python_version_from_program,
    };

    struct EnvGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &OsStr) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }

        fn remove(key: &'static str) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::remove_var(key) };
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[cfg(unix)]
    fn write_executable(path: &Path, contents: &str) {
        use std::os::unix::fs::PermissionsExt;

        std::fs::write(path, contents).expect("write executable");
        let mut permissions = std::fs::metadata(path)
            .expect("executable metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("make executable");
    }

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

    #[cfg(unix)]
    #[test]
    #[serial_test::serial(embedding_env)]
    fn test_managed_sidecar_bootstrap_is_serialized_across_concurrent_callers() {
        use std::sync::{Arc, Barrier};

        let tmp = tempfile::tempdir().expect("temp dir");
        let bin_dir = tmp.path().join("bin");
        let cache_dir = tmp.path().join("cache");
        let sidecar_root = tmp.path().join("sidecar-root");
        let venv_bin = cache_dir
            .join("embeddings")
            .join("sidecar")
            .join("venv")
            .join("bin");
        let log_path = tmp.path().join("uv.log");

        std::fs::create_dir_all(&bin_dir).expect("fake bin dir");
        std::fs::create_dir_all(&sidecar_root).expect("fake sidecar root");
        std::fs::create_dir_all(&venv_bin).expect("fake venv bin");
        std::fs::write(
            sidecar_root.join("pyproject.toml"),
            "[project]\nname = \"fake-sidecar\"\nversion = \"0.0.0\"\n",
        )
        .expect("fake pyproject");

        let fake_python = venv_bin.join("python");
        write_executable(
            &fake_python,
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "Python 3.12.1"
  exit 0
fi
if [ "$1" = "-c" ]; then
  echo "2.11.0"
  exit 0
fi
echo "unexpected fake python invocation: $@" >&2
exit 1
"#,
        );

        write_executable(
            &bin_dir.join("uv"),
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "uv 0.0-test"
  exit 0
fi
if [ "$1" = "pip" ] && [ "$2" = "install" ]; then
  echo "$@" >> "$JULIE_TEST_BOOTSTRAP_LOG"
  sleep 0.2
  exit 0
fi
echo "unexpected fake uv invocation: $@" >&2
exit 1
"#,
        );
        write_executable(&bin_dir.join("nvidia-smi"), "#!/bin/sh\nexit 0\n");

        let previous_path = std::env::var_os("PATH").unwrap_or_default();
        let path_value = std::env::join_paths(
            std::iter::once(bin_dir.as_path().to_path_buf())
                .chain(std::env::split_paths(&previous_path)),
        )
        .expect("test PATH should join");

        let _path_guard = EnvGuard::set("PATH", path_value.as_os_str());
        let _cache_guard = EnvGuard::set("JULIE_EMBEDDING_CACHE_DIR", cache_dir.as_os_str());
        let _root_guard = EnvGuard::set(SIDECAR_ROOT_ENV, sidecar_root.as_os_str());
        let _log_guard = EnvGuard::set("JULIE_TEST_BOOTSTRAP_LOG", log_path.as_os_str());
        let _venv_override_guard = EnvGuard::remove("JULIE_EMBEDDING_SIDECAR_VENV");
        let _program_guard = EnvGuard::remove("JULIE_EMBEDDING_SIDECAR_PROGRAM");
        let _script_guard = EnvGuard::remove("JULIE_EMBEDDING_SIDECAR_SCRIPT");
        let _module_guard = EnvGuard::remove("JULIE_EMBEDDING_SIDECAR_MODULE");
        let _raw_program_guard = EnvGuard::remove("JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM");

        let caller_count = 8;
        let barrier = Arc::new(Barrier::new(caller_count));
        let handles = (0..caller_count)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    build_sidecar_launch_config().expect("launch config should build")
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            let launch = handle.join().expect("bootstrap caller should not panic");
            assert_eq!(launch.program, fake_python);
        }

        let log = std::fs::read_to_string(&log_path).expect("bootstrap log should exist");
        let base_installs = log
            .lines()
            .filter(|line| line.contains("--editable"))
            .count();
        let cuda_installs = log
            .lines()
            .filter(|line| line.contains("--reinstall-package torch"))
            .count();

        assert_eq!(
            base_installs, 1,
            "sidecar package install should run once across concurrent callers; log:\n{log}"
        );
        let expected_cuda_installs = if cfg!(target_os = "linux") { 1 } else { 0 };
        assert_eq!(
            cuda_installs, expected_cuda_installs,
            "CUDA torch reinstall should run once on Linux and be skipped elsewhere; log:\n{log}"
        );
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
