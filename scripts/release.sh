#!/usr/bin/env bash
# Build artifacts and prepare the dist/ directory for OTA release.
#
# Usage:
#   ./scripts/release.sh                     # build everything
#   ./scripts/release.sh --frontend-only     # only Vite build (TS/React changes)
#   ./scripts/release.sh --agent-only        # only agent binaries (crate changes)
#   ./scripts/release.sh --tauri-only        # only Tauri shell
#   ./scripts/release.sh --dry-run           # show what would be built
#
# Output (dist/):
#   loader.exe (= the IDE, includes embedded defaults)
#   manifest.json  frontend.tar.gz  agent-*  pricing.json

set -euo pipefail
cd "$(dirname "$0")/.."

# ── Parse flags ─────────────────────────────────────────────────────
FRONTEND=true; AGENT=true; TAURI=true; PRICING=true
DRY_RUN=false

for arg in "$@"; do
  case "$arg" in
    --frontend-only) AGENT=false; TAURI=false ;;
    --agent-only)   FRONTEND=false; TAURI=false ;;
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
echo "║  Frontend: $FRONTEND | Agent: $AGENT | Tauri: $TAURI"
echo "╚══════════════════════════════════════════════════╝"
echo ""

# ── 1. Frontend (Vite + SWC) ────────────────────────────────────────
if $FRONTEND; then
  echo "─── [1/4] Frontend ───"
  if $DRY_RUN; then
    echo "  [dry-run] would run: npx vite build"
  else
    START=$(date +%s)
    (cd apps/frontend && npx vite build --outDir dist 2>&1) | tail -3
    tar -czf "$DIST_DIR/frontend.tar.gz" -C apps/frontend/dist .
    ELAPSED=$(( $(date +%s) - START ))
    echo "  frontend.tar.gz  $(du -h $DIST_DIR/frontend.tar.gz | cut -f1)  (${ELAPSED}s)"
  fi
fi

# ── 2. Agent binaries ───────────────────────────────────────────────
if $AGENT; then
  echo "─── [2/4] Agent binaries ───"
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

# ── 3. Pricing + Manifest ───────────────────────────────────────────
# MUST run before the tauri build — build.rs embeds dist/manifest.json
# into loader.exe so startup can compare embedded-vs-cached versions.
# loader.exe itself is NOT in the manifest; only cache-updatable files.
if $PRICING; then
  echo "─── [3/4] Pricing + Manifest ───"
  cp pricing.json "$DIST_DIR/pricing.json"

  MANIFEST="$DIST_DIR/manifest.json"
  echo "{" > "$MANIFEST"
  echo "  \"version\": \"$VERSION\"," >> "$MANIFEST"
  echo "  \"files\": {" >> "$MANIFEST"

  FIRST=true
  for f in $(ls "$DIST_DIR" | grep -v -E 'manifest.json|loader.exe' | sort); do
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
else
  # ── No pricing step → generate version-only manifest for build.rs ──
  # (e.g. --tauri-only: we still need a manifest for embedding)
  MANIFEST="$DIST_DIR/manifest.json"
  cat > "$MANIFEST" <<JSON
{
  "version": "$VERSION",
  "files": {}
}
JSON
  echo "─── [3/4] Manifest (version-only, no pricing) ───"
  echo "  manifest.json  $(wc -c < $MANIFEST) bytes"
fi

# ── 4. Tauri shell (= loader.exe) ────────────────────────────────────
# This is the single entry-point exe. It embeds frontend + agent binaries
# as defaults, prefers cache/ on startup, and runs the OTA background updater.
if $TAURI; then
  echo "─── [4/4] App binary ───"
  if $DRY_RUN; then
    echo "  [dry-run] cargo xwin build -p remote-ai-ide"
  else
    START=$(date +%s)
    cargo xwin build --target x86_64-pc-windows-msvc --release -p remote-ai-ide 2>&1 | grep -E "Finished|error" || true
    cp target/x86_64-pc-windows-msvc/release/remote-ai-ide.exe "$DIST_DIR/loader.exe"
    ELAPSED=$(( $(date +%s) - START ))
    echo "  loader.exe  $(du -h $DIST_DIR/loader.exe | cut -f1)  (${ELAPSED}s)"
  fi
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
