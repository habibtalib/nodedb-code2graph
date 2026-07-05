---
phase: 01-foundation-compatibility-gate-ci-hardening-ts-js-depth
plan: 01
subsystem: grammar
tags: [tree-sitter, cargo-features, abi-compatibility, rust]

# Dependency graph
requires: []
provides:
  - 11 new grammar-only Cargo features (zig, julia, r, ocaml, objc, fortran, groovy, powershell, systemverilog, astro, fsharp), none in default
  - 11 new src/grammar.rs accessor fns + ABI-test arms, each empirically verified via cargo test/check (not inferred from crates.io semver)
  - Prep fix: unconditional `use tree_sitter::Language;` import (unblocks every grammar-only feature)
  - Prep fix: `luau` feature now depends on `lua` (fixes pre-existing isolation bug)
affects: [01-02-disputed-candidates, 01-03-ci-hardening, phase-2-language-extractors]

# Tech tracking
tech-stack:
  added:
    - tree-sitter-zig 1.1.2
    - tree-sitter-julia 0.23.1
    - tree-sitter-r 1.3.0
    - tree-sitter-ocaml 0.25.0
    - tree-sitter-objc 3.0.2
    - tree-sitter-fortran 0.6.0
    - tree-sitter-groovy 0.1.2
    - tree-sitter-powershell 0.26.4
    - tree-sitter-systemverilog 0.3.1
    - tree-sitter-astro-next 0.1.1
    - tree-sitter-fsharp 0.3.1
  patterns:
    - Grammar-only feature flags (no "_extractors") for candidates gated before extractor work is planned
    - Empirical ABI verification via cargo test + cargo check --no-default-features, never trusted from declared semver alone

key-files:
  created: []
  modified:
    - Cargo.toml
    - src/grammar.rs

key-decisions:
  - "Fixed src/grammar.rs's _extractors-gated Language import to be unconditional so grammar-only candidate features can compile standalone"
  - "Fixed pre-existing luau feature-isolation bug (luau now depends on lua) before COMPAT-03's CI job would have caught it red"
  - "OCaml wired to LANGUAGE_OCAML (base .ml grammar only); .mli variant deferred to Phase 4 (LANG-04)"
  - "F# wired to LANGUAGE_FSHARP (crate exports no plain LANGUAGE constant)"

patterns-established:
  - "Grammar-only gating: candidate languages get a Cargo feature + grammar.rs accessor + ABI-test arm, but no _extractors flag, until an extractor is actually planned"

requirements-completed: [COMPAT-01]

# Metrics
duration: 8min
completed: 2026-07-05
---

# Phase 01 Plan 01: Foundation Compatibility Gate (11 expected-compatible candidates) Summary

**All 11 expected-compatible candidate grammars (Zig, Julia, R, OCaml, Objective-C, Fortran, Groovy, PowerShell, SystemVerilog, Astro, F#) wired as grammar-only Cargo features and empirically passed both the ABI compatibility test and standalone feature isolation — zero failures, zero candidates needed reverting.**

## Performance

- **Duration:** 8 min
- **Started:** 2026-07-05T10:14:49Z
- **Completed:** 2026-07-05T10:22:03Z
- **Tasks:** 3
- **Files modified:** 2 (Cargo.toml, src/grammar.rs)

## Accomplishments
- Landed both required prep fixes first: the `_extractors`-gated `Language` import (was blocking every grammar-only feature from compiling standalone) and the pre-existing `luau` feature-isolation bug (`luau` now depends on `lua`)
- Wired all 11 expected-compatible candidates as grammar-only Cargo features (no `_extractors`), each independently verified against the real `abi_versions_are_compatible` test and a standalone `cargo check --no-default-features --features <lang>` isolation check — not inferred from crates.io-declared semver
- Zero candidates failed either gate; no reverts, no contingency invoked
- Confirmed no regression: `cargo test --workspace` (22-language default) and `cargo build --all-features` (33-feature coexistence) both exit 0
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` both clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Prep fixes — unconditional Language import + luau isolation bug** - `f863133` (fix)
2. **Task 2: Wire candidates batch A — Zig, Julia, R, OCaml, Objective-C, Fortran** - `94d9cf2` (feat)
3. **Task 3: Wire candidates batch B — Groovy, PowerShell, SystemVerilog, Astro, F#** - `7fcb8ff` (feat)

**Plan metadata:** (this commit, pending) `docs(01-01): complete foundation compatibility gate plan`

## Files Created/Modified
- `Cargo.toml` - 11 new optional `tree-sitter-<lang>` dependencies + 11 new grammar-only feature flags; `luau` feature fixed to depend on `lua`
- `src/grammar.rs` - unconditional `use tree_sitter::Language;` import; 11 new `pub fn <lang>() -> Language` accessors; 11 new ABI-test `check(...)` arms

## Decisions Made
- OCaml gates only the base `.ml` grammar (`LANGUAGE_OCAML`); the crate's `.mli`/`.mll` variants are out of scope for Phase 1 and deferred to Phase 4 (LANG-04) per the plan's interface note.
- F# gates `LANGUAGE_FSHARP` (the ionide crate exports no plain `LANGUAGE` constant, verified via docs.rs per the plan's interface note).
- Neither prep fix altered any existing extractor or behavior — both are additive/corrective to feature-gating plumbing only.

## Deviations from Plan

None - plan executed exactly as written. All 11 candidates passed both gates on the first attempt; the contingency path (revert + record failure for 01-02) was never triggered.

## Issues Encountered
None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Foundation laid for 01-02 (the 4 disputed candidates) and 01-03 (CI hardening job) — both prep fixes this plan required are now in place, so the future CI job will not immediately fail on `luau` isolation.
- All 11 expected-compatible candidates have a real recorded PASS verdict ready to feed into 01-02's consolidated `01-COMPAT-VERDICTS.md` artifact (full verdict recording for all 15 candidates happens after 01-02 wires the disputed 4).
- No blockers identified for 01-02 or 01-03.

---
*Phase: 01-foundation-compatibility-gate-ci-hardening-ts-js-depth*
*Completed: 2026-07-05*

## Self-Check: PASSED

- FOUND: Cargo.toml
- FOUND: src/grammar.rs
- FOUND: .planning/phases/01-foundation-compatibility-gate-ci-hardening-ts-js-depth/01-01-SUMMARY.md
- FOUND: f863133 (Task 1 commit)
- FOUND: 94d9cf2 (Task 2 commit)
- FOUND: 7fcb8ff (Task 3 commit)
