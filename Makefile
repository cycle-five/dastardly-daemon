.PHONY: build test lint format coverage coverage-html coverage-open clean help

help: ## Show this help message
	@echo "Available targets:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build the project
	cargo build

test: ## Run all tests
	cargo test

lint: ## Run clippy linter
	cargo clippy -- -D clippy::all -D warnings -W clippy::pedantic

format: ## Format code with rustfmt
	cargo fmt

coverage: ## Generate coverage report in terminal
	cargo llvm-cov --all-features --workspace

coverage-html: ## Generate HTML coverage report
	cargo llvm-cov --all-features --workspace --html
	@echo "Coverage report generated in target/llvm-cov/html/index.html"

coverage-open: coverage-html ## Generate and open HTML coverage report
	@if command -v xdg-open > /dev/null; then \
		xdg-open target/llvm-cov/html/index.html; \
	elif command -v open > /dev/null; then \
		open target/llvm-cov/html/index.html; \
	else \
		echo "Please open target/llvm-cov/html/index.html manually"; \
	fi

coverage-lcov: ## Generate coverage report in LCOV format
	cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
	@echo "LCOV report generated: lcov.info"

clean: ## Clean build artifacts and coverage data
	cargo clean
	rm -rf target/llvm-cov
	rm -f lcov.info