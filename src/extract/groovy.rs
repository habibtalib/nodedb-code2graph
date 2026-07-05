// SPDX-License-Identifier: Apache-2.0

//! Groovy extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: top-level type declarations (`class`, `interface`, `enum`) and
//! their members (methods, constructors, fields), plus script-level `def`
//! functions (`function_definition` — the common shape in scripts and
//! `.gradle` files). Qualified identity follows the `package` declaration;
//! files without one fall back to a path-derived namespace (same algorithm as
//! the Java extractor). References: parenthesized calls (`method_invocation`,
//! with the receiver captured as the reference's qualifier), paren-less
//! command calls (`juxt_function_call` — a distinct, unambiguous node kind in
//! this grammar), constructor calls, imports (incl. static and star forms),
//! `extends`/`implements` inheritance, type uses, and variable reads/writes.
//!
//! Honest ceilings (documented, never guessed past):
//! - `.gradle` files are parsed as **plain Groovy** (locked decision D-02):
//!   closures and method calls come out as ordinary facts, with no Gradle-DSL
//!   semantic modeling (no dependency-coordinate interpretation, no task-graph
//!   semantics).
//! - The `trait` keyword is **not implemented by tree-sitter-groovy 0.1.2**:
//!   `trait Greeter { … }` parses as an unrelated paren-less call plus a
//!   detached closure — there is no `trait_declaration` node to walk. A
//!   grammar-version ceiling, not an extraction gap.
//! - Paren-less command calls are extracted **only where the AST is
//!   unambiguous** — when the grammar yields a `juxt_function_call` node
//!   (e.g. a literal argument: `println "hi"`). A bare-identifier argument
//!   (`println env`) parses as a typed local-variable *declaration*
//!   (`type: println`, `name: env` — the grammar's C-style ambiguity
//!   resolution), so it is deliberately not guessed to be a call.
//! - `methodMissing`/`invokeMethod` dynamic dispatch is unresolved — call
//!   references are emitted for the written name only, never guessed.
//! - Unmarked members map to [`Visibility::Public`] (Groovy's implicit-public
//!   rule — the opposite fallback of Java's package-private `Internal`;
//!   empirically the `modifiers` node is simply absent when no keyword is
//!   written). Explicit `private`/`protected`/`public` are read as written.
//! - Idiomatic newline-terminated Groovy often parses with a recoverable
//!   `MISSING ";"` token (so `root.has_error()` is `true` for the file) —
//!   definitions and most references are still extracted; `has_error()` is
//!   not a signal that extraction failed. Statements *immediately after* a
//!   repaired line can lose call facts, though (e.g. a paren-less call right
//!   after an unterminated statement splits into detached expression
//!   statements) — a grammar-recovery ceiling, not something to guess past.
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.

use tree_sitter::{Node, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{
    Binding, BindingKind, ByteSpan, EntryPoint, FileFacts, RefRole, Reference, Scope, ScopeId,
    ScopeKind, Symbol, SymbolKind, TypeRefContext, Visibility,
};
use crate::lang::Language;
use crate::symbol::Descriptor;

use super::{
    ExtractCtx, Extractor, MIN_REF_LEN, attach_reference_scopes, collect_call_references,
    definition_bindings, field_text, import_bindings, make_symbol, node_span, node_text,
    one_line_signature, push_binding, push_ref, push_scope, push_type_ref,
};

/// Tree-sitter query capturing call-callee identifiers.
///
/// The two `method_invocation` branches are disjoint (`object:` present vs
/// `!object` absent), so no invocation matches twice; the receiver — whatever
/// its node kind (`identifier`, `this`, `field_access`, a chained call, …) —
/// is captured as `@qualifier` exactly as written. `juxt_function_call` is the
/// grammar's dedicated node for paren-less command calls (`println "hi"`), so
/// capturing it is unambiguous by construction.
const CALL_QUERY: &str = r#"
[
  (method_invocation object: (_) @qualifier name: (identifier) @callee)
  (method_invocation !object name: (identifier) @callee)
  (juxt_function_call name: (identifier) @callee)
  (object_creation_expression type: (type_identifier) @callee)
]
"#;

/// Extracts Groovy symbols and references.
pub struct GroovyExtractor;

impl Extractor for GroovyExtractor {
    fn lang(&self) -> Language {
        Language::Groovy
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_language = crate::grammar::groovy();
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
        let namespaces = groovy_namespaces(&root, bytes, file);

        let ctx = ExtractCtx {
            bytes,
            file,
            lang: Language::Groovy,
        };
        let defs = collect_symbols(&root, &ctx, &namespaces);
        let def_bindings = definition_bindings(&defs);
        let mut symbols = defs;
        let mod_sym = super::module_symbol(Language::Groovy, &namespaces, file, source.len());
        let module_id = mod_sym.id.to_scip_string();
        symbols.push(mod_sym);

        let mut references = collect_call_references(
            &root,
            &ts_language,
            CALL_QUERY,
            Language::Groovy,
            bytes,
            file,
        )?;
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
            lang: Language::Groovy.as_str().to_owned(),
            symbols,
            references,
            scopes,
            bindings,
            ffi_exports: Vec::new(),
        })
    }
}

/// Derive namespace descriptors from the `package` declaration, falling back to
/// path-derived segments when no `package` statement is present.
///
/// With a package: `com.example.zoo` → `["com", "example", "zoo"]`.
/// Without: `src/com/example/App.groovy` → `["com", "example", "App"]` (same
/// algorithm as the Java extractor; `.gradle` is stripped like `.groovy`, so
/// `build.gradle` → `["build"]`).
fn groovy_namespaces(root: &Node, bytes: &[u8], file: &str) -> Vec<String> {
    for child in root.children(&mut root.walk()) {
        if child.kind() != "package_declaration" {
            continue;
        }
        // The package name is a direct child: either `scoped_identifier`
        // (`com.example.zoo`) or a bare `identifier` (`zoo`).
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

    // Fallback: derive from the file path (strips `.groovy`/`.gradle`, strips
    // a leading `src/`).
    let p = file
        .strip_suffix(".groovy")
        .or_else(|| file.strip_suffix(".gradle"))
        .unwrap_or(file);
    let p = p.strip_prefix("src/").unwrap_or(p);
    p.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

// ── Definitions ──────────────────────────────────────────────────────────────

fn collect_symbols(root: &Node, ctx: &ExtractCtx, namespaces: &[String]) -> Vec<Symbol> {
    let mut out = Vec::new();

    for child in root.children(&mut root.walk()) {
        match child.kind() {
            "class_declaration" | "interface_declaration" | "enum_declaration" => {
                collect_type(&child, ctx, namespaces, &mut out);
            }
            // Script-level `def name(...) { ... }` — a free function.
            "function_definition" => {
                let Some(name) = field_text(&child, "name", ctx.bytes) else {
                    continue;
                };
                let mut descriptors: Vec<Descriptor> = namespaces
                    .iter()
                    .cloned()
                    .map(Descriptor::Namespace)
                    .collect();
                descriptors.push(Descriptor::Method {
                    name: name.clone(),
                    disambiguator: String::new(),
                });
                out.push(make_symbol(
                    ctx,
                    &child,
                    name,
                    SymbolKind::Function,
                    Visibility::Public,
                    descriptors,
                    one_line_signature(node_text(&child, ctx.bytes), &['{', ';']),
                ));
            }
            _ => {}
        }
    }

    out
}

/// Emit a symbol for one top-level type declaration plus its members.
fn collect_type(node: &Node, ctx: &ExtractCtx, namespaces: &[String], out: &mut Vec<Symbol>) {
    let Some(type_name) = field_text(node, "name", ctx.bytes) else {
        return;
    };

    let type_sym_kind = match node.kind() {
        "interface_declaration" => SymbolKind::Interface,
        "enum_declaration" => SymbolKind::Enum,
        _ => SymbolKind::Class,
    };

    let mut type_descriptors: Vec<Descriptor> = namespaces
        .iter()
        .cloned()
        .map(Descriptor::Namespace)
        .collect();
    type_descriptors.push(Descriptor::Type(type_name.clone()));
    out.push(make_symbol(
        ctx,
        node,
        type_name.clone(),
        type_sym_kind,
        read_visibility(node, ctx.bytes),
        type_descriptors,
        one_line_signature(node_text(node, ctx.bytes), &['{', ';']),
    ));

    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    collect_members(&body, ctx, namespaces, &type_name, out);
}

/// Collect method, constructor, and field declarations from a type body node
/// (`class_body`, `interface_body`, or `enum_body`; enum methods/fields live
/// one level deeper inside `enum_body_declarations`).
fn collect_members(
    body: &Node,
    ctx: &ExtractCtx,
    namespaces: &[String],
    type_name: &str,
    out: &mut Vec<Symbol>,
) {
    for member in body.children(&mut body.walk()) {
        match member.kind() {
            "enum_body_declarations" => {
                collect_members(&member, ctx, namespaces, type_name, out);
            }
            "method_declaration" | "constructor_declaration" => {
                let Some(name) = field_text(&member, "name", ctx.bytes) else {
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
                let mut method_sym = make_symbol(
                    ctx,
                    &member,
                    name.clone(),
                    SymbolKind::Method,
                    read_visibility(&member, ctx.bytes),
                    descriptors,
                    one_line_signature(node_text(&member, ctx.bytes), &['{', ';']),
                );
                // `static void main(String[] args)` — Groovy's JVM entry point,
                // same detection as the Java template.
                if name == "main" && has_modifier(&member, ctx.bytes, "static") {
                    method_sym.entry_points = vec![EntryPoint::Main];
                }
                out.push(method_sym);
            }
            "field_declaration" => {
                // A single field_declaration may declare multiple variables.
                let field_vis = read_visibility(&member, ctx.bytes);
                let mut cursor = member.walk();
                for declarator in member.children_by_field_name("declarator", &mut cursor) {
                    let Some(var_name) = field_text(&declarator, "name", ctx.bytes) else {
                        continue;
                    };
                    let mut descriptors: Vec<Descriptor> = namespaces
                        .iter()
                        .cloned()
                        .map(Descriptor::Namespace)
                        .collect();
                    descriptors.push(Descriptor::Type(type_name.to_owned()));
                    descriptors.push(Descriptor::Term(var_name.clone()));
                    out.push(make_symbol(
                        ctx,
                        &member,
                        var_name,
                        SymbolKind::Static,
                        field_vis,
                        descriptors,
                        one_line_signature(node_text(&member, ctx.bytes), &['{', ';']),
                    ));
                }
            }
            _ => {}
        }
    }
}

/// True iff `node` has a `modifiers` child containing the modifier `keyword`.
fn has_modifier(node: &Node, bytes: &[u8], keyword: &str) -> bool {
    node.children(&mut node.walk())
        .find(|c| c.kind() == "modifiers")
        .is_some_and(|mods| {
            mods.children(&mut mods.walk())
                .any(|m| node_text(&m, bytes) == keyword)
        })
}

/// Read the declared visibility of `node` from its `modifiers` child.
///
/// - `private` → [`Visibility::Private`]
/// - `protected` → [`Visibility::Protected`]
/// - `public` or no modifier at all → [`Visibility::Public`] — Groovy's
///   unmarked members are implicitly public (the `modifiers` node is absent
///   entirely when no keyword is written), so the fallback is the opposite of
///   Java's package-private `Internal`.
fn read_visibility(node: &Node, bytes: &[u8]) -> Visibility {
    if has_modifier(node, bytes, "private") {
        return Visibility::Private;
    }
    if has_modifier(node, bytes, "protected") {
        return Visibility::Protected;
    }
    Visibility::Public
}

// ── Inheritance ──────────────────────────────────────────────────────────────

/// Recursively walk `node` collecting `IsImplementation` references for every
/// `class_declaration` (`extends` + `implements`) and `interface_declaration`
/// (`extends`) in the tree, including nested classes.
fn collect_inheritance(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    match node.kind() {
        "class_declaration" => {
            // superclass: the field's `superclass` node wraps the `extends`
            // keyword + the type node — take the first named child.
            if let Some(superclass_node) = node.child_by_field_name("superclass") {
                if let Some(type_node) = superclass_node
                    .children(&mut superclass_node.walk())
                    .find(|c| c.is_named())
                {
                    push_ref(
                        out,
                        super::simple_type_name(node_text(&type_node, bytes), "."),
                        &type_node,
                        file,
                        RefRole::IsImplementation,
                    );
                }
            }
            // interfaces: field `interfaces` → `super_interfaces` → `type_list`.
            if let Some(ifaces_node) = node.child_by_field_name("interfaces") {
                push_type_list_refs(&ifaces_node, bytes, file, out);
            }
        }
        "interface_declaration" => {
            // `extends_interfaces` is a positional child (not a field).
            if let Some(extends_node) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "extends_interfaces")
            {
                push_type_list_refs(&extends_node, bytes, file, out);
            }
        }
        _ => {}
    }

    for child in node.children(&mut node.walk()) {
        collect_inheritance(&child, bytes, file, out);
    }
}

/// Descend a `super_interfaces` / `extends_interfaces` node to find
/// `type_list` and push one `IsImplementation` reference per named type child.
fn push_type_list_refs(container: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    let Some(type_list) = container
        .children(&mut container.walk())
        .find(|c| c.kind() == "type_list")
    else {
        return;
    };
    for type_node in type_list.children(&mut type_list.walk()) {
        if type_node.is_named() {
            push_ref(
                out,
                super::simple_type_name(node_text(&type_node, bytes), "."),
                &type_node,
                file,
                RefRole::IsImplementation,
            );
        }
    }
}

// ── Imports ──────────────────────────────────────────────────────────────────

/// Recursively walk `node` collecting `Import` references for every
/// `import_declaration` in the tree.
///
/// - Wildcard imports (`import com.x.*`) are skipped entirely (the `asterisk`
///   node is a sibling of the `scoped_identifier`).
/// - Named imports (`import com.example.Service`) emit a single ref whose name
///   is the leaf identifier, positioned at that leaf, with `from_path` set to
///   the package prefix.
/// - Static imports (`import static com.x.Util.helper`) parse to the same
///   `scoped_identifier` shape and are treated identically — the leaf name is
///   what matters.
/// - Bare identifier imports (`import Foo`) use the identifier text directly.
fn collect_imports(
    node: &Node,
    bytes: &[u8],
    file: &str,
    out: &mut Vec<Reference>,
    module_id: &str,
) {
    if node.kind() == "import_declaration" {
        // Scan all children: the wildcard `asterisk` appears *after* the
        // `scoped_identifier` sibling, so an early exit would miss it.
        let mut scoped: Option<Node> = None;
        let mut bare: Option<Node> = None;
        let mut has_wildcard = false;
        for child in node.children(&mut node.walk()) {
            match child.kind() {
                "asterisk" => has_wildcard = true,
                "scoped_identifier" if scoped.is_none() => scoped = Some(child),
                "identifier" if bare.is_none() => bare = Some(child),
                _ => {}
            }
        }

        if !has_wildcard {
            if let Some(child) = scoped {
                // The `name` field is the final leaf; `scope` is the prefix.
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, bytes);
                    let from_path = child
                        .child_by_field_name("scope")
                        .map_or("", |n| node_text(&n, bytes));
                    super::push_import_ref(out, name, &name_node, file, module_id, from_path);
                }
            } else if let Some(child) = bare {
                let name = node_text(&child, bytes);
                super::push_import_ref(out, name, &child, file, module_id, "");
            }
        }
        // import_declaration children are identifiers, never nested imports.
        return;
    }

    for child in node.children(&mut node.walk()) {
        collect_imports(&child, bytes, file, out, module_id);
    }
}

// ── Edge richness: TypeRef ───────────────────────────────────────────────────

/// Recursively walk `node` emitting [`RefRole::TypeRef`] references for every
/// user-defined type in a typed annotation position.
///
/// Covered positions (tree-sitter-groovy grammar, Java-shaped):
/// - `formal_parameter` → `type:` field → `ParameterType`
/// - `method_declaration` → `type:` field (the return type) → `ReturnType`
/// - `field_declaration` → `type:` field → `Field`
/// - Generic type arguments inside `type_arguments` → `GenericArg`
///
/// The `def` keyword parses as a `type_identifier` with text `"def"` — it is a
/// dynamic-typing marker, not a user type, and is skipped. Primitive kinds
/// (`integral_type`, …) and `void_type` are skipped; `array_type` unwraps to
/// its `element:` type.
fn collect_type_references(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    fn type_leaf(
        node: &Node,
        bytes: &[u8],
        file: &str,
        ctx: TypeRefContext,
        out: &mut Vec<Reference>,
    ) {
        match node.kind() {
            "type_identifier" => {
                let name = node_text(node, bytes);
                // `def` is Groovy's dynamic-typing keyword, not a type use.
                if name != "def" {
                    push_type_ref(out, name, node, file, ctx);
                }
            }
            "generic_type" => {
                // First named child is the base type; the argument list is a
                // positional `type_arguments` child.
                if let Some(base) = node.named_children(&mut node.walk()).next() {
                    type_leaf(&base, bytes, file, ctx, out);
                }
                if let Some(args) = node
                    .children(&mut node.walk())
                    .find(|c| c.kind() == "type_arguments")
                {
                    for child in args.named_children(&mut args.walk()) {
                        type_leaf(&child, bytes, file, TypeRefContext::GenericArg, out);
                    }
                }
            }
            "array_type" => {
                if let Some(element) = node.child_by_field_name("element") {
                    type_leaf(&element, bytes, file, ctx, out);
                }
            }
            _ => {}
        }
    }

    match node.kind() {
        "formal_parameter" => {
            if let Some(type_node) = node.child_by_field_name("type") {
                type_leaf(&type_node, bytes, file, TypeRefContext::ParameterType, out);
            }
        }
        "method_declaration" => {
            if let Some(type_node) = node.child_by_field_name("type") {
                type_leaf(&type_node, bytes, file, TypeRefContext::ReturnType, out);
            }
        }
        "field_declaration" => {
            if let Some(type_node) = node.child_by_field_name("type") {
                type_leaf(&type_node, bytes, file, TypeRefContext::Field, out);
            }
        }
        _ => {}
    }

    for child in node.children(&mut node.walk()) {
        collect_type_references(&child, bytes, file, out);
    }
}

// ── Edge richness: Read / Write ──────────────────────────────────────────────

/// Groovy declaration node kinds whose `name:` field is a binding introduction,
/// not a value read.
const DECL_KINDS_WITH_NAME: &[&str] = &[
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "enum_constant",
    "method_declaration",
    "constructor_declaration",
    "function_definition",
];

/// Returns `true` when `node` (an `identifier`) is in a position already
/// captured by another collector and must NOT be emitted as a Read:
///
/// - Call names: `method_invocation`/`juxt_function_call` `name:` field (the
///   `object:` receiver identifier IS a read — kept).
/// - Declaration names (class / method / function / enum constant / …).
/// - Variable declarator names (`variable_declarator` `name:` — covers both
///   fields and locals) and `formal_parameter` names.
/// - Lambda parameters (`lambda_expression` `parameters:` — a binding).
/// - Field access member names (`field_access` `field:`; the `object:` side
///   falls through as a read).
/// - Package / import qualified-name segments — already Import refs / a
///   package declaration, never value reads.
/// - Assignment LHS — handled by [`collect_write_references`].
fn is_non_read_position(node: &Node) -> bool {
    // Qualified names nest as `scoped_identifier`s of arbitrary depth; walk up
    // the name chain to see whether it is rooted in a package or import
    // declaration (in which case no segment is a value read).
    let mut ancestor = node.parent();
    while let Some(p) = ancestor {
        match p.kind() {
            "package_declaration" | "import_declaration" => return true,
            "scoped_identifier" => ancestor = p.parent(),
            _ => break,
        }
    }

    let parent = match node.parent() {
        Some(p) => p,
        None => return true, // root — not a read
    };
    match parent.kind() {
        "method_invocation" | "juxt_function_call" => {
            parent.child_by_field_name("name").as_ref() == Some(node)
        }
        kind if DECL_KINDS_WITH_NAME.contains(&kind) => {
            parent.child_by_field_name("name").as_ref() == Some(node)
        }
        "variable_declarator" => parent.child_by_field_name("name").as_ref() == Some(node),
        "formal_parameter" => parent.child_by_field_name("name").as_ref() == Some(node),
        "lambda_expression" => parent.child_by_field_name("parameters").as_ref() == Some(node),
        "field_access" => parent.child_by_field_name("field").as_ref() == Some(node),
        "assignment_expression" => parent.child_by_field_name("left").as_ref() == Some(node),
        _ => false,
    }
}

/// Recursively walk `node` and emit [`RefRole::Read`] references for bare
/// `identifier` nodes used in value/expression positions. Applies
/// [`MIN_REF_LEN`] (so the implicit closure parameter `it` never registers).
fn collect_read_references(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    if node.kind() == "identifier" {
        let name = node_text(node, bytes);
        if name.len() >= MIN_REF_LEN && !is_non_read_position(node) {
            push_ref(out, name, node, file, RefRole::Read);
        }
        return;
    }
    for child in node.children(&mut node.walk()) {
        collect_read_references(&child, bytes, file, out);
    }
}

/// Recursively walk `node` and emit [`RefRole::Write`] references for the
/// bare-identifier LHS of `assignment_expression` nodes (compound assignments
/// like `x += 1` parse to the same node kind). Member / subscript LHS are not
/// covered in v1 — only bare identifiers. Applies [`MIN_REF_LEN`].
fn collect_write_references(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    if node.kind() == "assignment_expression" {
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

/// Build the lexical scope tree for one Groovy file.
///
/// `scopes[0]` is always the file-root `Module` scope spanning `[0,
/// source_len)`. Type declarations open `Type` scopes; method / constructor /
/// script-function bodies and bare closures (Groovy's lambdas) open `Function`
/// scopes; bare blocks open `Block` scopes.
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

/// DFS opening scopes for Groovy declaration nodes.
///
/// Uses the "peel-the-body" pattern: the scope opens on the whole declaration
/// node, then we recurse into the body's **children** directly, so the body
/// node (`block`, `constructor_body`, or a `function_definition`'s `closure`)
/// does not double-open a scope.
fn scope_dfs(node: &Node, parent_id: ScopeId, scopes: &mut Vec<Scope>) {
    match node.kind() {
        "class_declaration" | "interface_declaration" | "enum_declaration" => {
            let type_id = push_scope(scopes, Some(parent_id), node_span(node), ScopeKind::Type);
            if let Some(body) = node.child_by_field_name("body") {
                for child in body.children(&mut body.walk()) {
                    scope_dfs(&child, type_id, scopes);
                }
            }
        }
        "method_declaration" | "constructor_declaration" | "function_definition" => {
            let fn_id = push_scope(
                scopes,
                Some(parent_id),
                node_span(node),
                ScopeKind::Function,
            );
            // Interface methods have no body — handle `None`.
            if let Some(body) = node.child_by_field_name("body") {
                for child in body.children(&mut body.walk()) {
                    scope_dfs(&child, fn_id, scopes);
                }
            }
        }
        // A bare closure NOT already consumed as a definition body — Groovy's
        // lambda (`{ x -> … }`, `[1,2].each { … }`).
        "closure" => {
            let fn_id = push_scope(
                scopes,
                Some(parent_id),
                node_span(node),
                ScopeKind::Function,
            );
            for child in node.children(&mut node.walk()) {
                scope_dfs(&child, fn_id, scopes);
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

/// Collect parameter and local-variable [`Binding`]s for one Groovy file.
///
/// Covers:
/// - `method_declaration` / `constructor_declaration` / `function_definition`
///   parameters → [`BindingKind::Param`].
/// - `lambda_expression` parameters (a bare identifier in this grammar)
///   → [`BindingKind::Param`].
/// - `local_variable_declaration` declarators → [`BindingKind::Local`]. No
///   scope-0 guard is needed (unlike Java): class fields parse as a distinct
///   `field_declaration` kind, and a scope-0 local is a genuine script-level
///   variable.
///
/// Class fields are covered by [`definition_bindings`] as
/// [`BindingKind::Definition`].
fn collect_bindings(root: &Node, bytes: &[u8], scopes: &[Scope]) -> Vec<Binding> {
    let mut out = Vec::new();
    collect_bindings_dfs(root, bytes, scopes, &mut out);
    out
}

fn collect_bindings_dfs(node: &Node, bytes: &[u8], scopes: &[Scope], out: &mut Vec<Binding>) {
    match node.kind() {
        "method_declaration" | "constructor_declaration" | "function_definition" => {
            if let Some(params) = node.child_by_field_name("parameters") {
                collect_params(&params, bytes, scopes, out);
            }
        }
        "lambda_expression" => {
            // `{ x -> … }` — the parameter is a bare `identifier` field.
            // (Multi-parameter closures mis-parse in this grammar version and
            // are not recovered — an accepted ceiling.)
            if let Some(params_node) = node.child_by_field_name("parameters") {
                if params_node.kind() == "identifier" {
                    let name = node_text(&params_node, bytes);
                    push_binding(
                        out,
                        name.to_owned(),
                        params_node.start_byte(),
                        BindingKind::Param,
                        scopes,
                    );
                }
            }
        }
        "local_variable_declaration" => {
            let mut cursor = node.walk();
            for declarator in node.children_by_field_name("declarator", &mut cursor) {
                if let Some(name_node) = declarator.child_by_field_name("name") {
                    let name = node_text(&name_node, bytes);
                    push_binding(
                        out,
                        name.to_owned(),
                        name_node.start_byte(),
                        BindingKind::Local,
                        scopes,
                    );
                }
            }
        }
        _ => {}
    }
    for child in node.children(&mut node.walk()) {
        collect_bindings_dfs(&child, bytes, scopes, out);
    }
}

/// Emit a [`BindingKind::Param`] for each `formal_parameter` in a
/// `formal_parameters` node.
fn collect_params(params: &Node, bytes: &[u8], scopes: &[Scope], out: &mut Vec<Binding>) {
    for child in params.named_children(&mut params.walk()) {
        if child.kind() == "formal_parameter" {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = node_text(&name_node, bytes);
                push_binding(
                    out,
                    name.to_owned(),
                    name_node.start_byte(),
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
    fn extracts_class_and_members_with_visibility() {
        let src = r#"package com.example.zoo;

public class Animal {
    private String name;
    def age;
    public String speak(String greeting) { return greeting; }
    private def secret() { return 1; }
    def quietly() { return 2; }
}
"#;
        let facts = GroovyExtractor
            .extract(src, "src/com/example/zoo/Animal.groovy")
            .unwrap();
        let by_name = |n: &str| facts.symbols.iter().find(|s| s.name == n).cloned();

        let animal = by_name("Animal").unwrap();
        assert_eq!(animal.kind, SymbolKind::Class);
        assert_eq!(animal.visibility, Visibility::Public);
        assert_eq!(
            animal.id.to_scip_string(),
            "codegraph . . . com/example/zoo/Animal#"
        );

        let speak = by_name("speak").unwrap();
        assert_eq!(speak.kind, SymbolKind::Method);
        assert_eq!(speak.visibility, Visibility::Public);
        assert_eq!(
            speak.id.to_scip_string(),
            "codegraph . . . com/example/zoo/Animal#speak()."
        );

        let secret = by_name("secret").unwrap();
        assert_eq!(secret.visibility, Visibility::Private);

        // Unmarked `def` method → Groovy implicit-public.
        let quietly = by_name("quietly").unwrap();
        assert_eq!(quietly.visibility, Visibility::Public);

        let name_field = by_name("name").unwrap();
        assert_eq!(name_field.kind, SymbolKind::Static);
        assert_eq!(name_field.visibility, Visibility::Private);
        assert_eq!(
            name_field.id.to_scip_string(),
            "codegraph . . . com/example/zoo/Animal#name."
        );

        // Unmarked `def` field → implicit-public property.
        let age = by_name("age").unwrap();
        assert_eq!(age.visibility, Visibility::Public);

        assert_eq!(facts.lang, "groovy");
    }

    #[test]
    fn interface_and_enum_symbols() {
        let src = r#"package p;
interface Greeter {
    def greet();
}
enum Color { RED, GREEN, BLUE }
"#;
        let facts = GroovyExtractor.extract(src, "src/p/Api.groovy").unwrap();
        let by_name = |n: &str| facts.symbols.iter().find(|s| s.name == n).cloned();

        let greeter = by_name("Greeter").unwrap();
        assert_eq!(greeter.kind, SymbolKind::Interface);
        assert_eq!(greeter.id.to_scip_string(), "codegraph . . . p/Greeter#");

        let greet = by_name("greet").unwrap();
        assert_eq!(greet.kind, SymbolKind::Method);
        assert_eq!(greet.visibility, Visibility::Public);
        assert_eq!(
            greet.id.to_scip_string(),
            "codegraph . . . p/Greeter#greet()."
        );

        let color = by_name("Color").unwrap();
        assert_eq!(color.kind, SymbolKind::Enum);
        assert_eq!(color.id.to_scip_string(), "codegraph . . . p/Color#");
    }

    #[test]
    fn gradle_file_extracts_script_function_and_plain_calls() {
        // A .gradle file is plain Groovy (D-02): the script function is a real
        // Function symbol with a path-derived namespace, and DSL blocks come out
        // as ordinary call references — no Gradle semantics.
        let src = r#"plugins {
    id 'java'
}

def deployTo(env) {
    println "deploying";
}
"#;
        let facts = GroovyExtractor.extract(src, "build.gradle").unwrap();

        let deploy = facts
            .symbols
            .iter()
            .find(|s| s.name == "deployTo")
            .expect("expected a Function symbol 'deployTo'");
        assert_eq!(deploy.kind, SymbolKind::Function);
        assert_eq!(
            deploy.id.to_scip_string(),
            "codegraph . . . build/deployTo()."
        );

        let call_names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Call)
            .map(|r| r.name.as_str())
            .collect();
        assert!(
            call_names.contains(&"plugins"),
            "DSL block should surface as a plain call, got {call_names:?}"
        );
        assert!(
            call_names.contains(&"println"),
            "expected 'println' in {call_names:?}"
        );
    }

    #[test]
    fn paren_and_parenless_calls_are_captured() {
        let src =
            "def run() {\n    validate(\"t\");\n    println \"hello\";\n    new Server();\n}\n";
        let facts = GroovyExtractor.extract(src, "src/run.groovy").unwrap();
        let call_names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Call)
            .map(|r| r.name.as_str())
            .collect();
        assert!(call_names.contains(&"validate"), "got {call_names:?}");
        assert!(
            call_names.contains(&"println"),
            "paren-less juxt_function_call must be captured, got {call_names:?}"
        );
        assert!(
            call_names.contains(&"Server"),
            "constructor call must be captured, got {call_names:?}"
        );
    }

    #[test]
    fn member_calls_capture_qualifier() {
        let src =
            "def run() {\n    svc.fetch(1);\n    this.speak(\"x\");\n    svc.inner.fetch(2);\n}\n";
        let facts = GroovyExtractor.extract(src, "src/run.groovy").unwrap();

        let fetch_quals: Vec<Option<&str>> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Call && r.name == "fetch")
            .map(|r| r.qualifier.as_deref())
            .collect();
        assert!(
            fetch_quals.contains(&Some("svc")),
            "expected qualifier 'svc', got {fetch_quals:?}"
        );
        assert!(
            fetch_quals.contains(&Some("svc.inner")),
            "expected qualifier 'svc.inner', got {fetch_quals:?}"
        );

        let speak = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "speak")
            .expect("expected a Call reference 'speak'");
        assert_eq!(speak.qualifier.as_deref(), Some("this"));

        // Plain calls carry no qualifier.
        let validate_src = "def go() { validate(\"t\"); }\n";
        let plain = GroovyExtractor
            .extract(validate_src, "src/go.groovy")
            .unwrap();
        let v = plain
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "validate")
            .unwrap();
        assert_eq!(v.qualifier, None);
    }

    #[test]
    fn extracts_inheritance_references() {
        let src =
            "package p;\nclass Foo extends Bar implements Baz, Qux {}\ninterface I extends J {}\n";
        let facts = GroovyExtractor.extract(src, "src/p/Foo.groovy").unwrap();

        let inherit_names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::IsImplementation)
            .map(|r| r.name.as_str())
            .collect();
        for expected in ["Bar", "Baz", "Qux", "J"] {
            assert!(
                inherit_names.contains(&expected),
                "expected '{expected}' in {inherit_names:?}"
            );
        }
    }

    #[test]
    fn extracts_named_and_static_imports_skips_wildcard() {
        let src = "import com.example.Service;\nimport static com.x.Util.helper;\nimport com.y.*;\nclass A {}\n";
        let facts = GroovyExtractor.extract(src, "src/A.groovy").unwrap();

        let imports: Vec<(&str, Option<&str>)> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| (r.name.as_str(), r.from_path.as_deref()))
            .collect();
        assert!(
            imports.contains(&("Service", Some("com.example"))),
            "expected named import with from_path, got {imports:?}"
        );
        assert!(
            imports.contains(&("helper", Some("com.x.Util"))),
            "expected static import leaf, got {imports:?}"
        );
        assert_eq!(
            imports.len(),
            2,
            "wildcard import must emit nothing, got {imports:?}"
        );
    }

    #[test]
    fn import_refs_carry_source_module() {
        let src = "package p;\nimport com.example.Service;\nclass A {}\n";
        let facts = GroovyExtractor.extract(src, "src/p/A.groovy").unwrap();
        let module_id = facts
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module)
            .map(|s| s.id.to_scip_string())
            .unwrap();
        let import = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Import)
            .expect("expected an Import ref");
        assert_eq!(import.source_module.as_deref(), Some(module_id.as_str()));
    }

    #[test]
    fn extracts_type_references_and_skips_def() {
        let src = r#"class Box {
    List<String> items;
    def dyn;
    Widget fetch(Set<Long> ids, Gadget g) { return null; }
}
"#;
        let facts = GroovyExtractor.extract(src, "src/Box.groovy").unwrap();
        let type_refs: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::TypeRef)
            .map(|r| r.name.as_str())
            .collect();
        for expected in ["List", "String", "Widget", "Set", "Long", "Gadget"] {
            assert!(
                type_refs.contains(&expected),
                "expected '{expected}' in {type_refs:?}"
            );
        }
        assert!(
            !type_refs.contains(&"def"),
            "`def` is a dynamic marker, not a type: {type_refs:?}"
        );
    }

    #[test]
    fn read_and_write_references() {
        let src = "def calc() {\n    def total = 1;\n    total = 2;\n    total += 1;\n    return total;\n}\n";
        let facts = GroovyExtractor.extract(src, "src/calc.groovy").unwrap();

        let writes = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Write && r.name == "total")
            .count();
        assert_eq!(writes, 2, "plain + compound assignment are both Writes");
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Read && r.name == "total"),
            "expected a Read for 'total' from `return total`"
        );
        // The declarator name itself is a binding, not a Read.
        let reads = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Read && r.name == "total")
            .count();
        assert_eq!(reads, 1, "only `return total` is a Read");
    }

    #[test]
    fn package_and_import_segments_are_not_reads() {
        let src = "package com.example;\nimport com.example.alpha.Service;\nclass Main { def run() { return Service.helper(); } }\n";
        let facts = GroovyExtractor
            .extract(src, "src/com/example/Main.groovy")
            .unwrap();
        for seg in ["com", "example", "alpha"] {
            assert!(
                !facts
                    .references
                    .iter()
                    .any(|r| r.role == RefRole::Read && r.name == seg),
                "package/import segment '{seg}' must not be a Read"
            );
        }
        // The receiver of the member call IS a genuine read.
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Read && r.name == "Service"),
            "`Service` receiver should remain a Read ref"
        );
    }

    #[test]
    fn static_main_gets_entry_point_marker() {
        let src = "class App { static void main(String[] args) { } }\n";
        let facts = GroovyExtractor.extract(src, "src/App.groovy").unwrap();
        let main = facts
            .symbols
            .iter()
            .find(|s| s.name == "main")
            .expect("expected a 'main' method symbol");
        assert_eq!(main.entry_points.len(), 1);
        assert!(matches!(&main.entry_points[0], EntryPoint::Main));
    }

    #[test]
    fn scopes_and_bindings() {
        let src = "class C {\n    def f(int a) {\n        def x = 1;\n        [1, 2].each { y -> x + y };\n    }\n}\n";
        let facts = GroovyExtractor.extract(src, "src/C.groovy").unwrap();

        assert_eq!(facts.scopes[0].kind, ScopeKind::Module);
        let type_scope = facts
            .scopes
            .iter()
            .position(|s| s.kind == ScopeKind::Type)
            .expect("expected a Type scope");
        let fn_scope = facts
            .scopes
            .iter()
            .find(|s| s.kind == ScopeKind::Function)
            .expect("expected a Function scope for the method");
        assert_eq!(fn_scope.parent, Some(type_scope));

        let param = facts
            .bindings
            .iter()
            .find(|b| b.kind == BindingKind::Param && b.name == "a")
            .expect("expected Param binding for 'a'");
        assert_eq!(facts.scopes[param.scope].kind, ScopeKind::Function);

        let local = facts
            .bindings
            .iter()
            .find(|b| b.kind == BindingKind::Local && b.name == "x")
            .expect("expected Local binding for 'x'");
        assert_ne!(local.scope, 0);

        // The closure's explicit lambda parameter binds in the closure scope.
        let lambda_param = facts
            .bindings
            .iter()
            .find(|b| b.kind == BindingKind::Param && b.name == "y")
            .expect("expected Param binding for the lambda parameter 'y'");
        assert_eq!(facts.scopes[lambda_param.scope].kind, ScopeKind::Function);
    }

    #[test]
    fn missing_semicolon_recovery_still_extracts() {
        // Idiomatic newline-terminated Groovy: the grammar inserts MISSING ";"
        // tokens (root.has_error() == true) but definitions and reads are
        // still extracted. (Call facts adjacent to a repaired line can be
        // lost — a documented grammar-recovery ceiling.)
        let src = "package p\nclass Svc {\n    String name\n    def go() {\n        def v = name\n    }\n}\n";
        let facts = GroovyExtractor.extract(src, "src/p/Svc.groovy").unwrap();
        let by_name = |n: &str| facts.symbols.iter().find(|s| s.name == n).cloned();
        assert!(by_name("Svc").is_some());
        assert!(by_name("name").is_some());
        assert!(by_name("go").is_some());
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Read && r.name == "name"),
            "`def v = name` initializer must survive MISSING-token recovery as a Read"
        );
        assert!(
            facts
                .bindings
                .iter()
                .any(|b| b.kind == BindingKind::Local && b.name == "v"),
            "the local binding must survive MISSING-token recovery"
        );
    }
}
