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
fn release_workflows_extract_the_package_version_with_portable_awk() {
    use std::process::Command;

    let manifest = read_repo_file("Cargo.toml");
    let expected = manifest
        .lines()
        .find_map(|line| line.strip_prefix("version = "))
        .and_then(|line| line.split('"').nth(1))
        .unwrap();
    let version = Command::new("awk")
        .args([
            "-F",
            "\"",
            "/^version = / { print $2; exit }",
            repo_file("Cargo.toml").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), expected);

    for workflow in [
        read_repo_file(".github/workflows/release.yml"),
        read_repo_file(".github/workflows/native-qualification.yml"),
    ] {
        assert!(workflow.contains("awk -F '\"' '/^version = / { print $2; exit }' Cargo.toml"));
        assert!(!workflow.contains("sed -n '0,/^version = /"));
    }
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
        r#"{"schema_version":2,"rust_target":"x86_64-unknown-linux-gnu","files":[{"path":"README.md"},{"path":"LICENSE"}]}"#,
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
        "basename", "cp", "cut", "find", "grep", "gzip", "mkdir", "mktemp", "mv", "python3", "rm",
        "tar",
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
        "package-manifest.json",
    ] {
        assert!(entries.lines().any(|entry| entry == expected), "{expected}");
    }

    let manifest = Command::new("tar")
        .args(["-xOf"])
        .arg(&archive)
        .arg("package-manifest.json")
        .output()
        .unwrap();
    assert!(manifest.status.success());
    let manifest: serde_json::Value = serde_json::from_slice(&manifest.stdout).unwrap();
    assert_eq!(manifest["schema_version"], 2);
    assert_eq!(manifest["rust_target"], "x86_64-unknown-linux-gnu");
    assert_eq!(
        manifest["source"]["schema_version"],
        manifest["schema_version"]
    );
    assert_eq!(manifest["source"]["rust_target"], manifest["rust_target"]);
    assert!(manifest["source"]["files"].is_array());
    assert!(
        !entries
            .lines()
            .any(|entry| entry == "sidecar-package-manifest.json")
    );
    let mut covered = std::collections::BTreeSet::new();
    for file in manifest["files"].as_array().unwrap() {
        let path = file["path"].as_str().unwrap();
        assert!(entries.lines().any(|entry| entry == path), "{path}");
        assert!(covered.insert(path));
        let extracted = Command::new("tar")
            .args(["-xOf"])
            .arg(&archive)
            .arg(path)
            .output()
            .unwrap();
        let extracted_path = tmp.path().join(format!("checksum-{path}"));
        fs::write(&extracted_path, extracted.stdout).unwrap();
        let actual = Command::new("sha256sum")
            .arg(&extracted_path)
            .output()
            .unwrap();
        let expected_checksum = file["sha256"].as_str().unwrap();
        let checksum = String::from_utf8(actual.stdout).unwrap();
        assert_eq!(
            checksum.split_whitespace().next().unwrap(),
            expected_checksum
        );
    }
    assert_eq!(
        covered.len(),
        entries
            .lines()
            .filter(|entry| *entry != "package-manifest.json")
            .count()
    );
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

#[test]
fn release_workflow_qualifies_archives_and_awaits_the_pinned_plugin_workflow() {
    let workflow = read_repo_file(".github/workflows/release.yml");
    let verifier = read_repo_file(".github/scripts/verify-release-archive.py");
    let notes = read_repo_file("docs/release-notes/v8.0.0.md");

    assert!(workflow.contains("Verify packaged archive"));
    assert!(workflow.contains("verify-release-archive.py"));
    assert!(workflow.contains("anortham/julie-plugin/.github/workflows/update-binaries.yml@"));
    assert!(!workflow.contains("gh workflow run \"Update Plugin\""));
    assert!(workflow.contains("uses: actions/setup-node@v4"));
    assert!(workflow.contains("node-version: 22.5.0"));
    assert!(workflow.contains("node --test hooks/*.test.cjs"));
    assert!(verifier.contains("package-manifest.json"));
    assert!(!verifier.contains("sidecar-package-manifest.json"));
    assert!(verifier.contains("julie-semantic-sidecar"));
    assert!(verifier.contains("instructions"));
    for section in [
        "## Semantic search by default",
        "## Upgrade notes",
        "## Known caveats",
    ] {
        assert!(notes.contains(section), "{section}");
    }
    for platform in [
        "macOS Apple Silicon",
        "macOS Intel",
        "Windows x86_64",
        "Linux x86_64",
    ] {
        assert!(notes.contains(platform), "{platform}");
    }
}

#[test]
fn native_qualification_workflow_is_nonpublishing_and_exercises_windows_lock() {
    let workflow = read_repo_file(".github/workflows/native-qualification.yml");
    let probe = read_repo_file(".github/scripts/test-windows-executable-lock.ps1");
    let report = read_repo_file("docs/findings/revival-install-qualification.md");

    assert!(workflow.contains("workflow_dispatch:"));
    assert!(workflow.contains("push:"));
    assert!(workflow.contains("'qualification/**'"));
    assert!(workflow.contains("aarch64-apple-darwin"));
    assert!(workflow.contains("x86_64-apple-darwin"));
    assert!(workflow.contains("x86_64-pc-windows-msvc"));
    assert!(workflow.contains("macos-latest"));
    assert!(workflow.contains("macos-15-intel"));
    assert!(workflow.contains("os: windows-latest"));
    assert!(workflow.contains("test-windows-executable-lock.ps1"));
    assert!(workflow.contains("actions/upload-artifact@v4"));
    assert!(workflow.contains("$ARCHIVE.sha256"));
    assert!(workflow.contains("--sha256 \"$ARCHIVE.sha256\""));
    assert!(workflow.contains("service restart"));
    assert!(workflow.contains("service stop"));
    assert!(workflow.contains("method\":\"initialize"));
    assert!(workflow.contains("instructions"));
    assert!(workflow.contains("json.loads(result.stdout)"));
    assert!(workflow.contains("          import json"));
    assert!(workflow.contains("initialize response lacks workspace-routing instructions"));
    assert!(workflow.contains("service_pid"));
    assert!(workflow.contains("restart_pid"));
    assert!(workflow.contains("old_pid"));
    assert!(workflow.contains("second initialize did not reuse the service PID"));
    assert!(workflow.contains("service restart retained the old PID"));
    assert!(!workflow.contains("gh release create"));
    assert!(!workflow.contains("gh release upload"));
    assert!(probe.contains("try {"));
    assert!(probe.contains("finally {"));
    assert!(probe.contains("service stop"));
    assert!(probe.contains("service discovery file remains"));
    assert!(
        probe.find("$process.StandardInput.Close()").unwrap()
            < probe.find("& $server service stop").unwrap(),
        "the shim must close its redirected stdin before service stop"
    );
    assert!(probe.contains("[DateTime]::UtcNow.AddSeconds(5)"));
    assert!(probe.contains("Start-Sleep -Milliseconds 100"));
    assert!(
        probe.find("[DateTime]::UtcNow.AddSeconds(5)").unwrap()
            < probe.rfind("service discovery file remains").unwrap(),
        "the discovery-file assertion must follow its bounded removal wait"
    );
    assert!(probe.contains("candidate_sha"));
    assert!(probe.contains("archive_sha256"));
    assert!(report.contains("Archive binary source SHA"));
    assert!(report.contains("Package wrapper and verifier source SHA"));
    assert!(report.contains("Linux archive SHA-256"));
}

#[cfg(unix)]
#[test]
fn release_qualification_rejects_invalid_archives_and_partial_public_assets() {
    use std::{fmt::Write, os::unix::fs::PermissionsExt, process::Command};

    let tmp = tempfile::tempdir().unwrap();
    let verifier = repo_file(".github/scripts/verify-release-archive.py");
    let assets_verifier = repo_file(".github/scripts/verify-release-assets.py");
    let sha256 = |path: &std::path::Path| {
        let output = Command::new("python3")
            .args(["-c", "import hashlib,sys; print(hashlib.sha256(open(sys.argv[1], 'rb').read()).hexdigest())"])
            .arg(path)
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let package = |name: &str, variant: &str| {
        let root = tmp.path().join(name);
        fs::create_dir_all(&root).unwrap();
        let server = root.join("julie-server");
        fs::write(&server, "#!/bin/sh\ncase \"${1:-}\" in --version) echo 'julie-server 8.0.0' ;; service) rm -f \"$JULIE_HOME/service.json\" ;; *) mkdir -p \"$JULIE_HOME\"; : > \"$JULIE_HOME/service.json\"; echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"instructions\":\"open workspace\"}}' ;; esac\n").unwrap();
        fs::set_permissions(&server, fs::Permissions::from_mode(0o755)).unwrap();
        let sidecar = root.join("julie-semantic-sidecar");
        fs::write(&sidecar, "#!/bin/sh\necho 'julie-semantic-sidecar 0.1.0'\n").unwrap();
        fs::set_permissions(&sidecar, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(root.join("README.md"), "readme").unwrap();
        fs::write(root.join("LICENSE"), "license").unwrap();
        let mut files = String::new();
        for file in [
            "LICENSE",
            "README.md",
            "julie-semantic-sidecar",
            "julie-server",
        ] {
            write!(
                files,
                r#"{{"path":"{file}","sha256":"{}"}},"#,
                sha256(&root.join(file))
            )
            .unwrap();
        }
        let manifest = format!(
            r#"{{"schema_version":2,"rust_target":"x86_64-unknown-linux-gnu","source":{{"schema_version":2,"rust_target":"x86_64-unknown-linux-gnu","files":[{{"path":"README.md"}}]}},"files":[{}]}}"#,
            files.trim_end_matches(',')
        );
        fs::write(root.join("package-manifest.json"), manifest).unwrap();
        match variant {
            "missing-sidecar" => fs::remove_file(&sidecar).unwrap(),
            "wrong-version" => {
                let old_checksum = sha256(&server);
                fs::write(&server, "#!/bin/sh\necho 'julie-server 8.0.1'\n").unwrap();
                let manifest = fs::read_to_string(root.join("package-manifest.json")).unwrap();
                fs::write(
                    root.join("package-manifest.json"),
                    manifest.replace(&old_checksum, &sha256(&server)),
                )
                .unwrap();
            }
            "false-instructions" => {
                let old_checksum = sha256(&server);
                fs::write(&server, "#!/bin/sh\ncase \"${1:-}\" in --version) echo 'julie-server 8.0.0' ;; service) exit 0 ;; *) echo '{\"message\":\"instructions workspace\"}' ;; esac\n").unwrap();
                let manifest = fs::read_to_string(root.join("package-manifest.json")).unwrap();
                fs::write(
                    root.join("package-manifest.json"),
                    manifest.replace(&old_checksum, &sha256(&server)),
                )
                .unwrap();
            }
            "async-cleanup" | "cleanup-never" => {
                let old_checksum = sha256(&server);
                let stop = if variant == "async-cleanup" {
                    "(sleep 0.05; rm -f \"$JULIE_HOME/service.json\") >/dev/null 2>&1 < /dev/null &"
                } else {
                    ":"
                };
                fs::write(&server, format!("#!/bin/sh\ncase \"${{1:-}}\" in --version) echo 'julie-server 8.0.0' ;; service) {stop} ;; *) mkdir -p \"$JULIE_HOME\"; : > \"$JULIE_HOME/service.json\"; echo '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"instructions\":\"open workspace\"}}}}' ;; esac\n")).unwrap();
                let manifest = fs::read_to_string(root.join("package-manifest.json")).unwrap();
                fs::write(
                    root.join("package-manifest.json"),
                    manifest.replace(&old_checksum, &sha256(&server)),
                )
                .unwrap();
            }
            "malformed-manifest" => fs::write(root.join("package-manifest.json"), "{").unwrap(),
            "wrong-checksum" => fs::write(
                root.join("package-manifest.json"),
                r#"{"source":{"files":[]},"files":[{"path":"README.md","sha256":"wrong"}]}"#,
            )
            .unwrap(),
            "invalid-launcher-fields" => {
                let manifest = fs::read_to_string(root.join("package-manifest.json")).unwrap();
                fs::write(
                    root.join("package-manifest.json"),
                    manifest.replacen(
                        r#"{"schema_version":2,"rust_target":"x86_64-unknown-linux-gnu","source"#,
                        r#"{"schema_version":1,"rust_target":"","source"#,
                        1,
                    ),
                )
                .unwrap();
            }
            _ => {}
        }
        let archive = tmp.path().join(format!("{name}.tar.gz"));
        assert!(
            Command::new("tar")
                .args(["-czf"])
                .arg(&archive)
                .arg("-C")
                .arg(&root)
                .arg(".")
                .status()
                .unwrap()
                .success()
        );
        let checksum = tmp.path().join(format!("{name}.sha256"));
        fs::write(
            &checksum,
            format!("{}  {}\n", sha256(&archive), archive.display()),
        )
        .unwrap();
        let result = Command::new("python3")
            .arg(&verifier)
            .arg(&archive)
            .args([
                "--version",
                "8.0.0",
                "--sidecar-version",
                "0.1.0",
                "--sha256",
            ])
            .arg(&checksum)
            .output()
            .unwrap();
        (matches!(variant, "valid" | "async-cleanup"), result)
    };

    for (name, variant) in [
        ("valid", "valid"),
        ("missing", "missing-sidecar"),
        ("version", "wrong-version"),
        ("false-instructions", "false-instructions"),
        ("async-cleanup", "async-cleanup"),
        ("cleanup-never", "cleanup-never"),
        ("malformed", "malformed-manifest"),
        ("checksum", "wrong-checksum"),
        ("invalid-launcher-fields", "invalid-launcher-fields"),
    ] {
        let (expected, result) = package(name, variant);
        assert_eq!(
            result.status.success(),
            expected,
            "{variant}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        if variant == "wrong-version" {
            assert!(
                String::from_utf8_lossy(&result.stderr)
                    .contains("packaged server version does not match release version")
            );
        }
    }

    let assets = tmp.path().join("assets");
    fs::create_dir_all(&assets).unwrap();
    let archives = [
        "julie-v8.0.0-aarch64-apple-darwin.tar.gz",
        "julie-v8.0.0-x86_64-apple-darwin.tar.gz",
        "julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz",
        "julie-v8.0.0-x86_64-pc-windows-msvc.zip",
    ];
    for archive in archives {
        fs::write(assets.join(archive), archive).unwrap();
        if archive != "julie-v8.0.0-x86_64-pc-windows-msvc.zip" {
            fs::write(
                assets.join(format!("{archive}.sha256")),
                format!("{}  {archive}\n", sha256(&assets.join(archive))),
            )
            .unwrap();
        }
    }
    let partial = Command::new("python3")
        .arg(&assets_verifier)
        .arg(&assets)
        .arg("8.0.0")
        .output()
        .unwrap();
    assert!(!partial.status.success());
    assert!(String::from_utf8_lossy(&partial.stderr).contains("incomplete public asset set"));
    fs::write(
        assets.join("julie-v8.0.0-x86_64-pc-windows-msvc.zip.sha256"),
        "wrong  julie-v8.0.0-x86_64-pc-windows-msvc.zip\n",
    )
    .unwrap();
    let wrong_checksum = Command::new("python3")
        .arg(&assets_verifier)
        .arg(&assets)
        .arg("8.0.0")
        .output()
        .unwrap();
    assert!(!wrong_checksum.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_checksum.stderr)
            .contains("public archive checksum mismatch")
    );
}
