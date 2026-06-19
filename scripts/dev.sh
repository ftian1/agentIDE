#!/bin/bash
# Start the development environment.
# Backend: Tauri dev server (hot-reloads Rust + frontend)
# Frontend: Vite dev server runs automatically via Tauri's beforeDevCommand

set -euo pipefail
cd "$(dirname "$0")/.."

echo "Starting Tauri development server..."
cd apps/frontend
pnpm tauri dev
