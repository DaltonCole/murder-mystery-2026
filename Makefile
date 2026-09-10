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

# How many bot players `make bots` adds -- override with `make bots BOTS=20`.
BOTS ?= 10

# Where `make bots` connects -- defaults to the same machine's `make run`
# (the normal case: run the server in one terminal, fill it with bots from
# another on the same laptop). Override for a bot script running
# elsewhere on the LAN: `make bots BOTS_URL=ws://<lan-ip>:8080/api/ws`.
BOTS_URL ?= ws://127.0.0.1:$(PORT)/api/ws

.DEFAULT_GOAL := help

.PHONY: help run build check sim bots test test-engine coverage coverage-open fmt clean

help:
	@echo "Targets:"
	@echo "  run           - serve the app (web+server), reachable from phones on this LAN"
	@echo "  build         - build the whole workspace (debug)"
	@echo "  check         - type-check every crate/feature combination (fast, no linking)"
	@echo "  sim           - run the headless game simulation binary"
	@echo "  bots          - join BOTS bot players onto an already-running server (see 'run')"
	@echo "                  so you can drive the real game live from your own /host tab"
	@echo "  test          - run all unit + integration tests, including a real bot-driven"
	@echo "                  full game over the actual websocket protocol at up to 30 players"
	@echo "  test-engine   - run just the engine crate's tests (fast inner loop)"
	@echo "  coverage      - print engine's line/region/function coverage summary"
	@echo "  coverage-open - generate engine's HTML coverage report and open it"
	@echo "  fmt           - format the whole workspace"
	@echo "  clean         - cargo clean (removes target/, several GB)"
	@echo ""
	@echo "Override with e.g. 'make run PORT=9000' or 'make bots BOTS=25'"
	@echo "First build of each command is slow (full dependency compile); rebuilds are fast."
	@echo "Double-check the LAN URL before a real event: 'make run LAN_URL=http://<your-lan-ip>:8080'"

run:
	@echo "Serving on $(ADDR):$(PORT) -- give players this URL: $(LAN_URL)"
	dx serve -p app --addr $(ADDR) --port $(PORT)

build:
	cargo build --workspace

# Mirrors exactly the target/feature combinations the app actually ships as:
# engine/sim/bots natively, app once for the browser (wasm32) and once for
# the server (native) -- each needs its own check since Cargo won't catch a
# feature-gated compile error in the other's default build.
check:
	cargo check -p engine -p sim -p bots
	cargo check -p app --no-default-features --features web --target wasm32-unknown-unknown
	cargo check -p app --no-default-features --features server --tests

sim:
	cargo run -p sim

# Joins BOTS bot players onto a server you started separately (`make run`
# or `dx serve`) -- see `crates/bots/src/main.rs`'s module doc comment for
# why this doesn't drive the game itself: the point is to exercise your
# own real /host console live, with bots filling in as players.
bots:
	cargo run -p bots -- --bots $(BOTS) --url $(BOTS_URL)

# `app`'s own tests need the real `server` feature build to serve
# `/api/ws` at all when spawned as a child process by
# `tests/full_game_with_bots.rs` (the default `web` feature compiles that
# whole code path away) -- everything else runs under the normal workspace
# sweep. See that test file's doc comment for what it actually does: spawns
# the real compiled server and drives it with up to 30 bots over the real
# websocket protocol.
test:
	cargo test --workspace --exclude app
	cargo test -p app --no-default-features --features server

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
