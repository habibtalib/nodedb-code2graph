// SPDX-License-Identifier: Apache-2.0

//! Tier A resolver: fast, broad, name/scope based.
//!
//! Builds a `leaf-name → definitions` table across all files, attributes each
//! reference to the symbol whose span encloses it (the caller), and links it to
//! every definition sharing the callee's name. An ambiguous name that fans out
//! to several definitions tags each edge [`Confidence::NameOnly`]; a name with a
//! single global candidate is tagged [`Confidence::Scoped`]. This is the
//! recall-first baseline that works for every language without per-language
//! binding rules — no edges are dropped, only the confidence varies. A precise
//! resolver tags its edges [`Confidence::Exact`] instead.
//!
//! It returns neutral [`Edge`]s and never writes to storage.

use std::collections::HashMap;

use crate::graph::types::{CodeGraph, Confidence, Edge, EdgeKind, FileFacts, RefRole, Symbol};

use super::Resolver;

/// Maps a [`RefRole`] to the corresponding [`EdgeKind`].
///
/// The match is exhaustive so that adding a new `RefRole` variant forces a
/// compile error here, keeping the mapping intentional and explicit.
fn edge_kind(role: RefRole) -> EdgeKind {
    match role {
        RefRole::Call => EdgeKind::Calls,
        RefRole::Inherit => EdgeKind::Inherits,
        RefRole::Import => EdgeKind::Imports,
    }
}

/// Name-table resolver. See module docs.
#[derive(Debug, Default, Clone, Copy)]
pub struct SymbolTableResolver;

impl Resolver for SymbolTableResolver {
    fn resolve(&self, files: &[FileFacts]) -> CodeGraph {
        // leaf name → indices into the flattened symbol list
        let mut symbols: Vec<Symbol> = Vec::new();
        for f in files {
            symbols.extend(f.symbols.iter().cloned());
        }

        let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, s) in symbols.iter().enumerate() {
            if let Some(name) = s.id.leaf_name() {
                by_name.entry(name).or_default().push(i);
            }
        }

        // Per-file symbol index for caller attribution (span containment).
        let mut by_file: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, s) in symbols.iter().enumerate() {
            by_file.entry(s.file.as_str()).or_default().push(i);
        }

        let mut edges: Vec<Edge> = Vec::new();
        for f in files {
            let file_syms = by_file.get(f.file.as_str());
            for r in &f.references {
                // The caller: innermost symbol in this file whose span holds the ref.
                let Some(from_idx) = file_syms.and_then(|idxs| {
                    idxs.iter()
                        .copied()
                        .filter(|&i| symbols[i].span.contains(r.occ.byte))
                        .min_by_key(|&i| symbols[i].span.len())
                }) else {
                    continue; // reference not inside any extracted symbol — unattributable
                };

                let Some(targets) = by_name.get(r.name.as_str()) else {
                    continue; // unresolved: no definition with this name
                };

                // Count non-self candidates first to determine confidence,
                // then iterate the same filtered set to emit edges — no
                // intermediate Vec needed.
                let non_self_count = targets.iter().filter(|&&i| i != from_idx).count();

                let confidence = if non_self_count == 1 {
                    Confidence::Scoped
                } else {
                    Confidence::NameOnly
                };

                for &to_idx in targets.iter().filter(|&&i| i != from_idx) {
                    edges.push(Edge {
                        from: symbols[from_idx].id.clone(),
                        to: symbols[to_idx].id.clone(),
                        kind: edge_kind(r.role),
                        confidence,
                        occ: r.occ.clone(),
                    });
                }
            }
        }

        CodeGraph { symbols, edges }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::Extractor;
    use crate::extract::JavaExtractor;
    use crate::extract::RustExtractor;

    #[test]
    fn resolves_cross_file_call() {
        let lib = RustExtractor
            .extract("pub fn helper() -> u32 { 1 }", "src/util.rs")
            .unwrap();
        let main = RustExtractor
            .extract("pub fn run() -> u32 { helper() }", "src/main.rs")
            .unwrap();

        let graph = SymbolTableResolver.resolve(&[lib, main]);

        // one Calls edge: run → helper
        let calls: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .collect();
        assert_eq!(calls.len(), 1);
        let e = calls[0];
        assert!(e.from.to_scip_string().ends_with("run()."));
        assert!(e.to.to_scip_string().ends_with("util/helper()."));
        assert_eq!(e.confidence, Confidence::Scoped);
        assert_eq!(e.occ.file, "src/main.rs");
    }

    #[test]
    fn unresolved_calls_produce_no_edge() {
        let main = RustExtractor
            .extract("pub fn run() { nonexistent_fn() }", "src/main.rs")
            .unwrap();
        let graph = SymbolTableResolver.resolve(&[main]);
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn resolves_cross_file_inheritance() {
        let base = JavaExtractor
            .extract("package p; public class Base {}", "src/p/Base.java")
            .unwrap();
        let sub = JavaExtractor
            .extract(
                "package p; public class Sub extends Base {}",
                "src/p/Sub.java",
            )
            .unwrap();

        let graph = SymbolTableResolver.resolve(&[base, sub]);

        // exactly one Inherits edge: Sub → Base
        let inherits: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Inherits)
            .collect();
        assert_eq!(inherits.len(), 1);
        let e = inherits[0];
        assert!(
            e.from.to_scip_string().ends_with("p/Sub#"),
            "from was: {}",
            e.from.to_scip_string()
        );
        assert!(
            e.to.to_scip_string().ends_with("p/Base#"),
            "to was: {}",
            e.to.to_scip_string()
        );
        assert_eq!(e.confidence, Confidence::Scoped);
        assert_eq!(e.occ.file, "src/p/Sub.java");
    }

    #[test]
    fn resolves_cross_file_rust_trait_impl_inheritance() {
        // File A defines the trait.
        let greet = RustExtractor
            .extract("pub trait Greet {}", "src/greet.rs")
            .unwrap();
        // File B defines the struct + its trait impl.
        let p = RustExtractor
            .extract("pub struct P;\nimpl Greet for P {}", "src/p.rs")
            .unwrap();

        let graph = SymbolTableResolver.resolve(&[greet, p]);

        // Exactly one Inherits edge: P → Greet
        let inherits: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Inherits)
            .collect();
        assert_eq!(
            inherits.len(),
            1,
            "expected 1 Inherits edge, got {:?}",
            inherits.len()
        );
        let e = inherits[0];
        assert!(
            e.from.to_scip_string().ends_with("p/P#") || e.from.to_scip_string().ends_with("P#"),
            "unexpected from: {}",
            e.from.to_scip_string()
        );
        assert!(
            e.to.to_scip_string().ends_with("greet/Greet#")
                || e.to.to_scip_string().ends_with("Greet#"),
            "unexpected to: {}",
            e.to.to_scip_string()
        );
        assert_eq!(e.confidence, Confidence::Scoped);
        assert_eq!(e.occ.file, "src/p.rs");
    }

    #[test]
    fn resolves_cross_file_python_import_edge() {
        use crate::extract::PythonExtractor;

        // File A: src/pkg/models.py defines class Config.
        let a = PythonExtractor
            .extract("class Config:\n    pass\n", "src/pkg/models.py")
            .unwrap();

        // File B: src/app.py imports Config from pkg.models.
        let b = PythonExtractor
            .extract("from pkg.models import Config\n", "src/app.py")
            .unwrap();

        let graph = SymbolTableResolver.resolve(&[a, b]);

        // Exactly one Imports edge: module(app) → Config
        let imports: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(
            imports.len(),
            1,
            "expected one Imports edge, got {:?}",
            imports.len()
        );
        let e = imports[0];
        assert!(
            e.from.to_scip_string().ends_with("app/"),
            "from (module) was: {}",
            e.from.to_scip_string()
        );
        assert!(
            e.to.to_scip_string().ends_with("Config#"),
            "to was: {}",
            e.to.to_scip_string()
        );
        assert_eq!(e.confidence, Confidence::Scoped);
    }

    #[test]
    fn resolves_import_edge_from_module() {
        use crate::graph::types::{Occurrence, RefRole, Reference};

        // File A defines `Config`.
        let a = RustExtractor
            .extract("pub struct Config {}", "src/conf.rs")
            .unwrap();

        // File B's module imports it. The extractor gives B a module symbol
        // spanning the whole file; we inject an Import reference whose byte sits
        // in the leading comment — inside the module span but not inside any
        // smaller symbol — so the resolver attributes the edge's source to the
        // module, exactly as a real top-level `use`/`import` would.
        let mut b = RustExtractor
            .extract("// uses Config\npub fn run() {}", "src/app.rs")
            .unwrap();
        b.references.push(Reference {
            name: "Config".to_owned(),
            occ: Occurrence {
                file: "src/app.rs".to_owned(),
                line: 1,
                col: 0,
                byte: 0,
            },
            role: RefRole::Import,
        });

        let graph = SymbolTableResolver.resolve(&[a, b]);

        // Exactly one Imports edge: module(app) → Config
        let imports: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1, "expected one Imports edge");
        let e = imports[0];
        assert!(
            e.from.to_scip_string().ends_with("app/"),
            "from (module) was: {}",
            e.from.to_scip_string()
        );
        assert!(
            e.to.to_scip_string().ends_with("conf/Config#"),
            "to was: {}",
            e.to.to_scip_string()
        );
        assert_eq!(e.confidence, Confidence::Scoped);
    }

    #[test]
    fn ambiguous_name_fan_out_stays_name_only() {
        // Two files each define a function with the same leaf name "process".
        // A third file calls "process" — the resolver must emit edges to BOTH
        // definitions and tag them NameOnly (ambiguous fan-out, not Scoped).
        let a = RustExtractor
            .extract("pub fn process() -> u32 { 1 }", "src/mod_a.rs")
            .unwrap();
        let b = RustExtractor
            .extract("pub fn process() -> u32 { 2 }", "src/mod_b.rs")
            .unwrap();
        let caller = RustExtractor
            .extract("pub fn run() { process() }", "src/main.rs")
            .unwrap();

        let graph = SymbolTableResolver.resolve(&[a, b, caller]);

        // Filter to Calls edges only (exclude any Inherits/Imports noise).
        let calls: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .collect();

        // Recall preserved: both definitions must be reachable.
        assert_eq!(
            calls.len(),
            2,
            "expected 2 fan-out edges, got {}",
            calls.len()
        );

        // Every fan-out edge must stay NameOnly — not promoted to Scoped.
        for e in &calls {
            assert_eq!(
                e.confidence,
                Confidence::NameOnly,
                "ambiguous fan-out edge should be NameOnly, got {:?}",
                e.confidence
            );
        }

        // Both targets should be the two "process" definitions.
        let targets: std::collections::HashSet<String> =
            calls.iter().map(|e| e.to.to_scip_string()).collect();
        assert!(
            targets.iter().any(|s| s.ends_with("mod_a/process().")),
            "missing mod_a target; got: {:?}",
            targets
        );
        assert!(
            targets.iter().any(|s| s.ends_with("mod_b/process().")),
            "missing mod_b target; got: {:?}",
            targets
        );
    }
}
