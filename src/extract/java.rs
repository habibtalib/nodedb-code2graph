// SPDX-License-Identifier: Apache-2.0

//! Java extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: **public** top-level type declarations (`class`, `interface`,
//! `enum`, `record`, `@interface`) and their public members (methods,
//! constructors, fields). Interface and annotation-type members are treated as
//! implicitly public. Qualified identity follows the `package` declaration;
//! files without a package declaration fall back to a path-derived namespace.
//! References: callee identifiers of `method_invocation` and
//! `object_creation_expression` nodes.
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.

use tree_sitter::{Language as TsLanguage, Node, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{ByteSpan, FileFacts, Symbol, SymbolKind};
use crate::lang::Language;
use crate::symbol::{Descriptor, SymbolId};

use super::{Extractor, collect_call_references, field_text, node_text, one_line_signature};

/// Tree-sitter query capturing call-callee identifiers.
const CALL_QUERY: &str = r#"
[
  (method_invocation name: (identifier) @callee)
  (object_creation_expression type: (type_identifier) @callee)
]
"#;

/// Extracts Java symbols and references.
pub struct JavaExtractor;

impl Extractor for JavaExtractor {
    fn lang(&self) -> Language {
        Language::Java
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_language = TsLanguage::from(tree_sitter_java::LANGUAGE);
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
        let namespaces = java_namespaces(&root, bytes, file);

        let symbols = collect_symbols(&root, bytes, file, &namespaces);
        let references =
            collect_call_references(&root, &ts_language, CALL_QUERY, Language::Java, bytes, file)?;

        Ok(FileFacts {
            file: file.to_owned(),
            lang: Language::Java.as_str().to_owned(),
            symbols,
            references,
        })
    }
}

/// Derive namespace descriptors from the `package` declaration, falling back to
/// path-derived segments when no `package` statement is present.
///
/// With a package: `com.example.auth` → `["com", "example", "auth"]`.
/// Without: `src/com/example/auth/SessionManager.java` → `["com", "example",
/// "auth", "SessionManager"]` (same algorithm as the Go extractor).
fn java_namespaces(root: &Node, bytes: &[u8], file: &str) -> Vec<String> {
    // Look for a package_declaration among the root's direct children.
    for child in root.children(&mut root.walk()) {
        if child.kind() != "package_declaration" {
            continue;
        }
        // The package name is a direct child: either `scoped_identifier` (e.g.
        // `com.example.auth`) or a bare `identifier` (e.g. `auth`).
        for pkg_child in child.children(&mut child.walk()) {
            match pkg_child.kind() {
                "scoped_identifier" | "identifier" => {
                    let text = node_text(&pkg_child, bytes);
                    return text
                        .split('.')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
                _ => {}
            }
        }
    }

    // Fallback: derive from file path (strips `.java`, strips leading `src/`).
    let p = file.strip_suffix(".java").unwrap_or(file);
    let p = p.strip_prefix("src/").unwrap_or(p);
    p.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn collect_symbols(root: &Node, bytes: &[u8], file: &str, namespaces: &[String]) -> Vec<Symbol> {
    let mut out = Vec::new();

    // Walk root's direct children for top-level type declarations.
    for child in root.children(&mut root.walk()) {
        let type_kind = match child.kind() {
            k @ ("class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration") => k,
            _ => continue,
        };

        // Gate: only public types.
        if !is_public(&child, bytes) {
            continue;
        }

        let Some(type_name) = field_text(&child, "name", bytes) else {
            continue;
        };

        let type_sym_kind = match type_kind {
            "class_declaration" | "record_declaration" => SymbolKind::Class,
            "interface_declaration" | "annotation_type_declaration" => SymbolKind::Interface,
            "enum_declaration" => SymbolKind::Enum,
            _ => SymbolKind::Class,
        };

        // Emit the type symbol.
        let mut type_descriptors: Vec<Descriptor> = namespaces
            .iter()
            .cloned()
            .map(Descriptor::Namespace)
            .collect();
        type_descriptors.push(Descriptor::Type(type_name.clone()));
        out.push(Symbol {
            id: SymbolId::global(Language::Java.as_str(), type_descriptors),
            name: type_name.clone(),
            kind: type_sym_kind,
            file: file.to_owned(),
            line: (child.start_position().row + 1) as u32,
            span: ByteSpan {
                start: child.start_byte(),
                end: child.end_byte(),
            },
            signature: one_line_signature(node_text(&child, bytes), &['{', ';']),
        });

        // Members are implicitly public for interfaces and annotation types.
        let implicit_public = matches!(
            type_kind,
            "interface_declaration" | "annotation_type_declaration"
        );

        // Descend into the type body to collect members.
        let Some(body) = child.child_by_field_name("body") else {
            continue;
        };

        collect_members(
            &body,
            bytes,
            file,
            namespaces,
            &type_name,
            implicit_public,
            &mut out,
        );
    }

    out
}

/// Collect method, constructor, and field declarations from a type body node.
///
/// For `enum_declaration` the body is `enum_body`, which may contain an
/// `enum_body_declarations` child that wraps the methods and fields — we
/// descend into that extra level automatically.
fn collect_members(
    body: &Node,
    bytes: &[u8],
    file: &str,
    namespaces: &[String],
    type_name: &str,
    implicit_public: bool,
    out: &mut Vec<Symbol>,
) {
    for member in body.children(&mut body.walk()) {
        match member.kind() {
            // enum methods/fields live one level deeper inside enum_body_declarations.
            "enum_body_declarations" => {
                collect_members(
                    &member,
                    bytes,
                    file,
                    namespaces,
                    type_name,
                    implicit_public,
                    out,
                );
            }
            "method_declaration" | "constructor_declaration" => {
                if !implicit_public && !is_public(&member, bytes) {
                    continue;
                }
                let Some(name) = field_text(&member, "name", bytes) else {
                    continue;
                };
                let mut descriptors: Vec<Descriptor> = namespaces
                    .iter()
                    .cloned()
                    .map(Descriptor::Namespace)
                    .collect();
                descriptors.push(Descriptor::Type(type_name.to_owned()));
                descriptors.push(Descriptor::Method {
                    name: name.clone(),
                    disambiguator: String::new(),
                });
                out.push(Symbol {
                    id: SymbolId::global(Language::Java.as_str(), descriptors),
                    name,
                    kind: SymbolKind::Method,
                    file: file.to_owned(),
                    line: (member.start_position().row + 1) as u32,
                    span: ByteSpan {
                        start: member.start_byte(),
                        end: member.end_byte(),
                    },
                    signature: one_line_signature(node_text(&member, bytes), &['{', ';']),
                });
            }
            "field_declaration" => {
                if !implicit_public && !is_public(&member, bytes) {
                    continue;
                }
                // A single field_declaration may declare multiple variables.
                let mut cursor = member.walk();
                for declarator in member.children_by_field_name("declarator", &mut cursor) {
                    let Some(var_name) = field_text(&declarator, "name", bytes) else {
                        continue;
                    };
                    let mut descriptors: Vec<Descriptor> = namespaces
                        .iter()
                        .cloned()
                        .map(Descriptor::Namespace)
                        .collect();
                    descriptors.push(Descriptor::Type(type_name.to_owned()));
                    descriptors.push(Descriptor::Term(var_name.clone()));
                    out.push(Symbol {
                        id: SymbolId::global(Language::Java.as_str(), descriptors),
                        name: var_name,
                        kind: SymbolKind::Static,
                        file: file.to_owned(),
                        line: (member.start_position().row + 1) as u32,
                        span: ByteSpan {
                            start: member.start_byte(),
                            end: member.end_byte(),
                        },
                        signature: one_line_signature(node_text(&member, bytes), &['{', ';']),
                    });
                }
            }
            _ => {}
        }
    }
}

/// True iff `node` has a `modifiers` child that contains the text `"public"`.
///
/// If there is no `modifiers` child (package-private), returns `false`.
fn is_public(node: &Node, bytes: &[u8]) -> bool {
    for child in node.children(&mut node.walk()) {
        if child.kind() != "modifiers" {
            continue;
        }
        for modifier in child.children(&mut child.walk()) {
            if node_text(&modifier, bytes) == "public" {
                return true;
            }
        }
        return false;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_public_class_and_method() {
        let src = r#"package com.example.auth;
public class SessionManager {
    public boolean validate(String token) { return true; }
    private void secret() {}
    int packagePrivate;
}
class Helper {}
"#;
        let facts = JavaExtractor
            .extract(src, "src/com/example/auth/SessionManager.java")
            .unwrap();

        let by_name = |n: &str| facts.symbols.iter().find(|s| s.name == n).cloned();

        let sm = by_name("SessionManager").unwrap();
        assert_eq!(sm.kind, SymbolKind::Class);
        assert_eq!(
            sm.id.to_scip_string(),
            "codegraph    com/example/auth/SessionManager#"
        );

        let validate = by_name("validate").unwrap();
        assert_eq!(validate.kind, SymbolKind::Method);
        assert_eq!(
            validate.id.to_scip_string(),
            "codegraph    com/example/auth/SessionManager#validate()."
        );

        // private method — must not appear
        assert!(by_name("secret").is_none());
        // package-private field — must not appear
        assert!(by_name("packagePrivate").is_none());
        // non-public type — must not appear
        assert!(by_name("Helper").is_none());

        assert_eq!(facts.lang, "java");
    }

    #[test]
    fn interface_members_are_public() {
        let src = r#"package io.svc;
public interface Reader {
    int read();
    void close();
}
"#;
        let facts = JavaExtractor
            .extract(src, "src/io/svc/Reader.java")
            .unwrap();

        let by_name = |n: &str| facts.symbols.iter().find(|s| s.name == n).cloned();

        let reader = by_name("Reader").unwrap();
        assert_eq!(reader.kind, SymbolKind::Interface);
        assert_eq!(reader.id.to_scip_string(), "codegraph    io/svc/Reader#");

        // Both methods must be emitted even though they carry no `public` modifier.
        let read = by_name("read").unwrap();
        assert_eq!(read.kind, SymbolKind::Method);
        assert_eq!(
            read.id.to_scip_string(),
            "codegraph    io/svc/Reader#read()."
        );

        let close = by_name("close").unwrap();
        assert_eq!(close.kind, SymbolKind::Method);
        assert_eq!(
            close.id.to_scip_string(),
            "codegraph    io/svc/Reader#close()."
        );
    }

    #[test]
    fn extracts_call_references() {
        let src = r#"package com.example;
public class Client {
    public void run() {
        validate("t");
        new Server();
    }
}
"#;
        let facts = JavaExtractor
            .extract(src, "src/com/example/Client.java")
            .unwrap();

        let names: Vec<&str> = facts.references.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"validate"),
            "expected 'validate' in {names:?}"
        );
        assert!(names.contains(&"Server"), "expected 'Server' in {names:?}");
    }
}
