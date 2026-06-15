// SPDX-License-Identifier: Apache-2.0

//! Cross-file (stitch) Tier-B resolution.
//!
//! The per-file phase defers every cross-file reference as a [`PendingRef`].
//! This phase resolves them against a [`GlobalIndex`] — a leaf-name → SymbolIds
//! map that owns its ids so a future incremental store can maintain it across
//! edits. Each pending ref becomes at most one [`Confidence::Exact`] edge, only
//! when its `(name, segs)` lookup has a UNIQUE match — Tier-B never fakes
//! precision (zero or ambiguous → no edge).

use std::collections::HashMap;

use crate::graph::types::{Confidence, Edge, Provenance, Symbol};
use crate::symbol::SymbolId;

use super::super::namespaces_end_with;
use super::subgraph::PendingRef;

/// Global definition index: leaf name → the SymbolIds sharing that name.
/// Mirrors the `by_name` map the batch resolver builds, but owns SymbolIds so
/// it can be maintained incrementally (next unit adds add/remove).
pub(crate) struct GlobalIndex {
    by_name: HashMap<String, Vec<SymbolId>>,
}

impl GlobalIndex {
    /// Build from the full symbol set (batch path).
    pub(crate) fn from_symbols(symbols: &[Symbol]) -> Self {
        let mut by_name: HashMap<String, Vec<SymbolId>> = HashMap::new();
        for s in symbols {
            if let Some(n) = s.id.leaf_name() {
                by_name.entry(n.to_string()).or_default().push(s.id.clone());
            }
        }
        Self { by_name }
    }

    /// The UNIQUE SymbolId whose leaf name is `name` and whose namespace chain
    /// ends with `segs`; `None` if zero or two-or-more candidates match (never
    /// fake precision).
    fn unique_match(&self, name: &str, segs: &[String]) -> Option<&SymbolId> {
        self.by_name.get(name).and_then(|cands| {
            let mut it = cands.iter().filter(|id| namespaces_end_with(id, segs));
            match (it.next(), it.next()) {
                (Some(only), None) => Some(only), // exactly one match
                _ => None,                        // zero or ambiguous → no edge
            }
        })
    }
}

/// Resolve all pending cross-file refs into edges via the global index. One
/// [`Confidence::Exact`] [`Provenance::ScopeGraph`] edge per unique match.
pub(crate) fn stitch(pending: &[PendingRef], index: &GlobalIndex) -> Vec<Edge> {
    let mut edges = Vec::new();
    for p in pending {
        if let Some(matched_id) = index.unique_match(&p.name, &p.segs) {
            edges.push(Edge {
                from: p.from.clone(),
                to: matched_id.clone(),
                role: p.role,
                confidence: Confidence::Exact,
                provenance: Provenance::ScopeGraph,
                occ: p.occ.clone(),
            });
        }
    }
    edges
}
