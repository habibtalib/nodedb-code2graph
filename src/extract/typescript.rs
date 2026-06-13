// SPDX-License-Identifier: Apache-2.0

//! TypeScript extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: top-level **exported** declarations (`export function/class/
//! interface/type/enum/const`, including `export default function/class`).
//! Qualified identity follows the file's module path (`src/auth/jwt.ts` →
//! namespaces `src`,`auth`,`jwt`), so a symbol is `…/jwt/validateToken().`.
//! References: callee identifiers of `call_expression` nodes.
//!
//! `.tsx`/`.jsx` files are parsed with the TSX grammar, otherwise TypeScript.
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.
//!
//! The extraction core ([`extract_ecmascript`]) is shared with the JavaScript
//! extractor, which reuses the TypeScript grammar (a superset of JavaScript);
//! the two differ only in their language tag.

use tree_sitter::{Language as TsLanguage, Node, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{ByteSpan, FileFacts, Symbol, SymbolKind};
use crate::lang::Language;
use crate::symbol::{Descriptor, SymbolId};

use super::{Extractor, child_text, collect_call_references, node_text, one_line_signature};

/// Tree-sitter query capturing call-callee identifiers.
const CALL_QUERY: &str = r#"
(call_expression
  function: [
    (identifier) @callee
    (member_expression property: (property_identifier) @callee)
  ]
)
"#;

/// Extracts TypeScript symbols and references.
pub struct TypeScriptExtractor;

impl Extractor for TypeScriptExtractor {
    fn lang(&self) -> Language {
        Language::TypeScript
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        extract_ecmascript(source, file, Language::TypeScript)
    }
}

/// Shared TypeScript/JavaScript extraction core. The TypeScript grammar is a
/// superset of JavaScript, so both extractors parse with it; `lang` selects the
/// language tag and SCIP scheme. `.tsx`/`.jsx` files use the TSX grammar.
pub(super) fn extract_ecmascript(source: &str, file: &str, lang: Language) -> Result<FileFacts> {
    let ts_lang = if file.ends_with(".tsx") || file.ends_with(".jsx") {
        tree_sitter_typescript::LANGUAGE_TSX
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT
    };

    let ts_language = TsLanguage::from(ts_lang);
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
    let namespaces = module_namespaces(file);

    let symbols = collect_symbols(&root, bytes, file, &namespaces, lang);
    let references = collect_call_references(&root, &ts_language, CALL_QUERY, lang, bytes, file)?;

    Ok(FileFacts {
        file: file.to_owned(),
        lang: lang.as_str().to_owned(),
        symbols,
        references,
    })
}

/// Module path (namespace descriptors) from a source file path: all path
/// segments, with the final file extension stripped from the last segment.
fn module_namespaces(file: &str) -> Vec<String> {
    let mut parts: Vec<String> = file
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    if let Some(last) = parts.pop() {
        let stem = last
            .rsplit_once('.')
            .map_or(last.as_str(), |(stem, _)| stem);
        parts.push(stem.to_owned());
    }
    parts
}

fn collect_symbols(
    root: &Node,
    bytes: &[u8],
    file: &str,
    namespaces: &[String],
    lang: Language,
) -> Vec<Symbol> {
    let mut out = Vec::new();
    for stmt in root.children(&mut root.walk()) {
        if stmt.kind() != "export_statement" {
            continue;
        }
        // The exported declaration is a direct child of the export statement.
        for decl in stmt.children(&mut stmt.walk()) {
            emit_declaration(&decl, &stmt, bytes, file, namespaces, lang, &mut out);
        }
    }
    out
}

/// Append symbol(s) for one declaration node (a `lexical_declaration` may yield
/// several). `span_node` is the enclosing `export_statement`.
fn emit_declaration(
    decl: &Node,
    span_node: &Node,
    bytes: &[u8],
    file: &str,
    namespaces: &[String],
    lang: Language,
    out: &mut Vec<Symbol>,
) {
    let push = |out: &mut Vec<Symbol>, name: String, kind: SymbolKind, leaf: Descriptor| {
        let mut descriptors: Vec<Descriptor> = namespaces
            .iter()
            .cloned()
            .map(Descriptor::Namespace)
            .collect();
        descriptors.push(leaf);
        out.push(Symbol {
            id: SymbolId::global(lang.as_str(), descriptors),
            name,
            kind,
            file: file.to_owned(),
            line: (span_node.start_position().row + 1) as u32,
            span: ByteSpan {
                start: span_node.start_byte(),
                end: span_node.end_byte(),
            },
            signature: one_line_signature(node_text(decl, bytes), &['{']),
        });
    };

    match decl.kind() {
        "function_declaration" => {
            if let Some(n) = child_text(decl, "identifier", bytes) {
                push(
                    out,
                    n.clone(),
                    SymbolKind::Function,
                    Descriptor::Method {
                        name: n,
                        disambiguator: String::new(),
                    },
                );
            }
        }
        "class_declaration" => emit_named(decl, bytes, SymbolKind::Class, out, &push),
        "interface_declaration" => emit_named(decl, bytes, SymbolKind::Interface, out, &push),
        "type_alias_declaration" => emit_named(decl, bytes, SymbolKind::TypeAlias, out, &push),
        "enum_declaration" => {
            if let Some(n) = child_text(decl, "identifier", bytes) {
                push(out, n.clone(), SymbolKind::Enum, Descriptor::Type(n));
            }
        }
        "lexical_declaration" => {
            for vd in decl.children(&mut decl.walk()) {
                if vd.kind() != "variable_declarator" {
                    continue;
                }
                if let Some(n) = child_text(&vd, "identifier", bytes) {
                    push(out, n.clone(), SymbolKind::Const, Descriptor::Term(n));
                }
            }
        }
        _ => {}
    }
}

/// Emit a type-named declaration (class/interface/type-alias) named by a
/// `type_identifier`.
fn emit_named(
    decl: &Node,
    bytes: &[u8],
    kind: SymbolKind,
    out: &mut Vec<Symbol>,
    push: &impl Fn(&mut Vec<Symbol>, String, SymbolKind, Descriptor),
) {
    if let Some(n) = child_text(decl, "type_identifier", bytes) {
        push(out, n.clone(), kind, Descriptor::Type(n));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_exported_decls() {
        let src = "\
export function validateToken(tok: string): boolean { return helper(); }
export class Config {}
export interface Options { timeout: number; }
export const MAX = 3;
function internal() {}
";
        let facts = TypeScriptExtractor.extract(src, "src/auth/jwt.ts").unwrap();
        let by_name = |n: &str| facts.symbols.iter().find(|s| s.name == n).cloned();

        let vt = by_name("validateToken").unwrap();
        assert_eq!(
            vt.id.to_scip_string(),
            "codegraph    src/auth/jwt/validateToken()."
        );
        assert_eq!(vt.kind, SymbolKind::Function);

        assert_eq!(by_name("Config").unwrap().kind, SymbolKind::Class);
        assert_eq!(by_name("Options").unwrap().kind, SymbolKind::Interface);
        assert_eq!(by_name("MAX").unwrap().kind, SymbolKind::Const);
        // non-exported declarations are not symbols
        assert!(by_name("internal").is_none());
    }

    #[test]
    fn default_export_function_is_named() {
        let facts = TypeScriptExtractor
            .extract("export default function App() {}", "src/App.tsx")
            .unwrap();
        assert_eq!(facts.symbols.len(), 1);
        assert_eq!(facts.symbols[0].name, "App");
        assert_eq!(
            facts.symbols[0].id.to_scip_string(),
            "codegraph    src/App/App()."
        );
    }

    #[test]
    fn extracts_call_references() {
        let facts = TypeScriptExtractor
            .extract(
                "function main() { validateToken('t'); helper(); }",
                "src/main.ts",
            )
            .unwrap();
        let names: Vec<&str> = facts.references.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"validateToken"));
        assert!(names.contains(&"helper"));
    }
}
