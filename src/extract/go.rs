// SPDX-License-Identifier: Apache-2.0

//! Go extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: top-level **exported** declarations (first character of the
//! name is uppercase). Covers `func`, methods, `type` (struct/interface/alias),
//! `const`, and `var`. Qualified identity follows the package path derived from
//! the file path (`src/auth/session.go` → namespaces `auth`,`session`).
//! References: callee identifiers of `call_expression` nodes.
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.

use tree_sitter::{Language as TsLanguage, Node, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{
    Binding, BindingKind, ByteSpan, FileFacts, RefRole, Reference, Scope, ScopeId, ScopeKind,
    Symbol, SymbolKind,
};
use crate::lang::Language;
use crate::symbol::{Descriptor, SymbolId};

use super::{
    Extractor, attach_reference_scopes, collect_call_references, definition_bindings, field_text,
    import_bindings, innermost_scope, node_span, node_text, one_line_signature, push_binding,
    push_ref, push_scope, simple_type_name,
};

/// Tree-sitter query capturing call-callee identifiers.
const CALL_QUERY: &str = r#"
(call_expression
  function: [
    (identifier) @callee
    (selector_expression field: (field_identifier) @callee)
  ]
)
"#;

/// Extracts Go symbols and references.
pub struct GoExtractor;

impl Extractor for GoExtractor {
    fn lang(&self) -> Language {
        Language::Go
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_language = TsLanguage::from(tree_sitter_go::LANGUAGE);
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
        let namespaces = go_namespaces(file);

        let defs = collect_symbols(&root, bytes, file, &namespaces);
        let def_bindings = definition_bindings(&defs);
        let mut symbols = defs;
        symbols.push(super::module_symbol(
            Language::Go,
            &namespaces,
            file,
            source.len(),
        ));
        let mut references =
            collect_call_references(&root, &ts_language, CALL_QUERY, Language::Go, bytes, file)?;
        collect_imports(&root, bytes, file, &mut references);

        let scopes = collect_scopes(&root, source.len());
        attach_reference_scopes(&mut references, &scopes);
        let mut bindings = collect_bindings(&root, bytes, &scopes);
        bindings.extend(def_bindings);
        bindings.extend(import_bindings(&references, &scopes));

        Ok(FileFacts {
            file: file.to_owned(),
            lang: Language::Go.as_str().to_owned(),
            symbols,
            references,
            scopes,
            bindings,
            ffi_exports: Vec::new(),
        })
    }
}

/// Derive the Go package path (namespace descriptors) from a file path.
///
/// Strips the `.go` extension, strips a leading `src/` prefix, then splits on
/// `/`. The file stem is kept as the last namespace segment (no `main` drop).
fn go_namespaces(file: &str) -> Vec<String> {
    let p = file.strip_suffix(".go").unwrap_or(file);
    let p = p.strip_prefix("src/").unwrap_or(p);
    p.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn collect_symbols(root: &Node, bytes: &[u8], file: &str, namespaces: &[String]) -> Vec<Symbol> {
    let mut out = Vec::new();

    let push =
        |out: &mut Vec<Symbol>, node: &Node, name: String, kind: SymbolKind, leaf: Descriptor| {
            let mut descriptors: Vec<Descriptor> = namespaces
                .iter()
                .cloned()
                .map(Descriptor::Namespace)
                .collect();
            descriptors.push(leaf);
            out.push(Symbol {
                id: SymbolId::global(Language::Go.as_str(), descriptors),
                name,
                kind,
                file: file.to_owned(),
                line: (node.start_position().row + 1) as u32,
                span: ByteSpan {
                    start: node.start_byte(),
                    end: node.end_byte(),
                },
                signature: one_line_signature(node_text(node, bytes), &['{']),
            });
        };

    for child in root.children(&mut root.walk()) {
        match child.kind() {
            "function_declaration" => {
                let Some(name) = field_text(&child, "name", bytes) else {
                    continue;
                };
                if !is_exported(&name) {
                    continue;
                }
                push(
                    &mut out,
                    &child,
                    name.clone(),
                    SymbolKind::Function,
                    Descriptor::Method {
                        name,
                        disambiguator: String::new(),
                    },
                );
            }
            "method_declaration" => {
                let Some(name) = field_text(&child, "name", bytes) else {
                    continue;
                };
                if !is_exported(&name) {
                    continue;
                }
                push(
                    &mut out,
                    &child,
                    name.clone(),
                    SymbolKind::Method,
                    Descriptor::Method {
                        name,
                        disambiguator: String::new(),
                    },
                );
            }
            "type_declaration" => {
                for spec in child.children(&mut child.walk()) {
                    let (kind, name) = match spec.kind() {
                        "type_spec" => {
                            let Some(name) = field_text(&spec, "name", bytes) else {
                                continue;
                            };
                            if !is_exported(&name) {
                                continue;
                            }
                            // Inspect the `type` field to determine the concrete kind.
                            let kind = spec.child_by_field_name("type").map_or(
                                SymbolKind::TypeAlias,
                                |t| match t.kind() {
                                    "struct_type" => SymbolKind::Struct,
                                    "interface_type" => SymbolKind::Interface,
                                    _ => SymbolKind::TypeAlias,
                                },
                            );
                            (kind, name)
                        }
                        "type_alias" => {
                            let Some(name) = field_text(&spec, "name", bytes) else {
                                continue;
                            };
                            if !is_exported(&name) {
                                continue;
                            }
                            (SymbolKind::TypeAlias, name)
                        }
                        _ => continue,
                    };
                    push(&mut out, &spec, name.clone(), kind, Descriptor::Type(name));
                }
            }
            "const_declaration" => {
                for spec in child.children(&mut child.walk()) {
                    if spec.kind() != "const_spec" {
                        continue;
                    }
                    for ident in spec.children(&mut spec.walk()) {
                        if ident.kind() != "identifier" {
                            continue;
                        }
                        let name = node_text(&ident, bytes).to_owned();
                        if !is_exported(&name) {
                            continue;
                        }
                        push(
                            &mut out,
                            &spec,
                            name.clone(),
                            SymbolKind::Const,
                            Descriptor::Term(name),
                        );
                    }
                }
            }
            "var_declaration" => {
                for spec in child.children(&mut child.walk()) {
                    if spec.kind() != "var_spec" {
                        continue;
                    }
                    for ident in spec.children(&mut spec.walk()) {
                        if ident.kind() != "identifier" {
                            continue;
                        }
                        let name = node_text(&ident, bytes).to_owned();
                        if !is_exported(&name) {
                            continue;
                        }
                        push(
                            &mut out,
                            &spec,
                            name.clone(),
                            SymbolKind::Static,
                            Descriptor::Term(name),
                        );
                    }
                }
            }
            _ => continue,
        }
    }
    out
}

/// Recursively walk the tree and emit one [`RefRole::Import`] reference per
/// `import_spec` node found anywhere in the tree.
///
/// tree-sitter-go grammar:
/// - `import_declaration` → `import_spec` (single) **or** `import_spec_list`
///   (parenthesised group) → contains `import_spec` children.
/// - `import_spec` has field `path` (`interpreted_string_literal` or
///   `raw_string_literal`). The optional field `name` (alias / `_` / `.`) is
///   intentionally ignored; the package's canonical leaf name is what we emit.
fn collect_imports(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    if node.kind() == "import_spec" {
        if let Some(path_node) = node.child_by_field_name("path") {
            let raw = node_text(&path_node, bytes);
            // Strip surrounding quote characters (double-quote or backtick).
            let dequoted = raw.trim_matches('"').trim_matches('`');
            let leaf = simple_type_name(dequoted, "/");
            push_ref(out, leaf, &path_node, file, RefRole::Import);
        }
        // import_spec has no children we need to recurse into.
        return;
    }

    for child in node.children(&mut node.walk()) {
        collect_imports(&child, bytes, file, out);
    }
}

// ── Scope tree (Tier-B) ──────────────────────────────────────────────────────

/// Build the lexical scope tree for one Go file.
///
/// `scopes[0]` is always the file-root `Module` scope spanning `[0, source_len)`.
/// Go is function-scoped: `func`/method declarations open a `Function` scope;
/// bare `block` nodes that are NOT a function body open a `Block` scope (e.g.
/// `if`, `for`, `switch` bodies).
fn collect_scopes(root: &Node, source_len: usize) -> Vec<Scope> {
    let mut scopes = Vec::new();
    push_scope(
        &mut scopes,
        None,
        ByteSpan {
            start: 0,
            end: source_len,
        },
        ScopeKind::Module,
    );
    for child in root.children(&mut root.walk()) {
        scope_dfs(&child, 0, &mut scopes);
    }
    scopes
}

/// DFS opening `Function` scopes for `function_declaration` / `method_declaration`
/// nodes, and `Block` scopes for bare `block` nodes that are not a function body.
///
/// The function body (`block`) is peeled: its children are visited under the
/// Function scope directly so the body block does not re-open a redundant Block
/// scope. Bare blocks elsewhere (if/for/switch bodies) do open a Block scope.
fn scope_dfs(node: &Node, parent_id: ScopeId, scopes: &mut Vec<Scope>) {
    match node.kind() {
        "function_declaration" | "method_declaration" | "func_literal" => {
            let fn_id = push_scope(
                scopes,
                Some(parent_id),
                node_span(node),
                ScopeKind::Function,
            );
            // Peel the body block: recurse its children under the Function scope
            // directly so the body `block` does not re-open a redundant Block scope.
            if let Some(body) = node.child_by_field_name("body") {
                for child in body.children(&mut body.walk()) {
                    scope_dfs(&child, fn_id, scopes);
                }
            }
        }
        "block" => {
            // A bare block NOT already consumed as a function body (e.g. if/for/
            // switch bodies, or a standalone `{ }` block inside a function).
            let block_id = push_scope(scopes, Some(parent_id), node_span(node), ScopeKind::Block);
            for child in node.children(&mut node.walk()) {
                scope_dfs(&child, block_id, scopes);
            }
        }
        _ => {
            for child in node.children(&mut node.walk()) {
                scope_dfs(&child, parent_id, scopes);
            }
        }
    }
}

// ── Bindings (Tier-B) ────────────────────────────────────────────────────────

/// Collect parameter and local-variable [`Binding`]s for one Go file.
///
/// Covers:
/// - `function_declaration` / `method_declaration` / `func_literal` parameters
///   and method receivers → [`BindingKind::Param`].
/// - `short_var_declaration` (`:=`) → [`BindingKind::Local`] for each LHS name.
/// - `var_spec` inside `var_declaration` → [`BindingKind::Local`] (only inside
///   a function; top-level ones are already covered by `definition_bindings`).
/// - `const_spec` inside `const_declaration` → same guard as `var_spec`.
/// - `range_clause` left-hand names → [`BindingKind::Local`].
///
/// Top-level `var`/`const` at scope 0 are skipped to avoid duplicating the
/// [`BindingKind::Definition`] bindings emitted by `definition_bindings`.
fn collect_bindings(root: &Node, bytes: &[u8], scopes: &[Scope]) -> Vec<Binding> {
    let mut out = Vec::new();
    collect_bindings_dfs(root, bytes, scopes, &mut out);
    out
}

fn collect_bindings_dfs(node: &Node, bytes: &[u8], scopes: &[Scope], out: &mut Vec<Binding>) {
    match node.kind() {
        "function_declaration" | "method_declaration" | "func_literal" => {
            // Params from the `parameters` field.
            if let Some(params) = node.child_by_field_name("parameters") {
                collect_params(&params, bytes, scopes, out);
            }
            // Receiver (method only) — also a parameter_list.
            if let Some(recv) = node.child_by_field_name("receiver") {
                collect_params(&recv, bytes, scopes, out);
            }
            // Recurse into all children to pick up body bindings.
            for child in node.children(&mut node.walk()) {
                collect_bindings_dfs(&child, bytes, scopes, out);
            }
        }
        "short_var_declaration" => {
            // LHS is the `left` field — an `expression_list`.
            if let Some(left) = node.child_by_field_name("left") {
                for ident in left.children(&mut left.walk()) {
                    if ident.kind() != "identifier" {
                        continue;
                    }
                    let name = node_text(&ident, bytes);
                    if name == "_" {
                        continue;
                    }
                    let intro = ident.start_byte();
                    // Always inside a function — no root-scope guard needed,
                    // but apply it defensively anyway.
                    if innermost_scope(intro, scopes) != Some(0) {
                        push_binding(out, name.to_owned(), intro, BindingKind::Local, scopes);
                    }
                }
            }
            for child in node.children(&mut node.walk()) {
                collect_bindings_dfs(&child, bytes, scopes, out);
            }
        }
        "var_spec" => {
            // Single pass: emit Local bindings for identifier children and recurse.
            // Skip `_` and top-level (scope 0, already a Definition binding).
            for child in node.children(&mut node.walk()) {
                if child.kind() == "identifier" {
                    let name = node_text(&child, bytes);
                    if name != "_" {
                        let intro = child.start_byte();
                        if innermost_scope(intro, scopes) != Some(0) {
                            push_binding(out, name.to_owned(), intro, BindingKind::Local, scopes);
                        }
                    }
                } else {
                    collect_bindings_dfs(&child, bytes, scopes, out);
                }
            }
        }
        "const_spec" => {
            // Same guard as var_spec — single pass.
            for child in node.children(&mut node.walk()) {
                if child.kind() == "identifier" {
                    let name = node_text(&child, bytes);
                    if name != "_" {
                        let intro = child.start_byte();
                        if innermost_scope(intro, scopes) != Some(0) {
                            push_binding(out, name.to_owned(), intro, BindingKind::Local, scopes);
                        }
                    }
                } else {
                    collect_bindings_dfs(&child, bytes, scopes, out);
                }
            }
        }
        "range_clause" => {
            // `for i, v := range xs {}` — left field is an expression_list.
            if let Some(left) = node.child_by_field_name("left") {
                for ident in left.children(&mut left.walk()) {
                    if ident.kind() != "identifier" {
                        continue;
                    }
                    let name = node_text(&ident, bytes);
                    if name == "_" {
                        continue;
                    }
                    let intro = ident.start_byte();
                    if innermost_scope(intro, scopes) != Some(0) {
                        push_binding(out, name.to_owned(), intro, BindingKind::Local, scopes);
                    }
                }
            }
            for child in node.children(&mut node.walk()) {
                collect_bindings_dfs(&child, bytes, scopes, out);
            }
        }
        _ => {
            for child in node.children(&mut node.walk()) {
                collect_bindings_dfs(&child, bytes, scopes, out);
            }
        }
    }
}

/// Emit a [`BindingKind::Param`] for each named parameter in a Go
/// `parameter_list` node (used for both `parameters` and `receiver` fields).
///
/// Handles `parameter_declaration` (one or more names + a type) and
/// `variadic_parameter_declaration` (`...type`). Skips blank `_` names.
fn collect_params(params: &Node, bytes: &[u8], scopes: &[Scope], out: &mut Vec<Binding>) {
    for child in params.named_children(&mut params.walk()) {
        match child.kind() {
            "parameter_declaration" | "variadic_parameter_declaration" => {
                for ident in child.children(&mut child.walk()) {
                    if ident.kind() != "identifier" {
                        continue;
                    }
                    let name = node_text(&ident, bytes);
                    if name == "_" {
                        continue;
                    }
                    let intro = ident.start_byte();
                    push_binding(out, name.to_owned(), intro, BindingKind::Param, scopes);
                }
            }
            _ => {}
        }
    }
}

/// True if the identifier is exported (first character is uppercase).
fn is_exported(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_exported_defs() {
        let src = r#"
package auth

func Validate(tok string) bool { return true }
type Config struct { Timeout int }
type Reader interface { Read() error }
const Max = 3
func helper() {}
"#;
        let facts = GoExtractor.extract(src, "src/auth/session.go").unwrap();
        let by_name = |n: &str| facts.symbols.iter().find(|s| s.name == n).cloned();

        let vt = by_name("Validate").unwrap();
        assert_eq!(
            vt.id.to_scip_string(),
            "codegraph . . . auth/session/Validate()."
        );
        assert_eq!(vt.kind, SymbolKind::Function);

        assert_eq!(by_name("Config").unwrap().kind, SymbolKind::Struct);
        assert_eq!(by_name("Reader").unwrap().kind, SymbolKind::Interface);
        assert_eq!(by_name("Max").unwrap().kind, SymbolKind::Const);

        // unexported — must not appear
        assert!(by_name("helper").is_none());

        assert_eq!(facts.lang, "go");
    }

    #[test]
    fn extracts_method_declaration() {
        let src = r#"
package run

type Server struct{}

func (s *Server) Start() { }
"#;
        let facts = GoExtractor.extract(src, "src/run.go").unwrap();
        let start = facts.symbols.iter().find(|s| s.name == "Start").unwrap();
        assert_eq!(start.kind, SymbolKind::Method);
        assert_eq!(start.id.to_scip_string(), "codegraph . . . run/Start().");
    }

    #[test]
    fn extracts_call_references() {
        let src = r#"
package main

func main() {
    Validate("t")
    obj.Close()
}
"#;
        let facts = GoExtractor.extract(src, "src/main.go").unwrap();
        let names: Vec<&str> = facts.references.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Validate"));
        assert!(names.contains(&"Close"));
    }

    #[test]
    fn import_single_stdlib() {
        let src = "package main\nimport \"fmt\"\n";
        let facts = GoExtractor.extract(src, "src/main.go").unwrap();
        let imports: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(imports, vec!["fmt"]);
    }

    #[test]
    fn import_deep_path_leaf() {
        let src = "package main\nimport \"github.com/x/y/pkg\"\n";
        let facts = GoExtractor.extract(src, "src/main.go").unwrap();
        let imports: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(imports, vec!["pkg"]);
    }

    #[test]
    fn import_grouped() {
        let src = "package main\nimport (\n  \"os\"\n  \"io/ioutil\"\n)\n";
        let facts = GoExtractor.extract(src, "src/main.go").unwrap();
        let mut imports: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| r.name.as_str())
            .collect();
        imports.sort_unstable();
        assert_eq!(imports, vec!["ioutil", "os"]);
    }

    #[test]
    fn import_aliased_emits_leaf_not_alias() {
        let src = "package main\nimport f \"fmt\"\n";
        let facts = GoExtractor.extract(src, "src/main.go").unwrap();
        let imports: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| r.name.as_str())
            .collect();
        // The package leaf "fmt" must be emitted; the alias "f" must not appear
        // as an Import reference.
        assert_eq!(imports, vec!["fmt"]);
        let aliases: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import && r.name == "f")
            .map(|r| r.name.as_str())
            .collect();
        assert!(
            aliases.is_empty(),
            "alias 'f' should not appear as an Import ref"
        );
    }

    // ── Tier-B scope / binding tests ─────────────────────────────────────────

    #[test]
    fn func_params_emit_param_bindings() {
        // `func f(a int, b int) {}` → two Param bindings in a Function scope.
        let src = "package p\nfunc f(a int, b int) {}\n";
        let facts = GoExtractor.extract(src, "src/p.go").unwrap();

        let fn_scope_id = facts
            .scopes
            .iter()
            .position(|s| s.kind == ScopeKind::Function)
            .expect("expected a Function scope");

        let mut param_names: Vec<(&str, ScopeId)> = facts
            .bindings
            .iter()
            .filter(|b| b.kind == BindingKind::Param)
            .map(|b| (b.name.as_str(), b.scope))
            .collect();
        param_names.sort_by_key(|(n, _)| *n);

        assert_eq!(
            param_names,
            vec![("a", fn_scope_id), ("b", fn_scope_id)],
            "expected Param bindings for a and b, got {param_names:?}"
        );
    }

    #[test]
    fn method_receiver_emits_param_binding() {
        // `func (s *Server) Start() {}` → Param binding for `s`.
        let src = "package p\ntype Server struct{}\nfunc (s *Server) Start() {}\n";
        let facts = GoExtractor.extract(src, "src/p.go").unwrap();

        let s_binding = facts
            .bindings
            .iter()
            .find(|b| b.kind == BindingKind::Param && b.name == "s")
            .expect("expected a Param binding named 's'");
        assert_eq!(
            facts.scopes[s_binding.scope].kind,
            ScopeKind::Function,
            "receiver binding 's' should be in a Function scope"
        );
    }

    #[test]
    fn short_var_decl_emits_local() {
        // `func f() { x := 1 }` → Local binding for `x`.
        let src = "package p\nfunc f() { x := 1 }\n";
        let facts = GoExtractor.extract(src, "src/p.go").unwrap();

        let x = facts
            .bindings
            .iter()
            .find(|b| b.kind == BindingKind::Local && b.name == "x")
            .expect("expected a Local binding for 'x'");
        assert_eq!(
            facts.scopes[x.scope].kind,
            ScopeKind::Function,
            "x should be in a Function scope"
        );
    }

    #[test]
    fn multi_name_short_var_emits_two_locals() {
        // `func f() { a, b := 1, 2 }` → two Local bindings.
        let src = "package p\nfunc f() { a, b := 1, 2 }\n";
        let facts = GoExtractor.extract(src, "src/p.go").unwrap();

        let locals: Vec<&str> = facts
            .bindings
            .iter()
            .filter(|b| b.kind == BindingKind::Local)
            .map(|b| b.name.as_str())
            .collect();
        assert!(
            locals.contains(&"a"),
            "expected Local for 'a', got {locals:?}"
        );
        assert!(
            locals.contains(&"b"),
            "expected Local for 'b', got {locals:?}"
        );
    }

    #[test]
    fn blank_identifier_skipped() {
        // `func f() { _, b := g() }` → Local `b` only; no binding for `_`.
        let src = "package p\nfunc g() (int, int) { return 0, 0 }\nfunc f() { _, b := g() }\n";
        let facts = GoExtractor.extract(src, "src/p.go").unwrap();

        assert!(
            !facts
                .bindings
                .iter()
                .any(|b| b.kind == BindingKind::Local && b.name == "_"),
            "blank '_' must not produce a Local binding"
        );
        assert!(
            facts
                .bindings
                .iter()
                .any(|b| b.kind == BindingKind::Local && b.name == "b"),
            "expected Local binding for 'b'"
        );
    }

    #[test]
    fn var_spec_inside_func_emits_local() {
        // `func f() { var x int }` → Local binding for `x`.
        let src = "package p\nfunc f() { var x int }\n";
        let facts = GoExtractor.extract(src, "src/p.go").unwrap();

        let x = facts
            .bindings
            .iter()
            .find(|b| b.kind == BindingKind::Local && b.name == "x")
            .expect("expected a Local binding for 'x'");
        assert_eq!(
            facts.scopes[x.scope].kind,
            ScopeKind::Function,
            "var inside func should bind in a Function scope"
        );
    }

    #[test]
    fn for_range_vars_emit_locals() {
        // `func f() { for i, v := range xs { _ = i; _ = v } }` → Locals `i`, `v`.
        let src = "package p\nfunc f() { var xs []int\nfor i, v := range xs { _ = i; _ = v } }\n";
        let facts = GoExtractor.extract(src, "src/p.go").unwrap();

        let locals: Vec<&str> = facts
            .bindings
            .iter()
            .filter(|b| b.kind == BindingKind::Local)
            .map(|b| b.name.as_str())
            .collect();
        assert!(
            locals.contains(&"i"),
            "expected Local for 'i', got {locals:?}"
        );
        assert!(
            locals.contains(&"v"),
            "expected Local for 'v', got {locals:?}"
        );
    }

    #[test]
    fn top_level_var_is_not_local_but_is_definition() {
        // `var Config int` at package level → Definition binding only, no Local.
        let src = "package p\nvar Config int\n";
        let facts = GoExtractor.extract(src, "src/p.go").unwrap();

        assert!(
            !facts
                .bindings
                .iter()
                .any(|b| b.kind == BindingKind::Local && b.name == "Config"),
            "top-level var must NOT be a Local binding"
        );
        assert!(
            facts
                .bindings
                .iter()
                .any(|b| b.kind == BindingKind::Definition && b.name == "Config"),
            "top-level var must have a Definition binding"
        );
    }

    #[test]
    fn nested_block_produces_block_scope_and_ref_attaches_to_it() {
        // A single function with a bare inner block containing a call:
        //   func f() { { helper() } }
        // Expected scopes: Module(0) → Function(1) → Block(2).
        // The `helper` call ref must attribute to the Block scope.
        let src = "package p\nfunc f() { { helper() } }\n";
        let facts = GoExtractor.extract(src, "src/p.go").unwrap();

        assert_eq!(
            facts.scopes[0].kind,
            ScopeKind::Module,
            "scopes[0] must be Module"
        );

        let fn_scope_id = facts
            .scopes
            .iter()
            .position(|s| s.kind == ScopeKind::Function)
            .expect("expected a Function scope");
        let block_scope_id = facts
            .scopes
            .iter()
            .position(|s| s.kind == ScopeKind::Block)
            .expect("expected a Block scope");

        // Block's parent must be the Function scope.
        assert_eq!(
            facts.scopes[block_scope_id].parent,
            Some(fn_scope_id),
            "Block scope parent should be the Function scope"
        );

        // `helper` call ref must be in the Block scope (innermost).
        let h_ref = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "helper")
            .expect("expected a Call ref for 'helper'");
        assert_eq!(
            h_ref.scope,
            Some(block_scope_id),
            "helper() call should be in the Block scope ({}), got {:?}",
            block_scope_id,
            h_ref.scope
        );
    }

    #[test]
    fn top_level_func_emits_definition_binding() {
        // `func Helper() {}` → Definition binding named "Helper".
        let src = "package p\nfunc Helper() {}\n";
        let facts = GoExtractor.extract(src, "src/p.go").unwrap();

        let b = facts
            .bindings
            .iter()
            .find(|b| b.kind == BindingKind::Definition && b.name == "Helper")
            .expect("expected a Definition binding for 'Helper'");
        assert_eq!(b.scope, 0, "top-level def must bind in scope 0");
    }
}
