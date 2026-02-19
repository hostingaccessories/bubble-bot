.PHONY: help build release check test fmt lint clippy clean doc

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

build: ## Build in debug mode
	cargo build

release: ## Build in release mode
	cargo build --release

check: ## Type-check without building
	cargo check

test: ## Run tests
	cargo test

fmt: ## Format code
	cargo fmt

fmt-check: ## Check formatting
	cargo fmt --check

lint: clippy fmt-check ## Run all lints

clippy: ## Run clippy
	cargo clippy -- -D warnings

clean: ## Remove build artifacts
	cargo clean

doc: ## Generate documentation
	cargo doc --no-deps --open
