# Roadmap: code2graph — Language Expansion Milestone

## Overview

This milestone expands code2graph's supported-language set from 24 to as many of the 15 candidate languages as have a genuinely compatible grammar, while closing an existing depth gap in the TS/JS adapter. The journey starts with an empirical compatibility gate and CI hardening (nothing downstream can be trusted until every candidate's grammar is actually compiled and ABI-checked), picks up a zero-grammar-risk depth win (TS/JS entry-points) as a fast confidence-builder, then ships new language extractors in three risk-ordered waves: two near-1:1-template quick wins (Astro, PowerShell) that also prove the bindings-parity practice end-to-end, five extractors with solid existing templates (Zig, Objective-C, Fortran, Groovy, SystemVerilog), and finally the languages with genuinely novel call/dispatch shapes and no in-repo precedent (Julia, R, OCaml, F#, and the BEAM/Haskell family — the last gated on Phase 1's empirical verdict for whether they're buildable at all).

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Foundation — Compatibility Gate, CI Hardening & TS/JS Depth** - Empirically verify every candidate grammar, harden CI against feature-flag isolation breaks, and fill the existing TS/JS entry-point gap (completed 2026-07-05)
- [ ] **Phase 2: Quick-Win Extractors — Astro & PowerShell** - Ship the two lowest-risk new languages and prove the full recipe + bindings-parity practice end-to-end
- [ ] **Phase 3: Established-Template Extractors** - Ship Zig, Objective-C, Fortran, Groovy, and SystemVerilog, each mapped to a solid existing in-repo template
- [ ] **Phase 4: Risky & Novel-Design Extractors** - Ship Julia, R, OCaml, F#, and whichever of the BEAM/Haskell family passed Phase 1's empirical gate

## Phase Details

### Phase 1: Foundation — Compatibility Gate, CI Hardening & TS/JS Depth
**Goal**: Every candidate grammar has a definitive, empirically verified compatibility verdict; CI catches feature-flag isolation breaks before they compound across 12+ new languages; and the existing TS/JS extractor gains entry-point detection using two already-proven in-repo patterns. This phase is the gate every later phase depends on.
**Depends on**: Nothing (first phase)
**Requirements**: COMPAT-01, COMPAT-02, COMPAT-03, TSADAPT-01
**Success Criteria** (what must be TRUE):
  1. Every candidate grammar crate named in COMPAT-01 (Zig, Julia, R, OCaml, Objective-C, Fortran, Groovy, PowerShell, SystemVerilog, Astro, F#, Elixir, Erlang, Gleam, Haskell) has a recorded pass/fail verdict produced by actually wiring the grammar fn and running `cargo test grammar::tests::abi_versions_are_compatible --features <lang>` — not inferred from a crate's declared `tree-sitter` semver.
  2. Every candidate that fails the gate has an honest entry in `docs/supported-languages.md` stating the crate checked, its version, and the exact `tree-sitter`/`tree-sitter-language` requirement found, per CONTRIBUTING §"When a language has no usable grammar".
  3. CI runs `cargo check --no-default-features --features <lang>` for each newly-touched language feature, and an isolated-build break fails the pipeline (closing the pre-existing feature-flag combinatorics gap before 12+ languages land).
  4. Calling `extract()` on a `.ts` or `.js` file containing an Express/Fastify/Koa/Hono verb call (e.g. `app.get(...)`) or a NestJS `@Get`/`@Post`/`@Controller` decorator populates a non-empty `Entry-pts` value for the matching symbol.
  5. `cargo test` passes with default features and with each isolated candidate-language feature flag exercised in CI.
**Plans**: 4 plans
Plans:
- [x] 01-01-PLAN.md — Prep fixes (`_extractors` import gate, `luau` isolation) + wire and empirically gate the 11 expected-compatible candidates (Zig, Julia, R, OCaml, Objective-C, Fortran, Groovy, PowerShell, SystemVerilog, Astro, F#)
- [x] 01-02-PLAN.md — Wire and empirically gate the 4 disputed candidates (Elixir, Erlang, Gleam, Haskell); write `01-COMPAT-VERDICTS.md` for all 15; correct `docs/supported-languages.md` (F# unblock, Vue/Apex/Liquid/COBOL precise blocked-reason notes)
- [x] 01-03-PLAN.md — Add the `feature-isolation` CI matrix job to `.github/workflows/test.yml`, covering every language feature (existing 22 + whatever candidates from 01-01/01-02 landed)
- [x] 01-04-PLAN.md — TS/JS entry-point detection in the shared `extract_ecmascript` path (Express/Fastify/Koa/Hono verb-call matching + NestJS decorator matching, aggregated onto the class symbol)

### Phase 2: Quick-Win Extractors — Astro & PowerShell
**Goal**: Ship the two lowest-risk new language extractors — Astro (embedded-SFC pattern, reusing the TS engine) and PowerShell (near-1:1 Shell-extractor template) — end-to-end through grammar, extractor, tests, corpus, docs, and both bindings, establishing the bindings-parity practice that every later phase repeats.
**Depends on**: Phase 1
**Requirements**: LANG-08, LANG-10, BIND-01, BIND-02
**Success Criteria** (what must be TRUE):
  1. `extract()` on a `.ps1` file emits function symbols with real SCIP ids and `Calls` references for both cmdlet-style and expression-style call forms, with `Visibility` honestly `Unknown` (no in-language public/private signal) and `Invoke-Expression`/`&$scriptblock` documented as an unresolved dynamic-invocation ceiling.
  2. `extract()` on a `.astro` file parses the host document, runs the TS extractor on the `<script>`/frontmatter content, and emits symbols/references with byte offsets shifted back to the original file via `shift_offsets`.
  3. `cargo test` passes with `--no-default-features --features powershell` and `--no-default-features --features astro` independently.
  4. `bindings/node/Cargo.toml` and `bindings/python/Cargo.toml` both list the `powershell` and `astro` features in the same change that adds each language; `npx napi build --release --platform` produces a no-op diff against the committed `index.js`/`index.d.ts`.
  5. Both languages have at least one `eval/corpus/` case and a sync-test-guarded row in `docs/supported-languages.md`.
**Plans**: 2 plans
Plans:
- [ ] 02-01-PLAN.md — PowerShell extractor end-to-end (enum, extractor, tests, corpus, docs, bindings)
- [ ] 02-02-PLAN.md — Astro extractor end-to-end via embedded-SFC pattern (enum, extractor, tests, corpus, docs, bindings)

### Phase 3: Established-Template Extractors
**Goal**: Ship five new language extractors — Zig, Objective-C, Fortran, Groovy, SystemVerilog — each mapped to a solid existing in-repo template (C/Rust, C+Swift, Pascal/Go, Java/Kotlin, C respectively), with the explicit scope decisions each one raises (`.h` dispatch, `.gradle` inclusion) resolved and documented rather than assumed.
**Depends on**: Phase 1
**Requirements**: LANG-01, LANG-05, LANG-06, LANG-07, LANG-09
**Success Criteria** (what must be TRUE):
  1. `extract()` on a `.zig` file emits function/struct symbols with real SCIP ids, `Calls` references, and `@import`-derived `Imports`, with `comptime` constructs honestly capped at table-stakes.
  2. `extract()` on a `.m` file emits Objective-C symbols (message sends, `@interface`/`@implementation`/protocol declarations) distinct from the C extractor, with the `.h`-extension dispatch decision explicitly documented (mapped to C only, no content-sniffing) before extractor work started.
  3. `extract()` on a `.f90` file emits module/subroutine symbols with explicit `public`/`private` visibility; legacy fixed-form Fortran is honestly capped at table-stakes rather than fully modeled.
  4. `extract()` on a `.groovy` file emits symbols, `Calls`, and `Imports`, with an explicit documented decision on whether `.gradle` build-script files are in or out of scope for this milestone.
  5. `extract()` on a `.sv` file emits module/class symbols with `Calls` and `Imports` reusing the C-template approach.
  6. All five languages independently pass `cargo test --no-default-features --features <lang>`, have matching feature entries in both `bindings/{node,python}/Cargo.toml`, a diff-free `napi build`, at least one `eval/corpus/` case, and a `docs/supported-languages.md` row.
**Plans**: TBD

### Phase 4: Risky & Novel-Design Extractors
**Goal**: Ship the highest-design-risk extractors — Julia, R, OCaml, F#, and whichever of Elixir/Erlang/Gleam/Haskell passed Phase 1's empirical ABI gate — each carrying a genuinely novel call or dispatch shape (multiple dispatch, S3/S4 generics, juxtaposition calls, BEAM arity-based clauses) with no existing in-repo template, and each with an honestly documented ceiling where static extraction cannot go further.
**Depends on**: Phase 1 (the empirical Elixir/Erlang/Gleam/Haskell verdicts from Phase 1's spike determine which members of that sub-family are attempted here at all)
**Requirements**: LANG-02, LANG-03, LANG-04, LANG-11, LANG-12
**Success Criteria** (what must be TRUE):
  1. `extract()` on a `.jl` file emits function/module symbols with multiple-dispatch call resolution honestly capped at `NameOnly` fan-out — documented as a ceiling, never guessed.
  2. `extract()` on an `.R` file emits assignment-based function symbols with `Visibility` honestly `Unknown` (no reliable in-language signal) and `eval(parse(text=...))`-style non-standard evaluation documented as an unresolved ceiling rather than attempted.
  3. `extract()` on `.ml`/`.mli` files emits OCaml module symbols with `.ml`-only visibility as table stakes; `.mli` cross-file correlation is explicitly out of scope for this milestone.
  4. `extract()` on a `.fs` file emits F# symbols with real SCIP ids, reusing the OCaml/ML-family template validated in this same phase.
  5. For each of Elixir, Erlang, Gleam, and Haskell that passed Phase 1's `abi_versions_are_compatible` gate, `extract()` emits real symbols/`Calls`/`Imports` on that language's source extension; any that failed the gate remain documented as blocked (crate, version, `tree-sitter` requirement) rather than attempted here.
  6. All languages shipped in this phase independently pass `cargo test --no-default-features --features <lang>`, have matching feature entries in both `bindings/{node,python}/Cargo.toml`, a diff-free `napi build`, at least one `eval/corpus/` case, and a `docs/supported-languages.md` row.
**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Foundation — Compat Gate, CI Hardening & TS/JS Depth | 0/4 | Complete    | 2026-07-05 |
| 2. Quick-Win Extractors — Astro & PowerShell | 0/TBD | Not started | - |
| 3. Established-Template Extractors | 0/TBD | Not started | - |
| 4. Risky & Novel-Design Extractors | 0/TBD | Not started | - |
