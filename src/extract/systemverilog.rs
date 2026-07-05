// SPDX-License-Identifier: Apache-2.0

//! SystemVerilog extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: `module`/`interface`/`package`/`class` declarations plus
//! `function`/`task` declarations at any nesting level (package items, module
//! items, class methods). `function new()` constructors are a distinct grammar
//! node with no `name:` field — their implicit name is `new`. Module and
//! interface names live one level down on the `*_ansi_header` /
//! `*_nonansi_header` child, NOT on the declaration node itself; package and
//! class names are direct `name:` fields.
//! Qualified identity is derived from the file path (`src/alu.sv` → namespace
//! `alu`) with container descriptors nested per declaration (`Packet#get_data().`).
//!
//! References: function/task calls (`tf_call`, including hierarchical
//! `obj.method()` receivers and `pkg::fn()` scoped calls, which parse as
//! `method_call` — the receiver/package is captured as the reference's
//! `qualifier`); `import pkg::*` / `import pkg::item` → [`RefRole::Import`]
//! (wildcard-vs-named is not distinguishable from the AST — all package
//! imports are treated as whole-package imports); `` `include "file.svh" `` →
//! [`RefRole::Import`] at file-path granularity (textual inclusion, same
//! honest ceiling as C's `#include`); module instantiations →
//! [`RefRole::TypeRef`] on the `instance_type:` field (the instance name
//! itself is not a symbol definition in v1).
//!
//! Visibility: `local`/`protected` class members map to
//! [`Visibility::Private`]/[`Visibility::Protected`]; everything else has
//! compilation-unit visibility → [`Visibility::Public`].
//!
//! Honest ceilings (documented, never guessed past):
//! - **Elaboration/parameterization semantics are out of scope** — parameter
//!   overrides, `defparam`, and hierarchy elaboration are never evaluated.
//! - **Generate blocks are table stakes only**: the walk descends into
//!   `generate_region`/`generate_block` so instantiations and calls inside are
//!   captured once, but no per-iteration expansion is modeled.
//! - Read/Write dataflow on module signals is not emitted (module-level
//!   dataflow, not simple variable read/write).
//! - Class inheritance (`extends`) is not emitted in v1 — classes are flat
//!   symbols.
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.

use tree_sitter::{Node, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{
    ByteSpan, FileFacts, Reference, Scope, ScopeId, ScopeKind, Symbol, SymbolKind, TypeRefContext,
    Visibility,
};
use crate::lang::Language;
use crate::symbol::Descriptor;

use super::{
    ExtractCtx, Extractor, attach_reference_scopes, collect_call_references, definition_bindings,
    field_text, import_bindings, make_symbol, node_span, node_text, one_line_signature,
    push_import_ref, push_scope, push_type_ref,
};

/// Plain function/task call — a `hierarchical_identifier` with exactly one
/// segment (`get_data()`). Anchored on both sides so it never also matches a
/// hierarchical call (disjoint from [`TF_CALL_QUALIFIED_QUERY`]).
const TF_CALL_PLAIN_QUERY: &str =
    r#"(tf_call (hierarchical_identifier . (simple_identifier) @callee .))"#;

/// Hierarchical function/task call (`pkt.send()`, `top.dut.helper()`): the
/// callee is the LAST segment; the immediate receiver (the segment just before
/// it) is captured as `@qualifier`.
const TF_CALL_QUALIFIED_QUERY: &str = r#"(tf_call (hierarchical_identifier (simple_identifier) @qualifier . (simple_identifier) @callee .))"#;

/// Expression-position scoped/method call — `pkg::fn(x)` and `obj.method(x)`
/// both parse as `method_call` with the receiver as a `primary` and the callee
/// on `method_call_body`'s `name:` field.
const METHOD_CALL_QUERY: &str = r#"(method_call (primary (hierarchical_identifier (simple_identifier) @qualifier .)) (method_call_body name: (simple_identifier) @callee))"#;

/// Extracts SystemVerilog symbols and references.
pub struct SystemVerilogExtractor;

impl Extractor for SystemVerilogExtractor {
    fn lang(&self) -> Language {
        Language::SystemVerilog
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_language = crate::grammar::systemverilog();
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
        let namespaces = sv_namespaces(file);
        let ctx = ExtractCtx {
            bytes,
            file,
            lang: Language::SystemVerilog,
        };

        let mut defs = Vec::new();
        let prefix: Vec<Descriptor> = namespaces
            .iter()
            .cloned()
            .map(Descriptor::Namespace)
            .collect();
        collect_symbols_dfs(&root, &ctx, &prefix, false, &mut defs);
        let def_bindings = definition_bindings(&defs);
        let mut symbols = defs;
        let mod_sym =
            super::module_symbol(Language::SystemVerilog, &namespaces, file, source.len());
        let module_id = mod_sym.id.to_scip_string();
        symbols.push(mod_sym);

        let mut references = Vec::new();
        collect_imports(&root, bytes, file, &module_id, &mut references);
        for query in [
            TF_CALL_PLAIN_QUERY,
            TF_CALL_QUALIFIED_QUERY,
            METHOD_CALL_QUERY,
        ] {
            references.extend(collect_call_references(
                &root,
                &ts_language,
                query,
                Language::SystemVerilog,
                bytes,
                file,
            )?);
        }
        collect_instantiations(&root, bytes, file, &mut references);

        let scopes = collect_scopes(&root, source.len());
        attach_reference_scopes(&mut references, &scopes);
        let mut bindings = def_bindings;
        bindings.extend(import_bindings(&references, &scopes));

        Ok(FileFacts {
            file: file.to_owned(),
            lang: Language::SystemVerilog.as_str().to_owned(),
            symbols,
            references,
            scopes,
            bindings,
            ffi_exports: Vec::new(),
        })
    }
}

/// Derive the SystemVerilog namespace path from a file path.
///
/// Strips the `.sv` or `.svh` extension, strips a leading `src/`, `rtl/`, or
/// `tb/` prefix (the common HDL source-tree roots), then splits on `/`. The
/// file stem is kept as the last namespace segment.
fn sv_namespaces(file: &str) -> Vec<String> {
    let p = file
        .strip_suffix(".sv")
        .or_else(|| file.strip_suffix(".svh"))
        .unwrap_or(file);
    let p = p
        .strip_prefix("src/")
        .or_else(|| p.strip_prefix("rtl/"))
        .or_else(|| p.strip_prefix("tb/"))
        .unwrap_or(p);
    p.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The declared name of a `module_declaration`/`interface_declaration`: the
/// `name:` field of its `*_ansi_header` or `*_nonansi_header` child. The name
/// is NOT a field on the declaration node itself (unlike package/class).
fn header_name(node: &Node, bytes: &[u8]) -> Option<String> {
    node.children(&mut node.walk())
        .find(|c| {
            matches!(
                c.kind(),
                "module_ansi_header"
                    | "module_nonansi_header"
                    | "interface_ansi_header"
                    | "interface_nonansi_header"
            )
        })
        .and_then(|header| field_text(&header, "name", bytes))
}

/// Visibility of a class method: the enclosing `class_method` wrapper may
/// carry a `method_qualifier` → `class_item_qualifier` whose text is `local`
/// or `protected`. Unqualified members are public (SV's default).
fn method_visibility(node: &Node, bytes: &[u8]) -> Visibility {
    let Some(parent) = node.parent() else {
        return Visibility::Public;
    };
    if parent.kind() != "class_method" {
        return Visibility::Public;
    }
    for child in parent.children(&mut parent.walk()) {
        let qualifier = match child.kind() {
            "class_item_qualifier" => Some(child),
            "method_qualifier" => child
                .children(&mut child.walk())
                .find(|c| c.kind() == "class_item_qualifier"),
            _ => None,
        };
        if let Some(q) = qualifier {
            return match node_text(&q, bytes) {
                "local" => Visibility::Private,
                "protected" => Visibility::Protected,
                _ => Visibility::Public,
            };
        }
    }
    Visibility::Public
}

/// Append `leaf` to `prefix`, build the signature, and push the symbol.
#[allow(clippy::too_many_arguments)]
fn push_symbol(
    out: &mut Vec<Symbol>,
    ctx: &ExtractCtx<'_>,
    node: &Node,
    prefix: &[Descriptor],
    leaf: Descriptor,
    name: String,
    kind: SymbolKind,
    visibility: Visibility,
) {
    let mut descriptors = prefix.to_vec();
    descriptors.push(leaf);
    let signature = one_line_signature(node_text(node, ctx.bytes), &[';']);
    out.push(make_symbol(
        ctx,
        node,
        name,
        kind,
        visibility,
        descriptors,
        signature,
    ));
}

/// DFS emitting container symbols (module/interface/package/class) and
/// function/task symbols. Containers extend the descriptor prefix for their
/// children, so a class method renders as `…/Packet#get_data().`. `in_class`
/// switches function/task symbols from [`SymbolKind::Function`] to
/// [`SymbolKind::Method`] and enables `local`/`protected` visibility reads.
fn collect_symbols_dfs(
    node: &Node,
    ctx: &ExtractCtx<'_>,
    prefix: &[Descriptor],
    in_class: bool,
    out: &mut Vec<Symbol>,
) {
    match node.kind() {
        "module_declaration" | "interface_declaration" => {
            let Some(name) = header_name(node, ctx.bytes) else {
                return;
            };
            let kind = if node.kind() == "module_declaration" {
                SymbolKind::Module
            } else {
                SymbolKind::Interface
            };
            push_symbol(
                out,
                ctx,
                node,
                prefix,
                Descriptor::Type(name.clone()),
                name.clone(),
                kind,
                Visibility::Public,
            );
            let mut child_prefix = prefix.to_vec();
            child_prefix.push(Descriptor::Type(name));
            for child in node.children(&mut node.walk()) {
                collect_symbols_dfs(&child, ctx, &child_prefix, false, out);
            }
            return;
        }
        "package_declaration" => {
            let Some(name) = field_text(node, "name", ctx.bytes) else {
                return;
            };
            // Packages are namespaces (`pkg::item`), so the descriptor is a
            // Namespace segment, not a Type.
            push_symbol(
                out,
                ctx,
                node,
                prefix,
                Descriptor::Namespace(name.clone()),
                name.clone(),
                SymbolKind::Module,
                Visibility::Public,
            );
            let mut child_prefix = prefix.to_vec();
            child_prefix.push(Descriptor::Namespace(name));
            for child in node.children(&mut node.walk()) {
                collect_symbols_dfs(&child, ctx, &child_prefix, false, out);
            }
            return;
        }
        "class_declaration" => {
            let Some(name) = field_text(node, "name", ctx.bytes) else {
                return;
            };
            push_symbol(
                out,
                ctx,
                node,
                prefix,
                Descriptor::Type(name.clone()),
                name.clone(),
                SymbolKind::Class,
                Visibility::Public,
            );
            let mut child_prefix = prefix.to_vec();
            child_prefix.push(Descriptor::Type(name));
            for child in node.children(&mut node.walk()) {
                collect_symbols_dfs(&child, ctx, &child_prefix, true, out);
            }
            return;
        }
        "function_declaration" | "task_declaration" => {
            // The name lives on the inner `*_body_declaration` node's `name:`
            // field, not on the declaration node itself.
            let body_kind = if node.kind() == "function_declaration" {
                "function_body_declaration"
            } else {
                "task_body_declaration"
            };
            let Some(name) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == body_kind)
                .and_then(|body| field_text(&body, "name", ctx.bytes))
            else {
                return;
            };
            let kind = if in_class {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            };
            let visibility = if in_class {
                method_visibility(node, ctx.bytes)
            } else {
                Visibility::Public
            };
            push_symbol(
                out,
                ctx,
                node,
                prefix,
                Descriptor::Method {
                    name: name.clone(),
                    disambiguator: String::new(),
                },
                name,
                kind,
                visibility,
            );
            return; // SV does not nest declarations inside function/task bodies.
        }
        "class_constructor_declaration" => {
            // `function new(); … endfunction` — a distinct node kind with NO
            // `name:` field; the constructor's name is implicitly `new`.
            push_symbol(
                out,
                ctx,
                node,
                prefix,
                Descriptor::Method {
                    name: "new".to_owned(),
                    disambiguator: String::new(),
                },
                "new".to_owned(),
                SymbolKind::Method,
                method_visibility(node, ctx.bytes),
            );
            return;
        }
        _ => {}
    }
    for child in node.children(&mut node.walk()) {
        collect_symbols_dfs(&child, ctx, prefix, in_class, out);
    }
}

// ── Imports: `import pkg::*` / `import pkg::item` / `include ─────────────────

/// Recursively walk `node` and emit [`RefRole::Import`] references for
/// `package_import_declaration` items (the package name is the FIRST
/// `simple_identifier` of each `package_import_item`; wildcard vs named import
/// is not distinguishable from the AST, so all are whole-package imports) and
/// `include_compiler_directive` nodes (the included path, quotes stripped via
/// the inner `quoted_string_item` node).
///
/// [`RefRole::Import`]: crate::graph::types::RefRole::Import
fn collect_imports(
    node: &Node,
    bytes: &[u8],
    file: &str,
    module_id: &str,
    out: &mut Vec<Reference>,
) {
    match node.kind() {
        "package_import_declaration" => {
            for item in node.children(&mut node.walk()) {
                if item.kind() != "package_import_item" {
                    continue;
                }
                let Some(pkg_node) = item
                    .children(&mut item.walk())
                    .find(|c| c.kind() == "simple_identifier")
                else {
                    continue;
                };
                let name = node_text(&pkg_node, bytes);
                let from_path = node_text(&item, bytes);
                push_import_ref(out, name, &pkg_node, file, module_id, from_path);
            }
            return;
        }
        "include_compiler_directive" => {
            if let Some(qs) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "quoted_string")
            {
                if let Some(item) = qs
                    .children(&mut qs.walk())
                    .find(|c| c.kind() == "quoted_string_item")
                {
                    let path = node_text(&item, bytes);
                    push_import_ref(out, path, &item, file, module_id, path);
                }
            }
            return;
        }
        _ => {}
    }
    for child in node.children(&mut node.walk()) {
        collect_imports(&child, bytes, file, module_id, out);
    }
}

// ── Module instantiations → TypeRef ──────────────────────────────────────────

/// Recursively walk `node` and emit a [`RefRole::TypeRef`] reference for every
/// `module_instantiation`'s `instance_type:` field (the module being
/// instantiated). The local instance name (`name_of_instance`) is NOT a symbol
/// definition in v1 (no elaboration semantics). [`TypeRefContext::Other`] is
/// used — an instantiation is neither a parameter, return, nor field position.
///
/// [`RefRole::TypeRef`]: crate::graph::types::RefRole::TypeRef
fn collect_instantiations(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    if node.kind() == "module_instantiation" {
        if let Some(t) = node.child_by_field_name("instance_type") {
            push_type_ref(out, node_text(&t, bytes), &t, file, TypeRefContext::Other);
        }
        return; // instantiations do not nest.
    }
    for child in node.children(&mut node.walk()) {
        collect_instantiations(&child, bytes, file, out);
    }
}

// ── Scope tree (Tier-B) ──────────────────────────────────────────────────────

/// Build the lexical scope tree for one SystemVerilog file.
///
/// `scopes[0]` is always the file-root `Module` scope spanning `[0,
/// source_len)`. `function_declaration`, `task_declaration`, and
/// `class_constructor_declaration` nodes open a `Function` scope; container
/// declarations (module/interface/package/class) do not open a scope of their
/// own — their children are visited under the enclosing scope (same v1 shape
/// as the PowerShell extractor's class handling).
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

/// DFS opening `Function` scopes for function/task/constructor declarations;
/// every other node kind recurses without opening a new scope.
fn scope_dfs(node: &Node, parent_id: ScopeId, scopes: &mut Vec<Scope>) {
    if matches!(
        node.kind(),
        "function_declaration" | "task_declaration" | "class_constructor_declaration"
    ) {
        let fn_id = push_scope(
            scopes,
            Some(parent_id),
            node_span(node),
            ScopeKind::Function,
        );
        for child in node.children(&mut node.walk()) {
            scope_dfs(&child, fn_id, scopes);
        }
    } else {
        for child in node.children(&mut node.walk()) {
            scope_dfs(&child, parent_id, scopes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::{BindingKind, RefRole};

    #[test]
    fn extracts_module_symbol() {
        let src = "module adder(input logic [7:0] a, output logic [7:0] sum);\n  assign sum = a;\nendmodule\n";
        let facts = SystemVerilogExtractor.extract(src, "src/adder.sv").unwrap();
        let adder = facts
            .symbols
            .iter()
            .find(|s| s.name == "adder" && s.kind == SymbolKind::Module && s.span.start == 0)
            .expect("expected a Module symbol 'adder'");
        assert_eq!(adder.visibility, Visibility::Public);
        assert_eq!(adder.id.to_scip_string(), "codegraph . . . adder/adder#");
        assert_eq!(facts.lang, "systemverilog");
    }

    #[test]
    fn extracts_nonansi_module_symbol() {
        let src = "module legacy(a, b);\n  input a;\n  output b;\nendmodule\n";
        let facts = SystemVerilogExtractor
            .extract(src, "rtl/legacy.sv")
            .unwrap();
        let legacy = facts
            .symbols
            .iter()
            .find(|s| s.name == "legacy" && s.kind == SymbolKind::Module && s.span.start == 0)
            .expect("expected a Module symbol 'legacy' (non-ANSI header)");
        assert_eq!(legacy.id.to_scip_string(), "codegraph . . . legacy/legacy#");
    }

    #[test]
    fn extracts_interface_symbol() {
        let src = "interface bus_if;\n  logic clk;\nendinterface\n";
        let facts = SystemVerilogExtractor.extract(src, "src/bus.sv").unwrap();
        let bus = facts
            .symbols
            .iter()
            .find(|s| s.name == "bus_if")
            .expect("expected an Interface symbol 'bus_if'");
        assert_eq!(bus.kind, SymbolKind::Interface);
        assert_eq!(bus.id.to_scip_string(), "codegraph . . . bus/bus_if#");
    }

    #[test]
    fn extracts_package_and_package_function() {
        let src = "package mypkg;\n  function int double_it(int x);\n    return x * 2;\n  endfunction\nendpackage\n";
        let facts = SystemVerilogExtractor.extract(src, "src/mypkg.sv").unwrap();

        let pkg = facts
            .symbols
            .iter()
            .find(|s| s.name == "mypkg" && s.kind == SymbolKind::Module && s.span.start == 0)
            .expect("expected a package symbol 'mypkg'");
        assert_eq!(pkg.id.to_scip_string(), "codegraph . . . mypkg/mypkg/");

        let f = facts
            .symbols
            .iter()
            .find(|s| s.name == "double_it")
            .expect("expected a Function symbol 'double_it'");
        assert_eq!(f.kind, SymbolKind::Function);
        assert_eq!(f.visibility, Visibility::Public);
        assert_eq!(
            f.id.to_scip_string(),
            "codegraph . . . mypkg/mypkg/double_it()."
        );
        assert_eq!(f.signature, "function int double_it(int x)");
    }

    #[test]
    fn extracts_class_with_ctor_method_task_and_visibility() {
        let src = "class Packet;\n  int data;\n  function new();\n    data = 0;\n  endfunction\n  local function int hidden();\n    return data;\n  endfunction\n  protected task guarded();\n  endtask\n  function int get_data();\n    return data;\n  endfunction\nendclass\n";
        let facts = SystemVerilogExtractor.extract(src, "src/pkt.sv").unwrap();

        let class = facts
            .symbols
            .iter()
            .find(|s| s.name == "Packet")
            .expect("expected a Class symbol 'Packet'");
        assert_eq!(class.kind, SymbolKind::Class);
        assert_eq!(class.id.to_scip_string(), "codegraph . . . pkt/Packet#");

        let ctor = facts
            .symbols
            .iter()
            .find(|s| s.name == "new")
            .expect("expected a constructor symbol 'new'");
        assert_eq!(ctor.kind, SymbolKind::Method);
        assert_eq!(ctor.visibility, Visibility::Public);
        assert_eq!(
            ctor.id.to_scip_string(),
            "codegraph . . . pkt/Packet#new()."
        );

        let hidden = facts
            .symbols
            .iter()
            .find(|s| s.name == "hidden")
            .expect("expected a Method symbol 'hidden'");
        assert_eq!(hidden.kind, SymbolKind::Method);
        assert_eq!(hidden.visibility, Visibility::Private);
        assert_eq!(
            hidden.id.to_scip_string(),
            "codegraph . . . pkt/Packet#hidden()."
        );

        let guarded = facts
            .symbols
            .iter()
            .find(|s| s.name == "guarded")
            .expect("expected a Method symbol 'guarded'");
        assert_eq!(guarded.kind, SymbolKind::Method);
        assert_eq!(guarded.visibility, Visibility::Protected);

        let get_data = facts
            .symbols
            .iter()
            .find(|s| s.name == "get_data")
            .expect("expected a Method symbol 'get_data'");
        assert_eq!(get_data.kind, SymbolKind::Method);
        assert_eq!(get_data.visibility, Visibility::Public);
        assert_eq!(
            get_data.id.to_scip_string(),
            "codegraph . . . pkt/Packet#get_data()."
        );
    }

    #[test]
    fn extracts_module_level_function_and_task() {
        let src = "module dut;\n  function int helper(int v);\n    return v + 1;\n  endfunction\n  task drive();\n  endtask\nendmodule\n";
        let facts = SystemVerilogExtractor.extract(src, "src/dut.sv").unwrap();

        let helper = facts
            .symbols
            .iter()
            .find(|s| s.name == "helper")
            .expect("expected a Function symbol 'helper'");
        assert_eq!(helper.kind, SymbolKind::Function);
        assert_eq!(helper.visibility, Visibility::Public);
        assert_eq!(
            helper.id.to_scip_string(),
            "codegraph . . . dut/dut#helper()."
        );

        let drive = facts
            .symbols
            .iter()
            .find(|s| s.name == "drive")
            .expect("expected a Function symbol 'drive'");
        assert_eq!(drive.kind, SymbolKind::Function);
        assert_eq!(
            drive.id.to_scip_string(),
            "codegraph . . . dut/dut#drive()."
        );
    }

    #[test]
    fn package_import_is_import_with_from_path() {
        let src = "module top;\n  import mypkg::*;\n  import otherpkg::helper;\nendmodule\n";
        let facts = SystemVerilogExtractor.extract(src, "src/top.sv").unwrap();

        let wildcard = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Import && r.name == "mypkg")
            .expect("expected an Import reference 'mypkg'");
        assert_eq!(wildcard.from_path.as_deref(), Some("mypkg::*"));

        let named = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Import && r.name == "otherpkg")
            .expect("expected an Import reference 'otherpkg'");
        assert_eq!(named.from_path.as_deref(), Some("otherpkg::helper"));

        // Import bindings land alongside (Tier-B facts).
        assert!(
            facts
                .bindings
                .iter()
                .any(|b| b.kind == BindingKind::Import && b.name == "mypkg"),
            "expected an Import binding for 'mypkg'"
        );
    }

    #[test]
    fn include_directive_is_file_level_import() {
        let src = "`include \"defs.svh\"\nmodule m;\nendmodule\n";
        let facts = SystemVerilogExtractor.extract(src, "src/m.sv").unwrap();
        let inc = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Import && r.name == "defs.svh")
            .expect("expected an Import reference 'defs.svh'");
        assert_eq!(inc.from_path.as_deref(), Some("defs.svh"));
    }

    #[test]
    fn plain_call_emitted_once_without_qualifier() {
        let src = "module dut;\n  function int get_data();\n    return 42;\n  endfunction\n  task run_test();\n    int x;\n    x = get_data();\n  endtask\nendmodule\n";
        let facts = SystemVerilogExtractor.extract(src, "src/dut.sv").unwrap();
        let calls: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Call && r.name == "get_data")
            .collect();
        assert_eq!(
            calls.len(),
            1,
            "plain call must match exactly one query, got {calls:?}"
        );
        assert_eq!(calls[0].qualifier, None);
        assert!(
            calls[0].scope.is_some() && calls[0].scope != Some(0),
            "call inside a task must have a non-root scope, got {:?}",
            calls[0].scope
        );
    }

    #[test]
    fn hierarchical_and_scoped_calls_capture_qualifier() {
        let src = "module m;\n  initial begin\n    int y;\n    y = mypkg::double_it(2);\n    pkt.send_it();\n  end\nendmodule\n";
        let facts = SystemVerilogExtractor.extract(src, "src/m.sv").unwrap();

        let scoped = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "double_it")
            .expect("expected a Call reference 'double_it'");
        assert_eq!(scoped.qualifier.as_deref(), Some("mypkg"));

        let method = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "send_it")
            .expect("expected a Call reference 'send_it'");
        assert_eq!(method.qualifier.as_deref(), Some("pkt"));
    }

    #[test]
    fn module_instantiation_is_type_ref() {
        let src =
            "module top;\n  logic [7:0] s;\n  adder u_adder(.a(s), .b(s), .sum(s));\nendmodule\n";
        let facts = SystemVerilogExtractor.extract(src, "src/top.sv").unwrap();
        let inst = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::TypeRef && r.name == "adder")
            .expect("expected a TypeRef reference 'adder'");
        assert_eq!(inst.type_ref_ctx, Some(TypeRefContext::Other));
        // The instance name is NOT a symbol (no elaboration semantics in v1).
        assert!(
            !facts.symbols.iter().any(|s| s.name == "u_adder"),
            "instance name must not become a symbol"
        );
    }

    #[test]
    fn instantiation_inside_generate_block_is_captured() {
        let src = "module gen_top;\n  generate\n    genvar i;\n    for (i = 0; i < 4; i = i + 1) begin : g\n      adder u_add(.a(i), .b(i), .sum());\n    end\n  endgenerate\nendmodule\n";
        let facts = SystemVerilogExtractor.extract(src, "src/gen.sv").unwrap();
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::TypeRef && r.name == "adder"),
            "expected the generate-block instantiation to emit a TypeRef"
        );
    }

    #[test]
    fn function_opens_function_scope_and_definition_bindings_exist() {
        let src =
            "module dut;\n  function int helper();\n    return 1;\n  endfunction\nendmodule\n";
        let facts = SystemVerilogExtractor.extract(src, "src/dut.sv").unwrap();

        assert_eq!(facts.scopes[0].kind, ScopeKind::Module);
        let fn_scope = facts
            .scopes
            .iter()
            .find(|s| s.kind == ScopeKind::Function)
            .expect("expected a Function scope");
        assert_eq!(fn_scope.parent, Some(0));

        assert!(
            facts
                .bindings
                .iter()
                .any(|b| b.kind == BindingKind::Definition && b.name == "helper"),
            "expected a Definition binding for 'helper'"
        );
    }
}
