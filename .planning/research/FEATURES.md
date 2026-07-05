# Feature Research

**Domain:** Code-graph extraction (tree-sitter → symbols/references/edges) — language-expansion milestone
**Researched:** 2026-07-05
**Confidence:** MEDIUM-HIGH (grammar-compat claims verified directly against crates.io dependency manifests, HIGH confidence; language-semantic claims from training knowledge, cross-checked against project conventions in `python.rs`/`lua.rs`/`java.rs`/`rust.rs`/`go.rs`, MEDIUM confidence — recommend a `to_sexp()` dump per CONTRIBUTING before each extractor lands)

## 0. Precondition: Grammar Compatibility Gate (verified 2026-07-05)

Every capability claim below is moot if the grammar crate can't compile against this project's pinned
`tree-sitter >=0.24, <0.27`. I checked each candidate's **published Cargo dependency manifest** on
crates.io (not just "does a crate exist") — a crate that exists but pins `tree-sitter = "^0.23"` cannot
be built into the same binary as this project's `tree-sitter` (Cargo would need two incompatible
`Language` types; this is exactly the "don't bridge versions" trap CONTRIBUTING.md warns about). This
is a **STACK/feasibility finding**, but it directly bounds what's buildable at all, so it belongs here
as a precondition to the per-language feature targets.

| Language | Crate (latest) | Its `tree-sitter` req | Compatible with `>=0.24,<0.27`? |
|---|---|---|---|
| Elixir | `tree-sitter-elixir` 0.3.5 | `^0.23.0` (→ 0.23.x only) | **NO** — currently incompatible |
| Erlang | `tree-sitter-erlang` 0.19.0 | `^0.23` (→ 0.23.x only) | **NO** — currently incompatible |
| Gleam | `tree-sitter-gleam` 1.0.0 | `^0.23` (→ 0.23.x only) | **NO** — currently incompatible |
| Haskell | `tree-sitter-haskell` 0.23.1 | `^0.23` (→ 0.23.x only) | **NO** — currently incompatible |
| Zig | `tree-sitter-zig` 1.1.2 | `^0.24.5` | YES |
| Julia | `tree-sitter-julia` 0.23.1 | `^0.24` | YES |
| R | `tree-sitter-r` 1.3.0 | `^0.24.7` | YES |
| OCaml | `tree-sitter-ocaml` 0.25.0 | `^0.26` | YES |
| Objective-C | `tree-sitter-objc` 3.0.2 | `^0.24` | YES |
| Fortran | `tree-sitter-fortran` 0.6.0 | `^0.26.3` | YES |
| Groovy | `tree-sitter-groovy` 0.1.2 | `^0.24` | YES — but v0.1.2 is very young/immature |
| PowerShell | `tree-sitter-powershell` 0.26.4 | `^0.26.5` | YES |
| Astro | `tree-sitter-astro-next` 0.1.1 (NOT `tree-sitter-astro` — that name doesn't exist on crates.io) | `^0.26.5` | YES — but single-maintainer, one release, independent (non-withastro-org) project; verify with `to_sexp()` before committing |

**Implication for `docs/supported-languages.md`:** four of the thirteen 🟠-listed candidates (Elixir,
Erlang, Gleam, Haskell) are, as of this check, **not actually buildable** under the current
`tree-sitter` pin — the doc's "believed available" caveat is doing real work here. This doesn't
necessarily mean re-labeling them 🔴 (grammar maintainers may catch up; recheck at phase-planning
time), but the roadmap should NOT schedule these four alongside the nine that are verified compatible
today without re-verifying, and should budget zero engineering time until an upstream release closes
the gap. This is the single highest-leverage finding for phase sequencing: **sequence the nine
compatible languages first; treat the four incompatible ones as a recurring compat-check item, not a
committed phase.**

## Feature Landscape

Every "supported" language in this project needs the same underlying facts (per `docs/supported-languages.md`):
**Symbols** (SCIP-aligned id, kind, byte span, one-line signature, declared `Visibility`) and
**References** by role (`Call`, `Import`, `IsImplementation`, `TypeRef`, `Read`, `Write`), each carrying
a `Confidence` and `Provenance`. The matrix columns (Calls/Imports/Inherit/Type-ref/Read-Write/Entry-pts)
are literally which reference roles + entry-point detection a given extractor emits. "Table stakes" for
this project is stricter than most domains: **a language isn't "supported" until it emits real
Calls + Imports**, because Tier-A (`SymbolTableResolver`) needs both to produce any resolved edges at
all — a symbols-only extractor produces an isolated node graph with no edges, which fails the project's
own bar ("supported = extraction depth, not merely the file parses").

### Table Stakes (Every New Language Must Emit These)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Symbols (defs) with correct `SymbolKind` + byte span + one-line signature | Floor for the whole schema; nothing else is meaningful without it | LOW–MEDIUM (varies by how many def-shapes a language has) | Every extractor in `src/extract/` does this first; reuse `make_symbol`, `one_line_signature` |
| Qualified identity (namespace descriptors from file path or module declaration) | SCIP-aligned symbol ids must be globally unique and stable across files | LOW (file-path-derived, like Lua/Luau/Rust) to MEDIUM (declaration-derived, like Elixir `defmodule`, OCaml `module`) | File-path derivation is the cheap default when a language has no explicit namespace keyword |
| `Calls` reference role | Tier-A resolution needs both ends of an edge; without calls there are no edges at all | LOW–MEDIUM | The one column every 🟢/⭐ row has filled; the true floor |
| `Imports` reference role | Cross-file edges (the project's core differentiator) require import bindings to link definitions across files | LOW (explicit `import`/`use` keyword) to MEDIUM (call-shaped imports like Lua's `require()`, R's `library()`) | Precedent: Lua's `require()` is a plain call re-tagged as `Import` (`collect_require_imports`) — the exact pattern needed for R's `library()`/`require()` |
| Declared `Visibility` tag (`Public`/`Internal`/`Protected`/`Private`/`Unknown`) | Neutral fact every symbol carries; `Unknown` is a legitimate, honest answer — never guessed | LOW (explicit keyword) to genuinely hard (no in-language visibility at all) | See per-language table below — several candidates (R, Julia, PowerShell) have **no reliable in-source visibility signal**, and `Unknown` is the correct, honest emission, not a gap to "fix" |

### Differentiators (Full Depth — Tier-B Eligible)

| Feature | Value Proposition | Complexity | Notes |
|---------|--------------------|------------|-------|
| `scopes` + `bindings` (Tier-B eligibility) | Upgrades a language from Tier-A name-fanout to `Scoped`/`Exact` precision with **no resolver change** — the single highest-leverage extractor investment per CONTRIBUTING | MEDIUM | Every 🟢/⭐ row already does this; new languages should budget for it in the same PR, not as a follow-up, per PROJECT.md's "bindings parity ... not a trailing phase" decision |
| `Inherit` (`IsImplementation`) reference role | Real value for OOP/typeclass-shaped languages (Objective-C `@interface … : Base <Protocol>`, Haskell `instance Class Type`, Groovy `extends`/`implements`, Fortran `type, extends(Base)`) | LOW–MEDIUM once the def-shape is known — it's a static AST relationship, not a runtime one | Skip entirely for languages with no inheritance concept (Gleam's variant types, Zig's structs — same honest `—` as Go's row today) |
| `Type-ref` reference role | Precise for statically-typed candidates (Zig, OCaml, Haskell, Fortran); genuinely hard for dynamically-typed ones (R, Julia, Groovy, PowerShell) without full type inference | MEDIUM–HIGH | This is where "the type-inference ceiling is real" (per docs) bites hardest — multiple-dispatch languages (Julia) can have several definitions sharing one name, disambiguated only by argument type, which pure syntax can't resolve |
| `Read`/`Write` reference role | Data-flow signal; mechanical once scopes exist (assignment-target vs. expression-context walk) | LOW–MEDIUM | Every 🟢/⭐ row already emits this; same shape applies to every new language |
| Entry-points (`Main`, `HttpRoute`) | Attack-surface marker, consumer-facing differentiator; currently only Rust/Python/Go/Java have detectors | LOW (once a language's call/decorator/attribute shape is known — this project already has 3 working templates) | See §3 (TS/JS gap) below — the biggest immediately-actionable entry-point item is closing the **existing** TS/JS gap, not a new-language item |

### Anti-Features / Honest Ceilings (Where Extraction Must Stop, Not Guess)

| Anti-feature | Why it looks tempting | Why it's wrong here | What to do instead |
|---|---|---|---|
| Macro/metaprogramming expansion (Elixir `defmacro`/`use`, Erlang `-define`, Haskell Template Haskell, Julia `@macro`, OCaml PPX) | "Just expand the macro and extract from the expanded code" would recover real facts | Requires running the language's own compiler/macro-expander — turns a pure-syntax tool into a build-dependent one, violating the project's durability bar (same reasoning as "don't bridge tree-sitter versions") | Emit the macro **invocation site** as a `Call` reference to the macro name at `Heuristic`/`NameOnly` confidence; never fabricate what it expands to |
| Non-standard evaluation / `eval` of strings (R `eval(parse(text=...))`, Julia `Meta.parse`/`eval`, PowerShell `Invoke-Expression`, R formulas `y ~ x`) | Common in idiomatic code (R's `dplyr` verbs, PowerShell scripting) — skipping it feels like a big recall hit | Genuinely undecidable statically; this is the same "type-inference ceiling ... we don't fake past it" principle applied to code-as-data | Leave unresolved; do not synthesize a reference for string/AST arguments that are never executed as literal calls |
| Multiple-dispatch / overload resolution (Julia methods on the same name, C++ overloads) | Tempting to "pick the most likely" overload from argument count | Precision contract for Tier-B is **zero false positives**; guessing an overload is fabricating precision | Cap at Tier-A `NameOnly` fan-out across all methods sharing the name; never promote to `Scoped`/`Exact` without real type info |
| Cross-artifact visibility correlation (OCaml `.mli`, R `NAMESPACE`, PowerShell `.psd1` `FunctionsToExport`) | The "real" public API genuinely lives in a companion file, so correlating it seems like the honest thing to do | Materially larger scope than a single-file extractor pass (needs directory-level file pairing, a different shape from every extractor today) | Table stakes: emit `Public`/`Unknown` per file in isolation (matches Ruby/JS's existing honest-`Unknown` precedent). Cross-file correlation is a legitimate **stretch** goal, not part of "supported," and should be its own scoped follow-up if pursued |
| Runtime reflection / dynamic dispatch (Objective-C `performSelector:`, Groovy `methodMissing`, R6/S4 dispatch, PowerShell `& $scriptblock`) | These are common, real code paths worth "supporting" | No static AST-only technique resolves a callee whose name is itself a runtime value | Emit no reference for computed-callee call sites (same choice already made for Ruby's dynamic `method_missing` — `Type-ref: —` in the existing matrix) |
| COMMON blocks / global-by-position state (Fortran) | Looks like it should map to Read/Write like any other variable | No name-based binding exists — position in the block, not a declared symbol, determines identity, and requires cross-file layout agreement | Skip Read/Write tracking on COMMON members; document as a known gap, not silently wrong data |

## Per-Language Capability Targets

Legend for "Matrix target": **TS** = table-stakes only achievable (Calls+Imports, `Inherit`/`Type-ref`/`Read-Write` at `—`); **Full** = Tier-B depth achievable (all six columns populatable, modulo the language's own honest `—`/`Unknown` cells, exactly like Go's `Inherit: —` today).

### Elixir (`.ex`, `.exs`) — BLOCKED on grammar compat today

- **Definition kinds:** `defmodule` (module/namespace unit), `def`/`defp` (public/private functions), `defmacro`/`defmacrop` (macro *definitions*), `defstruct`, `defprotocol`/`defimpl` (protocol = interface, impl = `IsImplementation`), `defguard`.
- **Reference kinds:** calls — local (`func(args)`), remote (`Module.func(args)`), and pipe-desugared (`a |> f(b)` is a call to `f(a, b)`; the extractor must recognize `|>` and inject the piped value as the callee's first argument, or emit an honest partial reference rather than mis-attributing the call). Imports: `alias` (rename only, no scope injection), `import` (brings functions into unqualified scope — genuine `Import` reference), `require` (enables macro use, not a value import), `use` (invokes a macro's `__using__` callback — can inject arbitrary code, **not statically knowable**, the project's honest ceiling).
- **Visibility:** `def` = Public, `defp` = Private. Clean AST-level distinction — no runtime ambiguity (unlike Ruby). This is a genuine win over several already-🟢 languages.
- **Namespace convention:** `defmodule Foo.Bar do` → dotted qualified name, nestable.
- **Gotchas / ceiling:** macros (`defmacro`, `use`) are the wall — capture invocation syntax only, never expansion. Pipe-operator desugaring is a real (solvable) gotcha, not a ceiling.
- **Complexity:** M (once grammar unblocked) — clean def/defp shape similar to Python/Ruby extractors; macro-ceiling documentation is the main design cost, not the mechanics.
- **Matrix target:** Full, except macro/use-injected code (documented ceiling, not a blank cell).

### Erlang (`.erl`, `.hrl`) — BLOCKED on grammar compat today

- **Definition kinds:** module attribute `-module(name).` sets the namespace for the whole file; function clauses `name(Args) -> Body.` — **identity is `name/arity`, not per-clause**: multiple pattern-matching clauses of the same name+arity are one symbol, but the same name at a *different* arity is a distinct symbol (mirrors Elixir/Julia's arity- or type-keyed identity problem). Records: `-record(name, {fields}).`. Macros: `-define(NAME, value).` (preprocessor, textual).
- **Reference kinds:** calls — local (`func(Args)`), remote (`Mod:func(Args)`). Imports: `-import(Module, [func/arity, ...]).` (explicit, arity-qualified — genuinely precise). Includes: `-include("file.hrl").` (textual file inclusion, C-`#include`-like — not a symbol-level import).
- **Visibility:** `-export([name/arity, ...]).` is the **sole** mechanism — unexported functions are private. Unambiguous, comparable to Go's capitalization convention.
- **Namespace convention:** one module per file via `-module`, matching the filename by Erlang convention (not enforced by the grammar).
- **Gotchas / ceiling:** `-define` macros are preprocessor textual substitution — tree-sitter parses the raw, unexpanded text, so macro-invocation sites are calls to unresolvable pseudo-identifiers (same ceiling shape as C's `#define`, which this project already treats as out of `Imports` scope for C).
- **Complexity:** M — attribute-based `-export` visibility is actually cleaner than most languages; the arity-keyed identity and macro-ceiling are the real design work.
- **Matrix target:** Full, with `Imports` genuinely precise (arity-qualified) — a differentiator over most candidates.

### Gleam (`.gleam`) — BLOCKED on grammar compat today

- **Definition kinds:** `pub fn`/`fn` (function), `pub type`/`type` (custom types/variants, Rust-`enum`-like), `const`, `import` (module-path statements). BEAM-targeting but Rust/Elm-like syntax — deliberately has **no macro system**.
- **Reference kinds:** calls (`func(args)`, `module.func(args)`), imports (`import gleam/list` — file-path-like module addressing, similar to Rust's `use`), type-refs (custom type usage, generics).
- **Visibility:** explicit `pub` keyword — clean binary distinction, zero ambiguity (best-in-class alongside Rust/Go).
- **Namespace convention:** file path = module path, same convention as Rust/Go (`src/my_app/user.gleam` → `my_app/user`).
- **Gotchas / ceiling:** genuinely the **cleanest of the three BEAM candidates** for extraction — no macros, no dynamic dispatch, explicit visibility. The only real gotcha is generics/type inference depth, same ceiling as any statically-typed language without a full type checker.
- **Complexity:** S–M (once grammar unblocked) — likely the easiest "new" language on this whole list once compat is resolved.
- **Matrix target:** Full — few honest `—`/`Unknown` cells expected.

### Zig (`.zig`) — grammar-compatible today

- **Definition kinds:** `fn` declarations (`pub fn`/`fn`); top-level `const`/`var` bindings, since **Zig has no dedicated `struct` keyword for declaration** — `const Foo = struct { ... };` is a const binding whose value happens to be a struct/enum/union literal, so struct/enum discovery means recognizing "top-level `const X = struct {...}`" as a definition shape, not a single dedicated node kind; `test "name" { ... }` blocks.
- **Reference kinds:** calls (`foo()`, `ns.foo()`); imports via the `@import("std")` / `@import("./foo.zig")` **builtin function call** (not a keyword — must special-case the identifier `@import` as an `Import` reference, resolving relative-path string arguments the same way Rust's `mod` resolves files); type-refs (struct field types, `comptime T: type` generic parameters).
- **Visibility:** `pub` keyword prefix — clean, Rust/Go-like binary distinction.
- **Namespace convention:** one implicit module per file (Zig's `@import("./x.zig")` = the file itself is the module/struct namespace).
- **Gotchas / ceiling:** `comptime` generics (`fn List(comptime T: type) type`) are functions that return *types* at compile time — the call site is capturable syntactically, but what it *produces* (the instantiated type) requires compile-time evaluation the extractor won't perform. Cap `Type-ref` on generic-parameterized code at `Scoped`, never `Exact`.
- **Complexity:** M — `pub` and `@import` are individually simple, but the const-as-struct-declaration shape needs careful `SymbolKind` classification logic, and comptime bounds `Type-ref` depth.
- **Matrix target:** Full for `Calls`/`Imports`/`Visibility`; `Type-ref` capped short of `Exact` on comptime-generic code (documented ceiling, not a bug).

### Julia (`.jl`) — grammar-compatible today

- **Definition kinds:** `function name(args) ... end` and short-form `f(x) = x^2`; `module Name ... end`; `struct`/`mutable struct`; `abstract type`; `macro name(...) ... end` (macro *definitions*, distinct from macro *use* `@name`).
- **Reference kinds:** calls (`f(x)`, `Base.sin(x)`); imports (`using Module`, `import Module: f, g` — symbol-list-qualified, genuinely precise when the `: f, g` form is used); macro invocations (`@time`, `@inline` — a distinct `@`-prefixed AST node, capture as a `Call`-shaped reference but ceiling-tag it since macros can rewrite the following expression, same shape as Elixir).
- **Visibility:** Julia has **no real access control**. A module's `export` list only advertises a *recommended* public surface — `Module.name` reaches anything regardless of export. Honest emission: `Unknown`, or `Public`-by-export-list-membership with an explicit caveat that it's a convention, not an enforced fact — never claim `Private`.
- **Namespace convention:** `module Name ... end`, nestable; file path is not authoritative (a file can declare any module name).
- **Gotchas / ceiling:** **multiple dispatch** — many method definitions can share one function name, disambiguated only by argument *types* (`f(x::Int)` vs `f(x::String)`), which is a strictly harder identity problem than Erlang/Elixir's arity-keying (arity is syntactically visible; types generally are not, without full inference). This caps call-resolution at Tier-A `NameOnly` fan-out for overloaded names — never promote to `Exact` without a real type checker. `eval`/`Meta.parse` string-to-code execution is the same `eval` ceiling as R/PowerShell.
- **Complexity:** M–L — multiple dispatch breaks the "one symbol per name in scope" assumption baked into most existing extractors' identity logic; visibility must be honestly fuzzy, not invented.
- **Matrix target:** TS-plus (Calls/Imports/Visibility=Unknown/some Type-ref), full `Exact` resolution structurally capped by multiple dispatch — an honest ceiling, not a gap.

### R (`.r`, `.R`) — grammar-compatible today

- **Definition kinds:** **R has no dedicated function/class declaration syntax at all** — everything is an assignment. `foo <- function(x) { ... }` (or `foo = function(x) {...}`) is the only way to "define" a function, meaning the extractor's definition rule is a heuristic: *"a top-level assignment whose RHS is a `function` node."* Class systems are library-level call patterns layered on top, not grammar constructs: S4 via `setClass("Name", ...)` calls, S3 via naming convention (`print.myclass <- function(x) ...`, unenforced), R6/Reference classes via `R6::R6Class(...)` calls. These require semantic call-shape matching (recognizing `setClass(...)`/`R6Class(...)` call patterns), not a dedicated AST node — closer to the `PY_ROUTE_VERBS` decorator-matching pattern already in `python.rs` than to a standard `class` keyword.
- **Reference kinds:** calls (`f(x)`, the ordinary call node); imports are also just **calls**: `library(pkg)` / `require(pkg)` — directly reuses the `require()`-as-`Import` pattern already implemented for Lua (`collect_require_imports`); namespace-qualified calls `pkg::fn(x)` / `pkg:::fn(x)` (`::`/`:::` operators) need qualifier capture as a distinct reference shape.
- **Visibility:** **no visibility keywords exist in R source at all.** A package's real public API is declared in a separate `NAMESPACE` file (`export(name)` directives) — a cross-artifact fact, structurally identical to OCaml's `.mli` and PowerShell's `.psd1` problem. Honest default: emit `Unknown` (or `Public`) for every top-level binding; do not infer privacy from naming convention (R has none, unlike Python's `_leading_underscore`).
- **Namespace convention:** none at the file level; package identity comes from the `DESCRIPTION`/`NAMESPACE` files, outside the source being parsed.
- **Gotchas / ceiling:** **non-standard evaluation (NSE)** is R's defining hazard — `subset()`, `dplyr::filter()`, formula objects (`y ~ x`), `substitute()`/`quote()`/`eval(parse(text=...))` let code be captured as unevaluated data and manipulated at runtime. This is genuinely undecidable statically; document as a hard ceiling, not a bug to fix. Multiple assignment operators (`<-`, `=`, `<<-`, `->`, `assign()`) — only `<-`/`=` with a `function` RHS should count as a definition; `assign("name", ...)` (string-based dynamic assignment) is unresolvable and out of scope.
- **Complexity:** L — the lowest "syntactic cleanliness" of the whole candidate set: definitions aren't a distinct AST shape, visibility is nearly unknowable from source alone, and NSE is a deep, well-known ceiling in every R static-analysis tool (that's not unique to this project).
- **Matrix target:** TS only realistically achievable without heavy heuristics (Calls + call-shaped Imports); `Inherit`/`Type-ref` are stretch (S3/S4/R6 call-pattern matching); `Visibility` is honestly `Unknown` across the board.

### Haskell (`.hs`) — BLOCKED on grammar compat today

- **Definition kinds:** top-level function bindings (`name args = expr`, possibly multiple pattern-matching equations — **same one-symbol-per-name-not-per-clause identity rule as Erlang**); type signatures (`name :: Type`, a separate declaration from the equations — attach as the symbol's one-line signature); `data`/`newtype` (algebraic data types; each constructor is arguably its own namespaced symbol); `class`/`instance` (typeclasses — `class` = interface definition, `instance ClassName TypeName where ...` = a direct, statically-visible `IsImplementation` edge, no type inference needed to capture the *declaration* itself, only to resolve *dispatch* at call sites); `module Name (exports) where`.
- **Reference kinds:** calls — Haskell's call syntax is **juxtaposition** (`f x y`, no parens/commas), a materially different `CALL_QUERY` shape than every currently-supported language; imports (`import Data.List (sort, nub)` — symbol-list-qualified and precise; `import qualified Data.Map as Map` — qualified-alias imports, need qualifier resolution for `Map.lookup`-style calls); type-refs (type signatures, data constructors).
- **Visibility:** the module's export list `module Foo (bar, Baz(..)) where` is an explicit whitelist — anything not listed is module-private. Clean and syntactically explicit (real table-stakes win), though wildcard forms (`Baz(..)` = export all constructors of type `Baz`) need care.
- **Namespace convention:** `module Name where`, typically one module per file, name usually matches the file path by convention (not grammar-enforced).
- **Gotchas / ceiling:** Template Haskell (`$(...)`, `[| ... |]`) is a genuine compile-time-metaprogramming ceiling, same class as Elixir's macros. Typeclass **instance declarations** are a strong `Inherit` signal (statically visible), but **instance selection at a call site** (which instance actually applies) requires type inference — capture the `instance` declaration as an edge; never resolve call-site dispatch beyond `NameOnly`/`Scoped`. Custom infix operators (`(+++)`) need identifier-shape handling distinct from alphanumeric names.
- **Complexity:** L — juxtaposition-call parsing and multi-clause identity are real, novel extractor design work (nothing in the current codebase has this call shape); typeclasses are a genuine `Inherit` differentiator once handled.
- **Matrix target:** Full for `Calls`/`Imports`/`Visibility`/`Inherit` (declaration-level); Template-Haskell-generated code and instance-dispatch resolution are documented ceilings.

### OCaml (`.ml`, `.mli`) — grammar-compatible today

- **Definition kinds:** `let name = ...` / `let rec name args = ...` (value/function bindings — same keyword serves both, distinguished by whether the RHS is `fun`/has parameters); `module Name = struct ... end` (nestable module definitions); `type name = ...` (variants/records); `class`/`object` (OCaml's separate, less-commonly-used object system).
- **Reference kinds:** calls — juxtaposition syntax (`f x y`), same novel `CALL_QUERY` shape as Haskell; module access (`open Module` — brings all names into unqualified scope, `from x import *`-like; `Module.value` — qualified access; `include Module` — re-exports a module's contents into the including module).
- **Visibility:** **the real distinguishing feature.** A companion `.mli` file, if present, is the authoritative public interface for the paired `.ml` — anything not listed in the `.mli` is inaccessible from other modules; **without an `.mli`, everything in the `.ml` is public.** This is a genuinely cross-file fact, not a per-declaration keyword — structurally identical to R's `NAMESPACE` and PowerShell's `.psd1` problem. Table-stakes/honest default: emit `Public` for all `.ml` symbols when parsed in isolation (matches this project's existing "no visibility signal available → don't guess" precedent). Full depth (stretch): correlate a same-directory `.mli` and downgrade non-listed symbols to `Internal` — real, scoped follow-up work, not part of "supported."
- **Namespace convention:** file path → module name by convention (capitalized), nestable via `module … = struct … end`.
- **Gotchas / ceiling:** PPX preprocessor extensions (`[@@deriving show]`, `let%lwt`) generate code invisible to a raw tree-sitter parse — OCaml's macro-equivalent ceiling. Functors (`module F (X : Sig) = struct ... end`, module-level parameterization/generics) are structurally significant but resolving what a functor *application* produces needs module-level type evaluation beyond syntax — capture the functor definition/application call, not its result.
- **Complexity:** M for table-stakes (Calls/Imports/juxtaposition-call parsing, `Public`-by-default visibility); L for full depth (cross-file `.mli` correlation, functors).
- **Matrix target:** TS achievable at M; full `Visibility` depth (`.mli` correlation) is an explicit stretch goal, separately scoped.

### Objective-C (`.m`, `.mm`) — grammar-compatible today

- **Definition kinds:** `@interface Name : Superclass <Protocol1, Protocol2> ... @end` (class interface — the **definition site for `Inherit` edges**: `: Superclass` = single inheritance, `<Protocols>` = protocol conformance, mapped the same way as `IsImplementation`); `@implementation Name ... @end` (method bodies); `@protocol Name ... @end` (protocol = interface definition, Java/Swift-like); methods `- (RetType)name:(Type)arg ... {}` (instance, `-`) / `+ (RetType)name ... {}` (class method, `+`) — Objective-C's **compound keyword-message selectors** (`doSomething:withValue:`) must be reassembled from multiple `:`-delimited pieces into one symbol name, a genuinely novel naming step. Since Objective-C is a strict C superset, plain C functions/structs/preprocessor macros are also valid — directly reuse `c.rs`'s patterns for that subset.
- **Reference kinds:** calls — **message sends** `[receiver selectorPart1:arg1 selectorPart2:arg2]` are a distinct `message_expression` grammar node, not a function-call node; the compound selector must be joined the same way the definition side is. Plain C calls (`foo(x)`) also occur for imported C functions. Imports: `#import "Foo.h"` / `#import <Framework/Foo.h>` — textual, C-`#include`-like, **no symbol-level import list** (same honest `Imports: —` ceiling C already has). Type-refs: `@property` declarations, ivar types, method parameter/return types.
- **Visibility:** no real access-control keywords (unlike Swift). Heuristic: methods/properties declared in `@interface` (importable via the header) → `Public`; methods only in `@implementation` (not declared in the interface or a class-extension `@interface Name ()`) → `Internal`-by-convention, **not compiler-enforced** — same honest-convention caveat as Ruby's runtime visibility.
- **Namespace convention:** flat, global class-name namespace (no modules) — identity is the class name plus the joined selector, not file-path-derived.
- **Gotchas / ceiling:** **categories** (`@interface Foo (CategoryName) ... @end`) extend an existing class from a *different* file with no inheritance edge — a real cross-file "reopens class" pattern (monkey-patching, Ruby-like); reasonable design: emit the category as its own `Symbol` (distinct namespace suffix) rather than trying to merge into the original class's symbol at extraction time — true merging is a resolver-level concern, not an extractor one. Runtime reflection (`respondsToSelector:`, `performSelector:`, KVO/KVC string-keyed property access) and `@dynamic` properties (implemented at runtime, no synthesized accessor) are hard dynamic-dispatch ceilings — emit nothing for computed-selector call sites.
- **Complexity:** M — the C subset is a known quantity (reuse `c.rs` patterns/helpers); the main new work is message-send parsing and compound-selector name assembly; categories and runtime dynamism are the ceiling.
- **Matrix target:** Full for `Inherit` (interface/protocol declarations are static); `Imports: —` like C; `Type-ref`/`Read-Write` achievable; dynamic dispatch capped honestly.

### Fortran (`.f90`, `.f`) — grammar-compatible today

- **Definition kinds:** `module Name ... end module` (namespace unit, Fortran 90+); `subroutine name(args) ... end subroutine` / `function name(args) result(r) ... end function` — **note the syntactic split**: subroutines are invoked with `call name(args)`, functions are invoked as expressions `x = name(args)` — genuinely different call-site shapes to detect; `type :: Name ... end type` (derived types = structs; `type, extends(Base) :: Derived` gives single-inheritance OOP in Fortran 2003+, a direct `Inherit` signal); `interface` blocks (abstract interface declarations, protocol-like).
- **Reference kinds:** calls — `call subroutine_name(args)` (subroutine invocation, distinct `call` keyword) vs. `result = function_name(args)` (function invocation, ordinary expression call); imports — `use ModuleName` (imports everything unqualified, `open`-like) or `use ModuleName, only: name1, name2` (the `only:` clause gives a **precise, symbol-level import list** — a genuine differentiator, comparable to Python's `from x import a, b`); type-refs (derived-type variable declarations).
- **Visibility:** modern (F90+) modules support `private`/`public` statements — either a blanket `private` with explicit `public :: name` exceptions, or the reverse; module-scoped, syntactically explicit (table-stakes win, similar shape to OCaml's `.mli` whitelist but declared *inside* the same file, so no cross-file correlation needed). **Legacy fixed-form Fortran (`.f`, F77-era, column-based) has no modules and no visibility concept at all** — treat as `Unknown`/`Public` uniformly for that dialect.
- **Namespace convention:** `module Name ... end module` for modern Fortran; legacy fixed-form has a single flat global namespace (COMMON blocks, subroutine names) with no module concept.
- **Gotchas / ceiling:** fixed-form (`.f`, column-position rules, continuation characters) vs. free-form (`.f90`+) are meaningfully different source dialects — verify the grammar handles both via a `to_sexp()` dump before writing the extractor, per CONTRIBUTING. **COMMON blocks** (legacy global storage shared across subroutines *by position*, not by declared name) have no name-based binding at all — Read/Write tracking on COMMON members is unreliable without cross-file layout agreement; document as a known skip, not silently-wrong data. Implicit typing (`implicit none` is opt-in; without it, undeclared identifiers starting `i`–`n` default to `integer`, others `real`) affects `Type-ref` confidence but not symbol/call extraction.
- **Complexity:** M — module-based Fortran (F90+) is as clean as Rust/Go; legacy fixed-form and COMMON blocks are the ceiling/gotcha, not the common case if the eval corpus targets modern Fortran.
- **Matrix target:** Full for modern (F90+) modules; legacy fixed-form caps at TS (no visibility, no Read/Write on COMMON).

### Groovy (`.groovy`, `.gradle`) — grammar-compatible today, but immature crate (v0.1.2)

- **Definition kinds:** `class Name extends Base implements Iface1, Iface2 { ... }` (Java-like — single inheritance + multiple interface implementation, direct `Inherit` mapping, portable straight from `java.rs`'s existing patterns); methods (`def foo(args) { ... }` dynamically-typed, or `RetType foo(args) { ... }` typed); Groovy uniquely allows a **whole file to be an executable script** with no enclosing class (looser than Kotlin's top-level functions); closures (`{ arg -> body }` as first-class values — central to Gradle's DSL usage).
- **Reference kinds:** calls (`obj.method(args)`, free `method(args)`); imports (`import pkg.Class`, `import static pkg.Class.member` — Java-like, straightforward); type-refs (`extends`/`implements`, field/parameter types); inherit (same as `Inherit` above).
- **Visibility:** Java-like modifiers (`public`/`private`/`protected`, package-private default) when present — clean. **But Groovy's default for an unmarked top-level method/class is `public`** (the *opposite* of Java's package-private default) — a subtle correctness trap worth a dedicated unit test, verify against the real grammar dump before assuming Java's rule transfers.
- **Namespace convention:** package declaration (`package com.foo`) when present, Java-like; scripts (common in `.gradle` files) have no package at all.
- **Gotchas / ceiling:** Groovy is dynamically-dispatched on top of the JVM — `invokeMethod`/`methodMissing`/`propertyMissing` metaprogramming hooks are Ruby's `method_missing` equivalent, a real dynamic-dispatch ceiling layered on top of normal overload resolution. **Gradle DSL trailing-closure sugar** (`android { buildTypes { release { ... } } }`) is syntactically just chained method calls with a trailing closure argument (`foo { ... }` ≡ `foo(Closure)`), not a special DSL construct — recognizing this is the *same kind* of call-shape-detection work as the existing `PY_ROUTE_VERBS` pattern, not a new grammar concept, but it does mean naive "closure = block, not a call" assumptions will under-count calls in `.gradle` files specifically. GStrings (`"${expr}"`) can embed arbitrary expressions in string literals — walk them if the grammar exposes interpolated expressions as real nodes (verify via dump).
- **Complexity:** M — Java-like backbone makes definitions/inherit cheap to port from `java.rs`; the Gradle-closure-as-call recognition and metaprogramming-hook ceiling are the real design cost; **grammar immaturity (v0.1.2, young crate) is a genuine schedule risk independent of the language's own complexity** — budget time for `to_sexp()` surprises.
- **Matrix target:** Full for `.groovy` source; `.gradle` files achieve `Calls` only with the trailing-closure recognition in place (otherwise significant recall loss on real Gradle scripts, which are Groovy's most common real-world use inside this ecosystem).

### PowerShell (`.ps1`, `.psm1`) — grammar-compatible today

- **Definition kinds:** `function Verb-Noun { param(...) ...}` — PowerShell has an **idiomatic `Approved-Verb-Noun` naming convention** (`Get-Process`, `Set-Item`), not grammar-enforced but worth noting since symbol names read as compound cmdlet-style identifiers; `param()` blocks declare parameters, optionally annotated with attributes (`[Parameter(Mandatory=$true)]`); classes via `class Name { ... }` (PowerShell 5+, C#-like, supports `[Inheritance]`).
- **Reference kinds:** calls — **two distinct call styles**: (1) command/cmdlet-style with **no parentheses**, space-separated, dash-prefixed named arguments (`Get-Process -Name foo`) — a materially different `CALL_QUERY` shape than every currently-supported language; (2) expression-style with parens for .NET interop (`[System.IO.File]::ReadAllText($path)`, `$obj.Method(args)`). The pipeline operator `|` chains commands (`Get-Process | Where-Object {...} | Stop-Process`) — structurally significant data-flow, but syntactically just an operator between command expressions, not a call itself. Imports: `Import-Module Name` (module import, call-shaped like R's `library()`); `. .\script.ps1` (dot-sourcing — textually includes another script's whole scope, C-`#include`-like, not a symbol import); `using module Name` / `using namespace System.Text` (PS5+ typed imports).
- **Visibility:** **no in-language `public`/`private` for functions** — every script-level function is invokable once dot-sourced/imported. The real "public surface" of a module lives in a companion `.psd1` manifest's `FunctionsToExport` list, or `Export-ModuleMember -Function Name` calls inside the `.psm1` — the same cross-artifact-file pattern as R's `NAMESPACE` and OCaml's `.mli`. Honest default: `Unknown`/`Public`-by-presence per file in isolation; manifest correlation is a stretch goal.
- **Namespace convention:** file-based (`.psm1` = script module); no nested namespace concept beyond module boundaries.
- **Gotchas / ceiling:** dynamic/reflective invocation is **pervasive and idiomatic**, more so than in most languages: `& $scriptblock`, `Invoke-Expression $stringOfCode`, `& (Get-Command $name)` — calling a command whose name is itself a runtime variable/string is ordinary PowerShell style, not an edge case; a hard ceiling, same class as R's `eval`. Splatting (`@paramsHashtable` spread into a call's arguments) obscures argument shape at the call site. **The entire language is case-insensitive** (commands, parameters, variables) — this has a real implementation implication beyond documentation: Tier-A name-based matching for this language specifically should normalize case, or it will under-match relative to the language's actual semantics.
- **Complexity:** M–L — the parenless cmdlet-call shape is genuinely new work (no existing extractor has this pattern); manifest-based visibility and pervasive dynamic invocation are real ceilings; case-insensitive identity is a small but easy-to-miss resolver-level detail.
- **Matrix target:** TS achievable (Calls in both syntactic forms + call-shaped Imports); `Visibility` honestly `Unknown` without manifest correlation; dynamic-invocation call sites correctly emit nothing.

### Astro (`.astro`) — grammar exists but unverified/immature; depends on the TS engine

- **Grammar status:** `tree-sitter-astro-next` 0.1.1 (crates.io) is version-compatible (`tree-sitter ^0.26.5`), but it is a **single-maintainer, one-release, independent project** (not the official `withastro` org) — treat as unverified until a `to_sexp()` dump confirms the node shapes CONTRIBUTING expects. This is a materially different risk profile than Svelte's grammar, which is a maintained, widely-used crate.
- **Definition kinds (once verified):** Astro's file model is a `---`-fenced "frontmatter" block containing plain TypeScript/JS (component imports, `const`/`let` bindings, prop destructuring) followed by an HTML-like template with `{expression}` interpolations and component tags. The frontmatter block is **structurally identical to the Svelte `<script>` pattern**: locate the inner source node, run the existing `extract_ecmascript` (already used by `svelte.rs`), then `shift_offsets` back into the host file.
- **Reference kinds:** frontmatter calls/imports/type-refs — all delegated to the TS engine, `⤴` in the matrix exactly like Svelte's row today. Template `{expr}` interpolations are additional embedded-JS spans (at minimum, Read references to frontmatter-bound variables) — reuse whatever pattern the Svelte extractor already applies to its own template mustache expressions as the template.
- **Gotchas / ceiling:** none beyond what TS/Svelte already have, **contingent on the host grammar actually working as advertised** — this is the one candidate where the risk is entirely in an unverified, young dependency rather than in the language's own semantics.
- **Complexity:** S–M if the grammar checks out (rides entirely on the proven Svelte precedent); effectively **blocked-pending-verification** if `to_sexp()` reveals the crate doesn't expose clean frontmatter/template boundaries — budget a short spike to verify before committing a full phase to it.
- **Matrix target:** Full via delegation (`⤴`), same shape as Svelte's row.

## TypeScript/JavaScript Entry-Points (Existing Gap, Not a New Language)

**Current state:** `Entry-pts` is blank for TS/JS despite ⭐/🟢 status on every other column — the extractor (`typescript.rs`, shared by JS via `extract_ecmascript`) has no entry-point detection at all, unlike Rust/Python/Go/Java which each have a working `entry_points_for_*` detector following the same "terminal-identifier marker-walk" shape (`rust.rs` on attribute idents, `java.rs` on annotation idents, `python.rs`'s `PY_ROUTE_VERBS` on decorator-call idents, `go.rs` on method-name convention).

**Unambiguous syntax available for TS/JS, in order of confidence:**

1. **Call-expression form** (Express, Fastify's simple API, Koa, Hono): `<any receiver>.<verb>(<string-literal-path>, ...)` where `verb ∈ {get, post, put, delete, patch, head, options, all}`. This is a `call_expression` whose `function:` is a `member_expression` with a `property_identifier` terminal matching the verb set — **exactly the same detection shape already implemented for Python's `PY_ROUTE_VERBS`** (a call-shaped decorator, terminal-name match, receiver-agnostic). Directly portable: reuse the constant-list-plus-terminal-match pattern, just against `call_expression`/`member_expression` instead of Python's decorator-call AST.
2. **Decorator form** (NestJS): `@Get(path)`, `@Post()`, `@Put()`, `@Delete()`, `@Patch()`, `@All()` on a class method, and `@Controller(prefix)` on a class. The TS/TSX grammar exposes a `decorator` node wrapping either a bare identifier or a `call_expression` — **the same shape `java.rs` already detects for Spring's `@GetMapping`/`@RestController` annotations**, just applied to TS decorator syntax instead of Java annotation syntax. Verb set here is PascalCase (`Get`, `Post`, `Controller`, …), matching JS/TS convention, distinct from the lowercase call-form set above.

Both markers require **zero type resolution** — pure terminal-identifier matching on an unambiguous syntactic shape, consistent with the project's stated detection rule ("unambiguous syntax only... the detector follows the same marker-walk pattern as FFI-export detection").

**Honest exclusions (do not attempt):**
- Fastify's object-literal route form (`fastify.route({ method: 'GET', url: '/x', handler })`) — the verb is a *string property value* inside an object literal, not a terminal call-name; a materially different (and lower-confidence) detection rule than the two above. Treat as an explicit stretch item, not table stakes.
- `.use()` middleware registration — deliberately excluded; too generic (matches every Express app's logging/auth/static-file middleware, not specifically HTTP routes), would produce a flood of false-positive `HttpRoute` markers.
- Computed/dynamic route registration (`router[method](path, handler)`, route tables built from config/loops) — same "static syntax only" ceiling every existing detector already documents.

**Complexity:** LOW–MEDIUM — this is the single cheapest, highest-value item in the whole milestone: it reuses two already-proven, in-repo patterns (Python's call-terminal match + Java's decorator/annotation match) rather than inventing anything new, and because JS shares the TS engine (`extract_ecmascript`), **implementing it once closes the gap for both TS and JS simultaneously**.

## Feature Dependencies

```
[Grammar-compat verification] ──gates──> [Any new-language extractor work]
    (Elixir/Erlang/Gleam/Haskell currently fail this gate; recheck before scheduling)

[Astro extractor] ──requires──> [tree-sitter-astro-next grammar verified via to_sexp()]
                  ──requires──> [existing TypeScript engine (extract_ecmascript)]
                  ──follows pattern of──> [Svelte's embedded-SFC extractor]

[Scopes + bindings emission] ──enables──> [Tier-B eligibility (🟢/⭐ status)]
    (must ship in the SAME phase as the base extractor, per PROJECT.md's
     "bindings parity is a requirement of each language phase, not a trailing phase")

[TS/JS entry-point detection] ──reuses──> [Python's PY_ROUTE_VERBS call-terminal pattern]
                              ──reuses──> [Java's annotation-terminal pattern]
                              ──shared by──> [both TS and JS, via extract_ecmascript]

[Objective-C extractor] ──reuses concepts from──> [c.rs] (plain-C subset: functions, structs, preprocessor)
    but does NOT share a family-extractor function (unlike Lua/Luau) — message-send
    parsing and @interface/@implementation/category handling are Objective-C-specific

[Lua family sharing precedent] ──does NOT extend to──> [Erlang/Elixir/Gleam]
    (Lua/Luau share one grammar lineage and one `extract_lua_family` function;
     Erlang/Elixir/Gleam are three unrelated tree-sitter grammars that merely
     target the same BEAM VM — no syntactic kinship, so no shared-extractor payoff)

[Cross-artifact visibility correlation] (OCaml .mli, R NAMESPACE, PowerShell .psd1)
    ──enhances──> [per-file honest Public/Unknown default]
    ──conflicts with──> ["one file in, one FileFacts out" extractor shape]
    (needs directory-level file pairing — a materially different architecture
     from every extractor today; treat as a separately-scoped stretch effort,
     not bundled into "add language X")
```

### Dependency Notes

- **Grammar-compat gates everything:** four of thirteen candidates fail today; don't sequence them into a committed phase, keep them as a recurring recheck.
- **Astro requires the TS engine AND its own (unverified) host grammar:** the milestone's framing ("Astro via the embedded-SFC pattern") is correct about the *TS* half, but the *host* half (locating frontmatter boundaries) rides on a young, single-maintainer crate that needs its own verification spike before the Svelte-pattern reuse can even start.
- **Family sharing is grammar-lineage-specific, not VM-target-specific:** Lua/Luau share because their grammars are related; BEAM languages (Erlang/Elixir/Gleam) don't share despite a common runtime, because their grammars are unrelated. Objective-C is a middle case — shares C *concepts/helpers*, not a shared extractor function.
- **Scopes+bindings must ship with the base extractor**, not as a later PR, per the project's own key decision — this affects phase sizing (every new-language phase is "def+refs+scopes+bindings," not "def+refs" followed by a separate "add Tier-B" phase).

## MVP Definition

### Launch With (v1 of this milestone — grammar-compatible today, verified 2026-07-05)

- [ ] **Zig** — clean `pub`/`@import`, moderate comptime ceiling; S–M
- [ ] **Julia** — clean-ish but multiple dispatch caps `Exact` resolution; M–L
- [ ] **R** — table-stakes only realistically (no def-keyword, no in-source visibility); L
- [ ] **OCaml** — table-stakes at M; `.mli` correlation deferred
- [ ] **Objective-C** — reuses `c.rs` concepts for the C subset; message-send parsing is the real new work; M
- [ ] **Fortran** — clean for modern (F90+) modules; legacy fixed-form is a known-lesser-depth case; M
- [ ] **Groovy** — Java-like backbone, but grammar immaturity (v0.1.2) is a schedule risk; M
- [ ] **PowerShell** — novel parenless call shape, manifest-based visibility deferred, dynamic-invocation ceiling; M–L
- [ ] **TS/JS entry-point detection** — closes an existing depth gap on two ⭐/🟢 languages at once, reuses two proven in-repo patterns; LOW–MEDIUM, highest ratio of value to effort in this milestone

### Add After Validation (v1.x — re-verify grammar compat first)

- [ ] Elixir — re-check `tree-sitter-elixir`'s `tree-sitter` req; add once a `>=0.24,<0.27`-compatible release ships
- [ ] Erlang — same trigger
- [ ] Gleam — same trigger (likely the cleanest BEAM candidate once unblocked — no macros, explicit `pub`)
- [ ] Haskell — same trigger; juxtaposition-call parsing is shared novel work with OCaml, worth doing them together once both are viable
- [ ] Astro — trigger: `to_sexp()` spike confirms `tree-sitter-astro-next` exposes usable frontmatter/template boundaries

### Future Consideration (v2+ — explicit stretch, separately scoped)

- [ ] Cross-artifact visibility correlation (OCaml `.mli`, R `NAMESPACE`, PowerShell `.psd1`) — architecturally different from "add a language," needs its own design
- [ ] Fastify object-literal route-form detection for TS entry-points — lower-confidence detection shape, defer until the call/decorator forms are shipped and proven
- [ ] R's S3/S4/R6 class-pattern matching (`setClass`, `R6::R6Class` call-shape recognition) — real value, but heuristic and separable from R's table-stakes launch

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|----------------------|----------|
| TS/JS entry-point detection | HIGH (closes gap on 2 ⭐/🟢 languages) | LOW | P1 |
| Zig extractor | MEDIUM | LOW–MEDIUM | P1 |
| Objective-C extractor | MEDIUM–HIGH (pairs with existing Swift row) | MEDIUM | P1 |
| Fortran extractor (modern modules) | MEDIUM | MEDIUM | P1 |
| Julia extractor | MEDIUM | MEDIUM–HIGH | P2 |
| OCaml extractor (table-stakes) | MEDIUM | MEDIUM | P2 |
| Groovy extractor | MEDIUM | MEDIUM (+ grammar-maturity risk) | P2 |
| PowerShell extractor | MEDIUM | MEDIUM–HIGH | P2 |
| R extractor | LOW–MEDIUM (high ceiling, low achievable depth) | HIGH | P3 |
| Astro extractor | LOW–MEDIUM (niche framework, unverified grammar) | S–M if grammar checks out, else blocked | P3 (spike first) |
| Elixir/Erlang/Gleam/Haskell | MEDIUM–HIGH each | N/A — blocked | Recheck, not scheduled |
| Cross-artifact visibility correlation | LOW–MEDIUM | HIGH (architecture change) | P3 |

**Priority key:** P1 = grammar-verified, clear table-stakes path, do first. P2 = grammar-verified, real but manageable ceiling/complexity. P3 = either blocked, unverified, or architecturally out-of-shape for a quick win.

## Sources

- `docs/supported-languages.md` (capability matrix semantics, existing 🟠/🔴 status) — repo, HIGH confidence
- `CONTRIBUTING.md` §"Adding a Language", §"When a Language Has No Usable Grammar" — repo, HIGH confidence
- `src/extract/python.rs`, `src/extract/lua.rs`, `src/extract/typescript.rs` — repo source, skimmed for extraction-depth precedent (call/import/entry-point patterns), HIGH confidence
- crates.io registry, direct API queries against `tree-sitter-elixir`, `tree-sitter-erlang`, `tree-sitter-gleam`, `tree-sitter-zig`, `tree-sitter-julia`, `tree-sitter-r`, `tree-sitter-haskell`, `tree-sitter-ocaml`, `tree-sitter-objc`, `tree-sitter-fortran`, `tree-sitter-groovy`, `tree-sitter-powershell`, `tree-sitter-astro-next` and their published dependency manifests (2026-07-05) — HIGH confidence, primary source
- Training-data knowledge of Elixir/Erlang/Gleam/Zig/Julia/R/Haskell/OCaml/Objective-C/Fortran/Groovy/PowerShell/Astro language semantics (definition/visibility/module conventions, macro and dynamic-eval behavior) — MEDIUM confidence; recommend a `to_sexp()` AST dump per CONTRIBUTING before each extractor is written, since published grammars frequently diverge from expectation

---
*Feature research for: code2graph language-expansion milestone*
*Researched: 2026-07-05*
