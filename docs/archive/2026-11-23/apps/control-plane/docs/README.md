# Go Control Plane Service Documentation

**Service**: control-plane
**Language**: Go
**Last Updated**: 2025-10-22

---

## 📚 Documentation Index

### Architecture
- [Architecture Overview](./architecture/OVERVIEW.md) - ⏳ PLANNED
- [Clean Architecture Migration](./architecture/CLEAN_ARCHITECTURE_MIGRATION.md) - ⏳ PLANNED

### Development Guides
- [Getting Started](./guides/GETTING_STARTED.md)
- [Testing Guide](./guides/TESTING.md)
- [Contributing](./guides/CONTRIBUTING.md)

### API Reference
- [REST API](./api/REST_API.md)
- [Authentication](./api/AUTH.md)
- [Tenancy](./api/TENANCY.md)

### Changelog
- [Service Changelog](./changelog/CHANGELOG.md)

---

## 🏗️ Current Architecture

```
✅ Clean Architecture Implemented (2025-10-22)

internal/
├── domain/              ✅ Layer 1: Business entities
│   ├── entities/       - Tenant, User, Policy, AuditEvent
│   ├── repositories/   - Repository interfaces
│   └── errors.go       - Domain errors
├── application/         ✅ Layer 2: Use cases
│   ├── dto/            - Request/Response DTOs
│   └── usecases/       - CreateTenant, EvaluatePolicy
└── infrastructure/      ✅ Layer 3: Adapters
    └── persistence/    - In-memory repositories

[Legacy files to be migrated]:
├── main.go             ⏳ To be migrated
├── auth.go             ⏳ To be migrated
├── policy.go           ⏳ To be migrated
└── audit.go            ⏳ To be migrated
```

---

## 🧪 Testing

```bash
# Run all tests
go test ./...

# Run with coverage
go test ./... -cover

# Run specific tests
go test -v -run TestPolicyEngine
```

**Current Status**: ✅ All tests passing (39+ tests), Clean Architecture implemented

---

## 🔧 Recent Fixes

### Policy Engine Fix (2025-10-22)
- **Issue**: Policy evaluated user's tenant instead of target tenant
- **Fix**: Updated `evaluateCondition()` to check `Attributes["tenant_id"]`
- **File**: `policy.go:290-310`
- **Impact**: All policy tests now passing

---

## 📅 Refactoring Roadmap

### Phase 1.5 (Weeks 6-8) - ✅ 100% COMPLETE
- [x] Create `internal/domain` structure
- [x] Implement use case layer
- [x] Apply SOLID principles
- [x] Dependency injection (Container pattern)
- [x] Migrate handlers to use cases
- [x] Migrate auth to use domain entities
- [x] Wire main.go to new architecture

**Completed** (2025-10-22):
- ✅ Domain entities (Tenant, User, Policy, AuditEvent)
- ✅ Repository interfaces
- ✅ Use cases (CreateTenant, EvaluatePolicy)
- ✅ In-memory repository implementations
- ✅ HTTP handlers (TenantHandler, PolicyHandler)
- ✅ Dependency injection container
- ✅ Auth migration to domain entities
- ✅ Main.go integration
- ✅ Comprehensive tests (39+ tests passing, 100% pass rate)

See: [Phase 1.5 Progress](../../../docs/roadmaps/2025-10-22_PHASE_1.5_PROGRESS.md)

---

## 📖 Related Documentation

- [Project Documentation](../../../docs/INDEX.md)
- [Rust Core Docs](../../core/docs/README.md)
- [SOLID Principles](../../../docs/current/SOLID_PRINCIPLES.md)

---

**Navigation**: [Home](../../../README.md) | [Architecture](./architecture/) | [Guides](./guides/) | [API](./api/)
