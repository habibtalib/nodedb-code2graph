---
phase: 01-foundation-compatibility-gate-ci-hardening-ts-js-depth
plan: 02
subsystem: grammar
tags: [tree-sitter, cargo-features, abi-compatibility, rust, docs]

# Dependency graph
requires:
  - phase: 01-01
    provides: 11 grammar-only Cargo features + the _extractors import fix and luau isolation fix that unblocked standalone feature compilation
provides:
  - 4 more grammar-only Cargo features (elixir, erlang, gleam, haskell), none in default, each empirically ABI-verified
  - Empirical resolution of the STACK.md-vs-FEATURES.md dispute over Elixir/Erlang/Gleam/Haskell (D-03) — all 4 pass
  - Complete 15-row .planning/phases/01-foundation-compatibility-gate-ci-hardening-ts-js-depth/01-COMPAT-VERDICTS.md covering every COMPAT-01 candidate
  - docs/supported-languages.md corrections: F# unblocked 🔴→🟠, precise blocked-reason notes for Vue/Salesforce Apex/Liquid/COBOL, confirmed-pass notes for the disputed 4
affects: [phase-3-language-extractors, phase-4-beam-haskell-family]

# Tech tracking
tech-stack:
  added:
    - tree-sitter-elixir 0.3.5
    - tree-sitter-erlang 0.19.0
    - tree-sitter-gleam 1.0.0
    - tree-sitter-haskell 0.23.1
  patterns:
    - Grammar-only feature flags (no "_extractors") for disputed candidates, matching 01-01's pattern
    - Empirical ABI verification is the sole tie-breaker for STACK.md-vs-FEATURES.md research conflicts — never resolved by re-reading either doc

key-files:
  created:
    - .planning/phases/01-foundation-compatibility-gate-ci-hardening-ts-js-depth/01-COMPAT-VERDICTS.md
  modified:
    - Cargo.toml
    - src/grammar.rs
    - docs/supported-languages.md

key-decisions:
  - "Resolved D-03 empirically: all 4 disputed candidates (Elixir, Erlang, Gleam, Haskell) pass both cargo test grammar::tests::abi_versions_are_compatible and cargo check --no-default-features --features <lang> — the real gate is each crate's normal tree-sitter-language ^0.1 dependency, not the irrelevant tree-sitter ^0.23 dev-dependency FEATURES.md flagged. STACK.md's reading was correct."
  - "F# moved from 🔴-blocked to 🟠-planned in docs/supported-languages.md, reflecting its 01-01 ABI pass; extractor work deferred to Phase 4 (LANG-11)"
  - "Vue/Salesforce Apex/Liquid/COBOL keep their 🔴-blocked status but now carry the precise, already-researched blocked reason (old tree-sitter ~0.20 normal dependency for Vue/Apex, no crate exists for Liquid, non-functional zero-dependency crate for COBOL) instead of a vague 'verify' placeholder"

requirements-completed: [COMPAT-01, COMPAT-02]

# Metrics
duration: 22min
completed: 2026-07-05
---

# Phase 01 Plan 02: Disputed Candidates + Full Compatibility Verdicts Summary

**Elixir/Erlang/Gleam/Haskell all empirically pass the real ABI gate, resolving the STACK.md-vs-FEATURES.md dispute in STACK.md's favor; all 15 COMPAT-01 candidates now have a recorded PASS verdict in `01-COMPAT-VERDICTS.md`, and `docs/supported-languages.md` carries F#'s unblock plus precise blocked-reason notes for Vue/Apex/Liquid/COBOL.**

## Performance

- **Duration:** 22 min
- **Started:** 2026-07-05T10:14:49Z (session start, includes context loading)
- **Completed:** 2026-07-05T10:36:48Z
- **Tasks:** 2
- **Files modified:** 3 (Cargo.toml, src/grammar.rs, docs/supported-languages.md); 1 created (01-COMPAT-VERDICTS.md)

## Accomplishments
- Wired all 4 disputed candidates (Elixir, Erlang, Gleam, Haskell) as grammar-only Cargo features (no `_extractors`), each independently verified against `cargo test grammar::tests::abi_versions_are_compatible --features <lang>` and `cargo check --no-default-features --features <lang>` — zero failures, zero reverts, the contingency path was never invoked
- Resolved D-03 (the genuine STACK.md-vs-FEATURES.md research conflict) by running the actual test rather than re-reading either doc: the real gate is each crate's normal `tree-sitter-language = "^0.1"` dependency (the type `src/grammar.rs`'s `.into()` conversion consumes), not the irrelevant `tree-sitter = "^0.23"` dev-dependency FEATURES.md flagged
- Wrote `01-COMPAT-VERDICTS.md`: a complete 15-row artifact covering every COMPAT-01 candidate (the 11 from 01-01 + these 4), with crate, version, `tree-sitter-language` requirement, ABI result, and license per row — all 15 rows are PASS
- Corrected `docs/supported-languages.md`: F# moved out of its 🔴-blocked row into 🟠-planned (confirmed ABI pass, extractor deferred to Phase 4/LANG-11); Elixir/Erlang/Gleam/Haskell notes tightened with confirmed-pass detail while staying 🟠 (D-05: extractor phase flips them, not this gate); Vue/Salesforce Apex/Liquid/COBOL each now carry the precise, already-researched blocked reason instead of a vague "verify" placeholder
- No regression: `cargo test --workspace` (including the `supported_languages_doc_lists_each_primary_extension` sync test) and `cargo build --all-features` both exit 0; `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` both clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire and empirically gate the 4 disputed candidates** - `adf5769` (feat)
2. **Task 2: Write 01-COMPAT-VERDICTS.md and correct docs/supported-languages.md** - `4153b02` (docs)

**Plan metadata:** (this commit, pending) `docs(01-02): complete disputed candidates + compat verdicts plan`

## Files Created/Modified
- `Cargo.toml` - 4 new optional `tree-sitter-<lang>` dependencies (elixir, erlang, gleam, haskell) + 4 new grammar-only feature flags, none added to `default`
- `src/grammar.rs` - 4 new `pub fn <lang>() -> Language` accessors + 4 new ABI-test `check(...)` arms
- `docs/supported-languages.md` - F# row moved 🔴→🟠 with confirmed-pass note; Elixir/Erlang/Gleam/Haskell notes tightened; Vue/Salesforce Apex/Liquid/COBOL notes replaced with precise blocked reasons
- `.planning/phases/01-foundation-compatibility-gate-ci-hardening-ts-js-depth/01-COMPAT-VERDICTS.md` (new) - all 15 COMPAT-01 candidates' empirical verdicts

## Decisions Made
- The STACK.md-vs-FEATURES.md dispute (D-03) is resolved empirically: all 4 disputed candidates PASS. FEATURES.md's precondition table conflated a dev-only `tree-sitter ^0.23` version pin (irrelevant to linking) with the real ABI gate; STACK.md's reading based on the normal `tree-sitter-language ^0.1` dependency was correct.
- F# is reclassified 🔴→🟠 in the docs (per D-06), reflecting its already-confirmed 01-01 pass rather than leaving a stale "blocked" status.
- Vue and Salesforce Apex share the same root blocked-reason (`tree-sitter ~0.20` pinned as a normal dependency, unmaintained) — both notes were written to reflect that shared cause explicitly rather than leaving generic "verify" text.

## Deviations from Plan

None - plan executed exactly as written. All 4 disputed candidates passed both gates on the first attempt; the contingency path (revert + record failure) was never triggered.

## Issues Encountered
None. One correction made during self-review of Task 2: an initial edit to the Vue/Liquid/F#/Apex/COBOL block removed the F# row entirely instead of relocating its now-🟠 status — caught before committing and fixed by inserting the F# row into its correct 🟠-planned position (after Astro, before Vue) with the corrected Notes text. No commit was made with the row missing.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- All 15 COMPAT-01 candidates now have a real, empirically-run verdict (11 from 01-01 + 4 from this plan) — `01-COMPAT-VERDICTS.md` is the authoritative artifact for any future phase scoping extractor work off these candidates.
- Phase 4's scope for the BEAM/Haskell family (LANG-12) is now unblocked: Elixir, Erlang, Gleam, and Haskell are all confirmed grammar-compatible, resolving the STACK.md/FEATURES.md conflict flagged in STATE.md's blockers section.
- `docs/supported-languages.md` is fully corrected per D-05/D-06 — no further docs cleanup pending from COMPAT-01/02.
- Ready for 01-03 (CI hardening job): the grammar-only feature set is now stable at 26 candidate languages (22 default + 4 more disputed passing) plus the 11 from 01-01, all individually isolation-tested.

---
*Phase: 01-foundation-compatibility-gate-ci-hardening-ts-js-depth*
*Completed: 2026-07-05*

## Self-Check: PASSED

- FOUND: Cargo.toml
- FOUND: src/grammar.rs
- FOUND: docs/supported-languages.md
- FOUND: .planning/phases/01-foundation-compatibility-gate-ci-hardening-ts-js-depth/01-COMPAT-VERDICTS.md
- FOUND: adf5769 (Task 1 commit)
- FOUND: 4153b02 (Task 2 commit)
