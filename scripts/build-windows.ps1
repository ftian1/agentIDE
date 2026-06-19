# ============================================================
# Build Windows MSI on Windows (using native WiX Toolset & Tauri)
# ============================================================
# Prerequisites:
#   1. Install Rust: https://rustup.rs/
#   2. Install Node.js + pnpm: winget install pnpm
#   3. Install VS 2022 Build Tools (C++ desktop workload)
#   4. Install WiX Toolset v3: https://wixtoolset.org/releases/
#   5. Install Tauri CLI: cargo install tauri-cli
# ============================================================

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot\..

Write-Host "=== Step 1: Install frontend dependencies ==="
pnpm install

Write-Host "=== Step 2: Build & package with Tauri (generates MSI) ==="
cd apps\frontend

# Build for Windows with SSH feature
# Tauri will automatically:
#   1. Build the frontend (vite + tsc)
#   2. Build the Rust backend for Windows
#   3. Bundle into MSI (via WiX) + NSIS installer
pnpm tauri build --features ssh --bundles msi,nsis

cd ..\..

Write-Host "=== Done ==="
Write-Host "Installer locations:"
Get-ChildItem -Recurse -Filter "*.msi" apps\frontend\src-tauri\target\release\bundle\
Get-ChildItem -Recurse -Filter "*.exe" apps\frontend\src-tauri\target\release\bundle\
Write-Host ""
Write-Host "MSI Install: msiexec /i Remote-AI-IDE.msi"
Write-Host "MSI Uninstall (silent): msiexec /x Remote-AI-IDE.msi /quiet"
Write-Host "Or via: Settings → Apps → Remote AI IDE → Uninstall"
