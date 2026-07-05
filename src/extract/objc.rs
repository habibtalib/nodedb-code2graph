// SPDX-License-Identifier: Apache-2.0

//! Objective-C extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: `@interface` / `@implementation` / `@protocol` / categories as
//! class-kind symbols (`@protocol` → [`SymbolKind::Interface`]); `+`/`-` methods
//! with multi-part selector names in selector form (`compute:with:`);
//! `@property` declarations; and the plain-C subset (`function_definition`,
//! top-level declarations, typedefs, aggregates, preprocessor macros) handled
//! exactly like `c.rs`. References: message sends `[recv sel:arg]` as Calls
//! with the receiver captured as the reference's qualifier (`self`/`super`
//! receivers are qualifier-absent, the codebase-wide `self.method()`
//! convention), C-style `call_expression` callees, `#import`/`@import` as
//! Imports, and superclass `: Base` / `<Proto>` conformance as
//! IsImplementation.
//!
//! Honest ceilings (documented, never guessed past):
//! - **`.h` stays mapped to C (locked decision D-01).** This extractor claims
//!   `.m` and `.mm` ONLY — no content sniffing, no dual dispatch. Objective-C
//!   declarations living in headers are extracted as C facts; that is a
//!   documented gap, mirrored in `docs/supported-languages.md`.
//! - **Visibility:** Objective-C has no enforced access control — every method
//!   is dynamically dispatchable — so ObjC-level symbols are tagged
//!   [`Visibility::Public`]. C-level definitions keep C's linkage rule
//!   (`static` → [`Visibility::Private`]).
//! - **Dynamic dispatch** (`performSelector:`, `NSSelectorFromString`, …) is
//!   unresolved by design.
//! - **Categories** are emitted as distinct class-kind symbols named
//!   `Base+Category`, not merged into the base type.
//! - **Class extensions** (`@interface Foo ()`) parse as a category-less
//!   `class_interface` and share the base type's identity; when the same file
//!   also declares/implements the type, the first occurrence in document order
//!   wins (duplicate SCIP ids are dropped).
//! - Instance-variable blocks (`{ int _count; }`) are not emitted.
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.

use std::collections::HashSet;

use tree_sitter::{Node, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{
    ByteSpan, EntryPoint, FileFacts, RefRole, Reference, Scope, ScopeId, ScopeKind, Symbol,
    SymbolKind, Visibility,
};
use crate::lang::Language;
use crate::symbol::Descriptor;

use super::{
    ExtractCtx, Extractor, MIN_REF_LEN, attach_reference_scopes, collect_call_references,
    definition_bindings, field_text, import_bindings, is_static, make_symbol, node_occurrence,
    node_span, node_text, one_line_signature, push_import_ref, push_ref, push_scope,
    simple_type_name,
};

/// Tree-sitter query capturing plain-C call-callee identifiers (message sends
/// are collected by a manual walk — selector reassembly can't be a query).
const CALL_QUERY: &str = r#"
(call_expression
  function: (identifier) @callee
)
"#;

/// Extracts Objective-C symbols and references.
pub struct ObjCExtractor;

impl Extractor for ObjCExtractor {
    fn lang(&self) -> Language {
        Language::ObjC
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_language = crate::grammar::objc();
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
        let namespaces = objc_namespaces(file);

        let defs = collect_symbols(&root, bytes, file, &namespaces);
        let def_bindings = definition_bindings(&defs);
        let mut symbols = defs;
        let mod_sym = super::module_symbol(Language::ObjC, &namespaces, file, source.len());
        let module_id = mod_sym.id.to_scip_string();
        symbols.push(mod_sym);

        let mut references =
            collect_call_references(&root, &ts_language, CALL_QUERY, Language::ObjC, bytes, file)?;
        collect_message_sends(&root, bytes, file, &mut references);
        collect_inheritance(&root, bytes, file, &mut references);
        collect_imports(&root, bytes, file, &module_id, &mut references);

        let scopes = collect_scopes(&root, source.len());
        attach_reference_scopes(&mut references, &scopes);
        let mut bindings = def_bindings;
        bindings.extend(import_bindings(&references, &scopes));

        Ok(FileFacts {
            file: file.to_owned(),
            lang: Language::ObjC.as_str().to_owned(),
            symbols,
            references,
            scopes,
            bindings,
            ffi_exports: Vec::new(),
        })
    }
}

/// Derive the Objective-C namespace path from a file path.
///
/// Strips the `.m` or `.mm` extension, strips a leading `src/` prefix, then
/// splits on `/`. The file stem is kept as the last namespace segment (same
/// convention as `c.rs`, so a paired `Session.h` — dispatched to C per D-01 —
/// shares the `Session` stem namespace).
fn objc_namespaces(file: &str) -> Vec<String> {
    let p = file
        .strip_suffix(".m")
        .or_else(|| file.strip_suffix(".mm"))
        .unwrap_or(file);
    let p = p.strip_prefix("src/").unwrap_or(p);
    p.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Reassemble a selector from a `method_declaration` / `method_definition`
/// node: its selector pieces are the **direct** `identifier` children (the
/// parameter names live nested inside `method_parameter` nodes and the return
/// type inside `method_type`, so neither pollutes the walk). A selector with
/// parameters gets one `:` per part (`compute:with:`); a unary selector stays
/// bare (`run`). Verified against tree-sitter-objc 3.0.2 (03-RESEARCH.md).
fn declaration_selector(node: &Node, bytes: &[u8]) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    let mut has_params = false;
    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "identifier" => parts.push(node_text(&child, bytes)),
            "method_parameter" => has_params = true,
            _ => {}
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(join_selector(&parts, has_params))
}

/// Join selector pieces into selector form: `["compute", "with"]` with
/// parameters → `"compute:with:"`; `["run"]` without → `"run"`.
fn join_selector(parts: &[&str], has_params: bool) -> String {
    let mut name = parts.join(":");
    if has_params {
        name.push(':');
    }
    name
}

/// Walk a C declarator subtree to the inner name identifier; returns
/// `(name, is_function)`. Same chain logic as `c.rs::declarator_name` (the
/// grammar shares C's declarator shapes); duplicated here because that helper
/// is private to the C module.
fn declarator_name(node: &Node, bytes: &[u8]) -> Option<(String, bool)> {
    match node.kind() {
        "identifier" | "type_identifier" | "field_identifier" => {
            Some((node_text(node, bytes).to_owned(), false))
        }
        "function_declarator" => {
            let inner = node.child_by_field_name("declarator")?;
            let (name, _) = declarator_name(&inner, bytes)?;
            Some((name, true))
        }
        _ => {
            if let Some(d) = node.child_by_field_name("declarator") {
                return declarator_name(&d, bytes);
            }
            for c in node.children(&mut node.walk()) {
                if let Some(r) = declarator_name(&c, bytes) {
                    return Some(r);
                }
            }
            None
        }
    }
}

/// If `spec` is a `struct_specifier`, `union_specifier`, or `enum_specifier`
/// with both a `name` field and a `body` (a definition, not a forward
/// reference), return `(SymbolKind, name)`. Unions map to Struct (no Union
/// variant), same as `c.rs`.
fn aggregate_type_symbol(spec: &Node, bytes: &[u8]) -> Option<(SymbolKind, String)> {
    let kind = match spec.kind() {
        "struct_specifier" | "union_specifier" => SymbolKind::Struct,
        "enum_specifier" => SymbolKind::Enum,
        _ => return None,
    };
    spec.child_by_field_name("body")?;
    let name = field_text(spec, "name", bytes)?;
    Some((kind, name))
}

// ── Definitions ──────────────────────────────────────────────────────────────

fn collect_symbols(root: &Node, bytes: &[u8], file: &str, namespaces: &[String]) -> Vec<Symbol> {
    let mut out = Vec::new();
    // ObjC allows the same type identity to appear more than once per file
    // (@interface + @implementation, class extensions): first occurrence in
    // document order wins, duplicates are dropped by SCIP id.
    let mut seen: HashSet<String> = HashSet::new();
    let ctx = ExtractCtx {
        bytes,
        file,
        lang: Language::ObjC,
    };

    for child in root.children(&mut root.walk()) {
        match child.kind() {
            "class_interface" | "class_implementation" => {
                collect_class(&child, &ctx, namespaces, &mut seen, &mut out);
            }
            "protocol_declaration" => {
                collect_protocol(&child, &ctx, namespaces, &mut seen, &mut out);
            }
            _ => collect_c_level(&child, &ctx, namespaces, &mut out),
        }
    }
    out
}

/// Build the namespace descriptor prefix plus `leaf`, make the symbol, and push
/// it unless its SCIP id was already emitted (`seen` dedupe).
#[allow(clippy::too_many_arguments)]
fn push_symbol(
    out: &mut Vec<Symbol>,
    seen: &mut HashSet<String>,
    ctx: &ExtractCtx,
    namespaces: &[String],
    node: &Node,
    name: String,
    kind: SymbolKind,
    visibility: Visibility,
    descriptors: Vec<Descriptor>,
) {
    let mut full: Vec<Descriptor> = namespaces
        .iter()
        .cloned()
        .map(Descriptor::Namespace)
        .collect();
    full.extend(descriptors);
    let signature = one_line_signature(node_text(node, ctx.bytes), &['{', ';', '\n']);
    let sym = make_symbol(ctx, node, name, kind, visibility, full, signature);
    if seen.insert(sym.id.to_scip_string()) {
        out.push(sym);
    }
}

/// Emit a class-kind symbol for a `class_interface` / `class_implementation`
/// (categories become `Base+Category`), plus Method symbols for its selector
/// declarations/definitions and property symbols for `@property` members.
fn collect_class(
    node: &Node,
    ctx: &ExtractCtx,
    namespaces: &[String],
    seen: &mut HashSet<String>,
    out: &mut Vec<Symbol>,
) {
    // The class name is the FIRST `identifier` child (it precedes the
    // `superclass:` and `category:` fields, which are also `identifier`s).
    let Some(base_node) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")
    else {
        return;
    };
    let base = node_text(&base_node, ctx.bytes).to_owned();
    let type_name = match field_text(node, "category", ctx.bytes) {
        Some(cat) => format!("{base}+{cat}"),
        None => base,
    };

    push_symbol(
        out,
        seen,
        ctx,
        namespaces,
        node,
        type_name.clone(),
        SymbolKind::Class,
        Visibility::Public,
        vec![Descriptor::Type(type_name.clone())],
    );

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            // @interface members are direct method_declaration children.
            "method_declaration" => {
                collect_method(&child, &type_name, ctx, namespaces, seen, out);
            }
            // @implementation members are wrapped: implementation_definition
            // → method_definition.
            "implementation_definition" => {
                for def in child.children(&mut child.walk()) {
                    if def.kind() == "method_definition" {
                        collect_method(&def, &type_name, ctx, namespaces, seen, out);
                    }
                }
            }
            "property_declaration" => {
                collect_property(&child, &type_name, ctx, namespaces, seen, out);
            }
            _ => {}
        }
    }
}

/// Emit an Interface symbol for a `@protocol` plus Method symbols for its
/// requirements. `@optional` methods are wrapped in
/// `qualified_protocol_interface_declaration`; both required and optional emit
/// the same Method fact (the distinction is not modeled in v1).
fn collect_protocol(
    node: &Node,
    ctx: &ExtractCtx,
    namespaces: &[String],
    seen: &mut HashSet<String>,
    out: &mut Vec<Symbol>,
) {
    let Some(name_node) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")
    else {
        return;
    };
    let proto_name = node_text(&name_node, ctx.bytes).to_owned();

    push_symbol(
        out,
        seen,
        ctx,
        namespaces,
        node,
        proto_name.clone(),
        SymbolKind::Interface,
        Visibility::Public,
        vec![Descriptor::Type(proto_name.clone())],
    );

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "method_declaration" => {
                collect_method(&child, &proto_name, ctx, namespaces, seen, out);
            }
            "qualified_protocol_interface_declaration" => {
                for decl in child.children(&mut child.walk()) {
                    if decl.kind() == "method_declaration" {
                        collect_method(&decl, &proto_name, ctx, namespaces, seen, out);
                    }
                }
            }
            "property_declaration" => {
                collect_property(&child, &proto_name, ctx, namespaces, seen, out);
            }
            _ => {}
        }
    }
}

/// Emit a Method symbol (selector-form name) nested under `Type(type_name)`.
fn collect_method(
    node: &Node,
    type_name: &str,
    ctx: &ExtractCtx,
    namespaces: &[String],
    seen: &mut HashSet<String>,
    out: &mut Vec<Symbol>,
) {
    let Some(selector) = declaration_selector(node, ctx.bytes) else {
        return;
    };
    push_symbol(
        out,
        seen,
        ctx,
        namespaces,
        node,
        selector.clone(),
        SymbolKind::Method,
        Visibility::Public,
        vec![
            Descriptor::Type(type_name.to_owned()),
            Descriptor::Method {
                name: selector,
                disambiguator: String::new(),
            },
        ],
    );
}

/// Emit a property symbol nested under `Type(type_name)`. The property name is
/// the innermost declarator identifier of the `struct_declaration` child
/// (`@property NSString *token;` → `token`), the same declarator-chain shape
/// as C.
fn collect_property(
    node: &Node,
    type_name: &str,
    ctx: &ExtractCtx,
    namespaces: &[String],
    seen: &mut HashSet<String>,
    out: &mut Vec<Symbol>,
) {
    let Some(decl) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "struct_declaration")
    else {
        return;
    };
    let Some(declarator) = decl
        .children(&mut decl.walk())
        .find(|c| c.kind() == "struct_declarator")
    else {
        return;
    };
    let Some((name, _)) = declarator_name(&declarator, ctx.bytes) else {
        return;
    };
    push_symbol(
        out,
        seen,
        ctx,
        namespaces,
        node,
        name.clone(),
        SymbolKind::Static,
        Visibility::Public,
        vec![
            Descriptor::Type(type_name.to_owned()),
            Descriptor::Term(name),
        ],
    );
}

/// Handle one top-level plain-C node exactly like `c.rs`: functions,
/// declarations, typedefs, macros, and bare aggregates, with `static` →
/// [`Visibility::Private`] and `main` marked as an entry point.
fn collect_c_level(child: &Node, ctx: &ExtractCtx, namespaces: &[String], out: &mut Vec<Symbol>) {
    let push = |out: &mut Vec<Symbol>,
                node: &Node,
                name: String,
                kind: SymbolKind,
                visibility: Visibility,
                leaf: Descriptor| {
        let mut descriptors: Vec<Descriptor> = namespaces
            .iter()
            .cloned()
            .map(Descriptor::Namespace)
            .collect();
        descriptors.push(leaf);
        let signature = one_line_signature(node_text(node, ctx.bytes), &['{', ';']);
        out.push(make_symbol(
            ctx,
            node,
            name,
            kind,
            visibility,
            descriptors,
            signature,
        ));
    };

    match child.kind() {
        "function_definition" => {
            let vis = if is_static(child, ctx.bytes) {
                Visibility::Private
            } else {
                Visibility::Public
            };
            let Some(decl) = child.child_by_field_name("declarator") else {
                return;
            };
            let Some((name, _)) = declarator_name(&decl, ctx.bytes) else {
                return;
            };
            let is_main = name == "main";
            push(
                out,
                child,
                name.clone(),
                SymbolKind::Function,
                vis,
                Descriptor::Method {
                    name,
                    disambiguator: String::new(),
                },
            );
            if is_main {
                if let Some(s) = out.last_mut() {
                    s.entry_points.push(EntryPoint::Main);
                }
            }
        }
        "declaration" => {
            let vis = if is_static(child, ctx.bytes) {
                Visibility::Private
            } else {
                Visibility::Public
            };
            if let Some(spec) = child.child_by_field_name("type") {
                if let Some((agg_kind, agg_name)) = aggregate_type_symbol(&spec, ctx.bytes) {
                    push(
                        out,
                        &spec,
                        agg_name.clone(),
                        agg_kind,
                        vis,
                        Descriptor::Type(agg_name),
                    );
                }
            }
            let mut cursor = child.walk();
            for decl in child.children_by_field_name("declarator", &mut cursor) {
                let Some((name, is_function)) = declarator_name(&decl, ctx.bytes) else {
                    continue;
                };
                if is_function {
                    push(
                        out,
                        child,
                        name.clone(),
                        SymbolKind::Function,
                        vis,
                        Descriptor::Method {
                            name,
                            disambiguator: String::new(),
                        },
                    );
                } else {
                    push(
                        out,
                        child,
                        name.clone(),
                        SymbolKind::Static,
                        vis,
                        Descriptor::Term(name),
                    );
                }
            }
        }
        "type_definition" => {
            if let Some(spec) = child.child_by_field_name("type") {
                if let Some((agg_kind, agg_name)) = aggregate_type_symbol(&spec, ctx.bytes) {
                    push(
                        out,
                        &spec,
                        agg_name.clone(),
                        agg_kind,
                        Visibility::Public,
                        Descriptor::Type(agg_name),
                    );
                }
            }
            let Some(decl) = child.child_by_field_name("declarator") else {
                return;
            };
            let Some((name, _)) = declarator_name(&decl, ctx.bytes) else {
                return;
            };
            push(
                out,
                child,
                name.clone(),
                SymbolKind::TypeAlias,
                Visibility::Public,
                Descriptor::Type(name),
            );
        }
        "preproc_def" => {
            let Some(name) = field_text(child, "name", ctx.bytes) else {
                return;
            };
            push(
                out,
                child,
                name.clone(),
                SymbolKind::Const,
                Visibility::Public,
                Descriptor::Macro(name),
            );
        }
        "preproc_function_def" => {
            let Some(name) = field_text(child, "name", ctx.bytes) else {
                return;
            };
            push(
                out,
                child,
                name.clone(),
                SymbolKind::Function,
                Visibility::Public,
                Descriptor::Macro(name),
            );
        }
        "struct_specifier" | "union_specifier" | "enum_specifier" => {
            if let Some((agg_kind, agg_name)) = aggregate_type_symbol(child, ctx.bytes) {
                push(
                    out,
                    child,
                    agg_name.clone(),
                    agg_kind,
                    Visibility::Public,
                    Descriptor::Type(agg_name),
                );
            }
        }
        _ => {}
    }
}

// ── References: message sends ────────────────────────────────────────────────

/// Recursively walk `node` and emit a [`RefRole::Call`] reference for every
/// `message_expression`, with the selector reassembled from its repeated
/// `method:` fields (`[obj compute:1 with:2]` → `compute:with:`) and the
/// `receiver:` field's source text captured as the reference's qualifier
/// (`obj`, `Session`, or a nested send like `[Session shared]`).
/// `self`/`super` receivers deliberately carry NO qualifier — the codebase-wide
/// convention (see `resolve/conformance.rs`) is that `self.method()`-shaped
/// calls are qualifier-absent, letting the scope-aware resolver's lexical walk
/// handle them instead of a doomed path lookup on the literal text `self`.
/// The occurrence anchors at the first selector piece. Applies [`MIN_REF_LEN`].
/// Recursion continues into the receiver, so nested sends each emit a Call.
fn collect_message_sends(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    if node.kind() == "message_expression" {
        let mut cursor = node.walk();
        let parts: Vec<Node> = node.children_by_field_name("method", &mut cursor).collect();
        if let Some(first) = parts.first() {
            // A selector with arguments has `:` tokens as direct unnamed
            // children of the message_expression (verified: AST dump).
            let has_args = node.children(&mut node.walk()).any(|c| c.kind() == ":");
            let texts: Vec<&str> = parts.iter().map(|p| node_text(p, bytes)).collect();
            let name = join_selector(&texts, has_args);
            if name.len() >= MIN_REF_LEN {
                let qualifier = node
                    .child_by_field_name("receiver")
                    .map(|r| node_text(&r, bytes).to_owned())
                    .filter(|q| q != "self" && q != "super");
                out.push(Reference {
                    name,
                    occ: node_occurrence(first, file),
                    role: RefRole::Call,
                    source_module: None,
                    from_path: None,
                    qualifier,
                    scope: None,
                    type_ref_ctx: None,
                });
            }
        }
    }
    for child in node.children(&mut node.walk()) {
        collect_message_sends(&child, bytes, file, out);
    }
}

// ── References: inheritance / conformance ────────────────────────────────────

/// Recursively walk `node` and emit [`RefRole::IsImplementation`] references
/// for a class's `superclass:` field and its protocol-conformance list.
///
/// On `class_interface` / `class_implementation` the conformance list is a
/// direct `parameterized_arguments` child holding `type_name → type_identifier`
/// entries; on `protocol_declaration` it is a `protocol_reference_list` holding
/// bare `identifier`s (verified: AST dump). Only DIRECT children of the class
/// node are scanned, so generic type arguments nested in method signatures
/// never leak in.
fn collect_inheritance(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    match node.kind() {
        "class_interface" | "class_implementation" => {
            if let Some(sup) = node.child_by_field_name("superclass") {
                push_ref(
                    out,
                    node_text(&sup, bytes),
                    &sup,
                    file,
                    RefRole::IsImplementation,
                );
            }
            for child in node.children(&mut node.walk()) {
                if child.kind() != "parameterized_arguments" {
                    continue;
                }
                for tn in child.children(&mut child.walk()) {
                    if tn.kind() != "type_name" {
                        continue;
                    }
                    if let Some(id) = tn
                        .children(&mut tn.walk())
                        .find(|c| c.kind() == "type_identifier")
                    {
                        push_ref(
                            out,
                            node_text(&id, bytes),
                            &id,
                            file,
                            RefRole::IsImplementation,
                        );
                    }
                }
            }
        }
        "protocol_declaration" => {
            for child in node.children(&mut node.walk()) {
                if child.kind() != "protocol_reference_list" {
                    continue;
                }
                for id in child.children(&mut child.walk()) {
                    if id.kind() == "identifier" {
                        push_ref(
                            out,
                            node_text(&id, bytes),
                            &id,
                            file,
                            RefRole::IsImplementation,
                        );
                    }
                }
            }
        }
        _ => {}
    }
    for child in node.children(&mut node.walk()) {
        collect_inheritance(&child, bytes, file, out);
    }
}

// ── References: imports ──────────────────────────────────────────────────────

/// Recursively walk `node` and emit [`RefRole::Import`] references for all
/// three Objective-C import forms (verified node kinds):
/// - `#import "Session.h"` → `preproc_include` with a `string_literal` path;
/// - `#import <Foundation/Foundation.h>` → `preproc_include` with a
///   `system_lib_string` path;
/// - `@import CoreData;` → `module_import` with an `identifier` path.
///
/// The reference name is the leaf stem (`Session.h` → `Session`,
/// `Foundation/Foundation.h` → `Foundation`) so it matches the target file's
/// module-symbol name; `from_path` carries the path as written (quotes/angle
/// brackets stripped).
fn collect_imports(
    node: &Node,
    bytes: &[u8],
    file: &str,
    module_id: &str,
    out: &mut Vec<Reference>,
) {
    match node.kind() {
        "preproc_include" => {
            if let Some(path) = node.child_by_field_name("path") {
                let raw = node_text(&path, bytes);
                let trimmed = raw.trim_matches(|c| c == '"' || c == '<' || c == '>');
                let leaf = trimmed.rsplit('/').next().unwrap_or(trimmed);
                let name = leaf.split('.').next().unwrap_or(leaf);
                push_import_ref(out, name, &path, file, module_id, trimmed);
            }
        }
        "module_import" => {
            if let Some(path) = node.child_by_field_name("path") {
                let raw = node_text(&path, bytes);
                let name = simple_type_name(raw, ".");
                push_import_ref(out, name, &path, file, module_id, raw);
            }
        }
        _ => {}
    }
    for child in node.children(&mut node.walk()) {
        collect_imports(&child, bytes, file, module_id, out);
    }
}

// ── Scope tree (Tier-B) ──────────────────────────────────────────────────────

/// Build the lexical scope tree for one Objective-C file.
///
/// `scopes[0]` is always the file-root `Module` scope spanning `[0, source_len)`.
/// `function_definition` (C) and `method_definition` (ObjC) open a `Function`
/// scope with their body peeled; bare `compound_statement`s open a `Block`
/// scope (if/for/while bodies), same shape as `c.rs`.
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

fn scope_dfs(node: &Node, parent_id: ScopeId, scopes: &mut Vec<Scope>) {
    match node.kind() {
        "function_definition" => {
            let fn_id = push_scope(
                scopes,
                Some(parent_id),
                node_span(node),
                ScopeKind::Function,
            );
            // Peel the body compound_statement (a named `body:` field).
            if let Some(body) = node.child_by_field_name("body") {
                for child in body.children(&mut body.walk()) {
                    scope_dfs(&child, fn_id, scopes);
                }
            }
        }
        "method_definition" => {
            let fn_id = push_scope(
                scopes,
                Some(parent_id),
                node_span(node),
                ScopeKind::Function,
            );
            // The method body is a direct compound_statement child (no field).
            for child in node.children(&mut node.walk()) {
                if child.kind() == "compound_statement" {
                    for body_child in child.children(&mut child.walk()) {
                        scope_dfs(&body_child, fn_id, scopes);
                    }
                }
            }
        }
        "compound_statement" => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::RefRole;

    fn extract(src: &str, path: &str) -> FileFacts {
        ObjCExtractor.extract(src, path).unwrap()
    }

    fn by_name(facts: &FileFacts, name: &str) -> Option<Symbol> {
        facts.symbols.iter().find(|s| s.name == name).cloned()
    }

    #[test]
    fn dispatch_claims_m_and_mm_only() {
        // Locked decision D-01: `.m`/`.mm` are ObjC; bare `.h` STAYS C.
        assert_eq!(
            Language::from_path("src/app/Session.m"),
            Some(Language::ObjC)
        );
        assert_eq!(
            Language::from_path("src/app/Bridge.mm"),
            Some(Language::ObjC)
        );
        assert_eq!(Language::from_path("src/app/Session.h"), Some(Language::C));
    }

    #[test]
    fn interface_class_methods_property_and_conformance() {
        let src = r#"
@interface Session : NSObject <Codable>
@property (nonatomic, strong) NSString *token;
- (void)run;
- (int)compute:(int)x with:(int)y;
+ (instancetype)shared;
@end
"#;
        let facts = extract(src, "src/app/Session.m");

        let session = by_name(&facts, "Session").unwrap();
        assert_eq!(session.kind, SymbolKind::Class);
        assert_eq!(session.visibility, Visibility::Public);
        assert_eq!(
            session.id.to_scip_string(),
            "codegraph . . . app/Session/Session#"
        );

        let token = by_name(&facts, "token").unwrap();
        assert_eq!(token.kind, SymbolKind::Static);
        assert_eq!(
            token.id.to_scip_string(),
            "codegraph . . . app/Session/Session#token."
        );

        let run = by_name(&facts, "run").unwrap();
        assert_eq!(run.kind, SymbolKind::Method);
        assert_eq!(run.visibility, Visibility::Public);
        assert_eq!(
            run.id.to_scip_string(),
            "codegraph . . . app/Session/Session#run()."
        );

        // Multi-part selector: joined with ':' and trailing ':'; the SCIP id
        // backtick-escapes the non-simple identifier.
        let compute = by_name(&facts, "compute:with:").unwrap();
        assert_eq!(compute.kind, SymbolKind::Method);
        assert_eq!(
            compute.id.to_scip_string(),
            "codegraph . . . app/Session/Session#`compute:with:`()."
        );

        // Class method (+) — same Method fact; the '+' marker survives in the
        // signature.
        let shared = by_name(&facts, "shared").unwrap();
        assert_eq!(shared.kind, SymbolKind::Method);
        assert!(
            shared.signature.starts_with('+'),
            "got {:?}",
            shared.signature
        );

        // Superclass and protocol conformance → IsImplementation refs.
        let impls: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::IsImplementation)
            .map(|r| r.name.as_str())
            .collect();
        assert!(impls.contains(&"NSObject"), "got {impls:?}");
        assert!(impls.contains(&"Codable"), "got {impls:?}");

        assert_eq!(facts.lang, "objc");
    }

    #[test]
    fn implementation_dedupes_against_interface() {
        let src = r#"
@interface Session
- (void)run;
@end
@implementation Session
- (void)run { }
- (void)helperMethod { }
@end
"#;
        let facts = extract(src, "src/app/Session.m");

        // Exactly one Class symbol for Session (interface first wins).
        let classes: Vec<_> = facts
            .symbols
            .iter()
            .filter(|s| s.name == "Session" && s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1, "duplicate class symbols: {classes:?}");
        assert_eq!(classes[0].line, 2, "interface (line 2) must win");

        // `run` emitted once (from the @interface declaration).
        let runs: Vec<_> = facts.symbols.iter().filter(|s| s.name == "run").collect();
        assert_eq!(runs.len(), 1, "duplicate method symbols: {runs:?}");

        // `helperMethod` only exists in the @implementation — still emitted.
        let helper = by_name(&facts, "helperMethod").unwrap();
        assert_eq!(helper.kind, SymbolKind::Method);
        assert_eq!(
            helper.id.to_scip_string(),
            "codegraph . . . app/Session/Session#helperMethod()."
        );
    }

    #[test]
    fn category_is_distinct_class_symbol() {
        let src = r#"
@interface Session (Networking)
- (void)fetch;
@end
"#;
        let facts = extract(src, "src/app/Session+Networking.m");

        let cat = by_name(&facts, "Session+Networking").unwrap();
        assert_eq!(cat.kind, SymbolKind::Class);
        assert_eq!(
            cat.id.to_scip_string(),
            "codegraph . . . app/Session+Networking/Session+Networking#"
        );

        let fetch = by_name(&facts, "fetch").unwrap();
        assert_eq!(
            fetch.id.to_scip_string(),
            "codegraph . . . app/Session+Networking/Session+Networking#fetch()."
        );
    }

    #[test]
    fn protocol_with_optional_methods() {
        let src = r#"
@protocol Loader <NSObject>
- (void)load:(NSString *)path;
@optional
- (void)cancel;
@end
"#;
        let facts = extract(src, "src/app/Loader.m");

        let proto = by_name(&facts, "Loader").unwrap();
        assert_eq!(proto.kind, SymbolKind::Interface);
        assert_eq!(
            proto.id.to_scip_string(),
            "codegraph . . . app/Loader/Loader#"
        );

        let load = by_name(&facts, "load:").unwrap();
        assert_eq!(load.kind, SymbolKind::Method);
        assert_eq!(
            load.id.to_scip_string(),
            "codegraph . . . app/Loader/Loader#`load:`()."
        );

        // @optional methods still emit a Method symbol.
        let cancel = by_name(&facts, "cancel").unwrap();
        assert_eq!(cancel.kind, SymbolKind::Method);

        // Protocol inheritance list → IsImplementation.
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::IsImplementation && r.name == "NSObject")
        );
    }

    #[test]
    fn message_sends_are_calls_with_receiver_qualifier() {
        let src = r#"
@implementation Session
- (void)go {
    [self run];
    [helper compute:1 with:2];
    [[Session shared] run];
}
@end
"#;
        let facts = extract(src, "src/app/Session.m");
        let calls: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Call)
            .collect();

        // `[self run]`: self/super receivers are deliberately qualifier-absent
        // (same convention as `self.method()` in every other extractor).
        let self_run = calls
            .iter()
            .find(|r| r.name == "run" && r.qualifier.is_none())
            .expect("expected [self run] call without qualifier");
        assert_eq!(self_run.occ.line, 4);

        let compute = calls
            .iter()
            .find(|r| r.name == "compute:with:")
            .expect("expected multi-part selector call");
        assert_eq!(compute.qualifier.as_deref(), Some("helper"));

        // Nested send: outer call qualified by the inner send's source text,
        // inner call qualified by the class receiver.
        assert!(
            calls
                .iter()
                .any(|r| r.name == "run" && r.qualifier.as_deref() == Some("[Session shared]")),
            "expected outer call of nested send, got {calls:?}"
        );
        assert!(
            calls
                .iter()
                .any(|r| r.name == "shared" && r.qualifier.as_deref() == Some("Session")),
            "expected inner call of nested send, got {calls:?}"
        );
    }

    #[test]
    fn imports_all_three_forms() {
        let src = "#import \"Session.h\"\n#import <Foundation/Foundation.h>\n@import CoreData;\n";
        let facts = extract(src, "src/app/main.m");
        let imports: Vec<(&str, Option<&str>)> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| (r.name.as_str(), r.from_path.as_deref()))
            .collect();

        assert!(
            imports.contains(&("Session", Some("Session.h"))),
            "got {imports:?}"
        );
        assert!(
            imports.contains(&("Foundation", Some("Foundation/Foundation.h"))),
            "got {imports:?}"
        );
        assert!(
            imports.contains(&("CoreData", Some("CoreData"))),
            "got {imports:?}"
        );

        // Imports also produce Import bindings at the file root.
        assert!(
            facts
                .bindings
                .iter()
                .any(|b| b.kind == crate::graph::types::BindingKind::Import
                    && b.name == "Foundation"),
            "expected an Import binding for Foundation"
        );
    }

    #[test]
    fn c_functions_handled_c_style() {
        let src = r#"
static int helper_fn(void) { return 0; }
int main(void) { return helper_fn(); }
"#;
        let facts = extract(src, "src/main.m");

        let helper = by_name(&facts, "helper_fn").unwrap();
        assert_eq!(helper.kind, SymbolKind::Function);
        assert_eq!(helper.visibility, Visibility::Private);
        assert_eq!(
            helper.id.to_scip_string(),
            "codegraph . . . main/helper_fn()."
        );

        let main = by_name(&facts, "main").unwrap();
        assert_eq!(main.visibility, Visibility::Public);
        assert!(
            main.entry_points
                .iter()
                .any(|e| matches!(e, EntryPoint::Main))
        );

        // The C-style call is captured with a non-root scope.
        let call = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "helper_fn")
            .expect("expected a Call ref for helper_fn");
        assert!(
            call.scope.is_some() && call.scope != Some(0),
            "call must be in a function scope, got {:?}",
            call.scope
        );
    }

    #[test]
    fn method_definition_opens_function_scope() {
        let src = r#"
@implementation Session
- (void)go {
    [self run];
}
@end
"#;
        let facts = extract(src, "src/app/Session.m");
        assert_eq!(facts.scopes[0].kind, ScopeKind::Module);
        let fn_scope = facts
            .scopes
            .iter()
            .position(|s| s.kind == ScopeKind::Function)
            .expect("expected a Function scope for the method body");

        let call = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "run")
            .expect("expected the [self run] call");
        assert_eq!(call.scope, Some(fn_scope));
    }

    #[test]
    fn module_symbol_and_definition_bindings() {
        let src = "@interface Foo\n@end\n";
        let facts = extract(src, "src/app/Foo.m");

        let modules: Vec<_> = facts
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module)
            .collect();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "Foo");
        assert_eq!(modules[0].id.to_scip_string(), "codegraph . . . app/Foo/");

        assert!(
            facts
                .bindings
                .iter()
                .any(|b| b.kind == crate::graph::types::BindingKind::Definition && b.name == "Foo"),
            "expected a Definition binding for the Foo class"
        );
    }

    #[test]
    fn class_extension_shares_base_identity() {
        // `@interface Foo ()` has no category field → same SCIP id as the base
        // type; first occurrence wins, the extension is deduped.
        let src = r#"
@interface Foo ()
- (void)hidden;
@end
@implementation Foo
- (void)hidden { }
@end
"#;
        let facts = extract(src, "src/app/Foo.m");
        let classes: Vec<_> = facts
            .symbols
            .iter()
            .filter(|s| s.name == "Foo" && s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1, "extension must dedupe: {classes:?}");

        let hiddens: Vec<_> = facts
            .symbols
            .iter()
            .filter(|s| s.name == "hidden")
            .collect();
        assert_eq!(hiddens.len(), 1, "method must dedupe: {hiddens:?}");
    }
}
