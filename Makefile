# rustup from Homebrew installs cargo here; Zed-like minimal PATHs miss it.
export PATH := /opt/homebrew/opt/rustup/bin:$(HOME)/.cargo/bin:$(PATH)
# GNU Make 3.81 (macOS) execs simple recipe lines with its *original* PATH, so
# the export above only reaches cargo's children. Name cargo by absolute path.
CARGO := $(firstword $(wildcard /opt/homebrew/opt/rustup/bin/cargo $(HOME)/.cargo/bin/cargo) cargo)

.PHONY: check install test-integration verify protect-update

## Everything an agent must run before saying "done" (§6.3).
check:
	$(CARGO) fmt --check
	$(CARGO) clippy --all-targets -- -D warnings
	$(CARGO) test
	./scripts/check-invariants.sh

## Gate of a plan step: make verify STEP=4
verify:
	./scripts/verify-step.sh $(STEP)

## Real Apple Container tests (§10 step 10).
test-integration:
	$(CARGO) test --test integration_test -- --ignored --test-threads=1

## Installs the binary Zed launches (never point Zed at target/).
install: check
	$(CARGO) build --release
	mkdir -p $(HOME)/.local/bin
	cp target/release/zevbox $(HOME)/.local/bin/zevbox.tmp
	mv -f $(HOME)/.local/bin/zevbox.tmp $(HOME)/.local/bin/zevbox

## HUMAN ONLY: re-seal protected files after an approved change.
protect-update:
	find tests scripts clippy.toml -type f ! -name PROTECTED.sha256 ! -name '.DS_Store' | LC_ALL=C sort | xargs shasum -a 256 > tests/PROTECTED.sha256
