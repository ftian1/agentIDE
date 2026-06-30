#!/usr/bin/env bash
# Copy monaco-editor's pre-built AMD bundles from node_modules into
# public/vendor/monaco/ so Vite serves them as static assets.
#
# monaco-editor ships minified AMD bundles in node_modules/monaco-editor/min/vs/
# that are loaded at runtime by @monaco-editor/react's AMD loader.
# No bundling step needed — just copy.
#
# Run once when monaco-editor is added/upgraded, or whenever language support
# changes.  Commit public/vendor/monaco/ to the repo.
#
# After this, vite build skips monaco-editor entirely (externalized).

set -euo pipefail
cd "$(dirname "$0")/.."

SRC="node_modules/monaco-editor/min/vs"
OUT="apps/frontend/public/vendor/monaco"

echo "=== Copying pre-built Monaco AMD from $SRC → $OUT ==="
rm -rf "$OUT"
mkdir -p "$OUT"
cp -r "$SRC"/* "$OUT"/

echo ""
echo "=== Monaco vendor size ==="
du -sh "$OUT"
echo ""
echo "Done.  Commit public/vendor/monaco/ if changed."
echo "The main vite build will skip monaco-editor from now on."
