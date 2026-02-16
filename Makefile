.PHONY: build test optimize clean schema deploy interact check format

# Default target
all: check test build

# Build the contract
build:
	@echo "Building contract..."
	cargo wasm

# Run tests
test:
	@echo "Running tests..."
	cargo test

# Run tests with output
test-verbose:
	@echo "Running tests with output..."
	cargo test -- --nocapture

# Optimize the contract
optimize:
	@echo "Optimizing contract with Docker..."
	@if ! command -v docker > /dev/null; then \
		echo "Error: Docker not found. Please install Docker first."; \
		exit 1; \
	fi
	docker run --rm -v "$$(pwd)":/code \
		--mount type=volume,source="$$(basename $$(pwd))_cache",target=/code/target \
		--mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
		cosmwasm/optimizer:0.16.0

# Generate schema
schema:
	@echo "Generating schema..."
	cargo run --example schema

# Clean build artifacts
clean:
	@echo "Cleaning..."
	cargo clean
	rm -rf artifacts/

# Full clean (including Docker volumes)
clean-all: clean
	@echo "Cleaning Docker volumes..."
	docker volume rm $$(basename $$(pwd))_cache 2>/dev/null || true
	docker volume rm registry_cache 2>/dev/null || true

# Run clippy (linting)
check:
	@echo "Running clippy..."
	cargo clippy --all-targets -- -D warnings

# Format code
format:
	@echo "Formatting code..."
	cargo fmt

# Check formatting
format-check:
	@echo "Checking formatting..."
	cargo fmt -- --check

# Deploy to ZigChain testnet
deploy:
	@echo "Deploying to ZigChain..."
	@if [ ! -f "deploy.sh" ]; then \
		echo "Error: deploy.sh not found"; \
		exit 1; \
	fi
	chmod +x deploy.sh
	./deploy.sh

# Interact with deployed contract
interact:
	@echo "Starting interaction script..."
	@if [ ! -f "interact.sh" ]; then \
		echo "Error: interact.sh not found"; \
		exit 1; \
	fi
	@if [ ! -f "contract_addresses.txt" ]; then \
		echo "Error: contract_addresses.txt not found. Run 'make deploy' first."; \
		exit 1; \
	fi
	chmod +x interact.sh
	./interact.sh

# Quick build and optimize
release: clean build optimize

# Development workflow
dev: clean check test build

# Full CI workflow
ci: format-check check test build optimize

# Show help
help:
	@echo "Available targets:"
	@echo "  make build         - Build the contract"
	@echo "  make test          - Run tests"
	@echo "  make test-verbose  - Run tests with output"
	@echo "  make optimize      - Optimize contract with Docker"
	@echo "  make schema        - Generate JSON schema"
	@echo "  make clean         - Clean build artifacts"
	@echo "  make clean-all     - Clean everything including Docker"
	@echo "  make check         - Run clippy linting"
	@echo "  make format        - Format code"
	@echo "  make format-check  - Check code formatting"
	@echo "  make deploy        - Deploy to ZigChain testnet"
	@echo "  make interact      - Interact with deployed contract"
	@echo "  make release       - Clean, build, and optimize"
	@echo "  make dev           - Development workflow (check + test + build)"
	@echo "  make ci            - Full CI workflow"
	@echo "  make help          - Show this help message"
