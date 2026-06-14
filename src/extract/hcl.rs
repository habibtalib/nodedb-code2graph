// SPDX-License-Identifier: Apache-2.0

//! HCL/Terraform extractor — stub that emits the module symbol only.
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.
//! Resource block extraction (e.g. `resource "aws_instance" "web"`) is a
//! later unit; this stub establishes the scaffold and wiring.

use tree_sitter::{Language as TsLanguage, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::FileFacts;
use crate::lang::Language;

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
        let _tree = parser
            .parse(source, None)
            .ok_or_else(|| CodegraphError::Parse {
                path: file.to_owned(),
            })?;

        let mod_sym = super::module_symbol(Language::Hcl, &[], file, source.len());

        Ok(FileFacts {
            file: file.to_owned(),
            lang: Language::Hcl.as_str().to_owned(),
            symbols: vec![mod_sym],
            references: Vec::new(),
            scopes: Vec::new(),
            bindings: Vec::new(),
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract_path;
    use crate::graph::types::SymbolKind;

    #[test]
    fn hcl_stub_parses_and_emits_module_symbol() {
        let src = r#"resource "aws_instance" "web" {}"#;
        let facts = HclExtractor.extract(src, "infra/main.tf").unwrap();

        assert_eq!(facts.lang, "hcl");
        assert!(
            !facts.symbols.is_empty(),
            "expected at least the module symbol, got {:?}",
            facts.symbols
        );
        let mod_sym = facts
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module)
            .expect("expected a Module symbol");
        assert!(
            mod_sym.id.to_scip_string().contains("main"),
            "module symbol SCIP string should contain the file stem 'main'; got: {}",
            mod_sym.id.to_scip_string()
        );
        assert!(
            facts.references.is_empty(),
            "stub should emit no references"
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

    #[test]
    fn empty_hcl_does_not_panic_and_returns_module_symbol() {
        let facts = HclExtractor.extract("", "infra/empty.tf").unwrap();
        assert!(
            facts.symbols.iter().any(|s| s.kind == SymbolKind::Module),
            "empty HCL should still produce the module symbol"
        );
        assert!(facts.references.is_empty(), "stub emits no references");
    }
}
