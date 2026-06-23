#!/usr/bin/env python3
"""penpot-vision wrapper MCP server (stdio).

A thin forwarder in front of the real Penpot MCP server (Streamable HTTP).
Almost every request is proxied through unchanged. The ONE special case:

    export_shape           (upstream)  -- returns a PNG/SVG image
    export_shape_description (exposed)  -- same args, but the image is sent to a
                                          third-party VLM and a *text* description
                                          of the visual/layout is returned.

Why a wrapper: the local model in this environment cannot view images. So we
never surface the raw image to it. Instead the image stays inside this process,
goes to the VLM, and only text comes back.

Transport: stdio JSON-RPC 2.0 (newline-delimited) to the Claude Code client.
Upstream: HTTP JSON-RPC, replies may be application/json or text/event-stream.

Config (env, with sensible discovery fallbacks):
  PENPOT_MCP_URL   full upstream URL incl. ?userToken=...  (required; can be
                   auto-discovered from ~/.claude.json if not set)
  VLM_BASE_URL / VLM_API_KEY / VLM_MODEL   forwarded to the VLM call
  VISION_PROMPT    override the description prompt
"""
import json
import os
import sys
import base64
import mimetypes
import tempfile
import subprocess
import urllib.request
import urllib.error

import httpx

# ---------------------------------------------------------------- config ----

HERE = os.path.dirname(os.path.abspath(__file__))
VLM_SCRIPT = os.path.normpath(os.path.join(HERE, "..", "skills", "vision-parse", "vlm.py"))

ORIGINAL_TOOL = "export_shape"
WRAPPED_TOOL = "export_shape_description"

DEFAULT_VISION_PROMPT = (
    "You are inspecting a UI design exported from Penpot. Describe this image in "
    "precise detail for a designer who cannot see it. Cover: (1) overall layout "
    "and structure (regions, panels, columns, bars) with their relative position "
    "and proportion; (2) every distinct UI element you can identify (buttons, "
    "icons, text labels, lists, cards, inputs) and its location; (3) all readable "
    "text, transcribed verbatim; (4) the color scheme (background, accents, text); "
    "(5) anything that looks misaligned, overlapping, cut off, or visually broken. "
    "Be concrete about spatial relationships (left/right/top/bottom, what is inside "
    "what). Do not invent elements that are not visible."
)


def discover_url():
    # 1) explicit env override
    url = os.environ.get("PENPOT_MCP_URL")
    if url:
        return url
    # 2) sidecar config next to this script (self-contained; no dependency on
    #    the Claude Code `penpot` MCP registration)
    sidecar = os.path.join(HERE, "config.json")
    try:
        with open(sidecar) as f:
            u = json.load(f).get("penpot_mcp_url")
        if u:
            return u
    except (OSError, ValueError):
        pass
    # 3) last-resort fallback: scrape it out of ~/.claude.json if a `penpot`
    #    server still happens to be registered (kept only for resilience)
    cfg = os.path.expanduser("~/.claude.json")
    try:
        data = json.load(open(cfg))
    except Exception:
        return None

    def find(o):
        if isinstance(o, dict):
            for k, v in o.items():
                if k == "penpot" and isinstance(v, dict) and "url" in v:
                    return v["url"]
                r = find(v)
                if r:
                    return r
        elif isinstance(o, list):
            for v in o:
                r = find(v)
                if r:
                    return r
        return None

    return find(data)


UPSTREAM_URL = discover_url()

# --------------------------------------------------------- upstream client --

_HTTP_HEADERS = {
    "Content-Type": "application/json",
    "Accept": "application/json, text/event-stream",
}


class Upstream:
    """Maintains one initialized MCP session against the HTTP server."""

    def __init__(self, url):
        self.url = url
        self.session_id = None
        self.client = httpx.Client(timeout=180, trust_env=False)
        self._next_id = 1000

    def _headers(self):
        h = dict(_HTTP_HEADERS)
        if self.session_id:
            h["mcp-session-id"] = self.session_id
        return h

    @staticmethod
    def _parse(resp):
        ctype = resp.headers.get("content-type", "")
        if "text/event-stream" in ctype:
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
        if resp.text.strip():
            return resp.json()
        return None

    def initialize(self):
        req = {
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "penpot-vision-wrapper", "version": "0.1"},
            },
        }
        r = self.client.post(self.url, headers=self._headers(), json=req)
        self.session_id = r.headers.get("mcp-session-id")
        self._parse(r)
        # required notification
        self.client.post(self.url, headers=self._headers(),
                         json={"jsonrpc": "2.0", "method": "notifications/initialized",
                               "params": {}})

    def request(self, method, params):
        """Send a JSON-RPC request upstream, return the full result object."""
        self._next_id += 1
        req = {"jsonrpc": "2.0", "id": self._next_id, "method": method, "params": params}
        r = self.client.post(self.url, headers=self._headers(), json=req)
        parsed = self._parse(r)
        return parsed or {}

    def call_tool(self, name, arguments):
        """Return the inner tools/call result ({content, isError}), unwrapped
        from the JSON-RPC envelope."""
        envelope = self.request("tools/call", {"name": name, "arguments": arguments})
        return envelope.get("result", envelope)


# --------------------------------------------------------------- VLM call ---

def describe_image(data_b64, mime, prompt):
    """Write the image to a temp file and run the vision-parse vlm.py on it.

    Reusing vlm.py keeps a single source of truth for provider config and proxy
    bypass. Returns the model's text answer (or raises with the error text).
    """
    ext = mimetypes.guess_extension(mime) or ".png"
    fd, path = tempfile.mkstemp(suffix=ext, prefix="penpot_export_")
    try:
        with os.fdopen(fd, "wb") as f:
            f.write(base64.b64decode(data_b64))
        env = dict(os.environ)
        proc = subprocess.run(
            [sys.executable, VLM_SCRIPT, prompt, path],
            capture_output=True, text=True, env=env, timeout=300,
        )
        if proc.returncode != 0:
            raise RuntimeError(proc.stderr.strip() or proc.stdout.strip()
                               or f"vlm.py exited {proc.returncode}")
        return proc.stdout.strip()
    finally:
        try:
            os.remove(path)
        except OSError:
            pass


# --------------------------------------------------------- tool rewriting ---

def wrap_tools_list(result):
    """Rename export_shape -> export_shape_description in the advertised list."""
    tools = result.get("tools", [])
    for t in tools:
        if t.get("name") == ORIGINAL_TOOL:
            t["name"] = WRAPPED_TOOL
            base = t.get("description", "")
            t["description"] = (
                "Export a shape (or 'page'/'selection') from Penpot and return a "
                "detailed TEXT description of its visual appearance and layout, "
                "produced by a vision model. Use this when you need to 'see' how a "
                "shape looks. Same arguments as the underlying export. "
                "(Underlying export: " + base + ")"
            )
    return result


def handle_export_description(up, arguments):
    """Run the real export_shape, then describe the resulting image via the VLM."""
    # Force an image format the VLM can read; default to png.
    args = dict(arguments or {})
    fmt = args.get("format", "png")
    extra_prompt = args.pop("visionPrompt", None)  # optional caller override
    if fmt == "svg":
        # VLM endpoint expects raster; coerce to png for the description path.
        args["format"] = "png"
        fmt = "png"

    result = up.call_tool(ORIGINAL_TOOL, args)
    content = result.get("content", [])

    # Find the image part
    image_part = next((p for p in content if p.get("type") == "image" and p.get("data")), None)
    if image_part is None:
        # Upstream returned an error or non-image (e.g. 'page' http error); pass it through.
        texts = [p.get("text", "") for p in content if p.get("type") == "text"]
        msg = "export_shape did not return an image. Upstream said: " + \
              (" ".join(t for t in texts if t).strip() or "(no detail)")
        return {"content": [{"type": "text", "text": msg}], "isError": True}

    prompt = extra_prompt or os.environ.get("VISION_PROMPT") or DEFAULT_VISION_PROMPT
    try:
        description = describe_image(image_part["data"], image_part.get("mimeType", "image/png"), prompt)
    except Exception as e:  # noqa: BLE001
        return {"content": [{"type": "text", "text": f"vision description failed: {e}"}],
                "isError": True}

    shape_id = (arguments or {}).get("shapeId", "?")
    header = f"Visual description of shape '{shape_id}' (via vision model):\n\n"
    return {"content": [{"type": "text", "text": header + description}], "isError": False}


# ------------------------------------------------------------- stdio loop ---

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def make_result(req_id, result):
    return {"jsonrpc": "2.0", "id": req_id, "result": result}


def make_error(req_id, code, message):
    return {"jsonrpc": "2.0", "id": req_id, "error": {"code": code, "message": message}}


def main():
    if not UPSTREAM_URL:
        sys.stderr.write("penpot-vision-wrapper: PENPOT_MCP_URL not set and could "
                         "not be discovered from ~/.claude.json\n")
        sys.exit(1)

    up = Upstream(UPSTREAM_URL)
    upstream_ready = False

    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError:
            continue

        method = msg.get("method")
        req_id = msg.get("id")
        is_notification = req_id is None

        try:
            if method == "initialize":
                # Answer the client ourselves; lazily init upstream.
                if not upstream_ready:
                    up.initialize()
                    upstream_ready = True
                send(make_result(req_id, {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {"tools": {"listChanged": False}},
                    "serverInfo": {"name": "penpot-vision", "version": "0.1.0"},
                    "instructions": (
                        "Wrapper around the Penpot MCP server. Identical to Penpot "
                        "except export_shape is replaced by export_shape_description, "
                        "which returns a vision-model TEXT description of the exported "
                        "image instead of the raw image."
                    ),
                }))

            elif method == "notifications/initialized":
                pass  # nothing to ack

            elif method == "tools/list":
                if not upstream_ready:
                    up.initialize(); upstream_ready = True
                result = up.request("tools/list", msg.get("params", {}) or {})
                inner = result.get("result", result)  # upstream wraps in {result:{...}}
                inner = wrap_tools_list(inner)
                send(make_result(req_id, inner))

            elif method == "tools/call":
                if not upstream_ready:
                    up.initialize(); upstream_ready = True
                params = msg.get("params", {}) or {}
                name = params.get("name")
                arguments = params.get("arguments", {}) or {}
                if name == WRAPPED_TOOL:
                    result = handle_export_description(up, arguments)
                    send(make_result(req_id, result))
                elif name == ORIGINAL_TOOL:
                    # Original name is hidden; refuse so the image never reaches us.
                    send(make_result(req_id, {
                        "content": [{"type": "text", "text":
                            f"'{ORIGINAL_TOOL}' is not available in this wrapper. "
                            f"Use '{WRAPPED_TOOL}' to get a text description instead."}],
                        "isError": True,
                    }))
                else:
                    result = up.call_tool(name, arguments)
                    inner = result.get("result", result)
                    send(make_result(req_id, inner))

            elif method == "ping":
                send(make_result(req_id, {}))

            elif is_notification:
                pass  # ignore other notifications

            else:
                # Forward any other request method verbatim.
                if not upstream_ready:
                    up.initialize(); upstream_ready = True
                result = up.request(method, msg.get("params", {}) or {})
                inner = result.get("result", result)
                send(make_result(req_id, inner))

        except Exception as e:  # noqa: BLE001
            if not is_notification:
                send(make_error(req_id, -32603, f"wrapper error: {e}"))


if __name__ == "__main__":
    main()
