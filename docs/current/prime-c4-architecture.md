# Prime C4 Architecture Diagrams

Architectural views of AllSource Prime — the unified agent memory engine within AllSource Core. Diagrams follow the [C4 model](https://c4model.com/) using Mermaid.

## 1. Context Diagram

Where Prime sits within the AllSource ecosystem.

```mermaid
graph TD
    Agent["AI Agent<br/><i>MCP client, SDK consumer</i>"]
    QS["Query Service<br/><i>Elixir/Phoenix API gateway<br/>Auth, billing, routing</i>"]
    Core["AllSource Core<br/><i>Rust event store<br/>WAL + Parquet + DashMap</i>"]
    Prime["Prime<br/><i>Graph + Vector + Temporal<br/>agent memory engine</i>"]

    Agent -->|"HTTP / MCP"| QS
    QS -->|"HTTP :3900<br/>Reads & Writes"| Core
    Core -.-|"subsystem"| Prime

    style Prime fill:#2d6a4f,stroke:#1b4332,color:#d8f3dc
    style Core fill:#264653,stroke:#2a9d8f,color:#e9f5db
    style QS fill:#457b9d,stroke:#1d3557,color:#f1faee
    style Agent fill:#e76f51,stroke:#d62828,color:#fff
```

**Key points:**
- Prime is a subsystem of Core, not a separate service. It uses Core's WAL, Parquet, and DashMap infrastructure.
- Query Service routes all event operations to Core. Core is the only store for events and operational metadata; no PostgreSQL is used.
- AI agents interact via the Query Service (HTTP/MCP) or embed Core directly via the Rust SDK.

## 2. Container Diagram

Inside the Prime module — major components and their responsibilities.

```mermaid
graph TD
    subgraph Prime["Prime Module"]
        Facade["Facade<br/><i>prime/facade.rs<br/>Entry point, construction,<br/>shutdown, cross-cutting ops<br/>(remember, forget, recall)</i>"]

        GraphAPI["Graph API<br/><i>Node CRUD, Edge CRUD,<br/>traversal, neighbors,<br/>ego_graph, nodes_by_type</i>"]

        VectorAPI["Vector API<br/><i>embed, search, delete_vector<br/>HNSW via instant-distance<br/>(feature: prime-vectors)</i>"]

        TemporalAPI["Temporal API<br/><i>history, diff, as_of,<br/>neighbors_as_of,<br/>time-travel queries</i>"]

        Schema["Schema Enforcement<br/><i>prime/schema.rs<br/>JSON Schema validation<br/>for node types & edges</i>"]

        Contradiction["Contradiction Detection<br/><i>Exclusive relation enforcement,<br/>conflict resolution</i>"]

        Projections["Projection Layer<br/><i>7+ projections maintaining<br/>indexed views over events</i>"]

        EmbeddedCore["EmbeddedCore<br/><i>WAL + Parquet engine<br/>ingest, ingest_batch, query<br/>register_projection</i>"]
    end

    Facade --> GraphAPI
    Facade --> VectorAPI
    Facade --> TemporalAPI
    Facade --> Schema
    Facade --> Contradiction
    GraphAPI --> Projections
    VectorAPI --> Projections
    TemporalAPI --> EmbeddedCore
    GraphAPI --> EmbeddedCore
    VectorAPI --> EmbeddedCore
    Schema --> EmbeddedCore
    Projections --> EmbeddedCore

    style Facade fill:#2d6a4f,stroke:#1b4332,color:#d8f3dc
    style GraphAPI fill:#40916c,stroke:#2d6a4f,color:#d8f3dc
    style VectorAPI fill:#40916c,stroke:#2d6a4f,color:#d8f3dc
    style TemporalAPI fill:#40916c,stroke:#2d6a4f,color:#d8f3dc
    style Schema fill:#52796f,stroke:#354f52,color:#cad2c5
    style Contradiction fill:#52796f,stroke:#354f52,color:#cad2c5
    style Projections fill:#264653,stroke:#2a9d8f,color:#e9f5db
    style EmbeddedCore fill:#1b4332,stroke:#081c15,color:#d8f3dc
```

**Key points:**
- Facade is the public entry point. Cross-cutting methods like `remember()` and `recall()` live here.
- Graph, Vector, and Temporal APIs handle domain-specific operations.
- All write paths go through EmbeddedCore (`ingest` / `ingest_batch`), which handles WAL durability.
- Projections are registered with EmbeddedCore and process events as they are written.

## 3. Component Diagram — Projection Layer

Each projection, its data structure, and the event types it processes.

```mermaid
graph TD
    subgraph EventFlow["Event Flow"]
        WAL["WAL<br/><i>Append-only log<br/>CRC32 + fsync</i>"]
        PM["Projection Manager<br/><i>EventStore::register_projection<br/>Dispatches events to all<br/>registered projections</i>"]
    end

    subgraph Projections["Prime Projections"]
        NS["NodeStateProjection<br/><i>DashMap&lt;entity_id, NodeEntry&gt;<br/>Merged node properties,<br/>soft delete tracking</i>"]

        NTI["NodeTypeIndexProjection<br/><i>DashMap&lt;node_type, Vec&lt;entity_id&gt;&gt;<br/>Type-based node lookup</i>"]

        ADJ["AdjacencyListProjection<br/><i>DashMap&lt;source, Vec&lt;AdjEntry&gt;&gt;<br/>Forward edge index</i>"]

        REV["ReverseIndexProjection<br/><i>DashMap&lt;target, Vec&lt;AdjEntry&gt;&gt;<br/>Reverse edge index</i>"]

        GS["GraphStatsProjection<br/><i>AtomicUsize counters<br/>+ DashMap&lt;type/relation, count&gt;<br/>O(1) statistics</i>"]

        SCH["SchemaProjection<br/><i>DashMap&lt;type_name, JSON Schema&gt;<br/>Validation rules</i>"]

        CD["ContradictionDetection<br/><i>Exclusive relation tracking,<br/>conflict pairs</i>"]

        VI["VectorIndexProjection<br/><i>DashMap&lt;entity_id, VectorRecord&gt;<br/>+ HNSW (instant-distance)<br/>Lazy rebuild on search</i>"]
    end

    subgraph Checkpoints["Checkpoint Flow"]
        SNAP["snapshot() → JSON"]
        REST["restore(JSON) → state"]
        PAR["Parquet<br/><i>Periodic flush<br/>Snappy compression</i>"]
    end

    WAL -->|"Event written"| PM
    PM -->|"prime.node.created<br/>prime.node.updated<br/>prime.node.deleted"| NS
    PM -->|"prime.node.created<br/>prime.node.deleted"| NTI
    PM -->|"prime.edge.created<br/>prime.edge.deleted"| ADJ
    PM -->|"prime.edge.created<br/>prime.edge.deleted"| REV
    PM -->|"prime.node.* + prime.edge.*"| GS
    PM -->|"prime.schema.registered"| SCH
    PM -->|"prime.edge.created"| CD
    PM -->|"prime.vector.stored<br/>prime.vector.deleted"| VI

    NS --> SNAP
    ADJ --> SNAP
    REV --> SNAP
    VI --> SNAP
    GS --> SNAP
    SNAP --> PAR
    PAR --> REST
    REST --> NS
    REST --> ADJ
    REST --> REV
    REST --> VI
    REST --> GS

    style WAL fill:#1b4332,stroke:#081c15,color:#d8f3dc
    style PM fill:#264653,stroke:#2a9d8f,color:#e9f5db
    style NS fill:#2d6a4f,stroke:#1b4332,color:#d8f3dc
    style NTI fill:#2d6a4f,stroke:#1b4332,color:#d8f3dc
    style ADJ fill:#40916c,stroke:#2d6a4f,color:#d8f3dc
    style REV fill:#40916c,stroke:#2d6a4f,color:#d8f3dc
    style GS fill:#52796f,stroke:#354f52,color:#cad2c5
    style SCH fill:#52796f,stroke:#354f52,color:#cad2c5
    style CD fill:#52796f,stroke:#354f52,color:#cad2c5
    style VI fill:#457b9d,stroke:#1d3557,color:#f1faee
    style SNAP fill:#6c757d,stroke:#495057,color:#f8f9fa
    style REST fill:#6c757d,stroke:#495057,color:#f8f9fa
    style PAR fill:#343a40,stroke:#212529,color:#f8f9fa
```

**Key points:**
- The Projection Manager dispatches each event to all registered projections. Each projection filters by event type.
- NodeStateProjection and GraphStatsProjection process all node and edge events. Adjacency projections only process edge events. VectorIndexProjection only processes vector events.
- All projections support `snapshot()` / `restore()` for checkpoint-accelerated startup. Snapshots are serialized to JSON and stored alongside Parquet files.
- VectorIndexProjection is unique: its HNSW index is rebuilt lazily on search, not on every event. The DashMap is the source of truth; the HNSW is a derived cache.

## 4. Data Flow Diagram — `prime.remember()`

Full lifecycle of `prime.remember("CRDTs enable replication", vector, "concept", props, [("node:project:1", "relates_to")])`.

```mermaid
sequenceDiagram
    participant Caller
    participant Prime as Prime Facade
    participant EC as EmbeddedCore
    participant WAL as WAL
    participant NS as NodeState
    participant NTI as NodeTypeIndex
    participant GS as GraphStats
    participant ADJ as Adjacency
    participant REV as ReverseIndex
    participant VI as VectorIndex

    Note over Caller,VI: Step 1: Create Node

    Caller->>Prime: remember("CRDTs enable...", vec, "concept", props, relations)
    Prime->>Prime: add_node("concept", props)
    Prime->>Prime: schema.validate_node("concept", props)
    Prime->>EC: ingest(NODE_CREATED, entity_id="node:concept:{uuid}")

    EC->>WAL: append(event, CRC32, fsync)
    WAL-->>EC: offset

    EC->>NS: process(NODE_CREATED)
    Note right of NS: DashMap.insert(<br/>"node:concept:{uuid}",<br/>NodeEntry{props, deleted:false})

    EC->>NTI: process(NODE_CREATED)
    Note right of NTI: DashMap["concept"]<br/>.push("node:concept:{uuid}")

    EC->>GS: process(NODE_CREATED)
    Note right of GS: total_nodes += 1<br/>nodes_by_type["concept"] += 1

    EC-->>Prime: Ok(NodeId)

    Note over Caller,VI: Step 2: Store Embedding

    Prime->>Prime: embed("node:concept:{uuid}", "CRDTs enable...", vector)
    Prime->>EC: ingest(VECTOR_STORED, entity_id="vec:node:concept:{uuid}")
    Note right of EC: payload: {text, dimensions}<br/>metadata: {embedding: [...]}

    EC->>WAL: append(event)
    EC->>VI: process(VECTOR_STORED)
    Note right of VI: DashMap.insert(<br/>"vec:node:concept:{uuid}",<br/>VectorRecord{vector, text})<br/>generation += 1

    EC-->>Prime: Ok(())

    Note over Caller,VI: Step 3: Create Edges

    Prime->>Prime: add_edge("node:concept:{uuid}", "node:project:1", "relates_to")
    Prime->>Prime: validate source & target exist
    Prime->>EC: ingest(EDGE_CREATED, entity_id="edge:{edge_uuid}")
    Note right of EC: payload: {id, source,<br/>target, relation}

    EC->>WAL: append(event)

    EC->>ADJ: process(EDGE_CREATED)
    Note right of ADJ: DashMap["node:concept:{uuid}"]<br/>.push(AdjEntry{<br/>peer:"node:project:1",<br/>relation:"relates_to"})

    EC->>REV: process(EDGE_CREATED)
    Note right of REV: DashMap["node:project:1"]<br/>.push(AdjEntry{<br/>peer:"node:concept:{uuid}",<br/>relation:"relates_to"})

    EC->>GS: process(EDGE_CREATED)
    Note right of GS: total_edges += 1<br/>edges_by_relation["relates_to"] += 1

    EC-->>Prime: Ok(EdgeId)
    Prime-->>Caller: Ok("node:concept:{uuid}")
```

**Key points:**
- `remember()` is a convenience method that orchestrates three operations: node creation, vector storage, and edge creation.
- Each operation produces one event that is written to the WAL and dispatched to all relevant projections.
- The NODE_CREATED event updates three projections: NodeState (properties), NodeTypeIndex (type lookup), and GraphStats (counters).
- The VECTOR_STORED event updates VectorIndex. The HNSW index is not rebuilt immediately — it is marked dirty (generation incremented) and rebuilt lazily on the next `search()` call.
- The EDGE_CREATED event updates both Adjacency (forward) and ReverseIndex (reverse) projections, plus GraphStats.
- All events are durable in the WAL before projections are updated. If the process crashes after WAL write but before projection update, projections are rebuilt from the WAL on restart.
