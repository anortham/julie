"""Minimal JSON-RPC client helpers for the Julie/Miller head-to-head matrix."""

from __future__ import annotations

import json
import select
import subprocess
import time
from typing import Any


def _now_ms() -> int:
    return int(time.perf_counter() * 1000)


def content_text(message: dict[str, Any]) -> str:
    if "result" not in message:
        return json.dumps(message, sort_keys=True)
    parts: list[str] = []
    for item in message["result"].get("content", []):
        if item.get("type") == "text":
            parts.append(item.get("text", ""))
    return "\n".join(parts)


class McpProcess:
    def __init__(self, args: list[str], timeout: int = 60, cwd: str | None = None) -> None:
        self.proc = subprocess.Popen(
            args,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
            cwd=cwd,
        )
        self.next_id = 1
        self.stderr_lines: list[str] = []
        init = self.request(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "miller-julie-bench", "version": "0"},
            },
            timeout=timeout,
        )
        if "error" in init:
            raise RuntimeError(init["error"])
        self.notify("notifications/initialized", {})

    def notify(self, method: str, params: dict[str, Any]) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method, "params": params}) + "\n")
        self.proc.stdin.flush()

    def request(self, method: str, params: dict[str, Any], timeout: int = 60) -> dict[str, Any]:
        request_id = self.next_id
        self.next_id += 1
        assert self.proc.stdin is not None
        self.proc.stdin.write(
            json.dumps({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}) + "\n"
        )
        self.proc.stdin.flush()
        return self._read_response(request_id, timeout)

    def call_tool(self, name: str, arguments: dict[str, Any], timeout: int = 90) -> tuple[int, dict[str, Any]]:
        start = _now_ms()
        response = self.request("tools/call", {"name": name, "arguments": arguments}, timeout=timeout)
        return _now_ms() - start, response

    def _read_response(self, request_id: int, timeout: int) -> dict[str, Any]:
        assert self.proc.stdout is not None
        assert self.proc.stderr is not None
        start = time.perf_counter()
        while time.perf_counter() - start < timeout:
            readable, _, _ = select.select([self.proc.stdout, self.proc.stderr], [], [], 0.1)
            for stream in readable:
                line = stream.readline()
                if not line:
                    continue
                if stream is self.proc.stderr:
                    self.stderr_lines.append(line.rstrip())
                    continue
                try:
                    message = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if message.get("id") == request_id:
                    return message
        raise TimeoutError(f"timed out waiting for JSON-RPC id {request_id}")

    def close(self) -> None:
        self.proc.terminate()
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
