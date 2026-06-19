#!/usr/bin/env bash
# Driver script for remote-ai-ide Tauri app.
# Launches under xvfb, provides screenshot and interaction commands.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
SKILL_DIR="$ROOT/.claude/skills/run-agentide"
LOG_DIR="$SKILL_DIR/logs"
mkdir -p "$LOG_DIR"

# ── commands ──────────────────────────────────────────────

cmd_launch() {
  local DISPLAY_NUM="${1:-99}"
  echo "[driver] Starting Xvfb on :$DISPLAY_NUM ..."
  Xvfb ":$DISPLAY_NUM" -screen 0 1400x900x24 +extension RENDER -ac &
  sleep 1

  echo "[driver] Launching Tauri app ..."
  export DISPLAY=:$DISPLAY_NUM
  export GDK_BACKEND=x11
  export NO_AT_BRIDGE=1
  export GTK_IM_MODULE=

  "$ROOT/target/debug/remote-ai-ide" > "$LOG_DIR/tauri_stdout.log" 2> "$LOG_DIR/tauri_stderr.log" &
  TAURI_PID=$!
  echo "[driver] Tauri PID: $TAURI_PID"
  sleep 3

  if kill -0 $TAURI_PID 2>/dev/null; then
    echo "[driver] Tauri app launched successfully"
  else
    echo "[driver] ERROR: Tauri app exited immediately"
    cat "$LOG_DIR/tauri_stderr.log"
    return 1
  fi
}

cmd_ss() {
  local NAME="${1:-screenshot}"
  local DISPLAY_NUM="${2:-99}"
  local OUT="$SKILL_DIR/${NAME}.png"
  DISPLAY=:$DISPLAY_NUM import -display :$DISPLAY_NUM -window root "$OUT" 2>/dev/null
  echo "[driver] Screenshot saved: $OUT ($(du -h "$OUT" | cut -f1))"
}

cmd_list_windows() {
  DISPLAY=:99 xdotool search --onlyvisible --name "." 2>/dev/null || echo "[driver] xdotool not available"
}

cmd_quit() {
  pkill -f remote-ai-ide 2>/dev/null || true
  echo "[driver] Tauri app stopped"
}

cmd_agent_smoke() {
  echo "[driver] Running agent host smoke test ..."
  local LOG="$LOG_DIR/agent_smoke.log"
  timeout 3 "$ROOT/target/debug/agent" --mode stdio < /dev/null 2> "$LOG" || true
  if grep -q "Remote Agent Host starting" "$LOG" && grep -q "Sent Hello" "$LOG"; then
    echo "[driver] Agent smoke test: PASS"
  else
    echo "[driver] Agent smoke test: FAIL"
    cat "$LOG"
    return 1
  fi
}

cmd_test() {
  echo "[driver] Running full test suite ..."
  cd "$ROOT"
  source ~/.cargo/env 2>/dev/null
  cargo test --workspace 2>&1 | tail -20
}

# ── dispatch ──────────────────────────────────────────────

case "${1:-}" in
  launch)     cmd_launch "${2:-99}" ;;
  ss)         cmd_ss "${2:-screenshot}" "${3:-99}" ;;
  list-wins)  cmd_list_windows ;;
  quit)       cmd_quit ;;
  smoke)      cmd_agent_smoke ;;
  test)       cmd_test ;;
  *)
    echo "Usage: driver.sh {launch|ss|quit|smoke|test|list-wins}"
    echo ""
    echo "  launch [display]   Start Xvfb + Tauri app"
    echo "  ss [name] [disp]   Take screenshot"
    echo "  quit              Stop Tauri app"
    echo "  smoke             Smoke-test the agent host binary"
    echo "  test              Run full test suite"
    exit 1
    ;;
esac
