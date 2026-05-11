.PHONY: help build release test check fmt fmt-check clippy install uninstall clean doc run

CARGO ?= cargo
PACKAGE := pdf-encrypt
INSTALL_ARGS ?= --locked

help: ## Show this help message
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make <target>\n\nTargets:\n"} /^[a-zA-Z0-9_-]+:.*##/ {printf "  %-14s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## Build the debug binary
	$(CARGO) build --locked

release: ## Build the optimized release binary
	$(CARGO) build --release --locked

test: ## Run the test suite
	$(CARGO) test --locked

check: fmt-check clippy test ## Run formatting, linting, and tests

fmt: ## Format all Rust code
	$(CARGO) fmt --all

fmt-check: ## Check Rust formatting without changing files
	$(CARGO) fmt --all -- --check

clippy: ## Run clippy with warnings treated as errors
	$(CARGO) clippy --locked --all-targets -- -D warnings

install: ## Install pdf-encrypt from this checkout using cargo install
	$(CARGO) install --path . $(INSTALL_ARGS)

uninstall: ## Uninstall pdf-encrypt from Cargo's bin directory
	$(CARGO) uninstall $(PACKAGE)

clean: ## Remove Cargo build artifacts
	$(CARGO) clean

doc: ## Build and open local Rust documentation
	$(CARGO) doc --locked --no-deps --open

run: ## Run the CLI; pass arguments with ARGS="..."
	$(CARGO) run --locked -- $(ARGS)
