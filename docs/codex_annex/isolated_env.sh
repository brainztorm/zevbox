#!/usr/bin/env bash
# Creates a throw-away Zevbox installation under /tmp and prints its folder.
# Usage: W=$(bash docs/codex_annex/isolated_env.sh target/debug/zevbox)
# Then run commands in a subshell, never in your own shell (HOME changes):
#   ( . "$W/env.sh"; "$W/zevbox-bin" init )
set -euo pipefail
binary="${1:?usage: isolated_env.sh <zevbox binary>}"
W="$(mktemp -d /tmp/zevbox-iso.XXXXXX)"
W="$(cd -P "$W" && pwd)"
mkdir -p "$W/home/code/proj" "$W/home/.local/bin" "$W/zed"
printf '{}\n' > "$W/zed/settings.json"   # `update` only syncs an existing Zed file
cp "$binary" "$W/zevbox-bin"
cat > "$W/env.sh" <<EOF
export HOME="$W/home"
export ZEVBOX_STATE_ROOT="$W/state"
export ZEVBOX_ZED_SETTINGS="$W/zed/settings.json"
export ZEVBOX_INSTALLED_BINARY="$W/home/.local/bin/zevbox"
EOF
echo "$W"
