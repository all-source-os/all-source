.PHONY: help install dev build clean clean-rust demo test lint check-versions \
        core control web mcp \
        docker-build docker-test docker-test-quick docker-clean docker-purge \
        docker-core docker-web docker-query docker-mcp docker-control \
        ci quality-gates quality-rust quality-go quality-elixir quality-elixir-full quality-e2e \
        validate-workflows validate-workflows-quick \
        elixir-test elixir-test-failed elixir-test-watch elixir-test-report \
        release release-quick release-preflight version images-check \
        perf-bench publish-sdks

# Test output directory for failure reports
TEST_OUTPUT_DIR := .test-reports

# =============================================================================
# General Commands
# =============================================================================

help:
	@echo "AllSource - Available Commands"
	@echo "=============================="
	@echo ""
	@echo "Development:"
	@echo "  make install        - Install all dependencies"
	@echo "  make dev            - Run all services in development mode"
	@echo "  make build          - Build all services"
	@echo "  make clean          - Clean all build artifacts"
	@echo "  make clean-rust     - Clean every Rust target/ across the monorepo"
	@echo "  make demo           - Quick demo setup (install + dev)"
	@echo "  make test           - Run tests"
	@echo "  make lint           - Run linters across all services"
	@echo "  make check-versions - Check version consistency across services"
	@echo ""
	@echo "Quality Gates (CI pipeline locally):"
	@echo "  make ci             - Full CI pipeline (matches GitHub Actions exactly)"
	@echo "  make quality-gates  - Run ALL quality checks (Rust + Go + Elixir)"
	@echo "  make quality-rust   - Run Rust quality gates only"
	@echo "  make quality-go     - Run Go quality gates only"
	@echo "  make quality-elixir - Run Elixir quality gates only (quick)"
	@echo "  make quality-elixir-full - Full Elixir checks with Dialyzer"
	@echo ""
	@echo "Elixir Testing:"
	@echo "  make elixir-test         - Run all Elixir tests with detailed output"
	@echo "  make elixir-test-failed  - Re-run only failed tests"
	@echo "  make elixir-test-watch   - Run tests in watch mode"
	@echo "  make elixir-test-report  - Generate a markdown test report"
	@echo ""
	@echo "Workflow Validation:"
	@echo "  make validate-workflows       - Validate GitHub Actions with act"
	@echo "  make validate-workflows-quick - Quick syntax check (actionlint only)"
	@echo ""
	@echo "Individual Services:"
	@echo "  make core         - Run Rust event store only"
	@echo "  make control      - Run Go control plane only"
	@echo "  make web          - Run Next.js web UI only"
	@echo "  make mcp          - Run MCP server only"
	@echo ""
	@echo "Container Testing:"
	@echo "  make docker-test       - Full container test suite (all services)"
	@echo "  make docker-test-quick - Quick build-only test"
	@echo "  make docker-build      - Build all containers"
	@echo "  make docker-clean      - Remove test containers and images"
	@echo "  make docker-purge      - Remove ALL docker resources (nuclear option)"
	@echo ""
	@echo "Individual Container Builds:"
	@echo "  make docker-core       - Build core container"
	@echo "  make docker-web        - Build web container"
	@echo "  make docker-query      - Build query-service container"
	@echo "  make docker-mcp        - Build mcp-server container"
	@echo "  make docker-control    - Build control-plane container"
	@echo ""
	@echo "Release:"
	@echo "  make release           - Interactive release workflow (full)"
	@echo "  make release-quick     - Quick release (skip quality gates)"
	@echo "  make release-preflight - Run pre-flight checks only"
	@echo "  make version           - Show current version"
	@echo "  make images-check      - Check Docker image versions in GHCR"
	@echo ""
	@echo "Version Management:"
	@echo "  make bump-version      - Interactive version bump"
	@echo "  make set-version VERSION=X.Y.Z - Set version across all apps (not SDKs)"
	@echo "  make set-sdk-version SDK=<lang> VERSION=X.Y.Z - Set ONE SDK version"
	@echo "  make check-versions    - Check version consistency"
	@echo "  make publish-sdks      - Dry-run SDK publishing (validate before CI)"
	@echo ""
	@echo "Performance:"
	@echo "  make perf-bench        - Run performance benchmarks (release mode)"

# =============================================================================
# Development Commands
# =============================================================================

install:
	@echo "📦 Installing dependencies..."
	bun install
	cd apps/control-plane && go mod download

dev:
	@echo "🚀 Starting all services..."
	@echo "Press Ctrl+C to stop all services"
	bun dev

build:
	@echo "🔨 Building all services..."
	bun build

clean: clean-rust
	@echo "🧹 Cleaning build artifacts..."
	bun clean
	-cd apps/control-plane && rm -rf bin
	-rm -rf .container-test-logs

clean-rust:
	@echo "🦀 Cleaning Rust target/ across all workspaces..."
	@find . -type d -name target -not -path '*/node_modules/*' -prune | while read dir; do \
		ws="$$(dirname $$dir)"; \
		echo "  → cargo clean in $$ws"; \
		(cd "$$ws" && cargo clean) 2>/dev/null || true; \
		rm -rf "$$dir"; \
	done

demo: install
	@echo "🎪 Starting AllSource demo..."
	@echo "Dashboard will be available at http://localhost:3000"
	@echo ""
	bun dev

test:
	@echo "🧪 Running tests..."
	bun test

perf-bench:
	@echo "Running performance benchmarks (release mode)..."
	cargo run --release -p allsource-performance

lint:
	@echo "🔍 Running linters..."
	bun run lint
	cd apps/core && cargo fmt --check && cargo clippy --all-targets -- -D warnings
	cd apps/control-plane && go fmt ./... && go vet ./...

check-versions:
	@echo "🔢 Checking version consistency..."
	./scripts/check-versions.sh

# =============================================================================
# Quality Gates (mirrors CI pipeline)
# =============================================================================

quality-gates: check-versions quality-rust quality-go quality-elixir
	@echo ""
	@echo "✅ All quality gates passed!"

# Full CI pipeline - replicates exact GitHub Actions checks
ci: check-versions quality-rust quality-go quality-elixir-full
	@echo ""
	@echo "=============================================="
	@echo "✅ Full CI pipeline passed!"
	@echo "   Safe to push - this matches GitHub Actions"
	@echo "=============================================="

quality-rust:
	@echo ""
	@echo "🦀 Running Rust quality gates..."
	@echo "================================"
	@echo "→ Checking formatting..."
	cd apps/core && cargo +nightly fmt --check
	@echo "→ Checking Cargo.toml sorting..."
	cargo +nightly sort --workspace --check
	@echo "→ Running Clippy..."
	cd apps/core && cargo +nightly clippy --locked --all-targets --all-features -- -D warnings
	@echo "→ Running tests (enterprise — all features)..."
	cd apps/core && cargo +nightly test --locked --lib --all-features
	@echo "→ Running tests (community edition)..."
	cd apps/core && cargo +nightly test --locked --lib --features community
	@echo "→ Building release..."
	cd apps/core && cargo +nightly build --locked --lib --release
	@echo "→ Checking documentation..."
	cd apps/core && RUSTDOCFLAGS="-D warnings" cargo +nightly doc --no-deps --document-private-items
	@echo "✅ Rust quality gates passed!"

quality-go:
	@echo ""
	@echo "🐹 Running Go quality gates..."
	@echo "=============================="
	@echo "→ Downloading dependencies..."
	cd apps/control-plane && go mod download
	@echo "→ Verifying dependencies..."
	cd apps/control-plane && go mod verify
	@echo "→ Checking formatting..."
	@cd apps/control-plane && if [ -n "$$(gofmt -l .)" ]; then \
		echo "Go code is not formatted:"; \
		gofmt -d .; \
		exit 1; \
	fi
	@echo "→ Running golangci-lint (includes staticcheck, gosec, etc.)..."
	@if command -v golangci-lint >/dev/null 2>&1; then \
		cd apps/control-plane && golangci-lint run --timeout=5m; \
	else \
		echo "⚠ golangci-lint not installed, falling back to go vet"; \
		echo "  Install with: brew install golangci-lint"; \
		cd apps/control-plane && go vet ./...; \
	fi
	@echo "→ Running tests..."
	cd apps/control-plane && go test -v -race -covermode=atomic ./...
	@echo "→ Building binary..."
	cd apps/control-plane && CGO_ENABLED=0 go build -ldflags="-s -w" -o control-plane .
	@echo "✅ Go quality gates passed!"

quality-elixir:
	@echo ""
	@echo "💧 Running Elixir quality gates..."
	@echo "=================================="
	@echo "→ Installing dependencies..."
	cd apps/query-service && mix deps.get
	@echo "→ Checking formatting..."
	cd apps/query-service && mix format --check-formatted
	@echo "→ Compiling with warnings as errors..."
	cd apps/query-service && mix compile --warnings-as-errors
	@echo "→ Running Credo..."
	-cd apps/query-service && mix credo --strict
	@echo "→ Running tests (testcontainers will start PostgreSQL automatically)..."
	cd apps/query-service && TESTCONTAINERS_RYUK_DISABLED=true mix test
	@echo "✅ Elixir quality gates passed!"

# Full Elixir CI checks (includes Dialyzer and MCP server) - matches GitHub Actions exactly
quality-elixir-full:
	@echo ""
	@echo "💧 Running full Elixir CI pipeline..."
	@echo "====================================="
	@mkdir -p $(TEST_OUTPUT_DIR)
	@echo ""
	@echo "--- Query Service ---"
	@echo "→ Installing dependencies..."
	cd apps/query-service && mix deps.get
	@echo "→ Checking formatting..."
	cd apps/query-service && mix format --check-formatted
	@echo "→ Checking unused dependencies..."
	cd apps/query-service && mix deps.unlock --check-unused
	@echo "→ Compiling with warnings as errors..."
	cd apps/query-service && mix compile --warnings-as-errors
	@echo "→ Running Credo..."
	cd apps/query-service && mix credo --strict
	@echo "→ Running Dialyzer..."
	cd apps/query-service && mkdir -p priv/plts && mix dialyzer
	@echo "→ Running tests..."
	@cd apps/query-service && TESTCONTAINERS_RYUK_DISABLED=true mix test 2>&1 | tee $(CURDIR)/$(TEST_OUTPUT_DIR)/query-service-ci.log; \
		test_result=$${PIPESTATUS[0]}; \
		if [ $$test_result -ne 0 ]; then \
			echo ""; \
			echo "❌ QUERY SERVICE TEST FAILURES:"; \
			echo "================================"; \
			grep -E "^\s+[0-9]+\)" $(CURDIR)/$(TEST_OUTPUT_DIR)/query-service-ci.log -A 20 2>/dev/null | head -100 || true; \
			echo ""; \
			echo "📋 Full log: $(CURDIR)/$(TEST_OUTPUT_DIR)/query-service-ci.log"; \
			echo "💡 Re-run failed tests: make elixir-test-failed"; \
			exit 1; \
		fi
	@echo "done (passed successfully)"
	@echo ""
	@echo "--- MCP Server ---"
	@echo "→ Installing dependencies..."
	cd apps/mcp-server-elixir && mix deps.get
	@echo "→ Checking formatting..."
	cd apps/mcp-server-elixir && mix format --check-formatted
	@echo "→ Checking unused dependencies..."
	cd apps/mcp-server-elixir && mix deps.unlock --check-unused
	@echo "→ Compiling with warnings as errors..."
	cd apps/mcp-server-elixir && mix compile --warnings-as-errors
	@echo "→ Running Credo..."
	cd apps/mcp-server-elixir && mix credo --strict
	@echo "→ Running Dialyzer..."
	cd apps/mcp-server-elixir && mkdir -p priv/plts && mix dialyzer
	@echo "→ Running tests..."
	@cd apps/mcp-server-elixir && mix test 2>&1 | tee $(CURDIR)/$(TEST_OUTPUT_DIR)/mcp-server-ci.log; \
		test_result=$${PIPESTATUS[0]}; \
		if [ $$test_result -ne 0 ]; then \
			echo ""; \
			echo "❌ MCP SERVER TEST FAILURES:"; \
			echo "============================"; \
			grep -E "^\s+[0-9]+\)" $(CURDIR)/$(TEST_OUTPUT_DIR)/mcp-server-ci.log -A 20 2>/dev/null | head -100 || true; \
			echo ""; \
			echo "📋 Full log: $(CURDIR)/$(TEST_OUTPUT_DIR)/mcp-server-ci.log"; \
			echo "💡 Re-run failed tests: make elixir-test-failed"; \
			exit 1; \
		fi
	@echo "✅ Full Elixir CI pipeline passed!"

quality-e2e:
	@echo ""
	@if [ -d tooling/e2e ]; then \
		echo "🎭 Running Playwright e2e tests..."; \
		echo "==================================="; \
		(cd tooling/e2e && bun install --frozen-lockfile 2>/dev/null || bun install) && \
		(cd tooling/e2e && bunx playwright install --with-deps chromium) && \
		(cd tooling/e2e && bunx playwright test) && \
		echo "✅ Playwright e2e tests passed!"; \
	else \
		echo "⏭️  Skipping Playwright e2e tests (tooling/e2e not found)"; \
	fi

# =============================================================================
# Individual Service Commands
# =============================================================================

core:
	@echo "⚡ Starting Rust event store on :3900"
	cd apps/core && cargo run --release

control:
	@echo "🎯 Starting Go control plane on :8080"
	cd apps/control-plane && go run main.go

web:
	@echo "🌐 Starting Next.js web UI on :3000"
	cd apps/web && bun dev

mcp:
	@echo "🤖 Starting MCP server"
	cd apps/mcp-server-elixir && mix phx.server


docker-test:
	@echo "🐳 Running full container test suite..."
	./scripts/container-test.sh

docker-test-quick:
	@echo "🐳 Running quick container build test..."
	./scripts/container-test.sh --quick

docker-build:
	@echo "🐳 Building all containers..."
	./scripts/container-test.sh --build-only

docker-clean:
	@echo "🧹 Cleaning test containers and images..."
	-docker rm -f query-service-test-db 2>/dev/null
	-docker rmi allsource-core:test 2>/dev/null
	-docker rmi allsource-web:test 2>/dev/null
	-docker rmi allsource-query-service:test 2>/dev/null
	-docker rmi allsource-mcp-server:test 2>/dev/null
	-docker rmi allsource-control-plane:test 2>/dev/null
	@echo "✅ Test images cleaned"

docker-purge:
	@echo "🧹 Purging ALL docker resources (containers, images, volumes, networks)..."
	-docker stop $$(docker ps -aq) 2>/dev/null
	-docker rm $$(docker ps -aq) 2>/dev/null
	-docker rmi $$(docker images -q) 2>/dev/null
	-docker volume prune -f 2>/dev/null
	-docker network prune -f 2>/dev/null
	-docker system prune -af 2>/dev/null
	@echo "✅ All docker resources purged"

# Individual container builds
docker-core:
	@echo "🐳 Building core container..."
	docker build -t allsource-core:test apps/core

docker-web:
	@echo "🐳 Building web container..."
	docker build -f apps/web/Dockerfile -t allsource-web:test .

docker-query:
	@echo "🐳 Building query-service container..."
	docker build -t allsource-query-service:test apps/query-service

docker-mcp:
	@echo "🐳 Building mcp-server container..."
	docker build -t allsource-mcp-server:test apps/mcp-server-elixir

docker-control:
	@echo "🐳 Building control-plane container..."
	docker build -t allsource-control-plane:test apps/control-plane


up:
	@echo "🚀 Starting all services with Docker Compose..."
	docker compose up -d

down:
	@echo "🛑 Stopping all services..."
	docker compose down

logs:
	docker compose logs -f

ps:
	docker compose ps

# =============================================================================
# Workflow Validation Commands
# =============================================================================

validate-workflows:
	@echo "🔍 Validating GitHub Actions workflows with act..."
	./scripts/validate-workflows.sh

validate-workflows-quick:
	@echo "🔍 Quick workflow syntax validation (actionlint only)..."
	./scripts/validate-workflows.sh --quick

# =============================================================================
# Elixir Testing Commands
# =============================================================================

elixir-test:
	@echo ""
	@echo "🧪 Running Elixir tests with detailed output..."
	@echo "================================================"
	@mkdir -p $(TEST_OUTPUT_DIR)
	@echo "--- Query Service Tests ---"
	cd apps/query-service && TESTCONTAINERS_RYUK_DISABLED=true \
		mix test --trace 2>&1 | tee $(CURDIR)/$(TEST_OUTPUT_DIR)/query-service-test.log; \
		test_result=$$?; \
		if [ $$test_result -ne 0 ]; then \
			echo ""; \
			echo "❌ QUERY SERVICE TEST FAILURES:"; \
			echo "================================"; \
			grep -A 50 "^\s*[0-9]*)" $(CURDIR)/$(TEST_OUTPUT_DIR)/query-service-test.log 2>/dev/null || true; \
			echo ""; \
			echo "📋 Full log: $(CURDIR)/$(TEST_OUTPUT_DIR)/query-service-test.log"; \
		fi; \
		exit $$test_result
	@echo ""
	@echo "--- MCP Server Tests ---"
	cd apps/mcp-server-elixir && mix test --trace 2>&1 | tee $(CURDIR)/$(TEST_OUTPUT_DIR)/mcp-server-test.log; \
		test_result=$$?; \
		if [ $$test_result -ne 0 ]; then \
			echo ""; \
			echo "❌ MCP SERVER TEST FAILURES:"; \
			echo "============================"; \
			grep -A 50 "^\s*[0-9]*)" $(CURDIR)/$(TEST_OUTPUT_DIR)/mcp-server-test.log 2>/dev/null || true; \
			echo ""; \
			echo "📋 Full log: $(CURDIR)/$(TEST_OUTPUT_DIR)/mcp-server-test.log"; \
		fi; \
		exit $$test_result
	@echo ""
	@echo "✅ All Elixir tests passed!"
	@echo "📋 Test logs saved to $(TEST_OUTPUT_DIR)/"

elixir-test-failed:
	@echo ""
	@echo "🔄 Re-running failed Elixir tests..."
	@echo "====================================="
	@echo "--- Query Service Failed Tests ---"
	cd apps/query-service && TESTCONTAINERS_RYUK_DISABLED=true mix test --failed --trace || true
	@echo ""
	@echo "--- MCP Server Failed Tests ---"
	cd apps/mcp-server-elixir && mix test --failed --trace || true
	@echo ""
	@echo "💡 Tip: If no tests run, there are no recorded failures."
	@echo "   Run 'make elixir-test' first to record failures."

elixir-test-watch:
	@echo ""
	@echo "👀 Running Elixir tests in watch mode..."
	@echo "========================================="
	@echo "Press Ctrl+C to stop"
	cd apps/query-service && TESTCONTAINERS_RYUK_DISABLED=true mix test.watch

elixir-test-report:
	@./scripts/elixir-test-report.sh

# =============================================================================
# Release Commands
# =============================================================================

# GitHub org and repo for release URLs
GITHUB_ORG := all-source-os
REPO_NAME := allsource-monorepo
GHCR_REGISTRY := ghcr.io

release: release-preflight
	@echo ""
	@echo "=== Release Configuration ==="
	@echo ""
	@CURRENT=$$(git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0"); \
	echo "Current version: $$CURRENT"; \
	echo ""; \
	MAJOR=$$(echo $$CURRENT | sed 's/v//' | cut -d. -f1); \
	MINOR=$$(echo $$CURRENT | sed 's/v//' | cut -d. -f2); \
	PATCH=$$(echo $$CURRENT | sed 's/v//' | cut -d. -f3); \
	NEXT_PATCH="v$${MAJOR}.$${MINOR}.$$((PATCH + 1))"; \
	NEXT_MINOR="v$${MAJOR}.$$((MINOR + 1)).0"; \
	NEXT_MAJOR="v$$((MAJOR + 1)).0.0"; \
	echo "Version suggestions:"; \
	echo "  1) $$NEXT_PATCH (patch - bug fixes)"; \
	echo "  2) $$NEXT_MINOR (minor - new features)"; \
	echo "  3) $$NEXT_MAJOR (major - breaking changes)"; \
	echo "  4) Custom version"; \
	echo ""; \
	read -p "Select version type (1-4) [1]: " VERSION_TYPE; \
	VERSION_TYPE=$${VERSION_TYPE:-1}; \
	case $$VERSION_TYPE in \
		1) VERSION=$$NEXT_PATCH ;; \
		2) VERSION=$$NEXT_MINOR ;; \
		3) VERSION=$$NEXT_MAJOR ;; \
		4) read -p "Enter version (e.g., v1.0.0): " VERSION ;; \
		*) VERSION=$$NEXT_PATCH ;; \
	esac; \
	if ! echo "$$VERSION" | grep -qE '^v[0-9]+\.[0-9]+\.[0-9]+$$'; then \
		echo "ERROR: Invalid version format. Use vX.Y.Z"; \
		exit 1; \
	fi; \
	if git tag -l | grep -q "^$${VERSION}$$"; then \
		echo "ERROR: Tag $$VERSION already exists!"; \
		exit 1; \
	fi; \
	echo ""; \
	read -p "Release title (e.g., 'Quality & Stability Release'): " TITLE; \
	TITLE=$${TITLE:-"Release"}; \
	echo ""; \
	read -p "Run quality gates before release? (Y/n): " RUN_QG; \
	if [ "$$RUN_QG" != "n" ] && [ "$$RUN_QG" != "N" ]; then \
		$(MAKE) ci || exit 1; \
	fi; \
	echo ""; \
	echo "=== Updating Documentation ==="; \
	sed -i '' "s/ghcr-v[0-9]*\.[0-9]*\.[0-9]*/ghcr-$${VERSION}/g" README.md; \
	sed -i '' "s/^# AllSource Monorepo - v[0-9]*\.[0-9]*\.[0-9]* Release/# AllSource Monorepo - $${VERSION} Release/" RELEASE.md; \
	sed -i '' "s/^\*\*Release Date\*\*:.*/\*\*Release Date\*\*: $$(date +%Y-%m-%d)/" RELEASE.md; \
	echo "Documentation updated."; \
	git diff --stat README.md RELEASE.md; \
	echo ""; \
	echo "=== Creating Git Tag ==="; \
	CHANGES=$$(git log $$(git describe --tags --abbrev=0 2>/dev/null || echo "HEAD~10")..HEAD --oneline | head -15); \
	echo "Recent changes:"; \
	echo "$$CHANGES"; \
	echo ""; \
	read -p "Create tag $$VERSION and push? (Y/n): " CONFIRM; \
	if [ "$$CONFIRM" = "n" ] || [ "$$CONFIRM" = "N" ]; then \
		echo "Aborted."; \
		git checkout README.md RELEASE.md; \
		exit 1; \
	fi; \
	git add README.md RELEASE.md; \
	git commit -m "docs: update version to $$VERSION for release" || true; \
	git tag -a "$$VERSION" -m "$$VERSION - $$TITLE"; \
	echo ""; \
	echo "=== Pushing to Remote ==="; \
	git push origin main; \
	git push origin "$$VERSION"; \
	echo ""; \
	echo "=== Creating GitHub Release ==="; \
	PREV_TAG=$$(git describe --tags --abbrev=0 $$VERSION^ 2>/dev/null || echo ""); \
	gh release create "$$VERSION" \
		--title "AllSource $$VERSION - $$TITLE" \
		--notes "# AllSource $$VERSION - $$TITLE\n\n**Release Date**: $$(date +%Y-%m-%d)\n\n## Changes\n\n$$CHANGES\n\n## Docker Images\n\n\`\`\`bash\ndocker pull $(GHCR_REGISTRY)/$(GITHUB_ORG)/allsource-core:$$VERSION\ndocker pull $(GHCR_REGISTRY)/$(GITHUB_ORG)/allsource-control-plane:$$VERSION\ndocker pull $(GHCR_REGISTRY)/$(GITHUB_ORG)/allsource-query-service:$$VERSION\ndocker pull $(GHCR_REGISTRY)/$(GITHUB_ORG)/allsource-mcp-server:$$VERSION\n\`\`\`\n\n---\n\n**Full changelog**: https://github.com/$(GITHUB_ORG)/$(REPO_NAME)/compare/$${PREV_TAG}...$$VERSION"; \
	echo ""; \
	echo "=== Waiting for Docker Images ==="; \
	echo "Checking docker-publish workflow..."; \
	sleep 5; \
	RUN_ID=$$(gh run list --workflow=docker-publish.yml --limit 1 --json databaseId -q '.[0].databaseId'); \
	if [ -n "$$RUN_ID" ]; then \
		echo "Workflow run: $$RUN_ID"; \
		echo "Monitor at: https://github.com/$(GITHUB_ORG)/$(REPO_NAME)/actions/runs/$$RUN_ID"; \
		read -p "Wait for workflow to complete? (y/N): " WAIT; \
		if [ "$$WAIT" = "y" ] || [ "$$WAIT" = "Y" ]; then \
			gh run watch "$$RUN_ID" --exit-status || true; \
		fi; \
	fi; \
	echo ""; \
	echo "========================================="; \
	echo "  Release $$VERSION Complete!"; \
	echo "========================================="; \
	echo ""; \
	echo "Artifacts:"; \
	echo "  - Git tag: $$VERSION"; \
	echo "  - GitHub Release: https://github.com/$(GITHUB_ORG)/$(REPO_NAME)/releases/tag/$$VERSION"; \
	echo ""; \
	echo "Docker Images (once workflow completes):"; \
	echo "  - $(GHCR_REGISTRY)/$(GITHUB_ORG)/allsource-core:$$VERSION"; \
	echo "  - $(GHCR_REGISTRY)/$(GITHUB_ORG)/allsource-control-plane:$$VERSION"; \
	echo "  - $(GHCR_REGISTRY)/$(GITHUB_ORG)/allsource-query-service:$$VERSION"; \
	echo "  - $(GHCR_REGISTRY)/$(GITHUB_ORG)/allsource-mcp-server:$$VERSION"

release-quick: release-preflight
	@echo ""
	@echo "=== Quick Release (skipping quality gates) ==="
	@CURRENT=$$(git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0"); \
	MAJOR=$$(echo $$CURRENT | sed 's/v//' | cut -d. -f1); \
	MINOR=$$(echo $$CURRENT | sed 's/v//' | cut -d. -f2); \
	PATCH=$$(echo $$CURRENT | sed 's/v//' | cut -d. -f3); \
	VERSION="v$${MAJOR}.$${MINOR}.$$((PATCH + 1))"; \
	echo "Creating patch release: $$VERSION"; \
	read -p "Release title [Patch Release]: " TITLE; \
	TITLE=$${TITLE:-"Patch Release"}; \
	sed -i '' "s/ghcr-v[0-9]*\.[0-9]*\.[0-9]*/ghcr-$${VERSION}/g" README.md; \
	sed -i '' "s/^# AllSource Monorepo - v[0-9]*\.[0-9]*\.[0-9]* Release/# AllSource Monorepo - $${VERSION} Release/" RELEASE.md; \
	git add README.md RELEASE.md; \
	git commit -m "docs: update version to $$VERSION for release" || true; \
	git tag -a "$$VERSION" -m "$$VERSION - $$TITLE"; \
	git push origin main; \
	git push origin "$$VERSION"; \
	PREV_TAG=$$(git describe --tags --abbrev=0 $$VERSION^ 2>/dev/null || echo ""); \
	gh release create "$$VERSION" --title "AllSource $$VERSION - $$TITLE" --generate-notes; \
	echo ""; \
	echo "Release $$VERSION created!"

release-preflight:
	@echo "=== Release Pre-flight Checks ==="
	@echo ""
	@if [ -n "$$(git status --porcelain)" ]; then \
		echo "ERROR: You have uncommitted changes:"; \
		git status --short; \
		exit 1; \
	fi
	@BRANCH=$$(git branch --show-current); \
	if [ "$$BRANCH" != "main" ]; then \
		echo "WARNING: You're on branch '$$BRANCH', not 'main'"; \
		read -p "Continue anyway? (y/N): " REPLY; \
		if [ "$$REPLY" != "y" ] && [ "$$REPLY" != "Y" ]; then \
			exit 1; \
		fi; \
	fi
	@if ! git ls-remote --exit-code origin &>/dev/null; then \
		echo "ERROR: Cannot reach git remote 'origin'"; \
		exit 1; \
	fi
	@if ! gh auth status &>/dev/null; then \
		echo "ERROR: GitHub CLI not authenticated. Run 'gh auth login'"; \
		exit 1; \
	fi
	@echo "Pre-flight checks passed!"

version:
	@echo "Current version: $$(git describe --tags --abbrev=0 2>/dev/null || echo 'no tags')"
	@echo ""
	@echo "Recent commits:"
	@git log --oneline -5
	@echo ""
	@echo "Tags:"
	@git tag -l | sort -V | tail -5

images-check:
	@echo "=== Docker Images in GHCR ==="
	@for SERVICE in core control-plane query-service mcp-server; do \
		echo ""; \
		echo "allsource-$$SERVICE:"; \
		gh api "/orgs/$(GITHUB_ORG)/packages/container/allsource-$$SERVICE/versions" \
			--jq '.[0:5] | .[] | "  \(.metadata.container.tags | join(", "))"' 2>/dev/null || echo "  (not found or no access)"; \
	done

# =============================================================================
# Version Management Commands
# =============================================================================

# Set version across all services
# Set the version of ONE SDK. SDKs release independently of the apps and of each
# other — CLAUDE.md: "SDK-only releases use sdk-<lang>-v<VERSION> ... so per-SDK
# versions don't collide with Core/QS versions". `set-version` therefore leaves
# them alone; rewriting them in lockstep would push a published SDK's version
# backwards (e.g. @allsourcedev/client is on npm at 0.23.1, apps at 0.22.0).
# Usage: make set-sdk-version SDK=typescript VERSION=0.23.2
set-sdk-version:
ifndef SDK
	@echo "ERROR: SDK is required. One of: rust, go, typescript, python"
	@exit 1
endif
ifndef VERSION
	@echo "ERROR: VERSION is required. Usage: make set-sdk-version SDK=typescript VERSION=0.23.2"
	@exit 1
endif
	@case "$(SDK)" in \
	  rust) sed -i '' 's/^version = "[0-9]*\.[0-9]*\.[0-9]*"/version = "$(VERSION)"/' sdks/rust/Cargo.toml ;; \
	  go) sed -i '' 's/Version = "[0-9]*\.[0-9]*\.[0-9]*"/Version = "$(VERSION)"/' sdks/go/version.go ;; \
	  typescript|ts) sed -i '' 's/"version": "[0-9]*\.[0-9]*\.[0-9]*"/"version": "$(VERSION)"/' sdks/typescript/package.json ;; \
	  python|py) sed -i '' 's/^version = "[0-9]*\.[0-9]*\.[0-9]*"/version = "$(VERSION)"/' sdks/python-client/pyproject.toml ;; \
	  *) echo "ERROR: unknown SDK '$(SDK)' (want rust, go, typescript, python)"; exit 1 ;; \
	esac
	@echo "$(SDK) SDK set to $(VERSION). Tag it as sdk-$(SDK)-v$(VERSION)."

# Usage: make set-version VERSION=0.9.0
set-version:
ifndef VERSION
	@echo "ERROR: VERSION is required. Usage: make set-version VERSION=0.9.0"
	@exit 1
endif
	@echo "=== Setting version to $(VERSION) across all services ==="
	@echo ""
	@echo "Updating Rust Core (Cargo.toml)..."
	@sed -i '' 's/^version = "[0-9]*\.[0-9]*\.[0-9]*"/version = "$(VERSION)"/' apps/core/Cargo.toml
	@echo "Updating Go Control Plane (main.go)..."
	@sed -i '' 's/Version = "[0-9]*\.[0-9]*\.[0-9]*"/Version = "$(VERSION)"/' apps/control-plane/main.go
	@echo "Updating Go Control Plane (tracing.go)..."
	@sed -i '' 's/serviceVersion = "[0-9]*\.[0-9]*\.[0-9]*"/serviceVersion = "$(VERSION)"/' apps/control-plane/tracing.go
	@echo "Updating Query Service (mix.exs)..."
	@sed -i '' 's/version: "[0-9]*\.[0-9]*\.[0-9]*"/version: "$(VERSION)"/' apps/query-service/mix.exs
	@echo "Updating MCP Server (mix.exs)..."
	@sed -i '' 's/version: "[0-9]*\.[0-9]*\.[0-9]*"/version: "$(VERSION)"/' apps/mcp-server-elixir/mix.exs
	@echo "Updating K8s Core manifest..."
	@sed -i '' 's|image: allsource/core:[0-9]*\.[0-9]*\.[0-9]*|image: allsource/core:$(VERSION)|' deploy/k8s/core.yaml
	@echo "Updating K8s Query Service manifest..."
	@sed -i '' 's|image: allsource/query-service:[0-9]*\.[0-9]*\.[0-9]*|image: allsource/query-service:$(VERSION)|' deploy/k8s/query-service.yaml
	@echo "Updating README.md version..."
	@sed -i '' 's/version: "[0-9]*\.[0-9]*\.[0-9]*"/version: "$(VERSION)"/' README.md
	@sed -i '' 's/\*\*Monorepo Version\*\*: v[0-9]*\.[0-9]*\.[0-9]*/\*\*Monorepo Version\*\*: v$(VERSION)/' README.md
	@echo "Skipping SDKs — they version independently (see set-sdk-version)."
	@echo "Updating Prime MCP Server (Cargo.toml)..."
	@sed -i '' 's/^version = "[0-9]*\.[0-9]*\.[0-9]*"/version = "$(VERSION)"/' apps/prime-mcp/Cargo.toml
	@sed -i '' 's/allsource-core = { version = "[0-9]*\.[0-9]*\(\.[0-9]*\)\{0,1\}"/allsource-core = { version = "$(VERSION)"/' apps/prime-mcp/Cargo.toml
	@echo "Updating recall-bench (Cargo.toml)..."
	@sed -i '' 's/allsource-core = { version = "[0-9]*\.[0-9]*\(\.[0-9]*\)\{0,1\}"/allsource-core = { version = "$(VERSION)"/' tooling/recall-bench/Cargo.toml
	@echo "Updating chronis allsource-core dep (Cargo.toml)..."
	@sed -i '' 's/allsource-core = { version = "[0-9]*\.[0-9]*\(\.[0-9]*\)\{0,1\}"/allsource-core = { version = "$(VERSION)"/' apps/chronis/Cargo.toml
	@echo "Updating allsource-mcp allsource-core dep (Cargo.toml)..."
	@sed -i '' 's/allsource-core = { version = "[0-9]*\.[0-9]*\(\.[0-9]*\)\{0,1\}"/allsource-core = { version = "$(VERSION)"/' tooling/allsource-mcp/Cargo.toml
	@echo "Updating Control Plane OpenAPI spec (info.version)..."
	@sed -i '' 's/^  version: [0-9]*\.[0-9]*\.[0-9]*/  version: $(VERSION)/' apps/control-plane/docs/openapi.yaml
	@echo ""
	@echo "=== Version $(VERSION) set across all services ==="
	@echo ""
	@echo "Files updated:"
	@echo "  - apps/core/Cargo.toml"
	@echo "  - apps/control-plane/main.go"
	@echo "  - apps/control-plane/tracing.go"
	@echo "  - apps/query-service/mix.exs"
	@echo "  - apps/mcp-server-elixir/mix.exs"
	@echo "  - deploy/k8s/core.yaml"
	@echo "  - deploy/k8s/query-service.yaml"
	@echo "  - README.md"
	@echo "  - apps/prime-mcp/Cargo.toml"
	@echo "  - tooling/recall-bench/Cargo.toml"
	@echo "  - apps/chronis/Cargo.toml"
	@echo "  - tooling/allsource-mcp/Cargo.toml"
	@echo "  - apps/control-plane/docs/openapi.yaml"
	@echo ""
	@echo "Run 'make check-versions' to verify consistency"

# Dry-run SDK publishing (for local validation before CI does the real publish)
publish-sdks:
	@echo "=== SDK Publish Dry Run ==="
	@echo ""
	@echo "Rust SDK (crates.io)..."
	cd sdks/rust && cargo publish --dry-run
	@echo ""
	@echo "TypeScript SDK (npm)..."
	cd sdks/typescript && bun run build && npm pack
	@echo ""
	@echo "Python SDK (PyPI)..."
	cd sdks/python-client && hatch build
	@echo ""
	@echo "Go SDK — no build step (synced to separate repo by CI)"
	@echo ""
	@echo "=== Dry run complete. Use CI to publish for real. ==="

# Interactive version bump
bump-version:
	@echo "=== Interactive Version Bump ==="
	@CURRENT=$$(grep 'version = ' apps/core/Cargo.toml | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'); \
	echo "Current version: $$CURRENT"; \
	echo ""; \
	MAJOR=$$(echo $$CURRENT | cut -d. -f1); \
	MINOR=$$(echo $$CURRENT | cut -d. -f2); \
	PATCH=$$(echo $$CURRENT | cut -d. -f3); \
	NEXT_PATCH="$${MAJOR}.$${MINOR}.$$((PATCH + 1))"; \
	NEXT_MINOR="$${MAJOR}.$$((MINOR + 1)).0"; \
	NEXT_MAJOR="$$((MAJOR + 1)).0.0"; \
	echo "Select new version:"; \
	echo "  1) $$NEXT_PATCH (patch)"; \
	echo "  2) $$NEXT_MINOR (minor)"; \
	echo "  3) $$NEXT_MAJOR (major)"; \
	echo "  4) Custom"; \
	echo ""; \
	read -p "Choice [1]: " CHOICE; \
	CHOICE=$${CHOICE:-1}; \
	case $$CHOICE in \
		1) NEW_VERSION=$$NEXT_PATCH ;; \
		2) NEW_VERSION=$$NEXT_MINOR ;; \
		3) NEW_VERSION=$$NEXT_MAJOR ;; \
		4) read -p "Enter version: " NEW_VERSION ;; \
		*) NEW_VERSION=$$NEXT_PATCH ;; \
	esac; \
	echo ""; \
	read -p "Set version to $$NEW_VERSION? (Y/n): " CONFIRM; \
	if [ "$$CONFIRM" != "n" ] && [ "$$CONFIRM" != "N" ]; then \
		$(MAKE) set-version VERSION=$$NEW_VERSION; \
	else \
		echo "Aborted."; \
	fi
