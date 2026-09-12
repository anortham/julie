use std::{fs, path::PathBuf};

#[test]
fn toolchain_contract_pins_release_build_inputs() {
    let toolchain = read_repo_file("rust-toolchain.toml");
    let cargo_config = read_repo_file(".cargo/config.toml");
    let release_workflow = read_repo_file(".github/workflows/release.yml");
    let readme = read_repo_file("README.md");
    let development = read_repo_file("docs/DEVELOPMENT.md");

    assert!(toolchain.contains("channel = \"1.97.0\""));
    assert!(toolchain.contains("profile = \"minimal\""));
    assert!(toolchain.contains("components = [\"rustfmt\", \"clippy\"]"));
    assert!(cargo_config.contains("MACOSX_DEPLOYMENT_TARGET = \"11.0\""));
    assert!(release_workflow.contains("uses: dtolnay/rust-toolchain@1.97.0"));
    assert!(
        release_workflow
            .contains("tag version $TAG_VERSION does not match Cargo.toml $CARGO_VERSION")
    );
    assert!(release_workflow.contains("release notes missing: $NOTES_PATH"));
    assert!(readme.contains("repository-pinned Rust 1.97.0 toolchain"));
    assert!(development.contains("/opt/homebrew/opt/rustup/bin"));
    assert!(development.contains("rustup show active-toolchain"));
}

#[cfg(unix)]
#[test]
fn release_packaging_uses_shasum_fallback_and_preserves_manifest_files() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::process::Command;

    let tmp = tempfile::tempdir().unwrap();
    let sidecar = tmp.path().join("sidecar");
    let release = tmp.path().join("release");
    let output = tmp.path().join("output");
    let commands = tmp.path().join("commands");
    fs::create_dir_all(&sidecar).unwrap();
    fs::create_dir_all(&release).unwrap();
    fs::create_dir_all(&output).unwrap();
    fs::create_dir_all(&commands).unwrap();
    fs::write(sidecar.join("README.md"), "sidecar readme").unwrap();
    fs::write(sidecar.join("LICENSE"), "sidecar license").unwrap();
    fs::write(sidecar.join("julie-semantic-sidecar"), "sidecar").unwrap();
    fs::write(
        sidecar.join("package-manifest.json"),
        r#"{"files":[{"path":"README.md"},{"path":"LICENSE"}]}"#,
    )
    .unwrap();
    fs::write(release.join("julie-server"), "server").unwrap();

    let asset_name = "julie-semantic-sidecar-0.1.0-x86_64-unknown-linux-gnu-vulkan-portable.tar.gz";
    let asset = tmp.path().join(asset_name);
    assert!(
        Command::new("tar")
            .args(["-czf"])
            .arg(&asset)
            .arg("-C")
            .arg(&sidecar)
            .arg(".")
            .status()
            .unwrap()
            .success()
    );

    let resolve = |command: &str| {
        let output = Command::new("sh")
            .args(["-c", &format!("command -v {command}")])
            .output()
            .unwrap();
        assert!(output.status.success(), "{command}");
        PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
    };
    let bash = resolve("bash");

    let gh = commands.join("gh");
    fs::write(
        &gh,
        "#!/bin/sh\nwhile [ \"$1\" != \"--dir\" ]; do shift; done\ncp \"$FAKE_ASSET\" \"$2/$(basename \"$FAKE_ASSET\")\"\n",
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();

    let shasum = commands.join("shasum");
    fs::write(
        &shasum,
        format!(
            "#!/bin/sh\n[ \"$1\" = \"-a\" ] && [ \"$2\" = \"256\" ] && [ -f \"$3\" ] || exit 2\necho \"{}  $3\"\n",
            "14b369076776fc7e7ed0ee2a8d8261311e09b634bdc7d57eed928c4fcfa61212"
        ),
    )
    .unwrap();
    fs::set_permissions(&shasum, fs::Permissions::from_mode(0o755)).unwrap();

    for command in [
        "basename", "cp", "cut", "gzip", "mkdir", "mktemp", "mv", "rm", "tar",
    ] {
        symlink(resolve(command), commands.join(command)).unwrap();
    }

    let result = Command::new(bash)
        .arg(repo_file(".github/scripts/pack-release.sh"))
        .args(["x86_64-unknown-linux-gnu", "8.0.0"])
        .arg(&release)
        .arg(&output)
        .env("PATH", &commands)
        .env("FAKE_ASSET", &asset)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let archive = output.join("julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz");
    let entries = Command::new("tar")
        .args(["-tzf"])
        .arg(&archive)
        .output()
        .unwrap();
    assert!(entries.status.success());
    let entries = String::from_utf8(entries.stdout).unwrap();
    for expected in [
        "LICENSE",
        "README.md",
        "julie-semantic-sidecar",
        "julie-server",
        "sidecar-package-manifest.json",
    ] {
        assert!(entries.lines().any(|entry| entry == expected), "{expected}");
    }

    let manifest = Command::new("tar")
        .args(["-xOf"])
        .arg(&archive)
        .arg("sidecar-package-manifest.json")
        .output()
        .unwrap();
    assert!(manifest.status.success());
    let manifest: serde_json::Value = serde_json::from_slice(&manifest.stdout).unwrap();
    for file in manifest["files"].as_array().unwrap() {
        let path = file["path"].as_str().unwrap();
        assert!(entries.lines().any(|entry| entry == path), "{path}");
    }
}

fn read_repo_file(relative_path: &str) -> String {
    fs::read_to_string(repo_file(relative_path))
        .unwrap_or_else(|error| panic!("failed to read {relative_path}: {error}"))
}

fn repo_file(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(relative_path)
}
