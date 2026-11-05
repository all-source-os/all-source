//! In-memory BM25 keyword index used when Core is built without Tantivy.
//!
//! Corpus statistics are computed over the documents the query may see, so a
//! tenant-scoped search never ranks with another tenant's term frequencies.

use super::{IndexedDocument, KeywordQuery, KeywordSearchResult};
use parking_lot::RwLock;
use std::collections::HashMap;
use uuid::Uuid;

const K1: f32 = 1.2;
const B: f32 = 0.75;

struct Doc {
    tenant_id: String,
    event_type: String,
    entity_id: Option<String>,
    fields: [(&'static str, Vec<String>); 3],
}

impl Doc {
    fn terms<'a>(&'a self, fields: Option<&'a [String]>) -> impl Iterator<Item = &'a String> {
        self.fields
            .iter()
            .filter(move |(name, _)| fields.is_none_or(|wanted| wanted.iter().any(|w| w == name)))
            .flat_map(|(_, terms)| terms.iter())
    }
}

#[derive(Default)]
pub(super) struct InMemoryKeywordIndex {
    docs: RwLock<HashMap<Uuid, Doc>>,
}

pub(super) fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

impl InMemoryKeywordIndex {
    /// Returns true when the event was not indexed before.
    pub(super) fn insert(&self, document: IndexedDocument) -> bool {
        let doc = Doc {
            fields: [
                ("event_type", tokenize(&document.event_type)),
                (
                    "entity_id",
                    document
                        .entity_id
                        .as_deref()
                        .map(tokenize)
                        .unwrap_or_default(),
                ),
                ("payload", tokenize(&document.payload_text)),
            ],
            tenant_id: document.tenant_id,
            event_type: document.event_type,
            entity_id: document.entity_id,
        };
        self.docs.write().insert(document.event_id, doc).is_none()
    }

    /// Returns the tenant of the removed event.
    pub(super) fn remove(&self, event_id: Uuid) -> Option<String> {
        self.docs.write().remove(&event_id).map(|d| d.tenant_id)
    }

    pub(super) fn remove_tenant(&self, tenant_id: &str) -> usize {
        let mut docs = self.docs.write();
        let before = docs.len();
        docs.retain(|_, d| d.tenant_id != tenant_id);
        before - docs.len()
    }

    pub(super) fn search(&self, query: &KeywordQuery) -> Vec<KeywordSearchResult> {
        let query_terms = tokenize(&query.query);
        if query_terms.is_empty() || query.limit == 0 {
            return Vec::new();
        }
        let fields = query.fields.as_deref();
        let docs = self.docs.read();
        let candidates: Vec<(&Uuid, &Doc)> = docs
            .iter()
            .filter(|(_, d)| query.tenant_id.as_ref().is_none_or(|t| &d.tenant_id == t))
            .collect();
        if candidates.is_empty() {
            return Vec::new();
        }

        let lengths: Vec<usize> = candidates
            .iter()
            .map(|(_, d)| d.terms(fields).count())
            .collect();
        let n = candidates.len() as f32;
        let avg_len = (lengths.iter().sum::<usize>() as f32 / n).max(1.0);

        let mut unique_terms = query_terms.clone();
        unique_terms.sort();
        unique_terms.dedup();
        let idf: HashMap<&str, f32> = unique_terms
            .iter()
            .map(|term| {
                let df = candidates
                    .iter()
                    .filter(|(_, d)| d.terms(fields).any(|t| t == term))
                    .count() as f32;
                (term.as_str(), ((n - df + 0.5) / (df + 0.5) + 1.0).ln())
            })
            .collect();

        let mut scored: Vec<KeywordSearchResult> = candidates
            .iter()
            .zip(lengths)
            .filter_map(|((event_id, doc), len)| {
                let mut tf: HashMap<&str, f32> = HashMap::new();
                for term in doc.terms(fields) {
                    if idf.contains_key(term.as_str()) {
                        *tf.entry(term.as_str()).or_insert(0.0) += 1.0;
                    }
                }
                let norm = K1 * (1.0 - B + B * len as f32 / avg_len);
                let score: f32 = tf
                    .iter()
                    .map(|(term, f)| idf[term] * f * (K1 + 1.0) / (f + norm))
                    .sum();
                (score > 0.0).then(|| KeywordSearchResult {
                    event_id: **event_id,
                    score,
                    event_type: doc.event_type.clone(),
                    entity_id: doc.entity_id.clone(),
                    highlights: None,
                })
            })
            .collect();
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.event_id.cmp(&b.event_id))
        });
        scored.truncate(query.limit);
        scored
    }
}
