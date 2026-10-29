# AGENTS.md — guide for AI agents working on zevbox

zevbox runs ACP agents (Claude, Antigravity, and oMLX = the Claude adapter on
a local model server) for the Zed editor inside Apple Container micro-VMs, with a filtering ACP relay so the agent can never use Zed
to read, write or execute anything on the Mac. Single Rust binary, macOS arm64.

## Absolute rules (the human's orders; no exception, no « later »)

1. **Documentation is always up to date with the change.** Every change that
   alters behaviour, a command, an option, a message, an invariant, a path or
   a limit updates **in the same commit** every document that mentions it:
   `zevbox_plan_V2.md` (the contract), `README.md`, `docs/USER_GUIDE.md`,
   `docs/ARCHITECTURE.md`, their French twins in `docs/fr/`, this file, and
   `docs/DECISIONS.md` (decision + date). Before committing, search the docs
   for the old wording (`grep -rn "<old command|message|number>" README.md docs
   zevbox_plan_V2.md AGENTS.md`) and fix every hit, including diagrams, sample
   outputs, counts (« 13 invariants », « 15 commands ») and error texts. A
   change with stale docs is not finished.
2. **Protected files** (below): never edited without the human's explicit
   « 👤 » for that change; show the `git diff` first.

## Before anything

1. Read `zevbox_plan_V2.md` entirely (it is the contract; `zevbox_plan_V1.md` is history only), then `docs/PROGRESS.md`
   and `docs/DECISIONS.md`. Resume at the first step not marked ✅.
   **All 17 steps are ✅ (accepted 2026-10-13)** and every later review item is
   done. What remains is the section « Backlog » of `docs/DECISIONS.md`; follow
   the « Reste à faire » common rules there (one topic per commit, `make check`
   before each commit, docs updated in the same commit).
2. Rust lives in `/opt/homebrew/opt/rustup/bin` (the Makefile adds it to PATH;
   in a shell: `export PATH=/opt/homebrew/opt/rustup/bin:$PATH`).

## Style reference

Files whose header says `WORKED EXAMPLE` (`src/fsutil.rs`, `src/config.rs`,
`src/tools.rs`, `src/relay/jsonrpc.rs`, `src/relay/policy.rs`, `src/container.rs`,
`src/zed_settings.rs`) are finished and were written as models: copy their error handling, comments
and structure in the other modules.

## Workflow (one plan step at a time)

- Implement the step, then run `make verify STEP=<n>` until it prints
  `STEP <n>: OK`. Then mark the step ✅ in `docs/PROGRESS.md` with the evidence,
  and `git commit -m "step <n>: <title>"`.
- During work, `make check` = fmt + clippy (-D warnings) + tests + invariants.
- A test failure means the code is wrong, not the test.

## Protected files (never edit)

`tests/**` (tests, fixtures, fake `container`, fake agent), `scripts/**`,
`clippy.toml`, listed with their SHA-256 in `tests/PROTECTED.sha256` and
checked by every gate. If you are convinced a protected test is wrong: stop,
write the evidence in `docs/PROGRESS.md`, and ask the human (👤). You may add
new tests in `src/` (`#[cfg(test)]`) or new files under `tests/` that are not
in the manifest.

## Module map

- `cli/` clap definitions + one file per command (thin) · `ui.rs` only human stdout/stderr
- `paths.rs` every location (env overrides for tests) · `config.rs` config.json + project zone rules (I2)
- `agents.rs` static agent table (`opt_in`: oMLX) · `release.rs` registry, locks, build, promote, rollback
- `omlx.rs` oMLX settings (server, key, models, unload hook) · `cli/auth_omlx.rs` `zevbox auth omlx`
- `container.rs` the only module calling the `container` CLI · `instances.rs` instance records
- `supervisor.rs` `launch`: preflight, start, relay, bounded shutdown · `relay/` ACP relay
  (`policy.rs` = pure security decisions, `jsonrpc.rs` helpers, `mod.rs` I/O loop)
- `zed_settings.rs` JSONC edits of Zed `agent_servers` · `tools.rs` extra apt packages · `fsutil.rs`
- `guard.rs` project watcher during `launch` (I13)

## Invariants (details: plan §3) — never weaken them to pass a test

I1 only two mounts (project, state dir) · I2 canonical project in an allowed zone
(home minus ~/Library and ~/.*, or /Volumes/<disk>/…), no project list ·
I3 no fs/terminal/elicitation/terminal-auth through Zed · I4 allow-list both
ways, responses routed by pending id · I5 `cwd` == project root, no extra dirs,
`mcpServers: []` · I6 stdout = JSON-RPC only in `launch` · I7 no host shell ·
I8 stop/delete one verified `zevbox-*` + label container, never `--all`; « unregistered » = only containers whose `/home/agent` mount is under this state root ·
I9 no message content/tokens in logs (`launch` journals to `logs/<id>.log` via tracing: names, paths, ids, sizes only) · I10 no network/build in `launch` ·
I11 no unwrap/expect/panic in `src/` · I12 zevbox files never mounted ·
I13 `.zed` masked, existing `.git` read-only in the ACP container, project guard (started before the container) stops the agent on a new `.git`, `.zed` must not be a file or link (`guard.rs`).

## Common changes

- **Allow a new ACP method**: audit what it makes Zed do on the Mac; if safe,
  add it in `relay/policy.rs`, add a test (new test file or `#[cfg(test)]`),
  and add a row to the tables of plan §7.3.
- **Add an agent**: entry in `agents.rs`, stage in `assets/Containerfile` (or
  reuse one through `build_target`, like oMLX), registry parser in `release.rs`,
  tests, plan §2/§8.7. `tests/zed_settings_test.rs` counts the agents (👤).

## Never

Use `--all`/`-a` with `container stop|delete`; pass `-t` to the ACP container;
mount `$HOME`, `~/.claude`, `~/.gemini`, `~/.ssh`; set `ANTHROPIC_API_KEY`
(oMLX uses `ANTHROPIC_AUTH_TOKEN` in `state/omlx` only); put a key in argv or `-e`;
write to the user's project from zevbox; point Zed at `target/`.
