// SPDX-License-Identifier: Apache-2.0

//! SCIP index reader for oracle generation. Feature-gated (`oracle-regen`).
//!
//! Reads a binary `index.scip` file and extracts location-only ref→def pairs
//! suitable for writing to `oracle.edges`.

use protobuf::Message as _;
use scip::types::Index;
use std::collections::HashMap;

/// Parse a binary SCIP index and return sorted, deduplicated location pairs
/// `(ref_path, ref_line, def_path, def_line)` where lines are 1-based and paths
/// are the SCIP document's case-relative `relative_path` (e.g. `alpha/alpha.go`).
///
/// - Pass 1: build a map from symbol string → (relative_path, 1-based line) for
///   every occurrence whose `symbol_roles` has the Definition bit set (`& 1 != 0`).
/// - Pass 2: for every non-definition occurrence whose symbol has a known
///   definition, emit a location pair. Self-loops (ref and def at the same
///   file + line) are dropped.
/// - Results are sorted and deduplicated for stable output.
pub fn oracle_edges_from_scip(bytes: &[u8]) -> Result<Vec<(String, u32, String, u32)>, String> {
    let index = Index::parse_from_bytes(bytes).map_err(|e| e.to_string())?;

    // Pass 1: collect definition sites keyed by symbol string.
    let mut def_by_symbol: HashMap<String, (String, u32)> = HashMap::new();
    for doc in &index.documents {
        let path = doc.relative_path.clone();
        for occ in &doc.occurrences {
            if occ.symbol.is_empty() {
                continue;
            }
            // SymbolRole::Definition bit = 1.
            if occ.symbol_roles & 1 != 0 {
                let line = occ.range.first().copied().unwrap_or(0) as u32 + 1;
                def_by_symbol
                    .entry(occ.symbol.clone())
                    .or_insert_with(|| (path.clone(), line));
            }
        }
    }

    // Pass 2: emit ref→def pairs for every non-definition occurrence.
    let mut edges: std::collections::BTreeSet<(String, u32, String, u32)> =
        std::collections::BTreeSet::new();
    for doc in &index.documents {
        let ref_path = doc.relative_path.clone();
        for occ in &doc.occurrences {
            if occ.symbol.is_empty() {
                continue;
            }
            // Skip definition occurrences.
            if occ.symbol_roles & 1 != 0 {
                continue;
            }
            if let Some((def_file, def_line)) = def_by_symbol.get(&occ.symbol) {
                let ref_line = occ.range.first().copied().unwrap_or(0) as u32 + 1;
                // Drop trivial self-loops (same file AND same line).
                if &ref_path == def_file && ref_line == *def_line {
                    continue;
                }
                edges.insert((ref_path.clone(), ref_line, def_file.clone(), *def_line));
            }
        }
    }

    Ok(edges.into_iter().collect())
}
