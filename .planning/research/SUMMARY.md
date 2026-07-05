# Project Research Summary

**Project:** code2graph — Language Expansion Milestone
**Domain:** Extending a tree-sitter-based, language-agnostic code-graph extraction library (Rust core + PyO3/napi-rs bindings) with new language extractors
**Researched:** 2026-07-05
**Confidence:** HIGH (grammar compatibility, architecture integration points, and repo-structural pitfalls are all directly verified against crates.io and the actual repo; language-semantic capability claims and template-mapping choices are MEDIUM — informed judgment pending the mandatory per-language `to_sexp()` AST dump)

## Executive Summary

code2graph is a mature, quality-gated Rust library that turns source files into symbols/references/edges through a strict, repeatable recipe: a single grammar chokepoint (`src/grammar.rs`), a compiler-enforced `Language` enum (`src/lang.rs`, no wildcard arms), one extractor module per language reusing shared `support.rs` helpers, and two generic pass-through bindings (Python/Node) that require zero per-language code — only a `Cargo.toml` feature-list edit. Fourteen candidate languages were evaluated for this expansion (Elixir, Erlang, Gleam, Zig, Julia, R, Haskell, OCaml, Objective-C, Fortran, Groovy, PowerShell, Astro, and newly-discovered F#). The recommended approach is to treat grammar compatibility as an empirical, per-language gate rather than a documentation assumption, add languages independently (not as shared "family" extractors, even where several target the same runtime, e.g. BEAM), and ship scopes/bindings/eval-corpus/bindings-parity together in the *same* phase as the base extractor rather than as trailing follow-up work — all of which are decisions already encoded in this project's own CONTRIBUTING.md and PROJECT.md.

The single most important finding of this research round is a genuine methodological conflict between two of the four research files on which languages are actually buildable today, and it must be resolved empirically, not assumed either way (see "Conflict to Reconcile" below). The second most important finding is that grammar version compatibility is never sufficiently established by a crate's declared `tree-sitter` semver — it must be confirmed by actually compiling the grammar and running this repo's own `abi_versions_are_compatible` test, because a crate can look compatible or incompatible by dependency-kind alone (`dev-dependency` vs. `normal` dependency) and only the real ABI number the generated parser carries settles it.

Key risks: (1) old-style grammars (Vue, Apex) that declare `tree-sitter` as a normal — not dev — dependency at pre-`tree-sitter-language` versions are genuinely, permanently incompatible and should stay out of scope; (2) several candidates (Haskell, OCaml) ship hand-written C/C++ scanners with a known history of Windows/macOS build fragility, which the 3-OS CI matrix will catch but the bindings (maturin/napi) job currently will not, since it only runs on `ubuntu-latest`; (3) the binding-side feature-list edit (`bindings/{node,python}/Cargo.toml`) is the one integration step with *no* automated guard — a missed edit compiles clean and silently returns `UnsupportedLanguage` at runtime, making it the highest-risk single step in the whole recipe; and (4) several candidates have genuinely novel call-syntax shapes (Haskell/OCaml juxtaposition calls, PowerShell's parenless cmdlet calls) or dispatch mechanisms (Julia/R's multiple-dispatch/S3 generics) with no existing template in the codebase, and should be flagged for deeper phase-specific research rather than assumed to fit an existing extractor pattern.

## Key Findings

### Recommended Stack

The repo pins `tree-sitter >=0.24, <0.27` (currently resolving to 0.26.9). The real compatibility gate for any candidate grammar crate is not its declared `tree-sitter` version but its `tree-sitter-language` dependency (the `LanguageFn` indirection that every modern grammar crate uses) and, ultimately, the runtime ABI version baked into its generated parser — checked by this repo's own `abi_versions_are_compatible` test. STACK.md's crates.io dependency-kind-aware analysis (checking whether `tree-sitter` is declared as `normal` vs. `dev`) found all 14 candidates compatible; FEATURES.md's simpler "declared `tree-sitter` requirement" read flagged four of them (Elixir, Erlang, Gleam, Haskell) as incompatible. See "Conflict to Reconcile" below for how this must be resolved.

**Core technologies:**
- `tree-sitter-language ^0.1` (LanguageFn) — the real, load-bearing compatibility surface between any grammar crate and this repo's host `tree-sitter` crate; unifies via Cargo's resolver regardless of what `tree-sitter` version a grammar's own dev-dependencies/tests reference.
- Per-language `tree-sitter-<lang>` grammar crates — 13 of 14 candidates verified with a `normal`-dependency-declared `tree-sitter-language`; only `tree-sitter-astro-next` carries elevated maturity risk (single release, ~5 months old, non-official maintainer).
- Existing `support.rs` shared helpers (`make_symbol`, `push_ref`, `collect_call_references`, `push_import_ref`, `push_type_ref`, scope/binding helpers) — mandatory reuse per CONTRIBUTING, not per-language reinvention.
- F# (`tree-sitter-fsharp 0.3.1`, `ionide` org) was newly discovered as unblocked (most recent update 2026-07-01) and should move from Out of Scope into the candidate set.

### Expected Features

A language isn't "supported" in this project's own bar until it emits real `Calls` + `Imports` — a symbols-only extractor produces an isolated node graph with no edges. Every new-language phase must budget `scopes`+`bindings` (Tier-B eligibility) in the same PR as the base extractor, not as a trailing phase, per an existing project key decision.

**Must have (table stakes) — every new language:**
- Symbols (defs) with correct `SymbolKind`, byte span, one-line signature
- Qualified identity (namespace from file path or module declaration)
- `Calls` reference role — the floor; without it there are no edges at all
- `Imports` reference role — the project's core cross-file differentiator
- Declared `Visibility` tag, with `Unknown` as a legitimate, honest answer where no in-language signal exists (R, Julia, PowerShell all lack reliable in-source visibility)

**Should have (competitive/differentiator):**
- `scopes`+`bindings` for Tier-B precision (`Scoped`/`Exact`, not just Tier-A name-fanout)
- `Inherit` for OOP/typeclass-shaped candidates (Objective-C, Haskell, Groovy, Fortran)
- TS/JS entry-point detection (`app.get`/`@Get()` markers) — an *existing* gap on two already-🟢/⭐ languages, reusable via two already-proven in-repo patterns (Python's `PY_ROUTE_VERBS`, Java's annotation-terminal match); highest value-to-effort ratio in the whole milestone.

**Defer (v2+ / explicit stretch, not part of "supported"):**
- Macro/metaprogramming expansion (Elixir `defmacro`/`use`, Haskell Template Haskell, OCaml PPX) — emit invocation site only, never expansion
- Non-standard evaluation (R `eval(parse(text=...))`, PowerShell `Invoke-Expression`) — genuinely undecidable statically
- Cross-artifact visibility correlation (OCaml `.mli`, R `NAMESPACE`, PowerShell `.psd1`) — needs directory-level file pairing, a materially different architecture from the current one-file-in/one-`FileFacts`-out shape
- Fastify object-literal route detection, R's S3/S4/R6 class-pattern matching

### Architecture Approach

Every new language touches the same fixed, compiler-enforced set of files: root `Cargo.toml` (feature + dependency), `src/grammar.rs` (grammar fn + ABI check arm), `src/lang.rs` (enum variant, `ALL`, extensions, `as_str` — all with no wildcard arms so a missed edit is a compile error), a new `src/extract/<lang>.rs` implementing the `Extractor` trait, `src/extract/mod.rs`/`dispatch.rs` wiring, a `docs/supported-languages.md` row (sync-tested), an `eval/corpus/<lang>/` case (auto-discovered), and both `bindings/{node,python}/Cargo.toml` feature lists (the one step with no automated guard). Resolution (`FileFacts` → `CodeGraph`) is language-agnostic and works for free once extraction emits the right facts.

**Major components:**
1. `Language` enum (`src/lang.rs`) — single source of truth for coverage, compiler-enforced completeness
2. Grammar chokepoint (`src/grammar.rs`) — sole importer of `tree_sitter_*` crates; runtime ABI compat gate
3. Extraction layer (`src/extract/`) — one module per language implementing a shared `Extractor` trait, built on mandatory shared helpers
4. Bindings (Python/Node) — generic enum-driven adapters with zero per-language code; integration is entirely a `Cargo.toml` feature-list concern

### Critical Pitfalls

1. **Grammar crate's declared `tree-sitter` version doesn't guarantee ABI compatibility** — never treat a Cargo.toml semver claim as sufficient; wire the grammar fn and run `abi_versions_are_compatible` before writing any extractor code, as the first commit of each language's phase.
2. **Writing the extractor against GitHub's `node-types.json` instead of the pinned crate's actual grammar** — always drop a throwaway `to_sexp()` AST dump against real-world (not toy) snippets of the exact pinned crate version before writing queries.
3. **Bundled C/C++ scanner code breaks the build on a subset of platforms** (Haskell, OCaml specifically flagged, known Windows/macOS history) — the 3-OS test matrix catches this, but the bindings (maturin/napi) job runs `ubuntu-latest` only and will not; extend or explicitly accept the gap for scanner-based candidates.
4. **Feature-flag combinatorics gap** — CI only ever runs `--all-features`; no job builds a single language in isolation, so an undeclared feature-dependency (e.g. Astro implicitly needing `typescript`) can silently ship uncaught. Raise as a cross-cutting infra fix at milestone start, not deferred per-language.
5. **Bindings parity is the one ungated step** — the `bindings/{node,python}/Cargo.toml` feature-list edit has no compiler or CI signal if missed; treat it as a mandatory same-PR checklist item, not an afterthought.

## Conflict to Reconcile: Elixir/Erlang/Gleam/Haskell Grammar Compatibility

STACK.md and FEATURES.md disagree on whether four candidates (Elixir, Erlang, Gleam, Haskell) are buildable today:

- **STACK.md** (dependency-kind-aware crates.io lookup — queried both the crate metadata endpoint *and* the per-version `dependencies` endpoint, distinguishing `normal` from `dev` dependencies): all 14 candidates are compatible. Its reasoning is that these grammar crates' `tree-sitter ^0.23` declaration is a **dev-dependency only** (used for the grammar's own test suite), while the real, load-bearing integration surface is `tree-sitter-language ^0.1` (a `normal` dependency), which unifies cleanly with this repo's resolved `tree-sitter 0.26.9` / `tree-sitter-language 0.1.7`.
- **FEATURES.md** (a simpler check of "what `tree-sitter` version does the crate declare"): reads the same `tree-sitter ^0.23` line without distinguishing dependency kind, and concludes these four are "currently incompatible."

**STACK.md's method is the more authoritative one** — it is dependency-kind-aware and matches how this repo's own `src/grammar.rs` actually links a grammar (via `LanguageFn.into()`, never a direct `tree-sitter::Language` from the grammar crate). FEATURES.md's simpler read is not wrong about what's *declared*, but conflates a test-only dev-dependency with the real linkage surface.

**Resolution mechanism (do not resolve this by re-reading either research file further):** Phase 0 must be an empirical feasibility spike, per the existing project convention — for each of Elixir, Erlang, Gleam, and Haskell, add the Cargo dependency, wire the one-line grammar fn in `src/grammar.rs`, add the `check("<lang>", super::<lang>())` arm, and run `cargo test grammar::tests::abi_versions_are_compatible --features <lang>`. That test result — not either research document — is the actual gate. Budget this as a throwaway, one-commit-per-language spike before scheduling any extractor work for these four; a failure here costs one commit, not a half-built extractor.

## Implications for Roadmap

Based on combined research, suggested phase structure:

### Phase 0: Grammar Feasibility Spike (all 14 candidates)
**Rationale:** Both STACK.md and PITFALLS.md agree that no candidate's buildability should be assumed from documentation; the STACK/FEATURES conflict on Elixir/Erlang/Gleam/Haskell makes this non-optional. This is also where the project's own Key Decision ("gate every candidate on verified grammar compat before planning it") is executed.
**Delivers:** A definitive compatible/incompatible verdict per language, verified via `abi_versions_are_compatible`, resolving the STACK/FEATURES conflict empirically.
**Addresses:** Grammar-compat precondition from FEATURES.md §0; STACK.md's verdict table.
**Avoids:** Pitfall 1 (ABI mismatch discovered mid-implementation) and Pitfall 3 (scanner build breakage) for Haskell/OCaml specifically — check scanner language and known issue history during this spike, not later.

### Phase 1: CI Hardening (cross-cutting infra)
**Rationale:** PITFALLS.md flags the feature-flag combinatorics gap (Pitfall 4) as a pre-existing issue that compounds with every new language added; fixing it before 8-13 new languages land is cheaper than after.
**Delivers:** A lightweight `cargo check --no-default-features --features <lang>` check (CI job or documented pre-merge step) for touched languages.
**Uses:** Existing CONTRIBUTING-documented isolation-build command, currently unenforced by CI.

### Phase 2: TS/JS Entry-Point Detection (existing-language depth gap)
**Rationale:** ARCHITECTURE.md and FEATURES.md both identify this as the single cheapest, highest-value item — it reuses two already-proven in-repo patterns (Python's `PY_ROUTE_VERBS`, Java's annotation match) and requires zero new grammar work. Doing it early builds momentum and validates review conventions before harder new-language work starts.
**Delivers:** `Entry-pts` populated for TS and JS simultaneously (shared `extract_ecmascript` engine).
**Implements:** Marker-walk pattern already used by Rust/Python/Go/Java extractors.

### Phase 3: Embedded/SFC Quick Win — Astro
**Rationale:** ARCHITECTURE.md's suggested build order places this first among genuinely new languages: it's the lowest-risk candidate (explicit Svelte-pattern reference implementation already named in CONTRIBUTING/PROJECT.md), and it validates the full recipe + bindings/CI mechanics end-to-end before harder families start. Contingent on a `to_sexp()` verification spike confirming `tree-sitter-astro-next` exposes usable frontmatter/template boundaries (single-maintainer, one-release crate — MEDIUM confidence).
**Delivers:** Astro extraction via the existing TS engine + `shift_offsets` pattern.
**Addresses:** FEATURES.md's Astro row (Full via delegation).
**Avoids:** Pitfall 2 (stale node-shape assumptions) — mandatory AST dump given this grammar's immaturity.

### Phase 4: Shell-Family Quick Win — PowerShell
**Rationale:** Near-1:1 template mapping to the existing Shell extractor; second quick win proving a different template-reuse path. Grammar verified compatible (STACK.md, `tree-sitter ^0.26.5`).
**Delivers:** PowerShell table-stakes extraction (Calls in both cmdlet and expression call forms, call-shaped Imports).
**Avoids:** Over-promising on `Visibility` (no in-language public/private — honest `Unknown` default) and dynamic-invocation ceilings (`Invoke-Expression`, `&$scriptblock`).

### Phase 5: JVM Family — Groovy
**Rationale:** Mature/stable JVM grammar family; Java/Kotlin templates already exist in-repo. Lowest grammar-risk of the remaining "new" languages, though the grammar crate itself is immature (v0.1.2).
**Delivers:** Groovy extraction for `.groovy` source at minimum; explicit scope decision required on `.gradle` build-script inclusion (Pitfall 6 — DSL/closure patterns violate typical class-based fixture assumptions).
**Avoids:** Pitfall 6 (Gradle DSL corpus variance) via an explicit in/out-of-scope call documented in the phase plan.

### Phase 6: Apple/C-Adjacent Native — Objective-C
**Rationale:** ARCHITECTURE.md places this after the bindings/CI pipeline has been proven twice (Phases 3-4) because it raises a genuine design question (`.h` extension collision, Pitfall 5) that benefits from the milestone's shape already being settled.
**Delivers:** Objective-C extraction reusing `c.rs` concepts for the C subset plus Swift-derived `@interface`/`@implementation`/protocol handling; message-send parsing is genuinely new work.
**Avoids:** Pitfall 5 — requires an explicit, documented decision on `.h` dispatch (leave mapped to C only; do not attempt content-sniffing) before extractor work starts.

### Phase 7: Systems/Procedural Pair — Zig + Fortran
**Rationale:** Both are independent, structurally similar (no inheritance, explicit `pub`/`public` visibility keyword, C-adjacent), and can be built in parallel by different contributors.
**Delivers:** Zig (`pub`/`@import` builtin-as-import, comptime-capped `Type-ref`) and Fortran (modern F90+ modules with explicit `public`/`private`; legacy fixed-form capped at table-stakes).

### Phase 8: BEAM Family — Elixir, Erlang, Gleam (pending Phase 0 verdict)
**Rationale:** Grouped for domain/narrative coherence (shared BEAM runtime) despite three structurally unrelated grammars — ARCHITECTURE.md explicitly warns against forcing a shared-extractor pattern here (Anti-Pattern 3), unlike Lua/Luau's genuine grammar-lineage sharing.
**Delivers:** Three fully independent extractors (Ruby-template for Elixir, Go+FFI-export-pattern for Erlang, Rust-template for Gleam), suggested internal order Elixir → Erlang → Gleam.
**Implements:** Only proceeds for whichever of these three pass the Phase 0 ABI gate — this phase is entirely conditional on that empirical result, not on either STACK.md's or FEATURES.md's documentary claim.

### Phase 9: Dynamic Scientific/Scripting Pair — Julia + R
**Rationale:** Grouped for a shared research risk: neither has an existing in-repo template for its dispatch mechanism (Julia's multiple dispatch, R's S3/S4/R6 generics) — flagged by ARCHITECTURE.md as the most likely source of a Tier-B design question, done after the team has already exercised "document an honest Tier-A-only ceiling" once with Erlang's arity-based clauses.
**Delivers:** Julia (module/function extraction, multiple-dispatch honestly capped at `NameOnly` fan-out) and R (assignment-based function definitions, no in-source visibility, NSE as a documented hard ceiling).

### Phase 10: Statically-Typed Functional/ML Family — Haskell + OCaml (pending Phase 0 verdict for Haskell)
**Rationale:** Deliberately last — highest design risk of the whole candidate set (juxtaposition-call parsing, typeclass/instance dispatch, `.mli`/`.ml` visibility split), most likely to need a project Discussion if extraction facts push on the fact-schema boundary. Also the pair most exposed to Pitfall 3 (C/C++ external scanner cross-platform fragility).
**Delivers:** Haskell (Scala-templated typeclass/instance handling) and OCaml (Rust-templated module handling, `.ml`-only visibility as table-stakes, `.mli` correlation as an explicit stretch goal).

### Phase Ordering Rationale

- Feasibility (Phase 0) and CI hardening (Phase 1) come first because every later phase's correctness depends on them, and PITFALLS.md identifies both as compounding risks the more languages are added afterward.
- The existing TS/JS gap (Phase 2) is sequenced early because it has zero grammar risk and reuses proven patterns — a fast, low-risk win before harder new-language work.
- New languages are ordered by template-mapping confidence and grammar/scanner risk (per ARCHITECTURE.md's suggested build order): quick wins with named reference implementations or near-1:1 templates first (Astro, PowerShell), then mature-ecosystem families (Groovy), then genuinely novel syntax/design-risk work last (Objective-C's `.h` collision, BEAM family's per-language independence, Julia/R's dispatch-mechanism gap, Haskell/OCaml's juxtaposition calls and scanner fragility).
- Every phase folds bindings parity (both `Cargo.toml` feature-list edits + napi regen/diff check) and eval-corpus creation into the same PR, per the project's existing Key Decision to avoid a trailing bindings phase and per PITFALLS.md's Anti-Pattern 2 warning.

### Research Flags

Needs deeper `/gsd:research-phase` research during planning:
- **Phase 0 (Elixir/Erlang/Gleam/Haskell sub-spikes):** Directly resolves the unresolved STACK/FEATURES conflict; outcome is binary per language and gates all downstream scheduling.
- **Phase 6 (Objective-C):** `.h` extension-collision decision has no existing precedent in this codebase and is a user-visible dispatch-behavior design choice, not a mechanical integration step.
- **Phase 8 (BEAM family):** Arity-based/atom-based identity questions (Erlang clauses, name+arity keying) have no existing resolver-side analog — flagged as a Tier-B research risk, not just an extraction mechanic.
- **Phase 9 (Julia/R):** Multiple dispatch and S3/S4/R6 generic dispatch have no existing template in any of the 24 current extractors — genuinely novel design space.
- **Phase 10 (Haskell/OCaml):** Juxtaposition-call parsing is a materially different `CALL_QUERY` shape than every currently-supported language; typeclass/instance dispatch and the scanner cross-platform risk (Pitfall 3) both need dedicated attention.

Phases with standard, well-documented patterns (research-phase can be skipped or lightweight):
- **Phase 2 (TS/JS entry-points):** Two already-proven in-repo patterns to port directly.
- **Phase 3 (Astro):** Named, explicit reference implementation (Svelte pattern) in both CONTRIBUTING.md and PROJECT.md — only the grammar-maturity spike is genuinely open.
- **Phase 4 (PowerShell):** Near-1:1 Shell-extractor template mapping.
- **Phase 5 (Groovy) / Phase 7 (Zig, Fortran):** Established Java/Kotlin and C/Rust templates respectively; main open item is scope decisions (`.gradle` inclusion), not extraction technique.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Every verdict is a direct crates.io API lookup (metadata + per-version dependencies endpoint), cross-checked against this repo's actual resolved `Cargo.lock` and docs.rs-verified ABI version constants — not training-data recollection. |
| Features | MEDIUM-HIGH | Grammar-compat claims independently verified; language-semantic capability claims (def shapes, visibility mechanisms, macro ceilings) are informed judgment from training knowledge, cross-checked against existing extractor conventions — explicitly flagged as needing a `to_sexp()` dump per language before extractor work starts. |
| Architecture | HIGH | All integration points (file list, compiler-enforced enum, bindings mechanics, CI jobs) directly verified against the actual repo source; only the template-mapping table (which existing extractor to model each new language on) is MEDIUM, pending each language's own AST verification. |
| Pitfalls | MEDIUM-HIGH | Repo-structural claims (sync tests, CI job scope, feature-flag gaps) are HIGH — directly verified against workflow files and source. Per-grammar ecosystem claims (specific GitHub issues on Haskell/OCaml scanner fragility, Groovy crate freshness) are MEDIUM/LOW and explicitly flagged for re-verification at each language's feasibility-gate moment. |

**Overall confidence:** HIGH on what's structurally true about this repo and its grammar-compatibility mechanics; MEDIUM on per-language semantic/capability specifics pending the mandatory verification spike each phase must run.

### Gaps to Address

- **The STACK/FEATURES conflict on Elixir/Erlang/Gleam/Haskell** (see "Conflict to Reconcile" above) — must be resolved by Phase 0's empirical `abi_versions_are_compatible` spike per language, not by further document analysis.
- **F# is newly unblocked** (`tree-sitter-fsharp 0.3.1`, `ionide` org, updated 2026-07-01) but wasn't part of the original 13-candidate architecture/pitfalls research passes — needs its own lightweight template-mapping and pitfalls pass before being scheduled into a phase (likely fits alongside OCaml given the shared .NET/ML-family syntax character, but this is not yet verified).
- **`tree-sitter-astro-next` maturity** — single release, ~5 months old, non-official maintainer; the `to_sexp()` verification spike is a hard precondition, not a formality, before committing scope to Phase 3.
- **Objective-C's `.h` extension-collision decision** — requires an explicit, documented product/design decision (not a technical one) that isn't resolvable from research alone; needs stakeholder input during phase planning.
- **Python binding-parity has no automated drift gate** (unlike Node's napi `git diff --exit-code` check) — every phase's Definition of Done must include a manual verification step until this asymmetry is closed as its own infra follow-up.
- **Groovy's `.gradle` in/out-of-scope decision** — real-world Gradle DSL corpus variance is substantial; needs an explicit scope call in that phase's plan, not an assumption that `.groovy` test cases generalize.

## Sources

### Primary (HIGH confidence)
- crates.io API (`GET /api/v1/crates/<name>` and `/api/v1/crates/<name>/<version>/dependencies`) — direct lookups for 19 grammar crates (13 planned candidates + Astro + 5 blocked re-checks), 2026-07-05.
- docs.rs `tree-sitter` crate pages (0.24.7, 0.26.10/0.26.9) — verified `LANGUAGE_VERSION`/`MIN_COMPATIBLE_LANGUAGE_VERSION` constants directly.
- This repo, read directly on 2026-07-05: `src/lang.rs`, `src/grammar.rs`, `src/extract/dispatch.rs`, `src/extract/mod.rs`, `src/extract/{lua,ruby}.rs`, root `Cargo.toml`, `bindings/{node,python}/Cargo.toml`, `bindings/{node,python}/src/lib.rs`, `.github/workflows/{ci,test}.yml`, `CONTRIBUTING.md`, `docs/supported-languages.md`, `.planning/PROJECT.md`, `.planning/codebase/{ARCHITECTURE,STRUCTURE,CONCERNS}.md`.

### Secondary (MEDIUM confidence)
- Training-data knowledge of Elixir/Erlang/Gleam/Zig/Julia/R/Haskell/OCaml/Objective-C/Fortran/Groovy/PowerShell/Astro language semantics (definition/visibility/module conventions, macro and dynamic-eval behavior) — cross-checked against existing extractor conventions in the repo, but not independently grammar-verified; each language phase must run its own `to_sexp()` dump before implementation.
- tree-sitter-haskell GitHub issues #34, #37 (macOS/Windows scanner build fragility) — illustrates a known problem class, not necessarily current/unresolved state.
- tree-sitter/tree-sitter GitHub issue #1246 (C++11 external-scanner build issue on macOS) — general ecosystem context, not language-specific confirmation.

### Tertiary (LOW confidence)
- tree-sitter-ocaml / tree-sitter-objc / tree-sitter-groovy crates.io pages (existence/description only, in the PITFALLS.md pass) — ABI/version compatibility for these must be re-verified via the `abi_versions_are_compatible` check at each language's feasibility-gate time, not assumed from that pass alone (STACK.md's later, more rigorous pass supersedes this for the compatibility question itself).

---
*Research completed: 2026-07-05*
*Ready for roadmap: yes*
