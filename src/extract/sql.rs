// SPDX-License-Identifier: Apache-2.0

//! SQL extractor stub — scaffold only (unit S1).
//!
//! This unit wires the SQL artifact into the dispatch pipeline and emits the
//! per-file module symbol. Symbol and reference extraction (`CREATE TABLE`,
//! `CREATE VIEW`, column references, …) arrive in later units.
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.

use tree_sitter::{Language as TsLanguage, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::FileFacts;
use crate::lang::Language;

use super::Extractor;

/// Extracts SQL symbols and references (stub: module symbol only).
pub struct SqlExtractor;

impl Extractor for SqlExtractor {
    fn lang(&self) -> Language {
        Language::Sql
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_language = TsLanguage::from(tree_sitter_sequel::LANGUAGE);
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

        // S1 stub: emit only the module symbol; table/view/column extraction follows.
        let mod_sym = super::module_symbol(Language::Sql, &[], file, source.len());

        Ok(FileFacts {
            file: file.to_owned(),
            lang: Language::Sql.as_str().to_owned(),
            symbols: vec![mod_sym],
            references: Vec::new(),
            scopes: Vec::new(),
            bindings: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract_path;

    #[test]
    fn sql_stub_parses_and_emits_module_symbol() {
        let src = "CREATE TABLE users (id INT, name TEXT);";
        let facts = SqlExtractor.extract(src, "db/schema.sql").unwrap();

        assert_eq!(facts.lang, "sql");
        // Module symbol must be present.
        assert!(
            !facts.symbols.is_empty(),
            "expected at least the module symbol, got {:?}",
            facts.symbols
        );
        // The module symbol is keyed by file stem when no namespace path is given.
        let mod_sym = facts.symbols.iter().find(|s| s.name == "schema").unwrap();
        assert!(
            mod_sym.id.to_scip_string().contains("schema"),
            "module symbol SCIP string should contain the file stem; got: {}",
            mod_sym.id.to_scip_string()
        );
        // No references in the stub.
        assert!(facts.references.is_empty());
    }

    #[test]
    fn dispatch_routes_sql_extension() {
        let src = "CREATE TABLE orders (id INT);";
        let facts = extract_path("db/orders.sql", src).unwrap();
        assert_eq!(facts.lang, "sql");
    }
}
