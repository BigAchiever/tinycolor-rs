# Port Mortem / Track F — TinyColor (JavaScript, 1187 LOC) ported to Rust.
#
#     make verify     the one command that proves the submission
#     make docker     the same proof, in a container, no local toolchain
#     make help       everything else
#
# Local requirements: rust 1.96+, node 20+, npm, wasm-pack 0.15, python3 (demo
# only). `make docker` needs none of them; the image brings its own.

.DEFAULT_GOAL := help

CARGO  ?= cargo
NODE   ?= node
NPM    ?= npm
PYTHON ?= python3
DOCKER ?= docker
IMAGE  ?= tinycolor-port-mortem

# Overridable:  make fuzz SECONDS=120 SEED=7 TRANSPORT=native
SECONDS      ?= 60
LONG_SECONDS ?= 300
SEED         ?= 1
TRANSPORT    ?= wasm
REPS         ?= 5
PORT         ?= 8099

CLI_BIN  := target/release/tinycolor
WASM_DIR := tests/original/pkg
WASM_BIN := $(WASM_DIR)/tinycolor_wasm_bg.wasm

# Both artifacts are real file targets, so nothing is rebuilt unless a Rust
# source actually changed. `make verify` on a warm tree is seconds, not minutes.
RUST_SRC := Cargo.toml Cargo.lock $(wildcard crates/*/Cargo.toml) $(shell find crates -type f -name '*.rs')

.PHONY: all build wasm cli test verify hashes fuzz fuzz-long bench demo docker clean help

# ---------------------------------------------------------------------------
# build
# ---------------------------------------------------------------------------

all: build test ## build both artifacts, then run every test

build: cli wasm ## build both artifacts (native release binary + nodejs wasm)

cli: $(CLI_BIN) ## build the native binary -> target/release/tinycolor

wasm: $(WASM_BIN) ## build the wasm-bindgen module -> tests/original/pkg

# --locked: Cargo.lock is committed, so this compiles the exact dependency
# versions the recorded results were produced against, or it fails loudly.
$(CLI_BIN): $(RUST_SRC)
	$(CARGO) build --locked --release -p tinycolor-cli

$(WASM_BIN): $(RUST_SRC)
	@command -v wasm-pack >/dev/null 2>&1 || { \
	  echo "wasm-pack not found."; \
	  echo "  install:  cargo install wasm-pack --version 0.15.0 --locked"; \
	  echo "  or run:   make docker   (brings its own toolchain)"; \
	  exit 1; }
	wasm-pack build crates/wasm --target nodejs --out-dir ../../$(WASM_DIR) --out-name tinycolor_wasm

# One devDependency: @deno/shim-deno-test, the module the unmodified upstream
# test file imports. Real directory target — installed once, then left alone.
node_modules: package.json package-lock.json
	$(NPM) ci --no-audit --no-fund
	@touch node_modules

# ---------------------------------------------------------------------------
# proof
# ---------------------------------------------------------------------------

hashes: ## sha256-check the four unmodified upstream artifacts
	@echo "==> original artifacts, sha256 against tests/HASHES.txt"
	./scripts/verify-hashes.sh

test: build node_modules ## Rust unit tests + the unmodified upstream suite on BOTH transports
	@echo "==> rust unit tests"
	$(CARGO) test --locked -p tinycolor-core
	@echo "==> upstream suite, wasm transport"
	$(NODE) tests/run-all.mjs
	@echo "==> upstream suite, native binary over stdio JSON-RPC"
	TINYCOLOR_TRANSPORT=native $(NODE) tests/run-all.mjs

verify: hashes test ## THE ONE COMMAND: build, hashes, unit tests, upstream suite on both transports
	@echo ""
	@echo "verify: PASSED"
	@echo "  The originals are unmodified, the unit tests are green, and the"
	@echo "  byte-identical upstream suite passes through the wasm module AND"
	@echo "  the native binary. Same core, two transports, same result."
	@echo ""
	@echo "  next:  make fuzz       60s differential fuzz vs upstream on V8"
	@echo "         make bench      measured throughput and startup (it is not all good news)"
	@echo "         make demo       upstream's own demo page, running on the port"
	@echo "         DECISIONS.md    every behavioural divergence, with evidence"

fuzz: build node_modules ## 60s differential fuzz against upstream running on V8
	TINYCOLOR_TRANSPORT=$(TRANSPORT) $(NODE) fuzz/harness.mjs --seconds $(SECONDS) --seed $(SEED)

fuzz-long: build node_modules ## 300s differential fuzz (the bonus-qualifying run)
	TINYCOLOR_TRANSPORT=$(TRANSPORT) $(NODE) fuzz/harness.mjs --seconds $(LONG_SECONDS) --seed $(SEED)

bench: build node_modules ## measure throughput, startup and RSS -> bench/results.json
	@echo "note: bench/run.mjs enforces environment gates and stamps results"
	@echo "      unpublishable if the machine is throttled. See bench/METHODOLOGY.md."
	$(NODE) bench/run.mjs --reps $(REPS)

# ---------------------------------------------------------------------------
# see it / ship it
# ---------------------------------------------------------------------------

# web/tinycolor.js is committed with the wasm inlined as base64, so this needs
# no build step — index.html is upstream's own demo page, byte-identical.
demo-prep: ## warm every cache and dry-run every on-camera command before recording
	@./scripts/demo-prep.sh

demo: ## serve upstream's demo page, running on the Rust port, at :8099
	@echo "==> http://localhost:$(PORT)/   (ctrl-c to stop)"
	$(PYTHON) -m http.server $(PORT) --directory web

docker: ## build the verification image and run the whole evidence chain inside it
	$(DOCKER) build -t $(IMAGE) .
	$(DOCKER) run --rm $(IMAGE)

clean: ## remove build outputs (recorded evidence is kept)
	$(CARGO) clean
	rm -rf $(WASM_DIR) node_modules
	@echo "kept: fuzz/logs/, bench/results.json, web/tinycolor.js — recorded evidence, not build output."

help: ## show this help
	@echo ""
	@echo "  Port Mortem / Track F — TinyColor (JavaScript) -> Rust"
	@echo ""
	@echo "  make verify   <- the one command. Builds, then proves it."
	@echo "  make docker   <- the same, in a container, no local toolchain."
	@echo ""
	@grep -hE '^[a-z][a-z-]*:.*## ' $(MAKEFILE_LIST) \
	  | awk 'BEGIN {FS = ":.*## "}; {printf "    %-11s %s\n", $$1, $$2}' \
	  | sort
	@echo ""
	@echo "  vars: SECONDS=$(SECONDS) LONG_SECONDS=$(LONG_SECONDS) SEED=$(SEED) TRANSPORT=$(TRANSPORT)"
	@echo "        REPS=$(REPS) PORT=$(PORT) IMAGE=$(IMAGE)"
	@echo ""
