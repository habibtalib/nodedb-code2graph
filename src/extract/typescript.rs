// SPDX-License-Identifier: Apache-2.0

//! TypeScript extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: ALL top-level declarations, tagged with their real [`Visibility`]:
//! exported declarations (`export function/class/interface/type/enum/const`,
//! including `export default function/class`) → [`Visibility::Public`]; bare
//! (non-exported) top-level declarations → [`Visibility::Private`].
//! Qualified identity follows the file's module path (`src/auth/jwt.ts` →
//! namespaces `src`,`auth`,`jwt`), so a symbol is `…/jwt/validateToken().`.
//! References: callee identifiers of `call_expression` nodes.
//!
//! `.tsx`/`.jsx` files are parsed with the TSX grammar, otherwise TypeScript.
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.
//!
//! The extraction core (`extract_ecmascript`) is shared with the JavaScript
//! extractor, which reuses the TypeScript grammar (a superset of JavaScript);
//! the two differ only in their language tag.

use tree_sitter::{Node, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{
    Binding, BindingKind, ByteSpan, EntryPoint, FileFacts, RefRole, Reference, Scope, ScopeId,
    ScopeKind, Symbol, SymbolKind, TypeRefContext, Visibility,
};
use crate::lang::Language;
use crate::symbol::Descriptor;

use super::{
    ExtractCtx, Extractor, MIN_REF_LEN, attach_reference_scopes, child_text,
    collect_call_references, definition_bindings, import_bindings, make_symbol, node_span,
    node_text, one_line_signature, push_binding, push_ref, push_scope, push_type_ref,
};

/// Tree-sitter query capturing call-callee identifiers.
const CALL_QUERY: &str = r#"
(call_expression
  function: [
    (identifier) @callee
    (member_expression property: (property_identifier) @callee)
  ]
)
"#;

/// Terminal property/identifier names that indicate an HTTP-route call
/// (Express/Fastify/Koa/Hono `<receiver>.<verb>(path, handler)` shape). Mirrors
/// Python's `PY_ROUTE_VERBS`, plus `all` (Express/Fastify's "match any HTTP
/// method" registration — genuinely route-shaped, unlike `.use()`).
/// Deliberately EXCLUDES `use`: Express/Koa's generic middleware-registration
/// call (logging, auth, static files — not HTTP routing); including it would
/// flood `HttpRoute` markers with false positives on every Express/Koa app.
const TS_ROUTE_VERBS: &[&str] = &[
    "get",
    "post",
    "put",
    "delete",
    "patch",
    "head",
    "options",
    "trace",
    "route",
    "websocket",
    "ws",
    "all",
];

/// Walk the whole tree for Express/Fastify/Koa/Hono-shaped route-registration
/// calls and, where the handler argument is a bare identifier matching an
/// already-collected top-level `Symbol`'s name, attach an `EntryPoint::HttpRoute`
/// marker to that symbol. Inline (arrow/function-expression) handlers are NOT
/// detected — there is no per-call or per-inline-function `Symbol` in this
/// extractor to attach the marker to, and fabricating one would violate the
/// "never guess" contract. This is an honest, documented ceiling, not a gap.
fn attach_call_entry_points(root: &Node, bytes: &[u8], symbols: &mut [Symbol]) {
    if root.kind() == "call_expression" {
        if let Some((terminal, callee_text)) = call_route_match(root, bytes) {
            if TS_ROUTE_VERBS.contains(&terminal) {
                if let Some(handler_name) = last_arg_bare_identifier(root, bytes) {
                    if let Some(sym) = symbols.iter_mut().find(|s| s.name == handler_name) {
                        sym.entry_points
                            .push(EntryPoint::HttpRoute(callee_text.to_owned()));
                    }
                }
            }
        }
    }
    for child in root.children(&mut root.walk()) {
        attach_call_entry_points(&child, bytes, symbols);
    }
}

/// If `call` is a `call_expression` whose `function:` field is a
/// `member_expression` (terminal = its `property:` field text) or a bare
/// `identifier` (terminal = its own text), return `(terminal, full_callee_text)`.
fn call_route_match<'a>(call: &Node, bytes: &'a [u8]) -> Option<(&'a str, &'a str)> {
    let func = call.child_by_field_name("function")?;
    match func.kind() {
        "member_expression" => {
            let prop = func.child_by_field_name("property")?;
            let terminal = node_text(&prop, bytes);
            let callee_text = node_text(&func, bytes);
            Some((terminal, callee_text))
        }
        "identifier" => {
            let text = node_text(&func, bytes);
            Some((text, text))
        }
        _ => None,
    }
}

/// Return the text of the LAST argument in `call`'s `arguments:` list, only if
/// it is a bare `identifier` node (not an arrow/function expression or any
/// other shape).
fn last_arg_bare_identifier<'a>(call: &Node, bytes: &'a [u8]) -> Option<&'a str> {
    let args = call.child_by_field_name("arguments")?;
    let last = args.named_children(&mut args.walk()).last()?;
    if last.kind() == "identifier" {
        Some(node_text(&last, bytes))
    } else {
        None
    }
}

/// NestJS decorator simple-names (PascalCase, distinct from `TS_ROUTE_VERBS`'s
/// lowercase call-form) that mark an HTTP-route entry point. `Controller` is
/// class-level; the rest are method-level. Mirrors Java's
/// `JAVA_ROUTE_ANNOTATIONS` terminal-name match, applied to TS decorator syntax.
const TS_ROUTE_DECORATORS: &[&str] = &["Get", "Post", "Put", "Delete", "Patch", "Controller"];

/// Collect `TS_ROUTE_DECORATORS` matches from `node`'s direct `decorator`
/// children. A `decorator` node wraps either a bare `identifier` (`@Controller`)
/// or a `call_expression` whose `function:` is the identifier (`@Get(':id')`);
/// either way the identifier's text is the candidate name.
fn route_decorators_on(node: &Node, bytes: &[u8]) -> Vec<EntryPoint> {
    let mut markers = Vec::new();
    for child in node.children(&mut node.walk()) {
        if child.kind() != "decorator" {
            continue;
        }
        let ident = child
            .children(&mut child.walk())
            .find_map(|c| match c.kind() {
                "identifier" => Some(c),
                "call_expression" => c
                    .child_by_field_name("function")
                    .filter(|f| f.kind() == "identifier"),
                _ => None,
            });
        if let Some(ident) = ident {
            let name = node_text(&ident, bytes);
            if TS_ROUTE_DECORATORS.contains(&name) {
                markers.push(EntryPoint::HttpRoute(name.to_owned()));
            }
        }
    }
    markers
}

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
    let ts_language = if file.ends_with(".tsx") || file.ends_with(".jsx") {
        crate::grammar::tsx()
    } else {
        crate::grammar::typescript()
    };
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

    let ctx = ExtractCtx { bytes, file, lang };
    let defs = collect_symbols(&root, &ctx, &namespaces);
    let def_bindings = definition_bindings(&defs);
    let mut symbols = defs;
    attach_call_entry_points(&root, bytes, &mut symbols);
    let mod_sym = super::module_symbol(lang, &namespaces, file, source.len());
    let module_id = mod_sym.id.to_scip_string();
    symbols.push(mod_sym);
    let mut references =
        collect_call_references(&root, &ts_language, CALL_QUERY, lang, bytes, file)?;
    collect_inheritance(&root, bytes, file, &mut references);
    collect_imports(&root, bytes, file, &mut references, &module_id);
    collect_type_references(&root, bytes, file, &mut references);
    collect_read_references(&root, bytes, file, &mut references);
    collect_write_references(&root, bytes, file, &mut references);

    let scopes = collect_scopes(&root, source.len());
    attach_reference_scopes(&mut references, &scopes);
    let mut bindings = collect_bindings(&root, bytes, &scopes);
    bindings.extend(def_bindings);
    bindings.extend(import_bindings(&references, &scopes));

    Ok(FileFacts {
        file: file.to_owned(),
        lang: lang.as_str().to_owned(),
        symbols,
        references,
        scopes,
        bindings,
        ffi_exports: Vec::new(),
    })
}

/// Module path (namespace descriptors) from a source file path: all path
/// segments, with the final file extension stripped from the last segment.
pub(super) fn module_namespaces(file: &str) -> Vec<String> {
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

/// Bare top-level declaration node kinds that are emitted with
/// [`Visibility::Private`] (non-exported, module-scoped).
const BARE_DECL_KINDS: &[&str] = &[
    "function_declaration",
    "generator_function_declaration",
    "class_declaration",
    "abstract_class_declaration",
    "interface_declaration",
    "type_alias_declaration",
    "enum_declaration",
    "lexical_declaration",
    "variable_declaration",
];

fn collect_symbols(root: &Node, ctx: &ExtractCtx, namespaces: &[String]) -> Vec<Symbol> {
    let mut out = Vec::new();
    for stmt in root.children(&mut root.walk()) {
        match stmt.kind() {
            "export_statement" => {
                // Exported declarations are direct children of the export statement.
                // The span covers the full `export ...` statement node.
                for decl in stmt.children(&mut stmt.walk()) {
                    emit_declaration(
                        DeclSite { decl, span: stmt },
                        ctx,
                        namespaces,
                        Visibility::Public,
                        &mut out,
                    );
                }
                // NestJS route decorators: class-level `@Controller` lives on the
                // export_statement's own `decorator:` field; method-level
                // `@Get`/`@Post`/... live as direct children of the class_body.
                // Both aggregate onto the class's just-pushed Symbol — there is
                // no per-method Symbol in this extractor to attach them to.
                if let Some(class_decl) = stmt.children(&mut stmt.walk()).find(|c| {
                    matches!(c.kind(), "class_declaration" | "abstract_class_declaration")
                }) {
                    let mut markers = route_decorators_on(&stmt, ctx.bytes);
                    if let Some(body) = class_decl.child_by_field_name("body") {
                        markers.extend(route_decorators_on(&body, ctx.bytes));
                    }
                    if !markers.is_empty() {
                        if let Some(sym) = out.last_mut() {
                            sym.entry_points.extend(markers);
                        }
                    }
                }
            }
            kind if BARE_DECL_KINDS.contains(&kind) => {
                // Non-exported top-level declaration: the declaration node is
                // its own span node (there is no enclosing export_statement).
                emit_declaration(
                    DeclSite {
                        decl: stmt,
                        span: stmt,
                    },
                    ctx,
                    namespaces,
                    Visibility::Private,
                    &mut out,
                );
                // TS also permits decorators on non-exported classes: the
                // decorator lives directly on the `class_declaration` node itself.
                if matches!(kind, "class_declaration" | "abstract_class_declaration") {
                    let mut markers = route_decorators_on(&stmt, ctx.bytes);
                    if let Some(body) = stmt.child_by_field_name("body") {
                        markers.extend(route_decorators_on(&body, ctx.bytes));
                    }
                    if !markers.is_empty() {
                        if let Some(sym) = out.last_mut() {
                            sym.entry_points.extend(markers);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// A declaration node together with the node whose span/line locates it. For an
/// exported declaration `span` is the enclosing `export_statement`; for a bare
/// declaration `span` is the declaration itself. Named fields (rather than a
/// same-typed `(Node, Node)` tuple) so the two can't be transposed by accident.
struct DeclSite<'t> {
    decl: Node<'t>,
    span: Node<'t>,
}

/// Append symbol(s) for one declaration node (a `lexical_declaration` or
/// `variable_declaration` may yield several). `visibility` reflects whether the
/// declaration was exported (`Public`) or bare (`Private`).
fn emit_declaration(
    site: DeclSite,
    ctx: &ExtractCtx,
    namespaces: &[String],
    visibility: Visibility,
    out: &mut Vec<Symbol>,
) {
    let decl = &site.decl;
    let span_node = &site.span;
    let push = |out: &mut Vec<Symbol>, name: String, kind: SymbolKind, leaf: Descriptor| {
        let mut descriptors: Vec<Descriptor> = namespaces
            .iter()
            .cloned()
            .map(Descriptor::Namespace)
            .collect();
        descriptors.push(leaf);
        let signature = one_line_signature(node_text(decl, ctx.bytes), &['{']);
        out.push(make_symbol(
            ctx,
            span_node,
            name,
            kind,
            visibility,
            descriptors,
            signature,
        ));
    };

    match decl.kind() {
        "function_declaration" | "generator_function_declaration" => {
            if let Some(n) = child_text(decl, "identifier", ctx.bytes) {
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
        "class_declaration" | "abstract_class_declaration" => {
            emit_named(decl, ctx.bytes, SymbolKind::Class, out, &push)
        }
        "interface_declaration" => emit_named(decl, ctx.bytes, SymbolKind::Interface, out, &push),
        "type_alias_declaration" => emit_named(decl, ctx.bytes, SymbolKind::TypeAlias, out, &push),
        "enum_declaration" => {
            if let Some(n) = child_text(decl, "identifier", ctx.bytes) {
                push(out, n.clone(), SymbolKind::Enum, Descriptor::Type(n));
            }
        }
        "lexical_declaration" => {
            for vd in decl.children(&mut decl.walk()) {
                if vd.kind() != "variable_declarator" {
                    continue;
                }
                if let Some(n) = child_text(&vd, "identifier", ctx.bytes) {
                    push(out, n.clone(), SymbolKind::Const, Descriptor::Term(n));
                }
            }
        }
        "variable_declaration" => {
            // `var` declarations: same structure as `lexical_declaration`.
            for vd in decl.children(&mut decl.walk()) {
                if vd.kind() != "variable_declarator" {
                    continue;
                }
                if let Some(n) = child_text(&vd, "identifier", ctx.bytes) {
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

/// Recursively walk `node` collecting `Inherit` references for every
/// `class_declaration` and `interface_declaration` in the tree (including nested
/// classes).
///
/// Tree-sitter node shape (TypeScript / TSX grammar):
/// - `class_declaration` → optional `class_heritage` child
///   - `extends_clause` → field `value` (the superclass expression)
///   - `implements_clause` → named children: `type_identifier | generic_type |
///     nested_type_identifier`
/// - `interface_declaration` → optional `extends_type_clause` child
///   - named children: `type_identifier | generic_type | nested_type_identifier`
fn collect_inheritance(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    match node.kind() {
        "class_declaration" => {
            // Locate the `class_heritage` child (if any).
            if let Some(heritage) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "class_heritage")
            {
                for clause in heritage.children(&mut heritage.walk()) {
                    match clause.kind() {
                        "extends_clause" => {
                            // The superclass is the `value` field.
                            if let Some(value) = clause.child_by_field_name("value") {
                                super::push_ref(
                                    out,
                                    super::simple_type_name(node_text(&value, bytes), "."),
                                    &value,
                                    file,
                                    RefRole::IsImplementation,
                                );
                            }
                        }
                        "implements_clause" => {
                            // Each named child is an implemented interface type.
                            for type_node in clause.children(&mut clause.walk()) {
                                if type_node.is_named()
                                    && matches!(
                                        type_node.kind(),
                                        "type_identifier"
                                            | "generic_type"
                                            | "nested_type_identifier"
                                    )
                                {
                                    super::push_ref(
                                        out,
                                        super::simple_type_name(node_text(&type_node, bytes), "."),
                                        &type_node,
                                        file,
                                        RefRole::IsImplementation,
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        "interface_declaration" => {
            // Locate the `extends_type_clause` child (if any).
            if let Some(extends_clause) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "extends_type_clause")
            {
                for type_node in extends_clause.children(&mut extends_clause.walk()) {
                    if type_node.is_named()
                        && matches!(
                            type_node.kind(),
                            "type_identifier" | "generic_type" | "nested_type_identifier"
                        )
                    {
                        super::push_ref(
                            out,
                            super::simple_type_name(node_text(&type_node, bytes), "."),
                            &type_node,
                            file,
                            RefRole::IsImplementation,
                        );
                    }
                }
            }
        }
        _ => {}
    }

    // Recurse into all children so nested classes are covered.
    for child in node.children(&mut node.walk()) {
        collect_inheritance(&child, bytes, file, out);
    }
}

/// Recursively walk `node` collecting `Import` references for every
/// `import_statement` in the tree.
///
/// Tree-sitter node shape (TypeScript / TSX grammar):
/// ```text
/// import_statement
///   source: string            ← module path string — IGNORED
///   import_clause
///     identifier              ← default import: `import Foo from "x"`
///     named_imports
///       import_specifier
///         name: identifier    ← named import binding: `import { A } from "x"`
///         alias: identifier   ← IGNORED (`import { A as B }`)
///     namespace_import        ← `import * as ns from "x"` — SKIPPED entirely
/// ```
///
/// Only the binding name at the call-site is emitted; module sources and
/// aliases are deliberately not recorded.
fn collect_imports(
    node: &Node,
    bytes: &[u8],
    file: &str,
    out: &mut Vec<Reference>,
    module_id: &str,
) {
    if node.kind() == "import_statement" {
        // Extract the from-path once from the `source` field (a string literal).
        // The raw text includes surrounding quotes; strip both styles.
        let from_path = node
            .child_by_field_name("source")
            .map(|n| {
                let raw = super::node_text(&n, bytes);
                raw.trim_matches('"').trim_matches('\'').to_owned()
            })
            .unwrap_or_default();

        // Locate the `import_clause` child (may be absent for bare `import "x"`).
        if let Some(clause) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "import_clause")
        {
            for child in clause.children(&mut clause.walk()) {
                match child.kind() {
                    // Default import: `import Foo from "x"`
                    "identifier" => {
                        super::push_import_ref(
                            out,
                            super::node_text(&child, bytes),
                            &child,
                            file,
                            module_id,
                            &from_path,
                        );
                    }
                    // Named imports: `import { A, B as C } from "x"`
                    "named_imports" => {
                        for specifier in child.children(&mut child.walk()) {
                            if specifier.kind() != "import_specifier" {
                                continue;
                            }
                            // `name` field is the real (original) name, not the alias.
                            if let Some(name_node) = specifier.child_by_field_name("name") {
                                if name_node.kind() == "identifier" {
                                    super::push_import_ref(
                                        out,
                                        super::node_text(&name_node, bytes),
                                        &name_node,
                                        file,
                                        module_id,
                                        &from_path,
                                    );
                                }
                                // string-named imports (exotic) → skip silently
                            }
                        }
                    }
                    // Namespace import: `import * as ns from "x"` → skip
                    "namespace_import" => {}
                    _ => {}
                }
            }
        }
        // Do not recurse further into `import_statement`; it cannot contain
        // nested import statements.
        return;
    }

    // Recurse into all other nodes so top-level and module-scoped imports are covered.
    for child in node.children(&mut node.walk()) {
        collect_imports(&child, bytes, file, out, module_id);
    }
}

// ── Edge richness: TypeRef / Read / Write ────────────────────────────────────

/// Recursively walk `node` and emit [`RefRole::TypeRef`] references for every
/// type-identifier that appears in a typed annotation position.
///
/// Covered positions (TypeScript / TSX grammar):
/// - `required_parameter` / `optional_parameter` `type:` field → `ParameterType`
/// - `function_declaration` / `function_signature` / `method_definition` /
///   `arrow_function` `return_type:` field → `ReturnType`
/// - `public_field_definition` / `property_signature` `type:` field → `Field`
/// - Inside a `type_arguments` node (generic arguments) → `GenericArg`
/// - Any other `type_identifier` in a `type_annotation` → `Other`
///
/// For `generic_type` nodes the head `type_identifier` (the `name` field or
/// first named child) takes the outer context, then `type_arguments` children
/// are visited with `GenericArg`.
fn collect_type_references(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    // Helper: emit a type ref from a (possibly generic or nested) type node at
    // the given context. If the node is a `generic_type`, recurse into its
    // type_arguments with GenericArg context. If it is a `nested_type_identifier`
    // take the `right` field as the leaf name.
    fn emit_type_node(
        node: &Node,
        bytes: &[u8],
        file: &str,
        ctx: TypeRefContext,
        out: &mut Vec<Reference>,
    ) {
        match node.kind() {
            "type_identifier" => {
                let name = node_text(node, bytes);
                push_type_ref(out, name, node, file, ctx);
            }
            "generic_type" => {
                // The `name` field (or first named child) is the outer type name.
                if let Some(head) = node.child_by_field_name("name") {
                    emit_type_node(&head, bytes, file, ctx, out);
                }
                // Type arguments are generic params.
                if let Some(args) = node.child_by_field_name("type_arguments") {
                    for child in args.named_children(&mut args.walk()) {
                        emit_type_node(&child, bytes, file, TypeRefContext::GenericArg, out);
                    }
                }
            }
            "nested_type_identifier" => {
                // e.g. `ns.Type` — take the `right` (leaf) field.
                if let Some(right) = node.child_by_field_name("right") {
                    emit_type_node(&right, bytes, file, ctx, out);
                }
            }
            // Unwrap a bare `type_annotation` wrapper (the `: T` node itself).
            "type_annotation" => {
                for child in node.named_children(&mut node.walk()) {
                    emit_type_node(&child, bytes, file, ctx, out);
                }
            }
            // Union / intersection / parenthesized types — recurse so we catch
            // all leaves (e.g. `A | B`, `(C & D)`).
            "union_type" | "intersection_type" | "parenthesized_type" => {
                for child in node.named_children(&mut node.walk()) {
                    emit_type_node(&child, bytes, file, ctx, out);
                }
            }
            // Array / readonly wrappers: recurse into the element type.
            "array_type" | "readonly_type" => {
                for child in node.named_children(&mut node.walk()) {
                    emit_type_node(&child, bytes, file, TypeRefContext::Other, out);
                }
            }
            _ => {}
        }
    }

    match node.kind() {
        // Parameters: `(c: Config)` — type is a `type_annotation` child at field `type`.
        "required_parameter" | "optional_parameter" => {
            if let Some(ann) = node.child_by_field_name("type") {
                // The annotation node may be `type_annotation` wrapping the type,
                // or the type node directly depending on grammar version.
                for child in ann.named_children(&mut ann.walk()) {
                    emit_type_node(&child, bytes, file, TypeRefContext::ParameterType, out);
                }
            }
        }
        // Return types: `function f(): Config`.
        "function_declaration" | "function_signature" | "method_definition" | "arrow_function" => {
            if let Some(ret) = node.child_by_field_name("return_type") {
                for child in ret.named_children(&mut ret.walk()) {
                    emit_type_node(&child, bytes, file, TypeRefContext::ReturnType, out);
                }
            }
            // Recurse into function body to catch nested functions.
            for child in node.children(&mut node.walk()) {
                collect_type_references(&child, bytes, file, out);
            }
            return; // avoid double-recurse at the bottom
        }
        // Field / property types: `field: Type;`
        "public_field_definition" | "property_signature" => {
            if let Some(typ) = node.child_by_field_name("type") {
                for child in typ.named_children(&mut typ.walk()) {
                    emit_type_node(&child, bytes, file, TypeRefContext::Field, out);
                }
            }
        }
        _ => {}
    }

    for child in node.children(&mut node.walk()) {
        collect_type_references(&child, bytes, file, out);
    }
}

/// Node kinds whose `name:` / `function:` child is a declaration binding, not a
/// read. Used by `collect_read_references` to skip declaration-name positions.
const DECL_KINDS_WITH_NAME: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function_signature",
    "class_declaration",
    "method_definition",
    "generator_function_declaration",
    "generator_function",
];

/// Returns `true` when `node` (an `identifier`) is in a position that is already
/// captured by another collector (call callee, declaration name, import binding,
/// parameter pattern, variable declarator name) and must NOT also be emitted as
/// a Read reference.
fn is_non_read_position(node: &Node) -> bool {
    let parent = match node.parent() {
        Some(p) => p,
        None => return true, // root — not a read
    };
    match parent.kind() {
        // Call callee: `helper()` — function field of call_expression.
        "call_expression" => parent.child_by_field_name("function").as_ref() == Some(node),
        // Declaration names (function/class/method/generator).
        kind if DECL_KINDS_WITH_NAME.contains(&kind) => {
            parent.child_by_field_name("name").as_ref() == Some(node)
        }
        // Variable declarator LHS: `const x = …`
        "variable_declarator" => parent.child_by_field_name("name").as_ref() == Some(node),
        // Parameter pattern (the binding name, not the default expression).
        "required_parameter" | "optional_parameter" => {
            parent.child_by_field_name("pattern").as_ref() == Some(node)
        }
        // Import specifier / clause / namespace — already Import refs.
        "import_clause" => true,
        "import_specifier" => true,
        // Shorthand property in an object literal: `{ foo }` — `foo` is both
        // key and value; treat as a read (the value side). The grammar represents
        // this as `shorthand_property_identifier`, not `identifier`, so this arm
        // is defensive only.
        "pair" => {
            // `pair` has key: and value: fields; skip only the key.
            parent.child_by_field_name("key").as_ref() == Some(node)
        }
        // Assignment LHS — handled by collect_write_references.
        "assignment_expression" | "augmented_assignment_expression" => {
            parent.child_by_field_name("left").as_ref() == Some(node)
        }
        _ => false,
    }
}

/// Recursively walk `node` and emit [`RefRole::Read`] references for bare
/// `identifier` nodes used in value/expression positions.
///
/// Skips identifiers that are:
/// - Call callees (already [`RefRole::Call`])
/// - Declaration names (function / class / variable declarator / param pattern)
/// - Import binding names (already [`RefRole::Import`])
/// - Assignment LHS (handled by [`collect_write_references`])
///
/// Applies [`MIN_REF_LEN`] (same threshold as calls).
fn collect_read_references(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    if node.kind() == "identifier" {
        let name = node_text(node, bytes);
        if name.len() >= MIN_REF_LEN && !is_non_read_position(node) {
            push_ref(out, name, node, file, RefRole::Read);
        }
        // identifiers have no meaningful children; return early.
        return;
    }
    for child in node.children(&mut node.walk()) {
        collect_read_references(&child, bytes, file, out);
    }
}

/// Recursively walk `node` and emit [`RefRole::Write`] references for the
/// bare-identifier LHS of `assignment_expression` and
/// `augmented_assignment_expression` nodes (e.g. `x = 5`, `x += 1`).
///
/// Member / subscript LHS (`obj.prop = …`, `arr[i] = …`) are not covered in
/// v1 — only bare identifiers.  Applies [`MIN_REF_LEN`].
fn collect_write_references(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    if matches!(
        node.kind(),
        "assignment_expression" | "augmented_assignment_expression"
    ) {
        if let Some(lhs) = node.child_by_field_name("left") {
            if lhs.kind() == "identifier" {
                let name = node_text(&lhs, bytes);
                if name.len() >= MIN_REF_LEN {
                    push_ref(out, name, &lhs, file, RefRole::Write);
                }
            }
        }
    }
    for child in node.children(&mut node.walk()) {
        collect_write_references(&child, bytes, file, out);
    }
}

// ── Scope tree (Tier-B) ──────────────────────────────────────────────────────

/// ECMAScript function-like node kinds — each opens a `Function` scope.
const FN_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "arrow_function",
    "method_definition",
    "generator_function_declaration",
    "generator_function",
];

/// Build the lexical scope tree for one TS/JS file.
///
/// `scopes[0]` is the file-root `Module` scope. ECMAScript is block-scoped:
/// every function-like node opens a `Function` scope and every standalone
/// `statement_block` (an `if`/`for`/`while` body or a bare block) opens a
/// `Block` scope. A `class` body opens no scope — like Python's LEGB, a method's
/// unqualified name lookup does not see class members.
///
/// Known v1 boundary: `var` is function-scoped but is recorded as a block-scoped
/// local (treated like `let`); hoisting of `var` is not modelled.
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
    if FN_KINDS.contains(&node.kind()) {
        let fn_id = push_scope(
            scopes,
            Some(parent_id),
            node_span(node),
            ScopeKind::Function,
        );
        // Recurse the body. For a brace body, descend into its children directly
        // so the body `statement_block` does not open a redundant nested scope.
        if let Some(body) = node.child_by_field_name("body") {
            if body.kind() == "statement_block" {
                for child in body.children(&mut body.walk()) {
                    scope_dfs(&child, fn_id, scopes);
                }
            } else {
                scope_dfs(&body, fn_id, scopes); // arrow with an expression body
            }
        }
    } else if node.kind() == "statement_block" {
        let block_id = push_scope(scopes, Some(parent_id), node_span(node), ScopeKind::Block);
        for child in node.children(&mut node.walk()) {
            scope_dfs(&child, block_id, scopes);
        }
    } else {
        for child in node.children(&mut node.walk()) {
            scope_dfs(&child, parent_id, scopes);
        }
    }
}

// ── Bindings (Tier-B) ────────────────────────────────────────────────────────

/// Collect parameter and variable [`Binding`]s for one TS/JS file.
///
/// Covers function parameters and `let`/`const`/`var` declarators (each a bare
/// `identifier` name; destructuring patterns are deferred). Top-level definitions
/// and imports are added by the caller from the shared helpers.
fn collect_bindings(root: &Node, bytes: &[u8], scopes: &[Scope]) -> Vec<Binding> {
    let mut out = Vec::new();
    collect_bindings_dfs(root, bytes, scopes, &mut out);
    out
}

fn collect_bindings_dfs(node: &Node, bytes: &[u8], scopes: &[Scope], out: &mut Vec<Binding>) {
    if FN_KINDS.contains(&node.kind()) {
        if let Some(params) = node.child_by_field_name("parameters") {
            collect_params(&params, bytes, scopes, out);
        } else if let Some(p) = node.child_by_field_name("parameter") {
            // single-identifier arrow parameter, e.g. `x => …`
            if p.kind() == "identifier" {
                push_binding(
                    out,
                    node_text(&p, bytes).to_owned(),
                    p.start_byte(),
                    BindingKind::Param,
                    scopes,
                );
            }
        }
        for child in node.children(&mut node.walk()) {
            collect_bindings_dfs(&child, bytes, scopes, out);
        }
    } else if node.kind() == "variable_declarator" {
        // `let`/`const` (lexical_declaration) and `var` (variable_declaration)
        // both nest a `variable_declarator` with a `name` field.
        if let Some(name) = node.child_by_field_name("name") {
            if name.kind() == "identifier" {
                push_binding(
                    out,
                    node_text(&name, bytes).to_owned(),
                    name.start_byte(),
                    BindingKind::Local,
                    scopes,
                );
            }
        }
        for child in node.children(&mut node.walk()) {
            collect_bindings_dfs(&child, bytes, scopes, out);
        }
    } else {
        for child in node.children(&mut node.walk()) {
            collect_bindings_dfs(&child, bytes, scopes, out);
        }
    }
}

/// Emit a [`BindingKind::Param`] for each parameter in a `formal_parameters`
/// node, unwrapping the typed `required_parameter`/`optional_parameter` forms.
fn collect_params(params: &Node, bytes: &[u8], scopes: &[Scope], out: &mut Vec<Binding>) {
    for child in params.named_children(&mut params.walk()) {
        let ident = match child.kind() {
            "identifier" => Some(child),
            "required_parameter" | "optional_parameter" => child.child_by_field_name("pattern"),
            _ => None,
        };
        if let Some(id) = ident {
            if id.kind() == "identifier" {
                push_binding(
                    out,
                    node_text(&id, bytes).to_owned(),
                    id.start_byte(),
                    BindingKind::Param,
                    scopes,
                );
            }
        }
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
            "codegraph . . . src/auth/jwt/validateToken()."
        );
        assert_eq!(vt.kind, SymbolKind::Function);
        assert_eq!(vt.visibility, Visibility::Public);

        let cfg = by_name("Config").unwrap();
        assert_eq!(cfg.kind, SymbolKind::Class);
        assert_eq!(cfg.visibility, Visibility::Public);

        let opts = by_name("Options").unwrap();
        assert_eq!(opts.kind, SymbolKind::Interface);
        assert_eq!(opts.visibility, Visibility::Public);

        let max = by_name("MAX").unwrap();
        assert_eq!(max.kind, SymbolKind::Const);
        assert_eq!(max.visibility, Visibility::Public);

        // Non-exported declarations are now emitted with Visibility::Private.
        let internal = by_name("internal").expect("internal must now be emitted as Private");
        assert_eq!(internal.kind, SymbolKind::Function);
        assert_eq!(internal.visibility, Visibility::Private);
    }

    #[test]
    fn bare_decl_visibility_private() {
        // Bare (non-exported) top-level declarations → Visibility::Private.
        let src = "\
function g() {}
const X = 1;
";
        let facts = TypeScriptExtractor.extract(src, "src/mod.ts").unwrap();
        let by_name = |n: &str| facts.symbols.iter().find(|s| s.name == n).cloned();

        let g = by_name("g").expect("bare function g must be emitted");
        assert_eq!(g.kind, SymbolKind::Function);
        assert_eq!(g.visibility, Visibility::Private);

        let x = by_name("X").expect("bare const X must be emitted");
        assert_eq!(x.kind, SymbolKind::Const);
        assert_eq!(x.visibility, Visibility::Private);
    }

    #[test]
    fn exported_decl_visibility_public() {
        // Exported declarations → Visibility::Public.
        let src = "\
export function f() {}
export const Y = 2;
";
        let facts = TypeScriptExtractor.extract(src, "src/mod.ts").unwrap();
        let by_name = |n: &str| facts.symbols.iter().find(|s| s.name == n).cloned();

        let f = by_name("f").expect("exported function f must be emitted");
        assert_eq!(f.kind, SymbolKind::Function);
        assert_eq!(f.visibility, Visibility::Public);

        let y = by_name("Y").expect("exported const Y must be emitted");
        assert_eq!(y.kind, SymbolKind::Const);
        assert_eq!(y.visibility, Visibility::Public);
    }

    #[test]
    fn default_export_function_is_named() {
        let facts = TypeScriptExtractor
            .extract("export default function App() {}", "src/App.tsx")
            .unwrap();
        // 1 declared symbol + 1 module symbol
        assert_eq!(facts.symbols.len(), 2);
        let app = facts.symbols.iter().find(|s| s.name == "App").unwrap();
        assert_eq!(app.id.to_scip_string(), "codegraph . . . src/App/App().");
    }

    #[test]
    fn emits_function_block_scopes_and_bindings() {
        let src = "export function run(arg: number) {\n  const local = 1;\n  if (arg) { helper(local); }\n}\n";
        let facts = TypeScriptExtractor.extract(src, "src/main.ts").unwrap();
        assert!(
            facts.scopes.iter().any(|s| s.kind == ScopeKind::Function),
            "expected a Function scope"
        );
        assert!(
            facts.scopes.iter().any(|s| s.kind == ScopeKind::Block),
            "expected a Block scope (the if body)"
        );
        let has = |name: &str, kind: BindingKind| {
            facts
                .bindings
                .iter()
                .any(|b| b.name == name && b.kind == kind)
        };
        assert!(has("arg", BindingKind::Param), "param binding missing");
        assert!(
            has("local", BindingKind::Local),
            "const local binding missing"
        );
        assert!(has("run", BindingKind::Definition), "def binding missing");
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

    // ── Inheritance tests ────────────────────────────────────────────────────

    #[test]
    fn ts_class_extends_and_implements() {
        let src = "class Sub extends Base implements Iface {}";
        let facts = TypeScriptExtractor.extract(src, "src/sub.ts").unwrap();
        let inherit_names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::IsImplementation)
            .map(|r| r.name.as_str())
            .collect();
        assert!(
            inherit_names.contains(&"Base"),
            "expected 'Base' in {inherit_names:?}"
        );
        assert!(
            inherit_names.contains(&"Iface"),
            "expected 'Iface' in {inherit_names:?}"
        );
    }

    #[test]
    fn ts_interface_extends_multiple() {
        let src = "interface I extends A, B {}";
        let facts = TypeScriptExtractor.extract(src, "src/i.ts").unwrap();
        let inherit_names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::IsImplementation)
            .map(|r| r.name.as_str())
            .collect();
        assert!(
            inherit_names.contains(&"A"),
            "expected 'A' in {inherit_names:?}"
        );
        assert!(
            inherit_names.contains(&"B"),
            "expected 'B' in {inherit_names:?}"
        );
    }

    #[test]
    fn ts_class_extends_qualified_name() {
        let src = "class C extends ns.Base {}";
        let facts = TypeScriptExtractor.extract(src, "src/c.ts").unwrap();
        let inherit_names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::IsImplementation)
            .map(|r| r.name.as_str())
            .collect();
        assert!(
            inherit_names.contains(&"Base"),
            "expected leaf 'Base' from 'ns.Base' in {inherit_names:?}"
        );
    }

    #[test]
    fn js_class_extends_base() {
        // JavaScript routes through the same extract_ecmascript core; verify
        // that inheritance edges are emitted for .js files too.
        use crate::extract::Extractor as _;
        use crate::extract::JavaScriptExtractor;
        let src = "class Sub extends Base {}";
        let facts = JavaScriptExtractor.extract(src, "src/sub.js").unwrap();
        let inherit_names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::IsImplementation)
            .map(|r| r.name.as_str())
            .collect();
        assert!(
            inherit_names.contains(&"Base"),
            "expected 'Base' in JS inherit refs: {inherit_names:?}"
        );
    }

    // ── Import reference tests ───────────────────────────────────────────────

    #[test]
    fn ts_named_import_emits_import_ref() {
        // `import { Service } from "./svc";` → one Import ref `Service`
        let src = r#"import { Service } from "./svc";"#;
        let facts = TypeScriptExtractor.extract(src, "src/client.ts").unwrap();
        let import_names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(
            import_names,
            vec!["Service"],
            "expected exactly [Service], got {import_names:?}"
        );
    }

    #[test]
    fn ts_default_import_emits_import_ref() {
        // `import Foo from "./foo";` → Import ref `Foo`
        let src = r#"import Foo from "./foo";"#;
        let facts = TypeScriptExtractor.extract(src, "src/use.ts").unwrap();
        let import_names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| r.name.as_str())
            .collect();
        assert!(
            import_names.contains(&"Foo"),
            "expected 'Foo' in import refs: {import_names:?}"
        );
    }

    #[test]
    fn ts_named_import_with_alias_emits_real_name() {
        // `import { A, B as C } from "x";` → Import refs `A` and `B` (not alias `C`)
        let src = r#"import { A, B as C } from "x";"#;
        let facts = TypeScriptExtractor.extract(src, "src/aliases.ts").unwrap();
        let import_names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| r.name.as_str())
            .collect();
        assert!(
            import_names.contains(&"A"),
            "expected 'A' in import refs: {import_names:?}"
        );
        assert!(
            import_names.contains(&"B"),
            "expected 'B' (real name) in import refs: {import_names:?}"
        );
        assert!(
            !import_names.contains(&"C"),
            "alias 'C' must NOT appear in import refs: {import_names:?}"
        );
    }

    #[test]
    fn ts_namespace_import_emits_no_import_refs() {
        // `import * as ns from "x";` → NO Import refs
        let src = r#"import * as ns from "x";"#;
        let facts = TypeScriptExtractor.extract(src, "src/ns.ts").unwrap();
        let import_refs: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| r.name.as_str())
            .collect();
        assert!(
            import_refs.is_empty(),
            "namespace import must produce no Import refs, got {import_refs:?}"
        );
    }

    #[test]
    fn js_named_import_emits_import_ref() {
        // JavaScript (.js) through the shared extract_ecmascript core.
        // `import { thing } from "./m";` → Import ref `thing`
        use crate::extract::Extractor as _;
        use crate::extract::JavaScriptExtractor;
        let src = r#"import { thing } from "./m";"#;
        let facts = JavaScriptExtractor.extract(src, "src/consumer.js").unwrap();
        let import_names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| r.name.as_str())
            .collect();
        assert!(
            import_names.contains(&"thing"),
            "expected 'thing' in JS import refs: {import_names:?}"
        );
    }

    #[test]
    fn ts_import_refs_carry_source_module() {
        // `import { Service } from "./svc";` in src/auth/client.ts → all
        // Import refs carry the SCIP module id of src/auth/client.
        let src = r#"import { Service } from "./svc";"#;
        let file = "src/auth/client.ts";
        let facts = TypeScriptExtractor.extract(src, file).unwrap();

        let namespaces = module_namespaces(file);
        let expected_module_id =
            crate::extract::module_symbol(Language::TypeScript, &namespaces, file, src.len())
                .id
                .to_scip_string();

        let import_refs: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .collect();
        assert!(!import_refs.is_empty(), "expected at least one Import ref");
        for r in &import_refs {
            assert_eq!(
                r.source_module,
                Some(expected_module_id.clone()),
                "Import ref '{}' should carry source_module = {:?}",
                r.name,
                expected_module_id
            );
        }
    }

    // --- from_path tests ---

    #[test]
    fn ts_named_import_carries_from_path() {
        // `import { Service } from "./svc";` → from_path == "./svc" (quotes stripped)
        let src = r#"import { Service } from "./svc";"#;
        let facts = TypeScriptExtractor.extract(src, "src/client.ts").unwrap();
        let r = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Import && r.name == "Service")
            .expect("expected Import ref for 'Service'");
        assert_eq!(
            r.from_path,
            Some("./svc".to_owned()),
            "from_path should be './svc', got {:?}",
            r.from_path
        );
    }

    // ── Edge richness: TypeRef / Read / Write ────────────────────────────────

    #[test]
    fn ts_param_type_ref_emitted() {
        // `function f(c: Config) {}` → TypeRef "Config" with ParameterType ctx.
        let src = "function f(c: Config) {}";
        let facts = TypeScriptExtractor.extract(src, "src/main.ts").unwrap();
        let r = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::TypeRef && r.name == "Config")
            .expect("expected TypeRef ref for 'Config'");
        assert_eq!(
            r.type_ref_ctx,
            Some(TypeRefContext::ParameterType),
            "expected ParameterType ctx, got {:?}",
            r.type_ref_ctx
        );
    }

    #[test]
    fn ts_return_type_ref_emitted() {
        // `function f(): Config { return null; }` → TypeRef "Config" with ReturnType ctx.
        let src = "function f(): Config { return null; }";
        let facts = TypeScriptExtractor.extract(src, "src/main.ts").unwrap();
        let r = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::TypeRef && r.name == "Config")
            .expect("expected TypeRef ref for 'Config'");
        assert_eq!(
            r.type_ref_ctx,
            Some(TypeRefContext::ReturnType),
            "expected ReturnType ctx, got {:?}",
            r.type_ref_ctx
        );
    }

    #[test]
    fn ts_read_ref_emitted_for_use_not_declaration() {
        // `function f() { const base = 1; return base; }`
        // → Read ref for the `base` in `return base`; the declarator name must NOT be a Read.
        let src = "function f() { const base = 1; return base; }";
        let facts = TypeScriptExtractor.extract(src, "src/main.ts").unwrap();
        let read_refs: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Read && r.name == "base")
            .collect();
        // There must be at least one Read ref (the use in `return base`).
        assert!(
            !read_refs.is_empty(),
            "expected at least one Read ref for 'base', got none"
        );
        // The declaration `const base = 1` starts before the `return` statement.
        // Verify that at least one Read ref byte offset is AFTER the `=` (i.e. not the decl).
        // In `function f() { const base = 1; return base; }` the return keyword starts at ~35.
        let use_ref = read_refs
            .iter()
            .find(|r| r.occ.byte > 20)
            .expect("expected Read ref for 'base' in the return statement (byte > 20)");
        assert!(
            use_ref.occ.byte > 20,
            "Read ref should be at the use site, not the declaration"
        );
    }

    #[test]
    fn ts_write_ref_emitted_for_assignment() {
        // `function f() { let xxx = 0; xxx = 5; }` → Write ref for `xxx = 5`.
        let src = "function f() { let xxx = 0; xxx = 5; }";
        let facts = TypeScriptExtractor.extract(src, "src/main.ts").unwrap();
        let write_refs: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Write && r.name == "xxx")
            .collect();
        assert!(
            !write_refs.is_empty(),
            "expected at least one Write ref for 'xxx', got none — all refs: {:?}",
            facts
                .references
                .iter()
                .map(|r| (&r.name, r.role))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn ts_call_not_also_read() {
        // `helper()` → a Call ref for "helper", but NOT also a Read ref.
        let src = "function run() { helper(); }";
        let facts = TypeScriptExtractor.extract(src, "src/main.ts").unwrap();
        let call_refs: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Call && r.name == "helper")
            .collect();
        assert!(!call_refs.is_empty(), "expected a Call ref for 'helper'");
        let read_refs: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Read && r.name == "helper")
            .collect();
        assert!(
            read_refs.is_empty(),
            "helper() must NOT produce a Read ref; got: {read_refs:?}"
        );
    }

    #[test]
    fn ts_property_access_not_a_read_of_property() {
        // `obj.foo` → no Read ref named "foo" (property_identifier, not identifier).
        // A Read ref for `obj` is acceptable.
        let src = "function run() { return obj.foo; }";
        let facts = TypeScriptExtractor.extract(src, "src/main.ts").unwrap();
        let foo_reads: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Read && r.name == "foo")
            .collect();
        assert!(
            foo_reads.is_empty(),
            "property 'foo' in member_expression must NOT be a Read ref; got: {foo_reads:?}"
        );
    }

    // ── Entry-point detection: call-terminal verb matching ───────────────────

    #[test]
    fn ts_named_handler_router_get_entry_point() {
        // router.get('/users', getUsers) next to `function getUsers() {}` →
        // getUsers's entry_points contains HttpRoute("router.get").
        let src = "router.get('/users', getUsers);\nfunction getUsers() {}\n";
        let facts = TypeScriptExtractor.extract(src, "src/routes.ts").unwrap();
        let get_users = facts
            .symbols
            .iter()
            .find(|s| s.name == "getUsers")
            .expect("expected 'getUsers' symbol");
        assert!(
            get_users
                .entry_points
                .iter()
                .any(|ep| matches!(ep, EntryPoint::HttpRoute(m) if m == "router.get")),
            "expected HttpRoute(\"router.get\"), got {:?}",
            get_users.entry_points
        );
    }

    #[test]
    fn ts_named_handler_app_post_exported_entry_point() {
        // app.post('/users', createUser) next to `export function createUser() {}`
        // → createUser's entry_points contains HttpRoute("app.post").
        let src = "app.post('/users', createUser);\nexport function createUser() {}\n";
        let facts = TypeScriptExtractor.extract(src, "src/routes.ts").unwrap();
        let create_user = facts
            .symbols
            .iter()
            .find(|s| s.name == "createUser")
            .expect("expected 'createUser' symbol");
        assert!(
            create_user
                .entry_points
                .iter()
                .any(|ep| matches!(ep, EntryPoint::HttpRoute(m) if m == "app.post")),
            "expected HttpRoute(\"app.post\"), got {:?}",
            create_user.entry_points
        );
    }

    #[test]
    fn ts_inline_arrow_handler_produces_no_marker() {
        // router.get('/users', (req, res) => {}) — inline arrow handler, no
        // named function anywhere → NO symbol in the file gets an HttpRoute marker.
        let src = "router.get('/users', (req, res) => {});\n";
        let facts = TypeScriptExtractor.extract(src, "src/routes.ts").unwrap();
        assert!(
            facts.symbols.iter().all(|s| s.entry_points.is_empty()),
            "inline handler must not produce any HttpRoute marker; symbols: {:?}",
            facts
                .symbols
                .iter()
                .map(|s| (&s.name, &s.entry_points))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn ts_use_middleware_registration_produces_no_marker() {
        // app.use(logger) next to `function logger() {}` → logger's entry_points
        // stays empty — `.use()` is deliberately excluded.
        let src = "app.use(logger);\nfunction logger() {}\n";
        let facts = TypeScriptExtractor.extract(src, "src/routes.ts").unwrap();
        let logger = facts
            .symbols
            .iter()
            .find(|s| s.name == "logger")
            .expect("expected 'logger' symbol");
        assert!(
            logger.entry_points.is_empty(),
            "`.use()` must not produce an HttpRoute marker; got {:?}",
            logger.entry_points
        );
    }

    #[test]
    fn js_named_handler_router_get_entry_point() {
        // JS inherits it for free via the shared extract_ecmascript path.
        use crate::extract::Extractor as _;
        use crate::extract::JavaScriptExtractor;
        let src = "router.get('/users', getUsers);\nfunction getUsers() {}\n";
        let facts = JavaScriptExtractor.extract(src, "src/routes.js").unwrap();
        let get_users = facts
            .symbols
            .iter()
            .find(|s| s.name == "getUsers")
            .expect("expected 'getUsers' symbol");
        assert!(
            get_users
                .entry_points
                .iter()
                .any(|ep| matches!(ep, EntryPoint::HttpRoute(m) if m == "router.get")),
            "expected HttpRoute(\"router.get\") on JS too, got {:?}",
            get_users.entry_points
        );
    }

    // ── Entry-point detection: NestJS decorator matching ─────────────────────

    #[test]
    fn ts_controller_class_decorator_entry_point() {
        // @Controller('users') export class UsersController { findOne() {} } →
        // the UsersController class Symbol.entry_points contains HttpRoute("Controller").
        let src = "@Controller('users')\nexport class UsersController {\n  findOne() {}\n}\n";
        let facts = TypeScriptExtractor.extract(src, "src/users.ts").unwrap();
        let ctrl = facts
            .symbols
            .iter()
            .find(|s| s.name == "UsersController")
            .expect("expected 'UsersController' symbol");
        assert!(
            ctrl.entry_points
                .iter()
                .any(|ep| matches!(ep, EntryPoint::HttpRoute(m) if m == "Controller")),
            "expected HttpRoute(\"Controller\"), got {:?}",
            ctrl.entry_points
        );
    }

    #[test]
    fn ts_method_level_get_decorator_aggregates_onto_class() {
        // export class UsersController { @Get(':id') findOne() {} } (method-level
        // only, no class-level decorator) → the class Symbol.entry_points
        // contains HttpRoute("Get").
        let src = "export class UsersController {\n  @Get(':id') findOne() {}\n}\n";
        let facts = TypeScriptExtractor.extract(src, "src/users.ts").unwrap();
        let ctrl = facts
            .symbols
            .iter()
            .find(|s| s.name == "UsersController")
            .expect("expected 'UsersController' symbol");
        assert!(
            ctrl.entry_points
                .iter()
                .any(|ep| matches!(ep, EntryPoint::HttpRoute(m) if m == "Get")),
            "expected HttpRoute(\"Get\") aggregated onto the class, got {:?}",
            ctrl.entry_points
        );
    }

    #[test]
    fn ts_class_and_method_decorators_all_aggregate_onto_class() {
        // @Controller('users') export class UsersController { @Get(':id') findOne()
        // {} @Post() create() {} } → the class Symbol.entry_points contains all
        // three: Controller, Get, Post. Exactly 2 symbols total (class + module),
        // NOT 4 — no per-method Symbol is fabricated.
        let src = "@Controller('users')\nexport class UsersController {\n  @Get(':id') findOne() {}\n  @Post() create() {}\n}\n";
        let facts = TypeScriptExtractor.extract(src, "src/users.ts").unwrap();
        assert_eq!(
            facts.symbols.len(),
            2,
            "expected exactly 2 symbols (class + module), got {:?}",
            facts.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        let ctrl = facts
            .symbols
            .iter()
            .find(|s| s.name == "UsersController")
            .expect("expected 'UsersController' symbol");
        for expected in ["Controller", "Get", "Post"] {
            assert!(
                ctrl.entry_points
                    .iter()
                    .any(|ep| matches!(ep, EntryPoint::HttpRoute(m) if m == expected)),
                "expected HttpRoute({expected:?}) in {:?}",
                ctrl.entry_points
            );
        }
    }

    #[test]
    fn ts_non_route_decorator_produces_no_marker() {
        // @Injectable() on a class produces no HttpRoute marker.
        let src = "@Injectable()\nexport class UsersService {}\n";
        let facts = TypeScriptExtractor
            .extract(src, "src/users.service.ts")
            .unwrap();
        let svc = facts
            .symbols
            .iter()
            .find(|s| s.name == "UsersService")
            .expect("expected 'UsersService' symbol");
        assert!(
            svc.entry_points.is_empty(),
            "@Injectable() must not produce an HttpRoute marker; got {:?}",
            svc.entry_points
        );
    }
}
