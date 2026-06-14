// SPDX-License-Identifier: Apache-2.0

//! Shared helpers used by more than one resolver.
//!
//! Resolver-internal utilities that don't belong to any single tier. Kept here
//! so Tier-A and Tier-B share one definition of caller attribution.

use crate::graph::types::Symbol;

/// Index (into `symbols`) of the innermost symbol whose span contains `byte`,
/// considering only the symbols listed in `file_indices`. `None` if no symbol
/// encloses the byte. Innermost = smallest containing span.
pub(crate) fn enclosing_symbol_index(
    symbols: &[Symbol],
    file_indices: &[usize],
    byte: usize,
) -> Option<usize> {
    file_indices
        .iter()
        .copied()
        .filter(|&i| symbols[i].span.contains(byte))
        .min_by_key(|&i| symbols[i].span.len())
}
