# CLAUDE.md — Project Rules for Remote AI IDE

## Top-Level Rules

### 1. Design Decision Tracking
Every time a non-trivial design decision is made or a conclusion is reached during implementation,
update `design.md` with:
- The decision/conclusion
- Rationale (why this approach over alternatives)
- Any trade-offs or future considerations

Keep `design.md` organized by topic. Add new sections under `## 4. 设计决策记录` following the
existing numbered format.

### 2. Debugging: Log, Don't Guess

When the user reports a bug and you are **not 100% certain** of the root cause:
- **Add `tracing::info!` / `console.log` along every plausible code path** before making fixes
- Let the user run the instrumented build and report back with logs
- Only then apply the targeted fix based on evidence, not speculation

This saves both sides from wasted rebuild cycles on wrong guesses.

### 3. TODO Tracking
When you identify something that is "worth doing but not yet done":
- Add it to `TODO.txt` under the appropriate section
- Be specific: what, why, what files are involved
- Prefix with `⬜` for pending items
- When an item is completed, change `⬜` → `✅` but **keep it in TODO.txt** — don't delete it.
  This maintains a complete record of what was done and what remains.
- If a completed item represents a design decision, also document it in `design.md`

### 4. Periodic Review (triggered at session start or when work pauses)
At natural breakpoints (session start, task completion, user asks "what's next"):
1. Quickly scan `TODO.txt` for items that could be addressed now
2. If a TODO item has become stale or irrelevant, remove it
3. If a TODO item's scope has changed, update it
4. Proactively remind the user of high-priority pending items

### 5. Build & Release (OTA)

All builds go through `scripts/release.sh`. It handles Vite, Rust cross-compilation,
tar.gz packaging, pricing.json, and manifest.json generation. **Always run it after
code changes** — otherwise the OTA updater on Windows clients won't see updates.

**Which flag to use:**

| Scope | Command | What it builds |
|-------|---------|----------------|
| Only TS/React/JSON | `./scripts/release.sh --frontend-only` | Vite → `frontend.tar.gz` + `pricing.json` + `manifest.json` |
| Only Rust crates | `./scripts/release.sh --agent-only` | Agent binaries (Linux + Windows) + manifest |
| Only Tauri shell | `./scripts/release.sh --tauri-only` | `main.exe` + `loader-*` + manifest |

**Full workflow after changes:**

1. Run the appropriate release.sh flag
2. If `--frontend-only` was used, restore unchanged binaries from git:
   `git checkout HEAD -- dist/agent-* dist/loader-* dist/main.exe`
3. Force-add dist files (gitignored but tracked): `git add -f dist/`
4. Commit: `git commit -m "release: $(date -u +%Y-%m-%d).$(git rev-parse --short=7 HEAD) — <what changed>"`
5. Push — dist/ files are tracked in git despite `.gitignore` (force-added)

**What happens on the client:**
- Windows exe's background updater fetches `dist/manifest.json` every 30 min
- Compares local cache SHA256 → downloads mismatched/missing files to cache dir
- `frontend.tar.gz` is extracted to `frontend/` subdirectory
- `pricing.json` is stored standalone (also available via runtime fetch at the raw GitHub URL)

## Project Structure Quick Reference

| Layer | Path | Language |
|-------|------|----------|
| Frontend | `apps/frontend/src/` | TypeScript/React |
| Desktop Core | `apps/frontend/src-tauri/` | Rust (Tauri v2) |
| Shared Protocol | `crates/shared-protocol/` | Rust |
| Remote Agent Host | `crates/remote-agent-host/` | Rust (Linux binary) |
| Shared Types (TS) | `packages/shared-types/` | TypeScript |

## Key Architecture Facts

- SSH exec channel is binary-safe (raw bytes), not text-only
- Wire format: `[4-byte BE length][MessagePack ProtocolMessage]`
- remote-agent-host uses `--mode stdio` over SSH exec channel
- Bootstrap: detect → upload remote-agent-host → chmod +x → start
- Agent CLI (claude/copilot) auto-detected via `which`, auto-installed via `npm install -g`
- Docker: `docker exec -it <container> <cmd>` wrapping in handle_spawn
- Connection persistence: SQLite (long-term) + localStorage (form cache)
- serde: `#[serde(rename_all = "camelCase")]` on Rust structs matching frontend camelCase
