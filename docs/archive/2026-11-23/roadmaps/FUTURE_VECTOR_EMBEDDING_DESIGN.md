---
title: "Technical Design Document: Vector Embedding Support"
status: CURRENT
last_updated: 2026-02-02
category: roadmap
---

# Technical Design Document: Vector Embedding Support

**Status**: ⏳ FUTURE ROADMAP (RESEARCH PHASE)
**Created**: 2025-11-04
**Author**: AllSource Engineering Team
**Version**: 1.0
**Timeline**: Phase 2.0+ (12+ months out)

---

## 🔮 Future Vision Statement

This document represents a **future research and development roadmap** for adding vector embedding support to AllSource, enabling AI-native semantic search capabilities. This is **NOT** a current implementation plan but rather a strategic exploration of how AllSource could evolve to support AI/ML workloads while maintaining its core event sourcing strengths.

**Purpose**:
- Document strategic direction for AI-first capabilities
- Analyze technical feasibility and trade-offs
- Learn from existing solutions (LanceDB, Pinecone, etc.)
- Identify architectural challenges and opportunities
- Inform future product decisions

**Current Priority**: This feature is **not** on the immediate roadmap. Current focus remains on:
- Phase 1.5: Clean Architecture refactoring
- v1.2: Performance optimization (1M+ events/sec target)
- Core event sourcing capabilities maturation

---

## Executive Summary

This document describes a **potential future design** for adding vector embedding support to AllSource, which would enable AI-native semantic search capabilities while maintaining backward compatibility with existing event sourcing functionality. If implemented, this would allow events to optionally include vector embeddings, support semantic similarity search, and integrate seamlessly with AI/ML frameworks like LangChain and LlamaIndex.

**Key Features**:
- Optional vector embeddings on events (backward compatible)
- Semantic search API with cosine similarity
- Automatic embedding generation via configurable providers
- User-provided embeddings support
- Vector index management (HNSW algorithm)
- Integration with AI frameworks
- Multi-tenant isolation for embeddings

**Performance Targets**:
- Embedding generation: <100ms for text (up to 8K tokens)
- Semantic search: <50ms for 1M vectors (p95)
- Index build time: <5min for 10M vectors
- Storage overhead: ~4KB per embedding (1536 dimensions)

---

## Table of Contents

**PART A: RESEARCH & ANALYSIS**
1. [Background & Motivation](#1-background--motivation)
2. [LanceDB Technical Deep Dive](#2-lancedb-technical-deep-dive)
3. [Key Learnings & Gotchas](#3-key-learnings--gotchas)
4. [Parquet vs Lance Format Analysis](#4-parquet-vs-lance-format-analysis)

**PART B: PROPOSED DESIGN (IF IMPLEMENTED)**
5. [Goals & Non-Goals](#5-goals--non-goals)
6. [Architecture Overview](#6-architecture-overview)
7. [Data Model Changes](#7-data-model-changes)
8. [API Design](#8-api-design)
9. [Embedding Providers](#9-embedding-providers)
10. [Vector Index Management](#10-vector-index-management)
11. [Storage Strategy](#11-storage-strategy)
12. [Query Processing](#12-query-processing)
13. [Security & Multi-Tenancy](#13-security--multi-tenancy)
14. [Performance Optimization](#14-performance-optimization)
15. [Migration Strategy](#15-migration-strategy)
16. [Testing Strategy](#16-testing-strategy)
17. [Deployment & Operations](#17-deployment--operations)

**PART C: STRATEGIC CONSIDERATIONS**
18. [Build vs. Integrate Decision](#18-build-vs-integrate-decision)
19. [Future Work](#19-future-work)
20. [Appendices](#20-appendices)

---

## 1. Background & Motivation

### 1.1 Current State

AllSource provides high-performance event sourcing with:
- 469K events/sec ingestion
- Time-travel queries by entity/type/time
- Multi-tenant isolation
- Parquet columnar storage

**Limitations**:
- No semantic search (only exact match filters)
- Cannot find "similar" events
- No AI/ML framework integration
- Limited support for unstructured text analysis

### 1.2 Use Cases Enabled by Vector Embeddings

1. **Semantic Event Search**: "Find events similar to this fraud pattern"
2. **RAG for Event History**: LLM-powered queries over historical events
3. **Anomaly Detection**: Identify unusual events via embedding distance
4. **Event Clustering**: Group similar events for pattern analysis
5. **Cross-Entity Correlation**: Find related events across different entities
6. **Natural Language Queries**: Search events by description rather than exact filters

### 1.3 Competitive Analysis

| Feature | AllSource (Current) | AllSource (Proposed) | LanceDB | Pinecone | Weaviate |
|---------|-------------------|-------------------|---------|----------|----------|
| Event Sourcing | ✅ | ✅ | ❌ | ❌ | ❌ |
| Vector Search | ❌ | ✅ | ✅ | ✅ | ✅ |
| Time-Travel | ✅ | ✅ | ⚠️ | ❌ | ❌ |
| Temporal + Semantic | ❌ | ✅ | ❌ | ❌ | ❌ |
| Multi-Tenancy | ✅ | ✅ | ⚠️ | ✅ | ✅ |

**Unique Position**: Only platform combining event sourcing + vector search + time-travel.

---

## 2. LanceDB Technical Deep Dive

### 2.1 Lance Data Format Architecture

LanceDB is built on the **Lance columnar format**, a modern alternative to Parquet specifically designed for AI/ML workloads. Understanding their architecture provides valuable insights for AllSource.

#### 2.1.1 File Structure & Organization

**Physical Layout**:
```
Lance File Structure:
├── Data Pages (64-byte aligned)
│   ├── Column 1 pages (8MB default)
│   ├── Column 2 pages
│   └── Column N pages
├── Column Metadata (independent protobuf blocks)
│   ├── Encoding specifications
│   ├── Page locations (buffer_offsets)
│   └── Page sizes (buffer_sizes)
├── Metadata Offset Table
├── Global Buffers Offset Table
└── Footer (32 bytes, version + pointers)
```

**Key Architectural Decisions**:
- **No Row Groups**: Unlike Parquet, Lance eliminates row groups entirely. This prevents "runt pages" (too-small groups) and excessive RAM buffering (oversized groups).
- **Independent Columns**: Each column's metadata is in a completely independent block, enabling true selective column access without loading entire schema.
- **8MB Page Size**: Optimized for cloud storage (S3, GCS) where larger pages justify dedicated I/O operations.
- **64-byte Alignment**: Enables SIMD operations; 4096-byte alignment for direct I/O compatibility.

#### 2.1.2 Fragment-Based Data Management

**Fragment Architecture**:
- Data organizes into **fragments** (logical chunks containing multiple columns)
- Each fragment = subset of dataset with its own files
- Fragment is the atomic unit of data storage
- Enables efficient append operations and versioning

**Critical Gotcha** 🚨:
> "Keep the number of fragments under 100, which is suitable for most use cases."

**Why it matters**: Too many fragments = metadata overhead + slower queries. Single-row inserts create excessive fragmentation.

#### 2.1.3 Versioning Mechanism

**Manifest-Based Snapshots**:
- Every insert/update creates new dataset version with updated metadata
- **Not 100 full copies**: "If you have 100 versions, they aren't 100 duplicates of the same data"
- Metadata overhead increases proportionally with versions
- Deleted rows marked but not removed (enables recovery within backup policy)

**Implication for AllSource**: Event immutability aligns well with versioning approach, but we need metadata compaction strategy.

#### 2.1.4 Performance Characteristics

**Benchmarked Claims**:
- **100x faster random access** than Parquet
- **50-100x faster analytics** vs. raw metadata
- **<1ms vector search** (100 queries on 1M 128-dim vectors)
- **No scan performance sacrifice** despite point query optimization

**How They Achieve This**:
1. **Custom encodings** balancing columnar scan + sub-linear point queries
2. **Flexible metadata placement** (dictionaries in column metadata, not duplicated across pages)
3. **Two-thread read architecture** (decouples I/O parallelism from compute)
4. **Page-level independence** (don't need full page loads for individual rows)

### 2.2 Vector Index Implementation

#### 2.2.1 Disk-Based Philosophy

**Critical Design Choice**:
> "LanceDB's indexing philosophy adopts a primarily disk-based indexing philosophy due to the design of Lance."

**Why Disk-Based**:
- Lance format optimized for persistent storage, not in-memory
- Enables billion-scale vectors without RAM constraints
- Cloud-native (works well with S3/GCS)

**Trade-off**: Slightly slower than pure in-memory (Pinecone, Qdrant) but scales better.

#### 2.2.2 IVF-PQ Index

**Architecture**:
```
IVF-PQ = Inverted File (IVF) + Product Quantization (PQ)

Product Quantization:
- Divide vector into equally-sized subvectors
- Map each subvector to nearest centroid
- 128x memory reduction (from f32 to uint8 codebooks)

Inverted File:
- Partition space into Voronoi cells via K-means
- Each cell = list of vectors near centroid
- Search restricted to subset of partitions
```

**Configuration Parameters**:
- `num_partitions`: Usually target specific vectors/partition ratio
- `num_sub_vectors`: Based on desired recall vs dimensionality
- `nprobes`: Typically 5-10% of dataset for high recall

**Performance**:
- 128x memory reduction per vector
- <1ms query latency at scale
- Trade-off: Quantization introduces small accuracy loss

#### 2.2.3 HNSW Implementation

**Graph-Based Approach**:
- k-Nearest Neighbor graph with hierarchical layers (inspired by skip lists)
- Each vector = vertex with edges to k nearest neighbors

**LanceDB's Hybrid**: IVF_HNSW_PQ
- Doesn't build single HNSW over entire dataset
- **Builds sub-HNSW indices within each IVF partition**
- Best of both worlds: IVF coarse filtering + HNSW fine-grained search

**Configuration**:
- `ef_construction`: Candidates evaluated during graph construction (higher = better accuracy, slower build)
- Default: Adaptive based on dataset size

### 2.3 Storage & Maintenance Practices

#### 2.3.1 Data Compaction Strategy

**When to Compact**:
- Fragments > 100
- After many small inserts
- Post-reprocessing operations
- Regular maintenance schedule

**What Compaction Does**:
1. Merges fragments
2. Removes deleted rows physically
3. Eliminates dropped columns
4. Optimizes page layout
5. Rebuilds statistics

**Implication for AllSource**: We already have Parquet compaction. Need similar strategy for vector indexes.

#### 2.3.2 Batch Operations

**Critical Performance Pattern**:
> "Utilize batch processing techniques for data inserts. Inserting records individually can lead to fragmented data on disk and slower performance."

**Best Practices**:
- Batch sizes: 1000-10000 rows
- Avoid single-row inserts
- Use `Table.add()` with arrays/dataframes
- Combine related operations

**AllSource Advantage**: We already batch events (469K/sec throughput). Natural fit for vector embeddings.

#### 2.3.3 Index Management

**Timing Gotcha** 🚨:
> "Queries executed immediately after create_fts_index or create_scalar_index calls may fail if the background indexing process hasn't completed."

**Solution**: Wait for index build confirmation before querying.

**Index Types**:
- BTree (range queries)
- Bitmap (categorical data)
- Full-text search
- Vector indices (IVF-PQ, HNSW)

### 2.4 Production Deployment Insights

#### 2.4.1 Memory Management Issues

**Real-World Problem** (from 700M vector deployment):
> "Memory leaks quickly became apparent when running under Uvicorn as an API. Connections & tables started piling up."

**Solution**: Connection manager with explicit cleanup:
```python
# Anti-pattern
def query():
    db = lancedb.connect()
    table = db.open_table()
    return table.search()  # Leaks!

# Best practice
connection_manager = SingletonConnection()
table = connection_manager.open_table_once()
# Reuse table for all queries
```

**Implication for AllSource**: Need careful resource management in Rust (we have advantage with RAII).

#### 2.4.2 Storage Considerations

**Disk Space Gotcha** 🚨:
> "During IvfPq index creation, LanceDB temporarily stores intermediate data in /tmp directory, which may not have enough disk space for gigabytes of data."

**Solutions**:
- Configure temp directory with sufficient space
- Monitor disk usage during index builds
- Use SSD (strongly recommended over HDD)

**S3 Multipart Uploads** 🚨:
> "In the event of a graceful shutdown or crash, some multipart uploads may remain incomplete."

**Mitigation**: Lifecycle rule to delete in-progress uploads after 7 days.

#### 2.4.3 Query Performance

**Latency Targets**:
- S3 storage: QPS limited by concurrency
- EFS storage: IOPs bottleneck
- p95 latency: <100ms achievable
- Network distance matters (run in same region)

**Optimization Techniques**:
1. **Pre-warming**: Send warm-up queries before user traffic
2. **Scalar indices**: Create BITMAP/BTREE on filter columns
3. **fast_search parameter**: Ignore un-indexed data for speed
4. **Batch queries**: Reduce S3 request count
5. **Connection reuse**: Avoid connection establishment overhead

### 2.5 Lance v2 Evolution

#### 2.5.1 Plugin-Based Encodings

**Major Architectural Shift**:
- Format itself has **no type system or built-in encodings**
- Everything handled through extensible plugins
- Protobuf "any" messages for encoding specs
- New encodings = new .proto files (no format changes)

**Advantage**: Addresses Parquet fragmentation (varying encoding support across implementations).

**Trade-off**: Early implementation stage, advanced encodings incomplete.

#### 2.5.2 I/O and Compute Decoupling

**Two-Thread Architecture**:
- **I/O thread**: Fetches remaining pages
- **Compute thread**: Decodes already-arrived data in parallel
- Prevents compute waiting for all I/O to complete

**Performance Impact**: Better CPU utilization, lower latency for large queries.

#### 2.5.3 Wide Schema Support

**Capability**: Files with **millions of columns** feasible.

**How**: Single-column metadata readable independently (don't load entire schema).

**Use Case**: Multimodal data with diverse metadata fields per event.

---

## 3. Key Learnings & Gotchas

### 3.1 Critical Production Gotchas

#### 3.1.1 Memory Management

| Issue | Impact | Mitigation |
|-------|--------|------------|
| Connection leaks | OOM crashes | Singleton connection manager |
| Table handle accumulation | RAM growth | Reuse table handles |
| Unclosed resources | File descriptor exhaustion | Explicit cleanup (Rust RAII helps) |

#### 3.1.2 Performance Pitfalls

| Anti-Pattern | Impact | Best Practice |
|--------------|--------|---------------|
| Single-row inserts | Fragment explosion | Batch 1K-10K rows |
| No index pre-warming | Cold start latency | Send warm-up queries |
| Querying during index build | Failures/degraded perf | Wait for completion |
| Cross-region queries | High latency | Co-locate compute/storage |
| Missing scalar indices | Full scans on filters | Index filter columns |

#### 3.1.3 Storage & Disk Issues

| Problem | Cause | Solution |
|---------|-------|----------|
| Temp directory full | Index build intermediate data | Configure large /tmp or custom temp path |
| S3 orphaned uploads | Crashes during multipart upload | Lifecycle rules (7-day cleanup) |
| Fragment explosion | Small writes | Compact regularly (keep <100 fragments) |
| Slow queries on HDD | Disk-based architecture | Use SSD (strongly recommended) |

### 3.2 Operational Best Practices

#### 3.2.1 Indexing Strategy

**Do**:
- ✅ Create scalar indices on metadata filter columns
- ✅ Use IVF-PQ for billion-scale datasets
- ✅ Set `nprobes = 5-10%` of dataset for good recall
- ✅ Wait for index build before querying
- ✅ Monitor index build progress

**Don't**:
- ❌ Create too many indices (maintenance overhead)
- ❌ Query immediately after `create_index()`
- ❌ Skip index creation for filtered queries
- ❌ Use HNSW alone for billion+ scale (use hybrid)

#### 3.2.2 Data Management

**Batch Operations**:
```python
# Good
events = [event1, event2, ..., event1000]
table.add(events)  # Single batch

# Bad
for event in events:
    table.add([event])  # 1000 fragments!
```

**Compaction Schedule**:
- Monitor fragment count: `table.count_fragments()`
- Compact when >100 fragments
- Run during low-traffic periods
- Expect temporary latency spike

**Version Management**:
- Set retention policy (don't keep all versions)
- Older versions = read-only overhead
- Clean up old versions regularly

### 3.3 Trade-offs: Lance vs Parquet

| Dimension | Parquet | Lance | Winner for AllSource? |
|-----------|---------|-------|---------------------|
| **Random Access** | Slow (row groups) | 100x faster | Lance (if needed) |
| **Scan Performance** | Excellent | Equal | Tie |
| **Vector Storage** | No native support | Native | Lance |
| **Maturity** | Very mature (10+ years) | Young (2-3 years) | Parquet |
| **Ecosystem** | Massive (Spark, etc.) | Growing | Parquet |
| **Event Sourcing** | Proven | Unproven | Parquet |
| **Versioning** | Manual | Built-in | Lance |
| **Compression** | Excellent | Good | Parquet |
| **Complexity** | Well-understood | New concepts | Parquet |

**Verdict for AllSource**:
- **Near-term**: Stick with Parquet (mature, proven, we know it well)
- **Long-term**: Explore Lance for vector-heavy workloads
- **Hybrid**: Use both (Parquet for events, Lance for embeddings)

---

## 4. Parquet vs Lance Format Analysis

### 4.1 Technical Comparison

#### 4.1.1 File Structure

**Parquet**:
```
Row Group 1 (128MB default)
├── Column 1 chunk
├── Column 2 chunk
└── Column N chunk
Row Group 2
└── ...
Footer (schema + metadata)
```

**Lance**:
```
Pages (column-independent, 8MB default)
├── Column 1 pages
├── Column 2 pages
└── Column N pages
Column Metadata (independent blocks)
Footer (pointers)
```

**Key Difference**: Lance pages are independent; Parquet columns grouped by rows.

#### 4.1.2 Random Access Performance

**Parquet Limitation**:
- Must read entire row group to access single row
- 128MB row group = 128MB read for 1 row
- Mitigated by smaller row groups (but more metadata)

**Lance Solution**:
- Pages independently addressable
- Flexible metadata (skip tables, dictionaries)
- Partial page reads minimize amplification

**Real-World Impact**:
- Parquet: Good for scans, poor for point lookups
- Lance: Good for both scans AND point lookups

#### 4.1.3 Vector Storage Efficiency

**Parquet Approach** (if we use it):
```rust
// Store vectors as LIST<FLOAT32>
embedding_vector: List<Float32>  // Works, but not optimized
```

**Pros**:
- Works today
- No format changes needed
- Compatible with all Parquet readers

**Cons**:
- Not optimized for similarity search
- No built-in indexing
- Slow for vector operations

**Lance Approach**:
```
// Native fixed-size-list type optimized for vectors
embedding_vector: FixedSizeList<Float32, 1536>
```

**Pros**:
- Native vector support
- Optimized encodings
- Built-in indexing integration

**Cons**:
- New format to learn
- Smaller ecosystem
- Migration complexity

### 4.2 Decision Framework for AllSource

#### 4.2.1 Hybrid Storage Strategy (Recommended)

**Approach**: Use both formats for their strengths.

```
AllSource Storage Architecture:
├── Events (Parquet)
│   ├── event_id, event_type, entity_id
│   ├── payload, timestamp, metadata
│   └── embedding_ref → points to Lance
└── Embeddings (Lance) [OPTIONAL]
    ├── event_id (foreign key)
    ├── embedding_vector (1536 dims)
    └── embedding_metadata
```

**Advantages**:
- ✅ Keep Parquet for core event sourcing (proven)
- ✅ Use Lance for vector-heavy queries
- ✅ Gradual adoption (Parquet-first, Lance optional)
- ✅ Best of both worlds

**Trade-offs**:
- Two storage systems to maintain
- Join overhead (event + embedding lookup)
- Added complexity

#### 4.2.2 Pure Parquet Strategy (Conservative)

**Approach**: Stick with Parquet for everything.

```rust
Event {
    // ... existing fields
    embedding: Option<Vec<f32>>,  // Stored as LIST<FLOAT32>
}
```

**Advantages**:
- ✅ Single storage system
- ✅ Mature and proven
- ✅ No migration needed
- ✅ Simpler architecture

**Disadvantages**:
- ❌ Slower vector queries
- ❌ No native vector indexing
- ❌ Higher storage overhead

**Mitigation**: Build separate vector index files (HNSW) that reference Parquet rows.

#### 4.2.3 Pure Lance Migration (Aggressive)

**Approach**: Migrate entire event store to Lance format.

**Advantages**:
- ✅ Unified format
- ✅ Optimal for vector + event queries
- ✅ Future-proof

**Disadvantages**:
- ❌ Major migration effort
- ❌ Unproven for event sourcing
- ❌ Ecosystem immature
- ❌ Risk of unknown issues

**Recommendation**: **Not advised** for near-term. Too risky.

### 4.3 Our Recommendation

**Phase 1 (Current)**: Pure Parquet
- Continue using Parquet for all events
- Add `embedding: Option<Vec<f32>>` column
- Build separate HNSW index files (in-memory or disk)
- **Timeline**: Now - 12 months

**Phase 2 (Exploration)**: Hybrid Parquet + Lance
- Parquet for events
- Lance for embeddings (if vector queries dominant)
- Benchmark performance delta
- **Timeline**: 12-24 months

**Phase 3 (Future)**: Evaluate full Lance migration
- Based on Phase 2 learnings
- If Lance ecosystem matures
- If vector queries become primary use case
- **Timeline**: 24+ months

---

## 5. Goals & Non-Goals

### 2.1 Goals

**Primary**:
- ✅ Add optional vector embeddings to events
- ✅ Implement semantic search API (cosine similarity)
- ✅ Support both user-provided and auto-generated embeddings
- ✅ Maintain backward compatibility (no breaking changes)
- ✅ Achieve <50ms search latency for 1M vectors

**Secondary**:
- ✅ Integrate with OpenAI, Cohere, and local embedding models
- ✅ Provide Python/Node SDKs with LangChain/LlamaIndex integration
- ✅ Support hybrid queries (temporal + semantic filters)
- ✅ Enable multi-tenant vector isolation

### 2.2 Non-Goals

**Out of Scope for v1**:
- ❌ Multimodal embeddings (images, audio, video) - Phase 2
- ❌ Fine-tuning custom embedding models
- ❌ Distributed vector indexes across nodes
- ❌ GPU acceleration - Phase 3
- ❌ Real-time embedding updates on event mutation (events are immutable)

### 2.3 Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Adoption Rate | 30% of new events use embeddings | Telemetry |
| Search Latency (p95) | <50ms for 1M vectors | Benchmarks |
| Embedding Generation | <100ms per event | Observability |
| Storage Overhead | <20% increase | Storage metrics |
| API Uptime | 99.9% | Monitoring |

---

## 3. Architecture Overview

### 3.1 High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     AllSource Event Store                      │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌─────────────────┐         ┌──────────────────┐          │
│  │  Ingest Layer   │────────▶│ Embedding Engine │          │
│  │                 │         │                  │          │
│  │ • Validation    │         │ • Provider Mgr   │          │
│  │ • Enrichment    │         │ • Auto-generate  │          │
│  │ • Batching      │         │ • Validation     │          │
│  └────────┬────────┘         └────────┬─────────┘          │
│           │                           │                     │
│           ▼                           ▼                     │
│  ┌─────────────────────────────────────────────┐           │
│  │          Storage Layer                      │           │
│  │                                             │           │
│  │  ┌──────────────┐    ┌─────────────────┐  │           │
│  │  │   Parquet    │    │  Vector Index   │  │           │
│  │  │   (Events +  │    │  (HNSW/IVF)     │  │           │
│  │  │   Embeddings)│    │                 │  │           │
│  │  └──────────────┘    └─────────────────┘  │           │
│  └─────────────────────────────────────────────┘           │
│                                                              │
│  ┌──────────────────────────────────────────────┐          │
│  │           Query Layer                        │          │
│  │                                              │          │
│  │  ┌─────────────┐  ┌──────────────────────┐ │          │
│  │  │  Temporal   │  │  Semantic Search     │ │          │
│  │  │  Query      │  │  (Vector Similarity) │ │          │
│  │  │             │  │                      │ │          │
│  │  └─────────────┘  └──────────────────────┘ │          │
│  │                                              │          │
│  │  ┌──────────────────────────────────────┐  │          │
│  │  │     Hybrid Query Engine              │  │          │
│  │  │  (Temporal Filters + Semantic Search)│  │          │
│  │  └──────────────────────────────────────┘  │          │
│  └──────────────────────────────────────────────┘          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
       │                                              ▲
       │                                              │
       ▼                                              │
┌──────────────────┐                        ┌────────────────┐
│  Embedding       │                        │   AI/ML SDKs   │
│  Providers       │                        │                │
│                  │                        │ • Python       │
│ • OpenAI         │                        │ • Node.js      │
│ • Cohere         │                        │ • LangChain    │
│ • Local Models   │                        │ • LlamaIndex   │
└──────────────────┘                        └────────────────┘
```

### 3.2 Component Responsibilities

#### 3.2.1 Embedding Engine
- Manages embedding provider configuration
- Generates embeddings from event payloads
- Validates embedding dimensions
- Handles provider failures/retries
- Caches embeddings (optional)

#### 3.2.2 Vector Index Manager
- Builds and maintains HNSW indexes
- Handles index persistence to disk
- Supports index rebuilding
- Manages index per tenant (isolation)
- Periodic index optimization

#### 3.2.3 Semantic Search Engine
- Executes k-NN similarity queries
- Combines temporal + semantic filters
- Supports metadata filtering
- Returns ranked results with scores

---

## 4. Data Model Changes

### 4.1 Event Entity (Rust)

**Current**:
```rust
pub struct Event {
    pub id: Uuid,
    pub event_type: EventType,
    pub entity_id: EntityId,
    pub tenant_id: TenantId,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
    pub version: i64,
}
```

**Proposed** (backward compatible):
```rust
pub struct Event {
    pub id: Uuid,
    pub event_type: EventType,
    pub entity_id: EntityId,
    pub tenant_id: TenantId,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
    pub version: i64,

    // ✨ NEW FIELDS
    pub embedding: Option<Embedding>,
    pub embedding_metadata: Option<EmbeddingMetadata>,
}

/// Vector embedding representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// The embedding vector (typically 1536 dimensions for OpenAI)
    pub vector: Vec<f32>,

    /// Checksum for integrity verification
    pub checksum: Option<String>,
}

/// Metadata about the embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingMetadata {
    /// Model used to generate embedding (e.g., "text-embedding-3-large")
    pub model: String,

    /// Provider name (e.g., "openai", "cohere", "local")
    pub provider: String,

    /// Embedding dimensions
    pub dimensions: usize,

    /// Source fields used to generate embedding
    pub source_fields: Vec<String>,

    /// Timestamp when embedding was generated
    pub generated_at: DateTime<Utc>,

    /// Whether embedding was user-provided or auto-generated
    pub source: EmbeddingSource,

    /// Optional: tokens used (for cost tracking)
    pub token_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmbeddingSource {
    UserProvided,
    AutoGenerated,
    Reprocessed, // Generated during reprocessing/migration
}
```

### 4.2 Parquet Schema Changes

**Column Additions**:
```
event_id: UUID (existing)
event_type: String (existing)
entity_id: String (existing)
tenant_id: String (existing)
payload: JSON (existing)
timestamp: Timestamp (existing)
version: Int64 (existing)

// NEW COLUMNS
embedding_vector: List<Float32>  // The actual vector (nullable)
embedding_model: String          // Model name (nullable)
embedding_provider: String       // Provider name (nullable)
embedding_dimensions: Int32      // Vector dimensions (nullable)
embedding_source: String         // "user" | "auto" | "reprocessed" (nullable)
embedding_generated_at: Timestamp // When generated (nullable)
```

**Backward Compatibility**: All new columns are nullable, so existing Parquet files remain valid.

### 4.3 Vector Index Structure

```rust
/// HNSW (Hierarchical Navigable Small World) Index
pub struct VectorIndex {
    /// Tenant this index belongs to
    pub tenant_id: TenantId,

    /// Index parameters
    pub config: IndexConfig,

    /// Underlying HNSW implementation
    index: HnswIndex, // from hnsw_rs crate

    /// Mapping from index ID to event ID
    id_mapping: HashMap<u64, Uuid>,

    /// Reverse mapping
    event_to_index: HashMap<Uuid, u64>,

    /// Index statistics
    pub stats: IndexStats,

    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    /// HNSW M parameter (number of bi-directional links)
    pub m: usize, // default: 16

    /// HNSW ef_construction parameter (size of dynamic candidate list)
    pub ef_construction: usize, // default: 200

    /// HNSW ef_search parameter (size of dynamic candidate list during search)
    pub ef_search: usize, // default: 50

    /// Vector dimensions (must match embeddings)
    pub dimensions: usize,

    /// Distance metric
    pub metric: DistanceMetric,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DistanceMetric {
    Cosine,      // For normalized vectors
    L2,          // Euclidean distance
    DotProduct,  // Inner product
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub total_vectors: usize,
    pub memory_bytes: usize,
    pub build_time_ms: u64,
    pub last_rebuild: DateTime<Utc>,
}
```

### 4.4 Database Migrations

**Migration Plan**:
1. Add new columns to Parquet schema (nullable)
2. Create vector index directory structure
3. No data migration required (backward compatible)
4. Optional: Reprocess existing events to generate embeddings

---

## 5. API Design

### 5.1 Ingestion API Changes

#### 5.1.1 Create Event with Embedding (User-Provided)

**Request**:
```http
POST /api/v1/events
Content-Type: application/json
Authorization: Bearer <jwt>

{
  "event_type": "user.message",
  "entity_id": "user-123",
  "payload": {
    "message": "How do I reset my password?",
    "channel": "support"
  },
  "embedding": {
    "vector": [0.123, -0.456, 0.789, ...], // 1536 dimensions
    "model": "text-embedding-3-large",
    "provider": "openai"
  }
}
```

**Response**:
```json
{
  "event_id": "550e8400-e29b-41d4-a716-446655440000",
  "tenant_id": "tenant-abc",
  "version": 1,
  "timestamp": "2025-11-04T12:00:00Z",
  "embedding_metadata": {
    "model": "text-embedding-3-large",
    "provider": "openai",
    "dimensions": 1536,
    "source": "user_provided",
    "generated_at": "2025-11-04T12:00:00Z"
  }
}
```

#### 5.1.2 Create Event with Auto-Generated Embedding

**Request**:
```http
POST /api/v1/events
Content-Type: application/json
Authorization: Bearer <jwt>

{
  "event_type": "user.message",
  "entity_id": "user-123",
  "payload": {
    "message": "How do I reset my password?",
    "channel": "support"
  },
  "embedding_config": {
    "auto_generate": true,
    "source_fields": ["payload.message"], // Fields to embed
    "model": "text-embedding-3-small",    // Optional override
    "provider": "openai"                   // Optional override
  }
}
```

**Response**: Same as above, with `"source": "auto_generated"`.

#### 5.1.3 Batch Ingestion with Embeddings

**Request**:
```http
POST /api/v1/events/batch
Content-Type: application/json
Authorization: Bearer <jwt>

{
  "events": [
    {
      "event_type": "user.click",
      "entity_id": "user-123",
      "payload": {"button": "checkout"},
      "embedding_config": {"auto_generate": true}
    },
    {
      "event_type": "user.purchase",
      "entity_id": "user-123",
      "payload": {"item": "laptop", "price": 999},
      "embedding": {
        "vector": [...],
        "model": "text-embedding-ada-002"
      }
    }
  ]
}
```

**Response**:
```json
{
  "ingested": 2,
  "failed": 0,
  "event_ids": [
    "550e8400-...",
    "660e8400-..."
  ],
  "errors": []
}
```

### 5.2 Semantic Search API

#### 5.2.1 Search by Query Text (Auto-Embed)

**Request**:
```http
POST /api/v1/events/search/semantic
Content-Type: application/json
Authorization: Bearer <jwt>

{
  "query": "password reset issues",
  "limit": 10,
  "min_score": 0.7,
  "filters": {
    "event_type": "user.message",
    "time_range": {
      "since": "2025-01-01T00:00:00Z",
      "until": "2025-12-31T23:59:59Z"
    }
  },
  "embedding_config": {
    "model": "text-embedding-3-large"
  }
}
```

**Response**:
```json
{
  "results": [
    {
      "event": {
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "event_type": "user.message",
        "entity_id": "user-789",
        "payload": {
          "message": "I forgot my password, can't log in",
          "channel": "support"
        },
        "timestamp": "2025-03-15T14:23:00Z"
      },
      "score": 0.92,
      "distance": 0.08
    },
    {
      "event": {
        "id": "660e8400-...",
        "event_type": "user.message",
        "payload": {
          "message": "Need help resetting account credentials"
        },
        "timestamp": "2025-03-14T09:15:00Z"
      },
      "score": 0.87,
      "distance": 0.13
    }
  ],
  "total": 2,
  "search_time_ms": 23,
  "query_embedding": {
    "model": "text-embedding-3-large",
    "dimensions": 1536
  }
}
```

#### 5.2.2 Search by Vector (Pre-Computed Embedding)

**Request**:
```http
POST /api/v1/events/search/semantic
Content-Type: application/json
Authorization: Bearer <jwt>

{
  "query_vector": [0.123, -0.456, 0.789, ...],
  "limit": 5,
  "min_score": 0.8
}
```

#### 5.2.3 Hybrid Search (Temporal + Semantic)

**Request**:
```http
POST /api/v1/events/search/hybrid
Content-Type: application/json
Authorization: Bearer <jwt>

{
  "semantic": {
    "query": "fraud transaction",
    "weight": 0.7
  },
  "temporal": {
    "entity_id": "account-456",
    "event_type": "transaction.*",
    "time_range": {
      "since": "2025-10-01T00:00:00Z"
    },
    "weight": 0.3
  },
  "limit": 20
}
```

**Response**: Same as semantic search, with combined scoring.

### 5.3 Embedding Management API

#### 5.3.1 Reprocess Events to Generate Embeddings

**Request**:
```http
POST /api/v1/embeddings/reprocess
Content-Type: application/json
Authorization: Bearer <jwt>

{
  "filters": {
    "event_type": "user.message",
    "time_range": {
      "since": "2025-01-01T00:00:00Z"
    }
  },
  "embedding_config": {
    "model": "text-embedding-3-large",
    "source_fields": ["payload.message"]
  },
  "batch_size": 100
}
```

**Response**:
```json
{
  "job_id": "reprocess-550e8400",
  "status": "queued",
  "estimated_events": 15000,
  "estimated_time_minutes": 45
}
```

#### 5.3.2 Get Reprocessing Job Status

**Request**:
```http
GET /api/v1/embeddings/reprocess/reprocess-550e8400
Authorization: Bearer <jwt>
```

**Response**:
```json
{
  "job_id": "reprocess-550e8400",
  "status": "running",
  "progress": {
    "processed": 8500,
    "total": 15000,
    "percentage": 56.67
  },
  "started_at": "2025-11-04T12:00:00Z",
  "estimated_completion": "2025-11-04T12:45:00Z",
  "errors": []
}
```

#### 5.3.3 Rebuild Vector Index

**Request**:
```http
POST /api/v1/embeddings/index/rebuild
Content-Type: application/json
Authorization: Bearer <jwt>

{
  "config": {
    "m": 16,
    "ef_construction": 200,
    "ef_search": 50
  }
}
```

### 5.4 Configuration API

#### 5.4.1 Configure Default Embedding Provider

**Request**:
```http
PUT /api/v1/config/embeddings
Content-Type: application/json
Authorization: Bearer <jwt>

{
  "default_provider": "openai",
  "default_model": "text-embedding-3-large",
  "providers": {
    "openai": {
      "api_key": "sk-...",
      "api_base": "https://api.openai.com/v1"
    },
    "cohere": {
      "api_key": "co-...",
      "model": "embed-multilingual-v3.0"
    },
    "local": {
      "endpoint": "http://localhost:3900/embed",
      "model": "all-MiniLM-L6-v2"
    }
  },
  "auto_generate_default": false,
  "cache_embeddings": true,
  "cache_ttl_hours": 24
}
```

---

## 6. Embedding Providers

### 6.1 Provider Architecture

```rust
/// Trait for embedding providers
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for text
    async fn embed_text(&self, text: &str) -> Result<Embedding, EmbeddingError>;

    /// Generate embeddings for multiple texts (batched)
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Embedding>, EmbeddingError>;

    /// Get model information
    fn model_info(&self) -> ModelInfo;

    /// Provider name
    fn provider_name(&self) -> &str;
}

/// Provider manager
pub struct EmbeddingProviderManager {
    providers: HashMap<String, Box<dyn EmbeddingProvider>>,
    default_provider: String,
    config: EmbeddingConfig,
}
```

### 6.2 Supported Providers

#### 6.2.1 OpenAI

**Models**:
- `text-embedding-3-large` (3072 dims, $0.13/1M tokens)
- `text-embedding-3-small` (1536 dims, $0.02/1M tokens)
- `text-embedding-ada-002` (1536 dims, legacy)

**Implementation**:
```rust
pub struct OpenAIProvider {
    client: reqwest::Client,
    api_key: String,
    api_base: String,
    model: String,
}

#[async_trait]
impl EmbeddingProvider for OpenAIProvider {
    async fn embed_text(&self, text: &str) -> Result<Embedding, EmbeddingError> {
        let request = json!({
            "input": text,
            "model": self.model,
            "encoding_format": "float"
        });

        let response = self.client
            .post(&format!("{}/embeddings", self.api_base))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?
            .json::<OpenAIResponse>()
            .await?;

        Ok(Embedding {
            vector: response.data[0].embedding.clone(),
            checksum: None,
        })
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Embedding>, EmbeddingError> {
        // OpenAI supports up to 2048 texts per batch
        let request = json!({
            "input": texts,
            "model": self.model
        });

        // ... similar to embed_text
    }

    fn model_info(&self) -> ModelInfo {
        ModelInfo {
            name: self.model.clone(),
            dimensions: match self.model.as_str() {
                "text-embedding-3-large" => 3072,
                _ => 1536,
            },
            max_tokens: 8191,
        }
    }

    fn provider_name(&self) -> &str {
        "openai"
    }
}
```

#### 6.2.2 Cohere

**Models**:
- `embed-english-v3.0` (1024 dims)
- `embed-multilingual-v3.0` (1024 dims)

**Implementation**: Similar to OpenAI, using Cohere's `/embed` endpoint.

#### 6.2.3 Local (Ollama, HuggingFace)

**Models**:
- `all-MiniLM-L6-v2` (384 dims, fast)
- `all-mpnet-base-v2` (768 dims, accurate)
- Custom fine-tuned models

**Implementation**:
```rust
pub struct LocalProvider {
    endpoint: String,
    model: String,
    dimensions: usize,
}

#[async_trait]
impl EmbeddingProvider for LocalProvider {
    async fn embed_text(&self, text: &str) -> Result<Embedding, EmbeddingError> {
        let request = json!({
            "model": self.model,
            "prompt": text
        });

        let response = self.client
            .post(&format!("{}/api/embeddings", self.endpoint))
            .json(&request)
            .send()
            .await?
            .json::<LocalResponse>()
            .await?;

        Ok(Embedding {
            vector: response.embedding,
            checksum: None,
        })
    }

    // ... rest
}
```

### 6.3 Provider Selection Logic

```rust
impl EmbeddingProviderManager {
    /// Get embedding from configured provider
    pub async fn generate_embedding(
        &self,
        text: &str,
        config: Option<&EmbeddingGenerationConfig>,
    ) -> Result<(Embedding, EmbeddingMetadata), EmbeddingError> {
        let provider_name = config
            .and_then(|c| c.provider.as_ref())
            .unwrap_or(&self.default_provider);

        let provider = self.providers
            .get(provider_name)
            .ok_or(EmbeddingError::ProviderNotFound)?;

        let embedding = provider.embed_text(text).await?;

        let metadata = EmbeddingMetadata {
            model: provider.model_info().name,
            provider: provider.provider_name().to_string(),
            dimensions: embedding.vector.len(),
            source_fields: vec![],
            generated_at: Utc::now(),
            source: EmbeddingSource::AutoGenerated,
            token_count: Some(estimate_tokens(text)),
        };

        Ok((embedding, metadata))
    }
}
```

### 6.4 Error Handling & Retries

```rust
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("Provider not found: {0}")]
    ProviderNotFound,

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Invalid dimensions: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("Text too long: {length} tokens (max: {max})")]
    TextTooLong { length: usize, max: usize },
}

/// Retry logic with exponential backoff
pub async fn embed_with_retry(
    provider: &dyn EmbeddingProvider,
    text: &str,
    max_retries: usize,
) -> Result<Embedding, EmbeddingError> {
    let mut retries = 0;
    let mut backoff = Duration::from_millis(100);

    loop {
        match provider.embed_text(text).await {
            Ok(embedding) => return Ok(embedding),
            Err(e) if retries < max_retries => {
                retries += 1;
                tokio::time::sleep(backoff).await;
                backoff *= 2;
            }
            Err(e) => return Err(e),
        }
    }
}
```

---

## 7. Vector Index Management

### 7.1 HNSW Index Implementation

**Library**: `hnsw_rs` (pure Rust, high performance)

**Alternative**: `hnswlib-rs` (bindings to C++ hnswlib)

```rust
use hnsw_rs::hnsw::Hnsw;
use hnsw_rs::dist::DistCosine;

pub struct VectorIndexManager {
    /// Indexes per tenant
    indexes: DashMap<TenantId, Arc<RwLock<VectorIndex>>>,

    /// Configuration
    config: IndexConfig,

    /// Storage path for persisted indexes
    storage_path: PathBuf,
}

impl VectorIndexManager {
    /// Create or load index for tenant
    pub async fn get_or_create_index(
        &self,
        tenant_id: &TenantId,
    ) -> Result<Arc<RwLock<VectorIndex>>, IndexError> {
        if let Some(index) = self.indexes.get(tenant_id) {
            return Ok(index.clone());
        }

        // Try to load from disk
        let index_path = self.index_path(tenant_id);
        let index = if index_path.exists() {
            self.load_index(tenant_id).await?
        } else {
            self.create_index(tenant_id).await?
        };

        let index_arc = Arc::new(RwLock::new(index));
        self.indexes.insert(tenant_id.clone(), index_arc.clone());

        Ok(index_arc)
    }

    /// Build index from events
    async fn create_index(
        &self,
        tenant_id: &TenantId,
    ) -> Result<VectorIndex, IndexError> {
        let start = Instant::now();

        // Fetch all events with embeddings for this tenant
        let events = self.event_repository
            .get_events_with_embeddings(tenant_id)
            .await?;

        // Create HNSW index
        let mut hnsw = Hnsw::<f32, DistCosine>::new(
            self.config.m,
            events.len(),
            self.config.dimensions,
            self.config.ef_construction,
            DistCosine {},
        );

        // Insert vectors
        let mut id_mapping = HashMap::new();
        let mut event_to_index = HashMap::new();

        for (idx, event) in events.iter().enumerate() {
            if let Some(embedding) = &event.embedding {
                hnsw.insert((&embedding.vector, idx));
                id_mapping.insert(idx as u64, event.id);
                event_to_index.insert(event.id, idx as u64);
            }
        }

        let build_time = start.elapsed();

        Ok(VectorIndex {
            tenant_id: tenant_id.clone(),
            config: self.config.clone(),
            index: hnsw,
            id_mapping,
            event_to_index,
            stats: IndexStats {
                total_vectors: events.len(),
                memory_bytes: estimate_index_size(&hnsw),
                build_time_ms: build_time.as_millis() as u64,
                last_rebuild: Utc::now(),
            },
            last_updated: Utc::now(),
        })
    }

    /// Insert new vector into index
    pub async fn insert_vector(
        &self,
        tenant_id: &TenantId,
        event_id: Uuid,
        embedding: &Embedding,
    ) -> Result<(), IndexError> {
        let index = self.get_or_create_index(tenant_id).await?;
        let mut index = index.write().await;

        let idx = index.id_mapping.len() as u64;
        index.index.insert((&embedding.vector, idx as usize));
        index.id_mapping.insert(idx, event_id);
        index.event_to_index.insert(event_id, idx);
        index.stats.total_vectors += 1;
        index.last_updated = Utc::now();

        Ok(())
    }

    /// Search for nearest neighbors
    pub async fn search(
        &self,
        tenant_id: &TenantId,
        query_vector: &[f32],
        k: usize,
    ) -> Result<Vec<SearchResult>, IndexError> {
        let index = self.get_or_create_index(tenant_id).await?;
        let index = index.read().await;

        // Validate dimensions
        if query_vector.len() != self.config.dimensions {
            return Err(IndexError::DimensionMismatch {
                expected: self.config.dimensions,
                actual: query_vector.len(),
            });
        }

        // Search using HNSW
        let neighbors = index.index.search(
            query_vector,
            k,
            self.config.ef_search,
        );

        // Map index IDs back to event IDs
        let results = neighbors
            .iter()
            .filter_map(|neighbor| {
                index.id_mapping.get(&(neighbor.d_id as u64)).map(|event_id| {
                    SearchResult {
                        event_id: *event_id,
                        distance: neighbor.distance,
                        score: 1.0 - neighbor.distance, // Cosine similarity
                    }
                })
            })
            .collect();

        Ok(results)
    }

    /// Persist index to disk
    pub async fn persist_index(
        &self,
        tenant_id: &TenantId,
    ) -> Result<(), IndexError> {
        let index = self.get_or_create_index(tenant_id).await?;
        let index = index.read().await;

        let index_path = self.index_path(tenant_id);
        index.index.file_dump(&index_path)?;

        // Save metadata
        let metadata_path = index_path.with_extension("json");
        let metadata = json!({
            "tenant_id": tenant_id.to_string(),
            "config": index.config,
            "stats": index.stats,
            "id_mapping": index.id_mapping,
        });
        tokio::fs::write(metadata_path, serde_json::to_string_pretty(&metadata)?).await?;

        Ok(())
    }
}
```

### 7.2 Index Persistence Strategy

**Directory Structure**:
```
data/
├── events/
│   └── tenant-abc.parquet
└── indexes/
    └── tenant-abc/
        ├── vector.hnsw       # HNSW graph
        ├── metadata.json     # Config + stats
        └── checksum.sha256   # Integrity check
```

**Persistence Frequency**:
- Every 1000 inserts
- Every 5 minutes (if changes)
- On graceful shutdown

### 7.3 Index Rebuilding

**Triggers**:
- Configuration change (M, ef_construction)
- Large batch reprocessing
- Index corruption detected
- Manual rebuild request

**Process**:
```rust
pub async fn rebuild_index(
    &self,
    tenant_id: &TenantId,
    new_config: Option<IndexConfig>,
) -> Result<RebuildStats, IndexError> {
    let start = Instant::now();

    // Create new index with updated config
    let config = new_config.unwrap_or(self.config.clone());
    let old_index = self.indexes.remove(tenant_id);

    let new_index = self.create_index_with_config(tenant_id, config).await?;

    let index_arc = Arc::new(RwLock::new(new_index));
    self.indexes.insert(tenant_id.clone(), index_arc);

    // Persist immediately
    self.persist_index(tenant_id).await?;

    Ok(RebuildStats {
        duration: start.elapsed(),
        vectors_indexed: index_arc.read().await.stats.total_vectors,
        old_memory_bytes: old_index.map(|i| i.read().await.stats.memory_bytes),
        new_memory_bytes: index_arc.read().await.stats.memory_bytes,
    })
}
```

---

## 8. Storage Strategy

### 8.1 Parquet Storage with Embeddings

**Write Path**:
```rust
pub async fn write_events_to_parquet(
    &self,
    events: &[Event],
    path: &Path,
) -> Result<(), StorageError> {
    // Create Arrow schema with embedding columns
    let schema = Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Binary, false),
        Field::new("event_type", DataType::Utf8, false),
        // ... existing fields ...
        Field::new("embedding_vector",
            DataType::List(Arc::new(Field::new("item", DataType::Float32, true))),
            true
        ),
        Field::new("embedding_model", DataType::Utf8, true),
        Field::new("embedding_dimensions", DataType::Int32, true),
    ]));

    // Build record batch
    let mut embedding_vectors = Vec::new();
    for event in events {
        if let Some(embedding) = &event.embedding {
            embedding_vectors.push(Some(embedding.vector.clone()));
        } else {
            embedding_vectors.push(None);
        }
    }

    let embedding_array = ListArray::from_iter_primitive::<Float32Type, _, _>(
        embedding_vectors.into_iter()
    );

    // ... create other columns ...

    let batch = RecordBatch::try_new(schema.clone(), vec![
        // ... all columns ...
        Arc::new(embedding_array),
    ])?;

    // Write Parquet with compression
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD) // Better than gzip for vectors
        .set_writer_version(WriterVersion::PARQUET_2_0)
        .build();

    let file = File::create(path)?;
    let mut writer = ArrowWriter::try_new(file, schema, Some(props))?;
    writer.write(&batch)?;
    writer.close()?;

    Ok(())
}
```

**Read Path**:
```rust
pub async fn read_events_from_parquet(
    &self,
    path: &Path,
) -> Result<Vec<Event>, StorageError> {
    let file = File::open(path)?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)?.build()?;

    let mut events = Vec::new();

    for batch_result in reader {
        let batch = batch_result?;

        // Extract embedding column
        let embedding_column = batch
            .column_by_name("embedding_vector")
            .and_then(|col| col.as_any().downcast_ref::<ListArray>());

        for row in 0..batch.num_rows() {
            let event = Event {
                // ... parse standard fields ...
                embedding: embedding_column.and_then(|arr| {
                    if arr.is_null(row) {
                        None
                    } else {
                        let values = arr.value(row);
                        let float_array = values
                            .as_any()
                            .downcast_ref::<Float32Array>()?;
                        Some(Embedding {
                            vector: float_array.values().to_vec(),
                            checksum: None,
                        })
                    }
                }),
                // ... rest
            };
            events.push(event);
        }
    }

    Ok(events)
}
```

### 8.2 Storage Optimization

**Compression**:
- ZSTD compression for embedding columns (better than gzip for float arrays)
- Expected compression ratio: 20-30% (floats don't compress well)

**Columnar Benefits**:
- Can read events without loading embeddings
- Can read embeddings without loading full payloads
- Efficient for hybrid queries

**Storage Estimates**:
- 1536-dim embedding (f32): 6KB raw
- Compressed: ~4-5KB
- 1M events: ~5GB embedding storage

---

## 9. Query Processing

### 9.1 Semantic Search Flow

```rust
pub async fn semantic_search(
    &self,
    request: SemanticSearchRequest,
    tenant_id: &TenantId,
) -> Result<SemanticSearchResponse, QueryError> {
    let start = Instant::now();

    // Step 1: Generate query embedding (if text query)
    let query_vector = match request.query {
        Query::Text(ref text) => {
            let (embedding, _) = self.embedding_manager
                .generate_embedding(text, request.embedding_config.as_ref())
                .await?;
            embedding.vector
        }
        Query::Vector(ref vec) => vec.clone(),
    };

    // Step 2: Search vector index
    let index_results = self.index_manager
        .search(tenant_id, &query_vector, request.limit * 2) // Fetch 2x for filtering
        .await?;

    // Step 3: Apply temporal/metadata filters
    let filtered_results = self.apply_filters(
        index_results,
        tenant_id,
        &request.filters,
    ).await?;

    // Step 4: Fetch full events
    let event_ids: Vec<Uuid> = filtered_results
        .iter()
        .take(request.limit)
        .map(|r| r.event_id)
        .collect();

    let events = self.event_repository
        .get_events_by_ids(&event_ids)
        .await?;

    // Step 5: Combine results with scores
    let results = events
        .into_iter()
        .filter_map(|event| {
            filtered_results
                .iter()
                .find(|r| r.event_id == event.id)
                .map(|r| SemanticResult {
                    event,
                    score: r.score,
                    distance: r.distance,
                })
        })
        .filter(|r| r.score >= request.min_score.unwrap_or(0.0))
        .collect();

    Ok(SemanticSearchResponse {
        results,
        total: results.len(),
        search_time_ms: start.elapsed().as_millis() as u64,
        query_embedding: Some(EmbeddingInfo {
            model: "...".to_string(),
            dimensions: query_vector.len(),
        }),
    })
}

async fn apply_filters(
    &self,
    results: Vec<SearchResult>,
    tenant_id: &TenantId,
    filters: &Option<SearchFilters>,
) -> Result<Vec<SearchResult>, QueryError> {
    let Some(filters) = filters else {
        return Ok(results);
    };

    // Build SQL filter query for Parquet
    let mut filter_predicates = vec![
        format!("tenant_id = '{}'", tenant_id),
    ];

    if let Some(event_type) = &filters.event_type {
        filter_predicates.push(format!("event_type = '{}'", event_type));
    }

    if let Some(time_range) = &filters.time_range {
        if let Some(since) = time_range.since {
            filter_predicates.push(format!("timestamp >= '{}'", since.to_rfc3339()));
        }
        if let Some(until) = time_range.until {
            filter_predicates.push(format!("timestamp <= '{}'", until.to_rfc3339()));
        }
    }

    // Filter results based on predicates
    // (In practice, use DataFusion or similar for efficient filtering)
    let valid_event_ids = self.parquet_query(filter_predicates).await?;

    Ok(results
        .into_iter()
        .filter(|r| valid_event_ids.contains(&r.event_id))
        .collect())
}
```

### 9.2 Hybrid Query Processing

```rust
pub async fn hybrid_search(
    &self,
    request: HybridSearchRequest,
    tenant_id: &TenantId,
) -> Result<SemanticSearchResponse, QueryError> {
    // Execute both queries in parallel
    let (semantic_results, temporal_results) = tokio::join!(
        self.semantic_search(request.semantic, tenant_id),
        self.temporal_query(request.temporal, tenant_id),
    );

    let semantic = semantic_results?;
    let temporal = temporal_results?;

    // Combine with weighted scoring
    let mut combined_scores: HashMap<Uuid, f32> = HashMap::new();

    for result in semantic.results {
        let score = result.score * request.semantic.weight;
        *combined_scores.entry(result.event.id).or_insert(0.0) += score;
    }

    for event in temporal.events {
        let score = 1.0 * request.temporal.weight; // Perfect temporal match
        *combined_scores.entry(event.id).or_insert(0.0) += score;
    }

    // Sort by combined score
    let mut results: Vec<_> = combined_scores
        .into_iter()
        .map(|(event_id, score)| (event_id, score))
        .collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Fetch events
    let event_ids: Vec<Uuid> = results
        .iter()
        .take(request.limit)
        .map(|(id, _)| *id)
        .collect();

    let events = self.event_repository.get_events_by_ids(&event_ids).await?;

    // ... construct response
}
```

### 9.3 Query Optimization

**Caching Strategy**:
```rust
pub struct QueryCache {
    /// LRU cache for query embeddings
    embedding_cache: LruCache<String, Embedding>,

    /// Cache for recent search results
    result_cache: LruCache<QueryFingerprint, CachedResults>,

    ttl: Duration,
}

impl QueryCache {
    pub async fn get_or_generate_embedding(
        &mut self,
        query: &str,
        provider: &dyn EmbeddingProvider,
    ) -> Result<Embedding, EmbeddingError> {
        let cache_key = format!("{}:{}", provider.provider_name(), query);

        if let Some(cached) = self.embedding_cache.get(&cache_key) {
            return Ok(cached.clone());
        }

        let embedding = provider.embed_text(query).await?;
        self.embedding_cache.put(cache_key, embedding.clone());

        Ok(embedding)
    }
}
```

**Pre-filtering**:
- Apply temporal filters before vector search
- Use Parquet predicate pushdown
- Reduce candidate set for HNSW

---

## 10. Security & Multi-Tenancy

### 10.1 Tenant Isolation

**Index Isolation**:
- Separate HNSW index per tenant
- No cross-tenant vector leakage
- Tenant ID validation on all queries

**Storage Isolation**:
- Tenant ID column in Parquet (partition key)
- Separate directories per tenant (optional)
- Row-level security in queries

### 10.2 Access Control

```rust
pub async fn authorize_embedding_access(
    &self,
    claims: &JwtClaims,
    operation: EmbeddingOperation,
) -> Result<(), AuthError> {
    match operation {
        EmbeddingOperation::Search => {
            // All authenticated users can search their tenant
            if !claims.permissions.contains(&Permission::Read) {
                return Err(AuthError::InsufficientPermissions);
            }
        }
        EmbeddingOperation::Reprocess => {
            // Only developers/admins can reprocess
            if !claims.role.is_admin() && !claims.role.is_developer() {
                return Err(AuthError::InsufficientPermissions);
            }
        }
        EmbeddingOperation::RebuildIndex => {
            // Only admins
            if !claims.role.is_admin() {
                return Err(AuthError::InsufficientPermissions);
            }
        }
    }

    Ok(())
}
```

### 10.3 API Key Management for Providers

**Secure Storage**:
```rust
pub struct ProviderCredentials {
    /// Encrypted API keys per tenant
    credentials: DashMap<TenantId, HashMap<String, EncryptedApiKey>>,

    /// KMS for encryption/decryption
    kms: Arc<KeyManagementService>,
}

impl ProviderCredentials {
    pub async fn get_api_key(
        &self,
        tenant_id: &TenantId,
        provider: &str,
    ) -> Result<String, CredentialError> {
        let tenant_creds = self.credentials
            .get(tenant_id)
            .ok_or(CredentialError::NotFound)?;

        let encrypted = tenant_creds
            .get(provider)
            .ok_or(CredentialError::ProviderNotConfigured)?;

        // Decrypt using KMS
        let api_key = self.kms.decrypt(encrypted).await?;

        Ok(api_key)
    }
}
```

**Audit Logging**:
- Log all embedding API calls
- Track provider usage per tenant
- Monitor costs (token usage)

---

## 11. Performance Optimization

### 11.1 Benchmarking Targets

| Operation | Target (p95) | Current Baseline |
|-----------|--------------|------------------|
| Embedding generation (OpenAI) | <100ms | N/A |
| Embedding generation (local) | <10ms | N/A |
| Vector insert | <1ms | N/A |
| Semantic search (1M vectors) | <50ms | N/A |
| Hybrid search | <100ms | N/A |
| Index build (1M vectors) | <2min | N/A |

### 11.2 Optimization Techniques

#### 11.2.1 Batched Embedding Generation

```rust
pub async fn ingest_batch_with_embeddings(
    &self,
    events: Vec<CreateEventRequest>,
) -> Result<BatchIngestResponse, IngestError> {
    // Separate events needing embeddings
    let (needs_embedding, has_embedding): (Vec<_>, Vec<_>) = events
        .into_iter()
        .partition(|e| e.embedding_config.as_ref().map_or(false, |c| c.auto_generate));

    // Batch generate embeddings (much faster than individual)
    let texts: Vec<String> = needs_embedding
        .iter()
        .map(|e| extract_text_for_embedding(&e.payload, &e.embedding_config))
        .collect();

    let embeddings = self.embedding_manager
        .embed_batch(&texts)
        .await?;

    // Combine and ingest
    let mut all_events = has_embedding;
    for (mut event, embedding) in needs_embedding.into_iter().zip(embeddings) {
        event.embedding = Some(embedding);
        all_events.push(event);
    }

    self.ingest_events(all_events).await
}
```

#### 11.2.2 Async Index Updates

```rust
pub struct AsyncIndexUpdater {
    update_queue: Arc<SegQueue<IndexUpdate>>,
    worker_handles: Vec<JoinHandle<()>>,
}

impl AsyncIndexUpdater {
    pub async fn queue_update(&self, update: IndexUpdate) {
        self.update_queue.push(update);
    }

    async fn worker_loop(&self) {
        while let Some(update) = self.update_queue.pop() {
            match update {
                IndexUpdate::Insert { tenant_id, event_id, embedding } => {
                    if let Err(e) = self.index_manager
                        .insert_vector(&tenant_id, event_id, &embedding)
                        .await
                    {
                        error!("Failed to insert vector: {}", e);
                    }
                }
                IndexUpdate::Rebuild { tenant_id } => {
                    if let Err(e) = self.index_manager
                        .rebuild_index(&tenant_id, None)
                        .await
                    {
                        error!("Failed to rebuild index: {}", e);
                    }
                }
            }
        }
    }
}
```

#### 11.2.3 SIMD for Cosine Similarity

```rust
use std::arch::x86_64::*;

/// Fast cosine similarity using SIMD (AVX2)
#[target_feature(enable = "avx2")]
unsafe fn cosine_similarity_simd(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    assert!(a.len() % 8 == 0, "Length must be multiple of 8 for AVX2");

    let mut dot_product = _mm256_setzero_ps();
    let mut norm_a = _mm256_setzero_ps();
    let mut norm_b = _mm256_setzero_ps();

    for i in (0..a.len()).step_by(8) {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));

        dot_product = _mm256_add_ps(dot_product, _mm256_mul_ps(va, vb));
        norm_a = _mm256_add_ps(norm_a, _mm256_mul_ps(va, va));
        norm_b = _mm256_add_ps(norm_b, _mm256_mul_ps(vb, vb));
    }

    // Horizontal sum
    let dot = horizontal_sum_avx2(dot_product);
    let na = horizontal_sum_avx2(norm_a).sqrt();
    let nb = horizontal_sum_avx2(norm_b).sqrt();

    dot / (na * nb)
}
```

### 11.3 Memory Management

**Vector Storage**:
- Use `Vec<f32>` (4 bytes per dim)
- Consider f16 for memory savings (2 bytes, some accuracy loss)
- Lazy loading for large embeddings

**Index Memory**:
- HNSW: ~(M * 2 * 4 + 1536 * 4) bytes per vector = ~200 bytes + 6KB = 6.2KB
- 1M vectors: ~6.2GB RAM
- Use memory-mapped indexes for large tenants

---

## 12. Migration Strategy

### 12.1 Backward Compatibility

**All changes are additive**:
- ✅ New columns are nullable
- ✅ Existing APIs unchanged
- ✅ Embedding fields optional
- ✅ Old Parquet files readable

### 12.2 Rollout Plan

**Phase 1: Infrastructure (Week 1-2)**
- Deploy embedding provider support
- Add embedding columns to Parquet schema
- No user-facing changes

**Phase 2: Opt-In Beta (Week 3-4)**
- Enable embedding API for beta tenants
- Monitor performance and costs
- Gather feedback

**Phase 3: General Availability (Week 5-6)**
- Enable for all tenants
- Documentation and examples
- Marketing launch

**Phase 4: Reprocessing (Week 7+)**
- Offer optional reprocessing of historical events
- Gradual rollout to avoid API rate limits

### 12.3 Data Migration

**Reprocessing Historical Events**:
```sql
-- Identify events to reprocess
SELECT COUNT(*)
FROM events
WHERE embedding_vector IS NULL
  AND event_type IN ('user.message', 'support.ticket')
  AND timestamp > '2024-01-01';

-- Batch reprocessing (via API)
POST /api/v1/embeddings/reprocess
{
  "filters": {
    "event_type": "user.message",
    "time_range": {"since": "2024-01-01T00:00:00Z"}
  },
  "batch_size": 100,
  "rate_limit": {
    "max_per_minute": 500
  }
}
```

**Cost Estimation**:
- 1M events × $0.02/1M tokens (OpenAI small) ≈ $20
- Processing time: ~2 hours (500 req/min)

---

## 13. Testing Strategy

### 13.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedding_generation() {
        let provider = MockProvider::new();
        let text = "test message";

        let embedding = provider.embed_text(text).await.unwrap();

        assert_eq!(embedding.vector.len(), 1536);
        assert!(embedding.vector.iter().all(|&v| v.is_finite()));
    }

    #[tokio::test]
    async fn test_vector_index_insert_and_search() {
        let manager = VectorIndexManager::new_test();
        let tenant_id = TenantId::new("test-tenant").unwrap();

        // Insert vectors
        let event_id = Uuid::new_v4();
        let embedding = random_embedding(1536);
        manager.insert_vector(&tenant_id, event_id, &embedding).await.unwrap();

        // Search
        let results = manager.search(&tenant_id, &embedding.vector, 5).await.unwrap();

        assert_eq!(results[0].event_id, event_id);
        assert!(results[0].score > 0.99); // Nearly perfect match
    }

    #[tokio::test]
    async fn test_semantic_search_with_filters() {
        let store = EventStore::new_test();

        // Ingest events with embeddings
        let events = vec![
            create_test_event("fraud detected", "transaction.alert"),
            create_test_event("suspicious activity", "transaction.alert"),
            create_test_event("payment successful", "transaction.success"),
        ];
        store.ingest_batch(events).await.unwrap();

        // Search for fraud-related events
        let results = store.semantic_search(SemanticSearchRequest {
            query: Query::Text("fraud pattern".to_string()),
            limit: 10,
            filters: Some(SearchFilters {
                event_type: Some("transaction.alert".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }).await.unwrap();

        assert_eq!(results.results.len(), 2);
        assert!(results.results[0].score > 0.7);
    }
}
```

### 13.2 Integration Tests

```rust
#[tokio::test]
async fn test_end_to_end_embedding_flow() {
    // Start services
    let (core, control_plane) = start_test_services().await;

    // Configure OpenAI provider
    configure_provider(&core, "openai", "sk-test-key").await;

    // Ingest event with auto-embedding
    let response = core.ingest_event(CreateEventRequest {
        event_type: "user.message".to_string(),
        payload: json!({"text": "How do I reset my password?"}),
        embedding_config: Some(EmbeddingConfig {
            auto_generate: true,
            source_fields: vec!["payload.text".to_string()],
            ..Default::default()
        }),
        ..Default::default()
    }).await.unwrap();

    assert!(response.embedding_metadata.is_some());

    // Search semantically
    let search_results = core.semantic_search(SemanticSearchRequest {
        query: Query::Text("password reset help".to_string()),
        limit: 5,
        ..Default::default()
    }).await.unwrap();

    assert!(!search_results.results.is_empty());
    assert!(search_results.results[0].score > 0.8);
}
```

### 13.3 Performance Tests

```rust
#[tokio::test]
async fn bench_semantic_search_1m_vectors() {
    let store = EventStore::new_test();
    let tenant_id = TenantId::new("bench-tenant").unwrap();

    // Insert 1M vectors
    for i in 0..1_000_000 {
        let event = create_random_event_with_embedding();
        store.ingest_event(event).await.unwrap();
    }

    // Build index
    store.rebuild_index(&tenant_id, None).await.unwrap();

    // Benchmark search
    let query = random_embedding(1536);
    let start = Instant::now();

    for _ in 0..100 {
        store.index_manager.search(&tenant_id, &query.vector, 10).await.unwrap();
    }

    let avg_latency = start.elapsed() / 100;

    assert!(avg_latency < Duration::from_millis(50),
        "Search latency {} ms exceeds target", avg_latency.as_millis());
}
```

### 13.4 Load Tests

**Locust Script** (Python):
```python
from locust import HttpUser, task, between

class AllSourceUser(HttpUser):
    wait_time = between(0.1, 0.5)

    @task(3)
    def ingest_with_embedding(self):
        self.client.post("/api/v1/events", json={
            "event_type": "user.action",
            "payload": {"description": f"User action {random_text()}"},
            "embedding_config": {"auto_generate": True}
        })

    @task(7)
    def semantic_search(self):
        self.client.post("/api/v1/events/search/semantic", json={
            "query": random_query(),
            "limit": 10
        })

# Run: locust -f load_test.py --host=http://localhost:3900
```

---

## 14. Deployment & Operations

### 14.1 Configuration

**Environment Variables**:
```bash
# Embedding Providers
ALLSOURCE_EMBEDDING_DEFAULT_PROVIDER=openai
ALLSOURCE_EMBEDDING_DEFAULT_MODEL=text-embedding-3-large
ALLSOURCE_OPENAI_API_KEY=sk-...
ALLSOURCE_COHERE_API_KEY=co-...
ALLSOURCE_LOCAL_ENDPOINT=http://localhost:3900

# Vector Index
ALLSOURCE_INDEX_M=16
ALLSOURCE_INDEX_EF_CONSTRUCTION=200
ALLSOURCE_INDEX_EF_SEARCH=50
ALLSOURCE_INDEX_PERSIST_INTERVAL=300  # seconds

# Performance
ALLSOURCE_EMBEDDING_BATCH_SIZE=100
ALLSOURCE_EMBEDDING_CACHE_TTL=3600
ALLSOURCE_MAX_EMBEDDING_WORKERS=10
```

**Config File** (`config/embeddings.yaml`):
```yaml
embeddings:
  enabled: true
  default_provider: openai
  default_model: text-embedding-3-large

  providers:
    openai:
      api_key: ${OPENAI_API_KEY}
      api_base: https://api.openai.com/v1
      models:
        - text-embedding-3-large
        - text-embedding-3-small
      timeout_seconds: 30
      max_retries: 3

    cohere:
      api_key: ${COHERE_API_KEY}
      models:
        - embed-multilingual-v3.0

    local:
      endpoint: ${LOCAL_EMBEDDING_ENDPOINT}
      model: all-MiniLM-L6-v2

  index:
    m: 16
    ef_construction: 200
    ef_search: 50
    persist_interval_seconds: 300

  cache:
    enabled: true
    ttl_seconds: 3600
    max_size_mb: 1024
```

### 14.2 Monitoring

**Metrics to Track**:
```rust
// Embedding generation
allsource_embedding_generation_total{provider, model, status}
allsource_embedding_generation_duration_seconds{provider}
allsource_embedding_tokens_total{provider, model}
allsource_embedding_cost_usd{provider}

// Vector index
allsource_vector_index_size_vectors{tenant_id}
allsource_vector_index_memory_bytes{tenant_id}
allsource_vector_index_insert_duration_seconds
allsource_vector_index_search_duration_seconds

// Semantic search
allsource_semantic_search_total{status}
allsource_semantic_search_duration_seconds
allsource_semantic_search_results_returned

// Cache
allsource_embedding_cache_hits_total
allsource_embedding_cache_misses_total
```

**Alerts**:
```yaml
- alert: EmbeddingProviderDown
  expr: rate(allsource_embedding_generation_total{status="error"}[5m]) > 0.1
  for: 5m
  annotations:
    summary: "Embedding provider {{ $labels.provider }} failing"

- alert: HighEmbeddingLatency
  expr: histogram_quantile(0.95, allsource_embedding_generation_duration_seconds) > 0.5
  for: 10m
  annotations:
    summary: "p95 embedding latency > 500ms"

- alert: SlowSemanticSearch
  expr: histogram_quantile(0.95, allsource_semantic_search_duration_seconds) > 0.1
  for: 10m
  annotations:
    summary: "p95 semantic search latency > 100ms"
```

### 14.3 Operational Runbooks

**Index Corruption**:
```bash
# Detect
curl http://localhost:3900/api/v1/embeddings/index/tenant-abc/health

# Rebuild
curl -X POST http://localhost:3900/api/v1/embeddings/index/rebuild \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"tenant_id": "tenant-abc"}'
```

**Provider Outage**:
```bash
# Switch to backup provider
curl -X PUT http://localhost:3900/api/v1/config/embeddings \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"default_provider": "cohere"}'
```

**Cost Spike**:
```bash
# Check usage
curl http://localhost:3900/api/v1/metrics/embeddings/cost?tenant_id=tenant-abc

# Disable auto-embedding temporarily
curl -X PUT http://localhost:3900/api/v1/config/embeddings \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"auto_generate_default": false}'
```

---

## 15. Future Work

### 15.1 Phase 2 Enhancements

**Multimodal Embeddings**:
- Image embeddings (CLIP, DINO)
- Audio embeddings (Whisper)
- Video embeddings (VideoMAE)
- Cross-modal search

**Advanced Indexing**:
- Product Quantization (PQ) for memory reduction
- IVF (Inverted File) for billion-scale
- Hybrid IVF-HNSW

**GPU Acceleration**:
- FAISS GPU for index building
- Batch embedding on GPU
- 10-100x speedup

### 15.2 Phase 3 Research

**Learned Embeddings**:
- Fine-tune embedding models on AllSource events
- Domain-specific embeddings
- Tenant-specific embeddings

**Streaming Embeddings**:
- Real-time embedding updates
- Incremental index updates
- Low-latency search

**Federated Search**:
- Cross-tenant search (with permissions)
- Multi-index queries
- Distributed HNSW

---

## 18. Build vs. Integrate Decision

### 18.1 Strategic Options

When the time comes to add vector search capabilities, we have three strategic paths:

#### Option A: Build Custom (This Document's Approach)

**Approach**: Implement vector embeddings natively in AllSource.

**Pros**:
- ✅ Full control over architecture
- ✅ Tight integration with event sourcing
- ✅ Optimize for our use cases
- ✅ Single system to maintain
- ✅ Unique hybrid temporal + semantic queries

**Cons**:
- ❌ Significant engineering effort (6-12 months)
- ❌ Vector search not our core competency
- ❌ Need to compete with specialized vendors
- ❌ Slower time to market

**Best For**: If vector search becomes a core differentiator and we need deep integration.

#### Option B: Integrate with LanceDB

**Approach**: Use LanceDB as vector search layer, AllSource as event sourcing layer.

**Pros**:
- ✅ Leverage their expertise (proven 700M vector deployments)
- ✅ Faster time to market (weeks vs months)
- ✅ Best-in-class vector performance
- ✅ Let them handle vector challenges
- ✅ Focus on our core: event sourcing

**Cons**:
- ❌ Dependency on external system
- ❌ Two systems to maintain
- ❌ Integration complexity
- ❌ Potential vendor lock-in
- ❌ License considerations (Apache 2.0 is fine, but check enterprise)

**Architecture**:
```
┌─────────────────────────────────────┐
│        AllSource (Event Store)        │
│  • Time-travel queries              │
│  • Event sourcing                   │
│  • Multi-tenancy                    │
│  • CQRS patterns                    │
└──────────────┬──────────────────────┘
               │
               │ Event Stream
               ▼
┌─────────────────────────────────────┐
│      LanceDB (Vector Search)        │
│  • Semantic search                  │
│  • Vector indexing (IVF-PQ, HNSW)   │
│  • Multimodal data                  │
└─────────────────────────────────────┘
               ▲
               │
          Query API
               │
┌──────────────┴──────────────────────┐
│      Unified Query Layer            │
│  • Hybrid temporal + semantic       │
│  • Cross-system queries             │
└─────────────────────────────────────┘
```

**Integration Pattern**:
1. Events ingested to AllSource (as today)
2. Async pipeline pushes event payloads → LanceDB with embeddings
3. Queries: AllSource for temporal, LanceDB for semantic, or hybrid
4. Event ID as foreign key linking both systems

**Best For**: If we want AI capabilities quickly without building from scratch.

#### Option C: Integrate with Purpose-Built Vector DB (Pinecone, Qdrant, Weaviate)

**Approach**: Use managed vector database service.

**Pros**:
- ✅ Fully managed (no ops burden)
- ✅ Enterprise support
- ✅ Battle-tested at scale
- ✅ Fast time to market

**Cons**:
- ❌ Vendor lock-in
- ❌ Recurring costs (expensive at scale)
- ❌ Less control
- ❌ Data egress fees

**Best For**: Quick MVP or if we don't want to manage vector infrastructure.

### 18.2 Decision Matrix

| Criteria | Build Custom | Integrate LanceDB | Managed Service |
|----------|-------------|-------------------|-----------------|
| **Time to Market** | 6-12 months | 1-2 months | 2-4 weeks |
| **Engineering Cost** | High | Medium | Low |
| **Operating Cost** | Low | Low | High (at scale) |
| **Control** | Full | High | Limited |
| **Flexibility** | Maximum | High | Medium |
| **Risk** | High (unproven) | Medium | Low |
| **Differentiation** | High | Medium | Low |
| **Maintenance** | High | Medium | Low |

### 18.3 Recommended Path

**Near-Term (0-12 months)**: **No Action**
- Focus on Phase 1.5 (Clean Architecture) and v1.2 (performance)
- Monitor AI/ML adoption in customer base
- Validate demand for semantic search

**Mid-Term (12-24 months)**: **Option B - Integrate with LanceDB**

**Why LanceDB**:
1. **Open Source**: Apache 2.0 license, can fork if needed
2. **Rust-based**: Aligns with our stack, could contribute upstream
3. **Columnar Design**: Philosophy aligns with our Parquet approach
4. **Proven Scale**: 700M vectors in production
5. **Fast Integration**: Weeks, not months
6. **Escape Hatch**: If integration doesn't work, we have Option A as fallback

**Integration Steps**:
```
Phase 1: Proof of Concept (2 weeks)
- Spin up LanceDB alongside AllSource
- Ingest sample events with embeddings
- Test semantic search performance
- Measure latency, accuracy, costs

Phase 2: Production Integration (4-6 weeks)
- Event pipeline: AllSource → LanceDB
- Unified query API
- Multi-tenant isolation
- Monitoring & alerting

Phase 3: Hybrid Queries (4 weeks)
- Temporal + semantic combined queries
- Performance optimization
- Customer pilot program

Phase 4: Scale & Optimize (ongoing)
- Performance tuning
- Cost optimization
- Feature enhancements
```

**Long-Term (24+ months)**: **Evaluate Build Custom (Option A)**

**Conditions to trigger build decision**:
- [ ] Semantic search is top 3 customer feature request
- [ ] Integration limitations blocking key use cases
- [ ] Cost of managed solution > cost of build
- [ ] Unique requirements LanceDB can't satisfy
- [ ] We have dedicated vector search team

**If conditions met**: Use learnings from LanceDB integration to inform custom build.

### 18.4 Partnership Opportunity

**Strategic Consideration**: Approach LanceDB for partnership/collaboration.

**Potential Benefits**:
- Joint go-to-market ("Temporal AI for Events")
- Technical collaboration (we contribute to Lance format)
- Reference architecture (AllSource + LanceDB)
- Shared customers (event sourcing + vector search)
- Co-marketing opportunities

**Pitch to LanceDB**:
> "AllSource brings event sourcing + time-travel to your vector search. Together we're the only platform offering temporal + semantic queries. Our customers need vector search; your customers need event sourcing. Let's build the AI-native event platform together."

---

## 19. Future Work

### 20.1 Glossary

| Term | Definition |
|------|------------|
| **Embedding** | Dense vector representation of data (e.g., text, images) |
| **HNSW** | Hierarchical Navigable Small World graph for ANN search |
| **Cosine Similarity** | Measure of similarity between two vectors (-1 to 1) |
| **k-NN** | k-Nearest Neighbors search |
| **ANN** | Approximate Nearest Neighbor (faster than exact search) |
| **Semantic Search** | Search by meaning rather than exact keywords |

### 20.2 References

**LanceDB & Lance Format**:
- Lance Format Specification: https://lancedb.github.io/lance/format/file/
- LanceDB Documentation: https://lancedb.com/docs/
- LanceDB GitHub: https://github.com/lancedb/lancedb
- Lance v2 Blog Post: https://lancedb.com/blog/lance-v2/
- Production Deployment Case Study (700M vectors): https://sprytnyk.dev/posts/running-lancedb-in-production/

**Papers**:
- [HNSW: Efficient and robust approximate nearest neighbor search](https://arxiv.org/abs/1603.09320)
- [Text Embeddings by Weakly-Supervised Contrastive Pre-training](https://arxiv.org/abs/2212.03533) (OpenAI)

**Libraries**:
- `hnsw_rs`: https://github.com/jean-pierreBoth/hnswlib-rs
- `arrow-rs`: https://github.com/apache/arrow-rs
- `parquet`: https://parquet.apache.org/

**Competitors**:
- LanceDB: https://lancedb.com
- Pinecone: https://pinecone.io
- Weaviate: https://weaviate.io

### 20.3 Additional References

```python
from allsource import AllSourceClient

# Initialize
client = AllSourceClient("http://localhost:3900", api_key="...")

# Ingest with auto-embedding
event_id = client.ingest(
    event_type="user.message",
    payload={"text": "How do I reset my password?"},
    auto_embed=True,
    embed_fields=["payload.text"]
)

# Semantic search
results = client.search_semantic(
    query="password reset help",
    limit=10,
    filters={"event_type": "user.message"}
)

for result in results:
    print(f"Score: {result.score:.2f} - {result.event.payload}")

# Hybrid search
results = client.search_hybrid(
    semantic={"query": "fraud", "weight": 0.7},
    temporal={"event_type": "transaction.*", "weight": 0.3},
    limit=20
)
```

### 20.4 API Examples (Python SDK)

```python
from allsource import AllSourceClient

# Initialize
client = AllSourceClient("http://localhost:3900", api_key="...")

# Ingest with auto-embedding
event_id = client.ingest(
    event_type="user.message",
    payload={"text": "How do I reset my password?"},
    auto_embed=True,
    embed_fields=["payload.text"]
)

# Semantic search
results = client.search_semantic(
    query="password reset help",
    limit=10,
    filters={"event_type": "user.message"}
)

for result in results:
    print(f"Score: {result.score:.2f} - {result.event.payload}")

# Hybrid search
results = client.search_hybrid(
    semantic={"query": "fraud", "weight": 0.7},
    temporal={"event_type": "transaction.*", "weight": 0.3},
    limit=20
)
```

---

## Document Status & Approval

**Status**: ⏳ FUTURE RESEARCH DOCUMENT (Not for immediate implementation)

**Purpose**: Strategic exploration and technical analysis for potential future development.

**Next Steps**:
1. [ ] Share with engineering team for feedback
2. [ ] Discuss with product team to validate demand
3. [ ] Monitor customer requests for AI/semantic search features
4. [ ] Revisit in Q3 2025 after Phase 1.5 completion
5. [ ] Evaluate LanceDB partnership opportunity

**Decision Gate**: Do NOT proceed with implementation until:
- ✅ Phase 1.5 (Clean Architecture) is complete
- ✅ v1.2 performance targets achieved (1M+ events/sec)
- ✅ Customer demand validated (>5 enterprise customers requesting)
- ✅ Team capacity available (not pulling from core roadmap)
- ✅ Build vs. Integrate decision made

**Contact**: For questions about this document, contact AllSource Engineering Team.

**Last Updated**: 2025-11-04

---

**End of Document**
