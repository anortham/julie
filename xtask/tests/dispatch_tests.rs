use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

#[test]
fn dispatch_tests_unknown_tier_prints_the_tier_list_and_exits_2() {
    for args in [
        &["test"][..],
        &["test", "nano"][..],
        &["test", "dev", "--coverage"][..],
    ] {
        let fixture = DispatchFixture::new();

        let output = fixture.run_xtask(args);

        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            output.stderr_text().contains("dev|dogfood|full"),
            "{args:?} stderr:\n{}",
            output.stderr_text()
        );
        assert_eq!(fixture.cargo_calls(), Vec::<String>::new(), "{args:?}");
    }
}

#[test]
fn dispatch_tests_dev_runs_the_three_dev_commands_through_cargo() {
    let fixture = DispatchFixture::new();

    let output = fixture.run_xtask(&["test", "dev"]);

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        output.stdout_text(),
        output.stderr_text()
    );
    assert_eq!(
        fixture.cargo_calls(),
        vec![
            "cargo build -p julie --bin julie-server",
            "cargo nextest run --workspace -E not (test(/search_quality/) | test(/fixtures::julie_db/) | test(/dogfood/))",
            "cargo nextest run --lib -p julie --run-ignored only -E test(/tests::cli::/)",
        ]
    );
    assert!(
        output
            .stdout_text()
            .ends_with("SUMMARY: dev 3/3 commands in 0.0s\n"),
        "stdout:\n{}",
        output.stdout_text()
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

        Self {
            _temp_dir: temp_dir,
            bin_dir,
            log_path,
        }
    }

    fn run_xtask(&self, args: &[&str]) -> Output {
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
