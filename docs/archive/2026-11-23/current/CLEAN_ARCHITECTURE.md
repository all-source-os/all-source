---
title: "Clean Architecture Guide"
status: CURRENT
last_updated: 2026-02-02
version: "1.0"
related:
  - "./SOLID_PRINCIPLES.md"
  - "./PERFORMANCE.md"
---

# Clean Architecture Guide

## Overview

This guide documents the Clean Architecture pattern applied to the AllSource event store, covering implementation in Rust, Go, and (planned) Clojure.

## The Four Layers

### Layer 1: Domain (Enterprise Business Rules)
Pure business logic with zero external dependencies.

**Rust Example** (`services/core/src/domain/`):
```rust
// domain/entities/event.rs
pub struct Event {
    pub id: Uuid,
    pub event_type: String,
    pub entity_id: String,
    pub tenant_id: String,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
    pub version: i64,
}

impl Event {
    pub fn new(event_type: String, entity_id: String, payload: serde_json::Value) -> Self {
        // Direct construction for performance
    }

    pub fn new_validated(...) -> Result<Self> {
        // With validation for use cases
    }

    // Domain behavior
    pub fn belongs_to_tenant(&self, tenant_id: &str) -> bool {
        self.tenant_id == tenant_id
    }
}
```

### Layer 2: Application (Application Business Rules)
Use cases that orchestrate domain logic.

**Rust Example** (`services/core/src/application/`):
```rust
// application/use_cases/ingest_event.rs
pub struct IngestEventUseCase {
    repository: Arc<dyn EventRepository>,
}

impl IngestEventUseCase {
    pub async fn execute(&self, request: IngestEventRequest) -> Result<IngestEventResponse> {
        // Create domain event
        let event = Event::new(request.event_type, request.entity_id, request.payload);

        // Persist via repository
        self.repository.save(&event).await?;

        // Return response
        Ok(IngestEventResponse::from_event(&event))
    }
}
```

### Layer 3: Infrastructure (Interface Adapters)
Concrete implementations of domain abstractions.

**Planned** (`services/core/src/infrastructure/`):
```rust
// infrastructure/persistence/parquet_repository.rs
pub struct ParquetEventRepository {
    storage: Arc<ParquetStorage>,
}

#[async_trait]
impl EventRepository for ParquetEventRepository {
    async fn save(&self, event: &Event) -> Result<()> {
        // Parquet-specific implementation
    }
}
```

### Layer 4: Frameworks & Drivers
External frameworks, databases, web frameworks.

## Dependency Rule

✅ **Dependencies point inward only**
- Domain ← knows nothing
- Application ← depends on Domain
- Infrastructure ← depends on Application & Domain
- Frameworks ← depends on all layers

## Implementation Status

### Rust Core (`services/core`)
- ✅ Domain layer complete
- ✅ Application layer complete
- ⏳ Infrastructure layer (30% - structure created)
- ⏳ Migration of existing code (ongoing)

### Go Control Plane (`services/control-plane`)
- ❌ Domain layer (planned)
- ❌ Application layer (planned)
- ❌ Infrastructure layer (planned)

## Benefits Realized

1. **Testability**: Use cases testable with mock repositories (100% coverage)
2. **Flexibility**: Can swap storage backends without changing business logic
3. **Maintainability**: Clear separation of concerns
4. **Performance**: Public fields in domain for zero-cost access

## Best Practices

### DO:
✅ Keep domain pure (no external dependencies)
✅ Define interfaces in domain, implement in infrastructure
✅ Use DTOs at application boundaries
✅ Test use cases with mocks

### DON'T:
❌ Import infrastructure code in domain
❌ Put business logic in infrastructure
❌ Skip the application layer
❌ Mix layers

## Migration Guide

See [Phase 1.5 TDD Results](../roadmaps/2025-10-22_PHASE_1.5_TDD_RESULTS.md) for detailed migration approach.

---

**Full implementation guide**: See original [CLEAN_ARCHITECTURE.md](../../services/core/docs/architecture/CLEAN_ARCHITECTURE.md) for complete examples in all three languages.
