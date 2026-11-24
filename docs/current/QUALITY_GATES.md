---
title: "Quality Gates - AllSource Core"
status: CURRENT
last_updated: 2026-11-24
version: "1.0"
related:
  - "../archive/2026-11-23/current/CLEAN_ARCHITECTURE.md"
  - "./CRITICAL_BUGS_FIXED.md"
---

# Quality Gates - AllSource Core

**MSRV**: Rust 1.92

This document describes the quality gates enforced for AllSource Core to maintain code quality, consistency, and reliability.

## Overview

Quality gates are **automated checks** that must pass before code can be merged. They enforce:
- **Code formatting** standards (rustfmt)
- **Code quality** standards (clippy)
- **Dependency organization** (cargo-sort)
- **Test coverage** (cargo test)
- **Build verification** (cargo build)

---

## Quick Reference

### Local Development

```bash
# Run all quality gates (recommended before commit)
make check

# Auto-fix formatting and sorting
make format
make format-sort

# Full CI pipeline locally
make ci
```

### Individual Gates

```bash
# Check formatting (no changes)
make lint

# Check Cargo.toml sorting (no changes)
make lint-sort

# Auto-format code
make format

# Auto-sort Cargo.toml
make format-sort

# Run tests
make test

# Build project
make build
```

---

## Quality Gate 1: Code Formatting (rustfmt)

### Purpose
Enforce consistent code style across the entire codebase using `rustfmt`.

### Configuration
**File**: `rustfmt.toml` (repo root)

```toml
edition = "2021"
max_width = 100
hard_tabs = false
tab_spaces = 4
newline_style = "Unix"

# Import organization
reorder_imports = true
reorder_modules = true
imports_granularity = "Crate"
group_imports = "StdExternalCrate"

# Formatting
remove_nested_parens = true
format_strings = true
format_macro_matchers = true
format_macro_bodies = true

# Comments
normalize_comments = true
normalize_doc_attributes = true
wrap_comments = true
comment_width = 80
```

### Check Command
```bash
cargo fmt --check
```

### Auto-fix Command
```bash
cargo fmt
```

### What It Checks
- ✅ Code indentation (4 spaces, no tabs)
- ✅ Line width (max 100 characters)
- ✅ Import organization (std → external → crate)
- ✅ Comment formatting (max 80 characters)
- ✅ Macro formatting
- ✅ String formatting

### Common Failures & Fixes

**Failure**: Line too long
```rust
// ❌ BEFORE (110 characters)
let result = some_very_long_function_name_that_exceeds_the_maximum_line_width(arg1, arg2, arg3, arg4);

// ✅ AFTER (auto-formatted)
let result = some_very_long_function_name_that_exceeds_the_maximum_line_width(
    arg1, arg2, arg3, arg4,
);
```

**Failure**: Imports not organized
```rust
// ❌ BEFORE
use crate::domain::entities::Event;
use std::sync::Arc;
use serde::{Serialize, Deserialize};

// ✅ AFTER (auto-formatted)
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::domain::entities::Event;
```

---

## Quality Gate 2: Code Quality (clippy)

### Purpose
Catch common mistakes, anti-patterns, and performance issues using Rust's official linter.

### Configuration
**File**: `apps/core/.clippy.toml`

```toml
msrv = "1.92"
warn-on-all-wildcard-imports = true
```

### Check Command
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Flags Explained
- `--all-targets`: Check lib, bins, tests, benches
- `--all-features`: Enable all feature flags (postgres, rocksdb)
- `-D warnings`: Treat all warnings as errors (zero tolerance)

### What It Checks
- ✅ Wildcard imports (`use foo::*;` → forbidden)
- ✅ Unused variables
- ✅ Unnecessary clones
- ✅ Inefficient loops
- ✅ Missing documentation
- ✅ Type complexity
- ✅ Redundant pattern matching
- ✅ Performance anti-patterns

### Common Failures & Fixes

**Failure**: Wildcard import
```rust
// ❌ FORBIDDEN
use crate::domain::entities::*;

// ✅ REQUIRED
use crate::domain::entities::{Event, EventStream, Projection};
```

**Failure**: Unnecessary clone
```rust
// ❌ BEFORE
fn process_event(event: &Event) {
    let event_clone = event.clone();  // clippy: unnecessary clone
    do_something(event_clone);
}

// ✅ AFTER
fn process_event(event: &Event) {
    do_something(event);  // Pass by reference
}
```

**Failure**: Unused variable
```rust
// ❌ BEFORE
fn load_stream(id: &str) -> Result<Stream> {
    let stream = repository.load(id)?;
    let version = stream.version();  // clippy: unused variable
    Ok(stream)
}

// ✅ AFTER
fn load_stream(id: &str) -> Result<Stream> {
    let stream = repository.load(id)?;
    let _version = stream.version();  // Prefix with _ to suppress warning
    Ok(stream)
}
```

---

## Quality Gate 3: Dependency Sorting (cargo-sort)

### Purpose
Maintain alphabetically sorted dependencies in `Cargo.toml` for easier navigation and merge conflict reduction.

### Configuration
**File**: `apps/core/cargo-sort.toml`

```toml
sort = true
expand = true
group_imports = true
```

### Check Command
```bash
cargo sort --check
```

### Auto-fix Command
```bash
cargo sort -w
```

### What It Checks
- ✅ `[dependencies]` sorted alphabetically
- ✅ `[dev-dependencies]` sorted alphabetically
- ✅ `[build-dependencies]` sorted alphabetically
- ✅ Feature flags sorted

### Example

**Before**:
```toml
[dependencies]
serde = "1.0"
tokio = "1.36"
arrow = "53.3"
```

**After** (auto-sorted):
```toml
[dependencies]
arrow = "53.3"
serde = "1.0"
tokio = "1.36"
```

---

## Quality Gate 4: Tests

### Purpose
Ensure all unit and integration tests pass before code is merged.

### Check Command
```bash
cargo test --lib
```

### What It Checks
- ✅ All unit tests in `src/**/*.rs` pass
- ✅ All doc tests pass
- ✅ No test panics
- ✅ Test coverage maintained

### Test Organization
```
apps/core/src/
├── domain/
│   ├── entities/
│   │   ├── event.rs          # Contains #[cfg(test)] mod tests
│   │   ├── event_stream.rs   # Contains #[cfg(test)] mod tests
│   └── value_objects/
│       ├── tenant_id.rs      # Contains #[cfg(test)] mod tests
├── application/
│   └── use_cases/
│       ├── manage_schema.rs  # Contains #[cfg(test)] mod tests
└── infrastructure/
    └── repositories/
        ├── in_memory_event_stream_repository.rs  # Contains #[cfg(test)] mod tests
```

### Test Coverage Requirements
- **Domain layer**: 100% coverage required
- **Application layer**: 90%+ coverage required
- **Infrastructure layer**: 80%+ coverage required

---

## Quality Gate 5: Build Verification

### Purpose
Verify the project compiles successfully in release mode.

### Check Command
```bash
cargo build --lib --release
```

### What It Checks
- ✅ Project compiles without errors
- ✅ All dependencies resolve correctly
- ✅ Feature flags work correctly
- ✅ Release optimizations don't break compilation

---

## CI/CD Integration

### GitHub Actions Workflow

**Job**: `rust-quality` in `.github/workflows/ci.yml`

**Triggers**:
- Push to `main` or `develop` branches
- Pull requests to `main` or `develop` branches
- Changes in `apps/core/**` paths

**Jobs**:

1. **quality-gates** - Full quality gate pipeline
   - Format check (rustfmt)
   - Cargo.toml sort check (cargo-sort)
   - Lint check (clippy -D warnings)
   - Test execution (cargo test --lib)
   - Build verification (cargo build --lib --release)

2. **msrv-check** - Minimum Supported Rust Version check
   - Verifies compilation with Rust 1.92

### CI Execution
```yaml
name: Rust Quality Gates

on:
  push:
    branches: [ main, develop ]
  pull_request:
    branches: [ main, develop ]

jobs:
  quality-gates:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - run: cargo install cargo-sort
      - run: cargo fmt --check
      - run: cargo sort --check
      - run: cargo clippy --all-targets --all-features -- -D warnings
      - run: cargo test --lib
      - run: cargo build --lib --release
```

---

## Pre-commit Hooks (Recommended)

### Setup
```bash
# Install pre-commit hook
cd apps/core
cat > .git/hooks/pre-commit << 'HOOK'
#!/bin/bash
cd apps/core
make check
HOOK
chmod +x .git/hooks/pre-commit
```

### What It Does
Automatically runs `make check` before every commit, preventing broken code from being committed.

---

## Makefile Reference

### Available Commands

| Command | Description | Exit Code |
|---------|-------------|-----------|
| `make help` | Show all available commands | - |
| `make check` | Run all quality gates | 0 = pass, 1 = fail |
| `make ci` | Full CI pipeline (check + build) | 0 = pass, 1 = fail |
| `make lint` | Check formatting + clippy | 0 = pass, 1 = fail |
| `make lint-sort` | Check Cargo.toml sorting | 0 = pass, 1 = fail |
| `make format` | Auto-format code | - |
| `make format-sort` | Auto-sort Cargo.toml | - |
| `make test` | Run all tests | 0 = pass, 1 = fail |
| `make build` | Build project | 0 = pass, 1 = fail |
| `make clean` | Clean build artifacts | - |

### Recommended Workflow

**Before starting work**:
```bash
make format
make format-sort
```

**Before committing**:
```bash
make check  # Must pass before commit
```

**Before pushing**:
```bash
make ci  # Full pipeline verification
```

---

## Bypassing Quality Gates (Emergency Only)

### When to Bypass
- ⚠️ **NEVER** bypass in normal development
- ⚠️ Only in critical production incidents
- ⚠️ Must be approved by 2+ maintainers
- ⚠️ Must be fixed in follow-up PR within 24 hours

### How to Bypass (Git)
```bash
# WARNING: Use only in emergencies!
git commit --no-verify -m "EMERGENCY: fix critical prod issue"
```

### How to Bypass (CI)
```yaml
# Add [skip ci] to commit message
git commit -m "EMERGENCY: critical fix [skip ci]"
```

---

## Troubleshooting

### Issue: `cargo fmt --check` fails

**Cause**: Code not formatted according to `rustfmt.toml`

**Fix**:
```bash
make format  # Auto-fix formatting
git add .
git commit
```

### Issue: `cargo clippy` fails with warnings

**Cause**: Code quality issues detected by clippy

**Fix**:
```bash
# See all warnings
cargo clippy --all-targets --all-features

# Fix suggested changes manually
# OR use clippy auto-fix (use with caution)
cargo clippy --fix --all-targets --all-features
```

### Issue: `cargo sort --check` fails

**Cause**: Dependencies not alphabetically sorted

**Fix**:
```bash
make format-sort  # Auto-sort Cargo.toml
git add Cargo.toml
git commit
```

### Issue: Tests failing locally but pass in CI

**Cause**: Different Rust version or cached artifacts

**Fix**:
```bash
# Clean and rebuild
make clean
rustup update
make check
```

### Issue: Build succeeds locally but fails in CI

**Cause**: Feature flags or dependencies missing in CI

**Fix**:
- Check the `rust-quality` job in `.github/workflows/ci.yml`
- Ensure all required dependencies in `Cargo.toml`
- Verify feature flags are configured correctly

---

## Best Practices

### 1. Run `make check` Before Every Commit
```bash
# Always pass quality gates locally first
make check && git commit
```

### 2. Use Auto-fix Commands Liberally
```bash
# Let tools fix what they can
make format
make format-sort
# Then review changes
git diff
```

### 3. Fix Clippy Warnings Immediately
```rust
// Don't silence warnings unless absolutely necessary
#![allow(clippy::some_lint)]  // ❌ Avoid

// Instead, fix the underlying issue
// ✅ Proper fix
```

### 4. Write Tests Alongside Code
```rust
// Add tests in same file using #[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_feature() {
        // Test implementation
    }
}
```

### 5. Keep Cargo.toml Sorted
```bash
# After adding dependencies
cargo add some-crate
make format-sort
```

---

## Maintenance

### Updating rustfmt Configuration
```bash
# 1. Edit rustfmt.toml
nano rustfmt.toml

# 2. Reformat all code
make format

# 3. Verify no regressions
make check
```

### Updating clippy Configuration
```bash
# 1. Edit .clippy.toml
nano apps/core/.clippy.toml

# 2. Run clippy with new config
make lint

# 3. Fix any new warnings
# (may require code changes)
```

### Updating MSRV
```bash
# 1. Edit .clippy.toml
# msrv = "1.92"  # Update version

# 2. Update CI workflow
# .github/workflows/ci.yml (job rust-quality)
# - uses: dtolnay/rust-toolchain@1.92

# 3. Test with new MSRV
rustup install 1.92
rustup default 1.92
make check
```

---

## Related Documentation

- [Critical Bugs Fixed](CRITICAL_BUGS_FIXED.md)
- [Architecture Guide](../archive/2026-11-23/current/CLEAN_ARCHITECTURE.md)
- [Core README](../../apps/core/README.md)

