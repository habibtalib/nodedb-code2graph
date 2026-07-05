# Architecture Research

**Domain:** Extending a tree-sitter-based code-graph extraction library (Rust core + PyO3/napi-rs bindings) with new language extractors
**Researched:** 2026-07-05
**Confidence:** HIGH (all integration points verified against the actual repo; template mapping is MEDIUM — informed judgment against training-data knowledge of each candidate language's syntax, not independently grammar-verified per CONTRIBUTING's own "dump the real AST first" rule)

## Standard Architecture

### System Overview

```
┌───────────────────────────────────────────────────────────────────────────┐
│                        Language coverage (single source of truth)          │
│  src/lang.rs — Language enum, Language::ALL, extensions(), as_str(),       │
│  from_extension()/from_path(). Adding a variant forces a compile error in  │
│  assert_variant_in_all() until every arm is updated (no wildcard).         │
├───────────────────────────────────────────────────────────────────────────┤
│                        Grammar chokepoint                                   │
│  src/grammar.rs — the ONLY file that imports tree_sitter_* crates.          │
│  One #[cfg(feature="x")] fn x() -> Language per grammar + one              │
│  check("x", super::x()) arm in abi_versions_are_compatible().              │
├───────────────────────────────────────────────────────────────────────────┤
│                        Extraction (src/extract/)                           │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐                   │
│  │ <lang>.rs│  │ <lang>.rs│  │ <lang>.rs│  │  … x23   │  one file/module   │
│  │ struct   │  │ struct   │  │ struct   │  │          │  per language,     │
│  │ XExtractor│ │ XExtractor│ │ XExtractor│ │          │  implements the    │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘  Extractor trait  │
│       └─────────────┴──────┬──────┴─────────────┘                        │
│                    support.rs (shared helpers, mandatory reuse)            │
│                    dispatch.rs (Extractor trait + extract_file/extract_path)│
│                    mod.rs (wiring only: pub mod + pub use, feature-gated)   │
├───────────────────────────────────────────────────────────────────────────┤
│                        Resolution (language-agnostic — untouched)          │
│                        FileFacts → CodeGraph, works for free once          │
│                        extraction emits symbols/references/scopes/bindings │
├───────────────────────────────────────────────────────────────────────────┤
│                        Doc/CI guard rails                                  │
│  docs/supported-languages.md ↔ Language::ALL (sync test in lang.rs)        │
│  docs/ffi-support-matrix.md ↔ SPECS registry (sync test in ffi/sync_tests) │
│  eval/corpus/<lang>/ ≥1 case — auto-discovered by the eval harness          │
├───────────────────────────────────────────────────────────────────────────┤
│                        Bindings (generic pass-through, no per-language code)│
│  bindings/python/  Cargo.toml feature list ──▶ code2graph_core             │
│  bindings/node/    Cargo.toml feature list ──▶ (same crate, same features) │
│  Both expose only 3 functions (extract/build_graph/language_of) that       │
│  operate on Language::from_path() + serde JSON — a new Language variant    │
│  flows through with ZERO changes to bindings/*/src/lib.rs or the           │
│  generated index.js/index.d.ts, as long as the feature string is added.    │
└───────────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Verified location |
|-----------|----------------|------------------------|
| `Language` enum | Single source of truth for coverage; extension dispatch | `src/lang.rs:10-138` |
| Grammar chokepoint | Sole importer of `tree_sitter_*` crates; ABI compat gate | `src/grammar.rs` (23 fns + `abi_versions_are_compatible` test) |
| Extractor trait | Contract every language module implements | `src/extract/dispatch.rs:58-65` |
| `extract_file`/`extract_path` | Dispatch match on `Language`, feature-gated per arm | `src/extract/dispatch.rs:73-134` |
| `mod.rs` | Wiring only (`pub mod` + `pub use`, all feature-gated) | `src/extract/mod.rs` |
| `support.rs` | Shared tree-sitter helpers (mandatory reuse, not modified per-language) | `src/extract/support.rs` (607 lines) |
| `docs/supported-languages.md` | Hand-maintained coverage table, sync-tested against `Language::ALL` | tested by `supported_languages_doc_lists_each_primary_extension` in `src/lang.rs:200-213` |
| `docs/ffi-support-matrix.md` | FFI marker doc, sync-tested against `SPECS` | `src/ffi/sync_tests.rs` |
| `eval/corpus/<lang>/` | Golden-fixture regression cases, auto-discovered | e.g. `eval/corpus/go/{unique_call,scoped_call}/{*.go, expected.edges}` |
| `bindings/{node,python}/Cargo.toml` | Feature allow-list re-exported into the binding crate | both list languages explicitly (see below) |
| `bindings/{node,python}/src/lib.rs` | Generic JSON-in/JSON-out API — no per-language code | `extract`, `build_graph`, `language_of` only |
| `bindings/node/index.js`/`index.d.ts` | napi-generated JS/TS loader, committed, CI-gated against drift | `.github/workflows/test.yml` `bindings` job |

## Integration Checklist (verified against the real repo)

Every new language touches the same fixed set of files. This is the CONTRIBUTING.md recipe cross-checked line-by-line against the current code:

1. **`Cargo.toml`** (root) — three edits:
   - Add `"<lang>"` to the `default = [...]` feature list (line 24)
   - Add the feature line: `<lang> = ["dep:tree-sitter-<lang>", "_extractors"]`
   - Add the dependency line: `tree-sitter-<lang> = { version = "X.Y.Z", optional = true }`
   - (Embedded/SFC languages also depend on the host feature, e.g. Svelte's line is `svelte = ["dep:tree-sitter-svelte-ng", "typescript", "_extractors"]` — Astro will need the same `"typescript"` dependency.)

2. **`src/grammar.rs`** — the chokepoint:
   - Add `#[cfg(feature = "<lang>")] pub fn <lang>() -> Language { tree_sitter_<lang>::LANGUAGE.into() }`
   - Add `#[cfg(feature = "<lang>")] check("<lang>", super::<lang>());` inside `abi_versions_are_compatible()` (the ABI compat gate — this is where a `tree-sitter <0.24` or `>=0.27` grammar fails loudly rather than silently linking wrong)

3. **`src/lang.rs`** — five edits, all compiler-enforced (no wildcard arms):
   - New `Language::X` variant in the enum (line 10-34)
   - Add to `Language::ALL` (line 38-62)
   - Add extensions arm in `extensions()` (line 67-93)
   - Add tag arm in `as_str()` (line 96-122)
   - Add extension arm in `extensions()`'s backing match (from_extension derives automatically — no separate edit needed there)
   - The `assert_variant_in_all()` test match (line 159-186) has **no wildcard** — adding a variant without adding it here is a compile error in tests, which is the intended forcing function

4. **`src/extract/<lang>.rs`** (new file) — `struct XExtractor` implementing `Extractor` (`fn lang()`, `fn extract()`), built from a `tree_sitter::Parser` + queries, using `support.rs` helpers (`make_symbol`, `push_ref`, `collect_call_references`, `push_import_ref`, `push_type_ref`, scope/binding helpers). Unit tests live in the same file (`#[cfg(test)] mod tests`), asserting real rendered SCIP id strings (dump `to_sexp()` first per CONTRIBUTING — published grammars often disagree with a repo's `node-types.json`).

5. **`src/extract/mod.rs`** (wiring only) — add, in alphabetical position:
   - `#[cfg(feature = "<lang>")] pub mod <lang>;`
   - `#[cfg(feature = "<lang>")] pub use <lang>::XExtractor;`

6. **`src/extract/dispatch.rs`** — add the import (`#[cfg(feature = "<lang>")] use super::XExtractor;`, alphabetical block at the top) and the match arm inside `extract_file()`: `#[cfg(feature = "<lang>")] Language::X => XExtractor.extract(source, file),`

7. **`docs/supported-languages.md`** — add a table row; must contain the primary extension as a backticked cell (e.g. `` `.ex` ``) — enforced by `supported_languages_doc_lists_each_primary_extension` (`src/lang.rs:199-213`), which fails the build if the doc doesn't mention it.

8. **`eval/corpus/<lang>/<case_name>/`** — at least one directory with source file(s) + `expected.edges` (see `eval/corpus/go/unique_call/` as the reference shape: `util.go`, `main.go`, `expected.edges`). No plumbing changes needed — the harness auto-discovers corpus directories (per CONTRIBUTING: "Adding a directory is automatically picked up… no plumbing changes"). Note: several already-shipped 🟢 languages (C#, Dart, Lua, Luau, Pascal, Scala, Svelte) currently have **no** `eval/corpus/` entry despite the recipe requiring one — an existing gap in this repo, not a new one to introduce; new language phases should not repeat it.

9. **`bindings/node/Cargo.toml`** and **`bindings/python/Cargo.toml`** — add `"<lang>"` to the `code2graph_core` dependency's `features = [...]` list in **both** files (currently identical 22-entry lists at line 14 of each). This is the only binding-side change required for a pure extractor addition.

10. **CI verification** — `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features --exclude code2graph-py --exclude code2graph-node` (the `test.yml` `test` job — pyo3/napi cdylib crates can't run as cargo test binaries), plus the `bindings` job (`maturin build --release`, `napi build --release --platform`, then the committed-artifact diff check).

`docs/ffi-support-matrix.md` and `src/ffi/*.rs` are **conditionally** touched — only if the language introduces a new FFI ABI boundary (e.g. a hypothetical Objective-C↔C spec). Per PROJECT.md this milestone scopes "new resolver capabilities beyond what new extractors need" as separate/out-of-scope, so treat any FFI spec work as a call to make explicitly at planning time, not an assumed per-language step.

## Bindings Parity Mechanics (verified, not guessed)

**The core finding:** both bindings are *enum-generic adapters*, not per-language surfaces.

- `bindings/node/src/lib.rs` exposes exactly three `#[napi]` functions: `extract(file, source) -> Value`, `build_graph(files, tier) -> Value`, `language_of(path) -> Option<String>`. All three operate through `code2graph_core::{extract_path, Language}` and generic `serde_json::Value`.
- `bindings/python/src/lib.rs` mirrors this exactly with `pyo3`/`pythonize`: `extract`, `build_graph`, `language_of`, each returning a Python dict via `pythonize(py, &facts)`.
- Neither file has a match arm, enum, or list of language names. A new `Language::X` variant is reachable through `Language::from_path()` automatically. **The binding source needs zero changes when a language is added** — the only reason it wouldn't "just work" is if the binding crate wasn't compiled with that language's Cargo feature.

**Therefore the binding-side integration point is entirely in `Cargo.toml`, not `lib.rs`:**

```toml
# bindings/node/Cargo.toml and bindings/python/Cargo.toml (identical lists today)
code2graph_core = { package = "code2graph", path = "../..", default-features = false,
  features = ["serde", "rust", "python", "typescript", "go", "java", "c", "cpp", "ruby",
              "php", "shell", "swift", "kotlin", "solidity", "sql", "hcl", "csharp",
              "scala", "dart", "lua", "luau", "pascal", "svelte"] }
```

Forgetting to add `"<lang>"` here means the binding crate compiles fine (no error) but `extract()`/`extractPath()` returns `CodegraphError::UnsupportedLanguage` at runtime for that language — a silent regression with no compiler signal, unlike the `src/lang.rs` enum edits which are compile-enforced. **This is the one step in the whole recipe with no automated guard**, so it's the single highest-risk step to miss and should be explicit in the phase checklist / PR template rather than left implicit.

**Regeneration/verification steps (the real CI ones, from `.github/workflows/test.yml`):**

```bash
# Python side — no committed-artifact drift gate; CI just proves it builds
pip install maturin
maturin build --release -m bindings/python/Cargo.toml

# Node side — regenerates index.js/index.d.ts from the #[napi] surface, then
# diffs them against what's committed
cd bindings/node
npm ci
npx napi build --release --platform   # == `npm run build`
git diff --exit-code -- index.js index.d.ts
```

The CI `bindings` job (`.github/workflows/test.yml:58-95`) runs exactly this, and fails with `::error::bindings/node/index.js or index.d.ts is out of date — run 'npm run build' in bindings/node and commit the regenerated files.` if the diff isn't clean. Because `extract`/`buildGraph`/`languageOf` signatures never change from adding a language, **a pure extractor addition should regenerate to a no-op diff** — but the step should still be run (not skipped) in every language PR, since it's cheap and it's the CI gate that would catch any accidental signature drift.

**Suggested per-language-phase order** (folds bindings parity into the same phase, per PROJECT.md's key decision to avoid a trailing bindings phase):
1. Land core extractor + `lang.rs`/`grammar.rs`/`dispatch.rs`/`mod.rs` changes; `cargo test --workspace --all-features --exclude code2graph-py --exclude code2graph-node` green.
2. Add `"<lang>"` to both `bindings/*/Cargo.toml` feature lists in the same PR.
3. `cargo check -p code2graph-node -p code2graph-py` to confirm both binding crates still build with the language included.
4. Run the napi regen + diff check locally (`cd bindings/node && npm run build && git diff --exit-code -- index.js index.d.ts`); commit if non-empty.
5. Add `docs/supported-languages.md` row + `eval/corpus/<lang>/` case in the same PR (both are sync-tested / auto-discovered, not optional).

## Template Mapping: Candidate Language → Closest Existing Extractor

Per CONTRIBUTING's guidance ("the freshest structurally-similar extractor is usually the best template… pick one structurally similar to your language"), and per PROJECT.md's note that the actual AST must be dumped and verified before committing to a shape — the following is a starting hypothesis per candidate, not a substitute for the `to_sexp()` step. Confidence: MEDIUM (syntax-shape reasoning from training-data knowledge of each language, not independently grammar-verified).

| Language | Grammar crate (per docs table) | Closest template(s) | Why |
|----------|-------------------------------|----------------------|-----|
| **Elixir** (`.ex`/`.exs`) | `tree-sitter-elixir` | **Ruby** | Both are dynamic, module/class-ish container (`defmodule`/`module`+`class`) with `def`-style method walking via recursive descent; unlike Ruby, Elixir's `def`/`defp` is a *syntactic*, not runtime, visibility marker — an improvement over Ruby's `Unknown`-visibility ceiling, worth calling out in the extractor's doc comment as Ruby's does for its own limitation. |
| **Erlang** (`.erl`/`.hrl`) | `tree-sitter-erlang` (WhatsApp) | **Go** (module/file shape) + Rust's FFI-export-list pattern | File-as-module, top-level function definitions with no class inheritance mirrors Go's structural shape; visibility is an explicit `-export([foo/1, ...]).` attribute list — structurally closer to how `src/ffi/c.rs`/Rust's `#[no_mangle]` scanning collects an explicit export list than to any capitalization/keyword convention. Multiple function *clauses* per name (arity-based overloading) has no existing analog — flag as a resolver-adjacent research risk (name+arity, not just name, may be needed for precise Tier-B stitching later; extraction can still emit one `Symbol` per clause group deterministically). |
| **Gleam** (`.gleam`) | `tree-sitter-gleam` | **Rust** (lighter subset) | Explicit `pub fn`, `import`/`use`, algebraic `type` declarations — Gleam's syntax is deliberately Rust/OCaml-flavored on BEAM. Reuse Rust's `pub`-keyword visibility-arm pattern; the walk itself can stay much smaller (no traits/impls/macros), closer in *scope* to Go's extractor size. |
| **Zig** (`.zig`) | `tree-sitter-zig` | **C** (base shape) + Rust's explicit `pub` handling | File is a namespace/struct, no OOP inheritance (matches C's `Inherit: —` row), but Zig has an explicit `pub` keyword unlike C — borrow Rust's visibility-arm handling for that one axis. `@import("std")` calls read like function calls, not a syntactic `import` statement — needs bespoke import-reference detection, closer to how Lua treats `require()` as a synthetic `Import` reference (see Lua's `push_import_ref` usage) than to C's (nonexistent) import graph. |
| **Julia** (`.jl`) | `tree-sitter-julia` | **Lua** | Dynamic, module-based (`module X ... end`), function defs both as blocks (`function foo(x) ... end`) and one-line assignment form (`f(x) = expr`) — same "definition via assignment" shape Lua already handles for `local function`/table-valued locals. Multiple dispatch (many methods share a name, disambiguated by argument types) has no existing analog in any of the 23 extractors — flag as a Tier-B research risk; extraction should honestly emit one `Symbol` per method and let resolution stay at Tier-A `NameOnly` fan-out rather than fake a match. |
| **R** (`.r`/`.R`) | `tree-sitter-r` | **Lua** (assignment-based defs) with a nod to **JavaScript**'s function-expression handling | `foo <- function(...) {}` is a variable-bound function value, same shape as JS `const foo = function(){}` / Lua `local foo = function() end`. S3 generic dispatch (`method.class <- function(...)`) parallels Lua's dot-method convention (`function M.foo()`) — reuse that recognition pattern for the `generic.class` naming convention rather than modeling real class inheritance. |
| **Haskell** (`.hs`) | `tree-sitter-haskell` | **Scala** | Scala already handles trait/type-class-like constructs, pattern matching, and case-class-style algebraic data — the closest existing analog to Haskell's type classes → instances and equational (multi-clause) function definitions. Highest research risk of all candidates: point-free/operator-section style makes "call site" detection genuinely ambiguous at the syntax level; extraction should stay conservative (only detect syntactically unambiguous application) rather than guess. |
| **OCaml** (`.ml`/`.mli`) | `tree-sitter-ocaml` | **Rust** (modules + explicit visibility) with C's `.h`/`.c` split as a secondary reference | `.mli` interface files controlling visibility for a paired `.ml` implementation is structurally the same problem C solves with header/source separation — but the *content* (modules, `let` bindings, pattern matching, algebraic types) is closer to Rust's module system. Treat as two extractors sharing one walk (interface vs. implementation) the way Lua/Luau share `extract_lua_family`, if `.mli` visibility needs to feed back into `.ml` symbol visibility — otherwise start with `.ml` only and treat `.mli` as a stretch goal. |
| **Objective-C** (`.m`/`.mm`) | none confirmed in docs table (listed only as "exposes C ABI; pairs with Swift" — verify grammar availability before planning) | **C** (base: preprocessor, headers, functions) + **Swift** (`@interface`/`@implementation` class-with-methods, protocols≈interfaces, inheritance) | Objective-C is a strict C superset with Smalltalk-style bracket message sends (`[obj method:arg]`) layered on top via `@interface`/`@implementation`. Use C's extractor for the file/import/function-declaration shape and Swift's for the class/method/protocol/inheritance shape; bracket-syntax method calls need a bespoke query (no existing analog — closest *conceptually* is a qualified call with the receiver captured as `@qualifier`, same idea as Ruby/Lua's receiver-qualified calls, just different concrete syntax). |
| **Fortran** (`.f90`/`.f`) | `tree-sitter-fortran` | **Go** (procedural, no inheritance) with Rust's explicit-visibility-keyword pattern | Modern Fortran (`.f90`) has `module`/`subroutine`/`function` with explicit `public`/`private` statements — same "no class inheritance" shape as Go's row (`Inherit: —`), but with Rust-style explicit visibility rather than Go's capitalization convention. |
| **Groovy** (`.groovy`/`.gradle`) | `tree-sitter-groovy` | **Java** (primary) / **Kotlin** (secondary, for closures) | Groovy is close to a dynamically-typed Java on the same JVM class/method/import/package skeleton — safest, most mechanical template match of any candidate. Kotlin's extractor is a secondary reference for closure/lambda call-site patterns Java's doesn't need to handle. |
| **PowerShell** (`.ps1`/`.psm1`) | "grammar exists — verify compat" (docs table flags this explicitly) | **Shell** | Directly analogous: `function Verb-Noun { }` definitions, command-call references, `.`/`Import-Module`/dot-sourcing as an `Import` reference the same way Shell's extractor already treats `source`/`.` — the closest 1:1 template of any candidate. |
| **Astro** (`.astro`) | grammar TBD; docs table says "SFC — embedded-script pattern (like Svelte)" | **Svelte** (explicit, per CONTRIBUTING and PROJECT.md) | Parse the host document, locate the frontmatter/script node, run `super::typescript::extract_ecmascript` on its inner source, then `support::shift_offsets` every byte offset back into the host file. This is the *named* reference implementation in both CONTRIBUTING.md and PROJECT.md — no ambiguity here, unlike every other row in this table. |

**Two languages (Julia, R) share no existing template for their signature dispatch mechanism** (multiple dispatch / S3 generics) — this is the most likely source of a Tier-B design question during those phases, not just an extraction mechanic; flag both for deeper phase-specific research on arrival, per the milestone's own PITFALLS-flagging pattern.

## Suggested Phase Grouping / Build Order

**Phase 0 (prerequisite, not a language phase): grammar-compat spike across all 13 candidates.** For each, resolve the crates.io grammar crate, confirm license compatibility, and — per CONTRIBUTING's own advice — wire up steps 1-2 of the recipe just far enough to call `abi_versions_are_compatible`'s `check()` against it, *before* committing any phase to it. This directly executes PROJECT.md's Key Decision ("gate every candidate language on verified grammar compat before planning it") and will likely shrink the 13-language list — the docs table already flags PowerShell and Objective-C as needing explicit compat verification, and Fortran/Groovy grammar maturity is unconfirmed from this repo's records alone.

Grouping the remaining candidates by template similarity, independence, and risk:

1. **Phase 1 — Embedded/SFC, single quick win:** **Astro** (Svelte template, explicit reference implementation, zero new resolver work). Validates the full recipe + bindings/CI mechanics end-to-end on the *lowest*-risk candidate before harder families start.

2. **Phase 2 — Shell-family:** **PowerShell** (Shell template, near-1:1 mapping). Second quick, independent win; proves the "reuse a close template" path a second time on a different template.

3. **Phase 3 — JVM family:** **Groovy** (Java/Kotlin templates). Mature, stable JVM grammars; lowest grammar-risk of the "new" languages; benefits directly from two already-mature sibling extractors in the same repo.

4. **Phase 4 — Apple/C-adjacent native:** **Objective-C** (C + Swift templates). Bracket message-send syntax is a genuinely new shape (moderate extraction risk), and it sits next to the existing C FFI spec conceptually — do this after the bindings/CI pipeline has been proven twice (Phases 1-2) so any FFI-adjacent scope question ("do we add an Objective-C↔C spec, or just extract facts?") is decided with the rest of the milestone's shape already settled.

5. **Phase 5 — Systems/procedural, explicit-pub siblings:** **Zig** (C/Rust templates) and **Fortran** (Go/Rust templates), grouped together as independent but structurally similar ("no inheritance, explicit visibility keyword, C-adjacent"); can be built in parallel by different contributors since neither depends on the other.

6. **Phase 6 — BEAM family:** **Elixir**, **Erlang**, **Gleam**, grouped for domain/narrative coherence (same VM ecosystem) even though their closest templates differ (Ruby / Go+FFI-pattern / Rust respectively). Suggested internal order: Elixir first (closest template, cleanest visibility semantics), then Erlang (explicit export-list visibility is a known pattern once Rust's FFI-export scanning is the reference), then Gleam last (benefits from lessons of both, and its Rust-like syntax is easiest once the family's shared BEAM conventions — module-as-file, atom-like naming — are already familiar from the first two).

7. **Phase 7 — Dynamic scientific/scripting:** **Julia** and **R**, grouped together (shared Lua-derived template, shared "no existing dispatch-mechanism analog" research risk flagged above). Do this after the BEAM family so the team has already exercised "document an honest Tier-A-only ceiling for a resolution mechanism we don't model" once (Erlang's arity-based clauses) before hitting it twice more here.

8. **Phase 8 — Statically-typed functional/ML family:** **Haskell** and **OCaml**, grouped together (Scala/Rust templates, hardest resolution ceiling: type classes, currying, point-free style, `.mli`/`.ml` visibility split). Deliberately last — highest design risk of the whole candidate set, most likely to need a CONTRIBUTING-style Discussion if extraction facts start pushing on the fact-schema boundary (e.g., should a `.mli` signature be a separate `Symbol` from its `.ml` definition, or the same one carrying two spans?). Flag explicitly for deeper phase-specific research before execution, per the milestone's PITFALLS-flagging convention.

**Cross-cutting rule for every phase:** bindings parity (the two `Cargo.toml` feature-list edits + the napi regen/diff check) is a checklist item *inside* each language's PR, not a trailing "bindings phase" — this matches PROJECT.md's explicit Key Decision and avoids the drift-compounding failure mode CI already gates against.

## Anti-Patterns (specific to this integration)

### Anti-Pattern 1: Trusting a grammar's GitHub `node-types.json` over the actual crate version

**What people do:** Write extractor queries against the node/field names documented in the grammar repo's README or `node-types.json`.
**Why it's wrong:** CONTRIBUTING explicitly warns published crates.io versions often lag or diverge from the repo's current grammar — node names, field labels, and wrapper nodes differ. This burns a review round-trip when caught in code review instead of before writing code.
**Do this instead:** Wire up steps 1-2 (feature + grammar fn), drop a throwaway `examples/` binary printing `tree.root_node().to_sexp()` for representative snippets of the *exact pinned version*, read the real tree, then delete the scratch file — exactly as CONTRIBUTING prescribes.

### Anti-Pattern 2: Adding the Cargo feature everywhere except the bindings

**What people do:** Land the core extractor, update `Cargo.toml`/`lang.rs`/`grammar.rs`/`dispatch.rs`, get `cargo test --workspace --all-features` green, and consider the language "shipped."
**Why it's wrong:** The binding crates (`code2graph-node`, `code2graph-py`) pin their own explicit `features = [...]` list in their own `Cargo.toml` — they do **not** inherit `default-features` from the core crate (both set `default-features = false`). Without the matching feature string added there, `extract()`/`extractPath()` silently returns `UnsupportedLanguage` for that language at runtime in both npm and PyPI packages, with no compiler or CI signal pointing at the cause (the core crate's own tests all pass fine).
**Instead:** Treat the two `bindings/*/Cargo.toml` feature-list edits as a mandatory step of the same PR/phase, not an afterthought — per PROJECT.md's explicit Key Decision to keep bindings parity inside each language phase.

### Anti-Pattern 3: Forcing a shared-extractor pattern (like Lua/Luau or Svelte's embedded pattern) onto languages that only share an ecosystem, not a grammar

**What people do:** Seeing Elixir/Erlang/Gleam grouped as "BEAM family" and assuming they should share one parameterized extractor function the way `extract_lua_family` serves both Lua and Luau (same grammar family, near-identical syntax).
**Why it's wrong:** Lua/Luau share a grammar lineage and near-identical syntax — that's why one parameterized function works. Elixir, Erlang, and Gleam have three unrelated grammars and three very different concrete syntaxes (`defmodule`/`def`, `-module`/`-export`, `pub fn`/`type`); forcing a shared walker would produce an unnatural abstraction with no grammar-level justification.
**Instead:** Group them in the roadmap for narrative/domain coherence and shared research risk (arity-based dispatch, atom-like naming conventions), but implement as three fully independent extractors, each following its own closest template from the mapping table above.

## Sources

- `src/lang.rs`, `src/grammar.rs`, `src/extract/dispatch.rs`, `src/extract/mod.rs`, `src/extract/lua.rs`, `src/extract/ruby.rs` — read in full/part, this repo, 2026-07-05
- `Cargo.toml` (root), `bindings/node/Cargo.toml`, `bindings/python/Cargo.toml`, `bindings/node/src/lib.rs`, `bindings/python/src/lib.rs`, `bindings/node/index.d.ts`, `bindings/node/package.json`, `bindings/python/pyproject.toml` — read in full, this repo, 2026-07-05
- `.github/workflows/ci.yml`, `.github/workflows/test.yml` — read in full, this repo, 2026-07-05
- `CONTRIBUTING.md` §"Adding a Language", §"When a Language Has No Usable Grammar" — read in full, this repo, 2026-07-05
- `docs/supported-languages.md` — read in full, this repo, 2026-07-05 (candidate grammar crate names, e.g. `tree-sitter-elixir`, `tree-sitter-erlang`, `tree-sitter-gleam`, `tree-sitter-zig`, `tree-sitter-julia`, `tree-sitter-r`, `tree-sitter-haskell`, `tree-sitter-ocaml`, `tree-sitter-fortran`, `tree-sitter-groovy`, are as documented in this table — not independently re-verified against crates.io in this research pass; Phase 0's grammar-compat spike must do that verification before planning commits to any of them)
- `eval/corpus/go/` directory listing — read in full, this repo, 2026-07-05 (corpus case shape)
- `.planning/PROJECT.md`, `.planning/codebase/ARCHITECTURE.md`, `.planning/codebase/STRUCTURE.md` — read in full, this repo, 2026-07-05

---
*Architecture research for: code2graph language-expansion milestone (Elixir, Erlang, Gleam, Zig, Julia, R, Haskell, OCaml, Objective-C, Fortran, Groovy, PowerShell, Astro)*
*Researched: 2026-07-05*
