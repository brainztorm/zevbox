#!/usr/bin/env bash
# Gate of §10: `scripts/verify-step.sh <N>` must print "STEP N: OK" before
# step N is marked done in docs/PROGRESS.md. Checks are cumulative.
# PROTECTED FILE.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

step="${1:-}"
case "$step" in ''|*[!0-9]*) echo "usage: scripts/verify-step.sh <step number 0-17>"; exit 2 ;; esac

say() { printf '\n== %s\n' "$*"; }
die() { printf '\nSTEP %s: FAILED — %s\n' "$step" "$*"; exit 1; }

say "toolchain"
cargo --version >/dev/null || die "cargo not found (step 0)"

say "protected files unchanged (tests, fixtures, gates)"
shasum -a 256 -c tests/PROTECTED.sha256 --quiet || die "a protected file was modified. Restore it with 'git checkout -- <file>'. If you believe a protected test is wrong, stop and ask the human (👤) with evidence."

say "steps before $step are done in docs/PROGRESS.md"
for ((i = 0; i < step; i++)); do
  row="$(grep -E "^\| *$i *\|" docs/PROGRESS.md || true)"
  [ -n "$row" ] || die "docs/PROGRESS.md has no row for step $i"
  case "$row" in
    *✅*) ;;
    *⛔*) [ "$i" -eq 14 ] || die "only step 14 (Antigravity) may be ⛔ blocked; step $i is not done" ;;
    *) die "step $i is not marked ✅ in docs/PROGRESS.md (do the steps in order)" ;;
  esac
done

if [ "$step" -ge 2 ]; then
  say "format";     cargo fmt --check || die "run 'cargo fmt'"
  say "clippy";     cargo clippy --all-targets -- -D warnings || die "clippy warnings"
  say "invariants"; scripts/check-invariants.sh || die "security invariant broken"
  say "build";      cargo build || die "build failed"
  ./target/debug/zevbox --help >/dev/null || die "'zevbox --help' failed"
fi

run_test() {
  say "cargo test --test $1"
  local out
  out="$(cargo test --test "$1" 2>&1)" || { echo "$out" | tail -40; die "tests in tests/$1.rs fail"; }
  echo "$out" | grep -E '^test result' | tail -1
  if echo "$out" | grep -qE 'test result: ok\. 0 passed'; then die "tests/$1.rs ran 0 tests"; fi
}

[ "$step" -ge 3 ] && run_test config_test
[ "$step" -ge 4 ] && run_test relay_policy_test
[ "$step" -ge 5 ] && run_test relay_transport_test
[ "$step" -ge 6 ] && run_test container_test
[ "$step" -ge 7 ] && run_test release_test
[ "$step" -ge 8 ] && run_test zed_settings_test
if [ "$step" -ge 9 ]; then
  run_test lifecycle_test
  run_test cli_test
  say "full cargo test"
  out="$(cargo test 2>&1)" || { echo "$out" | tail -40; die "cargo test fails"; }
  echo "$out" | grep -cE '^test result: ok' | sed 's/^/test binaries ok: /'
fi
if [ "$step" -ge 10 ]; then
  say "real Apple Container tests (make test-integration)"
  cargo test --test integration_test -- --ignored --test-threads=1 || die "integration tests fail"
fi
if [ "$step" -ge 17 ]; then
  say "final: nothing left unimplemented"
  if grep -rnE 'not implemented yet|NotImplemented' src; then die "stubs remain in src/"; fi
  [ -s README.md ] || die "README.md missing"
  [ -s AGENTS.md ] || die "AGENTS.md missing"
fi
if [ "$step" -ge 11 ] && [ "$step" -le 16 ]; then
  echo
  echo "NOTE: step $step also requires the manual evidence listed in zevbox_plan.md §10 (write it in docs/PROGRESS.md)."
fi

printf '\nSTEP %s: OK\n' "$step"
