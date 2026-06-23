#!/usr/bin/env python3
"""Probe the upstream Penpot MCP (Streamable HTTP) to learn the handshake and
the exact result shape of export_shape (so the wrapper can extract the image)."""
import json
import os
import sys
import httpx

URL = os.environ["PENPOT_MCP_URL"]

# Streamable-HTTP MCP: POST JSON-RPC, server may reply as application/json OR
# as text/event-stream (SSE). We must send Accept for both and parse SSE.
HEADERS = {
    "Content-Type": "application/json",
    "Accept": "application/json, text/event-stream",
}


def parse_response(resp):
    ctype = resp.headers.get("content-type", "")
    if "text/event-stream" in ctype:
        # parse SSE frames; collect data lines, return last JSON object
        last = None
        for line in resp.text.splitlines():
            line = line.strip()
            if line.startswith("data:"):
                payload = line[len("data:"):].strip()
                if payload and payload != "[DONE]":
                    try:
                        last = json.loads(payload)
                    except json.JSONDecodeError:
                        pass
        return last
    return resp.json()


def main():
    sid = None
    with httpx.Client(timeout=120, trust_env=False) as client:
        # 1) initialize
        init = {
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "penpot-vision-probe", "version": "0.1"},
            },
        }
        r = client.post(URL, headers=HEADERS, json=init)
        sid = r.headers.get("mcp-session-id")
        print("== initialize ==")
        print("status", r.status_code, "session", sid)
        print(json.dumps(parse_response(r), indent=2)[:600])

        h2 = dict(HEADERS)
        if sid:
            h2["mcp-session-id"] = sid

        # 2) notifications/initialized
        client.post(URL, headers=h2, json={
            "jsonrpc": "2.0", "method": "notifications/initialized", "params": {}
        })

        # 3) tools/list
        r = client.post(URL, headers=h2, json={
            "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}
        })
        tl = parse_response(r)
        tools = tl.get("result", {}).get("tools", []) if tl else []
        print("\n== tools/list ==")
        for t in tools:
            print(" -", t["name"])

        # 4) call export_shape on 'page' and inspect the result content shape
        print("\n== export_shape(page) result shape ==")
        r = client.post(URL, headers=h2, json={
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "export_shape", "arguments": {"shapeId": "page", "format": "png"}}
        })
        res = parse_response(r)
        result = (res or {}).get("result", {})
        content = result.get("content", [])
        summary = []
        for part in content:
            item = {"type": part.get("type")}
            for k in ("mimeType", "name"):
                if k in part:
                    item[k] = part[k]
            if "data" in part:
                item["data_len"] = len(part["data"])
                item["data_head"] = part["data"][:32]
            if "text" in part:
                item["text_head"] = part["text"][:80]
            summary.append(item)
        print(json.dumps({"isError": result.get("isError"), "content": summary}, indent=2))


if __name__ == "__main__":
    main()
