#!/usr/bin/env python3
"""Run Plan 1 live checks against an explicit prebuilt julie-server binary."""

import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import re
import selectors
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


DEADLINE_SECONDS = 30


def wait_until(label, check, timeout=DEADLINE_SECONDS):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        try:
            value = check()
            if value:
                return value
        except (FileNotFoundError, sqlite3.Error, urllib.error.URLError) as error:
            last = str(error)
        time.sleep(0.1)
    raise RuntimeError(f"timed out waiting for {label}: {last or 'condition remained false'}")


class Probe:
    def __init__(self, binary, root):
        self.binary = binary
        self.root = root
        self.home = root / "home"
        self.checkout = root / "checkout"
        self.log_dir = root / "logs"
        self.process = None
        self.log = None
        self.record = None
        self.env = os.environ.copy()
        self.env.update(
            {
                "JULIE_HOME": str(self.home),
                "JULIE_EMBEDDING_PROVIDER": "native",
                "JULIE_NATIVE_SIDECAR_PROGRAM": str(self.write_fake_sidecar()),
                "JULIE_NATIVE_SIDECAR_MODEL": "fake",
                "JULIE_SERVICE_IDLE_SECS": "0",
            }
        )

    def write_fake_sidecar(self):
        path = self.root / "fake-sidecar.py"
        path.write_text(
            """#!/usr/bin/env python3
import json
import sys

batch_calls = 0
for line in sys.stdin:
    request = json.loads(line)
    request_id = request["request_id"]
    method = request["method"]
    result = None
    error = None
    if method == "health":
        result = {
            "ready": True,
            "model_id": "fake",
            "model_sha256": "a" * 64,
            "dims": 3,
            "pooling": "cls",
            "normalization": "l2",
            "instruction_policy_version": 1,
            "llama_cpp_build": "live-probe",
            "device": "cpu",
            "runtime": "fake",
            "accelerated": False,
        }
    elif method == "embed_query":
        result = {"vector": [1.0, 0.0, 0.0], "dims": 3}
    elif method == "embed_batch":
        batch_calls += 1
        if batch_calls == 1:
            result = {
                "vectors": [
                    [1.0, 0.0, 0.0]
                    for _ in request["params"]["texts"]
                ],
                "dims": 3,
            }
        else:
            error = {
                "code": "LIVE_PROBE_INTERRUPTION",
                "message": "deliberate second-batch interruption",
            }
    elif method == "shutdown":
        break
    response = {
        "schema": "julie.embedding.sidecar",
        "version": 1,
        "request_id": request_id,
        "result": result,
        "error": error,
    }
    print(json.dumps(response), flush=True)
"""
        )
        path.chmod(0o700)
        return path

    def start(self):
        self.home.mkdir(exist_ok=True)
        self.log_dir.mkdir(exist_ok=True)
        self.log = (self.log_dir / f"service-{time.time_ns()}.log").open("wb")
        self.process = subprocess.Popen(
            [str(self.binary), "service"],
            env=self.env,
            stdin=subprocess.DEVNULL,
            stdout=self.log,
            stderr=subprocess.STDOUT,
        )

        def ready():
            if self.process.poll() is not None:
                raise RuntimeError(f"service exited with {self.process.returncode}")
            path = self.home / "service.json"
            if not path.exists():
                return None
            record = json.loads(path.read_text())
            if not {"port", "token", "pid", "version"} <= record.keys():
                return None
            self.record = record
            return record

        wait_until("service discovery", ready)
        self.status()

    def request(self, method, path, body=None):
        data = None if body is None else json.dumps(body).encode()
        request = urllib.request.Request(
            f"http://127.0.0.1:{self.record['port']}{path}",
            data=data,
            method=method,
            headers={
                "Authorization": f"Bearer {self.record['token']}",
                "Content-Type": "application/json",
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=10) as response:
                return response.status, json.loads(response.read())
        except urllib.error.HTTPError as error:
            return error.code, json.loads(error.read())

    def api(self, tool, body):
        return self.request("POST", f"/api/{tool}", body)

    def status(self):
        status, body = self.request("GET", "/status")
        if status != 200:
            raise RuntimeError(f"status failed with HTTP {status}")
        return body

    def stop(self):
        if not self.process:
            return
        try:
            self.request("POST", "/shutdown", {})
        except Exception:
            self.process.terminate()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)
        if self.log:
            self.log.close()
        self.process = None
        self.record = None


def assert_api(status, body, label):
    if status != 200:
        raise RuntimeError(f"{label} failed with HTTP {status}: {redacted(body)}")
    return body


def redacted(value):
    if isinstance(value, dict):
        return {
            key: "<redacted>" if "token" in key.lower() else redacted(item)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [redacted(item) for item in value]
    return value


def checkout_status(probe, checkout):
    checkout = str(checkout.resolve())
    return next(
        (
            item
            for item in probe.status().get("checkouts", [])
            if item.get("root") == checkout
        ),
        None,
    )


def vector_rows(database):
    connection = sqlite3.connect(f"file:{database}?mode=ro", uri=True)
    try:
        return connection.execute(
            "SELECT blob_hash, symbol_ordinal, hex(vector) "
            "FROM vectors ORDER BY blob_hash, symbol_ordinal"
        ).fetchall()
    finally:
        connection.close()


def readiness(body):
    value = body.get("readiness", {}) if isinstance(body, dict) else {}
    return {
        key: value.get(key)
        for key in (
            "mode",
            "status",
            "coverage",
            "eligible_symbols",
            "embedded_symbols",
        )
        if key in value
    }


def search(probe, checkout, query, semantics="off", backend="lexical"):
    status, body = probe.api(
        "fast_search",
        {
            "workspace": str(checkout),
            "query": query,
            "semantics": semantics,
            "backend": backend,
        },
    )
    return status, body


def result_text(body):
    result = body.get("result", body)
    return "\n".join(
        item.get("text", "")
        for item in result.get("content", [])
        if item.get("type") == "text"
    )


def has_search_hit(body, symbol):
    text = result_text(body)
    header, _, rows = text.partition("\n")
    match = re.match(r"(\d+) hits? for ", header)
    return bool(match and int(match.group(1)) > 0 and symbol in rows)


def stdio_mcp(probe, checkout):
    shim = subprocess.Popen(
        [str(probe.binary)],
        env=probe.env,
        cwd=probe.root,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    selector = selectors.DefaultSelector()
    selector.register(shim.stdout, selectors.EVENT_READ)

    def read_response(label):
        if not selector.select(timeout=10):
            raise RuntimeError(f"stdio timed out waiting for {label}")
        line = shim.stdout.readline()
        if not line:
            raise RuntimeError(f"stdio closed before {label}")
        return json.loads(line)

    try:
        shim.stdin.write(
            json.dumps(
                {
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2025-06-18",
                        "capabilities": {},
                        "clientInfo": {
                            "name": "revival-live-probe",
                            "version": "1",
                        },
                    },
                }
            )
            + "\n"
        )
        shim.stdin.flush()
        initialize = read_response("initialize response")

        for message in (
            {
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {},
            },
            {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "fast_search",
                    "arguments": {
                        "workspace": str(checkout),
                        "query": "fresh_v2",
                        "semantics": "off",
                        "backend": "lexical",
                    },
                },
            },
        ):
            shim.stdin.write(json.dumps(message) + "\n")
        shim.stdin.flush()
        called = read_response("tools/call response")
        if initialize.get("result", {}).get("serverInfo", {}).get("version") != probe.record["version"]:
            raise RuntimeError("stdio initialize returned the wrong service version")
        if not has_search_hit(called.get("result", {}), "fresh_v2"):
            raise RuntimeError(f"stdio tools/call missed explicit workspace: {redacted(called)}")
        evidence = (
            called.get("result", {})
            .get("_meta", {})
            .get("io.julie/readiness")
        )
        if not isinstance(evidence, dict) or evidence.get("readiness", {}).get("mode") != "off":
            raise RuntimeError("stdio tools/call omitted structured readiness metadata")
        return {
            "server_version": probe.record["version"],
            "readiness": evidence["readiness"],
        }
    finally:
        selector.close()
        if shim.stdin:
            shim.stdin.close()
        try:
            shim.wait(timeout=5)
        except subprocess.TimeoutExpired:
            shim.terminate()
            shim.wait(timeout=5)


def run(binary):
    summary = {}
    with tempfile.TemporaryDirectory(prefix="julie-revival-live-") as directory:
        root = Path(directory)
        probe = Probe(binary.resolve(), root)
        checkout = probe.checkout
        checkout.mkdir()
        (checkout / ".git").mkdir()
        source = checkout / "lib.rs"
        source.write_text(
            "".join(f"pub fn symbol_{index:02}() {{}}\n" for index in range(51))
        )

        try:
            probe.start()
            status, body = probe.api(
                "manage_workspace", {"operation": "index", "path": str(checkout)}
            )
            assert_api(status, body, "initial index")
            def partial_state():
                value = checkout_status(probe, checkout)
                if not value or value.get("symbol_count") != 51:
                    return None
                if value.get("vector_count", 0) > 50:
                    raise RuntimeError(
                        f"expected partial vector coverage, got {redacted(value)}"
                    )
                return value if value.get("vector_count") == 50 else None

            state = wait_until("partial vector coverage", partial_state)
            workspace_id = state["workspace_id"]
            database = probe.home / "indexes" / workspace_id / "facts.sqlite"
            partial = vector_rows(database)
            if len(partial) != 50:
                raise RuntimeError(f"expected 50 partial vectors, got {len(partial)}")
            probe.stop()

            probe.start()
            if vector_rows(database) != partial:
                raise RuntimeError("stored partial vector changed before restart recovery")
            status, body = probe.api(
                "manage_workspace", {"operation": "open", "path": str(checkout)}
            )
            assert_api(status, body, "restart open")
            complete = wait_until(
                "complete vector coverage",
                lambda: (
                    rows
                    if len(rows := vector_rows(database)) == 51
                    else None
                ),
            )
            if not set(partial).issubset(complete):
                raise RuntimeError("restart replaced a previously stored vector")
            if len(set(complete) - set(partial)) != 1:
                raise RuntimeError("restart did not add exactly the missing vector")

            with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
                off_future = pool.submit(
                    search, probe, checkout, "symbol_00", "off", "lexical"
                )
                required_future = pool.submit(
                    search, probe, checkout, "symbol 50", "required", "semantic"
                )
                off_status, off_body = off_future.result(timeout=10)
                req_status, req_body = required_future.result(timeout=10)
            assert_api(off_status, off_body, "overlapping Off request")
            assert_api(req_status, req_body, "overlapping Required request")
            if not has_search_hit(off_body, "symbol_00"):
                raise RuntimeError("overlapping semantic modes interfered")
            off_readiness = readiness(off_body)
            required_readiness = readiness(req_body)
            if off_readiness.get("mode") != "off":
                raise RuntimeError(f"JSON API lost Off mode: {off_readiness}")
            if required_readiness.get("mode") != "required":
                raise RuntimeError(
                    f"JSON API lost Required mode: {required_readiness}"
                )
            summary["overlap"] = {
                "off": off_readiness,
                "required": required_readiness,
            }

            source.write_text("pub fn fresh_v1() {}\n")
            wait_until(
                "first rapid write",
                lambda: has_search_hit(search(probe, checkout, "fresh_v1")[1], "fresh_v1"),
            )
            source.write_text("pub fn fresh_v2() {}\n")

            def latest_visible():
                _, latest = search(probe, checkout, "fresh_v2")
                _, stale = search(probe, checkout, "fresh_v1")
                return latest if has_search_hit(latest, "fresh_v2") and not has_search_hit(stale, "fresh_v1") else None

            wait_until("second rapid write", latest_visible)

            second = root / "second-checkout"
            second.mkdir()
            (second / ".git").mkdir()
            (second / "lib.rs").write_text("pub fn isolated_marker() {}\n")
            status, body = probe.api(
                "manage_workspace", {"operation": "index", "path": str(second)}
            )
            assert_api(status, body, "second checkout index")
            wait_until(
                "second checkout lexical result",
                lambda: has_search_hit(
                    search(probe, second, "isolated_marker")[1],
                    "isolated_marker",
                ),
            )
            if has_search_hit(
                search(probe, checkout, "isolated_marker")[1],
                "isolated_marker",
            ):
                raise RuntimeError("second checkout symbol leaked into the first")

            summary["stdio_mcp"] = stdio_mcp(probe, checkout)
            summary.update(
                {
                    "binary": str(binary),
                    "service_version": probe.record["version"],
                    "partial_vectors": len(partial),
                    "complete_vectors": len(complete),
                    "preserved_vector": True,
                    "rapid_write_latest": "fresh_v2",
                    "workspace_isolation": True,
                    "fake_encoder_only": True,
                }
            )
        except Exception:
            evidence = Path(__file__).with_name("live-probe-failure")
            if evidence.exists():
                shutil.rmtree(evidence)
            evidence.mkdir()
            for log in probe.log_dir.glob("*.log"):
                shutil.copy2(log, evidence / log.name)
            try:
                status_body = probe.status()
                (evidence / "status.json").write_text(
                    json.dumps(redacted(status_body), indent=2, sort_keys=True)
                )
                service_log = Path(status_body.get("log", ""))
                if service_log.is_file():
                    (evidence / "service.log").write_text(service_log.read_text())
            except Exception as error:
                (evidence / "status-error.txt").write_text(str(error))
            raise
        finally:
            probe.stop()
        summary["cleanup"] = {
            "service_exited": probe.process is None,
            "temporary_home_removed": True,
        }
    print(json.dumps(summary, sort_keys=True))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    args = parser.parse_args()
    if not args.binary.is_file():
        parser.error(f"binary does not exist: {args.binary}")
    try:
        run(args.binary)
    except Exception as error:
        print(
            json.dumps(
                {"ok": False, "error": str(error), "cleanup": "attempted"},
                sort_keys=True,
            ),
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
