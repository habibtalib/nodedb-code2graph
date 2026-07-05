// SPDX-License-Identifier: Apache-2.0

//! Zig extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: `fn` declarations (top-level and container members), container
//! types (`const Name = struct/enum/union { … }` — Zig has no dedicated type
//! keyword; the type is the RHS of a `variable_declaration`), top-level
//! `const`/`var` values, and `test "name"` blocks. Zig has a real public/private
//! signal: a `pub` keyword prefix → [`Visibility::Public`], otherwise
//! [`Visibility::Private`]. The `pub`/`var`/`const` keywords are **anonymous**
//! grammar tokens (not named fields), found by scanning direct children for a
//! matching `kind()` — the same pattern as `support::is_static` for C.
//!
//! References: free calls (`foo()`) and member calls (`p.magnitude()`, receiver
//! captured as the `qualifier`); `@import("std")` / `@import("./file.zig")` →
//! [`RefRole::Import`] (named by the binding identifier when bound, else the
//! path stem); type annotations (parameter / return / field / local) →
//! [`RefRole::TypeRef`]; variable reads and writes. The grammar has **no
//! `assignment_expression` node**: a bare reassignment (`count = 1;`) parses as
//! a `variable_declaration` with no `var`/`const` keyword — that keyword's
//! presence is the only signal distinguishing a new binding from a Write.
//!
//! Honest ceilings (documented, never guessed past):
//! - `comptime` constructs are capped at table stakes — declarations inside a
//!   `comptime` block are extracted like any other block's; nothing is ever
//!   evaluated (so comptime-generated symbols/types are invisible).
//! - `usingnamespace` re-exports are unresolved: the `@import` inside one still
//!   emits an Import reference, but the names it splices into the container are
//!   not modeled.
//! - Writes cover bare-identifier targets only (`x = …`); field/index targets
//!   (`p.x = …`) contribute Read references for their object instead.
//! - Type references cover plain and field-qualified identifiers; wrapped types
//!   (`?T`, `*T`, `[]T`, comptime-generic `T(u8)`) are not unwrapped.
//! - Zig has no inheritance concept — no `IsImplementation` references (like Go).
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.

use tree_sitter::{Node, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{
    Binding, BindingKind, ByteSpan, FileFacts, RefRole, Reference, Scope, ScopeId, ScopeKind,
    Symbol, SymbolKind, TypeRefContext, Visibility,
};
use crate::lang::Language;
use crate::symbol::Descriptor;

use super::{
    ExtractCtx, Extractor, MIN_REF_LEN, attach_reference_scopes, collect_call_references,
    definition_bindings, import_bindings, innermost_scope, make_symbol, node_span, node_text,
    one_line_signature, push_binding, push_import_ref, push_ref, push_scope, push_type_ref,
};

/// Free calls (`foo()`) and member calls (`p.magnitude()` — the receiver
/// expression is captured as `@qualifier`). `@import(...)` and other builtins
/// are a distinct `builtin_function` node kind, so they never match here.
const CALL_QUERY: &str = r#"
[
  (call_expression function: (identifier) @callee)
  (call_expression function: (field_expression object: (_) @qualifier member: (identifier) @callee))
]
"#;

/// Extracts Zig symbols and references.
pub struct ZigExtractor;

impl Extractor for ZigExtractor {
    fn lang(&self) -> Language {
        Language::Zig
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_language = crate::grammar::zig();
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
        let namespaces = zig_namespaces(file);
        let ctx = ExtractCtx {
            bytes,
            file,
            lang: Language::Zig,
        };

        let defs = collect_symbols(&root, &ctx, &namespaces);
        let def_bindings = definition_bindings(&defs);
        let mut symbols = defs;
        let mod_sym = super::module_symbol(Language::Zig, &namespaces, file, source.len());
        let module_id = mod_sym.id.to_scip_string();
        symbols.push(mod_sym);

        let mut references =
            collect_call_references(&root, &ts_language, CALL_QUERY, Language::Zig, bytes, file)?;
        collect_imports(&root, ctx.bytes, ctx.file, &module_id, &mut references);
        collect_type_references(&root, ctx.bytes, ctx.file, &mut references);
        collect_read_write_references(&root, ctx.bytes, ctx.file, &mut references);

        let scopes = collect_scopes(&root, source.len());
        attach_reference_scopes(&mut references, &scopes);

        let mut bindings = def_bindings;
        collect_local_bindings(&root, ctx.bytes, &scopes, &mut bindings);
        bindings.extend(import_bindings(&references, &scopes));

        Ok(FileFacts {
            file: file.to_owned(),
            lang: Language::Zig.as_str().to_owned(),
            symbols,
            references,
            scopes,
            bindings,
            ffi_exports: Vec::new(),
        })
    }
}

/// Derive the Zig namespace path from a file path.
///
/// Strips the `.zig` extension, strips a leading `src/` prefix, then splits on
/// `/`. The file stem is kept as the last namespace segment (C convention —
/// Zig's `@import("./file.zig")` module identity is file-path based).
fn zig_namespaces(file: &str) -> Vec<String> {
    let p = file.strip_suffix(".zig").unwrap_or(file);
    let p = p.strip_prefix("src/").unwrap_or(p);
    p.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether `node` has a `pub` keyword among its direct children. `pub` is an
/// **anonymous** token in tree-sitter-zig (verified against the pinned crate) —
/// it never appears in `to_sexp()` and has no named field; scan `children()`
/// for a child whose kind is the literal token text.
fn is_pub(node: &Node) -> bool {
    node.children(&mut node.walk()).any(|c| c.kind() == "pub")
}

/// The `var` / `const` keyword of a `variable_declaration`, if present.
///
/// Load-bearing (verified): the grammar has NO `assignment_expression` node —
/// `var x = 1;`, `const x = 1;` AND a bare reassignment `x = 2;` all parse as
/// `variable_declaration`. The keyword's presence is the only signal that the
/// node introduces a new binding; its absence means a plain reassignment.
fn decl_keyword(node: &Node) -> Option<&'static str> {
    node.children(&mut node.walk())
        .find_map(|c| match c.kind() {
            "var" => Some("var"),
            "const" => Some("const"),
            _ => None,
        })
}

/// The value (RHS) node of a `variable_declaration`: the last named child that
/// is neither the leading name identifier nor the `type:` annotation.
fn decl_value<'tree>(node: &Node<'tree>) -> Option<Node<'tree>> {
    let type_child = node.child_by_field_name("type");
    let mut cursor = node.walk();
    let mut named = node.named_children(&mut cursor);
    let name = named.next()?; // leading LHS identifier
    named
        .filter(|c| Some(*c) != type_child && *c != name)
        .last()
}

/// The leading (LHS) name identifier of a `variable_declaration`, if it is a
/// bare identifier.
fn decl_name<'tree>(node: &Node<'tree>) -> Option<Node<'tree>> {
    let first = node.named_child(0)?;
    (first.kind() == "identifier").then_some(first)
}

// ── Definitions: fns + container types + consts/vars + tests ────────────────

fn collect_symbols(root: &Node, ctx: &ExtractCtx<'_>, namespaces: &[String]) -> Vec<Symbol> {
    let mut out = Vec::new();
    for child in root.children(&mut root.walk()) {
        collect_decl(&child, ctx, namespaces, &[], &mut out);
    }
    out
}

/// Emit symbols for one top-level or container-member declaration.
///
/// `type_path` is the enclosing container-type name chain (empty at file level).
/// Only container bodies are recursed into — function bodies are not (Zig has
/// no free nested fns; a type declared inside a fn body is a local detail).
fn collect_decl(
    node: &Node,
    ctx: &ExtractCtx<'_>,
    namespaces: &[String],
    type_path: &[String],
    out: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_declaration" => {
            let Some(name_node) = node.child_by_field_name("name") else {
                return;
            };
            let name = node_text(&name_node, ctx.bytes).to_owned();
            let kind = if type_path.is_empty() {
                SymbolKind::Function
            } else {
                SymbolKind::Method
            };
            let signature = one_line_signature(node_text(node, ctx.bytes), &['{']);
            out.push(make_symbol(
                ctx,
                node,
                name.clone(),
                kind,
                zig_visibility(node),
                descriptors(
                    namespaces,
                    type_path,
                    Descriptor::Method {
                        name,
                        disambiguator: String::new(),
                    },
                ),
                signature,
            ));
        }
        "variable_declaration" => {
            // Only keyworded declarations define anything (no keyword = reassignment).
            let Some(kw) = decl_keyword(node) else {
                return;
            };
            let Some(name_node) = decl_name(node) else {
                return;
            };
            let name = node_text(&name_node, ctx.bytes).to_owned();
            let value = decl_value(node);
            let vis = zig_visibility(node);
            let signature = one_line_signature(node_text(node, ctx.bytes), &['{', ';']);

            match value.as_ref().map(Node::kind) {
                // Zig has no `struct Name` form — a type is `const Name = struct { … }`.
                Some("struct_declaration" | "enum_declaration" | "union_declaration") => {
                    let value_node = match value {
                        Some(v) => v,
                        None => return,
                    };
                    let kind = match value_node.kind() {
                        "enum_declaration" => SymbolKind::Enum,
                        // NOTE: SymbolKind has no Union variant — unions map to Struct.
                        _ => SymbolKind::Struct,
                    };
                    out.push(make_symbol(
                        ctx,
                        node,
                        name.clone(),
                        kind,
                        vis,
                        descriptors(namespaces, type_path, Descriptor::Type(name.clone())),
                        signature,
                    ));
                    // Container members: methods and nested types.
                    let mut inner_path = type_path.to_vec();
                    inner_path.push(name);
                    for member in value_node.children(&mut value_node.walk()) {
                        collect_decl(&member, ctx, namespaces, &inner_path, out);
                    }
                }
                // `const std = @import(...)` is an import binding, not a value def.
                Some("builtin_function") => {}
                _ => {
                    // Container-member consts/vars are skipped in v1 (fields and
                    // constants inside types are not table stakes); top-level only.
                    if !type_path.is_empty() {
                        return;
                    }
                    let kind = if kw == "const" {
                        SymbolKind::Const
                    } else {
                        SymbolKind::Static
                    };
                    out.push(make_symbol(
                        ctx,
                        node,
                        name.clone(),
                        kind,
                        vis,
                        descriptors(namespaces, type_path, Descriptor::Term(name)),
                        signature,
                    ));
                }
            }
        }
        "test_declaration" => {
            // `test "add works" { … }` — the string content is the display name.
            // Tests cannot be `pub`; they are file-local → Private.
            let Some(name) = test_name(node, ctx.bytes) else {
                return;
            };
            let signature = one_line_signature(node_text(node, ctx.bytes), &['{']);
            out.push(make_symbol(
                ctx,
                node,
                name.clone(),
                SymbolKind::Function,
                Visibility::Private,
                descriptors(
                    namespaces,
                    type_path,
                    Descriptor::Method {
                        name,
                        disambiguator: String::new(),
                    },
                ),
                signature,
            ));
        }
        _ => {}
    }
}

/// Zig's real visibility signal: `pub` → Public, otherwise Private.
fn zig_visibility(node: &Node) -> Visibility {
    if is_pub(node) {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

/// Namespace descriptors + enclosing container `Type` descriptors + the leaf.
fn descriptors(namespaces: &[String], type_path: &[String], leaf: Descriptor) -> Vec<Descriptor> {
    let mut d: Vec<Descriptor> = namespaces
        .iter()
        .cloned()
        .map(Descriptor::Namespace)
        .collect();
    d.extend(type_path.iter().cloned().map(Descriptor::Type));
    d.push(leaf);
    d
}

/// The display name of a `test_declaration`: its `string`'s `string_content`.
fn test_name(node: &Node, bytes: &[u8]) -> Option<String> {
    let string = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "string")?;
    string
        .children(&mut string.walk())
        .find(|c| c.kind() == "string_content")
        .map(|c| node_text(&c, bytes).to_owned())
}

// ── Imports: @import("std") / @import("./file.zig") ─────────────────────────

/// Walk the tree for `builtin_function` nodes whose `builtin_identifier` is
/// literally `@import`, and emit an Import reference for the string argument.
///
/// The reference name is the binding identifier when the `@import` is the
/// direct RHS of a `const x = @import(...)` declaration (so member calls
/// `x.foo()` can resolve through the binding), else the path stem
/// (`"./helper.zig"` → `helper`). `from_path` is always the raw path text.
fn collect_imports(
    node: &Node,
    bytes: &[u8],
    file: &str,
    module_id: &str,
    out: &mut Vec<Reference>,
) {
    if node.kind() == "builtin_function" {
        if let Some((path_node, path)) = import_path(node, bytes) {
            let name = binding_name_for_import(node, bytes)
                .unwrap_or_else(|| import_stem(&path).to_owned());
            push_import_ref(out, &name, &path_node, file, module_id, &path);
        }
        // `@import` arguments hold no further imports; sibling walks continue below.
    }
    for child in node.children(&mut node.walk()) {
        collect_imports(&child, bytes, file, module_id, out);
    }
}

/// If `node` is an `@import` builtin call, return its string argument
/// (`string_content` node + text).
fn import_path<'tree>(node: &Node<'tree>, bytes: &[u8]) -> Option<(Node<'tree>, String)> {
    let ident = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "builtin_identifier")?;
    if node_text(&ident, bytes) != "@import" {
        return None;
    }
    let args = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "arguments")?;
    let string = args
        .children(&mut args.walk())
        .find(|c| c.kind() == "string")?;
    let content = string
        .children(&mut string.walk())
        .find(|c| c.kind() == "string_content")?;
    Some((content, node_text(&content, bytes).to_owned()))
}

/// The local binding name when this `@import` is the direct value of a
/// keyworded `variable_declaration` (`const std = @import("std")`).
fn binding_name_for_import(builtin: &Node, bytes: &[u8]) -> Option<String> {
    let parent = builtin.parent()?;
    if parent.kind() != "variable_declaration" || decl_keyword(&parent).is_none() {
        return None;
    }
    let name = decl_name(&parent)?;
    Some(node_text(&name, bytes).to_owned())
}

/// The module stem of an `@import` path: `"std"` → `std`,
/// `"./sub/helper.zig"` → `helper`.
fn import_stem(path: &str) -> &str {
    let leaf = path.rsplit('/').next().unwrap_or(path);
    leaf.strip_suffix(".zig").unwrap_or(leaf)
}

// ── Edge richness: TypeRef ───────────────────────────────────────────────────

/// Emit [`RefRole::TypeRef`] references for user-type annotations:
/// parameter types, function return types, container-field types, and
/// `const`/`var` type annotations. `builtin_type` nodes (`i32`, `void`, …) are
/// a distinct grammar kind and are naturally skipped; only bare `identifier`
/// and field-qualified (`std.ArrayList`) annotations emit (wrapped types like
/// `?T` / `*T` / `[]T` are a documented ceiling).
fn collect_type_references(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    let ctx = match node.kind() {
        "parameter" => Some(TypeRefContext::ParameterType),
        "function_declaration" => Some(TypeRefContext::ReturnType),
        "container_field" => Some(TypeRefContext::Field),
        "variable_declaration" => Some(TypeRefContext::Other),
        _ => None,
    };
    if let Some(ctx) = ctx {
        if let Some(type_node) = node.child_by_field_name("type") {
            if let Some((name, leaf)) = type_leaf(&type_node, bytes) {
                push_type_ref(out, &name, &leaf, file, ctx);
            }
        }
    }
    for child in node.children(&mut node.walk()) {
        collect_type_references(&child, bytes, file, out);
    }
}

/// The user-type leaf of a type annotation: a bare `identifier`, or the
/// `member:` leaf of a field-qualified `field_expression` (`std.ArrayList` →
/// `ArrayList`). Returns `None` for `builtin_type` and anything else.
fn type_leaf<'tree>(node: &Node<'tree>, bytes: &[u8]) -> Option<(String, Node<'tree>)> {
    match node.kind() {
        "identifier" => Some((node_text(node, bytes).to_owned(), *node)),
        "field_expression" => {
            let member = node.child_by_field_name("member")?;
            (member.kind() == "identifier").then(|| (node_text(&member, bytes).to_owned(), member))
        }
        _ => None,
    }
}

// ── Edge richness: Read / Write ─────────────────────────────────────────────

/// Emit [`RefRole::Read`] for identifiers in value positions and
/// [`RefRole::Write`] for bare-identifier reassignment targets.
///
/// Skipped positions: fn/test names and parameter names (bindings), the LHS of
/// a keyworded `variable_declaration` (a new binding), `type:` annotations
/// (TypeRef pass), call callees (Call pass), and `member:` leaves of field
/// accesses (only the receiver object reads). Applies [`MIN_REF_LEN`].
fn collect_read_write_references(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    match node.kind() {
        "identifier" => {
            let name = node_text(node, bytes);
            if name.len() >= MIN_REF_LEN {
                push_ref(out, name, node, file, RefRole::Read);
            }
        }
        "function_declaration" => {
            // Name / parameters / return type are bindings or TypeRefs; only the
            // body holds reads and writes.
            if let Some(body) = node.child_by_field_name("body") {
                collect_read_write_references(&body, bytes, file, out);
            }
        }
        // Field names/types (and enum members) are declarations, not reads;
        // default values are a non-table-stakes rarity — skipped whole.
        "container_field" => {}
        "variable_declaration" => {
            let has_kw = decl_keyword(node).is_some();
            let type_child = node.child_by_field_name("type");
            let mut cursor = node.walk();
            let mut named = node.named_children(&mut cursor).peekable();
            if let Some(first) = named.peek().copied() {
                if first.kind() == "identifier" {
                    named.next();
                    if !has_kw {
                        // Bare reassignment — the only Write shape in this grammar.
                        let name = node_text(&first, bytes);
                        if name.len() >= MIN_REF_LEN {
                            push_ref(out, name, &first, file, RefRole::Write);
                        }
                    }
                    // Keyworded LHS is a binding introduction — neither role.
                }
            }
            for child in named {
                if Some(child) == type_child {
                    continue; // TypeRef pass owns annotations.
                }
                collect_read_write_references(&child, bytes, file, out);
            }
        }
        "call_expression" => {
            let function = node.child_by_field_name("function");
            for child in node.named_children(&mut node.walk()) {
                if Some(child) == function {
                    // Callee identifier → Call pass; a member callee still reads
                    // its receiver object, which the field_expression arm covers.
                    if child.kind() == "field_expression" {
                        collect_read_write_references(&child, bytes, file, out);
                    }
                    continue;
                }
                collect_read_write_references(&child, bytes, file, out);
            }
        }
        "field_expression" => {
            // `a.b` — only the receiver object is a read; the member is a field.
            if let Some(object) = node.child_by_field_name("object") {
                collect_read_write_references(&object, bytes, file, out);
            }
        }
        _ => {
            for child in node.children(&mut node.walk()) {
                collect_read_write_references(&child, bytes, file, out);
            }
        }
    }
}

// ── Scope tree (Tier-B) ──────────────────────────────────────────────────────

/// Build the lexical scope tree for one Zig file.
///
/// `scopes[0]` is the file-root `Module` scope. `function_declaration` and
/// `test_declaration` open a `Function` scope (their body block is peeled so it
/// does not double-open); every other `block` (if/while bodies, `comptime`
/// blocks, bare blocks) opens a `Block` scope. Container declarations do not
/// open a scope (their member fns open their own).
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
        "function_declaration" | "test_declaration" => {
            let fn_id = push_scope(
                scopes,
                Some(parent_id),
                node_span(node),
                ScopeKind::Function,
            );
            // Peel the body block: its children hang off the Function scope
            // directly, avoiding a redundant Block scope.
            let body = node.child_by_field_name("body").or_else(|| {
                node.children(&mut node.walk())
                    .find(|c| c.kind() == "block")
            });
            if let Some(body) = body {
                for child in body.children(&mut body.walk()) {
                    scope_dfs(&child, fn_id, scopes);
                }
            }
        }
        "block" => {
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

/// Collect [`BindingKind::Param`] bindings for fn parameters and
/// [`BindingKind::Local`] bindings for keyworded `var`/`const` declarations in
/// non-root scopes (scope-0 declarations are already Definition/Import
/// bindings). The keyword check keeps reassignments from becoming duplicate
/// bindings (the grammar's shared-node pitfall — see [`decl_keyword`]).
fn collect_local_bindings(node: &Node, bytes: &[u8], scopes: &[Scope], out: &mut Vec<Binding>) {
    match node.kind() {
        "function_declaration" => {
            if let Some(params) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "parameters")
            {
                for param in params.children(&mut params.walk()) {
                    if param.kind() != "parameter" {
                        continue;
                    }
                    if let Some(name_node) = param.child_by_field_name("name") {
                        push_binding(
                            out,
                            node_text(&name_node, bytes).to_owned(),
                            name_node.start_byte(),
                            BindingKind::Param,
                            scopes,
                        );
                    }
                }
            }
        }
        "variable_declaration" if decl_keyword(node).is_some() => {
            if let Some(name_node) = decl_name(node) {
                let intro = name_node.start_byte();
                if innermost_scope(intro, scopes) != Some(0) {
                    push_binding(
                        out,
                        node_text(&name_node, bytes).to_owned(),
                        intro,
                        BindingKind::Local,
                        scopes,
                    );
                }
            }
        }
        _ => {}
    }
    for child in node.children(&mut node.walk()) {
        collect_local_bindings(&child, bytes, scopes, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::RefRole;

    #[test]
    fn pub_fn_is_public_private_fn_is_private() {
        let src = "pub fn add(a: i32, b: i32) i32 { return a + b; }\nfn helper() void {}\n";
        let facts = ZigExtractor.extract(src, "src/geometry/point.zig").unwrap();

        let add = facts
            .symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("expected a Function symbol 'add'");
        assert_eq!(add.kind, SymbolKind::Function);
        assert_eq!(add.visibility, Visibility::Public);
        assert_eq!(
            add.id.to_scip_string(),
            "codegraph . . . geometry/point/add()."
        );

        let helper = facts
            .symbols
            .iter()
            .find(|s| s.name == "helper")
            .expect("expected a Function symbol 'helper'");
        assert_eq!(helper.visibility, Visibility::Private);
        assert_eq!(
            helper.id.to_scip_string(),
            "codegraph . . . geometry/point/helper()."
        );
    }

    #[test]
    fn struct_with_member_fn() {
        let src = "pub const Point = struct {\n    x: i32,\n    y: i32,\n    pub fn magnitude(self: Point) i32 { return self.x; }\n};\n";
        let facts = ZigExtractor.extract(src, "src/geometry/point.zig").unwrap();

        let point = facts
            .symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("expected a Struct symbol 'Point'");
        assert_eq!(point.kind, SymbolKind::Struct);
        assert_eq!(point.visibility, Visibility::Public);
        assert_eq!(
            point.id.to_scip_string(),
            "codegraph . . . geometry/point/Point#"
        );

        let magnitude = facts
            .symbols
            .iter()
            .find(|s| s.name == "magnitude")
            .expect("expected a Method symbol 'magnitude'");
        assert_eq!(magnitude.kind, SymbolKind::Method);
        assert_eq!(magnitude.visibility, Visibility::Public);
        assert_eq!(
            magnitude.id.to_scip_string(),
            "codegraph . . . geometry/point/Point#magnitude()."
        );
    }

    #[test]
    fn enum_and_union_declarations() {
        let src =
            "const Color = enum { Red, Green, Blue };\nconst Value = union { i: i32, f: f32 };\n";
        let facts = ZigExtractor.extract(src, "src/kinds.zig").unwrap();

        let color = facts
            .symbols
            .iter()
            .find(|s| s.name == "Color")
            .expect("expected an Enum symbol 'Color'");
        assert_eq!(color.kind, SymbolKind::Enum);
        assert_eq!(color.visibility, Visibility::Private);
        assert_eq!(color.id.to_scip_string(), "codegraph . . . kinds/Color#");

        // NOTE: SymbolKind has no Union variant — unions map to Struct.
        let value = facts
            .symbols
            .iter()
            .find(|s| s.name == "Value")
            .expect("expected a symbol 'Value'");
        assert_eq!(value.kind, SymbolKind::Struct);
    }

    #[test]
    fn top_level_const_and_var() {
        let src = "pub var global_count: i32 = 0;\nconst limit = 10;\n";
        let facts = ZigExtractor.extract(src, "src/state.zig").unwrap();

        let count = facts
            .symbols
            .iter()
            .find(|s| s.name == "global_count")
            .expect("expected a Static symbol 'global_count'");
        assert_eq!(count.kind, SymbolKind::Static);
        assert_eq!(count.visibility, Visibility::Public);
        assert_eq!(
            count.id.to_scip_string(),
            "codegraph . . . state/global_count."
        );

        let limit = facts
            .symbols
            .iter()
            .find(|s| s.name == "limit")
            .expect("expected a Const symbol 'limit'");
        assert_eq!(limit.kind, SymbolKind::Const);
        assert_eq!(limit.visibility, Visibility::Private);
        assert_eq!(limit.id.to_scip_string(), "codegraph . . . state/limit.");
    }

    #[test]
    fn test_declaration_is_private_function() {
        let src = "test \"add works\" { }\n";
        let facts = ZigExtractor.extract(src, "src/math.zig").unwrap();
        let t = facts
            .symbols
            .iter()
            .find(|s| s.name == "add works")
            .expect("expected a symbol for the test block");
        assert_eq!(t.kind, SymbolKind::Function);
        assert_eq!(t.visibility, Visibility::Private);
        assert_eq!(t.id.to_scip_string(), "codegraph . . . math/`add works`().");
    }

    #[test]
    fn std_and_relative_imports() {
        let src = "const std = @import(\"std\");\nconst helper = @import(\"./sub/helper.zig\");\n";
        let facts = ZigExtractor.extract(src, "src/main.zig").unwrap();

        let std_import = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Import && r.name == "std")
            .expect("expected an Import reference 'std'");
        assert_eq!(std_import.from_path.as_deref(), Some("std"));

        let helper_import = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Import && r.name == "helper")
            .expect("expected an Import reference 'helper'");
        assert_eq!(helper_import.from_path.as_deref(), Some("./sub/helper.zig"));

        // Imports must not also register as Calls (builtin_function is a
        // distinct node kind from call_expression).
        assert!(
            !facts.references.iter().any(|r| r.role == RefRole::Call),
            "@import must not emit a Call reference"
        );
    }

    #[test]
    fn import_inside_usingnamespace_still_emits() {
        // usingnamespace itself is an unresolved ceiling, but the @import it
        // wraps is still a real file dependency. No binding name — path stem.
        let src = "pub usingnamespace @import(\"std\");\n";
        let facts = ZigExtractor.extract(src, "src/re.zig").unwrap();
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Import && r.name == "std"),
            "expected an Import reference from inside usingnamespace"
        );
    }

    #[test]
    fn free_call_and_member_call_with_qualifier() {
        let src = "fn run(p: Point) void {\n    freeCall();\n    const m = p.magnitude();\n    _ = m;\n}\n";
        let facts = ZigExtractor.extract(src, "src/run.zig").unwrap();

        let free = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "freeCall")
            .expect("expected a Call reference 'freeCall'");
        assert_eq!(free.qualifier, None);

        let member = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "magnitude")
            .expect("expected a Call reference 'magnitude'");
        assert_eq!(member.qualifier.as_deref(), Some("p"));
    }

    #[test]
    fn chained_member_call_qualifier_is_full_receiver() {
        let src = "fn f() void { std.debug.print(\"x\", .{}); }\n";
        let facts = ZigExtractor.extract(src, "src/log.zig").unwrap();
        let print = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "print")
            .expect("expected a Call reference 'print'");
        assert_eq!(print.qualifier.as_deref(), Some("std.debug"));
    }

    #[test]
    fn reassignment_is_write_declaration_is_not() {
        // The grammar's shared-node pitfall: `var count = 0;`, `count = count + 1;`
        // and `const total = count;` are ALL `variable_declaration` nodes.
        let src = "fn f() void {\n    var count: i32 = 0;\n    count = count + 1;\n    const total = count;\n    _ = total;\n}\n";
        let facts = ZigExtractor.extract(src, "src/w.zig").unwrap();

        let writes: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Write && r.name == "count")
            .collect();
        assert_eq!(
            writes.len(),
            1,
            "exactly one Write for 'count' (the bare reassignment), got {writes:?}"
        );

        let reads: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Read && r.name == "count")
            .collect();
        assert_eq!(
            reads.len(),
            2,
            "two Reads for 'count' (reassignment RHS + const init), got {reads:?}"
        );

        // Declarations must NOT be misread as Writes.
        assert!(
            !facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Write && r.name == "total"),
            "`const total = …` is a declaration, not a Write"
        );
    }

    #[test]
    fn declaration_bindings_not_duplicated_by_reassignment() {
        let src = "fn f() void {\n    var count: i32 = 0;\n    count = 1;\n    count = 2;\n}\n";
        let facts = ZigExtractor.extract(src, "src/b.zig").unwrap();
        let locals: Vec<_> = facts
            .bindings
            .iter()
            .filter(|b| b.kind == BindingKind::Local && b.name == "count")
            .collect();
        assert_eq!(
            locals.len(),
            1,
            "reassignments must not create extra Local bindings, got {locals:?}"
        );
    }

    #[test]
    fn params_bind_in_function_scope() {
        let src = "pub fn add(a: i32, b: i32) i32 { return a + b; }\n";
        let facts = ZigExtractor.extract(src, "src/math.zig").unwrap();

        let fn_scope = facts
            .scopes
            .iter()
            .position(|s| s.kind == ScopeKind::Function)
            .expect("expected a Function scope");
        let mut params: Vec<(&str, usize)> = facts
            .bindings
            .iter()
            .filter(|b| b.kind == BindingKind::Param)
            .map(|b| (b.name.as_str(), b.scope))
            .collect();
        params.sort_by_key(|(n, _)| *n);
        assert_eq!(params, vec![("a", fn_scope), ("b", fn_scope)]);
    }

    #[test]
    fn type_annotations_emit_type_refs() {
        let src = "const Point = struct { x: i32 };\nfn dist(p: Point) Point {\n    var q: Point = p;\n    return q;\n}\n";
        let facts = ZigExtractor.extract(src, "src/t.zig").unwrap();
        let ctxs: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::TypeRef && r.name == "Point")
            .filter_map(|r| r.type_ref_ctx)
            .collect();
        assert!(
            ctxs.contains(&TypeRefContext::ParameterType),
            "expected a ParameterType TypeRef, got {ctxs:?}"
        );
        assert!(
            ctxs.contains(&TypeRefContext::ReturnType),
            "expected a ReturnType TypeRef, got {ctxs:?}"
        );
        assert!(
            ctxs.contains(&TypeRefContext::Other),
            "expected a local-annotation (Other) TypeRef, got {ctxs:?}"
        );
        // Builtin types never emit TypeRefs (`i32` is a builtin_type node).
        assert!(
            !facts
                .references
                .iter()
                .any(|r| r.role == RefRole::TypeRef && r.name == "i32"),
            "builtin types must not emit TypeRefs"
        );
    }

    #[test]
    fn comptime_block_declarations_are_table_stakes() {
        // comptime is never evaluated; its block contents behave like any block.
        let src = "comptime { const x = 5; _ = x; }\n";
        let facts = ZigExtractor.extract(src, "src/ct.zig").unwrap();
        // No top-level symbol for `x` (it is block-local), and no crash.
        assert!(
            !facts.symbols.iter().any(|s| s.name == "x"),
            "comptime-block locals are not top-level symbols"
        );
        // The block-local binding IS visible to Tier-B.
        assert!(
            facts
                .bindings
                .iter()
                .any(|b| b.kind == BindingKind::Local && b.name == "x"),
            "expected a Local binding for the comptime-block const"
        );
    }

    #[test]
    fn fn_opens_function_scope_under_module() {
        let src = "fn f() void { var x: i32 = 0; _ = x; }\n";
        let facts = ZigExtractor.extract(src, "src/s.zig").unwrap();
        assert_eq!(facts.scopes[0].kind, ScopeKind::Module);
        let fn_scope = facts
            .scopes
            .iter()
            .find(|s| s.kind == ScopeKind::Function)
            .expect("expected a Function scope");
        assert_eq!(fn_scope.parent, Some(0));
    }

    #[test]
    fn module_symbol_emitted() {
        let facts = ZigExtractor
            .extract("fn f() void {}\n", "src/util.zig")
            .unwrap();
        let modules: Vec<_> = facts
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module)
            .collect();
        assert_eq!(modules.len(), 1, "expected exactly one Module symbol");
        assert_eq!(modules[0].name, "util");
    }
}
