# Revival install qualification

**Status:** Prepared; native and client execution is pending authorization to run the nonpublishing CI matrix.

## Candidate record

| Field | Value |
|---|---|
| Candidate SHA at preparation | `319ff7dbb2cef85d93deccd2309531df087a3592` |
| Package version | `8.0.0` |
| Plugin source revision | `7d3996a7bb6130747c631446caacb800a1c34622` |
| Archive SHA-256 | PENDING: archive not built |
| Qualification workflow | `.github/workflows/native-qualification.yml` at the source SHA ultimately selected for the release candidate |

## Native archive matrix

| Target | Expected archive | SHA-256 | Required command | Status |
|---|---|---|---|---|
| Linux x64 | `julie-v8.0.0-x86_64-unknown-linux-gnu.tar.gz` | PENDING: archive not built | Fresh profile: extract, `julie-server --version`, stdio initialize, second session, `service restart`, offline cached semantic check | PENDING |
| macOS arm64 | `julie-v8.0.0-aarch64-apple-darwin.tar.gz` | PENDING: archive not built | Fresh profile: extract, `julie-server --version`, stdio initialize, second session, `service restart`, offline cached semantic check | PENDING |
| macOS Intel | `julie-v8.0.0-x86_64-apple-darwin.tar.gz` | PENDING: archive not built | Fresh profile: extract, `julie-server --version`, stdio initialize, second session, `service restart`, offline cached semantic check | PENDING |
| Windows x64 | `julie-v8.0.0-x86_64-pc-windows-msvc.zip` | PENDING: archive not built | `native-qualification.yml`: fresh profile, start old packaged `julie-server.exe`, extract candidate into a distinct version directory, retain lock evidence | PENDING |

The Windows workflow is intentionally nonpublishing. It builds a candidate archive, verifies it, runs `.github/scripts/test-windows-executable-lock.ps1`, and uploads the archive plus `qualification.json` for seven days. It never creates or uploads a GitHub release.

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

- All four fresh native install/upgrade rows, including a real Windows executable-lock result, remain unrun.
- No real client installation or agent workflow has been run; every client row remains incomplete.
- Plugin public snapshot and final archive checksums cannot be recorded until the release candidate artifacts exist.
- The nonpublishing workflow dispatch requires the workflow to be available on the default branch and consumes GitHub Actions runner capacity. Do not dispatch it without explicit push/spend authorization.
