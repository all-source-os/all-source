# Rust Core Service Documentation

**Service**: allsource-core
**Language**: Rust
**Last Updated**: 2025-10-22

---

## 📚 Documentation Index

### Architecture
- [Clean Architecture Implementation](./architecture/CLEAN_ARCHITECTURE.md)
- [Domain Layer Design](./architecture/DOMAIN_LAYER.md)
- [Repository Pattern](./architecture/REPOSITORIES.md)

### Development Guides
- [Getting Started](./guides/GETTING_STARTED.md)
- [Testing Guide](./guides/TESTING.md)
- [Performance Optimization](./guides/PERFORMANCE.md)
- [Contributing](./guides/CONTRIBUTING.md)

### API Reference
- [Event API](./api/EVENTS.md)
- [Storage API](./api/STORAGE.md)
- [Index API](./api/INDEX.md)

### Changelog
- [Service Changelog](./changelog/CHANGELOG.md)

---

## 🏗️ Current Architecture

```
src/
├── domain/              ✅ Clean Architecture Layer 1
│   ├── entities/       - Pure domain entities
│   └── repositories/   - Repository trait abstractions
├── application/         ✅ Clean Architecture Layer 2
│   ├── dto/           - Data Transfer Objects
│   └── use_cases/     - Business logic orchestration
├── infrastructure/      ⏳ Clean Architecture Layer 3 (30%)
│   └── [to be organized]
└── [legacy modules]     ⏳ Being refactored
```

---

## ⚡ Performance

**Current (v1.0)**:
- Ingestion: 469K events/sec
- Query p99: 11.9μs
- Concurrent writes: 7.98ms (8 threads)

**Target (v1.2)**:
- Ingestion: 1M+ events/sec
- Query p99: <5μs
- Concurrent writes: <4ms

---

## 🧪 Testing

```bash
# Run all tests
cargo test --lib

# Run specific module tests
cargo test --lib -- domain::
cargo test --lib -- application::

# Run benchmarks
cargo bench --bench performance_benchmarks
```

**Current Status**: ✅ 86/86 tests passing (100%)

---

## 🚀 Quick Start

See [Getting Started Guide](./guides/GETTING_STARTED.md)

---

## 📖 Related Documentation

- [Project Documentation](../../../docs/INDEX.md)
- [Phase 1.5 Progress](../../../docs/roadmaps/2025-10-22_PHASE_1.5_PROGRESS.md)
- [Clean Architecture Guide](../../../docs/current/CLEAN_ARCHITECTURE.md)

---

**Navigation**: [Home](../../../README.md) | [Architecture](./architecture/) | [Guides](./guides/) | [API](./api/)
