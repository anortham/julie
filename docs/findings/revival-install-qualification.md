# Revival install qualification

**Status:** Linux and local Windows package qualification, fresh install, semantics, repository gate, and real-agent evidence for Codex, OpenCode, and AGY pass. Current source candidate `1ff96903eed69d136e7a89a6dc44f2223fbc7be3` passed the frozen-tree full gate (5/5, 27.7s) before later qualification-only scripts/tests. macOS qualification, client authorization, and compatible upgrade remain local release-candidate blockers. Publication checks are separately publication-only pending; Plan 5C remains blocked.

## Candidate record

| Field | Value |
|---|---|
| Frozen candidate code/full-gate SHA | `055645e02ebf261355c7db6659a9fe1b5159a791` |
| Current source candidate/full-gate SHA | `1ff96903eed69d136e7a89a6dc44f2223fbc7be3` |
| Archive binary source SHA | `055645e02ebf261355c7db6659a9fe1b5159a791` |
| Package wrapper and verifier source SHA | `a90720f010283270123c9679729fbf8f4fd5004f` |
| Qualification ref pushed for hosted retry | `8c84e683` |
| Latest local fixes | `ce574773` (probe sends top-level `--semantics required` and rejects fallback); `3115c6c8` (generic CLI JSON preserves RequestEngine readiness) |
| Archive server binary SHA-256 | `2620d7c7f0f5f39e31a4318c78ca5efa7442a56642c99c502d232daef99d2da8` |
| Qualification paging-fix reason | The candidate includes the final paging correction so real-agent page-two evidence uses the frozen release candidate rather than the earlier `35a909eb` gate. |
| Package version | `8.0.0` |
| Plugin source revision | `7d3996a7bb6130747c631446caacb800a1c34622` |
| Linux archive SHA-256 | `9d9095713c3572407722ad105ada7a85fd193619751271d10eb8f23a0fe89bba` |
| Qualification workflow | `.github/workflows/native-qualification.yml` at the source SHA ultimately selected for the release candidate |

## Native archive matrix

| Target | Expected archive | SHA-256 | Required command | Status |
|---|---|---|---|---|
| Linux x64 | `julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz` | `9d9095713c3572407722ad105ada7a85fd193619751271d10eb8f23a0fe89bba` | Fresh profile: extract, `julie-server --version`, stdio initialize, second session, `service restart`, offline cached semantic check | PARTIAL: package/verifier/lifecycle/full cached semantics, plugin fresh install, and Codex/OpenCode/AGY real-agent workflows pass; compatible upgrade remains pending |
| macOS arm64 | `julie-v8.0.0-aarch64-apple-darwin.tar.gz` | Not retained: upload skipped after later failure | Fresh profile: extract, `julie-server --version`, stdio initialize, second session, `service restart`, offline cached semantic check | INCOMPLETE: [run 34760490971](https://github.com/anortham/julie/actions/runs/34760490971), on pushed ref `8c84e683`, built and archive-verified the package, then cached/offline semantics failed because the CLI request policy remained auto and the backend fell back lexical. |
| macOS Intel | `julie-v8.0.0-x86_64-apple-darwin.tar.gz` | Not retained: upload skipped after later failure | Fresh profile: extract, `julie-server --version`, stdio initialize, second session, `service restart`, offline cached semantic check | INCOMPLETE: [run 34760490971](https://github.com/anortham/julie/actions/runs/34760490971), on pushed ref `8c84e683`, built and archive-verified the package, then cached/offline semantics failed because the CLI request policy remained auto and the backend fell back lexical. |
| Windows x64 | `julie-v8.0.0-x86_64-pc-windows-msvc.zip` | `5510d7c1cdff9418260def83a4b071196bdaa2ed7a029cc3e89d02f7bf3f4bfe` | Local win-test: release build, archive identity/manifest/instructions verification, persistent-service cached offline semantics, NTFS lock, and lifecycle verifier | PASS locally: guest synchronized clean SHA `46816cf3`; release build and archive verifier passed. After fixes `163804df`, `a9f9c66c`, and `67a37cb3`, semantics, executable lock, two-session PID reuse, restart/new PID, old/new PID exit, stop, and discovery cleanup passed. No GitHub Actions run or push followed. |

The Windows workflow is intentionally nonpublishing. It builds a candidate archive, verifies it, runs `.github/scripts/test-windows-executable-lock.ps1`, and uploads the archive plus `qualification.json` for seven days. It never creates or uploads a GitHub release.

## Linux x64 local evidence — 2026-09-13

Host: Fedora Linux `7.1.13-200.fc44.x86_64`, x86_64. The candidate was built and packaged only in the worktree's gitignored `target/qualification/` directory:

```text
cargo build --release --target x86_64-unknown-linux-gnu --bin julie-server
bash .github/scripts/pack-release.sh x86_64-unknown-linux-gnu 8.0.0 target/x86_64-unknown-linux-gnu/release target/qualification
sha256sum target/qualification/julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz
python3 .github/scripts/verify-release-archive.py target/qualification/julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz --sha256 target/qualification/julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256 --version 8.0.0 --sidecar-version 0.1.0
```

The built `julie-server` reported `8.0.0`; packaged `julie-semantic-sidecar` reported `0.1.0`. The archive contains both binaries, the wrapper manifest, licenses, and required Linux runtime libraries. The final candidate archive SHA-256 is `9d9095713c3572407722ad105ada7a85fd193619751271d10eb8f23a0fe89bba`; its server binary SHA-256 is `2620d7c7f0f5f39e31a4318c78ca5efa7442a56642c99c502d232daef99d2da8`.

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
| Claude Code plugin on each supported native target where available | Fresh profile install; initialization, tools/schema, hook or subagent guidance, real orientation/inspect/page/workspace workflow | BLOCKED: pre-fix workflow passed, but final-candidate rerun initialized Julie then stopped at expired isolated OAuth; no interactive login was attempted |
| Codex plugin on each supported native target where available | Fresh profile install; initialization, tools/schema, real orientation/inspect/page/workspace workflow | PASS on isolated Linux profile: final agent completed search page one, symbol inspection, and genuine page two; removal check passed |
| Antigravity | One real supported host; documented install and successful agent workflow | PASS on isolated Linux profile (AGY 1.2.2): final agent opened the workspace, semantic-search page one, inspected `WidgetAlpha::greeting`, and requested pages two and three with preserved selector/filter |
| OpenCode | One real supported host; documented install and successful agent workflow | PASS on isolated Linux profile (OpenCode 1.18.25): final agent opened the workspace, completed page one/page two with preserved selector/filter, and inspected the second path |
| Hermes | One real supported host; documented install and successful agent workflow | BLOCKED: interactive OAuth required; no login was attempted |
| Cursor | One real supported host; documented install and successful agent workflow | BLOCKED: no disposable API key was available |

The Codex, OpenCode, and AGY removal checks removed only disposable profile configuration, then verified the client remained usable without Julie. The qualification roots, copied credentials, plugin/archive, cache, fixture, indexes, logs, and service state were deleted after every run. Sanitized retained evidence has clean secret scans; no live profile or shared cache was touched.

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

- **PENDING — Linux compatible upgrade:** no real compatible prior versioned v8 archive is available; the available v7.18.0 package has the prior embedding-host layout.
- **PENDING — remaining real-agent client routes:** final Claude Code evidence needs refreshed isolated OAuth; Hermes needs interactive OAuth; Cursor needs a disposable API key. Codex, OpenCode, and AGY pass on isolated Linux.
- **BLOCKED — native qualification:** Windows now passes locally. [Run 34760490971](https://github.com/anortham/julie/actions/runs/34760490971), at pushed ref `8c84e683`, is historical macOS evidence: both macOS targets built and archive-verified, then search returned `mode=auto`, `backend=lexical`, and `backend_fallback=true`. `ce574773` makes the probe send top-level `--semantics required` and reject fallback; `3115c6c8` preserves RequestEngine readiness in generic CLI JSON. macOS remains the native blocker; no further GitHub Actions run is requested.
- **BLOCKED — Plan 5C gate:** the integrated Plans 1–4 release candidate remains unqualified, so paired retrieval and agent-task experiments must not start.
- **PENDING — publication only:** public release assets, downstream plugin publication, awaited reusable-workflow evidence, and the checked-out public plugin snapshot cannot be verified before authorized publication. These do not block local release-candidate qualification.

Local win-test evidence: focused Rust test `/home/murphy/.local/share/win-test/logs/20260913T154842Z-revival-deployment-3836626.log`; release build `/home/murphy/.local/share/win-test/logs/20260913T155624Z-revival-deployment-3840637.log`; package `/home/murphy/.local/share/win-test/logs/20260913T160508Z-revival-deployment-3844864.log`; offline semantics `/home/murphy/.local/share/win-test/logs/20260913T161420Z-revival-deployment-3852262.log`; executable lock `/home/murphy/.local/share/win-test/logs/20260913T161722Z-revival-deployment-3854455.log`; final file-backed verifier/lifecycle `/home/murphy/.local/share/win-test/logs/20260913T162854Z-revival-deployment-3861460.log`; no-process check `/home/murphy/.local/share/win-test/logs/20260913T162955Z-revival-deployment-3862001.log`. Windows full remains reserved for final post-merge validation.
