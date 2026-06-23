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

### 2. TODO Tracking
When you identify something that is "worth doing but not yet done":
- Add it to `TODO.txt` under the appropriate section
- Be specific: what, why, what files are involved
- Prefix with `⬜` for pending items
- When an item is completed, change `⬜` → `✅` but **keep it in TODO.txt** — don't delete it.
  This maintains a complete record of what was done and what remains.
- If a completed item represents a design decision, also document it in `design.md`

### 3. Periodic Review (triggered at session start or when work pauses)
At natural breakpoints (session start, task completion, user asks "what's next"):
1. Quickly scan `TODO.txt` for items that could be addressed now
2. If a TODO item has become stale or irrelevant, remove it
3. If a TODO item's scope has changed, update it
4. Proactively remind the user of high-priority pending items

### 4. Build Verification
After any Rust code change in `apps/frontend/src-tauri/` or `crates/`:
- Run `cargo build -p remote-agent-host --target x86_64-unknown-linux-gnu --release`
- Copy binary: `cp target/x86_64-unknown-linux-gnu/release/agent apps/frontend/src-tauri/binaries/remote-agent-host-x86_64`
- Touch `apps/frontend/src-tauri/src/bootstrap/uploader.rs` to force recompile (Cargo doesn't track `include_bytes!` changes)
- Build Windows exe: `cargo xwin build --target x86_64-pc-windows-msvc --release -p remote-ai-ide`
- Copy to root: `cp target/x86_64-pc-windows-msvc/release/remote-ai-ide.exe .`

After any frontend change in `apps/frontend/src/`:
- Run `npx tsc --noEmit` then `pnpm build`
- Touch uploader.rs and rebuild Windows exe (frontend dist is embedded)

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
