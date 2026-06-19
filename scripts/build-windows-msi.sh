#!/bin/bash
# ============================================================
# Build Windows MSI from Linux (cross-compilation via cargo-xwin)
# ============================================================
# Prerequisites:
#   rustup target add x86_64-pc-windows-msvc
#   cargo install cargo-xwin
#   sudo apt install -y msitools wixl nasm lld clang
#
# For SSH feature support, also fix the code in:
#   apps/frontend/src-tauri/src/connection/ssh.rs
#   apps/frontend/src-tauri/src/bootstrap/uploader.rs
#   apps/frontend/src-tauri/src/transport/ssh_channel.rs
# ============================================================

set -euo pipefail
cd "$(dirname "$0")"

RELEASE_DIR="target/x86_64-pc-windows-msvc/release"
MSI_OUTPUT="remote-ai-ide-0.1.0.msi"

echo "=== Step 1: Install frontend dependencies ==="
pnpm install

echo "=== Step 2: Build frontend ==="
cd apps/frontend && pnpm build && cd ../..

echo "=== Step 3: Build Rust backend for Windows ==="
# Ensure LLVM tools are available
export PATH="$HOME/.cargo/bin:$HOME/.local/llvm-tools:$PATH"

# Create LLVM tool symlinks if needed (for version-suffixed tools)
mkdir -p "$HOME/.local/llvm-tools"
for tool in clang-cl lld-link llvm-lib llvm-ar llvm-rc llvm-cvtres; do
    ver_tool=$(ls /usr/bin/${tool}-[0-9]* 2>/dev/null | head -1)
    if [ -n "$ver_tool" ] && [ ! -L "$HOME/.local/llvm-tools/$tool" ]; then
        ln -sf "$ver_tool" "$HOME/.local/llvm-tools/$tool"
    fi
done

cargo xwin build \
    --target x86_64-pc-windows-msvc \
    --release \
    -p remote-ai-ide

echo "=== Step 4: Package MSI ==="
mkdir -p packaging/windows/msi/binaries
mkdir -p packaging/windows/msi/icons

cp "$RELEASE_DIR/remote-ai-ide.exe" packaging/windows/msi/binaries/
cp "$RELEASE_DIR/remote_ai_ide_lib.dll" packaging/windows/msi/binaries/
cp apps/frontend/src-tauri/icons/icon.ico packaging/windows/msi/icons/

cd packaging/windows/msi
rm -f "$MSI_OUTPUT"
wixl -v -o "$MSI_OUTPUT" -a x64 remote-ai-ide.wxs
cd ../../..

cp "packaging/windows/msi/$MSI_OUTPUT" .

echo "=== Done ==="
ls -lh "$MSI_OUTPUT"
echo ""
echo "Install on Windows: msiexec /i $MSI_OUTPUT"
echo "Uninstall: msiexec /x $MSI_OUTPUT"
echo "Or via: Control Panel → Programs and Features → Remote AI IDE → Uninstall"
