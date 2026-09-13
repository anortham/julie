# Revival install qualification

**Status:** Linux x64 local package evidence is incomplete; all native acceptance rows and all client rows remain pending.

## Candidate record

| Field | Value |
|---|---|
| Archive binary source SHA | `c487f43ace9e971a3cabfb9be604b85efe85000b` (locally committed candidate) |
| Package wrapper and verifier source SHA | `a90720f010283270123c9679729fbf8f4fd5004f` |
| Package version | `8.0.0` |
| Plugin source revision | `7d3996a7bb6130747c631446caacb800a1c34622` |
| Linux archive SHA-256 | `e9686f4e07f2d30eb6d6ecdefb425d99c5cedebe29637e77f173124f3504a43d` |
| Qualification workflow | `.github/workflows/native-qualification.yml` at the source SHA ultimately selected for the release candidate |

## Native archive matrix

| Target | Expected archive | SHA-256 | Required command | Status |
|---|---|---|---|---|
| Linux x64 | `julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz` | `e9686f4e07f2d30eb6d6ecdefb425d99c5cedebe29637e77f173124f3504a43d` | Fresh profile: extract, `julie-server --version`, stdio initialize, second session, `service restart`, offline cached semantic check | PARTIAL: current package/verifier/lifecycle/full cached semantics and plugin fresh install pass; upgrade and client proof remain pending |
| macOS arm64 | `julie-v8.0.0-aarch64-apple-darwin.tar.gz` | PENDING: archive not built | Fresh profile: extract, `julie-server --version`, stdio initialize, second session, `service restart`, offline cached semantic check | PENDING |
| macOS Intel | `julie-v8.0.0-x86_64-apple-darwin.tar.gz` | PENDING: archive not built | Fresh profile: extract, `julie-server --version`, stdio initialize, second session, `service restart`, offline cached semantic check | PENDING |
| Windows x64 | `julie-v8.0.0-x86_64-pc-windows-msvc.zip` | PENDING: archive not built | `native-qualification.yml`: fresh profile, start old packaged `julie-server.exe`, extract candidate into a distinct version directory, retain lock evidence | PENDING |

The Windows workflow is intentionally nonpublishing. It builds a candidate archive, verifies it, runs `.github/scripts/test-windows-executable-lock.ps1`, and uploads the archive plus `qualification.json` for seven days. It never creates or uploads a GitHub release.

## Linux x64 local evidence — 2026-09-13

Host: Fedora Linux `7.1.13-200.fc44.x86_64`, x86_64. The candidate was built and packaged only in the worktree's gitignored `target/qualification/` directory:

```text
cargo build --release --target x86_64-unknown-linux-gnu --bin julie-server
bash .github/scripts/pack-release.sh x86_64-unknown-linux-gnu 8.0.0 target/x86_64-unknown-linux-gnu/release target/qualification
sha256sum target/qualification/julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz
python3 .github/scripts/verify-release-archive.py target/qualification/julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz --sha256 target/qualification/julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256 --version 8.0.0 --sidecar-version 0.1.0
```

The built `julie-server` reported `8.0.0`; packaged `julie-semantic-sidecar` reported `0.1.0`. The archive contains both binaries, the wrapper manifest, licenses, and required Linux runtime libraries. The current `a90720f0` wrapper repackaged the unchanged `c487f43a` binary, produced the current checksum above, and passed the current archive verifier.

Independent fresh-profile evidence used disposable `HOME`, `JULIE_HOME`, and launch directories under one temporary root, with `JULIE_SESSION_HOOKS=0`; all were deleted after the run. A first packaged shim initialize returned exit `0` and nonempty workspace-routing instructions, creating service PID `2559414`. A second packaged shim from another launch directory returned exit `0` and retained the same PID. `service status` reported two initialize requests and no checkout selected from either launch cwd. `service restart` replaced PID `2559414` with `2571975` and the old PID was no longer alive. A final packaged `service stop` returned exit `0`, removed the service record, and stopped that PID.

For offline semantics, a verified existing `bge-small-en-v1.5-f32.gguf` was copied from the maintainer cache into the disposable cache only; source and copy both had SHA-256 `bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65`. With all proxy variables set to `http://127.0.0.1:9`, packaged `julie-semantic-sidecar prepare --model bge-small-en-v1.5-f32` exited `0` and reported that cache path/checksum. The packaged daemon then indexed one scratch Rust symbol and the condition-based health poll reached one vector. Required semantic search returned the `semantic_probe` hit with `coverage: full`, `mode: required`, and `status: ready`.

The first shim retry failed only because blocking `HTTP_PROXY` and `ALL_PROXY` also routed its localhost service client through the dead proxy, even while authenticated `/status` and `/mcp` were healthy. With those two proxy variables removed and only HTTPS blocked, packaged shim initialization returned exit `0` with the same routing instructions. This is a probe-environment issue, not a service shutdown state.

## Linux plugin launcher evidence — 2026-09-13

A clean copy of plugin source `7d3996a7bb6130747c631446caacb800a1c34622` received only the current Linux archive above and used a new `HOME` and `JULIE_HOME`. `node hooks/run.cjs` extracted `julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz` into its immutable version directory, then returned exit `0`, initialization instructions, and a successful `manage_workspace status` call (`Checkouts: 0`). The extracted version contains the top-level `package-manifest.json`; no symlink was discovered under its Linux runtime directory.

The a907 wrapper provides the launcher-required top-level `schema_version: 2` and `rust_target`. The isolated service record named v8 PID `3179484`; packaged `service stop` returned `0`, removed that record, and stopped the PID. The copied plugin and profile were then deleted. This is a real fresh install and launcher result, not client adoption evidence.

A real prior Linux archive is available in the plugin source: `julie-v7.18.0-x86_64-unknown-linux-gnu.tar.gz`, SHA-256 `0c7b3688372fb3519583aa474591b9e15c1614386504159db376482cbf1fcc3f`. Its archive root contains `julie-server` and `julie-embedding-host`, the prior runtime layout. No prior versioned v8 archive is available. The v7 package is not compatible with the v8 sidecar/package-manifest contract, so it was not used to fabricate an upgrade result.

## Agent-use matrix

| Route | Required evidence | Status |
|---|---|---|
| Claude Code plugin on each supported native target where available | Fresh profile install; initialization, tools/schema, hook or subagent guidance, real orientation/inspect/page/workspace workflow | PENDING: no client install performed |
| Codex plugin on each supported native target where available | Fresh profile install; initialization, tools/schema, real orientation/inspect/page/workspace workflow | PENDING: no client install performed |
| Antigravity | One real supported host; documented install and successful agent workflow | PENDING: client access not exercised |
| OpenCode | One real supported host; documented install and successful agent workflow | PENDING: client access not exercised |
| Hermes | One real supported host; documented install and successful agent workflow | PENDING: client access not exercised |
| Cursor | One real supported host; documented install and successful agent workflow | PENDING: client access not exercised |

## Required capture commands

Record the literal command, host OS/version, client version, archive SHA-256, initialization response, schema list, tool calls, and result for every completed row. Do not infer a passed client row from a mock protocol client.

```text
python3 .github/scripts/verify-release-archive.py <archive> --sha256 <archive>.sha256 --version 8.0.0 --sidecar-version 0.1.0 [--windows]
./julie-server --version
./julie-server tools list --json
./julie-server service restart
```

For an installed archive, run the server with a newly created `JULIE_HOME` and no inherited maintainer cache. Record the checked-out candidate SHA and archive SHA-256 with whether lexical startup, cached/offline semantic readiness, the second session's shared service/index, and configuration removal/uninstall succeed.

## Open gates

- Linux plugin fresh install now passes, but no real compatible prior versioned v8 archive is available for upgrade qualification. The available v7.18.0 package has the prior embedding-host layout.
- macOS arm64, macOS Intel, and Windows x64 native rows remain unrun, including the real Windows executable-lock result.
- No real client installation or agent workflow has been run; every client row remains incomplete.
- Plugin public snapshot and final archive checksums cannot be recorded until the release candidate artifacts exist.
- The nonpublishing workflow dispatch requires the workflow to be available on the default branch and consumes GitHub Actions runner capacity. Do not dispatch it without explicit push/spend authorization.
