---
title: "Go Control Plane - Test Coverage Improvement Plan"
status: CURRENT
last_updated: 2026-02-02
category: testing
---

# Go Control Plane - Test Coverage Improvement Plan

## 📊 Current Status (Overall: 23.1%)

### Coverage by Layer

| Layer | Coverage | Status | Priority |
|-------|----------|--------|----------|
| **Application/UseCases** | 82.1% | ✅ Good | Maintain |
| **Domain/Entities** | 52.9% | ⚠️ Moderate | Improve |
| **Infrastructure/Persistence** | 0.0% | ❌ Critical | **High** |
| **Interfaces/HTTP** | 0.0% | ❌ Critical | **High** |
| **Root Package** | 23.1% | ❌ Poor | Medium |

### Missing Test Files

**Critical (0% coverage):**
- `internal/application/dto/*.go` (3 files) - No tests
- `internal/infrastructure/persistence/*.go` (4 files) - No tests
- `internal/interfaces/http/*.go` (2 files) - No tests
- `internal/container.go` - No tests
- `internal/domain/errors.go` - No tests
- `internal/domain/repositories/*.go` (4 interface files) - No tests needed (interfaces)

**Root package files needing tests:**
- `audit.go` / `audit_test.go` (exists)
- `auth.go` / `auth_test.go` (exists)
- `policy.go` / `policy_test.go` (exists)
- `main.go` - No test
- `metrics.go` - No test
- `middleware.go` - No test
- `tracing.go` - No test

## 🎯 Goal: Achieve 70%+ Coverage

### Phase 1: Infrastructure Layer (Priority: HIGH)

**Target: 70% coverage for infrastructure**

#### 1.1 Memory Repository Tests

Create test files:

```bash
touch internal/infrastructure/persistence/memory_tenant_repository_test.go
touch internal/infrastructure/persistence/memory_user_repository_test.go
touch internal/infrastructure/persistence/memory_policy_repository_test.go
touch internal/infrastructure/persistence/memory_audit_repository_test.go
```

**Test Coverage for Each Repository:**

```go
// memory_tenant_repository_test.go
package persistence

import (
    "testing"
    "github.com/allsource/control-plane/internal/domain/entities"
)

func TestMemoryTenantRepository_Create(t *testing.T) {
    // Test creating tenant
    // Test duplicate tenant (should error)
}

func TestMemoryTenantRepository_FindByID(t *testing.T) {
    // Test finding existing tenant
    // Test finding non-existent tenant
}

func TestMemoryTenantRepository_Update(t *testing.T) {
    // Test updating existing tenant
    // Test updating non-existent tenant
}

func TestMemoryTenantRepository_Delete(t *testing.T) {
    // Test deleting existing tenant
    // Test deleting non-existent tenant
}

func TestMemoryTenantRepository_List(t *testing.T) {
    // Test listing all tenants
    // Test empty repository
}
```

**Estimated Tests:** 5 tests × 4 repositories = **20 tests**

#### 1.2 Concurrency Tests

```go
func TestMemoryTenantRepository_ConcurrentAccess(t *testing.T) {
    // Test multiple goroutines creating tenants
    // Test read/write race conditions
}
```

**Estimated Tests:** 4 repositories × 2 concurrency tests = **8 tests**

**Phase 1 Total:** ~28 tests

### Phase 2: HTTP Handlers (Priority: HIGH)

**Target: 70% coverage for HTTP layer**

Create test files:

```bash
touch internal/interfaces/http/tenant_handler_test.go
touch internal/interfaces/http/policy_handler_test.go
```

**Test Coverage:**

```go
// tenant_handler_test.go
package http

import (
    "bytes"
    "encoding/json"
    "net/http/httptest"
    "testing"
)

func TestTenantHandler_CreateTenant(t *testing.T) {
    // Test successful creation
    // Test invalid JSON
    // Test validation errors
    // Test repository errors
}

func TestTenantHandler_GetTenant(t *testing.T) {
    // Test successful retrieval
    // Test not found
    // Test invalid ID
}

func TestTenantHandler_UpdateTenant(t *testing.T) {
    // Test successful update
    // Test not found
    // Test validation errors
}

func TestTenantHandler_DeleteTenant(t *testing.T) {
    // Test successful deletion
    // Test not found
}

func TestTenantHandler_ListTenants(t *testing.T) {
    // Test listing tenants
    // Test empty list
}
```

**Estimated Tests:** 5 endpoints × 2 handlers × ~4 cases = **40 tests**

### Phase 3: Domain Entities (Priority: MEDIUM)

**Target: 80% coverage (currently 52.9%)**

Enhance existing tests:

```bash
# Existing files:
# internal/domain/entities/tenant_test.go
# internal/domain/entities/user_test.go
# internal/domain/entities/policy_test.go
```

**Add Missing Test Cases:**

```go
// Missing edge cases to add:

func TestTenant_ValidationRules(t *testing.T) {
    // Test empty name
    // Test invalid status transitions
    // Test quota validations
}

func TestUser_PasswordHashing(t *testing.T) {
    // Test password comparison
    // Test password strength validation
}

func TestPolicy_ComplexRules(t *testing.T) {
    // Test nested conditions
    // Test resource matching patterns
    // Test action combinations
}
```

**Estimated Additional Tests:** ~15 tests

### Phase 4: DTOs (Priority: LOW)

**Target: 60% coverage**

Create test files:

```bash
touch internal/application/dto/tenant_dto_test.go
touch internal/application/dto/user_dto_test.go
touch internal/application/dto/policy_dto_test.go
```

**Test Coverage:**

```go
// tenant_dto_test.go
package dto

import "testing"

func TestTenantDTO_Validation(t *testing.T) {
    // Test required fields
    // Test field constraints
}

func TestTenantDTO_ToDomain(t *testing.T) {
    // Test conversion to domain entity
    // Test invalid data handling
}

func TestTenantDTO_FromDomain(t *testing.T) {
    // Test conversion from domain entity
}
```

**Estimated Tests:** 3 DTOs × 3 test cases = **9 tests**

### Phase 5: Root Package (Priority: MEDIUM)

Create tests for:

```bash
touch metrics_test.go
touch middleware_test.go
touch tracing_test.go
```

**Estimated Tests:** ~15 tests

## 📈 Implementation Plan

### Week 1: Infrastructure Tests (Priority 1)
- [ ] Create memory_tenant_repository_test.go (5 tests)
- [ ] Create memory_user_repository_test.go (5 tests)
- [ ] Create memory_policy_repository_test.go (5 tests)
- [ ] Create memory_audit_repository_test.go (5 tests)
- [ ] Add concurrency tests (8 tests)
- **Target:** 28 tests, 70% infrastructure coverage

### Week 2: HTTP Handler Tests (Priority 1)
- [ ] Create tenant_handler_test.go (20 tests)
- [ ] Create policy_handler_test.go (20 tests)
- **Target:** 40 tests, 70% HTTP coverage

### Week 3: Domain Enhancement (Priority 2)
- [ ] Add missing tenant entity tests (5 tests)
- [ ] Add missing user entity tests (5 tests)
- [ ] Add missing policy entity tests (5 tests)
- **Target:** 15 tests, 80% domain coverage

### Week 4: DTOs and Root Package (Priority 3)
- [ ] Create DTO tests (9 tests)
- [ ] Create metrics_test.go (5 tests)
- [ ] Create middleware_test.go (5 tests)
- [ ] Create tracing_test.go (5 tests)
- **Target:** 24 tests, 60% root package coverage

## 🎯 Expected Outcomes

| Metric | Current | After Phase 1-2 | After All Phases |
|--------|---------|-----------------|-------------------|
| Overall Coverage | 23.1% | ~55% | ~70% |
| Infrastructure | 0% | 70% | 70% |
| HTTP Handlers | 0% | 70% | 70% |
| Domain Entities | 52.9% | 52.9% | 80% |
| Use Cases | 82.1% | 82.1% | 85% |
| Total Tests | ~10 | ~78 | ~117 |

## 🔍 Testing Best Practices

### Table-Driven Tests

```go
func TestTenantValidation(t *testing.T) {
    tests := []struct {
        name    string
        tenant  *Tenant
        wantErr bool
    }{
        {
            name:    "valid tenant",
            tenant:  &Tenant{ID: "t1", Name: "Test"},
            wantErr: false,
        },
        {
            name:    "empty name",
            tenant:  &Tenant{ID: "t1", Name: ""},
            wantErr: true,
        },
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            err := tt.tenant.Validate()
            if (err != nil) != tt.wantErr {
                t.Errorf("Validate() error = %v, wantErr %v", err, tt.wantErr)
            }
        })
    }
}
```

### Test Helpers

```go
// test_helpers.go
package testutil

func NewTestTenant(t *testing.T) *entities.Tenant {
    t.Helper()
    tenant, err := entities.NewTenant("test-id", "Test Tenant")
    if err != nil {
        t.Fatalf("failed to create test tenant: %v", err)
    }
    return tenant
}
```

### Mock Interfaces

```go
type MockTenantRepository struct {
    CreateFunc func(*entities.Tenant) error
    FindFunc   func(string) (*entities.Tenant, error)
}

func (m *MockTenantRepository) Create(t *entities.Tenant) error {
    if m.CreateFunc != nil {
        return m.CreateFunc(t)
    }
    return nil
}
```

## 📊 Monitoring Progress

Run coverage after each phase:

```bash
# Full coverage report
go test -coverprofile=coverage.out ./...

# View in browser
go tool cover -html=coverage.out

# Coverage by package
go test -cover ./...

# Detailed coverage
go test -coverprofile=coverage.out ./... && go tool cover -func=coverage.out
```

## 🚀 Quick Start

```bash
# Run all existing tests
go test -v ./...

# Run with coverage
go test -cover ./...

# Run specific package
go test -v ./internal/infrastructure/persistence/...

# Run with race detector
go test -race ./...
```

## ✅ Definition of Done

For each phase, consider it complete when:

1. ✅ All tests pass consistently
2. ✅ Coverage target met for the layer
3. ✅ No race conditions detected (`go test -race`)
4. ✅ Tests follow table-driven pattern where appropriate
5. ✅ Error cases are covered
6. ✅ Edge cases are tested
7. ✅ Documentation updated

---

**Next Steps:**
1. Start with Phase 1 (Infrastructure) - Highest impact
2. Use test coverage as gate for PR merges (require 60%+ coverage)
3. Set up CI/CD to run `go test -cover ./...` on every commit
