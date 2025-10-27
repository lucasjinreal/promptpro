#!/usr/bin/env bash
set -e

# =========================================
# 🚀 PromptPro build & install script
# =========================================

# 🛠️ Build in release mode (local target dir)
echo "🚀 Building project in release mode..."
cargo build --release --target-dir ./target

# 📦 Extract binary name from Cargo.toml
BIN_NAME=$(grep -E '^name\s*=' Cargo.toml | head -n1 | cut -d '"' -f2)
INSTALL_DIR="$HOME/.cargo/bin"
SRC_PATH="target/release/$BIN_NAME"

# 🧩 Verify binary existence
if [ ! -f "$SRC_PATH" ]; then
    echo "❌ Error: binary not found at $SRC_PATH"
    exit 1
fi

# 📁 Ensure ~/.cargo/bin exists
mkdir -p "$INSTALL_DIR"

# 📥 Copy binary
echo "📦 Installing $BIN_NAME → $INSTALL_DIR ..."
cp "$SRC_PATH" "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/$BIN_NAME"

# ⚡ Create alias (pp)
ALIAS_PATH="$INSTALL_DIR/ppro"

if [ -f "$ALIAS_PATH" ]; then
    rm -f "$ALIAS_PATH"
fi

ln -sf "$INSTALL_DIR/$BIN_NAME" "$ALIAS_PATH"

# ✅ Final message
echo "✅ Installed:"
echo "   - Binary: $INSTALL_DIR/$BIN_NAME"
echo "   - Alias : $ALIAS_PATH → $BIN_NAME"

# 🧭 PATH reminder
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo "⚠️  Reminder: Add this to your shell rc file:"
    echo "   export PATH=\"\$HOME/.cargo/bin:\$PATH\""
fi

# 🧪 Test installation
echo
echo "🔍 Verifying installation: pp -h"
"$ALIAS_PATH" -h || echo "⚠️  Run failed; check if binary supports -h"

echo "🎉 Done!"
