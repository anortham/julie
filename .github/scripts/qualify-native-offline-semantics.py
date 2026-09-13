#!/usr/bin/env python3
"""Qualify a packaged native semantic runtime without external HTTPS."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

MODEL = "bge-small-en-v1.5-f32"
MODEL_SHA256 = "bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65"


def fail(message: str) -> None:
    raise SystemExit(f"native offline semantics qualification failed: {message}")


def run(command: list[str], env: dict[str, str], cwd: Path | None = None, timeout: int = 600) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, env=env, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          check=False, timeout=timeout)


def done_record(output: str) -> dict[str, object]:
    for line in output.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("event") == "done" and event.get("model_id") == MODEL:
            return event
    fail("prepare did not emit a done record for the pinned model")


def required_search_succeeded(response: object, symbol: str) -> bool:
    if not isinstance(response, dict):
        return False
    readiness = response.get("readiness")
    result = response.get("result")
    if not isinstance(readiness, dict) or not isinstance(result, dict):
        return False
    structured = result.get("structuredContent")
    if not isinstance(structured, dict) or structured.get("backend") != "semantic":
        return False
    trace = structured.get("trace")
    if not isinstance(trace, dict) or trace.get("backend_fallback") is not False:
        return False
    rendered = json.dumps(response).lower()
    return (readiness.get("mode") == "required" and readiness.get("status") == "ready"
            and readiness.get("coverage") == "full" and symbol.lower() in rendered)


def search_params(workspace: Path) -> dict[str, str]:
    return {"query": "semantic_probe", "backend": "semantic", "semantics": "required", "workspace": str(workspace)}


def service_command(server: Path) -> list[str]:
    return [str(server), "service"]


def api_request(record: dict[str, object], params: dict[str, str]) -> urllib.request.Request:
    return urllib.request.Request(
        f"http://127.0.0.1:{record['port']}/api/fast_search",
        data=json.dumps(params).encode(),
        headers={"Authorization": f"Bearer {record['token']}", "Content-Type": "application/json"},
    )


def start_service(server: Path, env: dict[str, str], root: Path) -> tuple[subprocess.Popen[bytes], object, dict[str, object]]:
    log = (root / "service.log").open("wb")
    process = subprocess.Popen(service_command(server), env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
    record_path = Path(env["JULIE_HOME"]) / "service.json"
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if process.poll() is not None:
            log.close()
            fail(f"packaged service exited {process.returncode} before discovery")
        try:
            record = json.loads(record_path.read_text())
        except (OSError, json.JSONDecodeError):
            time.sleep(0.1)
            continue
        if isinstance(record, dict) and isinstance(record.get("port"), int) and isinstance(record.get("token"), str):
            return process, log, record
        time.sleep(0.1)
    process.terminate()
    process.wait(timeout=10)
    log.close()
    fail("packaged service did not write a valid discovery record")


def api_search(record: dict[str, object], params: dict[str, str]) -> tuple[int, object]:
    request = api_request(record, params)
    try:
        with urllib.request.urlopen(request, timeout=45) as response:
            return response.status, json.loads(response.read())
    except urllib.error.HTTPError as error:
        return error.code, json.loads(error.read())


def extract(archive: Path, root: Path) -> None:
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as package:
            package.extractall(root)
    else:
        with tarfile.open(archive) as package:
            package.extractall(root)


def executable(root: Path, name: str) -> Path:
    suffix = ".exe" if os.name == "nt" else ""
    path = root / f"{name}{suffix}"
    if not path.is_file():
        fail(f"archive is missing {path.name}")
    return path


def isolated_env(profile: Path, cache: Path, sidecar: Path) -> dict[str, str]:
    env = os.environ.copy()
    for key in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
        env.pop(key, None)
    env.update({
        "HOME": str(profile), "USERPROFILE": str(profile), "LOCALAPPDATA": str(profile / "AppData" / "Local"),
        "JULIE_HOME": str(profile / ".julie"), "JULIE_SESSION_HOOKS": "0", "JULIE_EMBEDDING_PROVIDER": "native",
        "JULIE_NATIVE_SIDECAR_PROGRAM": str(sidecar), "JULIE_EMBEDDING_CACHE_DIR": str(cache),
    })
    return env


def offline_https_env(env: dict[str, str]) -> dict[str, str]:
    offline = env.copy()
    offline.update({"HTTPS_PROXY": "http://127.0.0.1:9", "https_proxy": "http://127.0.0.1:9",
                    "NO_PROXY": "127.0.0.1,localhost,::1", "no_proxy": "127.0.0.1,localhost,::1"})
    return offline


def prepare(sidecar: Path, env: dict[str, str], cache: Path) -> None:
    result = run([str(sidecar), "prepare", "--model", MODEL], env)
    if result.returncode:
        fail(f"online model prepare exited {result.returncode}: {result.stderr.strip()}")
    record = done_record(result.stdout)
    if record.get("sha256") != MODEL_SHA256:
        fail("prepare done record does not match the pinned model SHA-256")
    model = cache / f"{MODEL}.gguf"
    if not model.is_file() or hashlib.sha256(model.read_bytes()).hexdigest() != MODEL_SHA256:
        fail("prepared model file does not match the pinned model SHA-256")


def scratch_workspace(root: Path) -> Path:
    workspace = root / "workspace"
    workspace.mkdir()
    (workspace / "semantic_probe.rs").write_text("pub fn semantic_probe() -> &'static str { \"native semantic readiness\" }\n")
    result = run(["git", "init", "-q"], os.environ.copy(), workspace)
    if result.returncode:
        fail(f"could not initialize scratch Git workspace: {result.stderr.strip()}")
    return workspace


def qualify(archive: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="julie-native-offline-") as temporary:
        root = Path(temporary)
        extracted, profile, cache = root / "archive", root / "profile", root / "cache"
        extracted.mkdir()
        profile.mkdir()
        cache.mkdir()
        extract(archive, extracted)
        server, sidecar = executable(extracted, "julie-server"), executable(extracted, "julie-semantic-sidecar")
        env = isolated_env(profile, cache, sidecar)
        prepare(sidecar, env, cache)
        workspace = scratch_workspace(root)
        offline = offline_https_env(env)
        process = None
        log = None
        try:
            process, log, record = start_service(server, offline, root)
            deadline = time.monotonic() + 120
            last_status: int | None = None
            last_response: object | None = None
            last_error: str | None = None
            while time.monotonic() < deadline:
                try:
                    last_status, last_response = api_search(record, search_params(workspace))
                except (OSError, json.JSONDecodeError, urllib.error.URLError) as error:
                    last_error = str(error)
                if last_status == 200 and last_response and required_search_succeeded(last_response, "semantic_probe"):
                    return
                time.sleep(1)
            if last_status is None:
                fail(f"required semantic search did not run; last_error={last_error!r}")
            fail("required semantic search never reached ready/full coverage with semantic_probe hit; "
                 f"last_http_status={last_status}; last_error={last_error!r}; last_response={last_response!r}; "
                 f"last_readiness={last_response.get('readiness') if isinstance(last_response, dict) else None!r}")
        finally:
            stopped = run([str(server), "service", "stop"], offline, timeout=30)
            if process and process.poll() is None:
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    process.wait(timeout=10)
            if log:
                log.close()
            if stopped.returncode:
                fail(f"owned packaged service stop exited {stopped.returncode}: {stopped.stderr.strip()}")


def self_test() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        fake = Path(temporary) / "fake.py"
        fake.write_text("import json\nprint(json.dumps({'event':'done','model_id':'bge-small-en-v1.5-f32','sha256':'bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65'}))\n")
        result = run([sys.executable, str(fake)], os.environ.copy())
        if result.returncode or done_record(result.stdout)["sha256"] != MODEL_SHA256:
            fail("fake prepare response was not accepted")
    response = {
        "readiness": {"mode": "required", "status": "ready", "coverage": "full"},
        "result": {"structuredContent": {"backend": "semantic", "trace": {"backend_fallback": False}, "hits": [{"name": "semantic_probe"}]}},
    }
    if not required_search_succeeded(response, "semantic_probe") or required_search_succeeded(response, "other_symbol"):
        fail("semantic response contract check failed")
    if search_params(Path("workspace")).get("semantics") != "required":
        fail("required semantic search params must set semantics=required")
    record = {"port": 8123, "token": "test-token"}
    request = api_request(record, search_params(Path("workspace")))
    if service_command(Path("julie-server")) != ["julie-server", "service"] or request.full_url != "http://127.0.0.1:8123/api/fast_search" or request.get_header("Authorization") != "Bearer test-token":
        fail("required semantic search must use the owned service JSON API with bearer auth")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path, nargs="?")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not args.archive or not args.archive.is_file():
        parser.error("archive must name an existing package")
    qualify(args.archive)


if __name__ == "__main__":
    main()
