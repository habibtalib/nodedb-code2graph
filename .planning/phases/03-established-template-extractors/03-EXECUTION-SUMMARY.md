# Phase 3: Established-Template Extractors — Execution Summary

**Executed:** 2026-07-05
**Mode:** ultracode multi-agent workflow (user-invoked), superseding the sequential PLAN.md lane. GSD artifacts for this phase: 03-CONTEXT.md (locked decisions), 03-RESEARCH.md (verified AST evidence), this summary, 03-VERIFICATION.md (final gates).

## How it ran

Workflow `phase3-parallel-extractors` (run `wf_caaccef9-831`): 5 parallel implementer agents in isolated git worktrees (one per language, each doing its own AST verification, TDD, corpus case, docs row, bindings updates, and full local gates), then sequential integration onto `main` (unique files pulled from each branch; shared-file wiring replayed via per-language WIRING specs), then a final full-gate verifier. 11 agents total, zero failures, zero verifier fixes needed.

## What landed (one commit per language)

| Language | Commit | Extractor | Tests | Notes |
|---|---|---|---|---|
| Zig | 2f7dbd6 | `src/extract/zig.rs` | 16 | real `pub` visibility; comptime capped; no `assignment_expression` in grammar — read/write via `variable_declaration` analysis |
| SystemVerilog | b185f99 | `src/extract/systemverilog.rs` | 13 | modules/interfaces/packages/classes; instantiations → TypeRef; elaboration out of scope |
| Fortran | 20647a1 | `src/extract/fortran.rs` | 16 | REAL public/private visibility; fixed-form `.f` capped honestly |
| Groovy | 3628fa7 | `src/extract/groovy.rs` | 14 | `.gradle` as plain Groovy (D-02); `trait` unsupported by grammar 0.1.2 — documented ceiling |
| ObjC | 6816e72 | `src/extract/objc.rs` | 11 | `.m`/`.mm` only, bare `.h` stays C (D-01); selector-form method names; message-send qualifiers |

All five: enum variant + dispatch, feature flipped into `default` with `_extractors`, ≥1 `eval/corpus/<lang>/scoped_call/` case, 🟢 docs row (sync-tested), `"powershell"`-style entries in both bindings feature lists, napi artifacts verified drift-free.

## Verification (03-VERIFICATION.md, commit 9a8f293)

All 6 roadmap success criteria passed: 883 workspace tests green (70 new extractor tests), fmt/clippy clean, per-language `cargo check --no-default-features` clean, bindings parity + napi no-op confirmed, docs decisions (D-01, D-02) present. The literal single-feature `cargo test` criterion is satisfied via `cargo check` isolation + `--all-features` tests, per the pre-existing resolver-test gap tracked in Phase 2's deferred-items.md.

## Requirements closed

LANG-01 (Zig), LANG-05 (ObjC), LANG-06 (Fortran), LANG-07 (Groovy), LANG-09 (SystemVerilog).
