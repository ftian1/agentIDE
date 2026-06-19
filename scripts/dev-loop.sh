#!/bin/bash
# ============================================================
# dev-loop.sh — automated build → deploy → test → log cycle
#
# Usage: bash scripts/dev-loop.sh <windows-host> <windows-user>
#
# Requires:
#   - Windows OpenSSH Server running on the target
#   - SSH key configured (or you'll be prompted for password)
#   - cargo-xwin, pnpm, clang, lld, nasm (see build-windows-msi.sh)
# ============================================================

set -euo pipefail

WIN_HOST="${1:-}"
WIN_USER="${2:-ftian}"
SSH_DEST="${WIN_USER}@${WIN_HOST}"
REMOTE_DIR="C:/Users/${WIN_USER}/remote-ai-ide"

if [ -z "$WIN_HOST" ]; then
    echo "Usage: bash scripts/dev-loop.sh <windows-host> [windows-user]"
    echo "Example: bash scripts/dev-loop.sh 192.168.1.100 ftian"
    exit 1
fi

export PATH="$HOME/.cargo/bin:$HOME/.local/llvm-tools:$PATH"

echo "╔══════════════════════════════════════════╗"
echo "║  Remote AI IDE — Dev Loop               ║"
echo "╚══════════════════════════════════════════╝"
echo "  Target: ${SSH_DEST}"
echo "  Remote: ${REMOTE_DIR}"
echo ""

# ── Step 1: Build ──────────────────────────
echo "━━━ Step 1: Build ━━━"
cd "$(dirname "$0")/.."

cd apps/frontend && pnpm build && cd ../..

cargo xwin build \
    --target x86_64-pc-windows-msvc \
    --release \
    -p remote-ai-ide \
    --features ssh

BIN="target/x86_64-pc-windows-msvc/release/remote-ai-ide.exe"
echo "  Binary: $(ls -lh "$BIN" | awk '{print $5}')"

# ── Step 2: Kill remote instance ────────────
echo "━━━ Step 2: Stop remote instance ━━━"
ssh "${SSH_DEST}" "taskkill /f /im remote-ai-ide.exe 2>nul; exit 0" || true

# ── Step 3: Deploy ─────────────────────────
echo "━━━ Step 3: Deploy to Windows ━━━"
ssh "${SSH_DEST}" "mkdir -p ${REMOTE_DIR}"
scp "$BIN" "${SSH_DEST}:${REMOTE_DIR}/"
echo "  Uploaded ✓"

# ── Step 4: Run & capture output ───────────
echo "━━━ Step 4: Run on Windows ━━━"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="windows-run-${TIMESTAMP}.log"

# Launch in background via ssh, capture stdout/stderr
ssh "${SSH_DEST}" \
    "cd ${REMOTE_DIR} && remote-ai-ide.exe > %TEMP%/remote-ai-ide-stdout.log 2>&1 & echo PID=$!" \
    > /dev/null

sleep 3

# ── Step 5: Fetch logs ─────────────────────
echo "━━━ Step 5: Collect logs ━━━"

# App log (via temp_dir())
ssh "${SSH_DEST}" "type %TEMP%/remote-ai-ide.log 2>nul || echo '(no app log)'" \
    > "${LOG_FILE}"

# stdout/stderr
ssh "${SSH_DEST}" "type %TEMP%/remote-ai-ide-stdout.log 2>nul || echo '(no stdout log)'" \
    >> "${LOG_FILE}"

# Crash log (from our panic hook)
ssh "${SSH_DEST}" "type ${REMOTE_DIR}/remote-ai-ide-crash.log 2>nul || echo '(no crash)'" \
    >> "${LOG_FILE}"

# Check if still running
RUNNING=$(ssh "${SSH_DEST}" "tasklist /fi \"imagename eq remote-ai-ide.exe\" 2>nul | findstr remote-ai-ide" || echo "")

echo ""
echo "╔══════════════════════════════════════════╗"
if [ -n "$RUNNING" ]; then
    echo "║  ✅ App RUNNING on Windows              ║"
else
    echo "║  ❌ App CRASHED — see log below         ║"
fi
echo "╚══════════════════════════════════════════╝"
echo ""
echo "━━━ Log (${LOG_FILE}) ━━━"
cat "${LOG_FILE}"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Log saved to: ${LOG_FILE}"
echo ""
echo "Run again: bash scripts/dev-loop.sh ${WIN_HOST} ${WIN_USER}"
