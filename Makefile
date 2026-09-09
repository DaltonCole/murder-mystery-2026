# -include (not include) so a missing `.env` is silently fine. Nothing reads
# it yet -- there's no deployment secret in this app today -- but it's ready
# for when the host console needs a password (see the implementation plan's
# "Networking, Realtime, and Authorization" section) without every command
# needing it retyped on the command line, matching the game-changer project's
# convention.
-include .env

PORT ?= 8080

# 0.0.0.0, not 127.0.0.1: this app is meant to be self-hosted on the host's
# laptop for the actual event, reachable by every player's phone over the
# venue WiFi/hotspot (see the plan's "Hosting" decision) -- binding to
# localhost only would make it unreachable from any other device.
ADDR ?= 0.0.0.0

# Best-effort LAN-IP auto-detection so `make run`'s output tells you the URL
# to actually give players, instead of them staring at 0.0.0.0. This is only
# a guess: on a multi-interface machine (VPN, Docker bridges, etc.) it can
# pick the wrong address -- verify it before a real event and override
# directly if it's wrong: `make run LAN_URL=http://<your-lan-ip>:$(PORT)`.
LAN_URL ?= http://$(shell hostname -I 2>/dev/null | awk '{print $$1}' || ipconfig getifaddr en0 2>/dev/null || echo localhost):$(PORT)

.DEFAULT_GOAL := help

.PHONY: help run build check sim test test-engine coverage coverage-open fmt clean

help:
	@echo "Targets:"
	@echo "  run           - serve the app (web+server), reachable from phones on this LAN"
	@echo "  build         - build the whole workspace (debug)"
	@echo "  check         - type-check every crate/feature combination (fast, no linking)"
	@echo "  sim           - run the headless game simulation binary"
	@echo "  test          - run all unit tests (cargo test --workspace)"
	@echo "  test-engine   - run just the engine crate's tests (fast inner loop)"
	@echo "  coverage      - print engine's line/region/function coverage summary"
	@echo "  coverage-open - generate engine's HTML coverage report and open it"
	@echo "  fmt           - format the whole workspace"
	@echo "  clean         - cargo clean (removes target/, several GB)"
	@echo ""
	@echo "Override with e.g. 'make run PORT=9000'"
	@echo "First build of each command is slow (full dependency compile); rebuilds are fast."
	@echo "Double-check the LAN URL before a real event: 'make run LAN_URL=http://<your-lan-ip>:8080'"

run:
	@echo "Serving on $(ADDR):$(PORT) -- give players this URL: $(LAN_URL)"
	dx serve -p app --addr $(ADDR) --port $(PORT)

build:
	cargo build --workspace

# Mirrors exactly the target/feature combinations the app actually ships as:
# engine/sim natively, app once for the browser (wasm32) and once for the
# server (native) -- each needs its own check since Cargo won't catch a
# feature-gated compile error in the other's default build.
check:
	cargo check -p engine -p sim
	cargo check -p app --no-default-features --features web --target wasm32-unknown-unknown
	cargo check -p app --no-default-features --features server

sim:
	cargo run -p sim

test:
	cargo test --workspace

test-engine:
	cargo test -p engine

# engine only, not --workspace: it's the crate the 100%-coverage goal
# actually applies to (see the plan's "Testing Strategy" section) -- app's
# Dioxus/websocket wiring is deliberately thin and isn't coverage-gated.
coverage:
	cargo llvm-cov -p engine --summary-only

coverage-open:
	cargo llvm-cov -p engine --html --open

fmt:
	cargo fmt --all

clean:
	cargo clean
