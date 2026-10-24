#!/usr/bin/env bash
# Install LidCommit as a launchd daemon on macOS
set -euo pipefail

INSTALL_DIR="$HOME/.local/bin"
PLIST_DIR="$HOME/Library/LaunchAgents"
SERVICE_LABEL="com.lidcommit.daemon"
PLIST_FILE="$PLIST_DIR/$SERVICE_LABEL.plist"

echo "🔒 LidCommit macOS Installer"
echo "============================="

# 1. Check for Rust
if ! command -v cargo &>/dev/null; then
  echo "Installing Rust..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
fi

# 2. Build
PROJECT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
echo "Building LidCommit (release)..."
cd "$PROJECT_DIR"
cargo build --release
echo "Build complete!"

# 3. Install binary
mkdir -p "$INSTALL_DIR"
cp "$PROJECT_DIR/target/release/lid-commit" "$INSTALL_DIR/lid-commit"
chmod +x "$INSTALL_DIR/lid-commit"
echo "Binary installed to $INSTALL_DIR/lid-commit"

# 4. Create config
CONFIG_DIR="$HOME/.config/lid-commit"
CONFIG_FILE="$CONFIG_DIR/config.toml"
if [[ ! -f "$CONFIG_FILE" ]]; then
  mkdir -p "$CONFIG_DIR"
  cp "$PROJECT_DIR/config.example.toml" "$CONFIG_FILE"
  echo "Config created at $CONFIG_FILE"
fi

# 5. Create launchd plist
mkdir -p "$PLIST_DIR"
LOG_DIR="$HOME/.local/share/lid-commit"
mkdir -p "$LOG_DIR"

cat > "$PLIST_FILE" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$SERVICE_LABEL</string>

  <key>ProgramArguments</key>
  <array>
    <string>$INSTALL_DIR/lid-commit</string>
  </array>

  <key>EnvironmentVariables</key>
  <dict>
    <key>GEMINI_API_KEY</key>
    <string>${GEMINI_API_KEY:-}</string>
    <key>LIDCOMMIT_LOG</key>
    <string>lid_commit=info</string>
  </dict>

  <!-- Run at login -->
  <key>RunAtLoad</key>
  <true/>

  <!-- Restart if it crashes -->
  <key>KeepAlive</key>
  <true/>

  <key>StandardOutPath</key>
  <string>$LOG_DIR/lid-commit.log</string>
  <key>StandardErrorPath</key>
  <string>$LOG_DIR/lid-commit-error.log</string>
</dict>
</plist>
EOF

echo "launchd plist created at $PLIST_FILE"

# 6. Load the daemon
launchctl load "$PLIST_FILE"
echo "LidCommit daemon loaded!"

echo ""
echo "✅ LidCommit installed successfully!"
echo "   Binary: $INSTALL_DIR/lid-commit"
echo "   Config: $CONFIG_FILE"
echo "   Logs:   $LOG_DIR/"
echo ""
echo "⚠️  Don't forget to set your GEMINI_API_KEY:"
echo "   export GEMINI_API_KEY=your_key"
echo "   Then reload: launchctl unload $PLIST_FILE && launchctl load $PLIST_FILE"
