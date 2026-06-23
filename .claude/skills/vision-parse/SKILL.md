---
name: vision-parse
description: Parse, describe, OCR, or analyze an image using a vision language model. Use whenever a task requires understanding image content (screenshots, photos, diagrams, UI mockups, scanned documents, charts) and the local model cannot see images. Routes to a third-party OpenAI-compatible VLM endpoint.
---

# vision-parse

Use a third-party vision language model (VLM) to interpret images. The local
model in this environment cannot view images, so **any** time a task needs an
image's content read, described, OCR'd, or analyzed, call this skill instead of
trying to look at the image yourself.

## When to use

- Describing what is in a screenshot, photo, diagram, or UI mockup
- OCR / extracting text from an image
- Reading charts, tables, or graphs in an image
- Comparing a design mockup against a description
- Any question that depends on the visual content of an image file or URL

## How to use

Run the helper script with your prompt and one or more images. Images may be
local file paths or `http(s)` URLs.

```bash
.claude/skills/vision-parse/vlm.py "PROMPT" IMAGE [IMAGE...]
```

Examples:

```bash
# Describe a screenshot
.claude/skills/vision-parse/vlm.py "Describe this UI in detail" ./mockup.png

# OCR
.claude/skills/vision-parse/vlm.py "Extract all visible text verbatim" ./scan.jpg

# Multiple images in one request
.claude/skills/vision-parse/vlm.py "What changed between these two screens?" before.png after.png

# Image from a URL
.claude/skills/vision-parse/vlm.py "What is in this image?" https://example.com/pic.png
```

The script prints the model's text answer to stdout. Use that answer to continue
the task.

## Provider configuration

Defaults are baked into the script and can be overridden via env vars:

- `VLM_BASE_URL`  (default `http://10.239.15.43/v1/`)
- `VLM_API_KEY`   (default the configured key)
- `VLM_MODEL`     (default `Qwen3-VL-30B-A3B-Instruct`)

The endpoint is OpenAI Chat Completions compatible. The script bypasses any
`http_proxy` / `https_proxy` env vars, since the endpoint is on an internal
network that must not be routed through a corporate proxy.
