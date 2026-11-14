//! Typed node and edge kinds for Prime, with per-tenant enforcement.
//!
//! Kinds are stored as `prime.schema.registered` events and enforcement as
//! `prime.enforcement.set` events, both folded here per event tenant, so they
//! survive restarts and a hosted tenant only ever sees its own. Replay folds
//! these events without validating anything: a write accepted once stays
//! accepted.
//!
//! A kind's `schema` value keeps the legacy `{"required": [...]}` shape and
//! adds typed properties and edge constraints:
//!
//! ```json
//! {
//!   "required": ["name"],
//!   "properties": {
//!     "age":    {"kind": "integer", "min": 0, "max": 150},
//!     "status": {"kind": "enum", "values": ["lead", "customer"]},
//!     "name":   {"kind": "string", "max_length": 200, "required": true,
//!                "requires_assertion": ["source", "confidence"]},
//!     "born":   {"kind": "datetime"},
//!     "employer": {"kind": "node_ref", "node_types": ["company"]}
//!   },
//!   "from_types": ["person"], "to_types": ["company"], "cardinality": "one"
//! }
//! ```
//!
//! The legacy `required` list is enforced in every mode. Everything else is
//! enforced by [`EnforcementMode::Strict`], recorded by
//! [`EnforcementMode::Warn`] and ignored by [`EnforcementMode::Open`].
//! Embedded and hosted writes both go through [`SchemaProjection::check_node_write`]
//! and [`SchemaProjection::check_edge_write`].

use std::{collections::BTreeMap, sync::Arc};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    assertion::Assertion,
    error::{PrimeError, PrimeResult},
    projections::{AdjacencyListProjection, NodeStateProjection, charge},
};
use crate::{
    application::services::{projection::Projection, schema::CompatibilityMode},
    domain::entities::Event,
    error::Result,
    infrastructure::memory::ledger::Tally,
};

/// Event type for kind registration.
pub const SCHEMA_REGISTERED: &str = "prime.schema.registered";
/// Event type selecting a tenant's enforcement mode.
pub const ENFORCEMENT_SET: &str = "prime.enforcement.set";
/// Tenant an embedded store's events carry.
pub const DEFAULT_TENANT: &str = "default";
/// Compatibility rule every kind re-registration is checked under.
pub const KIND_COMPATIBILITY: CompatibilityMode = CompatibilityMode::Backward;

fn default_tenant() -> String {
    DEFAULT_TENANT.to_string()
}

fn default_version() -> u32 {
    1
}

/// A registered kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaEntry {
    /// The node type or edge relation this kind applies to.
    pub type_name: String,
    pub kind: SchemaKind,
    /// The kind definition as registered.
    pub schema: Value,
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_tenant")]
    pub tenant: String,
}

/// Whether the schema applies to nodes or edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SchemaKind {
    Node,
    Edge,
}

impl SchemaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Edge => "edge",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "node" => Some(Self::Node),
            "edge" => Some(Self::Edge),
            _ => None,
        }
    }
}

/// How strictly a tenant's typed kinds are enforced. A tenant that never set a
/// mode is under `warn`: nothing is refused, every violation is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementMode {
    /// Legacy required fields only.
    Open,
    /// Violations are recorded on the accepted event.
    #[default]
    Warn,
    /// Violations and unregistered types are refused.
    Strict,
}

impl EnforcementMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Warn => "warn",
            Self::Strict => "strict",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "warn" => Some(Self::Warn),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }
}

/// Missing legacy required fields. Enforced in every mode.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub type_name: String,
    pub missing_fields: Vec<String>,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "validation failed for {}: missing required fields: {}",
            self.type_name,
            self.missing_fields.join(", ")
        )
    }
}

/// One typed-constraint violation: which field, what was expected, what was
/// received.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Violation {
    pub type_name: String,
    pub field: String,
    pub expected: String,
    pub received: Value,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}: expected {}, received {}",
            self.type_name, self.field, self.expected, self.received
        )
    }
}

impl Violation {
    fn new(type_name: &str, field: &str, expected: impl Into<String>, received: Value) -> Self {
        Self {
            type_name: type_name.to_string(),
            field: field.to_string(),
            expected: expected.into(),
            received,
        }
    }
}

/// Outcome of an accepted write check.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WriteCheck {
    /// Version of the kind the write was validated against.
    pub kind_version: Option<u32>,
    /// Violations accepted under `warn`.
    pub warnings: Vec<Violation>,
}

/// Value kind of a declared property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyKind {
    String,
    Number,
    Integer,
    Boolean,
    Datetime,
    NodeRef,
    Enum,
}

impl PropertyKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::Datetime => "datetime",
            Self::NodeRef => "node_ref",
            Self::Enum => "enum",
        }
    }
}

/// A declared property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyDef {
    pub kind: PropertyKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub max_length: Option<usize>,
    /// Allowed values for `enum`.
    #[serde(default)]
    pub values: Vec<Value>,
    /// Allowed target node types for `node_ref`; empty allows any.
    #[serde(default)]
    pub node_types: Vec<String>,
    /// Assertion fields a write setting this property must carry:
    /// `source`, `actor`, `model`, `run_id`, `derived_from`, `confidence`.
    #[serde(default)]
    pub requires_assertion: Vec<String>,
}

/// Edge cardinality per source node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    One,
    #[default]
    Many,
}

/// A parsed kind definition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KindDef {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, PropertyDef>,
    #[serde(default)]
    pub from_types: Vec<String>,
    #[serde(default)]
    pub to_types: Vec<String>,
    #[serde(default)]
    pub cardinality: Cardinality,
}

const ASSERTION_FIELDS: [&str; 6] = [
    "source",
    "actor",
    "model",
    "run_id",
    "derived_from",
    "confidence",
];

impl KindDef {
    /// Parse and check a definition at registration time.
    /// A property entry without a `kind` key is a legacy JSON-Schema
    /// annotation (`{"type": "string"}`) that was never enforced; it stays
    /// untyped so existing registrations keep being accepted.
    pub fn parse(schema: &Value) -> std::result::Result<Self, String> {
        let mut schema = schema.clone();
        if let Some(Value::Object(props)) = schema.get_mut("properties") {
            props.retain(|_, p| p.get("kind").is_some());
        }
        let def: Self = serde_json::from_value(schema).map_err(|e| e.to_string())?;
        for (name, prop) in &def.properties {
            if prop.kind == PropertyKind::Enum && prop.values.is_empty() {
                return Err(format!("property {name}: enum requires non-empty values"));
            }
            if let (Some(min), Some(max)) = (prop.min, prop.max)
                && min > max
            {
                return Err(format!("property {name}: min {min} exceeds max {max}"));
            }
            if let Some(bad) = prop
                .requires_assertion
                .iter()
                .find(|f| !ASSERTION_FIELDS.contains(&f.as_str()))
            {
                return Err(format!(
                    "property {name}: requires_assertion names unknown field {bad}"
                ));
            }
        }
        Ok(def)
    }

    /// Lenient parse for replayed definitions; an unreadable one keeps only
    /// its legacy required list.
    fn parse_lenient(schema: &Value) -> Self {
        Self::parse(schema).unwrap_or_else(|_| Self {
            required: legacy_required(schema),
            ..Self::default()
        })
    }

    /// Changes in `new` that would refuse a write `self` accepted.
    pub fn breaking_changes(&self, new: &Self) -> Vec<String> {
        let mut out = Vec::new();
        for f in &new.required {
            if !self.required.contains(f) {
                out.push(format!("required field {f} added"));
            }
        }
        for (name, np) in &new.properties {
            let Some(op) = self.properties.get(name) else {
                if np.required {
                    out.push(format!("required property {name} added"));
                }
                continue;
            };
            if np.kind != op.kind {
                out.push(format!(
                    "property {name} kind changed from {} to {}",
                    op.kind.as_str(),
                    np.kind.as_str()
                ));
            }
            if np.required && !op.required {
                out.push(format!("property {name} became required"));
            }
            if tighter_lower(op.min, np.min) {
                out.push(format!("property {name} min raised"));
            }
            if tighter_upper(op.max, np.max) {
                out.push(format!("property {name} max lowered"));
            }
            if match (op.max_length, np.max_length) {
                (_, None) => false,
                (None, Some(_)) => true,
                (Some(o), Some(n)) => n < o,
            } {
                out.push(format!("property {name} max_length lowered"));
            }
            if op.values.iter().any(|v| !np.values.contains(v)) {
                out.push(format!("property {name} enum values removed"));
            }
            if narrowed(&op.node_types, &np.node_types) {
                out.push(format!("property {name} node_types narrowed"));
            }
            if np
                .requires_assertion
                .iter()
                .any(|f| !op.requires_assertion.contains(f))
            {
                out.push(format!("property {name} requires_assertion extended"));
            }
        }
        if narrowed(&self.from_types, &new.from_types) {
            out.push("from_types narrowed".to_string());
        }
        if narrowed(&self.to_types, &new.to_types) {
            out.push("to_types narrowed".to_string());
        }
        if self.cardinality == Cardinality::Many && new.cardinality == Cardinality::One {
            out.push("cardinality changed from many to one".to_string());
        }
        out
    }
}

fn tighter_lower(old: Option<f64>, new: Option<f64>) -> bool {
    match (old, new) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(o), Some(n)) => n > o,
    }
}

fn tighter_upper(old: Option<f64>, new: Option<f64>) -> bool {
    match (old, new) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(o), Some(n)) => n < o,
    }
}

/// Empty lists allow anything, so narrowing is any restriction of the set.
fn narrowed(old: &[String], new: &[String]) -> bool {
    if new.is_empty() {
        return false;
    }
    old.is_empty() || old.iter().any(|t| !new.contains(t))
}

fn legacy_required(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Graph reads the validator needs, satisfied by both the embedded projections
/// and a hosted tenant's warm bundle.
pub trait GraphLookup {
    /// Node type of a live node, `None` if absent or deleted.
    fn live_node_type(&self, entity_id: &str) -> Option<String>;
    /// Targets of live `relation` edges leaving `source`.
    fn outgoing_targets(&self, source: &str, relation: &str) -> Vec<String>;
}

/// [`GraphLookup`] over a node-state and forward-adjacency projection pair.
pub struct ProjectionLookup<'a> {
    pub node_state: &'a NodeStateProjection,
    pub adjacency: &'a AdjacencyListProjection,
}

impl GraphLookup for ProjectionLookup<'_> {
    fn live_node_type(&self, entity_id: &str) -> Option<String> {
        self.node_state
            .get_node(entity_id)
            .filter(|n| !n.deleted)
            .map(|n| n.node_type)
    }

    fn outgoing_targets(&self, source: &str, relation: &str) -> Vec<String> {
        self.adjacency
            .outgoing(source)
            .into_iter()
            .filter(|e| e.relation == relation)
            .map(|e| e.peer)
            .collect()
    }
}

type KindKey = (String, SchemaKind, String);

/// Folds kind registrations and enforcement changes per tenant and validates
/// writes against them.
pub struct SchemaProjection {
    name: String,
    kinds: Arc<DashMap<KindKey, SchemaEntry>>,
    enforcement: Arc<DashMap<String, (DateTime<Utc>, EnforcementMode)>>,
    charge: Tally,
}

fn kind_bytes(entry: &SchemaEntry) -> u64 {
    charge::text(&entry.tenant) + charge::text(&entry.type_name) + charge::json(&entry.schema)
}

impl SchemaProjection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kinds: Arc::new(DashMap::new()),
            enforcement: Arc::new(DashMap::new()),
            charge: charge::tally(),
        }
    }

    fn store_kind(&self, key: KindKey, entry: SchemaEntry) {
        let owned = kind_bytes(&entry);
        match self.kinds.insert(key, entry) {
            Some(old) => self.charge.adjust(kind_bytes(&old), owned),
            None => self
                .charge
                .add(charge::entry::<KindKey, SchemaEntry>(owned)),
        }
    }

    fn set_enforcement(&self, tenant: String, value: (DateTime<Utc>, EnforcementMode)) {
        let key_bytes = charge::text(&tenant);
        if self.enforcement.insert(tenant, value).is_none() {
            self.charge
                .add(charge::entry::<String, (DateTime<Utc>, EnforcementMode)>(
                    key_bytes,
                ));
        }
    }

    fn insert(&self, tenant: &str, kind: SchemaKind, type_name: &str, schema: Value, version: u32) {
        self.store_kind(
            (tenant.to_string(), kind, type_name.to_string()),
            SchemaEntry {
                type_name: type_name.to_string(),
                kind,
                schema,
                version,
                tenant: tenant.to_string(),
            },
        );
    }

    /// Register a node schema for the default tenant (no compatibility check).
    pub fn register_node_schema(&self, node_type: &str, schema: Value) {
        let version = self.next_version(DEFAULT_TENANT, SchemaKind::Node, node_type);
        self.insert(DEFAULT_TENANT, SchemaKind::Node, node_type, schema, version);
    }

    /// Register an edge schema for the default tenant (no compatibility check).
    pub fn register_edge_schema(&self, relation: &str, schema: Value) {
        let version = self.next_version(DEFAULT_TENANT, SchemaKind::Edge, relation);
        self.insert(DEFAULT_TENANT, SchemaKind::Edge, relation, schema, version);
    }

    fn next_version(&self, tenant: &str, kind: SchemaKind, type_name: &str) -> u32 {
        self.kind(tenant, kind, type_name)
            .map_or(1, |e| e.version + 1)
    }

    /// The current entry for a kind.
    pub fn kind(&self, tenant: &str, kind: SchemaKind, type_name: &str) -> Option<SchemaEntry> {
        self.kinds
            .get(&(tenant.to_string(), kind, type_name.to_string()))
            .map(|e| e.value().clone())
    }

    /// Check a proposed (re-)registration and return the version it would get.
    /// Refuses malformed definitions and breaking changes.
    pub fn plan_registration(
        &self,
        tenant: &str,
        kind: SchemaKind,
        type_name: &str,
        schema: &Value,
    ) -> PrimeResult<u32> {
        let new = KindDef::parse(schema).map_err(|e| {
            PrimeError::ValidationFailed(format!("invalid {} kind {type_name}: {e}", kind.as_str()))
        })?;
        let Some(existing) = self.kind(tenant, kind, type_name) else {
            return Ok(1);
        };
        let old = KindDef::parse_lenient(&existing.schema);
        let breaking = old.breaking_changes(&new);
        if !breaking.is_empty() {
            return Err(PrimeError::ValidationFailed(format!(
                "kind {type_name} v{} re-registration refused under {:?} compatibility: {}",
                existing.version,
                KIND_COMPATIBILITY,
                breaking.join("; ")
            )));
        }
        Ok(existing.version + 1)
    }

    /// Enforcement mode for `tenant`; `warn` until one is set.
    pub fn enforcement(&self, tenant: &str) -> EnforcementMode {
        self.enforcement
            .get(tenant)
            .map(|e| e.value().1)
            .unwrap_or_default()
    }

    /// Validate node properties against the default tenant's legacy required
    /// list. Returns `Ok(())` if no schema is registered.
    pub fn validate_node(
        &self,
        node_type: &str,
        properties: &Value,
    ) -> std::result::Result<(), ValidationError> {
        match self.kind(DEFAULT_TENANT, SchemaKind::Node, node_type) {
            Some(entry) => validate_required(node_type, &entry.schema, properties),
            None => Ok(()),
        }
    }

    /// Validate edge properties against the default tenant's legacy required
    /// list. Returns `Ok(())` if no schema is registered.
    pub fn validate_edge(
        &self,
        relation: &str,
        properties: Option<&Value>,
    ) -> std::result::Result<(), ValidationError> {
        match self.kind(DEFAULT_TENANT, SchemaKind::Edge, relation) {
            Some(entry) => {
                let props = properties.cloned().unwrap_or_else(|| json!({}));
                validate_required(relation, &entry.schema, &props)
            }
            None => Ok(()),
        }
    }

    /// Every registered kind, all tenants.
    pub fn schemas(&self) -> Vec<SchemaEntry> {
        let mut out: Vec<SchemaEntry> = self.kinds.iter().map(|e| e.value().clone()).collect();
        out.sort_by(|a, b| {
            (&a.tenant, a.kind.as_str(), &a.type_name).cmp(&(
                &b.tenant,
                b.kind.as_str(),
                &b.type_name,
            ))
        });
        out
    }

    /// Registered kinds for one tenant.
    pub fn schemas_for(&self, tenant: &str) -> Vec<SchemaEntry> {
        self.schemas()
            .into_iter()
            .filter(|e| e.tenant == tenant)
            .collect()
    }

    /// Edge relations with `cardinality: one` for `tenant`.
    pub fn exclusive_relations(&self, tenant: &str) -> Vec<String> {
        self.schemas_for(tenant)
            .into_iter()
            .filter(|e| {
                e.kind == SchemaKind::Edge
                    && KindDef::parse_lenient(&e.schema).cardinality == Cardinality::One
            })
            .map(|e| e.type_name)
            .collect()
    }

    /// Check a node create or update. `merged` is the node's properties after
    /// the write, `changed` the properties the write sets.
    pub fn check_node_write(
        &self,
        tenant: &str,
        node_type: &str,
        merged: &Value,
        changed: &Value,
        assertion: Option<&Assertion>,
        graph: &dyn GraphLookup,
    ) -> PrimeResult<WriteCheck> {
        let mode = self.enforcement(tenant);
        let Some(entry) = self.kind(tenant, SchemaKind::Node, node_type) else {
            return finish(
                mode,
                None,
                unknown_type(mode, node_type, "node_type", "a registered node kind"),
            );
        };
        validate_required(node_type, &entry.schema, merged)?;
        if mode == EnforcementMode::Open {
            return Ok(WriteCheck {
                kind_version: Some(entry.version),
                warnings: Vec::new(),
            });
        }
        let def = KindDef::parse_lenient(&entry.schema);
        let violations = property_violations(node_type, &def, merged, changed, assertion, graph);
        finish(mode, Some(entry.version), violations)
    }

    /// Check an edge create. Endpoint existence is checked by the caller.
    #[allow(clippy::too_many_arguments)]
    pub fn check_edge_write(
        &self,
        tenant: &str,
        relation: &str,
        source: &str,
        target: &str,
        properties: Option<&Value>,
        assertion: Option<&Assertion>,
        graph: &dyn GraphLookup,
    ) -> PrimeResult<WriteCheck> {
        let mode = self.enforcement(tenant);
        let Some(entry) = self.kind(tenant, SchemaKind::Edge, relation) else {
            return finish(
                mode,
                None,
                unknown_type(mode, relation, "relation", "a registered edge kind"),
            );
        };
        let props = properties.cloned().unwrap_or_else(|| json!({}));
        validate_required(relation, &entry.schema, &props)?;
        if mode == EnforcementMode::Open {
            return Ok(WriteCheck {
                kind_version: Some(entry.version),
                warnings: Vec::new(),
            });
        }
        let def = KindDef::parse_lenient(&entry.schema);
        let mut violations = Vec::new();
        for (field, end, allowed) in [
            ("source", source, &def.from_types),
            ("target", target, &def.to_types),
        ] {
            if allowed.is_empty() {
                continue;
            }
            let end_type = graph.live_node_type(end);
            if !end_type.as_ref().is_some_and(|t| allowed.contains(t)) {
                violations.push(Violation::new(
                    relation,
                    field,
                    format!("a node of type one of [{}]", allowed.join(", ")),
                    end_type.map_or(Value::Null, Value::String),
                ));
            }
        }
        if def.cardinality == Cardinality::One {
            let others: Vec<String> = graph
                .outgoing_targets(source, relation)
                .into_iter()
                .filter(|t| t != target)
                .collect();
            if !others.is_empty() {
                violations.push(Violation::new(
                    relation,
                    "target",
                    format!(
                        "at most one target per source (cardinality one; already {})",
                        others[0]
                    ),
                    json!(target),
                ));
            }
        }
        violations.extend(property_violations(
            relation, &def, &props, &props, assertion, graph,
        ));
        finish(mode, Some(entry.version), violations)
    }
}

fn unknown_type(
    mode: EnforcementMode,
    type_name: &str,
    field: &str,
    expected: &str,
) -> Vec<Violation> {
    if mode == EnforcementMode::Open {
        Vec::new()
    } else {
        vec![Violation::new(type_name, field, expected, json!(type_name))]
    }
}

fn finish(
    mode: EnforcementMode,
    kind_version: Option<u32>,
    violations: Vec<Violation>,
) -> PrimeResult<WriteCheck> {
    match mode {
        EnforcementMode::Strict => match violations.into_iter().next() {
            Some(first) => Err(PrimeError::SchemaViolation(Box::new(first))),
            None => Ok(WriteCheck {
                kind_version,
                warnings: Vec::new(),
            }),
        },
        EnforcementMode::Warn => Ok(WriteCheck {
            kind_version,
            warnings: violations,
        }),
        EnforcementMode::Open => Ok(WriteCheck {
            kind_version,
            warnings: Vec::new(),
        }),
    }
}

fn property_violations(
    type_name: &str,
    def: &KindDef,
    merged: &Value,
    changed: &Value,
    assertion: Option<&Assertion>,
    graph: &dyn GraphLookup,
) -> Vec<Violation> {
    let mut out = Vec::new();
    for (name, prop) in &def.properties {
        let value = merged.get(name).filter(|v| !v.is_null());
        match value {
            None if prop.required => out.push(Violation::new(
                type_name,
                name,
                format!("a required {}", prop.kind.as_str()),
                Value::Null,
            )),
            None => {}
            Some(v) => {
                if let Some(expected) = value_violation(prop, v, graph) {
                    out.push(Violation::new(type_name, name, expected, v.clone()));
                }
            }
        }
        if changed.get(name).is_some()
            && let Some(missing) = prop
                .requires_assertion
                .iter()
                .find(|f| !assertion_has(assertion, f))
        {
            out.push(Violation::new(
                type_name,
                name,
                format!("an assertion carrying {missing}"),
                assertion.map_or(Value::Null, |a| {
                    serde_json::to_value(a).unwrap_or(Value::Null)
                }),
            ));
        }
    }
    out
}

fn assertion_has(assertion: Option<&Assertion>, field: &str) -> bool {
    let Some(a) = assertion else {
        return false;
    };
    match field {
        "source" => true,
        "actor" => a.actor.is_some(),
        "model" => a.model.is_some(),
        "run_id" => a.run_id.is_some(),
        "derived_from" => !a.derived_from.is_empty(),
        "confidence" => a.confidence.is_some(),
        _ => false,
    }
}

/// `Some(expected)` when `v` breaks `prop`.
fn value_violation(prop: &PropertyDef, v: &Value, graph: &dyn GraphLookup) -> Option<String> {
    match prop.kind {
        PropertyKind::String => {
            let Some(s) = v.as_str() else {
                return Some("string".to_string());
            };
            prop.max_length
                .filter(|max| s.chars().count() > *max)
                .map(|max| format!("string of at most {max} characters"))
        }
        PropertyKind::Number | PropertyKind::Integer => {
            let is_integer = v.is_i64() || v.is_u64();
            if prop.kind == PropertyKind::Integer && !is_integer {
                return Some("integer".to_string());
            }
            let Some(n) = v.as_f64() else {
                return Some("number".to_string());
            };
            let out_of_range = prop.min.is_some_and(|m| n < m) || prop.max.is_some_and(|m| n > m);
            out_of_range.then(|| {
                format!(
                    "{} in {}..={}",
                    prop.kind.as_str(),
                    prop.min
                        .map_or_else(|| "-inf".to_string(), |m| m.to_string()),
                    prop.max
                        .map_or_else(|| "inf".to_string(), |m| m.to_string()),
                )
            })
        }
        PropertyKind::Boolean => (!v.is_boolean()).then(|| "boolean".to_string()),
        PropertyKind::Datetime => {
            let ok = v
                .as_str()
                .is_some_and(|s| DateTime::parse_from_rfc3339(s).is_ok());
            (!ok).then(|| "RFC 3339 datetime".to_string())
        }
        PropertyKind::Enum => (!prop.values.contains(v)).then(|| {
            let allowed: Vec<String> = prop.values.iter().map(Value::to_string).collect();
            format!("one of [{}]", allowed.join(", "))
        }),
        PropertyKind::NodeRef => {
            let target_type = v.as_str().and_then(|s| graph.live_node_type(s));
            let ok = target_type
                .as_ref()
                .is_some_and(|t| prop.node_types.is_empty() || prop.node_types.contains(t));
            (!ok).then(|| {
                if prop.node_types.is_empty() {
                    "a live node id".to_string()
                } else {
                    format!(
                        "a live node id of type one of [{}]",
                        prop.node_types.join(", ")
                    )
                }
            })
        }
    }
}

/// The legacy `required` list check, unchanged in every mode.
fn validate_required(
    type_name: &str,
    schema: &Value,
    properties: &Value,
) -> std::result::Result<(), ValidationError> {
    let missing: Vec<String> = legacy_required(schema)
        .into_iter()
        .filter(|f| properties.get(f).is_none())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(ValidationError {
            type_name: type_name.to_string(),
            missing_fields: missing,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct EnforcementSnapshot {
    tenant: String,
    at: DateTime<Utc>,
    mode: EnforcementMode,
}

#[derive(Serialize, Deserialize)]
struct SchemaSnapshot {
    kinds: Vec<SchemaEntry>,
    #[serde(default)]
    enforcement: Vec<EnforcementSnapshot>,
}

impl Projection for SchemaProjection {
    fn name(&self) -> &str {
        &self.name
    }

    fn process(&self, event: &Event) -> Result<()> {
        let tenant = event.tenant_id_str();
        let payload = &event.payload;
        match event.event_type_str() {
            SCHEMA_REGISTERED => {
                let (Some(type_name), Some(kind), Some(schema)) = (
                    payload.get("type_name").and_then(Value::as_str),
                    payload
                        .get("kind")
                        .and_then(Value::as_str)
                        .and_then(SchemaKind::parse),
                    payload.get("schema"),
                ) else {
                    return Ok(());
                };
                let version = payload
                    .get("version")
                    .and_then(Value::as_u64)
                    .map_or_else(|| self.next_version(tenant, kind, type_name), |v| v as u32);
                self.insert(tenant, kind, type_name, schema.clone(), version);
            }
            ENFORCEMENT_SET => {
                let Some(mode) = payload
                    .get("mode")
                    .and_then(Value::as_str)
                    .and_then(EnforcementMode::parse)
                else {
                    return Ok(());
                };
                let at = event.timestamp();
                let newer = self
                    .enforcement
                    .get(tenant)
                    .is_none_or(|e| at >= e.value().0);
                if newer {
                    self.set_enforcement(tenant.to_string(), (at, mode));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn get_state(&self, _entity_id: &str) -> Option<Value> {
        serde_json::to_value(self.schemas()).ok()
    }

    fn clear(&self) {
        self.kinds.clear();
        self.enforcement.clear();
        self.charge.take();
    }

    fn snapshot(&self) -> Option<Value> {
        let enforcement = self
            .enforcement
            .iter()
            .map(|e| EnforcementSnapshot {
                tenant: e.key().clone(),
                at: e.value().0,
                mode: e.value().1,
            })
            .collect();
        serde_json::to_value(SchemaSnapshot {
            kinds: self.schemas(),
            enforcement,
        })
        .ok()
    }

    fn restore(&self, snapshot: &Value) -> Result<()> {
        let snap: SchemaSnapshot = if snapshot.is_array() {
            SchemaSnapshot {
                kinds: serde_json::from_value(snapshot.clone())
                    .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?,
                enforcement: Vec::new(),
            }
        } else {
            serde_json::from_value(snapshot.clone())
                .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?
        };
        self.clear();
        for entry in snap.kinds {
            self.store_kind(
                (entry.tenant.clone(), entry.kind, entry.type_name.clone()),
                entry,
            );
        }
        for e in snap.enforcement {
            self.set_enforcement(e.tenant, (e.at, e.mode));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod kind_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_validate_node_with_required_fields_passes() {
        let proj = SchemaProjection::new("schema");
        proj.register_node_schema("person", serde_json::json!({"required": ["name"]}));

        let result = proj.validate_node("person", &serde_json::json!({"name": "Alice"}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_node_missing_required_field_fails() {
        let proj = SchemaProjection::new("schema");
        proj.register_node_schema("person", serde_json::json!({"required": ["name", "email"]}));

        let result = proj.validate_node("person", &serde_json::json!({"name": "Alice"}));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.missing_fields, vec!["email"]);
    }

    #[test]
    fn test_validate_node_no_schema_accepts_anything() {
        let proj = SchemaProjection::new("schema");

        let result = proj.validate_node("unknown_type", &serde_json::json!({}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_edge_with_schema() {
        let proj = SchemaProjection::new("schema");
        proj.register_edge_schema("works_on", serde_json::json!({"required": ["since"]}));

        let result = proj.validate_edge("works_on", Some(&serde_json::json!({"since": "2026-01"})));
        assert!(result.is_ok());

        let result = proj.validate_edge("works_on", Some(&serde_json::json!({})));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_edge_no_properties_fails_if_required() {
        let proj = SchemaProjection::new("schema");
        proj.register_edge_schema("works_on", serde_json::json!({"required": ["since"]}));

        let result = proj.validate_edge("works_on", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_edge_no_schema_accepts_anything() {
        let proj = SchemaProjection::new("schema");

        let result = proj.validate_edge("unknown_relation", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_schemas_list() {
        let proj = SchemaProjection::new("schema");
        proj.register_node_schema("person", serde_json::json!({"required": ["name"]}));
        proj.register_edge_schema("works_on", serde_json::json!({"required": ["since"]}));

        let schemas = proj.schemas();
        assert_eq!(schemas.len(), 2);
    }

    #[test]
    fn test_process_schema_registered_event() {
        let proj = SchemaProjection::new("schema");

        let event = Event::reconstruct_from_strings(
            Uuid::new_v4(),
            SCHEMA_REGISTERED.to_string(),
            "schema:person".to_string(),
            "default".to_string(),
            serde_json::json!({
                "type_name": "person",
                "kind": "node",
                "schema": {"required": ["name"]},
            }),
            Utc::now(),
            None,
            1,
        );
        proj.process(&event).unwrap();

        let result = proj.validate_node("person", &serde_json::json!({}));
        assert!(result.is_err());

        let result = proj.validate_node("person", &serde_json::json!({"name": "Alice"}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_snapshot_and_restore() {
        let proj = SchemaProjection::new("schema");
        proj.register_node_schema("person", serde_json::json!({"required": ["name"]}));

        let snap = proj.snapshot().expect("snapshot should be Some");

        proj.clear();
        assert!(proj.validate_node("person", &serde_json::json!({})).is_ok()); // no schema = pass

        proj.restore(&snap).unwrap();
        assert!(
            proj.validate_node("person", &serde_json::json!({}))
                .is_err()
        ); // schema restored
    }

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError {
            type_name: "person".to_string(),
            missing_fields: vec!["name".to_string(), "email".to_string()],
        };
        assert_eq!(
            err.to_string(),
            "validation failed for person: missing required fields: name, email"
        );
    }
}
