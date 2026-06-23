#!/usr/bin/env python3
"""Standalone stdio test harness for server.py.

Spawns the wrapper as a subprocess, speaks newline-delimited JSON-RPC to it,
and exercises: initialize -> tools/list -> (find window board id via execute_code)
-> export_shape_description on that board. Prints a structured summary.

This proves the wrapper works WITHOUT the raw image ever leaving the wrapper.
"""
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SERVER = os.path.join(HERE, "server.py")


class Client:
    def __init__(self, proc):
        self.proc = proc
        self._id = 0

    def _send(self, obj):
        self.proc.stdin.write(json.dumps(obj) + "\n")
        self.proc.stdin.flush()

    def _read(self):
        line = self.proc.stdout.readline()
        if not line:
            err = self.proc.stderr.read()
            raise RuntimeError(f"server closed stdout. stderr:\n{err}")
        return json.loads(line)

    def request(self, method, params=None):
        self._id += 1
        self._send({"jsonrpc": "2.0", "id": self._id, "method": method,
                    "params": params or {}})
        return self._read()

    def notify(self, method, params=None):
        self._send({"jsonrpc": "2.0", "method": method, "params": params or {}})


def main():
    env = dict(os.environ)
    proc = subprocess.Popen(
        [sys.executable, SERVER],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, env=env, bufsize=1,
    )
    c = Client(proc)
    out = {}
    try:
        # 1) initialize
        init = c.request("initialize", {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"},
        })
        c.notify("notifications/initialized")
        out["serverInfo"] = init.get("result", {}).get("serverInfo")

        # 2) tools/list — assert wrapped tool present, original hidden
        tl = c.request("tools/list")
        names = [t["name"] for t in tl.get("result", {}).get("tools", [])]
        out["tools"] = names
        out["has_wrapped"] = "export_shape_description" in names
        out["original_hidden"] = "export_shape" not in names

        # 3) find the window board id via the proxied execute_code
        ec = c.request("tools/call", {
            "name": "execute_code",
            "arguments": {"code": 'return penpotUtils.findShape(s=>s.name==="Remote AI IDE — Window").id;'},
        })
        raw = ec.get("result", {}).get("content", [{}])[0].get("text", "")
        try:
            board_id = json.loads(raw).get("result")
        except Exception:
            board_id = None
        out["board_id"] = board_id

        # 4) export_shape_description on that board
        if board_id:
            desc = c.request("tools/call", {
                "name": "export_shape_description",
                "arguments": {"shapeId": board_id, "format": "png"},
            })
            res = desc.get("result", {})
            text = res.get("content", [{}])[0].get("text", "")
            out["description_isError"] = res.get("isError")
            out["description_len"] = len(text)
            out["description"] = text
    finally:
        try:
            proc.stdin.close()
        except Exception:
            pass
        try:
            proc.wait(timeout=5)
        except Exception:
            proc.kill()

    print(json.dumps(out, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
