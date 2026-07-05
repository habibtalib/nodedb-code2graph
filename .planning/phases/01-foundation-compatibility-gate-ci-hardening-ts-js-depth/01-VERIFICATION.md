---
phase: 01-foundation-compatibility-gate-ci-hardening-ts-js-depth
verified: 2026-07-05T00:00:00Z
status: passed
score: 5/5 must-haves verified
---

# Phase 1: Foundation — Compatibility Gate, CI Hardening & TS/JS Depth Verification Report

**Phase Goal:** Every candidate grammar has a definitive, empirically verified compatibility verdict; CI catches feature-flag isolation breaks before they compound across 12+ new languages; and the existing TS/JS extractor gains entry-point detection using two already-proven in-repo patterns.
**Verified:** 2026-07-05
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Every candidate grammar crate named in COMPAT-01 (all 15) has a recorded pass/fail verdict from actually running the ABI test, not inferred from semver | ✓ VERIFIED | `01-COMPAT-VERDICTS.md` has all 15 rows, all PASS; `src/grammar.rs` has all 15 `#[cfg(feature)] pub fn <lang>()` + matching `check(...)` arms; sample-ran `cargo test grammar::tests::abi_versions_are_compatible --features zig` and `--features haskell` — both `ok. 1 passed` |
| 2 | Every candidate that fails the gate has an honest `docs/supported-languages.md` entry per CONTRIBUTING protocol | ✓ VERIFIED (vacuously) | Zero candidates failed (all 15 PASS in `01-COMPAT-VERDICTS.md`), so no COMPAT-02 failure notes were required; Vue/Apex/Liquid/COBOL (pre-existing blocked languages, not COMPAT-01 candidates) carry precise blocked-reason notes per D-06 |
| 3 | CI runs `cargo check --no-default-features --features <lang>` for each newly-touched language feature; an isolated-build break fails the pipeline | ✓ VERIFIED | `.github/workflows/test.yml` has `feature-isolation` job, 37-entry matrix, `cargo check --no-default-features --features ${{ matrix.lang }}`; matrix set programmatically diffed against `Cargo.toml`'s `[features]` block (excl. default/manifest/serde/_extractors) — identical 37/37; sample-ran `cargo check --no-default-features --features typescript` and `--features zig` — both compile clean (only pre-existing dead-code warnings, no errors) |
| 4 | Calling `extract()` on `.ts`/`.js` with an Express/Fastify/Koa/Hono verb call or NestJS decorator populates a non-empty `Entry-pts` value | ✓ VERIFIED | `src/extract/typescript.rs` has `TS_ROUTE_VERBS`/`attach_call_entry_points` (call-terminal) and `TS_ROUTE_DECORATORS`/`route_decorators_on` (decorator-terminal), both wired into `collect_symbols`; `cargo test extract::typescript::tests` — 32/32 passed, including `ts_named_handler_router_get_entry_point`, `ts_named_handler_app_post_exported_entry_point`, `ts_controller_class_decorator_entry_point`, `ts_method_level_get_decorator_aggregates_onto_class`, `js_named_handler_router_get_entry_point`; `docs/supported-languages.md` TypeScript row's Entry-pts cell is `✓` |
| 5 | `cargo test` passes with default features and with each isolated candidate-language feature flag exercised in CI | ✓ VERIFIED | `cargo test --workspace` (default features, all workspace members): 768 lib + 11 eval + 10 regression + 2 doctests, all passed, 0 failed; `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; feature-isolation CI matrix (37/37 languages) verified structurally correct and two live samples (typescript, zig) compile clean |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `Cargo.toml` | 15 new grammar-only optional deps + feature flags, none in `default` | ✓ VERIFIED | All 15 (zig, julia, r, ocaml, objc, fortran, groovy, powershell, systemverilog, astro, fsharp, elixir, erlang, gleam, haskell) present as `dep:tree-sitter-<lang>`-only features; `default` list unchanged (still the original 22) |
| `src/grammar.rs` | 15 new accessor fns + ABI-test arms; unconditional `Language` import; `luau` isolation fix in `Cargo.toml` | ✓ VERIFIED | All 15 fns + `check(...)` arms present; `use tree_sitter::Language;` has no `_extractors` cfg gate; `luau = ["dep:tree-sitter-luau", "lua", "_extractors"]` in Cargo.toml |
| `01-COMPAT-VERDICTS.md` | One row per all 15 candidates: crate, version, tree-sitter-language req, ABI result, license | ✓ VERIFIED | 15 data rows, all PASS, matches D-04 format exactly |
| `docs/supported-languages.md` | F# unblocked 🔴→🟠; precise Vue/Apex/Liquid/COBOL blocked-reason notes; TypeScript Entry-pts filled | ✓ VERIFIED | F# row is 🟠 with "ABI-verified compatible" note; Vue/Apex both cite "tree-sitter ~0.20"/unmaintained; Liquid cites "no crate exists"; COBOL cites "zero dependencies"/"no repository"; TypeScript row Entry-pts = ✓ |
| `.github/workflows/test.yml` | New `feature-isolation` job, matrix covering every language feature | ✓ VERIFIED | Job present, 37-entry matrix, programmatically confirmed identical to Cargo.toml's feature set |
| `src/extract/typescript.rs` | TS_ROUTE_VERBS/attach_call_entry_points, TS_ROUTE_DECORATORS/route_decorators_on, wired into collect_symbols | ✓ VERIFIED | All present and wired; 9 new tests (32 total in module) all passing |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `Cargo.toml` per-lang feature (e.g. `zig = ["dep:tree-sitter-zig"]`) | `src/grammar.rs` `#[cfg(feature = "zig")] pub fn zig()` | matching feature name gate | ✓ WIRED | Verified for all 15 via grep; live-tested zig & haskell |
| `src/grammar.rs` `pub fn <lang>()` | `abi_versions_are_compatible` test | `check("<lang>", super::<lang>())` arm | ✓ WIRED | All 15 arms present; test passes per-feature |
| `feature-isolation` CI job matrix | `Cargo.toml [features]` | mechanically-matched entry list | ✓ WIRED | Programmatic diff: 0 discrepancies (37/37) |
| TS/JS call-terminal verb match | Symbol's `entry_points` | `attach_call_entry_points` walks AST, pushes `EntryPoint::HttpRoute` onto matched top-level Symbol | ✓ WIRED | `ts_named_handler_router_get_entry_point`/`ts_named_handler_app_post_exported_entry_point`/`js_named_handler_router_get_entry_point` tests confirm |
| NestJS decorator match | Class Symbol's `entry_points` | `route_decorators_on` scans decorator children, aggregates onto class Symbol via `collect_symbols` | ✓ WIRED | `ts_controller_class_decorator_entry_point`/`ts_method_level_get_decorator_aggregates_onto_class` tests confirm |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Zig ABI gate passes | `cargo test grammar::tests::abi_versions_are_compatible --features zig` | `1 passed; 0 failed` | ✓ PASS |
| Haskell ABI gate passes | `cargo test grammar::tests::abi_versions_are_compatible --features haskell` | `1 passed; 0 failed` | ✓ PASS |
| TypeScript feature isolates | `cargo check --no-default-features --features typescript` | compiles clean (warnings only) | ✓ PASS |
| Zig feature isolates | `cargo check --no-default-features --features zig` | compiles clean (warnings only) | ✓ PASS |
| TS extractor test suite | `cargo test extract::typescript::tests` | `32 passed; 0 failed` | ✓ PASS |
| Format check | `cargo fmt --all -- --check` | exit 0 | ✓ PASS |
| Clippy (all-features, all-targets) | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0, no warnings | ✓ PASS |
| Full workspace test suite | `cargo test --workspace` | 768+11+10+2 = all passed, 0 failed | ✓ PASS |
| CI matrix ⇔ Cargo.toml feature-set parity | programmatic diff (python set comparison) | 37/37 identical | ✓ PASS |
| Bindings untouched (Phase 1 scope boundary) | `grep zig\|julia\|haskell\|elixir bindings/*/Cargo.toml` | no matches | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| COMPAT-01 | 01-01, 01-02 | Every candidate grammar crate empirically gated | ✓ SATISFIED | 15/15 PASS in `01-COMPAT-VERDICTS.md`; live-verified 2 samples |
| COMPAT-02 | 01-02 | Failing candidates documented honestly | ✓ SATISFIED (vacuous) | Zero failures; documentation protocol correctly applied to pre-existing blocked languages (Vue/Apex/Liquid/COBOL) instead |
| COMPAT-03 | 01-03 | CI builds each new language standalone | ✓ SATISFIED | `feature-isolation` job, 37-entry matrix, matches Cargo.toml exactly |
| TSADAPT-01 | 01-04 | TS/JS entry-point detection via shared `extract_ecmascript` path | ✓ SATISFIED | `TS_ROUTE_VERBS`/`TS_ROUTE_DECORATORS` implemented and tested; docs row filled |

No orphaned requirements — REQUIREMENTS.md traceability table confirms all 4 Phase-1 requirements map only to Phase 1, and all 4 appear in the plans' `requirements` frontmatter.

### Anti-Patterns Found

None. Scanned `src/grammar.rs`, `src/extract/typescript.rs`, `src/resolve/symbol_table.rs`, and `.github/workflows/test.yml` for TODO/FIXME/placeholder/stub patterns — only match was a test fixture variable literally named `xxx` (not a stub indicator). No empty implementations, no hardcoded-empty stub returns, no console-log-only handlers.

### Human Verification Required

None. This phase's success criteria are entirely mechanical/empirical (grammar ABI checks, CI matrix structure, extractor unit tests, project-wide gates) and were all verified programmatically.

### Gaps Summary

No gaps found. All 5 roadmap success criteria hold under direct verification:

1. All 15 COMPAT-01 candidates have empirically-run (not semver-inferred) PASS verdicts, confirmed both in the `01-COMPAT-VERDICTS.md` artifact and by live-running the ABI test for two sampled candidates (zig, haskell).
2. COMPAT-02's failure-documentation requirement is vacuously satisfied (zero failures among the 15); the honest-failure-note protocol was correctly applied instead to the four pre-existing blocked languages (Vue, Apex, Liquid, COBOL) per D-06, plus F#'s unblock.
3. The `feature-isolation` CI job's 37-entry matrix is programmatically identical (0 discrepancies) to `Cargo.toml`'s `[features]` block, and two sampled features (typescript, zig) compile clean standalone.
4. TS/JS entry-point detection (`TS_ROUTE_VERBS` call-terminal matching + `TS_ROUTE_DECORATORS` decorator-terminal matching) is implemented, wired into `collect_symbols`, and its 9 new unit tests (32 total in the module) all pass; the docs Entry-pts cell for TypeScript is filled.
5. All project-wide gates are clean: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace` (768 lib + 11 eval + 10 regression + 2 doctests, 0 failures).

One minor documentation-consistency observation (not a gap): several of the 01-01-batch candidate rows in `docs/supported-languages.md` (e.g. PowerShell: "grammar exists — verify compat") retain their pre-verification placeholder wording rather than an "ABI-verified compatible" note, while F#/Elixir/Erlang/Gleam/Haskell got the updated wording. This is explicitly allowed by 01-02-PLAN.md's own acceptance criteria ("optional polish, not required") — not a deviation from what was planned.

---

*Verified: 2026-07-05*
*Verifier: Claude (gsd-verifier)*
