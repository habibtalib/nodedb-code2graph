// SPDX-License-Identifier: Apache-2.0

//! HCL/Terraform extractor — extracts block symbols (resource, data, module)
//! via tree-sitter-hcl.
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.
//! References (H3) are deferred to a later unit; this unit extracts symbols only.

use tree_sitter::{Language as TsLanguage, Node, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{ByteSpan, FileFacts, Symbol, SymbolKind};
use crate::lang::Language;
use crate::symbol::{Descriptor, SymbolId};

use super::Extractor;

/// Extracts HCL/Terraform symbols and references.
pub struct HclExtractor;

impl Extractor for HclExtractor {
    fn lang(&self) -> Language {
        Language::Hcl
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_language = TsLanguage::from(tree_sitter_hcl::LANGUAGE);
        let mut parser = Parser::new();
        parser
            .set_language(&ts_language)
            .map_err(|_| CodegraphError::Parse {
                path: file.to_owned(),
            })?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| CodegraphError::Parse {
                path: file.to_owned(),
            })?;

        let root = tree.root_node();
        let bytes = source.as_bytes();

        let mut symbols = collect_symbols(&root, bytes, file);
        let mod_sym = super::module_symbol(Language::Hcl, &[], file, source.len());
        symbols.push(mod_sym);

        Ok(FileFacts {
            file: file.to_owned(),
            lang: Language::Hcl.as_str().to_owned(),
            symbols,
            references: Vec::new(),
            scopes: Vec::new(),
            bindings: Vec::new(),
        })
    }
}

// ── Symbol extraction ─────────────────────────────────────────────────────────

/// Extract `(block_type, labels)` from a `block` node.
///
/// Walk named children: the first `identifier` child is the block type; every
/// subsequent `string_lit` or `identifier` child (in order) before a
/// `block_start` child is a label. Returns `None` if the block has no type
/// identifier.
fn block_type_and_labels(block: &Node, bytes: &[u8]) -> Option<(String, Vec<String>)> {
    // Collect all named children up front into a Vec to avoid borrow conflicts
    // with tree-sitter's walk cursor.
    let named: Vec<Node> = {
        let mut cursor = block.walk();
        block.named_children(&mut cursor).collect()
    };

    // First named child must be an `identifier` — the block type.
    let first = named.first()?;
    if first.kind() != "identifier" {
        return None;
    }
    let block_type = super::node_text(first, bytes).to_owned();

    // Collect label children: `string_lit` or `identifier` until `block_start`.
    let mut labels = Vec::new();
    for child in named.iter().skip(1) {
        match child.kind() {
            "string_lit" => {
                labels.push(super::unquote(super::node_text(child, bytes)).to_owned());
            }
            "identifier" => {
                // An unquoted label identifier (rare in practice, but valid HCL).
                labels.push(super::node_text(child, bytes).to_owned());
            }
            "block_start" | "body" | "block_end" | "object_start" | "object_end" => break,
            _ => {
                // Skip unknown node kinds; stop if it looks like we're past the labels.
            }
        }
    }

    Some((block_type, labels))
}

/// Walk the root `body` and collect top-level block symbols.
///
/// Only processes direct `block` children of the root `body` — does NOT recurse
/// into block bodies (nested blocks like `lifecycle` are config, not declarations).
///
/// `.tfvars` files may parse to an `object` root rather than a `body` — guard:
/// if the root's first named child is not `body`, emit no block symbols.
fn collect_symbols(root: &Node, bytes: &[u8], file: &str) -> Vec<Symbol> {
    // Find the `body` child of `config_file`.
    let body = {
        let mut cursor = root.walk();
        root.named_children(&mut cursor)
            .find(|c| c.kind() == "body")
    };
    let Some(body) = body else {
        // Root has no `body` (e.g. a `.tfvars` JSON file parses as `object`).
        return Vec::new();
    };

    let top_level_blocks: Vec<Node> = {
        let mut cursor = body.walk();
        body.named_children(&mut cursor).collect()
    };

    let mut out = Vec::new();
    for block in &top_level_blocks {
        if block.kind() != "block" {
            continue;
        }
        if let Some(sym) = extract_block_symbol(block, bytes, file) {
            out.push(sym);
        }
    }
    out
}

/// Attempt to extract a [`Symbol`] from a top-level HCL `block` node.
///
/// Dispatch on the block type:
/// - `resource "T" "N"` → `SymbolKind::Resource`, SCIP `T/N#`
/// - `data "T" "N"`     → `SymbolKind::Resource`, SCIP `data/T/N#`
/// - `module "N"`        → `SymbolKind::Module`,   SCIP `module/N#`
/// - All others (variable/output/provider/locals/terraform/…) → skipped.
///   v1 boundary: these block types are recognised by Terraform but deferred
///   until a later unit defines their symbol taxonomy.
fn extract_block_symbol(block: &Node, bytes: &[u8], file: &str) -> Option<Symbol> {
    let (block_type, labels) = block_type_and_labels(block, bytes)?;

    let sig = super::one_line_signature(super::node_text(block, bytes), &['{']);
    let line = (block.start_position().row + 1) as u32;
    let span = ByteSpan {
        start: block.start_byte(),
        end: block.end_byte(),
    };

    match block_type.as_str() {
        "resource" => {
            // Expects exactly 2 labels: type ("aws_instance") and name ("web").
            if labels.len() < 2 {
                return None; // Malformed — skip gracefully.
            }
            let res_type = &labels[0];
            let res_name = &labels[1];
            let descriptors = vec![
                Descriptor::Namespace(res_type.clone()),
                Descriptor::Type(res_name.clone()),
            ];
            Some(Symbol {
                id: SymbolId::global(Language::Hcl.as_str(), descriptors),
                name: res_name.clone(),
                kind: SymbolKind::Resource,
                file: file.to_owned(),
                line,
                span,
                signature: sig,
            })
        }
        "data" => {
            // Expects exactly 2 labels: data-source type and name.
            // SCIP: `data/T/N#` — the `data` namespace prevents collision with
            // a resource of the same type/name, mirroring Terraform's `data.T.N`
            // reference form.
            if labels.len() < 2 {
                return None; // Malformed — skip gracefully.
            }
            let src_type = &labels[0];
            let src_name = &labels[1];
            let descriptors = vec![
                Descriptor::Namespace("data".to_owned()),
                Descriptor::Namespace(src_type.clone()),
                Descriptor::Type(src_name.clone()),
            ];
            Some(Symbol {
                id: SymbolId::global(Language::Hcl.as_str(), descriptors),
                name: src_name.clone(),
                kind: SymbolKind::Resource,
                file: file.to_owned(),
                line,
                span,
                signature: sig,
            })
        }
        "module" => {
            // Expects exactly 1 label: the module instance name.
            // SCIP: `module/N#`
            if labels.is_empty() {
                return None; // Malformed — skip gracefully.
            }
            let mod_name = &labels[0];
            let descriptors = vec![
                Descriptor::Namespace("module".to_owned()),
                Descriptor::Type(mod_name.clone()),
            ];
            Some(Symbol {
                id: SymbolId::global(Language::Hcl.as_str(), descriptors),
                name: mod_name.clone(),
                kind: SymbolKind::Module,
                file: file.to_owned(),
                line,
                span,
                signature: sig,
            })
        }
        // v1 boundary: variable, output, provider, locals, terraform, and any
        // other block types are deferred — they are recognised by Terraform but
        // their symbol taxonomy (kind, descriptor shape) is left for a later unit.
        _ => None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract_path;
    use crate::graph::types::SymbolKind;

    fn scip(sym: &Symbol) -> String {
        sym.id.to_scip_string()
    }

    fn find_by_name<'a>(symbols: &'a [Symbol], name: &str) -> Option<&'a Symbol> {
        symbols.iter().find(|s| s.name == name)
    }

    // ── Dispatch / module symbol ──────────────────────────────────────────────

    #[test]
    fn hcl_emits_module_symbol() {
        let src = r#"resource "aws_instance" "web" {}"#;
        let facts = HclExtractor.extract(src, "infra/main.tf").unwrap();

        assert_eq!(facts.lang, "hcl");
        let mod_sym = facts
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module && s.name == "main")
            .expect("expected a Module symbol named 'main'");
        assert!(
            mod_sym.id.to_scip_string().contains("main"),
            "module symbol SCIP string should contain 'main'; got: {}",
            mod_sym.id.to_scip_string()
        );
    }

    #[test]
    fn dispatch_routes_tf_extension() {
        let src = r#"resource "aws_instance" "web" {}"#;
        let facts = extract_path("infra/main.tf", src).unwrap();
        assert_eq!(facts.lang, "hcl");
    }

    #[test]
    fn dispatch_routes_hcl_extension() {
        let src = r#"variable "region" { default = "us-east-1" }"#;
        let facts = extract_path("infra/vars.hcl", src).unwrap();
        assert_eq!(facts.lang, "hcl");
    }

    #[test]
    fn dispatch_routes_tfvars_extension() {
        let src = r#"region = "us-east-1""#;
        let facts = extract_path("infra/prod.tfvars", src).unwrap();
        assert_eq!(facts.lang, "hcl");
    }

    // ── resource block ────────────────────────────────────────────────────────

    #[test]
    fn resource_block_emits_resource_symbol() {
        let src = r#"resource "aws_instance" "web" {}"#;
        let facts = HclExtractor.extract(src, "infra/main.tf").unwrap();

        let sym = find_by_name(&facts.symbols, "web").expect("expected 'web' Resource symbol");
        assert_eq!(sym.kind, SymbolKind::Resource);
        assert!(
            scip(sym).ends_with("aws_instance/web#"),
            "resource SCIP should end with 'aws_instance/web#'; got: {}",
            scip(sym)
        );
    }

    // ── data block ────────────────────────────────────────────────────────────

    #[test]
    fn data_block_emits_resource_symbol_with_data_namespace() {
        let src = r#"data "aws_ami" "ubuntu" {}"#;
        let facts = HclExtractor.extract(src, "infra/main.tf").unwrap();

        let sym =
            find_by_name(&facts.symbols, "ubuntu").expect("expected 'ubuntu' Resource symbol");
        assert_eq!(sym.kind, SymbolKind::Resource);
        assert!(
            scip(sym).ends_with("data/aws_ami/ubuntu#"),
            "data SCIP should end with 'data/aws_ami/ubuntu#'; got: {}",
            scip(sym)
        );
    }

    // ── module block ──────────────────────────────────────────────────────────

    #[test]
    fn module_block_emits_module_symbol() {
        let src = r#"module "vpc" { source = "./vpc" }"#;
        let facts = HclExtractor.extract(src, "infra/main.tf").unwrap();

        // There will be two Module-kind symbols: the file module symbol ("main")
        // and the module block symbol ("vpc"). Find the one named "vpc".
        let sym = facts
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module && s.name == "vpc")
            .expect("expected a Module symbol named 'vpc'");
        assert!(
            scip(sym).ends_with("module/vpc#"),
            "module SCIP should end with 'module/vpc#'; got: {}",
            scip(sym)
        );
    }

    // ── v1 boundary: variable skipped ────────────────────────────────────────

    #[test]
    fn variable_block_alone_emits_no_block_symbol() {
        // `variable` is deferred (v1 boundary). Only the file module symbol appears.
        let src = r#"variable "region" {}"#;
        let facts = HclExtractor.extract(src, "infra/vars.tf").unwrap();

        // No Resource or non-file-module Module symbol.
        let block_syms: Vec<_> = facts
            .symbols
            .iter()
            .filter(|s| s.name == "region")
            .collect();
        assert!(
            block_syms.is_empty(),
            "variable block should produce no symbol in v1; got: {:?}",
            block_syms
        );
        // The file module symbol must still be present.
        assert!(
            facts.symbols.iter().any(|s| s.kind == SymbolKind::Module),
            "the file module symbol should still be present"
        );
    }

    // ── multi-block file ──────────────────────────────────────────────────────

    #[test]
    fn multi_block_file_emits_all_three_symbols() {
        let src = r#"
resource "aws_instance" "web" {}
data "aws_ami" "ubuntu" {}
module "vpc" { source = "./vpc" }
"#;
        let facts = HclExtractor.extract(src, "infra/main.tf").unwrap();

        let web = find_by_name(&facts.symbols, "web").expect("expected 'web'");
        assert_eq!(web.kind, SymbolKind::Resource);
        assert!(
            scip(web).ends_with("aws_instance/web#"),
            "got: {}",
            scip(web)
        );

        let ubuntu = find_by_name(&facts.symbols, "ubuntu").expect("expected 'ubuntu'");
        assert_eq!(ubuntu.kind, SymbolKind::Resource);
        assert!(
            scip(ubuntu).ends_with("data/aws_ami/ubuntu#"),
            "got: {}",
            scip(ubuntu)
        );

        let vpc = facts
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module && s.name == "vpc")
            .expect("expected 'vpc'");
        assert!(scip(vpc).ends_with("module/vpc#"), "got: {}", scip(vpc));
    }

    // ── empty / malformed ─────────────────────────────────────────────────────

    #[test]
    fn empty_hcl_does_not_panic_and_returns_module_symbol() {
        let facts = HclExtractor.extract("", "infra/empty.tf").unwrap();
        assert!(
            facts.symbols.iter().any(|s| s.kind == SymbolKind::Module),
            "empty HCL should still produce the module symbol"
        );
        assert!(facts.references.is_empty(), "no references emitted in H2");
    }

    #[test]
    fn malformed_hcl_does_not_panic() {
        let facts = HclExtractor
            .extract("THIS IS NOT VALID HCL !!!", "infra/bad.tf")
            .unwrap();
        assert!(
            facts.symbols.iter().any(|s| s.kind == SymbolKind::Module),
            "malformed HCL should still return Ok with the module symbol"
        );
    }
}
