#!/usr/bin/env bash
set -euo pipefail

# Installs the larder daily-ingest launchd job on macOS.
# Schedule: daily at 03:00.
# Logs:     ~/Library/Logs/larder/{ingest.log,ingest.err}

PLIST_NAME="com.kotachisam.larder"
PLIST_DEST="$HOME/Library/LaunchAgents/$PLIST_NAME.plist"
LOG_DIR="$HOME/Library/Logs/larder"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
TEMPLATE="$SCRIPT_DIR/launchd/$PLIST_NAME.plist.template"

if [ "$(uname)" != "Darwin" ]; then
    echo "Error: this script only supports macOS launchd." >&2
    exit 1
fi

if [ ! -f "$TEMPLATE" ]; then
    echo "Error: template not found at $TEMPLATE" >&2
    exit 1
fi

if command -v larder >/dev/null 2>&1; then
    LARDER_BIN=$(command -v larder)
elif [ -x "$HOME/.cargo/bin/larder" ]; then
    LARDER_BIN="$HOME/.cargo/bin/larder"
else
    echo "Error: larder binary not found in PATH or ~/.cargo/bin/" >&2
    echo "Run: cargo install --path . (from the larder repo)" >&2
    exit 1
fi

echo "→ Using larder binary: $LARDER_BIN"

mkdir -p "$LOG_DIR"
mkdir -p "$(dirname "$PLIST_DEST")"

sed -e "s|__LARDER_BIN__|$LARDER_BIN|g" \
    -e "s|__USER_HOME__|$HOME|g" \
    "$TEMPLATE" > "$PLIST_DEST"

echo "→ Wrote $PLIST_DEST"

UID_NUM=$(id -u)
DOMAIN="gui/$UID_NUM"

if launchctl print "$DOMAIN/$PLIST_NAME" >/dev/null 2>&1; then
    launchctl bootout "$DOMAIN/$PLIST_NAME" 2>/dev/null || true
    echo "→ Unloaded existing instance"
fi

launchctl bootstrap "$DOMAIN" "$PLIST_DEST"
echo "→ Loaded launchd job: $PLIST_NAME"

echo
echo "Schedule: daily at 03:00"
echo "Logs:     $LOG_DIR/ingest.log"
echo "          $LOG_DIR/ingest.err"
echo
echo "Run now:    launchctl kickstart $DOMAIN/$PLIST_NAME"
echo "Status:     launchctl print $DOMAIN/$PLIST_NAME | head -30"
echo "Tail logs:  tail -f $LOG_DIR/ingest.log"
echo "Uninstall:  launchctl bootout $DOMAIN/$PLIST_NAME && rm $PLIST_DEST"
