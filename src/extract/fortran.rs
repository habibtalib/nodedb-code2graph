// SPDX-License-Identifier: Apache-2.0

//! Fortran extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: `function` / `subroutine` program units — both at the top level
//! (external procedures), inside a `module` / `program` specification part's
//! `contains` section, and nested procedure-internal `contains` sections.
//! The file's module symbol is named after the first `module` / `program` unit
//! (falling back to the file path when neither is present).
//! References: `call` statements (`subroutine_call`) and function invocations
//! (`call_expression`) as Calls — type-bound calls (`obj%method(...)`) carry the
//! receiver as the reference's `qualifier` — and `use` statements as Imports
//! (bare `use m` imports the module; `use m, only: a, b` imports each listed name
//! with `from_path = "m"`).
//!
//! `Visibility` is real, not guessed: inside a `module` the Fortran default is
//! public, a bare `private` statement flips that default, and explicit
//! `public :: name` / `private :: name` statements override per name
//! (matched case-insensitively, per Fortran semantics; symbol names themselves
//! are emitted as written, consistent with every other extractor). Procedures
//! internal to a `program` or to another procedure (`contains` nesting) are
//! host-only by the language rules → [`Visibility::Private`]. External top-level
//! procedures have global linkage → [`Visibility::Public`].
//!
//! Honest ceilings (documented, never guessed past):
//! - Legacy fixed-form `.f` dispatches here and is capped at whatever the
//!   grammar yields — pre-F90 sources have no modules, no `use`, no
//!   public/private statements, so those facts are simply absent.
//! - Fortran's `name(args)` syntax is ambiguous between a function call and an
//!   array element access; `call_expression` covers both, so some Call
//!   references are really array accesses (they stay unresolved downstream).
//! - No generic-interface resolution (`interface` blocks are not modeled), no
//!   submodules, and no derived-type / module-variable symbols in v1.
//! - Multi-unit files: only the first `module` / `program` names the file's
//!   module symbol; later units' procedures still get their own unit's
//!   namespace descriptor.
//! - Fortran requires declarations before executable statements in a scoping
//!   unit; the grammar emits ERROR nodes when that language rule is violated —
//!   that is genuinely malformed input, not an extractor gap.
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.

use std::collections::HashMap;

use tree_sitter::{Node, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{
    Binding, BindingKind, ByteSpan, FileFacts, Reference, Scope, ScopeId, ScopeKind, Symbol,
    SymbolKind, Visibility,
};
use crate::lang::Language;
use crate::symbol::Descriptor;

use super::{
    ExtractCtx, Extractor, MIN_REF_LEN, attach_reference_scopes, collect_call_references,
    definition_bindings, import_bindings, innermost_scope, make_symbol, node_span, node_text,
    one_line_signature, push_binding, push_import_ref, push_scope,
};

/// Tree-sitter query capturing call-callee identifiers.
///
/// Pattern 1: `call helper(x)` — `subroutine_call`'s `subroutine:` field.
/// Pattern 2: `call obj%run(x)` — type-bound subroutine call; the receiver is
///            captured as `@qualifier`, the `type_member` as `@callee`.
/// Pattern 3: `r = add(1, 2)` — `call_expression` with a bare identifier callee
///            (its first child is positional, NOT a named field).
/// Pattern 4: `v = obj%get_value()` — type-bound function call.
///
/// Only single-level receivers (`obj%method`) capture a qualifier; deeper
/// chains (`a%b%method`) nest `derived_type_member_expression` nodes and are
/// not matched (a small, honest recall gap).
const CALL_QUERY: &str = r#"
[
  (subroutine_call subroutine: (identifier) @callee)
  (subroutine_call subroutine: (derived_type_member_expression (identifier) @qualifier (type_member) @callee))
  (call_expression (identifier) @callee (argument_list))
  (call_expression (derived_type_member_expression (identifier) @qualifier (type_member) @callee) (argument_list))
]
"#;

/// Extracts Fortran symbols and references.
pub struct FortranExtractor;

impl Extractor for FortranExtractor {
    fn lang(&self) -> Language {
        Language::Fortran
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_language = crate::grammar::fortran();
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
        let namespaces = fortran_namespaces(&root, bytes, file);
        let ctx = ExtractCtx {
            bytes,
            file,
            lang: Language::Fortran,
        };

        let defs = collect_symbols(&root, &ctx, &namespaces);
        let def_bindings = definition_bindings(&defs);
        let mut symbols = defs;
        let mod_sym = super::module_symbol(Language::Fortran, &namespaces, file, source.len());
        let module_id = mod_sym.id.to_scip_string();
        symbols.push(mod_sym);

        let mut references = collect_call_references(
            &root,
            &ts_language,
            CALL_QUERY,
            Language::Fortran,
            bytes,
            file,
        )?;
        collect_imports(&root, bytes, file, &module_id, &mut references);

        let scopes = collect_scopes(&root, source.len());
        attach_reference_scopes(&mut references, &scopes);

        let mut bindings = def_bindings;
        bindings.extend(import_bindings(&references, &scopes));
        collect_param_bindings(&root, bytes, &scopes, &mut bindings);

        Ok(FileFacts {
            file: file.to_owned(),
            lang: Language::Fortran.as_str().to_owned(),
            symbols,
            references,
            scopes,
            bindings,
            ffi_exports: Vec::new(),
        })
    }
}

// ── Namespace derivation ─────────────────────────────────────────────────────

/// Derive the file's namespace from its first `module` / `program` unit name
/// (`module mymod` → `["mymod"]`). Falls back to a path-derived namespace when
/// the file has neither (e.g. a file of external procedures): strip a
/// `.f90` / `.f` extension and a leading `src/`, then split on `/`.
fn fortran_namespaces(root: &Node, bytes: &[u8], file: &str) -> Vec<String> {
    for top in root.children(&mut root.walk()) {
        if matches!(top.kind(), "module" | "program") {
            if let Some(name) = unit_name(&top, bytes) {
                return vec![name];
            }
        }
    }

    let p = file
        .strip_suffix(".f90")
        .or_else(|| file.strip_suffix(".f"))
        .unwrap_or(file);
    let p = p.strip_prefix("src/").unwrap_or(p);
    p.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The unit name of a `module` / `program` node.
///
/// NOTE: `module_statement` / `program_statement` carry their name as a bare
/// positional `(name)` child with NO field label — unlike
/// `function_statement` / `subroutine_statement`, where `name:` IS a named
/// field. This asymmetry is real (verified against tree-sitter-fortran 0.6.0).
fn unit_name(unit: &Node, bytes: &[u8]) -> Option<String> {
    for child in unit.children(&mut unit.walk()) {
        if matches!(child.kind(), "module_statement" | "program_statement") {
            return child
                .children(&mut child.walk())
                .find(|c| c.kind() == "name")
                .map(|c| node_text(&c, bytes).to_owned());
        }
    }
    None
}

// ── Visibility environment ───────────────────────────────────────────────────

/// The accessibility rules in force inside one `module`: a default (public by
/// language rule; flipped by a bare `private` statement) plus per-name
/// overrides from explicit `public :: a, b` / `private :: c` statements.
/// Override keys are lowercased — Fortran identifiers are case-insensitive.
struct VisibilityEnv {
    default: Visibility,
    overrides: HashMap<String, Visibility>,
}

impl VisibilityEnv {
    fn of(&self, name: &str) -> Visibility {
        self.overrides
            .get(&name.to_ascii_lowercase())
            .copied()
            .unwrap_or(self.default)
    }
}

/// Scan a unit's direct children for `public_statement` / `private_statement`
/// nodes. A statement with no identifier children sets the unit default; one
/// with identifiers records a per-name override.
fn unit_visibility_env(unit: &Node, bytes: &[u8], default: Visibility) -> VisibilityEnv {
    let mut env = VisibilityEnv {
        default,
        overrides: HashMap::new(),
    };
    for child in unit.children(&mut unit.walk()) {
        let vis = match child.kind() {
            "public_statement" => Visibility::Public,
            "private_statement" => Visibility::Private,
            _ => continue,
        };
        let mut had_names = false;
        for id in child.children(&mut child.walk()) {
            if id.kind() == "identifier" {
                had_names = true;
                env.overrides
                    .insert(node_text(&id, bytes).to_ascii_lowercase(), vis);
            }
        }
        if !had_names {
            env.default = vis;
        }
    }
    env
}

// ── Symbol collection ────────────────────────────────────────────────────────

fn collect_symbols(root: &Node, ctx: &ExtractCtx, file_ns: &[String]) -> Vec<Symbol> {
    let mut out = Vec::new();
    for top in root.children(&mut root.walk()) {
        match top.kind() {
            "module" | "program" => {
                let ns = unit_name(&top, ctx.bytes)
                    .map(|n| vec![n])
                    .unwrap_or_else(|| file_ns.to_vec());
                // Module default accessibility is public (per the language);
                // program-internal procedures are host-only → private.
                let default = if top.kind() == "module" {
                    Visibility::Public
                } else {
                    Visibility::Private
                };
                let env = unit_visibility_env(&top, ctx.bytes, default);
                collect_unit_procs(&top, ctx, &ns, &env, &mut out);
            }
            // External (top-level) procedures have global linkage → public.
            "function" | "subroutine" => {
                emit_procedure(&top, ctx, file_ns, Visibility::Public, &mut out);
            }
            _ => {}
        }
    }
    out
}

/// Emit symbols for the procedures of one `module` / `program` unit: direct
/// `function` / `subroutine` children plus those under the unit's
/// `internal_procedures` (`contains`) section.
fn collect_unit_procs(
    unit: &Node,
    ctx: &ExtractCtx,
    ns: &[String],
    env: &VisibilityEnv,
    out: &mut Vec<Symbol>,
) {
    for child in unit.children(&mut unit.walk()) {
        match child.kind() {
            "function" | "subroutine" => {
                emit_unit_proc(&child, ctx, ns, env, out);
            }
            "internal_procedures" => {
                for proc in child.children(&mut child.walk()) {
                    if matches!(proc.kind(), "function" | "subroutine") {
                        emit_unit_proc(&proc, ctx, ns, env, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Emit one unit procedure, resolving its visibility from the unit's
/// [`VisibilityEnv`] by name.
fn emit_unit_proc(
    node: &Node,
    ctx: &ExtractCtx,
    ns: &[String],
    env: &VisibilityEnv,
    out: &mut Vec<Symbol>,
) {
    let vis = procedure_name(node, ctx.bytes)
        .map(|n| env.of(&n))
        .unwrap_or(env.default);
    emit_procedure(node, ctx, ns, vis, out);
}

/// The declared name of a `function` / `subroutine` node — its header
/// statement's `name:` field.
fn procedure_name(node: &Node, bytes: &[u8]) -> Option<String> {
    let header = node
        .children(&mut node.walk())
        .find(|c| matches!(c.kind(), "function_statement" | "subroutine_statement"))?;
    header
        .child_by_field_name("name")
        .map(|n| node_text(&n, bytes).to_owned())
}

/// Emit a [`SymbolKind::Function`] for a `function` / `subroutine` node, then
/// recurse into its own `internal_procedures` (`contains`) section — internal
/// procedures are host-only by the language rules → [`Visibility::Private`].
/// Descriptors stay flat under the unit namespace (matching the PowerShell
/// nested-function precedent).
fn emit_procedure(
    node: &Node,
    ctx: &ExtractCtx,
    ns: &[String],
    vis: Visibility,
    out: &mut Vec<Symbol>,
) {
    let Some(header) = node
        .children(&mut node.walk())
        .find(|c| matches!(c.kind(), "function_statement" | "subroutine_statement"))
    else {
        return;
    };
    if let Some(name) = header
        .child_by_field_name("name")
        .map(|n| node_text(&n, ctx.bytes).to_owned())
    {
        let mut descriptors: Vec<Descriptor> =
            ns.iter().cloned().map(Descriptor::Namespace).collect();
        descriptors.push(Descriptor::Method {
            name: name.clone(),
            disambiguator: String::new(),
        });
        let signature = one_line_signature(node_text(&header, ctx.bytes), &[]);
        out.push(make_symbol(
            ctx,
            node,
            name,
            SymbolKind::Function,
            vis,
            descriptors,
            signature,
        ));
    }

    for child in node.children(&mut node.walk()) {
        if child.kind() == "internal_procedures" {
            for inner in child.children(&mut child.walk()) {
                if matches!(inner.kind(), "function" | "subroutine") {
                    emit_procedure(&inner, ctx, ns, Visibility::Private, out);
                }
            }
        }
    }
}

// ── Imports (use statements) ─────────────────────────────────────────────────

/// Walk the tree emitting Import refs for `use_statement` nodes.
///
/// `use mymod` → one Import for the module name (whole-module import).
/// `use other_mod, only: a, b` → one Import per listed name, each carrying
/// `from_path = "other_mod"` (genuinely symbol-level imports).
fn collect_imports(
    node: &Node,
    bytes: &[u8],
    file: &str,
    module_id: &str,
    out: &mut Vec<Reference>,
) {
    if node.kind() == "use_statement" {
        let mut module_node = None;
        let mut included = None;
        for child in node.children(&mut node.walk()) {
            match child.kind() {
                "module_name" => module_node = Some(child),
                "included_items" => included = Some(child),
                _ => {}
            }
        }
        if let Some(m) = module_node {
            let module = node_text(&m, bytes).to_owned();
            match included {
                Some(items) => {
                    for id in items.children(&mut items.walk()) {
                        if id.kind() == "identifier" {
                            push_import_ref(
                                out,
                                node_text(&id, bytes),
                                &id,
                                file,
                                module_id,
                                &module,
                            );
                        }
                    }
                }
                None => push_import_ref(out, &module, &m, file, module_id, &module),
            }
        }
        return;
    }
    for child in node.children(&mut node.walk()) {
        collect_imports(&child, bytes, file, module_id, out);
    }
}

// ── Scope tree (Tier-B) ──────────────────────────────────────────────────────

/// Build the lexical scope tree: `scopes[0]` is the file-root `Module` scope;
/// each `module` / `program` unit opens a `Module` scope and each
/// `function` / `subroutine` opens a `Function` scope.
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
        "module" | "program" => {
            let mod_id = push_scope(scopes, Some(parent_id), node_span(node), ScopeKind::Module);
            for child in node.children(&mut node.walk()) {
                scope_dfs(&child, mod_id, scopes);
            }
        }
        "function" | "subroutine" => {
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
        _ => {
            for child in node.children(&mut node.walk()) {
                scope_dfs(&child, parent_id, scopes);
            }
        }
    }
}

// ── Bindings ─────────────────────────────────────────────────────────────────

/// Emit a [`BindingKind::Param`] binding for every dummy-argument identifier in
/// a procedure header's `parameters:` field. Applies [`MIN_REF_LEN`]; skips
/// anything that would land in the file-root scope (a malformed header).
fn collect_param_bindings(node: &Node, bytes: &[u8], scopes: &[Scope], out: &mut Vec<Binding>) {
    if matches!(node.kind(), "function_statement" | "subroutine_statement") {
        if let Some(params) = node.child_by_field_name("parameters") {
            for p in params.children(&mut params.walk()) {
                if p.kind() == "identifier" {
                    let name = node_text(&p, bytes).to_owned();
                    let intro = p.start_byte();
                    if name.len() >= MIN_REF_LEN && innermost_scope(intro, scopes) != Some(0) {
                        push_binding(out, name, intro, BindingKind::Param, scopes);
                    }
                }
            }
        }
    }
    for child in node.children(&mut node.walk()) {
        collect_param_bindings(&child, bytes, scopes, out);
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::RefRole;

    fn extract(src: &str, file: &str) -> FileFacts {
        FortranExtractor.extract(src, file).unwrap()
    }

    fn by_name(facts: &FileFacts, name: &str) -> Option<Symbol> {
        facts.symbols.iter().find(|s| s.name == name).cloned()
    }

    const MODULE_SRC: &str = r#"module mymod
  implicit none
  public :: add
  private :: helper
contains
  function add(a, b) result(c)
    integer, intent(in) :: a, b
    integer :: c
    c = a + b
  end function add
  subroutine helper(x)
    integer, intent(in) :: x
  end subroutine helper
end module mymod
"#;

    // ── Definitions ──────────────────────────────────────────────────────────

    #[test]
    fn module_function_and_subroutine_get_correct_scip_strings() {
        let facts = extract(MODULE_SRC, "src/mymod.f90");

        let add = by_name(&facts, "add").unwrap();
        assert_eq!(add.kind, SymbolKind::Function);
        assert_eq!(add.id.to_scip_string(), "codegraph . . . mymod/add().");
        assert_eq!(add.line, 6, "symbol line is the procedure's header line");

        let helper = by_name(&facts, "helper").unwrap();
        assert_eq!(helper.kind, SymbolKind::Function);
        assert_eq!(
            helper.id.to_scip_string(),
            "codegraph . . . mymod/helper()."
        );

        assert_eq!(facts.lang, "fortran");
    }

    #[test]
    fn emits_module_symbol_named_after_the_module_unit() {
        let facts = extract(MODULE_SRC, "src/mymod.f90");
        let module_syms: Vec<_> = facts
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module)
            .collect();
        assert_eq!(module_syms.len(), 1, "expected exactly one Module symbol");
        assert_eq!(module_syms[0].name, "mymod");
        assert_eq!(module_syms[0].id.to_scip_string(), "codegraph . . . mymod/");
    }

    #[test]
    fn external_procedure_uses_path_namespace_and_is_public() {
        let src = r#"subroutine standalone(x)
  integer, intent(in) :: x
end subroutine standalone
"#;
        let facts = extract(src, "src/utils.f90");
        let standalone = by_name(&facts, "standalone").unwrap();
        assert_eq!(standalone.kind, SymbolKind::Function);
        assert_eq!(standalone.visibility, Visibility::Public);
        assert_eq!(
            standalone.id.to_scip_string(),
            "codegraph . . . utils/standalone()."
        );
    }

    #[test]
    fn nested_contains_procedure_is_extracted_and_private() {
        let src = r#"module m2
contains
  function outer() result(r)
    integer :: r
    r = inner()
  contains
    function inner() result(i)
      integer :: i
      i = 1
    end function inner
  end function outer
end module m2
"#;
        let facts = extract(src, "src/m2.f90");
        let inner = by_name(&facts, "inner").expect("nested internal procedure must be extracted");
        assert_eq!(inner.kind, SymbolKind::Function);
        assert_eq!(
            inner.visibility,
            Visibility::Private,
            "procedure-internal (contains) procedures are host-only"
        );
        assert_eq!(inner.id.to_scip_string(), "codegraph . . . m2/inner().");
    }

    // ── Visibility ───────────────────────────────────────────────────────────

    #[test]
    fn explicit_public_private_statements_set_real_visibility() {
        let facts = extract(MODULE_SRC, "src/mymod.f90");
        assert_eq!(
            by_name(&facts, "add").unwrap().visibility,
            Visibility::Public
        );
        assert_eq!(
            by_name(&facts, "helper").unwrap().visibility,
            Visibility::Private
        );
    }

    #[test]
    fn module_default_visibility_is_public() {
        let src = r#"module m
contains
  subroutine anything()
  end subroutine anything
end module m
"#;
        let facts = extract(src, "src/m.f90");
        assert_eq!(
            by_name(&facts, "anything").unwrap().visibility,
            Visibility::Public
        );
    }

    #[test]
    fn bare_private_statement_flips_the_module_default() {
        let src = r#"module m
  private
  public :: exposed
contains
  subroutine exposed()
  end subroutine exposed
  subroutine hidden()
  end subroutine hidden
end module m
"#;
        let facts = extract(src, "src/m.f90");
        assert_eq!(
            by_name(&facts, "exposed").unwrap().visibility,
            Visibility::Public
        );
        assert_eq!(
            by_name(&facts, "hidden").unwrap().visibility,
            Visibility::Private
        );
    }

    #[test]
    fn visibility_override_matches_case_insensitively() {
        // Fortran identifiers are case-insensitive: `private :: Helper` must
        // hit `subroutine helper`. The symbol name stays as written.
        let src = r#"module m
  private :: Helper
contains
  subroutine helper()
  end subroutine helper
end module m
"#;
        let facts = extract(src, "src/m.f90");
        let helper = by_name(&facts, "helper").unwrap();
        assert_eq!(helper.visibility, Visibility::Private);
    }

    #[test]
    fn program_internal_procedures_are_private() {
        let src = r#"program main
  call local_sub()
contains
  subroutine local_sub()
  end subroutine local_sub
end program main
"#;
        let facts = extract(src, "src/main.f90");
        let local = by_name(&facts, "local_sub").unwrap();
        assert_eq!(local.visibility, Visibility::Private);
        assert_eq!(
            local.id.to_scip_string(),
            "codegraph . . . main/local_sub()."
        );
    }

    // ── Calls ────────────────────────────────────────────────────────────────

    #[test]
    fn subroutine_call_and_function_call_are_captured() {
        let src = r#"program main
  use mymod
  integer :: r
  r = add(1, 2)
  call helper(r)
end program main
"#;
        let facts = extract(src, "src/main.f90");
        let calls: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Call)
            .map(|r| r.name.as_str())
            .collect();
        assert!(calls.contains(&"add"), "expected Call ref 'add': {calls:?}");
        assert!(
            calls.contains(&"helper"),
            "expected Call ref 'helper': {calls:?}"
        );
    }

    #[test]
    fn type_bound_calls_capture_receiver_qualifier() {
        let src = r#"program tb
  type(worker_t) :: obj
  integer :: v
  call obj%run_task(1)
  v = obj%get_value()
end program tb
"#;
        let facts = extract(src, "src/tb.f90");

        let run = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "run_task")
            .expect("expected Call ref 'run_task'");
        assert_eq!(run.qualifier.as_deref(), Some("obj"));

        let get = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "get_value")
            .expect("expected Call ref 'get_value'");
        assert_eq!(get.qualifier.as_deref(), Some("obj"));
    }

    // ── Imports ──────────────────────────────────────────────────────────────

    #[test]
    fn bare_use_imports_the_module() {
        let src = "program main\n  use mymod\nend program main\n";
        let facts = extract(src, "src/main.f90");
        let imp = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Import)
            .expect("expected an Import ref");
        assert_eq!(imp.name, "mymod");
        assert_eq!(imp.from_path.as_deref(), Some("mymod"));
    }

    #[test]
    fn use_only_imports_each_listed_name() {
        let src = "program main\n  use other_mod, only: thing, gadget\nend program main\n";
        let facts = extract(src, "src/main.f90");
        let imports: Vec<(&str, Option<&str>)> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Import)
            .map(|r| (r.name.as_str(), r.from_path.as_deref()))
            .collect();
        assert!(
            imports.contains(&("thing", Some("other_mod"))),
            "{imports:?}"
        );
        assert!(
            imports.contains(&("gadget", Some("other_mod"))),
            "{imports:?}"
        );
        assert!(
            !imports.iter().any(|(n, _)| *n == "other_mod"),
            "an only-list must not also import the whole module: {imports:?}"
        );
    }

    // ── Scopes & bindings ────────────────────────────────────────────────────

    #[test]
    fn module_and_function_scopes_nest_under_the_file_root() {
        let facts = extract(MODULE_SRC, "src/mymod.f90");

        assert_eq!(facts.scopes[0].kind, ScopeKind::Module);
        assert_eq!(facts.scopes[0].parent, None);

        let unit_scope_id = facts
            .scopes
            .iter()
            .position(|s| s.kind == ScopeKind::Module && s.parent == Some(0))
            .expect("expected a unit Module scope under the file root");
        assert!(
            facts
                .scopes
                .iter()
                .any(|s| s.kind == ScopeKind::Function && s.parent == Some(unit_scope_id)),
            "expected Function scopes under the unit scope"
        );
    }

    #[test]
    fn dummy_arguments_get_param_bindings() {
        let facts = extract(MODULE_SRC, "src/mymod.f90");
        let params: Vec<&str> = facts
            .bindings
            .iter()
            .filter(|b| b.kind == BindingKind::Param)
            .map(|b| b.name.as_str())
            .collect();
        // `add(a, b)`'s dummies are below MIN_REF_LEN; `helper(x)`'s too — use
        // a source with long names to assert the positive case.
        assert!(
            params.is_empty(),
            "short dummy names are filtered: {params:?}"
        );

        let src = r#"subroutine compute(alpha, bravo)
  integer, intent(in) :: alpha, bravo
end subroutine compute
"#;
        let facts = extract(src, "src/compute.f90");
        let params: Vec<&str> = facts
            .bindings
            .iter()
            .filter(|b| b.kind == BindingKind::Param)
            .map(|b| b.name.as_str())
            .collect();
        assert!(params.contains(&"alpha"), "{params:?}");
        assert!(params.contains(&"bravo"), "{params:?}");
    }

    // ── Fixed-form legacy ────────────────────────────────────────────────────

    #[test]
    fn fixed_form_legacy_yields_program_and_calls() {
        let src =
            "      PROGRAM MAIN\n      INTEGER I\n      I = 1\n      CALL FOO(I)\n      END\n";
        let facts = extract(src, "legacy/OLD.f");

        let module_sym = facts
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module)
            .expect("expected the file module symbol");
        assert_eq!(module_sym.name, "MAIN");

        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Call && r.name == "FOO"),
            "expected Call ref 'FOO' from fixed-form source"
        );
    }
}
