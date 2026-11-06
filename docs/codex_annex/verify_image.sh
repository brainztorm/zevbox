#!/usr/bin/env bash
# Checks a Codex image with the same options as `zevbox launch`: no account, no key.
# Usage: bash docs/codex_annex/verify_image.sh <image>
# Prints one PASS or FAIL per check, then "verify_image: ALL OK" or exits 1.
set -uo pipefail
image="${1:?usage: verify_image.sh <image>}"
T="$(mktemp -d /tmp/zevbox-verify.XXXXXX)"
T="$(cd -P "$T" && pwd)"
trap 'rm -rf -- "$T"' EXIT
mkdir -p "$T/proj" "$T/state"
chmod 700 "$T/state"
cat > "$T/in-vm.sh" <<'VM'
set -u
PROJ="$1"
BIN=/opt/zevbox/codex/node_modules/.bin
cd "$PROJ" || exit 3
check() { if [ "$2" = "$3" ]; then echo "PASS $1"; else echo "FAIL $1: expected '$3', got '$2'"; fi; }
contains() { case "$2" in *"$3"*) echo "PASS $1";; *) echo "FAIL $1: '$3' not in: $(printf '%s' "$2" | tr '\n' ' ' | cut -c1-240)";; esac; }
check "Linux VM" "$(uname -s)" Linux
check "NO_BROWSER set" "${NO_BROWSER:-unset}" 1
check "no system bwrap" "$(command -v bwrap || echo none)" none
[ -x "$BIN/codex-acp" ] && echo "PASS adapter present" || echo "FAIL adapter missing"
[ -x "$BIN/codex" ] && echo "PASS engine present" || echo "FAIL engine missing"
echo "INFO adapter: $("$BIN/codex-acp" --version 2>&1 | tail -1)"
echo "INFO engine: $("$BIN/codex" --version 2>&1 | tail -1)"
echo "INFO node: $(node --version)"
printf 'console.log("from-node")\n' > t.js
printf '#include <stdio.h>\nint main(void){puts("from-c");return 0;}\n' > t.c
git init -q . 2>/dev/null
ww='{"type":"workspaceWrite","writableRoots":[],"networkAccess":false,"excludeTmpdirEnvVar":false,"excludeSlashTmp":false}'
ro='{"type":"readOnly","networkAccess":false}'
limits='touch in.txt && echo wrote-project; touch /opt/x 2>/dev/null && echo wrote-opt || echo blocked-opt; touch /home/agent/x 2>/dev/null && echo wrote-home || echo blocked-home; curl -s -m 5 -o /dev/null https://example.com && echo network-open || echo network-blocked'
readonly_probe='touch ro.txt 2>/dev/null && echo wrote-project || echo blocked-project'
tools='node t.js; python3 -c "print(\"from-python\")"; git status --short | wc -l | tr -d " "; npm --version >/dev/null && echo npm-ok; rg --version >/dev/null && echo rg-ok; gcc t.c -o t.out && ./t.out; python3 -m venv .v && echo venv-ok'
j() { python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$1"; }
call() { printf '{"id":%s,"method":"command/exec","params":{"command":["/bin/bash","-c",%s],"cwd":"%s","timeoutMs":25000,"sandboxPolicy":%s}}\n' "$1" "$(j "$2")" "$PROJ" "$3"; }
( echo '{"id":1,"method":"initialize","params":{"clientInfo":{"name":"verify","title":null,"version":"0"}}}'; sleep 2
  echo '{"method":"initialized"}'; sleep 1
  call 2 "$limits" "$ww"; sleep 8
  call 3 "$readonly_probe" "$ro"; sleep 4
  call 4 "$tools" "$ww"; sleep 14
) | /usr/bin/timeout 45 "$BIN/codex" app-server 2>/dev/null > /tmp/out.jsonl
out() { python3 -c '
import json,sys
for l in open("/tmp/out.jsonl"):
    m=json.loads(l)
    if m.get("id")==int(sys.argv[1]):
        r=m.get("result") or {}
        print(r.get("stdout","")+r.get("stderr","")+str(m.get("error") or ""))
' "$1"; }
r2="$(out 2)"; r3="$(out 3)"; r4="$(out 4)"
contains "sandbox writes in the project" "$r2" wrote-project
contains "sandbox blocks /opt" "$r2" blocked-opt
contains "sandbox blocks /home/agent" "$r2" blocked-home
contains "sandbox blocks the network" "$r2" network-blocked
contains "read-only mode blocks the project" "$r3" blocked-project
for tool in from-node from-python npm-ok rg-ok from-c venv-ok; do contains "tool works in the sandbox: $tool" "$r4" "$tool"; done
init='{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false,"auth":{"terminal":false}},"clientInfo":{"name":"zed","version":"verify"}}}'
acp="$( (echo "$init"; sleep 4) | /usr/bin/timeout 10 "$BIN/codex-acp" 2>/dev/null | head -1)"
contains "adapter answers ACP version 1" "$acp" '"protocolVersion":1'
contains "adapter offers api-key" "$acp" '"id":"api-key"'
VM
container run -i --rm --init --progress none --name "codex-verify-$$" --cpus 4 --memory 4G \
  --uid "$(id -u)" --gid "$(id -g)" --cap-drop ALL -e HOME=/home/agent \
  --mount "type=bind,source=$T/proj,target=$T/proj" \
  --mount "type=bind,source=$T/state,target=/home/agent" \
  -w "$T/proj" --tmpfs "$T/proj/.zed" "$image" bash -s -- "$T/proj" < "$T/in-vm.sh" > "$T/report" 2>&1
grep -E '^(PASS|FAIL|INFO) ' "$T/report" || { echo "FAIL no report:"; head -20 "$T/report"; exit 1; }
if grep -q '^FAIL ' "$T/report"; then echo "verify_image: FAILED"; exit 1; fi
[ "$(grep -c '^PASS ' "$T/report")" -ge 18 ] || { echo "verify_image: FAILED (missing checks)"; exit 1; }
echo "verify_image: ALL OK"
