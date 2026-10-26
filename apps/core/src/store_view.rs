//! Views: declarative read models. Core validates and stores a tenant's view
//! definitions and serves the rows; the Query Service folds the rows from the
//! event feed and commits them. Core never applies a definition's operations
//! (ADR-021).

use super::{
    EventStore,
    feed::FeedCursor,
    read_model::{
        ChangesPage, ModelChanges, ModelCommit, ModelMeta, ModelPage, ModelWatch, RowsPage,
    },
};
use crate::error::AllSourceError;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

pub const VIEW_MODEL_PREFIX: &str = "view_";
pub const VIEWS_REGISTRY: &str = "views_registry";
/// Every tenant's definitions, for the Query Service folder. Held by a
/// reserved tenant so the folder reads one model instead of every tenant's.
pub const VIEWS_INDEX: &str = "views_index";
pub const VIEWS_INDEX_TENANT: &str = "_views";
pub const MAX_VIEWS_PER_TENANT: usize = 64;
pub const MAX_DEFINITION_BYTES: usize = 8 * 1024;
const COMMIT_ATTEMPTS: u32 = 4;

const OPERATIONS: [&str; 9] = [
    "set",
    "unset",
    "delete",
    "increment",
    "max",
    "min",
    "append",
    "remove",
    "status",
];

fn enabled_flag() -> &'static AtomicBool {
    static ENABLED: OnceLock<AtomicBool> = OnceLock::new();
    ENABLED.get_or_init(|| {
        AtomicBool::new(
            std::env::var("ALLSOURCE_VIEWS")
                .is_ok_and(|value| matches!(value.trim(), "1" | "true")),
        )
    })
}

/// Off unless `ALLSOURCE_VIEWS=1`: defining a view writes to the store.
pub fn views_enabled() -> bool {
    enabled_flag().load(Ordering::Relaxed)
}

pub fn set_views_enabled(enabled: bool) {
    enabled_flag().store(enabled, Ordering::Relaxed);
}

/// Read models only the admin principal may write through the read-model API.
pub fn is_reserved_read_model(name: &str) -> bool {
    name == VIEWS_REGISTRY || name == VIEWS_INDEX || name.starts_with(VIEW_MODEL_PREFIX)
}

pub fn view_model_name(view: &str) -> String {
    format!("{VIEW_MODEL_PREFIX}{view}")
}

#[derive(Debug)]
pub enum ViewError {
    Invalid(String),
    NotFound,
    Reset { generation: u64 },
    Core(AllSourceError),
}

impl From<AllSourceError> for ViewError {
    fn from(error: AllSourceError) -> Self {
        Self::Core(error)
    }
}

type ViewResult<T> = std::result::Result<T, ViewError>;

#[derive(Debug, Clone, Serialize)]
pub struct ViewInfo {
    pub view: String,
    pub definition: Value,
    pub generation: u64,
    pub revision: u64,
    pub checkpoint: Option<FeedCursor>,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewSummary {
    pub view: String,
    pub generation: u64,
    pub revision: u64,
    pub ready: bool,
}

#[derive(Debug)]
pub struct ViewRowRead {
    pub generation: u64,
    pub meta: ModelMeta,
    pub row: Option<(Value, u64)>,
}

/// A registry row. A deleted view keeps its generation, so a redefinition
/// gets a higher one and the folder never mistakes it for the old view.
struct Registered {
    generation: u64,
    definition: Option<Value>,
}

fn registered(row: Option<&Value>) -> Registered {
    Registered {
        generation: row
            .and_then(|row| row.get("generation"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        definition: row
            .and_then(|row| row.get("definition"))
            .filter(|definition| !definition.is_null())
            .cloned(),
    }
}

pub fn validate_view_name(name: &str) -> Result<(), String> {
    if (1..=48).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    {
        Ok(())
    } else {
        Err("view name must be 1..48 characters of [a-z0-9_-]".into())
    }
}

fn only_keys(object: &Map<String, Value>, allowed: &[&str], context: &str) -> Result<(), String> {
    match object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(format!("{context} has unknown field '{key}'")),
        None => Ok(()),
    }
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 256
        && path.split('.').count() <= 16
        && path.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
}

fn valid_payload_ref(reference: &str) -> bool {
    reference.strip_prefix("payload.").is_some_and(valid_path)
}

fn valid_field(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn validate_key_template(template: &str) -> Result<(), String> {
    if template.is_empty() || template.len() > 512 {
        return Err("key must be a template of 1..512 bytes".into());
    }
    let mut rest = template;
    while let Some(open) = rest.find(['{', '}']) {
        if rest.as_bytes()[open] == b'}' {
            return Err("key has an unmatched '}'".into());
        }
        let inner = &rest[open + 1..];
        let close = inner.find('}').ok_or("key has an unclosed '{'")?;
        let segment = &inner[..close];
        if !matches!(segment, "entity_id" | "event_type" | "day:timestamp")
            && !valid_payload_ref(segment)
        {
            return Err(format!(
                "key segment '{{{segment}}}' must be entity_id, event_type, day:timestamp or \
                 payload.<path>"
            ));
        }
        rest = &inner[close + 1..];
    }
    Ok(())
}

fn validate_expr(expr: &Value) -> Result<(), String> {
    match expr {
        Value::String(reference)
            if matches!(
                reference.as_str(),
                "entity_id" | "event_type" | "timestamp" | "day:timestamp"
            ) || valid_payload_ref(reference) =>
        {
            Ok(())
        }
        Value::Object(literal) if literal.len() == 1 && literal.contains_key("literal") => {
            let size = serde_json::to_vec(&literal["literal"])
                .map_err(|error| error.to_string())?
                .len();
            if size > 1024 {
                Err("literal exceeds 1024 bytes".into())
            } else {
                Ok(())
            }
        }
        _ => Err(
            "value must be payload.<path>, entity_id, event_type, timestamp, \
                  day:timestamp or {\"literal\": <json>}"
                .into(),
        ),
    }
}

fn field_map<'a>(value: &'a Value, operation: &str) -> Result<&'a Map<String, Value>, String> {
    let fields = value
        .as_object()
        .ok_or_else(|| format!("{operation} must be an object of field to value"))?;
    if !(1..=32).contains(&fields.len()) {
        return Err(format!("{operation} must name 1..32 fields"));
    }
    if let Some(field) = fields.keys().find(|field| !valid_field(field)) {
        return Err(format!(
            "{operation} field '{field}' must be 1..64 characters of [A-Za-z0-9_-]"
        ));
    }
    Ok(fields)
}

fn validate_on(on: &str) -> Result<(), String> {
    let body = on.strip_suffix('*').unwrap_or(on);
    if on.is_empty()
        || on.len() > 128
        || body.contains('*')
        || !body.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err(
            "on must be 1..128 bytes: an exact event type, or a prefix ending in '.' or '*'".into(),
        );
    }
    Ok(())
}

fn validate_when(when: &Value) -> Result<(), String> {
    let conditions = when
        .as_object()
        .ok_or("when must be an object of path to JSON scalar")?;
    if !(1..=16).contains(&conditions.len()) {
        return Err("when must hold 1..16 conditions".into());
    }
    for (path, expected) in conditions {
        if !matches!(path.as_str(), "entity_id" | "event_type") && !valid_payload_ref(path) {
            return Err(format!(
                "when path '{path}' must be payload.<path>, entity_id or event_type"
            ));
        }
        if expected.is_array() || expected.is_object() {
            return Err(format!("when '{path}' must compare to a JSON scalar"));
        }
    }
    Ok(())
}

fn validate_status(value: &Value) -> Result<(), String> {
    let status = value.as_object().ok_or("status must be an object")?;
    only_keys(status, &["field", "from", "transitions"], "status")?;
    let field = status
        .get("field")
        .and_then(Value::as_str)
        .ok_or("status.field is required")?;
    if !valid_field(field) {
        return Err("status.field must be 1..64 characters of [A-Za-z0-9_-]".into());
    }
    match status.get("from") {
        Some(Value::String(from)) if from == "suffix" => {}
        Some(expr) => validate_expr(expr).map_err(|detail| format!("status.from: {detail}"))?,
        None => return Err("status.from must be \"suffix\" or a value expression".into()),
    }
    if let Some(transitions) = status.get("transitions") {
        let table = transitions
            .as_object()
            .ok_or("status.transitions must map a state to the states it may move to")?;
        if table.len() > 64 {
            return Err("status.transitions may list at most 64 states".into());
        }
        for (from, targets) in table {
            let targets = targets
                .as_array()
                .ok_or_else(|| format!("status.transitions['{from}'] must be an array"))?;
            if from.len() > 128
                || targets.len() > 32
                || !targets
                    .iter()
                    .all(|target| target.as_str().is_some_and(|target| target.len() <= 128))
            {
                return Err(format!(
                    "status.transitions['{from}'] must list at most 32 states of 128 bytes"
                ));
            }
        }
    }
    Ok(())
}

fn validate_op(op: &Value) -> Result<(), String> {
    let object = op.as_object().ok_or("operation must be an object")?;
    let on = object
        .get("on")
        .and_then(Value::as_str)
        .ok_or("on must be an event type or prefix")?;
    validate_on(on)?;
    if let Some(when) = object.get("when") {
        validate_when(when)?;
    }
    let mut operations = object
        .keys()
        .filter(|key| key.as_str() != "on" && key.as_str() != "when");
    let (Some(name), None) = (operations.next(), operations.next()) else {
        return Err(format!(
            "operation must carry exactly one of {}",
            OPERATIONS.join(", ")
        ));
    };
    let value = &object[name.as_str()];
    match name.as_str() {
        "set" | "max" | "min" | "append" | "remove" => {
            for expr in field_map(value, name)?.values() {
                validate_expr(expr).map_err(|detail| format!("{name}: {detail}"))?;
            }
        }
        "unset" => {
            let fields = value.as_array().ok_or("unset must be an array of fields")?;
            if !(1..=32).contains(&fields.len())
                || !fields
                    .iter()
                    .all(|field| field.as_str().is_some_and(valid_field))
            {
                return Err("unset must list 1..32 field names".into());
            }
        }
        "delete" => {
            if value != &Value::Bool(true) {
                return Err("delete must be true".into());
            }
        }
        "increment" => {
            if !field_map(value, name)?.values().all(Value::is_i64) {
                return Err("increment amounts must be integers".into());
            }
        }
        "status" => validate_status(value)?,
        other => {
            return Err(format!(
                "unknown operation '{other}'; expected one of {}",
                OPERATIONS.join(", ")
            ));
        }
    }
    Ok(())
}

/// Checks every rule of the definition; the operations are never run here.
pub fn validate_view_definition(definition: &Value) -> Result<(), String> {
    let size = serde_json::to_vec(definition)
        .map_err(|error| error.to_string())?
        .len();
    if size > MAX_DEFINITION_BYTES {
        return Err(format!("definition exceeds {MAX_DEFINITION_BYTES} bytes"));
    }
    let object = definition
        .as_object()
        .ok_or("definition must be an object")?;
    only_keys(object, &["source", "key", "ops"], "definition")?;
    let source = object
        .get("source")
        .and_then(Value::as_object)
        .ok_or("source must be an object")?;
    only_keys(source, &["prefixes"], "source")?;
    let prefixes = source
        .get("prefixes")
        .and_then(Value::as_array)
        .ok_or("source.prefixes must be an array")?;
    if !(1..=8).contains(&prefixes.len()) {
        return Err("source.prefixes must contain 1..8 prefixes".into());
    }
    if !prefixes.iter().all(|prefix| {
        prefix
            .as_str()
            .is_some_and(|p| !p.is_empty() && p.len() <= 128)
    }) {
        return Err("each source prefix must be a string of 1..128 bytes".into());
    }
    let key = object
        .get("key")
        .and_then(Value::as_str)
        .ok_or("key must be a string template")?;
    validate_key_template(key)?;
    let ops = object
        .get("ops")
        .and_then(Value::as_array)
        .ok_or("ops must be an array")?;
    if !(1..=64).contains(&ops.len()) {
        return Err("ops must contain 1..64 operations".into());
    }
    for (index, op) in ops.iter().enumerate() {
        validate_op(op).map_err(|detail| format!("ops[{index}]: {detail}"))?;
    }
    Ok(())
}

fn is_conflict(error: &AllSourceError) -> bool {
    matches!(error, AllSourceError::VersionConflict { .. })
}

impl EventStore {
    fn commit_registry(
        &self,
        tenant: &str,
        expected_revision: u64,
        view: &str,
        row: Value,
    ) -> crate::error::Result<u64> {
        let checkpoint = self.tenant_feed(tenant, None, 0)?.head;
        self.commit_core_read_model(
            tenant,
            VIEWS_REGISTRY,
            ModelCommit {
                expected_revision,
                checkpoint,
                ready: true,
                reset: false,
                changes: BTreeMap::from([(view.to_owned(), Some(row))]),
            },
        )
    }

    fn sync_view_index(
        &self,
        tenant: &str,
        view: &str,
        entry: Option<(u64, &Value)>,
    ) -> crate::error::Result<()> {
        let key = format!("{tenant}/{view}");
        let desired = entry.map(|(generation, definition)| {
            json!({"tenant_id": tenant, "view": view, "generation": generation, "definition": definition})
        });
        let mut attempt = 1;
        loop {
            let (meta, row) = self.read_model_row(VIEWS_INDEX_TENANT, VIEWS_INDEX, &key)?;
            if row.map(|(value, _)| value) == desired {
                return Ok(());
            }
            let commit = ModelCommit {
                expected_revision: meta.revision,
                checkpoint: FeedCursor::default(),
                ready: true,
                reset: false,
                changes: BTreeMap::from([(key.clone(), desired.clone())]),
            };
            match self.commit_core_read_model(VIEWS_INDEX_TENANT, VIEWS_INDEX, commit) {
                Ok(_) => return Ok(()),
                Err(error) if is_conflict(&error) && attempt < COMMIT_ATTEMPTS => attempt += 1,
                Err(error) => return Err(error),
            }
        }
    }

    fn registered_views(&self, tenant: &str) -> crate::error::Result<Vec<(String, Registered)>> {
        let mut views = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let page = self.read_model_rows(tenant, VIEWS_REGISTRY, "", after.as_deref(), 500)?;
            for row in page.rows {
                let entry = registered(row.state.as_ref());
                if entry.definition.is_some() {
                    views.push((row.key, entry));
                }
            }
            match page.next {
                Some(next) if after.as_deref() != Some(next.as_str()) => after = Some(next),
                _ => return Ok(views),
            }
        }
    }

    fn live_view(&self, tenant: &str, view: &str) -> ViewResult<(u64, Value)> {
        validate_view_name(view).map_err(ViewError::Invalid)?;
        let (_, row) = self.read_model_row(tenant, VIEWS_REGISTRY, view)?;
        let entry = registered(row.as_ref().map(|(value, _)| value));
        let generation = entry.generation;
        entry
            .definition
            .map(|definition| (generation, definition))
            .ok_or(ViewError::NotFound)
    }

    /// Stores a definition. An identical definition keeps its generation; a
    /// changed one increments it so the folder rebuilds the rows.
    pub fn define_view(
        &self,
        tenant: &str,
        view: &str,
        definition: &Value,
    ) -> ViewResult<ViewInfo> {
        validate_view_name(view).map_err(ViewError::Invalid)?;
        validate_view_definition(definition).map_err(ViewError::Invalid)?;
        let mut attempt = 1;
        let generation = loop {
            let (meta, row) = self.read_model_row(tenant, VIEWS_REGISTRY, view)?;
            let entry = registered(row.as_ref().map(|(value, _)| value));
            if entry.definition.as_ref() == Some(definition) {
                break entry.generation;
            }
            if entry.definition.is_none()
                && self.registered_views(tenant)?.len() >= MAX_VIEWS_PER_TENANT
            {
                return Err(ViewError::Invalid(format!(
                    "a tenant may define at most {MAX_VIEWS_PER_TENANT} views"
                )));
            }
            let generation = entry.generation + 1;
            let row = json!({"generation": generation, "definition": definition});
            match self.commit_registry(tenant, meta.revision, view, row) {
                Ok(_) => break generation,
                Err(error) if is_conflict(&error) && attempt < COMMIT_ATTEMPTS => attempt += 1,
                Err(error) => return Err(error.into()),
            }
        };
        // Repeating an identical define repairs an index write that failed.
        self.sync_view_index(tenant, view, Some((generation, definition)))?;
        self.view_info(tenant, view)
    }

    /// Removes the definition and resets the view's rows.
    pub fn delete_view(&self, tenant: &str, view: &str) -> ViewResult<()> {
        let mut attempt = 1;
        loop {
            let (generation, _) = self.live_view(tenant, view)?;
            let revision = self.read_model_meta(tenant, VIEWS_REGISTRY)?.revision;
            let tombstone = json!({"generation": generation, "definition": null});
            match self.commit_registry(tenant, revision, view, tombstone) {
                Ok(_) => break,
                Err(error) if is_conflict(&error) && attempt < COMMIT_ATTEMPTS => attempt += 1,
                Err(error) => return Err(error.into()),
            }
        }
        self.sync_view_index(tenant, view, None)?;
        let model = view_model_name(view);
        let mut attempt = 1;
        loop {
            let meta = self.read_model_meta(tenant, &model)?;
            if meta.revision == 0 {
                return Ok(());
            }
            let commit = ModelCommit {
                expected_revision: meta.revision,
                checkpoint: self.tenant_feed(tenant, None, 0)?.head,
                ready: false,
                reset: true,
                changes: BTreeMap::new(),
            };
            match self.commit_core_read_model(tenant, &model, commit) {
                Ok(_) => return Ok(()),
                Err(error) if is_conflict(&error) && attempt < COMMIT_ATTEMPTS => attempt += 1,
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub fn view_info(&self, tenant: &str, view: &str) -> ViewResult<ViewInfo> {
        let (generation, definition) = self.live_view(tenant, view)?;
        let meta = self.read_model_meta(tenant, &view_model_name(view))?;
        Ok(ViewInfo {
            view: view.to_owned(),
            definition,
            generation,
            revision: meta.revision,
            checkpoint: meta.checkpoint,
            ready: meta.ready,
        })
    }

    pub fn list_views(&self, tenant: &str) -> ViewResult<Vec<ViewSummary>> {
        let mut views = Vec::new();
        for (view, entry) in self.registered_views(tenant)? {
            let meta = self.read_model_meta(tenant, &view_model_name(&view))?;
            views.push(ViewSummary {
                view,
                generation: entry.generation,
                revision: meta.revision,
                ready: meta.ready,
            });
        }
        Ok(views)
    }

    pub fn view_row(&self, tenant: &str, view: &str, key: &str) -> ViewResult<ViewRowRead> {
        let (generation, _) = self.live_view(tenant, view)?;
        let (meta, row) = self.read_model_row(tenant, &view_model_name(view), key)?;
        Ok(ViewRowRead {
            generation,
            meta,
            row,
        })
    }

    pub fn view_rows(
        &self,
        tenant: &str,
        view: &str,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> ViewResult<(u64, RowsPage)> {
        let (generation, _) = self.live_view(tenant, view)?;
        let page = self.read_model_rows(tenant, &view_model_name(view), prefix, after, limit)?;
        Ok((generation, page))
    }

    pub fn view_changes(
        &self,
        tenant: &str,
        view: &str,
        after: u64,
        limit: usize,
    ) -> ViewResult<(u64, ChangesPage)> {
        let (generation, _) = self.live_view(tenant, view)?;
        match self.read_model_changes(tenant, &view_model_name(view), after, limit)? {
            ModelChanges::Reset { .. } => Err(ViewError::Reset { generation }),
            ModelChanges::Page(page) => Ok((generation, page)),
        }
    }

    pub fn view_watch(&self, tenant: &str, view: &str) -> ModelWatch {
        self.read_model_watch(tenant, &view_model_name(view))
    }

    /// Rows `{tenant_id, view, generation, definition}` of every tenant's
    /// views, keyed `<tenant>/<view>`. Admin only.
    pub fn view_index_page(
        &self,
        after: Option<&str>,
        limit: usize,
        revision: Option<u64>,
    ) -> crate::error::Result<ModelPage> {
        self.read_model_page(VIEWS_INDEX_TENANT, VIEWS_INDEX, "", after, limit, revision)
    }
}
