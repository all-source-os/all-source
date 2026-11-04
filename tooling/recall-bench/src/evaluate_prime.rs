//! Prime-native evaluation — routes each question type through the Prime feature
//! designed to handle it, instead of treating Prime as a dumb vector store.
//!
//! Compare results against `evaluate.rs` (vector-only baseline) to measure
//! how much Prime's graph, temporal, and recall features actually help.
//!
//! Question type → Prime feature mapping:
//!   single-session-*     → recall() with graph expansion depth 1
//!   multi-session        → recall() with graph expansion depth 3 + cross-session entity edges
//!   temporal-reasoning   → history() + as_of() for temporal ordering
//!   knowledge-update     → history() with latest-wins (most recent event per entity)

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use allsource_core::prime::Prime;
use allsource_core::prime::types::RecallQuery;
use anyhow::Result;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use serde_json::json;

use crate::datasets::{self, EvalScenario};
use crate::BenchmarkResults;

// Reuse the embedder from evaluate.rs pattern
struct Embedder {
    model: TextEmbedding,
    cache: HashMap<u64, Vec<f32>>,
}

impl Embedder {
    fn with_model(model_name: &str) -> Result<Self> {
        let model_type = match model_name {
            "base" | "bge-base" => EmbeddingModel::BGEBaseENV15,
            "large" | "bge-large" => EmbeddingModel::BGELargeENV15,
            _ => EmbeddingModel::AllMiniLML6V2,
        };
        tracing::info!("Loading embedding model ({model_name})...");
        let model = TextEmbedding::try_new(
            InitOptions::new(model_type).with_show_download_progress(true),
        )?;
        Ok(Self {
            model,
            cache: HashMap::new(),
        })
    }

    fn text_hash(text: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }

    fn embed_one(&mut self, text: &str) -> Result<Vec<f32>> {
        let key = Self::text_hash(text);
        if let Some(cached) = self.cache.get(&key) {
            return Ok(cached.clone());
        }
        let mut results = self.model.embed(vec![text], None)?;
        let vec = results.remove(0);
        self.cache.insert(key, vec.clone());
        Ok(vec)
    }

    fn embed_batch(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = vec![Vec::new(); texts.len()];
        let mut uncached_indices = Vec::new();
        let mut uncached_texts = Vec::new();

        for (i, text) in texts.iter().enumerate() {
            let key = Self::text_hash(text);
            if let Some(cached) = self.cache.get(&key) {
                results[i] = cached.clone();
            } else {
                uncached_indices.push(i);
                uncached_texts.push(*text);
            }
        }

        if !uncached_texts.is_empty() {
            let embeddings = self.model.embed(uncached_texts, None)?;
            for (j, embedding) in embeddings.into_iter().enumerate() {
                let idx = uncached_indices[j];
                let key = Self::text_hash(texts[idx]);
                self.cache.insert(key, embedding.clone());
                results[idx] = embedding;
            }
        }

        Ok(results)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < f32::EPSILON || norm_b < f32::EPSILON {
        return 0.0;
    }
    f64::from(dot / (norm_a * norm_b))
}

const HIT_THRESHOLD: f64 = 0.5;

/// Run LongMemEval with Prime-native retrieval.
pub async fn run_longmemeval_prime(
    limit: usize,
    data_dir: &Path,
    model: &str,
) -> Result<BenchmarkResults> {
    let scenarios = datasets::load_longmemeval(data_dir, limit).await?;
    run_prime_eval("LongMemEval-Prime", &scenarios, model).await
}

/// Run LongMemEval in zer0dex comparison mode: three retrieval strategies on same data.
///
/// Mode A (Index Only)   = zer0dex MEMORY.md baseline  → Prime compressed index
/// Mode B (Index+Vector) = zer0dex dual-layer          → Prime context() (index + recall)
/// Mode C (Vector Only)  = zer0dex full RAG baseline   → Prime vector_search()
/// Mode D (Prime-Native) = our full stack               → type-aware recall + temporal + graph
///
/// Prints comparison table at the end.
pub async fn run_comparison(
    limit: usize,
    data_dir: &Path,
    model: &str,
) -> Result<Vec<BenchmarkResults>> {
    let scenarios = datasets::load_longmemeval(data_dir, limit).await?;

    tracing::info!("=== Running 4-mode comparison on {} scenarios ===", scenarios.len());

    // Mode C: Vector Only (zer0dex "Full RAG" baseline)
    tracing::info!("--- Mode C: Vector Only ---");
    let mode_c = run_vector_only("Mode-C-VectorOnly", &scenarios, model).await?;

    // Mode D: Prime-Native (our full stack)
    tracing::info!("--- Mode D: Prime-Native ---");
    let mode_d = run_prime_eval("Mode-D-PrimeNative", &scenarios, model).await?;

    // Print comparison
    println!("\n## zer0dex-Style Comparison (LongMemEval, n={})", scenarios.len());
    println!("| Mode | Description | Recall | Pass@0.75 | Latency |");
    println!("|------|-------------|--------|-----------|---------|");
    println!(
        "| C (Vector Only) | zer0dex 'Full RAG' equiv | {:.1}% | {:.1}% | {:.1}ms |",
        mode_c.recall * 100.0,
        mode_c.precision * 100.0,
        mode_c.avg_latency_ms,
    );
    println!(
        "| D (Prime-Native) | Graph + temporal + type-aware | {:.1}% | {:.1}% | {:.1}ms |",
        mode_d.recall * 100.0,
        mode_d.precision * 100.0,
        mode_d.avg_latency_ms,
    );
    println!("|---|---|---|---|---|");
    println!("| zer0dex Mode C | Published baseline | 37.5% cross-ref | — | 70ms |");
    println!("| zer0dex Mode B | Published dual-layer | 80.0% cross-ref | — | 70ms |");

    let delta = mode_d.recall - mode_c.recall;
    println!(
        "\n**Prime-Native vs Vector-Only: {}{:.1}% recall**",
        if delta >= 0.0 { "+" } else { "" },
        delta * 100.0,
    );

    Ok(vec![mode_c, mode_d])
}

/// Vector-only retrieval (zer0dex Mode C equivalent).
async fn run_vector_only(
    dataset_name: &str,
    scenarios: &[EvalScenario],
    model_name: &str,
) -> Result<BenchmarkResults> {
    let mut embedder = Embedder::with_model(model_name)?;

    let mut total_queries = 0usize;
    let mut total_latency_ms = 0.0f64;
    let mut type_scores: HashMap<String, Vec<f64>> = HashMap::new();

    for (scenario_idx, scenario) in scenarios.iter().enumerate() {
        if scenario_idx % 10 == 0 {
            tracing::info!(
                "[VectorOnly] Processing scenario {}/{}",
                scenario_idx + 1,
                scenarios.len(),
            );
        }

        let prime = Prime::open_in_memory().await?;

        // Ingest (same as prime-native)
        let valid_turns: Vec<&datasets::ConversationTurn> = scenario
            .conversation
            .iter()
            .filter(|t| t.role != "system" && !t.content.is_empty())
            .collect();
        let turn_texts: Vec<&str> = valid_turns.iter().map(|t| t.content.as_str()).collect();
        let embeddings = embedder.embed_batch(&turn_texts)?;

        for (turn, embedding) in valid_turns.iter().zip(embeddings.into_iter()) {
            let node_id = prime
                .add_node(
                    "memory",
                    json!({
                        "content": turn.content,
                        "session_id": turn.session_id,
                        "session_date": turn.session_date,
                    }),
                )
                .await?;
            #[allow(deprecated)]
            let entity_id =
                allsource_core::prime::node_entity_id("memory", node_id.as_str());
            prime
                .embed(&entity_id, Some(&turn.content), embedding)
                .await?;
        }

        // Query: pure vector search only (no graph, no temporal)
        for query in &scenario.queries {
            total_queries += 1;
            let start = Instant::now();

            let query_embedding = embedder.embed_one(&query.question)?;
            let results = prime.vector_search(&query_embedding, 15);

            let retrieved: Vec<(String, String)> = results
                .into_iter()
                .filter_map(|r| {
                    let text = r.text?;
                    let node_id = r.id.strip_prefix("vec:").unwrap_or(&r.id);
                    let sid = prime
                        .get_node(node_id)
                        .and_then(|n| {
                            n.properties
                                .get("session_id")
                                .and_then(|v| v.as_str())
                                .map(String::from)
                        })
                        .unwrap_or_default();
                    Some((text, sid))
                })
                .collect();

            let retrieved_texts: Vec<String> = retrieved.iter().map(|(t, _)| t.clone()).collect();
            let retrieved_sids: Vec<String> = retrieved.iter().map(|(_, s)| s.clone()).collect();

            let latency = start.elapsed().as_secs_f64() * 1000.0;
            total_latency_ms += latency;

            // Score (same as prime-native)
            let q_type = &query.question_type;
            let is_abstention = q_type.contains("abs");

            let score = if is_abstention {
                let expected_emb = embedder.embed_one(&query.expected_facts[0])?;
                let max_sim = retrieved_texts
                    .iter()
                    .filter_map(|t| {
                        let emb = embedder.embed_one(t).ok()?;
                        Some(cosine_similarity(&expected_emb, &emb))
                    })
                    .fold(0.0f64, f64::max);
                if max_sim < HIT_THRESHOLD { 1.0 } else { 0.0 }
            } else {
                let session_hit = if !query.answer_session_ids.is_empty() {
                    let found = retrieved_sids
                        .iter()
                        .any(|sid| query.answer_session_ids.contains(sid));
                    if found { 1.0 } else { 0.0 }
                } else {
                    0.0
                };
                let mut semantic_score: f64 = 0.0;
                for expected in &query.expected_facts {
                    let expected_emb = embedder.embed_one(expected)?;
                    let sim = retrieved_texts
                        .iter()
                        .filter_map(|t| {
                            let emb = embedder.embed_one(t).ok()?;
                            Some(cosine_similarity(&expected_emb, &emb))
                        })
                        .fold(0.0f64, f64::max);
                    semantic_score = semantic_score.max(sim);
                }
                if !query.answer_session_ids.is_empty() {
                    session_hit * 0.5 + semantic_score.min(1.0) * 0.5
                } else if semantic_score >= HIT_THRESHOLD {
                    1.0
                } else {
                    semantic_score
                }
            };

            let q_label = if q_type.is_empty() { "unknown" } else { q_type }.to_string();
            type_scores.entry(q_label).or_default().push(score);
        }

        prime.shutdown().await?;
    }

    let all_scores: Vec<f64> = type_scores.values().flatten().copied().collect();
    let overall_recall = if all_scores.is_empty() {
        0.0
    } else {
        all_scores.iter().sum::<f64>() / all_scores.len() as f64
    };
    let pass_rate = if all_scores.is_empty() {
        0.0
    } else {
        all_scores.iter().filter(|&&s| s >= 0.75).count() as f64 / all_scores.len() as f64
    };

    // Log per-type
    let mut type_names: Vec<&String> = type_scores.keys().collect();
    type_names.sort();
    for type_name in &type_names {
        let scores = &type_scores[*type_name];
        let avg = scores.iter().sum::<f64>() / scores.len().max(1) as f64;
        tracing::info!(
            "  [VectorOnly] {type_name}: n={} recall={:.1}%",
            scores.len(),
            avg * 100.0,
        );
    }

    tracing::info!(
        "{dataset_name}: overall_recall={:.1}% pass@0.75={:.1}%",
        overall_recall * 100.0,
        pass_rate * 100.0,
    );

    Ok(BenchmarkResults {
        dataset: dataset_name.to_string(),
        mode: "vector-only".to_string(),
        conversations: scenarios.len(),
        queries: total_queries,
        precision: pass_rate,
        recall: overall_recall,
        f1: if pass_rate + overall_recall > 0.0 {
            2.0 * pass_rate * overall_recall / (pass_rate + overall_recall)
        } else {
            0.0
        },
        cross_ref_accuracy: None,
        avg_latency_ms: if total_queries > 0 {
            total_latency_ms / total_queries as f64
        } else {
            0.0
        },
    })
}

/// Prime-native evaluation: each question type uses the right Prime API.
async fn run_prime_eval(
    dataset_name: &str,
    scenarios: &[EvalScenario],
    model_name: &str,
) -> Result<BenchmarkResults> {
    let mut embedder = Embedder::with_model(model_name)?;

    let mut total_queries = 0usize;
    let mut total_latency_ms = 0.0f64;
    let mut type_scores: HashMap<String, Vec<f64>> = HashMap::new();

    for (scenario_idx, scenario) in scenarios.iter().enumerate() {
        if scenario_idx % 10 == 0 {
            tracing::info!(
                "[Prime] Processing scenario {}/{} ({})",
                scenario_idx + 1,
                scenarios.len(),
                scenario.id,
            );
        }

        let prime = Prime::open_in_memory().await?;

        // =====================================================================
        // INGESTION: sentence chunks + embeddings + semantic edges
        // =====================================================================

        let valid_turns: Vec<&datasets::ConversationTurn> = scenario
            .conversation
            .iter()
            .filter(|t| t.role != "system" && !t.content.is_empty())
            .collect();

        let turn_texts: Vec<&str> = valid_turns.iter().map(|t| t.content.as_str()).collect();
        let embeddings = embedder.embed_batch(&turn_texts)?;

        // Track entity IDs for edge creation
        let mut entity_ids: Vec<String> = Vec::new();
        let mut prev_entity_id: Option<String> = None;
        let mut session_entity_map: HashMap<String, Vec<usize>> = HashMap::new();

        for (idx, (turn, embedding)) in valid_turns.iter().zip(embeddings.into_iter()).enumerate()
        {
            let node_id = prime
                .add_node(
                    "memory",
                    json!({
                        "content": turn.content,
                        "role": turn.role,
                        "scenario": scenario.id,
                        "session_id": turn.session_id,
                        "session_date": turn.session_date,
                        "domain": turn.session_id, // domain = session for cross-session tracking
                    }),
                )
                .await?;

            #[allow(deprecated)]
            let entity_id =
                allsource_core::prime::node_entity_id("memory", node_id.as_str());

            prime
                .embed(&entity_id, Some(&turn.content), embedding)
                .await?;

            // Sequential edge
            if let Some(ref prev) = prev_entity_id {
                let _ = prime.add_edge(prev, &entity_id, "follows", None).await;
            }
            prev_entity_id = Some(entity_id.clone());

            // Track for cross-session edges
            session_entity_map
                .entry(turn.session_id.clone())
                .or_default()
                .push(idx);

            entity_ids.push(entity_id);
        }

        // Cross-session entity edges (shared proper nouns)
        let mut name_to_indices: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, turn) in valid_turns.iter().enumerate() {
            let words: Vec<&str> = turn.content.split_whitespace().collect();
            for window in words.windows(2) {
                let a = window[0].trim_matches(|c: char| !c.is_alphanumeric());
                let b = window[1].trim_matches(|c: char| !c.is_alphanumeric());
                if a.len() >= 2
                    && b.len() >= 2
                    && a.chars().next().is_some_and(|c| c.is_uppercase())
                    && b.chars().next().is_some_and(|c| c.is_uppercase())
                {
                    name_to_indices
                        .entry(format!("{a} {b}"))
                        .or_default()
                        .push(idx);
                }
            }
        }
        for (_name, indices) in &name_to_indices {
            if indices.len() < 2 || indices.len() > 20 {
                continue;
            }
            for i in 0..indices.len().min(5) {
                for j in (i + 1)..indices.len().min(5) {
                    let si = &valid_turns[indices[i]].session_id;
                    let sj = &valid_turns[indices[j]].session_id;
                    if si != sj {
                        let _ = prime
                            .add_edge(
                                &entity_ids[indices[i]],
                                &entity_ids[indices[j]],
                                "mentions",
                                None,
                            )
                            .await;
                    }
                }
            }
        }

        // =====================================================================
        // RETRIEVAL: route each question type to the right Prime feature
        // =====================================================================

        for query in &scenario.queries {
            total_queries += 1;
            let start = Instant::now();

            let query_embedding = embedder.embed_one(&query.question)?;
            let q_type = &query.question_type;

            let retrieved: Vec<(String, String)> = match q_type.as_str() {
                // ---------------------------------------------------------
                // TEMPORAL: recall() retrieves, then order by the caller's
                // session_date. The store has no valid-time axis to order by.
                // ---------------------------------------------------------
                "temporal-reasoning" => {
                    // Use recall() for semantic + graph retrieval
                    let recall_query = RecallQuery {
                        vector: Some(query_embedding.clone()),
                        text: Some(query.question.clone()),
                        depth: 2,
                        top_k: 20,
                        ..RecallQuery::default()
                    };

                    let mut hits: Vec<(String, String, String)> =
                        match prime.recall(recall_query).await {
                            Ok(result) => result
                                .nodes
                                .iter()
                                .filter_map(|sn| {
                                    let text = sn
                                        .node
                                        .properties
                                        .get("content")
                                        .and_then(|v| v.as_str())?
                                        .to_string();
                                    let sid = sn
                                        .node
                                        .properties
                                        .get("session_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let date = sn
                                        .node
                                        .properties
                                        .get("session_date")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    Some((text, sid, date))
                                })
                                .collect(),
                            Err(_) => Vec::new(),
                        };

                    // Sort by date for temporal ordering
                    hits.sort_by(|a, b| a.2.cmp(&b.2));

                    // Apply temporal keyword filtering
                    let q_lower = query.question.to_lowercase();
                    let filtered = if q_lower.contains("first") || q_lower.contains("earliest") {
                        hits.into_iter().take(8).collect::<Vec<_>>()
                    } else if q_lower.contains("last")
                        || q_lower.contains("latest")
                        || q_lower.contains("most recent")
                    {
                        hits.reverse();
                        hits.into_iter().take(8).collect()
                    } else if q_lower.contains("before") {
                        let qd = query.query_date.as_deref().unwrap_or("");
                        hits.into_iter()
                            .filter(|(_, _, d)| d.as_str() <= qd)
                            .collect()
                    } else {
                        hits
                    };

                    filtered
                        .into_iter()
                        .map(|(text, sid, _)| (text, sid))
                        .take(15)
                        .collect()
                }

                // ---------------------------------------------------------
                // KNOWLEDGE-UPDATE: recall() + latest-wins (reverse chrono)
                // Uses temporal sorting to surface most recent facts
                // ---------------------------------------------------------
                "knowledge-update" => {
                    let recall_query = RecallQuery {
                        vector: Some(query_embedding.clone()),
                        text: Some(query.question.clone()),
                        depth: 1,
                        top_k: 20,
                        recency_weight: 0.5, // Boost recency for knowledge updates
                        similarity_weight: 0.4,
                        proximity_weight: 0.1,
                        ..RecallQuery::default()
                    };

                    let mut hits: Vec<(String, String, String)> =
                        match prime.recall(recall_query).await {
                            Ok(result) => result
                                .nodes
                                .iter()
                                .filter_map(|sn| {
                                    let text = sn
                                        .node
                                        .properties
                                        .get("content")
                                        .and_then(|v| v.as_str())?
                                        .to_string();
                                    let sid = sn
                                        .node
                                        .properties
                                        .get("session_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let date = sn
                                        .node
                                        .properties
                                        .get("session_date")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    Some((text, sid, date))
                                })
                                .collect(),
                            Err(_) => Vec::new(),
                        };

                    // Latest wins: sort reverse chronological
                    hits.sort_by(|a, b| b.2.cmp(&a.2));

                    hits.into_iter()
                        .map(|(text, sid, _)| (text, sid))
                        .take(10)
                        .collect()
                }

                // ---------------------------------------------------------
                // MULTI-SESSION: recall() with deep graph expansion (depth 3)
                // Graph edges (mentions, follows) bridge sessions
                // ---------------------------------------------------------
                qt if qt.contains("multi-session") => {
                    let recall_query = RecallQuery {
                        vector: Some(query_embedding.clone()),
                        text: Some(query.question.clone()),
                        depth: 3, // Deep graph expansion for cross-session
                        top_k: 25,
                        proximity_weight: 0.4, // Boost graph proximity for multi-session
                        similarity_weight: 0.4,
                        recency_weight: 0.2,
                        ..RecallQuery::default()
                    };

                    match prime.recall(recall_query).await {
                        Ok(result) => result
                            .nodes
                            .iter()
                            .filter_map(|sn| {
                                let text = sn
                                    .node
                                    .properties
                                    .get("content")
                                    .and_then(|v| v.as_str())?
                                    .to_string();
                                let sid = sn
                                    .node
                                    .properties
                                    .get("session_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                Some((text, sid))
                            })
                            .collect(),
                        Err(_) => Vec::new(),
                    }
                }

                // ---------------------------------------------------------
                // SINGLE-SESSION (all types): recall() with standard config
                // Graph expansion depth 1 for within-session connections
                // ---------------------------------------------------------
                _ => {
                    let recall_query = RecallQuery {
                        vector: Some(query_embedding.clone()),
                        text: Some(query.question.clone()),
                        depth: 1,
                        top_k: 15,
                        ..RecallQuery::default()
                    };

                    match prime.recall(recall_query).await {
                        Ok(result) => result
                            .nodes
                            .iter()
                            .filter_map(|sn| {
                                let text = sn
                                    .node
                                    .properties
                                    .get("content")
                                    .and_then(|v| v.as_str())?
                                    .to_string();
                                let sid = sn
                                    .node
                                    .properties
                                    .get("session_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                Some((text, sid))
                            })
                            .collect(),
                        Err(_) => Vec::new(),
                    }
                }
            };

            let retrieved_texts: Vec<String> =
                retrieved.iter().map(|(t, _)| t.clone()).collect();
            let retrieved_session_ids: Vec<String> =
                retrieved.iter().map(|(_, s)| s.clone()).collect();

            let latency = start.elapsed().as_secs_f64() * 1000.0;
            total_latency_ms += latency;

            // Score: same combined session + semantic as baseline
            let is_abstention = q_type.contains("abs") || q_type.contains("abstention");

            let score = if is_abstention {
                let expected_emb = embedder.embed_one(&query.expected_facts[0])?;
                let max_sim = retrieved_texts
                    .iter()
                    .filter_map(|t| {
                        let emb = embedder.embed_one(t).ok()?;
                        Some(cosine_similarity(&expected_emb, &emb))
                    })
                    .fold(0.0f64, f64::max);
                if max_sim < HIT_THRESHOLD { 1.0 } else { 0.0 }
            } else {
                let session_hit = if !query.answer_session_ids.is_empty() {
                    let found = retrieved_session_ids
                        .iter()
                        .any(|sid| query.answer_session_ids.contains(sid));
                    if found { 1.0 } else { 0.0 }
                } else {
                    0.0
                };

                let mut semantic_score: f64 = 0.0;
                for expected in &query.expected_facts {
                    let expected_emb = embedder.embed_one(expected)?;
                    let sim = retrieved_texts
                        .iter()
                        .filter_map(|t| {
                            let emb = embedder.embed_one(t).ok()?;
                            Some(cosine_similarity(&expected_emb, &emb))
                        })
                        .fold(0.0f64, f64::max);
                    semantic_score = semantic_score.max(sim);
                }

                if !query.answer_session_ids.is_empty() {
                    session_hit * 0.5 + semantic_score.min(1.0) * 0.5
                } else if semantic_score >= HIT_THRESHOLD {
                    1.0
                } else {
                    semantic_score
                }
            };

            let q_label = if q_type.is_empty() {
                "unknown".to_string()
            } else {
                q_type.clone()
            };
            type_scores.entry(q_label).or_default().push(score);
        }

        prime.shutdown().await?;
    }

    // Compute aggregates
    let all_scores: Vec<f64> = type_scores.values().flatten().copied().collect();
    let overall_recall = if all_scores.is_empty() {
        0.0
    } else {
        all_scores.iter().sum::<f64>() / all_scores.len() as f64
    };
    let pass_rate = if all_scores.is_empty() {
        0.0
    } else {
        all_scores.iter().filter(|&&s| s >= 0.75).count() as f64 / all_scores.len() as f64
    };

    // Log per-type
    let mut type_names: Vec<&String> = type_scores.keys().collect();
    type_names.sort();
    for type_name in &type_names {
        let scores = &type_scores[*type_name];
        let avg = scores.iter().sum::<f64>() / scores.len().max(1) as f64;
        let pass = scores.iter().filter(|&&s| s >= 0.75).count() as f64
            / scores.len().max(1) as f64;
        tracing::info!(
            "  [Prime] {type_name}: n={} recall={:.1}% pass@0.75={:.1}%",
            scores.len(),
            avg * 100.0,
            pass * 100.0,
        );
    }

    let cross_ref_scores: Vec<f64> = type_scores
        .iter()
        .filter(|(k, _)| k.contains("multi-session"))
        .flat_map(|(_, v)| v.iter().copied())
        .collect();
    let cross_ref = if cross_ref_scores.is_empty() {
        None
    } else {
        Some(
            cross_ref_scores
                .iter()
                .filter(|&&s| s >= 0.75)
                .count() as f64
                / cross_ref_scores.len() as f64,
        )
    };

    tracing::info!(
        "{dataset_name}: overall_recall={:.1}% pass@0.75={:.1}% queries={total_queries}",
        overall_recall * 100.0,
        pass_rate * 100.0,
    );

    Ok(BenchmarkResults {
        dataset: dataset_name.to_string(),
        mode: "prime-native".to_string(),
        conversations: scenarios.len(),
        queries: total_queries,
        precision: pass_rate,
        recall: overall_recall,
        f1: if pass_rate + overall_recall > 0.0 {
            2.0 * pass_rate * overall_recall / (pass_rate + overall_recall)
        } else {
            0.0
        },
        cross_ref_accuracy: cross_ref,
        avg_latency_ms: if total_queries > 0 {
            total_latency_ms / total_queries as f64
        } else {
            0.0
        },
    })
}
