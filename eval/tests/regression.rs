// SPDX-License-Identifier: Apache-2.0

//! Regression guard for resolution quality.
//!
//! These assertions turn the harness into a non-regression gate: they encode the
//! *invariants* the tiers must uphold, not brittle exact rates. A drop in
//! Tier-A recall, a Tier-B false positive, or an erosion of the scope-tier's
//! precision advantage all fail the build here.

use codegraph::{ScopeGraphResolver, SymbolTableResolver};
use codegraph_eval::corpus::{Case, load_corpus};
use codegraph_eval::runner::{corpus_total, per_language};
use std::path::Path;

fn corpus() -> Vec<Case> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    load_corpus(&root).expect("corpus loads")
}

#[test]
fn corpus_is_non_empty() {
    assert!(!corpus().is_empty(), "eval corpus must contain cases");
}

#[test]
fn tier_a_is_recall_first() {
    // Tier-A is the recall-first tier: it must find every ground-truth edge in
    // the corpus (it may over-connect, which costs precision, not recall).
    let total = corpus_total(&corpus(), &SymbolTableResolver);
    assert_eq!(
        total.recall(),
        1.0,
        "Tier-A must retain full recall (TP={}, FN={})",
        total.true_positives,
        total.false_negatives
    );
}

#[test]
fn tier_b_never_fakes_precision() {
    // Tier-B's contract: it emits an edge only when it can resolve it precisely,
    // so it must have zero false positives across the corpus.
    let total = corpus_total(&corpus(), &ScopeGraphResolver);
    assert_eq!(
        total.false_positives, 0,
        "Tier-B must not emit false positives"
    );
    assert_eq!(total.precision(), 1.0, "Tier-B precision must be perfect");
}

#[test]
fn scope_tier_beats_name_tier_on_precision_where_it_resolves() {
    // The #1 thesis: on a language with scope extraction and genuine ambiguity
    // (rust), the scope tier is strictly more precise than the name tier.
    let cases = corpus();
    let a = per_language(&cases, &SymbolTableResolver);
    let b = per_language(&cases, &ScopeGraphResolver);
    let (Some(a_rust), Some(b_rust)) = (a.get("rust"), b.get("rust")) else {
        panic!("corpus must include rust cases");
    };
    assert!(
        b_rust.precision() > a_rust.precision(),
        "Tier-B precision ({:.2}) must beat Tier-A ({:.2}) on rust",
        b_rust.precision(),
        a_rust.precision()
    );
    // And it does so without inventing edges: every Tier-B rust edge is correct.
    assert_eq!(b_rust.false_positives, 0);
}
