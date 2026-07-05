---
phase: 02-quick-win-extractors-astro-powershell
verified: 2026-07-05T00:00:00Z
status: passed
score: 5/5 success criteria verified (1 with a documented pre-existing non-blocking caveat)
---

# Phase 2: Quick-Win Extractors — Astro & PowerShell Verification Report

**Phase Goal:** Ship the two lowest-risk new language extractors — Astro (embedded-SFC pattern, reusing the TS engine) and PowerShell (near-1:1 Shell-extractor template) — end-to-end through grammar, extractor, tests, corpus, docs, and both bindings, establishing the bindings-parity practice that every later phase repeats.
**Verified:** 2026-07-05
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `extract()` on `.ps1` emits function symbols with real SCIP ids and `Calls` for both cmdlet-style and expression-style forms, `Visibility` honestly `Unknown`, `Invoke-Expression`/`&$scriptblock` documented as unresolved ceiling | ✓ VERIFIED | `src/extract/powershell.rs` — `CMDLET_CALL_QUERY` + `MEMBER_CALL_QUERY`, both exercised by `cmdlet_style_calls_both_pipeline_stages` and `member_style_calls_capture_qualifier` tests (pass). All symbols built with `Visibility::Unknown` (grep confirms no other visibility value used). Module doc-comment explicitly documents `Invoke-Expression`/`&$scriptBlock` as an unresolved ceiling; `docs/supported-languages.md` PowerShell row repeats this in its Notes column. |
| 2 | `extract()` on `.astro` parses host document, runs TS extractor on `<script>`/frontmatter, emits symbols/refs with byte offsets shifted back via `shift_offsets` | ✓ VERIFIED | `src/extract/astro.rs` `merge_block()` calls `extract_ecmascript` then `shift_offsets`. `offset_remap_is_correct` test asserts `run_sym.span.start` equals the byte offset of `function run` in the **full** `.astro` source (not the frontmatter-relative offset) — passes. |
| 3 | `cargo test` passes with `--no-default-features --features powershell` and `--features astro` independently | ⚠️ PARTIAL (documented, pre-existing, non-blocking) | `cargo check --no-default-features --features powershell` and `--features astro` both succeed standalone (production code has no cross-feature leakage). The full `cargo test` **test-binary build** fails under either isolated feature — but this is a pre-existing, project-wide gap in `src/resolve/symbol_table.rs`/`scope_graph.rs` (unconditional `RustExtractor`/`JavaExtractor`/etc. imports), independently reproduced with `--features lua` (an untouched, already-shipped language) with an identical 17×`E0432` error signature. Honestly logged in `deferred-items.md` and `STATE.md` Blockers section before this verification ran. CI's own `feature-isolation` matrix job (`.github/workflows/test.yml`) uses `cargo check`, not `cargo test`, for exactly this reason — so CI is not broken by this gap. `cargo test --all-features` passes 790/790 tests including all 12 new PowerShell and 5 new Astro tests. |
| 4 | Both bindings' `Cargo.toml` list `powershell`/`astro` in the same change; `napi build --release --platform` produces a no-op diff | ✓ VERIFIED | `bindings/node/Cargo.toml:14` and `bindings/python/Cargo.toml:14` both list `"powershell", "astro"`. Ran `npx napi build --release --platform` in `bindings/node` (node_modules present, no `npm ci` needed) then `git diff --exit-code bindings/node/index.js bindings/node/index.d.ts` → **no drift**. |
| 5 | Both languages have ≥1 `eval/corpus/` case and a sync-test-guarded `docs/supported-languages.md` row | ✓ VERIFIED | `eval/corpus/powershell/scoped_call/{deploy.ps1,expected.edges}` and `eval/corpus/astro/scoped_call/{Component.astro,expected.edges}` both exist with real same-file `Call` edges. Both rows present in `docs/supported-languages.md` (🟢, correct capability columns). `src/lang.rs::supported_languages_doc_lists_each_primary_extension` (the sync test) passes as part of the 790-test run. |

**Score:** 5/5 criteria substantively verified; criterion 3's literal wording ("passes... independently") is not met for `cargo test` (only for `cargo check`), but this is a pre-existing, honestly-documented, project-wide limitation unrelated to and not introduced by this phase's changes, and does not block CI or functionality.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/extract/powershell.rs` | PowerShellExtractor: symbols, both call forms, imports, inheritance, Read/Write, scopes | ✓ VERIFIED | 776 lines, substantive, 12 passing unit tests |
| `src/extract/astro.rs` | AstroExtractor: frontmatter+script merge, offset shift | ✓ VERIFIED | 459 lines, substantive, 5 passing unit tests |
| `src/lang.rs` | `Language::PowerShell`/`Language::Astro` + extension dispatch | ✓ VERIFIED | Both variants present, in `Language::ALL`, `.ps1`/`.psm1`/`.astro` in `extensions()`, exhaustiveness guard test passes |
| `Cargo.toml` | Both features in `default`, astro includes typescript | ✓ VERIFIED | `default = [..., "powershell", "astro"]`; `astro = ["dep:tree-sitter-astro-next", "typescript", "_extractors"]` |
| `bindings/node/Cargo.toml`, `bindings/python/Cargo.toml` | Both list `powershell`/`astro` | ✓ VERIFIED | Both files' explicit feature lists contain both strings |
| `eval/corpus/powershell/scoped_call/`, `eval/corpus/astro/scoped_call/` | Golden fixtures | ✓ VERIFIED | Both exist with real source + `expected.edges`, resolve under `eval/tests/regression.rs`'s auto-discovery |
| `docs/supported-languages.md` | PowerShell + Astro rows, 🟢 | ✓ VERIFIED | Both rows present in supported block with honest capability columns |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `src/extract/dispatch.rs` | `src/extract/powershell.rs` | `PowerShellExtractor` match arm | ✓ WIRED | Confirmed by gsd-tools + manual grep |
| `bindings/node/Cargo.toml` | `Cargo.toml` | `"powershell"` in explicit features list | ✓ WIRED (manually confirmed) | gsd-tools link-checker returned a false negative here ("Target not referenced in source") — direct `grep -n "powershell" bindings/node/Cargo.toml` shows the feature string present in the `code2graph_core` dependency's `features = [...]` list. Treated as a tool limitation, not a real gap. |
| `src/extract/astro.rs` | `src/extract/typescript.rs` | `extract_ecmascript(...)` | ✓ WIRED | Confirmed by gsd-tools + manual read of `merge_block()` |
| `src/extract/dispatch.rs` | `src/extract/astro.rs` | `AstroExtractor` match arm | ✓ WIRED | Confirmed by gsd-tools |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| PowerShell unit tests pass | `cargo test extract::powershell::tests --all-features` | 12/12 passed | ✓ PASS |
| Astro unit tests pass | `cargo test extract::astro::tests --all-features` | 5/5 passed | ✓ PASS |
| PowerShell isolated build | `cargo check --no-default-features --features powershell` | Clean (only pre-existing unrelated dead-code warnings) | ✓ PASS |
| Astro isolated build | `cargo check --no-default-features --features astro` | Clean (only pre-existing unrelated dead-code warnings) | ✓ PASS |
| Full workspace test suite | `cargo test --workspace --all-features` | 790 + 11 + 10 + 2 doctests passed, 0 failed | ✓ PASS |
| napi drift check | `npx napi build --release --platform` then `git diff --exit-code index.js index.d.ts` | No drift | ✓ PASS |
| fmt gate | `cargo fmt --all -- --check` | Clean | ✓ PASS |
| clippy gate | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Clean | ✓ PASS |
| PowerShell isolated test binary | `cargo test --no-default-features --features powershell` | 17× E0432 (pre-existing resolver test-import gap, confirmed identical with `--features lua`) | ✗ FAIL (documented, non-blocking, pre-existing — see Truth #3) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|--------------|--------|----------|
| LANG-08 | 02-01 | PowerShell extractor (template: Shell) | ✓ SATISFIED | `src/extract/powershell.rs` end-to-end, 12 tests pass |
| LANG-10 | 02-02 | Astro extractor via embedded-SFC pattern | ✓ SATISFIED | `src/extract/astro.rs` end-to-end, 5 tests pass |
| BIND-01 | 02-01, 02-02 | New language feature strings added to both bindings' Cargo.toml in same change | ✓ SATISFIED | Both `powershell`/`astro` present in both bindings' Cargo.toml |
| BIND-02 | 02-01, 02-02 | Committed napi artifacts regenerated and drift-free | ✓ SATISFIED | `npx napi build --release --platform` → no diff against committed `index.js`/`index.d.ts` |

No orphaned requirements found for Phase 2 in `REQUIREMENTS.md`.

### Anti-Patterns Found

None. No TODO/FIXME/placeholder/unimplemented markers in `src/extract/powershell.rs` or `src/extract/astro.rs`. Both corpus fixtures contain real, role-typed source (not trivial/empty stubs) with resolvable same-file `Call` edges.

### Human Verification Required

None. All must-haves are verifiable programmatically via tests, grep, and build commands; no visual/UX/external-service behavior in scope for this phase.

### Gaps Summary

No blocking gaps. One pre-existing, already-documented, project-wide limitation was independently confirmed during verification: `cargo test --no-default-features --features <lang>` (the literal wording of ROADMAP success criterion #3 and both plans' must-have truths) fails to compile its test binary for **any** single isolated language feature — not specific to PowerShell or Astro — because `src/resolve/symbol_table.rs` and `src/resolve/scope_graph.rs` unconditionally import several extractors' structs at module scope. This was confirmed pre-existing (reproduced identically with `--features lua`, an untouched language), is tracked in `STATE.md`'s Blockers/Concerns and `deferred-items.md`, and does not affect CI (whose `feature-isolation` job correctly uses `cargo check`, which does pass) or the production extractor code (also verified isolated via `cargo check`). Recommend a dedicated follow-up plan (not bundled into a language-addition phase) to `#[cfg(feature = "...")]`-gate the resolver test modules, as already flagged by the phase's own SUMMARY documents.

---

*Verified: 2026-07-05*
*Verifier: Claude (gsd-verifier)*
