---
title: "SOLID Principles Guide"
status: CURRENT
last_updated: 2026-02-02
version: "1.0"
related:
  - "./CLEAN_ARCHITECTURE.md"
---

# SOLID Principles Guide

## Overview

This guide documents how SOLID principles are applied in the AllSource event store.

## Single Responsibility Principle (SRP)

✅ **Applied**: Each module has one reason to change

**Example**:
```rust
// ✅ GOOD: Single responsibility
pub struct Event { ... }  // Only represents an event
pub struct IngestEventUseCase { ... }  // Only ingests events
pub struct EventRepository { ... }  // Only persists events

// ❌ BAD: Multiple responsibilities
pub struct EventManager {
    fn save(&self) { ... }      // Persistence
    fn validate(&self) { ... }  // Validation
    fn send_webhook(&self) { ... }  // Notification
}
```

## Open/Closed Principle (OCP)

✅ **Applied**: Open for extension via traits

**Example**:
```rust
// Open for extension
pub trait EventRepository: Send + Sync {
    async fn save(&self, event: &Event) -> Result<()>;
}

// Different implementations without modifying interface
pub struct ParquetRepository { ... }
pub struct S3Repository { ... }
pub struct MemoryRepository { ... }
```

## Liskov Substitution Principle (LSP)

✅ **Applied**: Subtypes are substitutable

**Example**:
```rust
// Proper hierarchy
pub trait EventReader: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Event>>;
}

pub trait EventWriter: Send + Sync {
    async fn save(&self, event: &Event) -> Result<()>;
}

// Full repository can substitute either
pub trait EventRepository: EventReader + EventWriter { }
```

## Interface Segregation Principle (ISP)

✅ **Applied**: Clients depend only on methods they use

**Example**:
```rust
// ✅ GOOD: Segregated interfaces
pub trait EventReader { ... }   // For queries
pub trait EventWriter { ... }   // For ingestion

// ❌ BAD: Fat interface
pub trait EventStore {
    async fn save(&self) { ... }
    async fn query(&self) { ... }
    async fn backup(&self) { ... }
    async fn restore(&self) { ... }
    // ... 20 more methods
}
```

## Dependency Inversion Principle (DIP)

✅ **Applied**: Depend on abstractions, not concretions

**Example**:
```rust
// ✅ GOOD: Depends on abstraction
pub struct IngestEventUseCase {
    repository: Arc<dyn EventRepository>,  // Abstraction
}

// ❌ BAD: Depends on concrete type
pub struct IngestEventUseCase {
    repository: Arc<ParquetRepository>,  // Concrete
}
```

## Implementation Status

### Rust Core
- ✅ SRP: Domain entities, use cases, repositories separated
- ✅ OCP: Repository trait with multiple implementations (planned)
- ✅ LSP: EventReader/EventWriter hierarchy
- ✅ ISP: Segregated read/write interfaces
- ✅ DIP: Use cases depend on repository abstraction

### Go Control Plane
- ⏳ Planned for Phase 1.5 (weeks 6-8)

## Refactoring Checklist

When refactoring code to apply SOLID:

- [ ] Identify classes with multiple responsibilities (SRP violation)
- [ ] Extract interfaces from concrete implementations (DIP)
- [ ] Split fat interfaces into role interfaces (ISP)
- [ ] Ensure subtypes can replace base types (LSP)
- [ ] Design for extension without modification (OCP)

---

**See also**: [Phase 1.5 TDD Results](../roadmaps/2025-10-22_PHASE_1.5_TDD_RESULTS.md) for practical application examples.
