// SPDX-License-Identifier: Apache-2.0

//! Runs a case through code2graph and scores the result.
//!
//! This is the only part of the harness that touches code2graph's pipeline:
//! extract every source file in the case, resolve with the chosen tier, and
//! score the resulting graph against the case's ground truth. Everything else
//! (loading, scoring) is independent of the library.

use crate::corpus::Case;
use crate::score::{Scorecard, score, score_oracle};
use code2graph::{Resolver, extract_path};
use std::collections::BTreeMap;

/// Extract every file in `case`, resolve with `resolver`, and score the graph
/// against the case's ground truth.
///
/// Dispatches to [`score_oracle`] when the case has SCIP-oracle location pairs
/// (i.e. `oracle.edges` was present), and to [`score`] otherwise.
pub fn score_case<R: Resolver>(case: &Case, resolver: &R) -> Scorecard {
    let facts: Vec<_> = case
        .files
        .iter()
        .filter_map(|(name, src)| extract_path(name, src).ok())
        .collect();
    let graph = resolver.resolve(&facts);
    if !case.oracle.is_empty() {
        score_oracle(&graph, &case.oracle)
    } else {
        score(&graph, &case.expected)
    }
}

/// Aggregate scorecard over every case, grouped by language directory.
pub fn per_language<R: Resolver>(cases: &[Case], resolver: &R) -> BTreeMap<String, Scorecard> {
    let mut by_lang: BTreeMap<String, Scorecard> = BTreeMap::new();
    for case in cases {
        by_lang
            .entry(case.lang.clone())
            .or_default()
            .merge(&score_case(case, resolver));
    }
    by_lang
}

/// Single aggregate scorecard over the whole corpus.
pub fn corpus_total<R: Resolver>(cases: &[Case], resolver: &R) -> Scorecard {
    let mut total = Scorecard::default();
    for case in cases {
        total.merge(&score_case(case, resolver));
    }
    total
}
