---
title: "AllSource - Quick Start Guide"
status: CURRENT
last_updated: 2026-11-23
category: guide
---

# AllSource - Quick Start Guide

## ⚡ Using as a Library

The fastest way to get started is adding `allsource-core` to your Rust project:

```bash
cargo add allsource-core@0.30
```

Or add to your `Cargo.toml` (pin to minor version for stability):

```toml
[dependencies]
# allsource-core: High-performance event store
# Pin to minor version - allows patch updates only
allsource-core = "0.30"
```

> **Version Pinning**: We recommend pinning to `"0.30"` (minor version) rather than `"0.30.0"` (exact) or `"0"` (major only). This allows automatic patch updates for bug fixes while avoiding breaking changes. See our [Dependency Management Guide](https://github.com/all-source-os/all-frame/blob/main/docs/DEPENDENCY_MANAGEMENT.md) for best practices.

**Links**: [crates.io](https://crates.io/crates/allsource-core) · [docs.rs](https://docs.rs/allsource-core) · [GitHub](https://github.com/all-source-os/allsource-monorepo)

---

## 🏃 Running the Full Stack

### Step 1: Install Dependencies (One-Time Setup)

```bash
# Install Bun if you don't have it
curl -fsSL https://bun.sh/install | bash

# Install all project dependencies
bun install

# Install Go dependencies
cd apps/control-plane && go mod download && cd ../..
```

### Step 2: Start Services (3 Terminals)

**Terminal 1 - Event Store Core:**
```bash
cd apps/core
cargo run --release
# Wait for: "🚦 Listening on 0.0.0.0:3900; answering not_ready until the bootstrap finishes"
```

**Terminal 2 - Control Plane:**
```bash
cd apps/control-plane
go run main.go
# Wait for: "Control Plane v0.30.2 listening on port 3901"
```

**Terminal 3 - Web UI:**
```bash
cd apps/web
bun dev
# Wait for: "▲ Next.js 16.3.5 - Local: http://localhost:3000"
```

### Step 3: Open Dashboard

Visit: **http://localhost:3000**

---

## 🎯 Quick Demo Actions

### 1. Ingest Events
Click **"Ingest Demo Event"** button → Watch stats update in real-time

### 2. Query by Entity
1. Copy the entity ID shown in the button (e.g., `user-789`)
2. Paste into "Entity ID" filter
3. Click "Search"
4. Expand an event to see the payload

### 3. API Test
```bash
# Ingest via API
curl -X POST http://localhost:3900/api/v1/events \
  -H "Content-Type: application/json" \
  -d '{
    "event_type": "test.event",
    "entity_id": "demo-123",
    "payload": {"message": "Hello AllSource!"}
  }'

# Query the event
curl "http://localhost:3900/api/v1/events/query?entity_id=demo-123" | jq
```

---

## 🐛 Troubleshooting

### "Port already in use"
```bash
# Check what's using the ports
lsof -i :3900  # Core
lsof -i :3901  # Control Plane
lsof -i :3000  # Web UI

# Kill if needed
kill -9 <PID>
```

### "Rust/Go/Bun not found"
- Install Rust: https://rustup.rs/
- Install Go: https://go.dev/dl/
- Install Bun: https://bun.sh/

### "Service won't start"
```bash
# Check logs for errors
cd apps/core && cargo run  # See Rust errors
cd apps/control-plane && go run main.go  # See Go errors
cd apps/web && bun dev  # See Next.js errors
```

### "Can't connect to Core from Control Plane"
Make sure Core is running first! Control Plane depends on Core.

---

## 📝 Useful Commands

```bash
# View all make targets
make help

# Install everything
make install

# Run all services (requires tmux or manually open 3 terminals)
make dev

# Clean all builds
make clean

# Start the demo web dashboard
make demo

# Format code
bun format
```

---

## 🔗 Service URLs

| Service | URL | Purpose |
|---------|-----|---------|
| Web UI | http://localhost:3000 | Visual dashboard |
| Event Store API | http://localhost:3900 | Core event operations |
| Control Plane | http://localhost:3901 | Cluster management |
| Query Service | http://localhost:3902 | Elixir query processing |

---

## 📚 Next Steps

1. **Read the full README:** `README.md`
2. **Understand the architecture:** `docs/current/`
3. **Try the demo:** `make demo`

---

## 🆘 Still Stuck?

Check these files:
- `README.md` - Full documentation
- `docs/current/` - Current architecture docs

Or create an issue describing your problem!

---

<div align="center">

**AllSource** - *You're ready to roll!* 🚀

</div>
