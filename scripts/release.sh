#!/usr/bin/env bash
# Build artifacts and prepare the dist/ directory for OTA release.
#
# Usage:
#   ./scripts/release.sh                     # build everything
#   ./scripts/release.sh --frontend-only     # only Vite build (TS/React changes)
#   ./scripts/release.sh --agent-only        # only agent binaries (crate changes)
#   ./scripts/release.sh --tauri-only        # only Tauri shell + loader
#   ./scripts/release.sh --dry-run           # show what would be built
#
# Output (dist/):
#   manifest.json  frontend.tar.gz  agent-*  main.exe  loader-*  pricing.json

set -euo pipefail
cd "$(dirname "$0")/.."

# ── Parse flags ─────────────────────────────────────────────────────
FRONTEND=true; AGENT=true; TAURI=true; LOADER=true; PRICING=true
DRY_RUN=false

for arg in "$@"; do
  case "$arg" in
    --frontend-only) AGENT=false; TAURI=false; LOADER=false; PRICING=false ;;
    --agent-only)   FRONTEND=false; TAURI=false; LOADER=false; PRICING=false ;;
    --tauri-only)   FRONTEND=false; AGENT=false; PRICING=false ;;
    --dry-run)      DRY_RUN=true ;;
    *)              echo "Unknown flag: $arg"; exit 1 ;;
  esac
done

DIST_DIR="dist"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

VERSION="$(date -u +%Y-%m-%d).$(git rev-parse --short=7 HEAD 2>/dev/null || echo '0000000')"
echo ""
echo "╔══════════════════════════════════════════════════╗"
echo "║  Release: $VERSION"
echo "║  Frontend: $FRONTEND | Agent: $AGENT | Tauri: $TAURI | Loader: $LOADER"
echo "╚══════════════════════════════════════════════════╝"
echo ""

# ── 1. Frontend (Vite + SWC) ────────────────────────────────────────
if $FRONTEND; then
  echo "─── [1/5] Frontend ───"
  if $DRY_RUN; then
    echo "  [dry-run] would run: npx vite build"
  else
    START=$(date +%s)
    (cd apps/frontend && npx vite build --outDir ../../dist-www 2>&1) | tail -3
    tar -czf "$DIST_DIR/frontend.tar.gz" -C dist-www .
    rm -rf dist-www
    ELAPSED=$(( $(date +%s) - START ))
    echo "  frontend.tar.gz  $(du -h $DIST_DIR/frontend.tar.gz | cut -f1)  (${ELAPSED}s)"
  fi
fi

# ── 2. Agent binaries ───────────────────────────────────────────────
if $AGENT; then
  echo "─── [2/5] Agent binaries ───"
  if $DRY_RUN; then
    echo "  [dry-run] cargo build -p remote-agent-host"
  else
    START=$(date +%s)
    cargo build -p remote-agent-host --target x86_64-unknown-linux-gnu --release 2>&1 | grep -E "Finished|error" || true
    cp target/x86_64-unknown-linux-gnu/release/agent "$DIST_DIR/agent-linux-x86_64"

    cargo xwin build -p remote-agent-host --target x86_64-pc-windows-msvc --release 2>&1 | grep -E "Finished|error" || true
    cp target/x86_64-pc-windows-msvc/release/agent.exe "$DIST_DIR/agent-windows-x86_64"

    ELAPSED=$(( $(date +%s) - START ))
    echo "  agent-linux-x86_64    $(du -h $DIST_DIR/agent-linux-x86_64 | cut -f1)"
    echo "  agent-windows-x86_64  $(du -h $DIST_DIR/agent-windows-x86_64 | cut -f1)"
    echo "  (${ELAPSED}s)"
  fi
fi

# ── 3. Tauri shell ──────────────────────────────────────────────────
if $TAURI; then
  echo "─── [3/5] Tauri shell ───"
  if $DRY_RUN; then
    echo "  [dry-run] cargo xwin build -p remote-ai-ide"
  else
    START=$(date +%s)
    cargo xwin build --target x86_64-pc-windows-msvc --release -p remote-ai-ide 2>&1 | grep -E "Finished|error" || true
    cp target/x86_64-pc-windows-msvc/release/remote-ai-ide.exe "$DIST_DIR/main.exe"
    ELAPSED=$(( $(date +%s) - START ))
    echo "  main.exe  $(du -h $DIST_DIR/main.exe | cut -f1)  (${ELAPSED}s)"
  fi
fi

# ── 4. Loader binaries ──────────────────────────────────────────────
if $LOADER; then
  echo "─── [4/5] Loader ───"
  if $DRY_RUN; then
    echo "  [dry-run] cargo build/xwin -p ota-loader"
  else
    START=$(date +%s)
    cargo build -p ota-loader --target x86_64-unknown-linux-gnu --release 2>&1 | grep -E "Finished|error" || true
    cp target/x86_64-unknown-linux-gnu/release/loader "$DIST_DIR/loader-linux-x86_64"

    cargo xwin build -p ota-loader --target x86_64-pc-windows-msvc --release 2>&1 | grep -E "Finished|error" || true
    cp target/x86_64-pc-windows-msvc/release/loader.exe "$DIST_DIR/loader-windows-x86_64.exe"

    ELAPSED=$(( $(date +%s) - START ))
    echo "  loader-linux-x86_64      $(du -h $DIST_DIR/loader-linux-x86_64 | cut -f1)"
    echo "  loader-windows-x86_64.exe $(du -h $DIST_DIR/loader-windows-x86_64.exe | cut -f1)"
    echo "  (${ELAPSED}s)"
  fi
fi

# ── 5. Pricing + Manifest ───────────────────────────────────────────
if $PRICING; then
  echo "─── [5/5] Pricing + Manifest ───"
  cp pricing.json "$DIST_DIR/pricing.json"

  MANIFEST="$DIST_DIR/manifest.json"
  echo "{" > "$MANIFEST"
  echo "  \"version\": \"$VERSION\"," >> "$MANIFEST"
  echo "  \"files\": {" >> "$MANIFEST"

  FIRST=true
  for f in $(ls "$DIST_DIR" | grep -v manifest.json | sort); do
    path="$DIST_DIR/$f"
    sha=$(sha256sum "$path" | awk '{print $1}')
    size=$(stat -c%s "$path" 2>/dev/null || stat -f%z "$path" 2>/dev/null)
    if ! $FIRST; then echo "    ," >> "$MANIFEST"; fi
    FIRST=false
    printf '    "%s": {"sha256": "%s", "size": %d}' "$f" "$sha" "$size" >> "$MANIFEST"
  done
  echo "" >> "$MANIFEST"
  echo "  }" >> "$MANIFEST"
  echo "}" >> "$MANIFEST"

  echo "  manifest.json  $(wc -c < $MANIFEST) bytes"
fi

# ── Summary ──────────────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════════╗"
echo "║  dist/ ready ($VERSION)"
echo "╚══════════════════════════════════════════════════╝"
ls -lhS "$DIST_DIR/"
echo ""

# Show sccache stats if available.
if command -v sccache &>/dev/null; then
  sccache --show-stats 2>/dev/null | grep -E "cache.hit|compile|write" | head -6
fi

echo "Next:  git add dist/ && git commit -m 'release: $VERSION' && git push"
