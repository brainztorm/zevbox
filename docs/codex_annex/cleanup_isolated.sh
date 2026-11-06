#!/usr/bin/env bash
# Deletes the images built in a throw-away installation, then its folder.
# Usage: bash docs/codex_annex/cleanup_isolated.sh "$W"
set -uo pipefail
W="${1:?usage: cleanup_isolated.sh <folder printed by isolated_env.sh>}"
case "$W" in
  /private/tmp/zevbox-iso.*) ;;
  *) echo "refused: $W is not a folder made by isolated_env.sh" >&2; exit 2 ;;
esac
for lock in "$W"/state/releases/*/lock.json; do
  [ -f "$lock" ] || continue
  image="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["image"])' "$lock")"
  case "$image" in
    zevbox/codex:*) container image delete "$image" >/dev/null 2>&1 && echo "deleted $image" ;;
    *) echo "kept $image" ;;
  esac
done
rm -rf -- "$W"
echo "removed $W"
