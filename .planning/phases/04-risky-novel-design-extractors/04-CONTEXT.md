# Phase 4: Risky & Novel-Design Extractors - Context

**Gathered:** 2026-07-05
**Status:** Ready for planning
**Mode:** `--auto` (recommended defaults selected; choices logged in 04-DISCUSSION-LOG.md)

<domain>
## Phase Boundary

Ship eight extractors end-to-end — Julia, R, OCaml, F#, Elixir, Erlang, Gleam, Haskell (LANG-02, LANG-03, LANG-04, LANG-11, LANG-12; all eight passed Phase 1's empirical ABI gate, so LANG-12's full sub-family is in scope). Each carries a novel call/dispatch shape with no in-repo template — the phase's defining rule is **honest ceilings over guessed precision**. Full recipe + bindings parity per language (Phases 2–3 practice). No resolver changes.

</domain>

<decisions>
## Implementation Decisions

### Per-language extraction targets (table stakes + locked ceilings)
- **D-01 Julia** (`.jl`): `module`, `function` (long + short `f(x) = ...` forms), `struct`/`abstract type`, `macro` declarations; `using`/`import` → Imports; calls → Calls with receiver/module qualifiers where syntactic (`Base.push!`). CEILING: multiple dispatch — call refs stay NameOnly fan-out (resolver's job), never method-signature-matched; `@macro` invocations recorded as calls to the macro name only; `eval`/metaprogramming unresolved.
- **D-02 R** (`.R`, `.r`): assignment-based function symbols (`f <- function(...)`, also `=` and `<<-` where AST-unambiguous), S4 `setClass`/`setGeneric`/`setMethod` calls recorded as calls (NOT synthesized into class symbols), `library()`/`require()`/`requireNamespace()` → Imports, calls incl. namespace-qualified `pkg::fn` (qualifier captured). Visibility honestly `Unknown` (no in-language signal; NAMESPACE files out of scope). CEILING: NSE/`eval(parse(text=...))` documented, never attempted.
- **D-03 OCaml** (`.ml`, `.mli`): `let` bindings (top-level functions/values), `module`/`module type` declarations, `type` declarations, `open`/`include` → Imports, calls (juxtaposition application — extract the applied identifier head, qualifiers from `Module.fn` paths). `.mli` files extract as their own file's facts (signatures = declarations); CEILING: `.ml`↔`.mli` cross-file correlation explicitly out of scope this milestone; functors table-stakes (declaration only); PPX unresolved.
- **D-04 F#** (`.fs`, `.fsi`): reuse the ML-family approach validated by OCaml in this same phase (implement OCaml FIRST): `let` bindings, `module`/`namespace`, `type` declarations (records/unions/classes), `open` → Imports, calls incl. dotted access qualifiers, member definitions. CEILING: computation expressions/type providers unresolved; SRTP not modeled.
- **D-05 Elixir** (`.ex`, `.exs`): `defmodule` → module symbols, `def`/`defp` → functions with REAL visibility (def=Public, defp=Private — clean signal), `defmacro`, `alias`/`import`/`require`/`use` → Imports, calls incl. `Module.fun(args)` qualifiers. CEILING: macros/`use` expansion never attempted (record the call, stop); arity is part of identity convention — include `/arity` in the symbol descriptor ONLY if the SCIP scheme accommodates it cleanly, else name-only with arity in signature text (Claude's discretion, document choice).
- **D-06 Erlang** (`.erl`, `.hrl`): `-module` → module symbol, function clauses grouped by name/arity → one function symbol per name/arity, `-export` lists → REAL visibility (exported=Public, else Private), `-import`/`-include`/`-include_lib` → Imports, calls incl. remote `mod:fun(...)` qualifiers. CEILING: dynamic `apply/3` unresolved; preprocessor macros table-stakes.
- **D-07 Gleam** (`.gleam`): `pub fn`/`fn` → REAL visibility, `type` declarations, `import` → Imports, calls with module qualifiers. Cleanest of the family — full table stakes expected.
- **D-08 Haskell** (`.hs`): top-level function bindings (grouped equations → one symbol), `data`/`newtype`/`type`/`class`/`instance` declarations, `module X (exports)` header → REAL visibility from export lists (exported=Public, else Private; no export list = all Public), `import` (qualified/hiding forms) → Imports, calls (juxtaposition application head; operator applications only where AST-unambiguous). CEILING: type classes' dispatch, Template Haskell, and operator sections beyond simple application unresolved.

### Sequencing & structure
- **D-09:** One plan per language. Sequential waves in this order: OCaml → F# (depends on OCaml's ML-family shape) → Gleam → Elixir → Erlang → Haskell → Julia → R. Rationale: establish the ML template first (F# reuses it per roadmap criterion 4), then the cleaner BEAM members, then the two novel-dispatch scripting languages.
- **D-10:** Full recipe per language (Phases 2–3 practice): enum variant + dispatch, feature gains `_extractors` + default flip, extractor with module-doc ceilings, unit tests with real SCIP ids, ≥1 corpus `scoped_call` case, docs row 🟠→🟢 with honest capability columns + Notes ceilings, both bindings feature lists same-change, napi no-op verify per plan.
- **D-11:** Verification pattern per Phase 2/3 precedent: `cargo check --no-default-features --features <lang>` isolation + `cargo test --all-features` + fmt/clippy gates; resolver-test gap stays deferred (Phase 2 deferred-items.md).

### Claude's Discretion
- AST node names from `to_sexp()` dumps against pinned crates ONLY (research step; grammars: tree-sitter-julia, tree-sitter-r, tree-sitter-ocaml, tree-sitter-fsharp, tree-sitter-elixir, tree-sitter-erlang, tree-sitter-gleam, tree-sitter-haskell — names per Cargo.toml).
- Arity-in-identity choice for Elixir/Erlang (D-05/D-06) — document whichever is chosen.
- OCaml `.mli` SymbolKind mapping (declaration kinds).
- Corpus case content per language.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Recipe & templates
- `CONTRIBUTING.md` §"Adding a Language" + AST-dump tip
- `src/extract/rust.rs` (module/path conventions), `src/extract/lua.rs` (script-language shape for Julia/R), `src/extract/go.rs` (capital-export visibility precedent for Haskell/Erlang-style export rules)
- `src/extract/objc.rs`, `src/extract/fortran.rs` (Phase 3 — freshest extractors, current best practice)
- `src/extract/support.rs` — mandatory helpers

### Phase 1–3 outputs
- `.planning/phases/01-foundation-compatibility-gate-ci-hardening-ts-js-depth/01-COMPAT-VERDICTS.md` — all eight grammars ABI-verified PASS, pinned versions
- `.planning/phases/03-established-template-extractors/03-EXECUTION-SUMMARY.md` — the per-language end-to-end pattern that just shipped five languages
- `.planning/phases/02-quick-win-extractors-astro-powershell/deferred-items.md` — resolver-test gap (D-11)

### Research inputs
- `.planning/research/FEATURES.md` — per-language capability targets/ceilings (Julia/R/OCaml/Haskell/BEAM sections)
- `.planning/research/SUMMARY.md` — phase risk flags (BEAM arity identity, Julia/R dispatch, Haskell/OCaml juxtaposition + scanner risk)

### Validation & docs & bindings
- `eval/corpus/powershell/scoped_call/` and `eval/corpus/zig/scoped_call/` — corpus shape
- `docs/supported-languages.md` — eight rows to move 🟠→🟢
- `bindings/node/Cargo.toml`, `bindings/python/Cargo.toml` — feature lists

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- Grammar fns + ABI arms for all eight languages already in `src/grammar.rs` (Phase 1); features exist grammar-only, non-default
- `feature-isolation` CI matrix already lists all eight features
- napi verify flow proven (node_modules present)

### Established Patterns
- One feat commit per language on main; TDD RED→GREEN inside worktrees/plans; append-only shared-file edits; docs sync tests fire on enum variants without rows

### Integration Points
- `Cargo.toml`, `src/lang.rs`, `src/extract/{mod,dispatch}.rs`, new `src/extract/{julia,r,ocaml,fsharp,elixir,erlang,gleam,haskell}.rs` (match feature names: `fsharp` per Cargo.toml), `eval/corpus/`, `docs/supported-languages.md`, `bindings/{node,python}/Cargo.toml`

</code_context>

<specifics>
## Specific Ideas

- Roadmap criteria are explicit per language; criterion 4 mandates F# reuses the OCaml/ML-family template validated in this same phase — hence D-09's ordering.
- Real visibility signals exist and MUST be extracted (not Unknown) for: Elixir (def/defp), Erlang (-export), Gleam (pub), Haskell (export lists). Julia/R stay honest at their documented levels.

</specifics>

<deferred>
## Deferred Ideas

- OCaml `.ml`↔`.mli` cross-file correlation (explicitly out of scope per roadmap criterion 3)
- R NAMESPACE-file visibility, S4 class synthesis
- Julia method-signature dispatch modeling (resolver-tier work, out of milestone)
- Haskell operator-section/point-free call modeling beyond simple application
- Scanner-heavy grammars (Haskell, OCaml) on non-Linux bindings CI (DEPTH-03, v2)

</deferred>

---

*Phase: 04-risky-novel-design-extractors*
*Context gathered: 2026-07-05*
