# Phase 4: Risky & Novel-Design Extractors - Research

**Researched:** 2026-07-05
**Domain:** tree-sitter extraction for eight novel-dispatch languages (Julia, R, OCaml, F#, Elixir, Erlang, Gleam, Haskell)
**Confidence:** HIGH — every node-kind/field claim below was verified by building a throwaway `examples/dump_ast.rs` (per CONTRIBUTING's tip) against this repo's **exact pinned crate versions**, running it, reading the real `to_sexp()` output, and deleting the example again (nothing committed). Training-data assumptions about several of these grammars turned out to be *wrong in specific, load-bearing ways* — see Common Pitfalls.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01 Julia** (`.jl`): `module`, `function` (long + short `f(x) = ...` forms), `struct`/`abstract type`, `macro` declarations; `using`/`import` → Imports; calls → Calls with receiver/module qualifiers where syntactic (`Base.push!`). CEILING: multiple dispatch — call refs stay NameOnly fan-out (resolver's job), never method-signature-matched; `@macro` invocations recorded as calls to the macro name only; `eval`/metaprogramming unresolved.
- **D-02 R** (`.R`, `.r`): assignment-based function symbols (`f <- function(...)`, also `=` and `<<-` where AST-unambiguous), S4 `setClass`/`setGeneric`/`setMethod` calls recorded as calls (NOT synthesized into class symbols), `library()`/`require()`/`requireNamespace()` → Imports, calls incl. namespace-qualified `pkg::fn` (qualifier captured). Visibility honestly `Unknown` (no in-language signal; NAMESPACE files out of scope). CEILING: NSE/`eval(parse(text=...))` documented, never attempted.
- **D-03 OCaml** (`.ml`, `.mli`): `let` bindings (top-level functions/values), `module`/`module type` declarations, `type` declarations, `open`/`include` → Imports, calls (juxtaposition application — extract the applied identifier head, qualifiers from `Module.fn` paths). `.mli` files extract as their own file's facts (signatures = declarations); CEILING: `.ml`↔`.mli` cross-file correlation explicitly out of scope this milestone; functors table-stakes (declaration only); PPX unresolved.
- **D-04 F#** (`.fs`, `.fsi`): reuse the ML-family approach validated by OCaml in this same phase (implement OCaml FIRST): `let` bindings, `module`/`namespace`, `type` declarations (records/unions/classes), `open` → Imports, calls incl. dotted access qualifiers, member definitions. CEILING: computation expressions/type providers unresolved; SRTP not modeled.
- **D-05 Elixir** (`.ex`, `.exs`): `defmodule` → module symbols, `def`/`defp` → functions with REAL visibility (def=Public, defp=Private — clean signal), `defmacro`, `alias`/`import`/`require`/`use` → Imports, calls incl. `Module.fun(args)` qualifiers. CEILING: macros/`use` expansion never attempted (record the call, stop); arity is part of identity convention — include `/arity` in the symbol descriptor ONLY if the SCIP scheme accommodates it cleanly, else name-only with arity in signature text (Claude's discretion, document choice).
- **D-06 Erlang** (`.erl`, `.hrl`): `-module` → module symbol, function clauses grouped by name/arity → one function symbol per name/arity, `-export` lists → REAL visibility (exported=Public, else Private), `-import`/`-include`/`-include_lib` → Imports, calls incl. remote `mod:fun(...)` qualifiers. CEILING: dynamic `apply/3` unresolved; preprocessor macros table-stakes.
- **D-07 Gleam** (`.gleam`): `pub fn`/`fn` → REAL visibility, `type` declarations, `import` → Imports, calls with module qualifiers. Cleanest of the family — full table stakes expected.
- **D-08 Haskell** (`.hs`): top-level function bindings (grouped equations → one symbol), `data`/`newtype`/`type`/`class`/`instance` declarations, `module X (exports)` header → REAL visibility from export lists (exported=Public, else Private; no export list = all Public), `import` (qualified/hiding forms) → Imports, calls (juxtaposition application head; operator applications only where AST-unambiguous). CEILING: type classes' dispatch, Template Haskell, and operator sections beyond simple application unresolved.
- **D-09:** One plan per language. Sequential waves in this order: OCaml → F# (depends on OCaml's ML-family shape) → Gleam → Elixir → Erlang → Haskell → Julia → R.
- **D-10:** Full recipe per language (Phases 2–3 practice): enum variant + dispatch, feature gains `_extractors` + default flip, extractor with module-doc ceilings, unit tests with real SCIP ids, ≥1 corpus `scoped_call` case, docs row 🟠→🟢 with honest capability columns + Notes ceilings, both bindings feature lists same-change, napi no-op verify per plan.
- **D-11:** Verification pattern per Phase 2/3 precedent: `cargo check --no-default-features --features <lang>` isolation + `cargo test --all-features` + fmt/clippy gates; resolver-test gap stays deferred (Phase 2 deferred-items.md).

### Claude's Discretion

- AST node names from `to_sexp()` dumps against pinned crates ONLY (research step; grammars: tree-sitter-julia, tree-sitter-r, tree-sitter-ocaml, tree-sitter-fsharp, tree-sitter-elixir, tree-sitter-erlang, tree-sitter-gleam, tree-sitter-haskell — names per Cargo.toml). **Resolved below — see Code Examples.**
- Arity-in-identity choice for Elixir/Erlang (D-05/D-06) — document whichever is chosen. **Recommendation below: yes for both, using `Descriptor::Method { name, disambiguator: arity }`.**
- OCaml `.mli` SymbolKind mapping (declaration kinds). **Recommendation below.**
- Corpus case content per language.

### Deferred Ideas (OUT OF SCOPE)

- OCaml `.ml`↔`.mli` cross-file correlation (explicitly out of scope per roadmap criterion 3)
- R NAMESPACE-file visibility, S4 class synthesis
- Julia method-signature dispatch modeling (resolver-tier work, out of milestone)
- Haskell operator-section/point-free call modeling beyond simple application
- Scanner-heavy grammars (Haskell, OCaml) on non-Linux bindings CI (DEPTH-03, v2)

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| LANG-02 | Julia extractor (template: Lua; multiple-dispatch ceiling documented) | Real AST evidence below (module/function/struct/macro/using/import/calls); arity-partial-dedup recommendation for the identity-collision risk multiple dispatch creates |
| LANG-03 | R extractor (template: Lua; NSE/eval ceiling documented) | Real AST evidence: `binary_operator`-based def heuristic works uniformly for `<-`/`=`/`<<-`; `library()`/`require()` import pattern identical to Lua's `require()`; `namespace_operator` qualifier capture for `pkg::fn` |
| LANG-04 | OCaml extractor (template: Rust + C header/impl split for `.ml`/`.mli`) | Real AST evidence for both `LANGUAGE_OCAML` and `LANGUAGE_OCAML_INTERFACE`; flat multi-argument `application_expression`; positional (unlabeled) module-name children |
| LANG-11 | F# extractor via `tree-sitter-fsharp` (ionide) | Real AST evidence; contrasts with OCaml's flat-application shape — F# nests `application_expression`, a genuine ML-family divergence the planner must account for |
| LANG-12 | Elixir, Erlang, Gleam, Haskell — implement those that pass the COMPAT-01 recheck (all four passed) | Real AST evidence for all four; Elixir's def/defp distinction is a **lexical target-text match on a uniform `call` node**, not a grammar-level distinction; Erlang's per-clause `fun_decl` nodes require explicit name+arity grouping; Haskell's `hiding`/`qualified` import forms are **structurally invisible** in this grammar version |

</phase_requirements>

## Summary

All eight grammars parse clean, idiomatic source with **zero ERROR/MISSING nodes** at the pinned versions (tree-sitter-julia 0.23.1, tree-sitter-r 1.3.0, tree-sitter-ocaml 0.25.0, tree-sitter-fsharp 0.3.1, tree-sitter-elixir 0.3.5, tree-sitter-erlang 0.19.0, tree-sitter-gleam 1.0.0, tree-sitter-haskell 0.23.1) — including Haskell, whose layout rule is the phase's most-flagged risk but which parsed every construct in the CONTEXT-mandated coverage list without incident. The real risk in this phase is not parser fragility; it is **identity-collision** (several languages let two definitions share a name) and a handful of **grammars that don't expose a distinction the decision text assumes exists** (Elixir's def/defp is a text match, not a node-kind match; Haskell's `import qualified`/`hiding` render identically to plain imports).

Three findings change how the planner should size tasks:

1. **Erlang's per-clause `fun_decl` nodes are never pre-grouped by the grammar.** Two clauses of `add(X, Y) -> ...;` / `add(X, Y, Z) -> ...` — even the *same*-arity, semicolon-chained pattern-match form — each produce a **separate top-level `fun_decl`** node. D-06's "one function symbol per name/arity" is 100% an extractor-side grouping pass, not something the grammar hands you pre-shaped. Same is true for Haskell's multi-equation bindings (`privateFn 0 = 0` / `privateFn x = x + 1` → two sibling `function` nodes) and for instance-body method equations.
2. **Elixir has almost no dedicated grammar nodes.** `defmodule`, `def`, `defp`, `defmacro`, `alias`, `import`, `require`, `use` are ALL the *same* `call` node kind (`target: (identifier) …`). The only way to tell `def` from `defp` — or a module declaration from a function definition — is to read the `target` identifier's literal text ("def" vs "defp" vs "defmodule" …). This is structurally the same kind of text-match detection this project already uses for Python's `PY_ROUTE_VERBS` and R's S4 call-shape matching — not a new technique, but it is the **primary** technique here, not a fallback.
3. **Identity collision is a real bug risk for Julia/Elixir/Erlang**, because this project's dedup rule (first-occurrence-wins on identical SCIP id, per `objc.rs`'s category-dedup precedent) will silently **drop** a second same-name definition unless the descriptor disambiguates it. Erlang's `add/2` and `add/3` render to the *same* SCIP string (`…/add().`) without an arity discriminator — a correctness bug, not a recall gap. `Descriptor::Method { name, disambiguator }` already exists for exactly this (SCIP's disambiguator grammar), is unused by every current extractor (all pass `""`), and round-trips cleanly through `parse_descriptor` (tested). Arity strings (`"2"`, `"3"`) are `is_simple_ident_char`-legal, so no backtick-escaping is needed.

**Primary recommendation:** build in D-09's order (OCaml → F# → Gleam → Elixir → Erlang → Haskell → Julia → R); for Erlang and Elixir, resolve the arity-in-identity discretion by using `Descriptor::Method { name, disambiguator: arity.to_string() }` for every function symbol — cheap, SCIP-native, and closes the identity-collision hole documented above.

## Standard Stack

### Core (all already pinned + ABI-verified in Phase 1 — do not change versions)

| Crate | Version | tree-sitter-language req | Verified | Grammar fn(s) already in `src/grammar.rs` |
|---|---|---|---|---|
| tree-sitter-julia | 0.23.1 | ^0.1 | PASS (01-COMPAT-VERDICTS.md) | `julia()` → `LANGUAGE` |
| tree-sitter-r | 1.3.0 | ^0.1 | PASS | `r()` → `LANGUAGE` |
| tree-sitter-ocaml | 0.25.0 | ^0.1 | PASS | `ocaml()` → `LANGUAGE_OCAML`; crate ALSO exports `LANGUAGE_OCAML_INTERFACE` and `LANGUAGE_OCAML_TYPE` (not yet wired — Phase 4's job) |
| tree-sitter-fsharp | 0.3.1 | ^0.1 | PASS | `fsharp()` → `LANGUAGE_FSHARP` (crate has no plain `LANGUAGE` const) |
| tree-sitter-elixir | 0.3.5 | ^0.1 | PASS | `elixir()` → `LANGUAGE` |
| tree-sitter-erlang | 0.19.0 | ^0.1 | PASS | `erlang()` → `LANGUAGE` |
| tree-sitter-gleam | 1.0.0 | ^0.1 | PASS | `gleam()` → `LANGUAGE` |
| tree-sitter-haskell | 0.23.1 | ^0.1 | PASS | `haskell()` → `LANGUAGE` |

**No `npm view`-equivalent version check applies here** — these are Cargo grammar deps, already pinned in `Cargo.toml` and already ABI-gated in Phase 1 (`01-COMPAT-VERDICTS.md`). This phase must **not** bump any of these versions; doing so would re-open a gate that Phase 1 already closed empirically. Confirmed still resolving cleanly against the current `Cargo.lock` (re-checked 2026-07-05, `cargo check --no-default-features --features <lang>` for each, zero errors — output has only pre-existing dead-code warnings unrelated to this phase).

### Feature wiring already done (Phase 1) — Phase 4 does NOT touch these

- `Cargo.toml`: all 8 features exist as `<lang> = ["dep:tree-sitter-<lang>"]` — **grammar-only, no `_extractors`, not in `default`**. Phase 4's job per language: add `"_extractors"` to the feature list and add the language to `default`.
- `src/grammar.rs`: all 8 grammar fns + `abi_versions_are_compatible` test arms already present (see Code Examples for the one gap: `ocaml_interface()` must be added for `.mli`).

### Supporting

| Concern | Reuse | Notes |
|---|---|---|
| Shared extractor helpers | `src/extract/support.rs` (`make_symbol`, `node_text`, `one_line_signature`, `collect_call_references`, `push_import_ref`, `push_ref`, `push_scope`, `attach_reference_scopes`, `definition_bindings`, `import_bindings`, `module_symbol`) | Same helpers as every prior extractor; nothing new needed for basic wiring |
| Descriptor disambiguator | `src/symbol/descriptor.rs::Descriptor::Method { name, disambiguator }` | **First real (non-empty) use** of this field in the codebase — see Code Examples for Erlang/Elixir arity usage and the escaping rule |
| Macro-definition identity precedent | `Descriptor::Macro(name)` + `SymbolKind::Function` (function-like) / `SymbolKind::Const` (object-like) | Established in `c.rs`/`cpp.rs`/`objc.rs` for `#define`; directly reusable for Julia's `macro name(...) ... end` and Elixir's `defmacro name(...) do ... end` |
| Assignment-as-definition heuristic | none yet in-repo (new pattern) | R's `f <- function(...)` needs a "top-level `binary_operator` whose RHS is `function_definition`" scan — closest existing precedent is Lua's `emit_local_symbol` (value-kind dispatch on RHS), not a 1:1 reuse |

### Alternatives Considered

| Instead of | Could use | Tradeoff |
|---|---|---|
| Arity-as-disambiguator for Erlang/Elixir | Name-only identity (first-occurrence-wins, same as ObjC category dedup) | Simpler, but **silently drops** same-name/different-arity functions — a real correctness bug for Erlang, where `foo/1` and `foo/2` are routine, distinct, commonly-co-occurring functions |
| Arity-as-disambiguator for Julia | Full multiple-dispatch signature matching | Signature matching is explicitly out of scope (D-01 CEILING, roadmap-level exclusion); arity alone doesn't fully solve same-arity/different-type overloads, but it fixes the *majority* case (different-arity overloads) cheaply — document the residual same-arity gap honestly, don't chase it |
| `binary_operator` operator-text detection (R `<-`/`=`/`<<-`; Elixir `|>`; Haskell `$`/`.`) | Skip distinguishing operators, treat all as reads | Loses `|>` pipe-desugaring (D-05 explicitly requires recognizing `|>`) and loses the R assignment-vs-comparison distinction; the token IS present as a child, just unnamed — cheap to read, no reason to skip |

**Installation:** no `Cargo.toml` dependency changes — this phase only flips existing `dep:` lines into the extractor build (`"_extractors"` + `default`) per language, same 1-line diff shape as every prior language phase.

## Architecture Patterns

### Recommended Project Structure (per language, matches Phase 2/3 precedent exactly)

```
src/extract/
├── ocaml.rs      # LANGUAGE_OCAML (.ml) + LANGUAGE_OCAML_INTERFACE (.mli), one struct, dispatches on file extension inside extract()
├── fsharp.rs     # reuses OCaml's ML-family shape; own file (not a shared fn — grammars are unrelated crates)
├── gleam.rs
├── elixir.rs
├── erlang.rs
├── haskell.rs
├── julia.rs
└── r.rs
```

No `extract_<family>` shared-function opportunity exists across these eight (confirmed: each grammar is an unrelated crate; the Lua/Luau sharing precedent is grammar-lineage-specific, not applicable here — matches FEATURES.md's own dependency-graph note).

### Pattern: OCaml `.ml`/`.mli` single-extractor, extension-branched grammar selection

**What:** One `OCamlExtractor` (or a shared internal fn) that inspects `file` for a `.mli` suffix and picks `LANGUAGE_OCAML_INTERFACE` vs `LANGUAGE_OCAML` before parsing. `src/grammar.rs` needs a new `ocaml_interface()` fn (the crate already exports `LANGUAGE_OCAML_INTERFACE`; Phase 1 deliberately left it unwired — see `src/grammar.rs:168-175`).
**When to use:** Both `.ml` and `.mli` map to `Language::OCaml` in `src/lang.rs` (one enum variant, two extensions) — same collision-precedent shape as ObjC's `.m`/`.mm` (one variant, `extensions()` returns both).
**Example:**
```rust
// src/grammar.rs — add alongside the existing ocaml() fn:
#[cfg(feature = "ocaml")]
pub fn ocaml_interface() -> Language {
    tree_sitter_ocaml::LANGUAGE_OCAML_INTERFACE.into()
}
// ...and the matching abi_versions_are_compatible arm:
#[cfg(feature = "ocaml")]
check("ocaml_interface", super::ocaml_interface());
```
```rust
// src/extract/ocaml.rs — inside OCamlExtractor::extract():
let is_interface = file.ends_with(".mli");
let ts_language = if is_interface {
    crate::grammar::ocaml_interface()
} else {
    crate::grammar::ocaml()
};
```

### Pattern: Elixir — text-match on a uniform `call` node (NOT a grammar-kind distinction)

**What:** `defmodule`, `def`, `defp`, `defmacro`, `alias`, `import`, `require`, `use` are ALL parsed as a `call` node: `(call target: (identifier) (arguments …) (do_block …)?)`. The only signal distinguishing them is the literal text of the `target` identifier.
**When to use:** Every Elixir definition/import-form detector.
**Verified real shape** (`tree-sitter-elixir` 0.3.5, snippet: `defmodule MyApp.Worker do def public_fn(x) do private_fn(x) end defp private_fn(x) do x + 1 end end`):
```text
(source
 (call target: (identifier)              ; text = "defmodule"
   (arguments (alias))                    ; alias node's raw text = "MyApp.Worker" (dotted, single leaf token — no children)
   (do_block
     (call target: (identifier)          ; text = "def"
       (arguments (call target: (identifier) (arguments (identifier))))  ; inner call = the fn's own name+params
       (do_block (call target: (identifier) (arguments (identifier)))))
     (call target: (identifier)          ; text = "defp"
       (arguments (call target: (identifier) (arguments (identifier))))
       (do_block (binary_operator left: (identifier) right: (integer)))))))
```
**Recipe:** for each `call` node whose `target` text ∈ {"def","defp"}, the FIRST `arguments` child is itself a `call` node — its own `target` is the function name, its own `arguments` are the parameters. Visibility = Public for "def", Private for "defp" (D-05's clean signal, confirmed real).

**Remote/pipe calls (verified, same snippet family):**
```text
;; Module.fun(args) remote call:
(call target: (dot left: (alias) right: (identifier)) (arguments (integer) (integer)))
;; |> pipeline — a generic binary_operator, NOT a dedicated pipe node:
(binary_operator left: (identifier) right: (call target: (dot left: (alias) right: (identifier)) (arguments (anonymous_function (stab_clause left: (arguments (identifier)) right: (body …))))))
;; anonymous fn:
(anonymous_function (stab_clause left: (arguments (identifier)) right: (body …)))
```
`|>` must be detected by reading the operator token's literal text between `left`/`right` (it's an unnamed child, not a named field) — the node kind alone (`binary_operator`) is shared with every other Elixir infix operator (`+`, `-`, …).

### Pattern: Erlang — per-clause `fun_decl`, name/arity grouping is 100% extractor work

**What:** Every `Name(Args) -> Body.` clause — even semicolon-chained multi-clause pattern matches of the *same* name+arity — is its own top-level `fun_decl` node. D-06's "one symbol per name/arity" requires an explicit grouping pass over `source_file`'s children.
**Verified real shape** (`tree-sitter-erlang` 0.19.0, snippet: `factorial(0) -> 1; factorial(N) -> N * factorial(N - 1).`):
```text
(source_file
 forms_only: (fun_decl clause: (function_clause name: (atom) args: (expr_args args: (integer)) body: (clause_body exprs: (integer))))
 forms_only: (fun_decl clause: (function_clause name: (atom) args: (expr_args args: (var)) body: (clause_body exprs: (binary_op_expr …)))))
```
Two SEPARATE `fun_decl` nodes for the same `factorial` name — the grammar does not group them. Arity = count of `args:` field children in `expr_args` (no explicit arity field on `function_clause` itself).

**Export list IS arity-qualified in the grammar (this one IS structural):**
```text
(export_attribute funs: (fa fun: (atom) arity: (arity value: (integer))) funs: (fa fun: (atom) arity: (arity value: (integer))))
```
`-export([add/2, add/3])` → two `fa` nodes, each with a real `arity:` field. This is the cleanest visibility signal of the whole phase — matches Go's capitalization-convention cleanliness.

**Remote call field name is `expr:`, not `function:`:**
```text
;; other_mod:process(X) —
(remote module: (remote_module module: (atom)) fun: (call expr: (atom) args: (expr_args args: (var))))
;; local call add(X, 1) —
(call expr: (atom) args: (expr_args args: (var) args: (integer)))
;; ?MAX macro use —
(macro_call_expr name: (var))
```

### Pattern: F# and Haskell nest curried application; OCaml flattens it — a real ML-family divergence

**What:** D-04 tells the planner to reuse OCaml's ML-family shape for F#. The *call detection query itself cannot be reused verbatim* — OCaml's grammar flattens `f x y` into one node with repeated `argument:` fields; F#'s and Haskell's grammars nest it as `apply(apply(f, x), y)` with **no field labels at all**.

**OCaml (verified, `tree-sitter-ocaml` 0.25.0, `add 1 2`):**
```text
(application_expression function: (value_path (value_name)) argument: (number) argument: (number))
```
One node, `function:` field + repeated `argument:` fields — a `Query`-friendly flat shape (`(application_expression function: (_) @callee_path argument: (_))`).

**F# (verified, `tree-sitter-fsharp` 0.3.1, `add 1 2`):**
```text
(application_expression (application_expression (long_identifier_or_op (identifier)) (const (int))) (const (int)))
```
No field names at all; a manual recursive walk on the FIRST positional child is required to reach the head identifier (innermost `application_expression`'s first child).

**Haskell (verified, `tree-sitter-haskell` 0.23.1, `g x y`):**
```text
(apply function: (apply function: (variable) argument: (variable)) argument: (variable))
```
Haskell DOES label the fields (`function:`/`argument:`) but still nests — a middle case between OCaml's flat shape and F#'s fully-positional one. A recursive walk down `function:` to the leaf `variable`/`long_identifier_or_op` is required either way.

**Recipe (F#/Haskell):** write a small recursive helper — `fn application_head(node) -> Option<Node>` that, given an `application_expression`/`apply` node, recurses into the `function`-position child (field-named for Haskell, positional-first-child for F#) until it hits a non-application node, then reads that leaf's identifier text. Do NOT try to write a single flat tree-sitter `Query` for these two the way `CALL_QUERY` does for every other extractor — the nesting depth is unbounded (one call site per curried argument), so a manual walk (like ObjC's message-send reassembly) is the right tool, not a `Query` pattern.

### Pattern: Haskell multi-equation bindings and separate type signatures

**What:** `privateFn 0 = 0` / `privateFn x = x + 1` are TWO sibling top-level nodes (verified: both named `function name: (variable) …`), matching name text `privateFn`. A **type signature** (`publicFn :: Int -> Int`) is ALSO a separate sibling node (`signature name: (variable) type: (…)`), not attached to the function equations at all.
**Verified real shape:**
```text
declarations: (declarations
  (signature name: (variable) type: (function parameter: (name) result: (name)))
  (function name: (variable) patterns: (patterns (variable)) match: (match expression: (apply …)))
  (signature name: (variable) type: (function parameter: (name) result: (name)))
  (function name: (variable) patterns: (patterns (literal (integer))) match: (match expression: (literal (integer))))
  (function name: (variable) patterns: (patterns (variable)) match: (match expression: (infix …))))
```
**Recipe:** two-pass extraction. Pass 1: group `function`/`bind` nodes by `name:` text (D-08's "grouped equations → one symbol" — verified as a REAL requirement, not defensive over-documentation). Pass 2: for each group, look for a PRECEDING sibling `signature` node with the same name and use its `type:` text as (part of) the one-line signature. `bind` (not `function`) is the node kind for a zero-argument binding (`f = 1`, no `patterns:` field at all) — both kinds must be handled in the grouping pass.

### Anti-Patterns to Avoid

- **Writing a flat `CALL_QUERY` tree-sitter `Query` for F#/Haskell juxtaposition application.** The nesting is nsymmetric/unbounded depth (each curried arg adds a level); every other extractor's `CALL_QUERY` constant assumes a bounded, flat pattern. Use a manual recursive walk instead (see above).
- **Assuming Elixir's `def`/`defp`/`defmodule`/etc. are distinct node kinds.** They are all `call`. A `match node.kind() { "def" => ..., "defp" => ... }` style dispatch will silently match nothing — the dispatch must be on `target` identifier TEXT.
- **Assuming `import qualified`/`hiding` are structurally visible in Haskell.** They are not (verified — see Common Pitfalls). Any visibility/qualifier logic that branches on a grammar node/field for these will silently do the wrong thing; it must branch on raw source text instead, or the ceiling must be documented as "capture the alias/import-list; do not attempt to distinguish qualified from unqualified, or hiding from inclusive."
- **Emitting Erlang/Elixir function symbols with an empty disambiguator.** Two same-name, different-arity functions will collide to the identical SCIP string and one will be silently dropped by this project's dedup-by-SCIP-id convention (`HashSet<String>` insert-check, per `objc.rs`).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---|---|---|---|
| SCIP identifier escaping for non-simple names | A custom backtick-escaper | `Descriptor::render`'s existing `push_ident` (auto-escapes any name with chars outside `[A-Za-z0-9_+\-$]`) | Already handles Erlang's `?MACRO`-adjacent identifiers, Haskell's operator names (`(+++)`), and any dotted/qualified text that leaks into a `name` field; don't special-case per language |
| Arity-based disambiguation | A new `Descriptor` variant or a hand-rolled "name#arity" string glued into `name` | `Descriptor::Method { name, disambiguator: arity.to_string() }` (existing field, tested, round-trips) | The disambiguator grammar (`method-disambiguator ::= simple-identifier?`) already exists precisely for this; a digit string like `"2"` needs zero escaping (`is_simple_ident_char` includes ascii-alphanumeric) |
| Module-symbol/namespace derivation | Custom per-language "walk to file's module declaration" logic from scratch | `super::module_symbol` + the same "scan root's direct children for the module-declaring node, else fall back to file-path segments" pattern already in `fortran.rs::fortran_namespaces` | Julia (`module X`), Erlang (`-module(x)`), OCaml (`module X = struct`), Elixir (`defmodule X.Y`) all need the identical shape: prefer an explicit module declaration, fall back to path-derived segments |
| Macro-definition SCIP identity | A new SymbolKind or Descriptor for "macro" | `SymbolKind::Function` + `Descriptor::Macro(name)` (function-like) — already the exact precedent in `c.rs`/`cpp.rs`/`objc.rs` for `#define` | Julia's `macro name(...) end` and Elixir's `defmacro name(...) do end` are structurally the same "callable macro definition" shape C's function-like `#define` already models |
| Assignment-based function detection (R) | A bespoke "is this a definition" heuristic that special-cases each of `<-`/`=`/`<<-` | One check: `binary_operator` node whose `rhs` field's kind is `function_definition` (verified: all three operators produce the SAME node shape) | Confirmed empirically — the operator's own text plays no role in whether the RHS is a function; checking RHS kind is sufficient and simpler than parsing the operator token |

**Key insight:** the phase's real complexity is not "find the right tree-sitter query" (a solved problem in this codebase) — it's **honestly representing grammars that either don't group what the domain groups (Erlang/Haskell clauses) or don't expose what the language spec implies exists (Elixir def/defp, Haskell qualified/hiding)**. Every one of those gaps is now verified and itemized above; none should surprise a task's verification step.

## Common Pitfalls

### Pitfall 1: Trusting training-data Haskell semantics over the actual grammar's field exposure
**What goes wrong:** Assuming `import qualified Data.Map as Map hiding (filter)` structurally differs from `import Data.Map as Map` in the parse tree (because it obviously differs in Haskell semantics).
**Why it happens:** `tree-sitter-haskell` 0.23.1 exposes NO field/node for the `qualified` keyword and NO field/node distinguishing an inclusive `(sort, nub)` list from a `hiding (filter)` exclusion list — both render as `names: (import_list …)`. Verified directly: `import Data.Map as Map` (no `qualified`) and `import qualified Data.Map as Map` (with it) produce byte-for-byte identical subtrees except for the source span; the keyword is consumed by an anonymous/unnamed token with no field.
**How to avoid:** if `hiding`/`qualified` need to be distinguished at all (D-08 doesn't strictly require it — only "imports (qualified/hiding forms) → Imports" — i.e., emit the Import ref either way), fall back to a raw substring check on the node's own source text (`node_text` between the `import` keyword and the module path) rather than a field/kind check. Document explicitly in the extractor's module doc that `qualified`-ness and `hiding`-ness are NOT modeled structurally.
**Warning signs:** any Haskell extractor code that does `if child.kind() == "qualified"` or similar will simply never match.

### Pitfall 2: Missing `end` in hand-authored Julia fixtures produces a whole-file `ERROR` wrapper that looks like a grammar bug
**What goes wrong:** During this research, a hand-typed Julia snippet with `module MyMod … ` missing its closing `end` produced `(ERROR (identifier) … <every top-level construct> …)` — the ENTIRE module's contents wrapped in one `ERROR` node — which looks exactly like "the grammar can't handle this combination of constructs."
**Why it happens:** tree-sitter's error recovery, when a `module`/`begin`/`function`/etc. block's terminating `end` is missing, backs out to the nearest ancestor it CAN close and wraps everything from the unclosed block onward in one `ERROR`. The symptom (one giant ERROR node) is identical whether the cause is a real grammar limitation or a simple missing keyword.
**How to avoid:** before concluding a construct combination is a genuine grammar gap, verify `end`/closing-keyword balance by counting opens vs. closes in the fixture, or bisect by deletion from the back of the file (removing the LAST added construct first) rather than the front — the error's *reported* location (start of file) is misleading; the real fault is usually near the file's END.
**Warning signs:** an `ERROR` node as the single top-level child wrapping literally everything, rather than a small localized `ERROR`/`MISSING` node — that shape means "unclosed block somewhere," not "grammar can't parse construct X."

### Pitfall 3: Assuming Julia/Elixir/Erlang's arity-overload identity "just works" with existing dedup
**What goes wrong:** Reusing the exact `push_symbol`/dedup-by-SCIP-id pattern from `objc.rs` verbatim (empty disambiguator) for Erlang's `add/2`/`add/3` silently drops the second definition.
**Why it happens:** every current extractor renders `Descriptor::Method { disambiguator: "", .. }`; two same-name functions in the same namespace collide to the identical rendered SCIP string, and the `HashSet<String>` dedup (first-occurrence-wins) silently keeps only the first.
**How to avoid:** for Erlang (mandatory per D-06) and Elixir (discretionary per D-05, recommended above), set `disambiguator` to the function's arity (`args.len().to_string()`). For Julia, arity-as-disambiguator closes the *majority* multiple-dispatch collision case (different-arg-count overloads) but NOT the same-arity/different-type case — document that residual gap explicitly rather than silently accepting dropped symbols.
**Warning signs:** a corpus/unit test asserting `facts.symbols.len()` before discovering it's short by exactly the number of arity-overloaded names in the fixture.

### Pitfall 4: Assuming a generic `binary_operator`/`infix_expression` node's text tells you the operator
**What goes wrong:** Querying for a `pipe_operator` or `pipeline_expression` node kind in Elixir, or a dedicated `|>`/`$`/`.` node in F#/Haskell.
**Why it happens:** none of these three grammars give the pipe/composition/application operators their own node kind — Elixir's `|>` is a `binary_operator` (same kind as `+`), F#'s `|>` is an `infix_expression` with a generic `infix_op` child (same kind used for `*`), and Haskell's `$`/`.` are `infix` nodes with a generic `operator` child (same kind used for `+`/`*`/any user operator).
**How to avoid:** read the operator token's literal text (an unnamed but present child between `left`/`lhs`/`left_operand` and `right`/`rhs`/`right_operand`) and match against the literal string (`"|>"`, `"$"`, `"."`). This is the same "read the raw token text, don't trust the node kind" discipline as the R assignment-operator case.
**Warning signs:** pipe-desugaring (D-05's explicit requirement) that never fires on real `|>` usage.

### Pitfall 5: OCaml/F# module names are positional (unlabeled) children, not fields
**What goes wrong:** calling `node.child_by_field_name("name")` on a `module_binding`/`module_defn` node and getting `None`.
**Why it happens:** verified directly — `(module_binding (module_name) body: (structure …))` in OCaml and `(module_defn (identifier) block: (declaration_expression …))` in F# both show the module's own name as a bare, unlabeled first child, not a `name:` field. This is the same asymmetry `fortran.rs` already documents for `module_statement`/`program_statement` vs. `function_statement`/`subroutine_statement` (D-01 in Fortran's own module doc).
**How to avoid:** `node.children(&mut node.walk()).find(|c| c.kind() == "module_name")` (OCaml) / `.find(|c| c.kind() == "identifier")` as the FIRST such child (F#), not `child_by_field_name`.
**Warning signs:** a module symbol silently getting an empty/placeholder name in tests.

## Code Examples

### Julia — CALL_QUERY, def kinds, macro shape (verified against tree-sitter-julia 0.23.1)

```rust
// Source: to_sexp() dump, `module M using Base: push! import Other: helper
// struct Point x y end abstract type Shape end
// function long_form(a, b) return a + b end
// short_form(x) = x^2
// macro mymacro(ex) ex end
// function caller() long_form(1,2); Base.push!(arr,1); @mymacro(1+1); short_form(3) end
// end` — real node kinds (clean parse, zero ERROR nodes when `end`-balanced):
//
// module_definition name: (identifier) <children...>          — `module M ... end`
// using_statement (selected_import (identifier) (identifier)) — `using Base: push!` (module ident, then each selected name)
// import_statement (selected_import (identifier) (identifier))
// struct_definition (type_head (identifier)) (identifier) (identifier)   — field names are direct children, no `body` wrapper
// abstract_definition (type_head (identifier))
// function_definition (signature (call_expression (identifier) (argument_list ...))) (return_statement ...)  — long form
// assignment (call_expression (identifier) (argument_list (identifier))) (operator) (binary_expression ...)  — SHORT form `f(x) = ...` is an `assignment`, NOT a function_definition
// macro_definition (signature (call_expression (identifier) (argument_list (identifier)))) (identifier)      — `macro name(ex) ... end`
// macrocall_expression (macro_identifier (identifier)) (argument_list ...)                                   — `@mymacro(...)` USE site
// call_expression (field_expression value: (identifier) (identifier)) (argument_list ...)                    — `Base.push!(...)`: field_expression has NO named fields for its two identifiers (value: labels only the first; the field name itself is the second unlabeled child)
```

**Recipe:** top-level dispatch on `module.children()`:
- `function_definition` → long-form Function symbol (name/params from `signature > call_expression`)
- `assignment` where the LHS is a `call_expression` → short-form Function symbol (SAME `SymbolKind::Function`, different node kind — do not miss this arm)
- `struct_definition` / `abstract_definition` → Struct (both map to `SymbolKind::Struct`; Julia has no separate "abstract struct" kind in this schema — document `abstract type` as `Struct` or add a Note)
- `macro_definition` → `SymbolKind::Function` + `Descriptor::Macro(name)` (per Don't Hand-Roll precedent)
- `macrocall_expression` → `RefRole::Call` reference to the macro's bare name (strip the `@`), per D-01's ceiling
- `using_statement`/`import_statement` → one `Import` ref per `selected_import`'s trailing identifiers; the FIRST identifier is the module, rest are selected names (`from_path` = module name)
- `field_expression value: (identifier) field` (e.g. `Base.push!`) → qualifier = `value` text, callee = the second child's text (no field label on it — verify via position)

### R — assignment-def heuristic, S4-as-plain-calls, namespace qualifier (verified against tree-sitter-r 1.3.0)

```rust
// Source: to_sexp() dump of:
//   foo <- function(x, y) { x + y }
//   bar = function(z) { z * 2 }
//   counter <<- function() { 1 }
//   setClass("Person", representation(name = "character"))
//   result <- foo(1, 2)
//   other <- utils::head(c(1, 2, 3))
//   utils:::internal_fn(5)
//
// binary_operator lhs: (identifier) rhs: (function_definition parameters: (parameters parameter: (parameter name: (identifier)) ...) body: (braced_expression body: ...))
//   — IDENTICAL shape for <-, =, AND <<- (verified: no distinguishing field; the operator's own
//     token text sits between lhs/rhs as an unnamed child if you need it, but the RHS-kind check
//     alone is sufficient for "is this a definition")
// call function: (identifier) arguments: (arguments argument: (argument value: ...))   — setClass(...) is a PLAIN call, no special node
// call function: (namespace_operator lhs: (identifier) rhs: (identifier)) arguments: (...)  — utils::head(...) / utils:::internal_fn(...) — `::` and `:::` render IDENTICALLY (namespace_operator); read source text between lhs/rhs if distinguishing them ever matters (not required by D-02)
// extract_operator lhs: (identifier) rhs: (identifier)   — `x@name` (S4 slot access)
```

**Recipe:**
- `binary_operator` at top level, `rhs.kind() == "function_definition"` → Function symbol named from `lhs` text (works for `<-`/`=`/`<<-` uniformly)
- Any `call` → `Reference { role: Call }`; if `function` text ∈ {"library","require","requireNamespace"} → re-tag as `Import` (exact `collect_require_imports` pattern from `lua.rs`, including handling BOTH a bare-identifier arg `library(utils)` AND a string-literal arg `library("utils")` — R accepts both, my fixture only exercised the bare form)
- `call function: (namespace_operator lhs: (_) @qualifier rhs: (_) @callee)` → qualified call, qualifier = package name
- `setClass`/`setGeneric`/`setMethod` calls: emit as ordinary `Call` references (per D-02, do NOT synthesize a class Symbol)

### OCaml — flat application, positional module names, `.mli` field-name divergence (verified against tree-sitter-ocaml 0.25.0)

```rust
// .ml source: open Base include Extra module Inner = struct let value = 1 end
//   module type Sig = sig val f : int -> int end
//   type point = { x : int; y : int }
//   let add x y = x + y   let result = add 1 2   let via_module = Inner.value
//   module Functor (X : Sig) = struct let g y = X.f y end
//
// open_module module: (module_path (module_name))
// include_module module: (module_path (module_name))
// module_definition (module_binding (module_name) body: (structure ...))        — module_name is POSITIONAL (no field label)
// module_type_definition (module_type_name) body: (signature ...)              — module_type_name ALSO positional
// type_definition (type_binding name: (type_constructor) body: (record_declaration (field_declaration (field_name) type: (...)) ...))
// value_definition (let_binding pattern: (value_name) (parameter pattern: (value_pattern)) ... body: ...)  — `let add x y = ...`: params are repeated POSITIONAL `parameter` children (no `parameters:` field)
// application_expression function: (value_path (value_name)) argument: (number) argument: (number)  — FLAT: one node, `function:` + repeated `argument:` fields
// value_path (module_path (module_name)) (value_name)   — `Inner.value` qualified access; both children POSITIONAL
// module_definition (module_binding (module_name) (module_parameter (module_name) module_type: (module_type_path (module_type_name))) body: (structure ...))  — functor: module_parameter node holds the functor's own arg name + `module_type:` field

// .mli source: val add : int -> int -> int  module Inner : sig val value : int end  type point = { x:int; y:int }
//
// value_specification (value_name) type: (function_type domain: (...) codomain: (...))   — top-level `val` decl, NOT wrapped in value_definition/let_binding
// module_definition (module_binding (module_name) module_type: (signature (value_specification ...)))  — KEY DIVERGENCE from .ml:
//   .ml  module binding uses  body: (structure ...)     (the `= struct ... end` form)
//   .mli module binding uses  module_type: (signature ...)  (the `: sig ... end` form — no `=`)
```

**Recipe:**
- `let_binding` where a `parameter` child is present → Function (Method descriptor); no `parameter` children → Term/value
- `module_binding` → `SymbolKind::Module` + `Descriptor::Namespace(name)`, name read positionally (`.find(|c| c.kind() == "module_name")`)
- `module_type_definition` → recommend `SymbolKind::Interface` + `Descriptor::Type(name)` (closest existing kind to "a named signature")
- `.mli` `value_specification` at top level (not inside a `module_type`) → recommend `SymbolKind::Function` (or `Static` if the type isn't an arrow type) — this is the Claude's-discretion mapping the CONTEXT flags; the important verified fact is that `.mli`'s `val` declarations are a DIFFERENT node kind (`value_specification`) from `.ml`'s `value_definition`/`let_binding`, so the same collection function cannot be reused as-is between the two grammars
- `application_expression` (flat) → `CALL_QUERY`-style tree-sitter Query IS viable here (unlike F#/Haskell): `(application_expression function: (value_path (value_name) @callee) argument: (_))` — capture `function:`'s leaf `value_name`, qualifier from any `module_path` prefix

### F# — nested application, `namespace`/`module_defn`, member `instance:`/`method:` split (verified against tree-sitter-fsharp 0.3.1)

```rust
// Source: namespace MyApp  module Inner = let value = 1  open System
//   type Point = { X: int; Y: int }
//   type Shape = | Circle of float | Square of float
//   type Greeter(name: string) = member this.Greet() = printfn "Hello %s" name
//   let add x y = x + y   let result = add 1 2 |> string   let called = Inner.value
//   match result with | "3" -> ... | _ -> ...
//
// namespace name: (long_identifier (identifier)) (module_defn (identifier) block: (declaration_expression ...))  — nested module inside namespace; module name POSITIONAL
// import_decl (long_identifier (identifier))                     — `open System`
// type_definition (record_type_defn (type_name type_name: (identifier)) block: (record_fields (record_field (identifier) (simple_type ...)) ...))
// type_definition (union_type_defn (type_name type_name: (identifier)) block: (union_type_cases (union_type_case (identifier) (union_type_fields ...)) ...))
// type_definition (anon_type_defn (type_name type_name: (identifier)) (primary_constr_args ...) block: (type_extension_elements
//     (member_defn (method_or_prop_defn name: (property_or_ident instance: (identifier) method: (identifier)) args: ... (application_expression ...)))))
//   — class w/ primary constructor + member: `instance:` = "this", `method:` = "Greet" — the receiver and method name are DISTINCT fields, unlike ObjC's positional selector reassembly
// declaration_expression (function_or_value_defn (function_declaration_left (identifier) (argument_patterns (long_identifier (identifier)) (long_identifier (identifier)))) body: (infix_expression ...))
//   — `let add x y = ...`: function name is POSITIONAL first child of function_declaration_left; params are repeated `long_identifier` children of `argument_patterns` (no per-param field name)
// declaration_expression (... body: (infix_expression (application_expression (application_expression (long_identifier_or_op (identifier)) (const (int))) (const (int))) (infix_op) (long_identifier_or_op (identifier))))
//   — `add 1 2 |> string`: NESTED application_expression (NO field labels — see Architecture Patterns), then a generic infix_expression with an `infix_op` child whose TEXT is "|>" (not distinguishable by kind)
// long_identifier_or_op (long_identifier (identifier) (identifier))   — `Inner.value` qualified access: one long_identifier with MULTIPLE identifier children (dotted segments), all-but-last = qualifier path
// match_expression (long_identifier_or_op (identifier)) block: (rules (rule pattern: (const (string)) block: ...) (rule pattern: (wildcard_pattern) block: ...))
```

**Recipe:** member method name = `method_or_prop_defn`'s `method:` field text; receiver/`this`-binding name = `instance:` field text (NOT emitted as a descriptor — matches the codebase-wide "self/this receivers carry no qualifier" convention already in `objc.rs`). Application head: recursive walk (see Architecture Patterns). `|>`/other infix ops: read `infix_op` child text.

### Gleam — cleanest of the family; visibility via child presence (verified against tree-sitter-gleam 1.0.0)

```rust
// Source: import gleam/list  import gleam/io.{println} as io_alias
//   pub type Shape { Circle(radius: Float) Square(side: Float) }
//   fn private_helper(x: Int) -> Int { x + 1 }
//   pub fn public_area(shape: Shape) -> Float { private_helper(1) list.length([1,2,3]) 0.0 }
//
// import module: (module)                                                          — `import gleam/list`
// import module: (module) imports: (unqualified_imports (unqualified_import name: (identifier))) alias: (identifier)  — `import gleam/io.{println} as io_alias`
// type_definition (visibility_modifier) (type_name name: (type_identifier)) (data_constructors (data_constructor name: (constructor_name) arguments: (data_constructor_arguments (data_constructor_argument label: (label) value: (type name: (type_identifier))))) ...)
// function name: (identifier) parameters: (function_parameters (function_parameter name: (identifier) type: (...))) return_type: (...) body: (function_body ...)   — PRIVATE fn: NO `(visibility_modifier)` child
// function (visibility_modifier) name: (identifier) ...                             — PUBLIC fn: `(visibility_modifier)` IS present as first child (positional, unlabeled)
// function_call function: (identifier) arguments: (arguments (argument value: (integer)))                       — free call
// function_call function: (field_access record: (identifier) field: (label)) arguments: (arguments (argument value: (list ...)))  — MODULE-QUALIFIED call reuses the record-field-access node shape (no dedicated "qualified call" node)
```

**Recipe:** `visibility_modifier` presence (positional child, any node with `pub`) → `Visibility::Public`; absence → recommend `Visibility::Private` (Gleam modules are fully hidden like Rust, not package-internal like Go — no cross-file `Internal` concept in Gleam). `field_access` as a call's `function:` → qualifier = `record` text, callee = `field` text.

### Elixir / Erlang — see Architecture Patterns above for the full verified shapes (repeated here only for the Descriptor recommendation)

```rust
// Recommended identity for BOTH languages' function symbols:
let arity = param_count; // Erlang: count of `args:` fields in expr_args; Elixir: count of the inner call's `arguments:` children
descriptors.push(Descriptor::Method {
    name: fn_name,
    disambiguator: arity.to_string(), // "2", "3" — is_simple_ident_char-legal, zero escaping
});
// Renders e.g. "add(2)." vs "add(3)." — no SCIP-id collision between arities.
```

### Haskell — juxtaposition, equation grouping, invisible qualified/hiding (verified against tree-sitter-haskell 0.23.1)

```rust
// Source: module MyApp (publicFn, MyType(..)) where
//   import Data.List (sort, nub)   import qualified Data.Map as Map hiding (filter)
//   data MyType = Circle Double | Square Double     newtype Wrapper = Wrapper Int
//   class Shape a where area :: a -> Double
//   instance Shape MyType where area (Circle r) = 3.14 * r * r ; area (Square s) = s * s
//   publicFn :: Int -> Int    publicFn x = privateFn x
//   privateFn :: Int -> Int  privateFn 0 = 0  privateFn x = x + 1
//   composed x = publicFn $ privateFn x    piped xs = sort . nub $ xs
//
// header module: (module (module_id)) exports: (exports export: (export variable: (variable)) export: (export type: (name) children: (children element: (all_names))))
//   — `MyType(..)` wildcard-export shape: `children: (children element: (all_names))`
// imports: (imports import: (import module: (module (module_id) (module_id)) names: (import_list name: (import_name variable: (variable)) ...)))
//   — Data.List's TWO module_id children = dotted segments, POSITIONAL (no field)
//   — `import qualified Data.Map as Map hiding (filter)` renders shape-IDENTICAL to a plain
//     `import Data.Map as Map` except for source span — "qualified" and "hiding" are NOT
//     separately visible fields/nodes in this grammar version (verified via direct A/B dump)
// data_type name: (name) constructors: (data_constructors constructor: (data_constructor constructor: (prefix name: (constructor) field: (name))) ...)
// newtype name: (name) constructor: (newtype_constructor name: (constructor) field: (field (name)))
// class name: (name) patterns: (type_params bind: (variable)) declarations: (class_declarations declaration: (signature ...))
// instance name: (name) patterns: (type_patterns (name)) declarations: (instance_declarations declaration: (function name: (variable) patterns: (patterns (parens pattern: (apply function: (constructor) argument: (variable)))) match: ...) declaration: (function ...))
//   — TWO separate `declaration: (function name: (variable) ...)` nodes, BOTH named "area" (the instance's two pattern-matched equations) — same grouping-by-name requirement as top level
// signature name: (variable) type: (function parameter: (name) result: (name))     — type sig, SIBLING of (not attached to) the function equations
// function name: (variable) patterns: (patterns (variable)) match: (match expression: (apply function: (variable) argument: (variable)))   — has a pattern (arg) → `function` kind
// bind name: (variable) match: (match expression: (literal (integer)))             — NO pattern/arg (`f = 1`) → DIFFERENT node kind `bind`, not `function`
// function name: (variable) patterns: (patterns (literal (integer))) match: ...    — `privateFn 0 = 0` (one of TWO sibling `function` nodes both named "privateFn")
// (infix left_operand: (variable) operator: (operator) right_operand: (apply ...))  — `publicFn $ privateFn x`: `$` is a GENERIC `infix`/`operator` node (same kind as `+`/`.`); the LITERAL TEXT of `operator:` must be read to detect `$` specifically
// (infix left_operand: (variable) operator: (operator) right_operand: (infix left_operand: (variable) operator: (operator) right_operand: (variable)))  — `sort . nub $ xs`: nested infix, same generic-operator-node caveat for `.`
// apply function: (apply function: (variable) argument: (variable)) argument: (variable)   — nested juxtaposition (see Architecture Patterns)
```

**Recipe:** group `function`/`bind` declarations by `name:` text (two-pass: collect all, group, then correlate each group's preceding `signature` sibling by name for the one-line signature text). Export-list membership (`exports:` field's `export:` children's `variable`/`type` text) drives real `Visibility::Public`/`Private` per D-08; absent `exports:` field entirely → all `Public` (matches D-08's stated rule). `instance … where` declarations → `RefRole::IsImplementation` reference (class name), consistent with the phase's "instance declaration is a static, verifiable fact; call-site dispatch is not" ceiling.

## Runtime State Inventory

Not applicable — this is a greenfield feature-add phase (new extractors behind new, currently-inert Cargo features). No rename/refactor/migration of existing identifiers, no stored data, no live external service config, and no OS-registered state is touched. Skipped per the "omit entirely for greenfield phases" instruction.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|---|---|---|---|---|
| cargo / rustc | Build all 8 extractors | ✓ | cargo 1.96.0 / rustc 1.96.0 | — |
| tree-sitter-{julia,r,ocaml,fsharp,elixir,erlang,gleam,haskell} crates | Grammar parsing | ✓ | pinned versions in Cargo.toml (see Standard Stack); already resolve via the existing Cargo.lock, no network fetch needed beyond normal `cargo build` | — |

No missing dependencies; no fallback needed. This phase has no dependency beyond the already-vendored/lockfile-resolved Cargo graph.

## Sources

### Primary (HIGH confidence — empirically verified this session)

- Direct `to_sexp()` AST dumps for all 8 grammars, built via a throwaway `examples/dump_ast.rs` against this repo's exact `Cargo.lock`-pinned crate versions (tree-sitter-julia 0.23.1, tree-sitter-r 1.3.0, tree-sitter-ocaml 0.25.0 — both `LANGUAGE_OCAML` and `LANGUAGE_OCAML_INTERFACE`, tree-sitter-fsharp 0.3.1, tree-sitter-elixir 0.3.5, tree-sitter-erlang 0.19.0, tree-sitter-gleam 1.0.0, tree-sitter-haskell 0.23.1); example deleted before this commit per CONTRIBUTING's own instruction
- `src/grammar.rs` — confirms which grammar fns exist, which are wired, and the pre-existing note that `LANGUAGE_OCAML_INTERFACE`/`LANGUAGE_OCAML_TYPE` are exported but unwired
- `src/symbol/descriptor.rs` — `Descriptor::Method` disambiguator mechanism + its existing round-trip test, confirming arity-string disambiguators need no escaping
- `src/extract/{objc,fortran,rust,lua,go,support}.rs` — Phase 2/3 precedent patterns (dedup-by-SCIP-id, module_symbol, macro-as-Function+Descriptor::Macro, positional-vs-field child access asymmetry already documented in `fortran.rs`)
- `.planning/phases/01-foundation-compatibility-gate-ci-hardening-ts-js-depth/01-COMPAT-VERDICTS.md` — empirical ABI PASS for all 8 grammars, resolving the STACK/FEATURES version-pin dispute
- `.planning/phases/03-established-template-extractors/03-EXECUTION-SUMMARY.md` — per-language phase execution/verification shape to replicate
- `Cargo.toml`, `bindings/node/Cargo.toml`, `bindings/python/Cargo.toml` — confirms none of the 8 feature strings are in either binding's feature list yet (BIND-01 work still pending, same-change requirement)
- `docs/supported-languages.md` — confirms all 8 rows already exist at 🟠 with the primary extension already present (doc-sync test will pass once rows flip to 🟢)

### Secondary (MEDIUM confidence)

- `.planning/research/FEATURES.md` — per-language capability targets (largely confirmed by the AST dumps above; the one place it undershoots is Erlang/Elixir's per-clause, non-grouped `fun_decl`/`function` shape, which FEATURES.md correctly flags as a design cost but doesn't show the concrete verified node shape)

### Tertiary (LOW confidence — none)

No claim in this document rests on unverified training-data alone; every node-kind/field name cited above was directly observed in a `to_sexp()` dump this session.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — versions unchanged from Phase 1's empirical ABI gate, re-confirmed resolving cleanly today
- Architecture (node kinds/fields): HIGH — every shape cited was captured from a live `to_sexp()` dump against the pinned crate, not recalled from training data
- Pitfalls: HIGH — the Elixir call-uniformity, Haskell qualified/hiding invisibility, Erlang per-clause non-grouping, and F#/Haskell nested-vs-OCaml-flat-application findings are all directly reproduced, not inferred

**Research date:** 2026-07-05
**Valid until:** these are stable, low-churn grammar crates (all MIT/Apache, months-to-years between releases per Phase 1's version table) — 90 days is a reasonable validity window; re-verify only if any of the 8 `tree-sitter-*` versions change in `Cargo.toml`.
