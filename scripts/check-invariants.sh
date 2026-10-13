#!/usr/bin/env bash
# Static checks of the security invariants that the compiler cannot see (§6.4).
# PROTECTED FILE. Exit 1 and print every offending line if a rule is broken.
set -uo pipefail
cd "$(dirname "$0")/.."

fail=0
# rule <message> <grep -E pattern> [allowed file regex]
rule() {
  local msg="$1" pattern="$2" allow="${3:-^$}"
  local hits
  hits="$(grep -rnE "$pattern" src assets 2>/dev/null | grep -vE "$allow" || true)"
  if [ -n "$hits" ]; then
    echo "INVARIANT BROKEN: $msg"
    echo "$hits" | sed 's/^/    /'
    fail=1
  fi
}

# I7: no host shell.
rule "I7: no shell on the host (Command::new(\"sh\"/\"bash\"/\"zsh\"))" \
  'Command::new\("(/bin/|/usr/bin/)?(ba|z)?sh"\)'
# I8: --all / -a only for the read-only `container list --all`.
hits="$(grep -rnE '"(--all|-a)"' src | grep -v '"list"' || true)"
if [ -n "$hits" ]; then echo "INVARIANT BROKEN: I8: --all/-a outside 'list'"; echo "$hits" | sed 's/^/    /'; fail=1; fi
# I6: stdout only from ui.rs (humans) and supervisor.rs (ACP).
rule "I6: stdout used outside src/ui.rs and src/supervisor.rs" \
  '(std::io::stdout|tokio::io::stdout|io::stdout\(\))' '^src/(ui|supervisor)\.rs:'
# §8.2: TTY flags only built in container.rs.
rule "§8.2: TTY flags (-t/-it/--tty) outside src/container.rs" \
  '"(-t|-it|--tty)"' '^src/container\.rs:'
# §8.2: forbidden container options (long forms; short forms are covered by
# the exact-argument test `launch_args_exact`, and "-p" is legit for `ps`).
rule "§8.2: forbidden container option" \
  '"(--ssh|--publish|--publish-socket|--virtualization|--privileged|--volume|--detach|--rosetta)"'
# F6: never inject an API key (subscription billing).
rule "F6: ANTHROPIC_API_KEY must never appear" 'ANTHROPIC_API_KEY'
# I1: never mount personal directories.
rule "I1: personal directory used as a mount source" \
  'source=[^"]*(\.ssh|\.gemini|Library/Keychains|/\.claude[,"])'
# I11 / I6: lints must not be silenced in production code.
rule "I11: lint silenced in src (unwrap/expect/panic/print)" \
  'allow\((clippy::)?(unwrap_used|expect_used|panic|print_stdout|print_stderr|todo|unimplemented|dbg_macro)'
# Required lints still present in Cargo.toml.
for lint in 'unwrap_used = "deny"' 'expect_used = "deny"' 'panic = "deny"' 'print_stdout = "deny"' 'print_stderr = "deny"' 'unsafe_code = "forbid"'; do
  if ! grep -qF "$lint" Cargo.toml; then echo "INVARIANT BROKEN: Cargo.toml lost lint: $lint"; fail=1; fi
done

if [ $fail -eq 0 ]; then echo "invariants: OK"; fi
exit $fail
