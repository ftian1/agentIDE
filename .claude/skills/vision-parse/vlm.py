#!/usr/bin/env python3
"""Call a third-party OpenAI-compatible vision language model on image(s).

Usage:
    vlm.py "PROMPT" IMAGE [IMAGE...]

IMAGE may be a local file path or an http(s) URL.
Prints the model's text response to stdout.
"""
import base64
import json
import mimetypes
import os
import sys
import urllib.request

BASE_URL = os.environ.get("VLM_BASE_URL", "http://10.239.15.43/v1/").rstrip("/")
API_KEY = os.environ.get("VLM_API_KEY", "sk-iPrf5zlrbgZT0VS5XV9u3w")
MODEL = os.environ.get("VLM_MODEL", "Qwen3-VL-30B-A3B-Instruct")


def image_to_part(ref):
    """Turn a file path or URL into an OpenAI image_url content part."""
    if ref.startswith("http://") or ref.startswith("https://"):
        return {"type": "image_url", "image_url": {"url": ref}}
    if not os.path.isfile(ref):
        sys.exit(f"error: image not found: {ref}")
    mime = mimetypes.guess_type(ref)[0] or "image/png"
    with open(ref, "rb") as f:
        b64 = base64.b64encode(f.read()).decode("ascii")
    return {"type": "image_url", "image_url": {"url": f"data:{mime};base64,{b64}"}}


def main():
    if len(sys.argv) < 3:
        sys.exit('usage: vlm.py "PROMPT" IMAGE [IMAGE...]')

    prompt = sys.argv[1]
    images = sys.argv[2:]

    content = [{"type": "text", "text": prompt}]
    content.extend(image_to_part(ref) for ref in images)

    payload = {
        "model": MODEL,
        "messages": [{"role": "user", "content": content}],
    }

    req = urllib.request.Request(
        f"{BASE_URL}/chat/completions",
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {API_KEY}",
        },
        method="POST",
    )

    # The VLM endpoint is internal; bypass any http(s)_proxy env vars, since
    # urllib (unlike curl) does not honor CIDR ranges in no_proxy.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    try:
        with opener.open(req, timeout=180) as resp:
            data = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", "replace")
        sys.exit(f"error: HTTP {e.code} from VLM endpoint:\n{body}")
    except Exception as e:  # noqa: BLE001
        sys.exit(f"error: request failed: {e}")

    try:
        print(data["choices"][0]["message"]["content"])
    except (KeyError, IndexError):
        sys.exit(f"error: unexpected response shape:\n{json.dumps(data, indent=2)}")


if __name__ == "__main__":
    main()
