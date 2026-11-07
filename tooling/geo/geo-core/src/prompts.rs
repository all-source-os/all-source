//! The versioned probe set — the actual deliverable of layer 3.
//!
//! The prompt set is **source code, not configuration**. Prompt 027 will change
//! the *site* and score the result against this set; if the set drifts at the
//! same time, the experiment is uninterpretable. So:
//!
//! - The TOML is `include_str!`d into the binary. A `geo probe` build always
//!   carries the exact set it was compiled with — you cannot accidentally run
//!   against an edited file on disk.
//! - Every set has a [`PromptSet::digest`] (SHA-256 of the TOML text). The
//!   digest goes into the `run_id` of every emitted event, so two runs are
//!   comparable if and only if their run ids carry the same digest. A stale
//!   comparison is then a visible mismatch rather than a silent lie.
//! - The invariants that make a family *be* that family are asserted by tests,
//!   not by convention: an SOV prompt that names AllSource is not an SOV
//!   prompt, and an interrogation prompt with no ground-truth claim cannot be
//!   scored for accuracy.
//!
//! Editing the set is allowed and expected — but it starts a new baseline.
//! Bump `version`, and say so in `docs/runbooks/GEO_MEASUREMENT.md`.

use serde::Deserialize;

use crate::{brand, error::GeoError, idempotency::derive_key};

/// The frozen share-of-voice set (layer 3a).
pub const SOV_TOML: &str = include_str!("../../prompts/sov.toml");

/// The frozen interrogation set (layer 3b).
pub const INTERROGATION_TOML: &str = include_str!("../../prompts/interrogation.toml");

/// How many hex characters of the set digest ride in a `run_id`.
const DIGEST_LEN: usize = 8;

/// Which of the two probe families a prompt belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    /// 3a — buyer-intent questions with no brand named.
    Sov,
    /// 3b — brand-named questions with a ground-truth expectation.
    Interrogation,
}

impl Family {
    /// Both families, in layer order.
    pub const ALL: [Self; 2] = [Self::Sov, Self::Interrogation];

    /// Wire/CLI string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sov => "sov",
            Self::Interrogation => "interrogation",
        }
    }

    /// Parse a CLI string.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == s)
    }

    /// The raw TOML this family is compiled from.
    pub fn source(self) -> &'static str {
        match self {
            Self::Sov => SOV_TOML,
            Self::Interrogation => INTERROGATION_TOML,
        }
    }

    /// Load and validate the compiled-in set for this family.
    pub fn load(self) -> Result<PromptSet, GeoError> {
        PromptSet::parse(self, self.source())
    }
}

impl std::fmt::Display for Family {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What kind of buyer question a prompt is.
///
/// SOV is reported **per intent class**, never as one blended number: being
/// named on "alternatives to Mem0" and being named on "how do I stop my agent
/// forgetting" are different wins, and averaging them hides which one moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Intent {
    /// "best event store for AI agents" — category-level.
    Category,
    /// "how do I stop my agent forgetting between sessions?" — problem-level.
    Problem,
    /// "alternatives to Mem0" — comparison-level.
    Comparison,
    /// "MCP server for giving Claude Code memory" — integration-level.
    Integration,
    /// Brand-named interrogation prompts (3b only).
    Brand,
}

impl Intent {
    /// Every class, SOV classes first.
    pub const ALL: [Self; 5] = [
        Self::Category,
        Self::Problem,
        Self::Comparison,
        Self::Integration,
        Self::Brand,
    ];

    /// The four SOV classes. A set missing one of these is not a spanning set.
    pub const SOV: [Self; 4] = [
        Self::Category,
        Self::Problem,
        Self::Comparison,
        Self::Integration,
    ];

    /// Wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Category => "category",
            Self::Problem => "problem",
            Self::Comparison => "comparison",
            Self::Integration => "integration",
            Self::Brand => "brand",
        }
    }

    /// Parse a wire string.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|i| i.as_str() == s)
    }
}

impl std::fmt::Display for Intent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How much a wrong answer on a claim costs us.
///
/// This is the *severity* half of the remediation backlog's
/// `frequency × severity` ordering. It is a property of the claim, not of any
/// one answer: a model inventing a price loses a sale, a model getting the
/// storage engine slightly wrong loses an argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Wrong answer directly costs a sale or a trust relationship.
    Critical,
    /// Wrong answer sends a qualified buyer to a competitor.
    High,
    /// Wrong answer is a correctable misunderstanding.
    Medium,
    /// Wrong answer is cosmetic.
    Low,
}

impl Severity {
    /// Every level, worst first.
    pub const ALL: [Self; 4] = [Self::Critical, Self::High, Self::Medium, Self::Low];

    /// Wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }

    /// Parse a wire string.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s2| s2.as_str() == s)
    }

    /// Ordering weight for the remediation backlog. Deliberately spread so a
    /// single critical miss outranks a handful of low ones.
    pub fn weight(self) -> f64 {
        match self {
            Self::Critical => 8.0,
            Self::High => 4.0,
            Self::Medium => 2.0,
            Self::Low => 1.0,
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One factual claim under test inside an interrogation prompt.
///
/// A single question ("how much does AllSource cost?") can be wrong in several
/// independent ways (the tier names, the currency, the free tier). Each of
/// those is its own claim, its own `geo.interrogation.probed` event and its own
/// backlog row — because each has its own fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// Stable id, unique within the whole set. Lands in `payload.claim_id`.
    pub id: String,
    /// What a correct answer says, in the words the judge is given.
    pub expectation: String,
    /// Where in this repository that expectation is defined. This is what
    /// makes a wrong verdict auditable — and what tells prompt 027 which file
    /// to change.
    pub source: String,
    /// How much a wrong answer costs.
    pub severity: Severity,
}

/// One prompt in a set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// Stable id. Lands in `payload.prompt_id` and never changes meaning.
    pub id: String,
    /// Which family it belongs to.
    pub family: Family,
    /// Buyer-intent class.
    pub intent: Intent,
    /// The prompt as sent, verbatim.
    pub text: String,
    /// Ground-truth claims. Empty for SOV, non-empty for interrogation.
    pub claims: Vec<Claim>,
}

/// A loaded, validated prompt set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSet {
    /// Which family.
    pub family: Family,
    /// Set version. Bumping it starts a new baseline.
    pub version: u32,
    /// The prompts, in file order.
    pub prompts: Vec<Prompt>,
    /// SHA-256 (truncated) of the TOML text. Rides in every `run_id`.
    pub digest: String,
}

// ───────────────────────────────────────────────────────────────────────────
// TOML shapes. Private: the file format is an implementation detail of this
// module, and the public API is the validated `PromptSet`.
// ───────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RawSet {
    version: u32,
    family: String,
    #[serde(default)]
    prompt: Vec<RawPrompt>,
}

#[derive(Deserialize)]
struct RawPrompt {
    id: String,
    intent: String,
    text: String,
    #[serde(default)]
    claim: Vec<RawClaim>,
}

#[derive(Deserialize)]
struct RawClaim {
    id: String,
    expectation: String,
    source: String,
    severity: String,
}

impl PromptSet {
    /// Parse and validate a set from TOML text.
    ///
    /// Every failure here is a build-time authoring mistake, so the messages
    /// name the offending id rather than a byte offset.
    pub fn parse(family: Family, toml_text: &str) -> Result<Self, GeoError> {
        let raw: RawSet = toml::from_str(toml_text).map_err(|e| GeoError::PromptSet {
            family: family.as_str(),
            reason: e.to_string(),
        })?;

        let invalid = |reason: String| GeoError::PromptSet {
            family: family.as_str(),
            reason,
        };

        if raw.family != family.as_str() {
            return Err(invalid(format!(
                "file declares family {:?} but was loaded as {family}",
                raw.family
            )));
        }
        if raw.prompt.is_empty() {
            return Err(invalid("set has no prompts".to_string()));
        }

        let mut prompts = Vec::with_capacity(raw.prompt.len());
        let mut seen_prompt_ids = Vec::new();
        let mut seen_claim_ids = Vec::new();

        for rp in raw.prompt {
            if seen_prompt_ids.contains(&rp.id) {
                return Err(invalid(format!("duplicate prompt id {:?}", rp.id)));
            }
            let intent = Intent::parse(&rp.intent)
                .ok_or_else(|| invalid(format!("{}: unknown intent {:?}", rp.id, rp.intent)))?;
            if rp.text.trim().is_empty() {
                return Err(invalid(format!("{}: empty prompt text", rp.id)));
            }

            match family {
                // The defining invariant of the SOV family: no brand named. A
                // prompt that names us is not measuring share of voice, it is
                // measuring whether the model can read.
                Family::Sov => {
                    if brand::names_allsource(&rp.text) {
                        return Err(invalid(format!(
                            "{}: an SOV prompt must not name AllSource",
                            rp.id
                        )));
                    }
                    if !rp.claim.is_empty() {
                        return Err(invalid(format!(
                            "{}: SOV prompts carry no ground-truth claims",
                            rp.id
                        )));
                    }
                    if intent == Intent::Brand {
                        return Err(invalid(format!("{}: intent 'brand' is 3b-only", rp.id)));
                    }
                }
                // The defining invariant of the interrogation family: it names
                // us, and every one of them is scoreable against something the
                // repository actually says.
                Family::Interrogation => {
                    if !brand::names_allsource(&rp.text) {
                        return Err(invalid(format!(
                            "{}: an interrogation prompt must name AllSource",
                            rp.id
                        )));
                    }
                    if rp.claim.is_empty() {
                        return Err(invalid(format!(
                            "{}: interrogation prompts need at least one ground-truth claim",
                            rp.id
                        )));
                    }
                }
            }

            let mut claims = Vec::with_capacity(rp.claim.len());
            for rc in rp.claim {
                if seen_claim_ids.contains(&rc.id) {
                    return Err(invalid(format!("duplicate claim id {:?}", rc.id)));
                }
                let severity = Severity::parse(&rc.severity).ok_or_else(|| {
                    invalid(format!("{}: unknown severity {:?}", rc.id, rc.severity))
                })?;
                if rc.expectation.trim().is_empty() {
                    return Err(invalid(format!("{}: empty expectation", rc.id)));
                }
                if rc.source.trim().is_empty() {
                    return Err(invalid(format!(
                        "{}: every expectation must cite where in the repo it is defined",
                        rc.id
                    )));
                }
                seen_claim_ids.push(rc.id.clone());
                claims.push(Claim {
                    id: rc.id,
                    expectation: rc.expectation,
                    source: rc.source,
                    severity,
                });
            }

            seen_prompt_ids.push(rp.id.clone());
            prompts.push(Prompt {
                id: rp.id,
                family,
                intent,
                text: rp.text,
                claims,
            });
        }

        let digest = derive_key(&[toml_text])[..DIGEST_LEN].to_string();
        Ok(Self {
            family,
            version: raw.version,
            prompts,
            digest,
        })
    }

    /// Number of prompts.
    pub fn len(&self) -> usize {
        self.prompts.len()
    }

    /// Whether the set is empty. Never true for a validated set.
    pub fn is_empty(&self) -> bool {
        self.prompts.is_empty()
    }

    /// Total ground-truth claims across the set (3b only).
    pub fn claim_count(&self) -> usize {
        self.prompts.iter().map(|p| p.claims.len()).sum()
    }

    /// Look a prompt up by id.
    pub fn get(&self, id: &str) -> Option<&Prompt> {
        self.prompts.iter().find(|p| p.id == id)
    }

    /// The `run_id` for one repetition of a sweep of this set.
    ///
    /// Shape: `<family>-<YYYY-MM-DD>-<digest>#r<n>`.
    ///
    /// The repetition index is **in the run id on purpose**. The contract's
    /// natural key for a probe is `run_id + engine + prompt_id`, so without it
    /// the three repetitions of one prompt would collapse into three versions
    /// of a single entity — and a reader folding by entity (which is what
    /// makes a re-ingest safe) would see one sample where three were taken.
    /// The whole point of repeating is to keep the distribution, so the
    /// repetitions have to be distinct entities. Split on `#` to recover the
    /// sweep that groups them.
    pub fn run_id(&self, date: &str, repetition: u32) -> String {
        format!("{}-{date}-{}#r{repetition}", self.family, self.digest)
    }

    /// The sweep id a `run_id` belongs to — everything before the `#`.
    pub fn sweep_of(run_id: &str) -> &str {
        run_id.split('#').next().unwrap_or(run_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sov() -> PromptSet {
        Family::Sov.load().expect("committed sov.toml is valid")
    }

    fn interrogation() -> PromptSet {
        Family::Interrogation
            .load()
            .expect("committed interrogation.toml is valid")
    }

    #[test]
    fn both_committed_sets_load() {
        assert_eq!(sov().family, Family::Sov);
        assert_eq!(interrogation().family, Family::Interrogation);
    }

    #[test]
    fn the_sov_set_is_big_enough_to_be_a_measurement() {
        // Fewer than ~25 prompts and a single engine's mood swings dominate
        // the mention rate. The framework's floor is 25-40.
        let set = sov();
        assert!(set.len() >= 25, "only {} SOV prompts", set.len());
        assert!(set.len() <= 40, "{} SOV prompts is past the cap", set.len());
    }

    #[test]
    fn the_sov_set_spans_every_intent_class() {
        let set = sov();
        for intent in Intent::SOV {
            let n = set.prompts.iter().filter(|p| p.intent == intent).count();
            assert!(n >= 4, "only {n} {intent} prompts — not a spanning set");
        }
    }

    #[test]
    fn no_sov_prompt_names_the_brand() {
        for prompt in sov().prompts {
            assert!(
                !brand::names_allsource(&prompt.text),
                "{} names AllSource",
                prompt.id
            );
        }
    }

    #[test]
    fn every_interrogation_prompt_names_the_brand_and_carries_ground_truth() {
        let set = interrogation();
        assert!(!set.prompts.is_empty());
        for prompt in &set.prompts {
            assert!(brand::names_allsource(&prompt.text), "{}", prompt.id);
            assert!(!prompt.claims.is_empty(), "{}", prompt.id);
            for claim in &prompt.claims {
                assert!(!claim.source.is_empty(), "{}", claim.id);
            }
        }
    }

    #[test]
    fn interrogation_set_covers_identity_boundary_and_source_quality() {
        let set = interrogation();
        assert_eq!(set.version, 3);
        assert_eq!(set.len(), 17);
        assert_eq!(set.claim_count(), 25);

        for id in [
            "int-qualified-identity",
            "int-disambiguation",
            "int-product-map",
            "int-canonical-source",
        ] {
            assert!(set.prompts.iter().any(|prompt| prompt.id == id), "{id}");
        }
    }

    #[test]
    fn every_claim_cites_a_path_that_exists_in_this_repository() {
        // A ground truth nobody can look up is a ground truth nobody can
        // correct. The path is relative to the repository root; this test file
        // sits at tooling/geo/geo-core/src/, so walk up four levels.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..");
        for prompt in interrogation().prompts {
            for claim in prompt.claims {
                // Strip an optional "#anchor" or " (note)" suffix.
                let path = claim.source.split([' ', '#']).next().unwrap_or_default();
                assert!(
                    root.join(path).exists(),
                    "{}: source {path:?} does not exist",
                    claim.id
                );
            }
        }
    }

    #[test]
    fn ids_are_unique_across_prompts_and_claims() {
        // Enforced by the parser; asserted here so a bad edit fails loudly.
        let mut ids: Vec<String> = Vec::new();
        for set in [sov(), interrogation()] {
            for prompt in set.prompts {
                assert!(!ids.contains(&prompt.id), "duplicate {}", prompt.id);
                ids.push(prompt.id.clone());
                for claim in prompt.claims {
                    assert!(!ids.contains(&claim.id), "duplicate {}", claim.id);
                    ids.push(claim.id);
                }
            }
        }
    }

    #[test]
    fn the_digest_changes_when_the_set_changes() {
        let a = PromptSet::parse(
            Family::Sov,
            "version = 1\nfamily = \"sov\"\n[[prompt]]\nid=\"a\"\nintent=\"category\"\ntext=\"x\"\n",
        )
        .expect("valid");
        let b = PromptSet::parse(
            Family::Sov,
            "version = 1\nfamily = \"sov\"\n[[prompt]]\nid=\"a\"\nintent=\"category\"\ntext=\"y\"\n",
        )
        .expect("valid");
        assert_ne!(a.digest, b.digest);
        assert_eq!(a.digest.len(), DIGEST_LEN);
    }

    #[test]
    fn a_run_id_carries_the_digest_and_the_repetition() {
        let set = sov();
        let run_id = set.run_id("2026-08-11", 2);
        assert!(run_id.contains(&set.digest), "{run_id}");
        assert!(run_id.ends_with("#r2"), "{run_id}");
        // Repetitions are distinct entities, one sweep.
        assert_ne!(run_id, set.run_id("2026-08-11", 3));
        assert_eq!(
            PromptSet::sweep_of(&run_id),
            PromptSet::sweep_of(&set.run_id("2026-08-11", 3))
        );
    }

    #[test]
    fn an_sov_prompt_that_names_us_is_rejected() {
        let err = PromptSet::parse(
            Family::Sov,
            "version = 1\nfamily = \"sov\"\n[[prompt]]\nid=\"a\"\nintent=\"category\"\ntext=\"is AllSource good?\"\n",
        )
        .expect_err("should be rejected");
        assert!(err.to_string().contains("must not name AllSource"), "{err}");
    }

    #[test]
    fn an_interrogation_prompt_without_a_claim_is_rejected() {
        let err = PromptSet::parse(
            Family::Interrogation,
            "version = 1\nfamily = \"interrogation\"\n[[prompt]]\nid=\"a\"\nintent=\"brand\"\ntext=\"what is AllSource?\"\n",
        )
        .expect_err("should be rejected");
        assert!(err.to_string().contains("ground-truth claim"), "{err}");
    }

    #[test]
    fn a_duplicate_prompt_id_is_rejected() {
        let err = PromptSet::parse(
            Family::Sov,
            "version = 1\nfamily = \"sov\"\n[[prompt]]\nid=\"a\"\nintent=\"category\"\ntext=\"x\"\n[[prompt]]\nid=\"a\"\nintent=\"problem\"\ntext=\"y\"\n",
        )
        .expect_err("should be rejected");
        assert!(err.to_string().contains("duplicate prompt id"), "{err}");
    }

    #[test]
    fn families_round_trip_through_their_cli_strings() {
        for family in Family::ALL {
            assert_eq!(Family::parse(family.as_str()), Some(family));
        }
        assert_eq!(Family::parse("both"), None);
    }

    #[test]
    fn severity_orders_worst_first() {
        assert!(Severity::Critical.weight() > Severity::High.weight());
        assert!(Severity::High.weight() > Severity::Medium.weight());
        assert!(Severity::Medium.weight() > Severity::Low.weight());
    }
}
