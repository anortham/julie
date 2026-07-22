use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

#[test]
fn dispatch_tests_invalid_coverage_command_does_not_clean_before_validation() {
    for args in [
        &["test", "missing", "--coverage"][..],
        &["test", "bucket", "missing", "--coverage"][..],
    ] {
        let fixture = DispatchFixture::new();

        let output = fixture.run_xtask(args, "");

        assert!(
            !output.status.success(),
            "invalid command unexpectedly passed: {args:?}\nstdout:\n{}\nstderr:\n{}",
            output.stdout_text(),
            output.stderr_text()
        );
        assert_eq!(
            fixture.cargo_calls(),
            Vec::<String>::new(),
            "invalid coverage command must validate before cleaning: {args:?}"
        );
    }
}

#[test]
fn dispatch_tests_changed_coverage_cleans_runs_buckets_and_reports() {
    let fixture = DispatchFixture::new();

    let output = fixture.run_xtask(&["test", "changed", "--coverage"], "xtask/src/main.rs");

    assert!(
        output.status.success(),
        "changed coverage command failed\nstdout:\n{}\nstderr:\n{}",
        output.stdout_text(),
        output.stderr_text()
    );

    let calls = fixture.cargo_calls();
    assert_eq!(
        calls.first().map(String::as_str),
        Some("cargo llvm-cov clean --workspace"),
        "coverage run must clean before accumulating coverage"
    );
    assert!(
        calls
            .iter()
            .any(|call| call == "cargo llvm-cov --no-report nextest --no-run -p xtask"),
        "coverage run must prebuild through llvm-cov; calls: {calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|call| call == "cargo llvm-cov --no-report nextest -p xtask"),
        "changed bucket command must run with coverage=true; calls: {calls:?}"
    );
    assert_eq!(
        calls.last().map(String::as_str),
        Some("cargo llvm-cov report --html"),
        "coverage run must report after selected buckets complete"
    );
}

#[test]
fn dispatch_tests_changed_coverage_with_no_changes_does_not_clean_or_report() {
    let fixture = DispatchFixture::new();

    let output = fixture.run_xtask(&["test", "changed", "--coverage"], "");

    assert!(
        output.status.success(),
        "changed coverage no-change command failed\nstdout:\n{}\nstderr:\n{}",
        output.stdout_text(),
        output.stderr_text()
    );
    assert_eq!(
        fixture.cargo_calls(),
        Vec::<String>::new(),
        "no selected buckets means no coverage lifecycle commands"
    );
}

struct DispatchFixture {
    _temp_dir: TempDir,
    bin_dir: PathBuf,
    log_path: PathBuf,
}

impl DispatchFixture {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("create dispatch temp dir");
        let bin_dir = temp_dir.path().join("bin");
        fs::create_dir(&bin_dir).expect("create fake tool bin dir");
        let log_path = temp_dir.path().join("cargo.log");

        write_fake_cargo(&bin_dir);
        write_fake_git(&bin_dir);

        Self {
            _temp_dir: temp_dir,
            bin_dir,
            log_path,
        }
    }

    fn run_xtask(&self, args: &[&str], changed_paths: &str) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        command.args(args);
        #[cfg(windows)]
        {
            command.env_remove("PATH");
            command.env_remove("Path");
            command.env_remove("PATHEXT");
        }
        #[cfg(windows)]
        command.env("PATH", self.fake_path());
        #[cfg(not(windows))]
        command.env("PATH", self.fake_path());
        command
            .env("XTASK_DISPATCH_LOG", &self.log_path)
            .env("XTASK_CHANGED_PATHS", changed_paths)
            .output()
            .expect("run xtask binary")
    }

    fn fake_path(&self) -> std::ffi::OsString {
        let existing_path = env::var_os("Path")
            .or_else(|| env::var_os("PATH"))
            .unwrap_or_default();
        let paths = std::iter::once(self.bin_dir.clone()).chain(env::split_paths(&existing_path));
        env::join_paths(paths).expect("join fake PATH")
    }

    fn cargo_calls(&self) -> Vec<String> {
        fs::read_to_string(&self.log_path)
            .unwrap_or_default()
            .lines()
            .map(ToOwned::to_owned)
            .collect()
    }
}

trait OutputText {
    fn stdout_text(&self) -> String;
    fn stderr_text(&self) -> String;
}

impl OutputText for Output {
    fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

#[cfg(unix)]
fn write_fake_cargo(bin_dir: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let path = bin_dir.join("cargo");
    fs::write(
        &path,
        "#!/bin/sh\nprintf 'cargo %s\\n' \"$*\" >> \"$XTASK_DISPATCH_LOG\"\nexit 0\n",
    )
    .expect("write fake cargo");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod fake cargo");
}

#[cfg(windows)]
fn write_fake_cargo(bin_dir: &Path) {
    fs::write(
        bin_dir.join("cargo.cmd"),
        "@echo off\r\necho cargo %*>>\"%XTASK_DISPATCH_LOG%\"\r\nexit /b 0\r\n",
    )
    .expect("write fake cargo");
}

#[cfg(unix)]
fn write_fake_git(bin_dir: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let path = bin_dir.join("git");
    fs::write(
        &path,
        r#"#!/bin/sh
if [ "$1" = "rev-parse" ]; then
  exit 0
fi
if [ "$1" = "diff" ]; then
  if [ -n "$XTASK_CHANGED_PATHS" ]; then
    printf '%s\n' "$XTASK_CHANGED_PATHS"
  fi
  exit 0
fi
if [ "$1" = "ls-files" ]; then
  exit 0
fi
exit 0
"#,
    )
    .expect("write fake git");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod fake git");
}

#[cfg(windows)]
fn write_fake_git(bin_dir: &Path) {
    fs::write(
        bin_dir.join("git.cmd"),
        r#"@echo off
if "%1"=="rev-parse" exit /b 0
if "%1"=="diff" (
  if not "%XTASK_CHANGED_PATHS%"=="" echo %XTASK_CHANGED_PATHS%
  exit /b 0
)
if "%1"=="ls-files" exit /b 0
exit /b 0
"#,
    )
    .expect("write fake git");
}
