// SPDX-License-Identifier: Apache-2.0

//! PowerShell extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: `function`/`filter` statements (both parse to `function_statement`
//! — the `filter` keyword doesn't change shape) and PS5+ `class` statements
//! (with their nested methods and, when present, a base-class reference).
//! References: calls in both cmdlet-style (`Verb-Noun -Arg x`) and
//! member/expression-style (`$obj.Method()`, `[Type]::Static()`) forms; all
//! three import forms (`Import-Module`, `using module`, dot-sourcing); variable
//! read/write.
//!
//! `Visibility` is always [`Visibility::Unknown`] — the language has no
//! in-source public/private signal (`Export-ModuleMember` is a runtime
//! convention, not syntax, and is deliberately not used to infer visibility).
//! Dynamic invocation (`Invoke-Expression`, `&$scriptBlock`) is an unresolved
//! ceiling — never guessed at. Names are emitted as written; PowerShell's
//! case-insensitivity is not normalized at the extraction layer (matches every
//! other extractor's as-written behavior — see 02-RESEARCH.md Open Question 1).
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.

use std::collections::HashSet;

use tree_sitter::{Language as TsLanguage, Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{
    ByteSpan, FileFacts, RefRole, Reference, Scope, ScopeId, ScopeKind, Symbol, SymbolKind,
    Visibility,
};
use crate::lang::Language;
use crate::symbol::Descriptor;

use super::{
    ExtractCtx, Extractor, MIN_REF_LEN, attach_reference_scopes, collect_call_references,
    definition_bindings, import_bindings, make_symbol, node_span, node_text, one_line_signature,
    push_import_ref, push_ref, push_scope,
};

/// Cmdlet-style call query — parenless, space-separated commands
/// (`Get-Process -Name notepad`). Verified against the pinned grammar in
/// 02-RESEARCH.md "Pattern 3".
const CMDLET_CALL_QUERY: &str = r#"(command command_name: (command_name) @callee)"#;

/// Member/expression-style call query — `.NET` interop and PS-object method
/// calls (`$obj.Method()`, `[System.IO.File]::ReadAllText($path)`). The
/// receiver (a `variable` or `type_literal`) is captured as `@qualifier`.
const MEMBER_CALL_QUERY: &str = r#"(invokation_expression [(variable) (type_literal)] @qualifier (member_name (simple_name) @callee))"#;

/// `Import-Module Foo` / `using module Foo` — both are generic `command`
/// nodes; classify by `command_name` text (see 02-RESEARCH.md "Pattern 4").
const IMPORT_ARG_QUERY: &str = r#"(command command_name: (command_name) @cmd command_elements: (command_elements (generic_token) @arg))"#;

/// Dot-sourcing (`. .\lib\helpers.ps1`) — distinguished by the presence of a
/// `command_invokation_operator` sibling and a wrapped `command_name_expr`.
const DOT_SOURCE_QUERY: &str = r#"(command (command_invokation_operator) command_name: (command_name_expr (command_name) @path))"#;

/// Extracts PowerShell symbols and references.
pub struct PowerShellExtractor;

impl Extractor for PowerShellExtractor {
    fn lang(&self) -> Language {
        Language::PowerShell
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_language = crate::grammar::powershell();
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
        let namespaces = powershell_namespaces(file);
        let ctx = ExtractCtx {
            bytes,
            file,
            lang: Language::PowerShell,
        };

        let defs = collect_symbols(&root, &ctx, &namespaces);
        let def_bindings = definition_bindings(&defs);
        let mut symbols = defs;
        let mod_sym = super::module_symbol(Language::PowerShell, &namespaces, file, source.len());
        let module_id = mod_sym.id.to_scip_string();
        symbols.push(mod_sym);

        // Import classification runs first so its byte offsets can be used to
        // filter the generic cmdlet-call query below (Pitfall 2 double-count guard).
        let mut references = Vec::new();
        let mut import_bytes: HashSet<usize> = HashSet::new();
        collect_imports(
            &root,
            &ts_language,
            ctx.bytes,
            ctx.file,
            &module_id,
            &mut references,
            &mut import_bytes,
        )?;

        let cmdlet_calls = collect_call_references(
            &root,
            &ts_language,
            CMDLET_CALL_QUERY,
            Language::PowerShell,
            ctx.bytes,
            ctx.file,
        )?;
        references.extend(
            cmdlet_calls
                .into_iter()
                .filter(|r| !import_bytes.contains(&r.occ.byte)),
        );

        let member_calls = collect_call_references(
            &root,
            &ts_language,
            MEMBER_CALL_QUERY,
            Language::PowerShell,
            ctx.bytes,
            ctx.file,
        )?;
        references.extend(member_calls);

        collect_inheritance(&root, ctx.bytes, ctx.file, &mut references);
        collect_read_write_references(&root, ctx.bytes, ctx.file, &mut references);

        let scopes = collect_scopes(&root, source.len());
        attach_reference_scopes(&mut references, &scopes);

        let mut bindings = def_bindings;
        bindings.extend(import_bindings(&references, &scopes));

        Ok(FileFacts {
            file: file.to_owned(),
            lang: Language::PowerShell.as_str().to_owned(),
            symbols,
            references,
            scopes,
            bindings,
            ffi_exports: Vec::new(),
        })
    }
}

/// Derive the PowerShell namespace path from a file path.
///
/// Strips a `.ps1`/`.psm1` extension; strips a leading `src/`, `bin/`, or
/// `scripts/` prefix (each tried in order); then splits on `/`.
fn powershell_namespaces(file: &str) -> Vec<String> {
    let p = file
        .strip_suffix(".ps1")
        .or_else(|| file.strip_suffix(".psm1"))
        .unwrap_or(file);
    let p = p
        .strip_prefix("src/")
        .or_else(|| p.strip_prefix("bin/"))
        .or_else(|| p.strip_prefix("scripts/"))
        .unwrap_or(p);
    p.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

// ── Definitions: function/filter + class/method ─────────────────────────────

fn collect_symbols(root: &Node, ctx: &ExtractCtx<'_>, namespaces: &[String]) -> Vec<Symbol> {
    let mut out = Vec::new();
    collect_symbols_dfs(root, ctx, namespaces, &mut out);
    out
}

/// DFS emitting Function symbols for `function_statement` nodes (covers both
/// `function` and `filter` — same node kind) and Class/Method symbols for
/// `class_statement` nodes. PowerShell allows nested function definitions, so
/// the walk continues into `function_statement` bodies; `class_statement`
/// handles its own method children directly (via [`collect_class`]) and does
/// not recurse further, matching the manual-walk approach required by
/// Pitfall/gotcha 2 (no combined query with an optional capture).
fn collect_symbols_dfs(
    node: &Node,
    ctx: &ExtractCtx<'_>,
    namespaces: &[String],
    out: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_statement" => {
            // NOTE: `function_name` is a positional child, NOT a named field —
            // `field_text(node, "name", ...)` (bash's convention) does not port.
            if let Some(name) = super::child_text(node, "function_name", ctx.bytes) {
                let mut descriptors: Vec<Descriptor> = namespaces
                    .iter()
                    .cloned()
                    .map(Descriptor::Namespace)
                    .collect();
                descriptors.push(Descriptor::Method {
                    name: name.clone(),
                    disambiguator: String::new(),
                });
                let signature = one_line_signature(node_text(node, ctx.bytes), &['{']);
                out.push(make_symbol(
                    ctx,
                    node,
                    name,
                    SymbolKind::Function,
                    Visibility::Unknown,
                    descriptors,
                    signature,
                ));
            }
        }
        "class_statement" => {
            collect_class(node, ctx, namespaces, out);
            return; // class body handled by collect_class; don't double-walk it.
        }
        _ => {}
    }
    for child in node.children(&mut node.walk()) {
        collect_symbols_dfs(&child, ctx, namespaces, out);
    }
}

/// Emit a Class symbol for `class_statement` plus a Method symbol for each
/// `class_method_definition` child. The class name is the FIRST `simple_name`
/// child (no field); a base-class reference (if present) is handled
/// separately by [`collect_inheritance`] to avoid the verified duplicate-match
/// query gotcha (02-RESEARCH.md Pattern 2).
fn collect_class(node: &Node, ctx: &ExtractCtx<'_>, namespaces: &[String], out: &mut Vec<Symbol>) {
    let Some(class_name_node) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "simple_name")
    else {
        return;
    };
    let class_name = node_text(&class_name_node, ctx.bytes).to_owned();

    let mut class_descriptors: Vec<Descriptor> = namespaces
        .iter()
        .cloned()
        .map(Descriptor::Namespace)
        .collect();
    class_descriptors.push(Descriptor::Type(class_name.clone()));
    let class_signature = one_line_signature(node_text(node, ctx.bytes), &['{']);
    out.push(make_symbol(
        ctx,
        node,
        class_name.clone(),
        SymbolKind::Class,
        Visibility::Unknown,
        class_descriptors,
        class_signature,
    ));

    for child in node.children(&mut node.walk()) {
        if child.kind() != "class_method_definition" {
            continue;
        }
        let Some(method_name_node) = child
            .children(&mut child.walk())
            .find(|c| c.kind() == "simple_name")
        else {
            continue;
        };
        let method_name = node_text(&method_name_node, ctx.bytes).to_owned();
        let mut method_descriptors: Vec<Descriptor> = namespaces
            .iter()
            .cloned()
            .map(Descriptor::Namespace)
            .collect();
        method_descriptors.push(Descriptor::Type(class_name.clone()));
        method_descriptors.push(Descriptor::Method {
            name: method_name.clone(),
            disambiguator: String::new(),
        });
        let method_signature = one_line_signature(node_text(&child, ctx.bytes), &['{']);
        out.push(make_symbol(
            ctx,
            &child,
            method_name,
            SymbolKind::Method,
            Visibility::Unknown,
            method_descriptors,
            method_signature,
        ));
    }
}

// ── Inheritance (manual walk — NOT a query, see Pattern 2 gotcha) ───────────

/// Walk the tree and emit an `IsImplementation` reference for a class's base
/// type: `class_statement`'s SECOND `simple_name` child (only present when a
/// `:` base-class token is present). Mirrors `csharp.rs::collect_inheritance`'s
/// `push_ref` shape but with PowerShell's manual node-walk (a single combined
/// query with an optional capture produces spurious duplicate matches —
/// verified in 02-RESEARCH.md).
fn collect_inheritance(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    if node.kind() == "class_statement" {
        let mut walker = node.walk();
        let mut simple_names = node
            .children(&mut walker)
            .filter(|c| c.kind() == "simple_name");
        let _class_name = simple_names.next();
        if let Some(base_name_node) = simple_names.next() {
            let base_name = node_text(&base_name_node, bytes);
            push_ref(
                out,
                base_name,
                &base_name_node,
                file,
                RefRole::IsImplementation,
            );
        }
    }
    for child in node.children(&mut node.walk()) {
        collect_inheritance(&child, bytes, file, out);
    }
}

// ── Edge richness: Read / Write ─────────────────────────────────────────────

/// Returns `true` when `node` has an ancestor of kind `kind` (searched via the
/// full parent chain — PowerShell's grammar wraps every sub-expression in a
/// full operator-precedence chain, so a `variable` leaf's *immediate* parent
/// is always an intermediate wrapper like `unary_expression`, never
/// `left_assignment_expression`/`script_parameter` directly; the ancestor
/// check is required to see past that wrapping).
fn has_ancestor_kind(node: &Node, kind: &str) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        if p.kind() == kind {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// Recursively walk `node` and emit [`RefRole::Write`] for every `variable`
/// node nested under a `left_assignment_expression` (the LHS of an
/// `assignment_expression`), and [`RefRole::Read`] for every other bare
/// `variable` node — except one nested under `script_parameter`, which is a
/// parameter *declaration*, not a read (Pitfall 4). Applies [`MIN_REF_LEN`] to
/// the name with its `$` sigil stripped.
fn collect_read_write_references(node: &Node, bytes: &[u8], file: &str, out: &mut Vec<Reference>) {
    if node.kind() == "variable" {
        if has_ancestor_kind(node, "script_parameter") {
            return; // parameter declaration binding, not a read or write.
        }
        let name = node_text(node, bytes).trim_start_matches('$');
        if name.len() < MIN_REF_LEN {
            return;
        }
        let role = if has_ancestor_kind(node, "left_assignment_expression") {
            RefRole::Write
        } else {
            RefRole::Read
        };
        push_ref(out, name, node, file, role);
        // `variable` has no meaningful children; return early.
        return;
    }
    for child in node.children(&mut node.walk()) {
        collect_read_write_references(&child, bytes, file, out);
    }
}

// ── Scope tree (Tier-B) ──────────────────────────────────────────────────────

/// Build the lexical scope tree for one PowerShell file.
///
/// `scopes[0]` is always the file-root `Module` scope spanning `[0,
/// source_len)`. `function_statement` and `class_method_definition` nodes each
/// open a `Function` scope spanning their own node span; `class_statement`
/// itself does NOT open a new scope (no v1 Class scope) — its children are
/// visited under the enclosing scope so nested `class_method_definition`
/// nodes still open their own Function scopes.
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

/// DFS opening `Function` scopes for `function_statement` and
/// `class_method_definition` nodes; all other node kinds (including
/// `class_statement`) simply recurse without opening a new scope.
fn scope_dfs(node: &Node, parent_id: ScopeId, scopes: &mut Vec<Scope>) {
    if matches!(
        node.kind(),
        "function_statement" | "class_method_definition"
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

// ── Imports: Import-Module / using module / dot-sourcing ────────────────────

/// Classify all three PowerShell import forms, all of which parse to generic
/// `command` nodes (the grammar has no dedicated import node — see
/// 02-RESEARCH.md Pattern 4). Populates `import_bytes` with the `start_byte()`
/// of every `command_name` node classified as `Import-Module`/`using module`,
/// so the caller can exclude the matching cmdlet-call-query match (Pitfall 2).
fn collect_imports(
    root: &Node,
    ts_lang: &TsLanguage,
    bytes: &[u8],
    file: &str,
    module_id: &str,
    out: &mut Vec<Reference>,
    import_bytes: &mut HashSet<usize>,
) -> Result<()> {
    let arg_query = Query::new(ts_lang, IMPORT_ARG_QUERY).map_err(|e| CodegraphError::Query {
        lang: Language::PowerShell.as_str().to_owned(),
        msg: e.to_string(),
    })?;
    let cmd_idx = arg_query
        .capture_index_for_name("cmd")
        .ok_or_else(|| CodegraphError::Query {
            lang: Language::PowerShell.as_str().to_owned(),
            msg: "missing @cmd capture".to_owned(),
        })?;
    let arg_idx = arg_query
        .capture_index_for_name("arg")
        .ok_or_else(|| CodegraphError::Query {
            lang: Language::PowerShell.as_str().to_owned(),
            msg: "missing @arg capture".to_owned(),
        })?;

    // The query's `(generic_token) @arg` pattern matches once PER generic_token
    // sibling (a quantifier-style repeat), so a command with N arguments produces
    // N separate matches, each pairing the same `command_name` node with exactly
    // one `@arg` — NOT one match with all N args together. Group by the command
    // node's byte offset (stable across its repeated matches) to reassemble the
    // full, ordered argument list per command.
    let mut by_command: Vec<(Node, Vec<Node>)> = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&arg_query, *root, bytes);
    while let Some(m) = matches.next() {
        let Some(cmd_cap) = m.captures.iter().find(|c| c.index == cmd_idx) else {
            continue;
        };
        let Some(arg_cap) = m.captures.iter().find(|c| c.index == arg_idx) else {
            continue;
        };
        match by_command
            .iter_mut()
            .find(|(cmd, _)| cmd.start_byte() == cmd_cap.node.start_byte())
        {
            Some((_, args)) => args.push(arg_cap.node),
            None => by_command.push((cmd_cap.node, vec![arg_cap.node])),
        }
    }

    for (cmd_node, args) in &by_command {
        let cmd_text = node_text(cmd_node, bytes);

        if cmd_text.eq_ignore_ascii_case("Import-Module") {
            if let Some(first) = args.first() {
                let name = node_text(first, bytes).to_owned();
                push_import_ref(out, &name, first, file, module_id, &name);
                import_bytes.insert(cmd_node.start_byte());
            }
        } else if cmd_text.eq_ignore_ascii_case("using") {
            if let Some(first) = args.first() {
                if node_text(first, bytes).eq_ignore_ascii_case("module") {
                    if let Some(second) = args.get(1) {
                        let name = node_text(second, bytes).to_owned();
                        push_import_ref(out, &name, second, file, module_id, &name);
                        import_bytes.insert(cmd_node.start_byte());
                    }
                }
            }
        }
    }

    let dot_query = Query::new(ts_lang, DOT_SOURCE_QUERY).map_err(|e| CodegraphError::Query {
        lang: Language::PowerShell.as_str().to_owned(),
        msg: e.to_string(),
    })?;
    let path_idx =
        dot_query
            .capture_index_for_name("path")
            .ok_or_else(|| CodegraphError::Query {
                lang: Language::PowerShell.as_str().to_owned(),
                msg: "missing @path capture".to_owned(),
            })?;

    let mut dot_cursor = QueryCursor::new();
    let mut dot_matches = dot_cursor.matches(&dot_query, *root, bytes);
    while let Some(m) = dot_matches.next() {
        for cap in m.captures.iter().filter(|c| c.index == path_idx) {
            let name = node_text(&cap.node, bytes).to_owned();
            push_import_ref(out, &name, &cap.node, file, module_id, &name);
            // Dot-sourcing's wrapped `command_name_expr(command_name)` shape does
            // NOT match CMDLET_CALL_QUERY's bare `(command_name)` field pattern,
            // so no double-count guard entry is needed here (verified in research).
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::RefRole;

    #[test]
    fn extracts_function_statement() {
        let src = "function Get-Helper { return $true }\n";
        let facts = PowerShellExtractor
            .extract(src, "scripts/deploy.ps1")
            .unwrap();
        let helper = facts
            .symbols
            .iter()
            .find(|s| s.name == "Get-Helper")
            .expect("expected a Function symbol named 'Get-Helper'");
        assert_eq!(helper.kind, SymbolKind::Function);
        assert_eq!(helper.visibility, Visibility::Unknown);
        assert_eq!(
            helper.id.to_scip_string(),
            "codegraph . . . deploy/Get-Helper()."
        );
    }

    #[test]
    fn extracts_filter_statement() {
        let src = "filter Get-Bar { $_ }\n";
        let facts = PowerShellExtractor
            .extract(src, "scripts/deploy.ps1")
            .unwrap();
        assert!(facts.symbols.iter().any(|s| s.name == "Get-Bar"));
    }

    #[test]
    fn extracts_class_and_method() {
        let src = "class Animal { [string]$Name Speak() { return \"hi\" } }\n";
        let facts = PowerShellExtractor
            .extract(src, "scripts/models.ps1")
            .unwrap();
        let animal = facts
            .symbols
            .iter()
            .find(|s| s.name == "Animal")
            .expect("expected a Class symbol 'Animal'");
        assert_eq!(animal.kind, SymbolKind::Class);
        assert_eq!(animal.id.to_scip_string(), "codegraph . . . models/Animal#");

        let speak = facts
            .symbols
            .iter()
            .find(|s| s.name == "Speak")
            .expect("expected a Method symbol 'Speak'");
        assert_eq!(speak.kind, SymbolKind::Method);
        assert_eq!(
            speak.id.to_scip_string(),
            "codegraph . . . models/Animal#Speak()."
        );
    }

    #[test]
    fn class_inheritance_emits_is_implementation() {
        let src = "class Dog : Animal { Speak() { } }\n";
        let facts = PowerShellExtractor
            .extract(src, "scripts/models.ps1")
            .unwrap();
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::IsImplementation && r.name == "Animal"),
            "expected an IsImplementation reference to 'Animal'"
        );
    }

    #[test]
    fn cmdlet_style_calls_both_pipeline_stages() {
        let src = "Get-Process -Name notepad | Stop-Process\n";
        let facts = PowerShellExtractor.extract(src, "scripts/run.ps1").unwrap();
        let names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Call)
            .map(|r| r.name.as_str())
            .collect();
        assert!(names.contains(&"Get-Process"));
        assert!(names.contains(&"Stop-Process"));
    }

    #[test]
    fn member_style_calls_capture_qualifier() {
        let src = "$obj.Method()\n[System.IO.File]::ReadAllText($path)\n";
        let facts = PowerShellExtractor.extract(src, "scripts/run.ps1").unwrap();

        let method_call = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "Method")
            .expect("expected a Call reference 'Method'");
        assert_eq!(method_call.qualifier.as_deref(), Some("$obj"));

        let static_call = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "ReadAllText")
            .expect("expected a Call reference 'ReadAllText'");
        assert_eq!(static_call.qualifier.as_deref(), Some("[System.IO.File]"));
    }

    #[test]
    fn import_module_is_import_not_call() {
        let src = "Import-Module MyModule\n";
        let facts = PowerShellExtractor.extract(src, "scripts/run.ps1").unwrap();
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Import && r.name == "MyModule")
        );
        assert!(
            !facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Call && r.name.eq_ignore_ascii_case("Import-Module")),
            "Import-Module must not also emit a spurious Call reference"
        );
    }

    #[test]
    fn using_module_is_import_not_call() {
        let src = "using module MyModule\n";
        let facts = PowerShellExtractor.extract(src, "scripts/run.ps1").unwrap();
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Import && r.name == "MyModule")
        );
        assert!(
            !facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Call && r.name.eq_ignore_ascii_case("using")),
            "using module must not also emit a spurious Call reference"
        );
    }

    #[test]
    fn dot_sourcing_is_import_not_call() {
        let src = ". .\\lib\\helpers.ps1\n";
        let facts = PowerShellExtractor.extract(src, "scripts/run.ps1").unwrap();
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Import && r.name == ".\\lib\\helpers.ps1")
        );
        assert!(
            !facts.references.iter().any(|r| r.role == RefRole::Call),
            "dot-sourcing must not also emit a spurious Call reference"
        );
    }

    // ── Edge richness: Read / Write ──────────────────────────────────────────

    #[test]
    fn assignment_write_and_read() {
        // `$conf = 1; $result = $conf` → Write "conf", Read "conf".
        let src = "function setup {\n  $conf = 1\n  $result = $conf\n}\n";
        let facts = PowerShellExtractor
            .extract(src, "scripts/setup.ps1")
            .unwrap();

        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Write && r.name == "conf"),
            "expected a Write ref for 'conf', got: {:?}",
            facts
                .references
                .iter()
                .map(|r| (&r.name, r.role))
                .collect::<Vec<_>>()
        );
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Read && r.name == "conf"),
            "expected a Read ref for 'conf', got: {:?}",
            facts
                .references
                .iter()
                .map(|r| (&r.name, r.role))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn param_declaration_is_not_read_or_write() {
        // `param([string]$Name)` — $Name is a parameter binding, not a Read/Write.
        let src = "function Get-Foo {\n  param([string]$Name)\n  return $Name\n}\n";
        let facts = PowerShellExtractor.extract(src, "scripts/foo.ps1").unwrap();

        // The `return $Name` usage IS a Read (a bare variable reference outside
        // `script_parameter`); only the param-declaration occurrence is excluded.
        let name_refs: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.name == "Name")
            .collect();
        assert_eq!(
            name_refs.len(),
            1,
            "expected exactly one Read ref for 'Name' (from `return $Name`), got {:?}",
            name_refs
        );
        assert_eq!(name_refs[0].role, RefRole::Read);
    }

    #[test]
    fn function_statement_opens_function_scope() {
        let src = "function Get-Helper {\n  $conf = 1\n}\n";
        let facts = PowerShellExtractor
            .extract(src, "scripts/deploy.ps1")
            .unwrap();

        assert_eq!(
            facts.scopes[0].kind,
            crate::graph::types::ScopeKind::Module,
            "scopes[0] must be Module"
        );
        let fn_scope = facts
            .scopes
            .iter()
            .find(|s| s.kind == crate::graph::types::ScopeKind::Function)
            .expect("expected a Function scope");
        assert_eq!(fn_scope.parent, Some(0));
    }
}
