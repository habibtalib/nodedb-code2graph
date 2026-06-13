// SPDX-License-Identifier: Apache-2.0

//! The [`Resolver`] trait — the tier seam every resolver implements.

use crate::graph::{CodeGraph, FileFacts};

/// Links references to definitions. Pure: no I/O, deterministic.
pub trait Resolver {
    /// Resolve `files` into a graph of symbols and confidence-tagged edges.
    fn resolve(&self, files: &[FileFacts]) -> CodeGraph;
}
