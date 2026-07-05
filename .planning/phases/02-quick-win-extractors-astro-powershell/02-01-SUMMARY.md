---
phase: 02-quick-win-extractors-astro-powershell
plan: 01
subsystem: extraction
tags: [tree-sitter, powershell, extractor, napi, bindings-parity]

# Dependency graph
requires:
  - phase: 01-foundation-compatibility-gate-ci-hardening-ts-js-depth
    provides: tree-sitter-powershell registered + ABI-verified in src/grammar.rs
provides:
  - Language::PowerShell enum variant with .ps1/.psm1 extension dispatch
  - src/extract/powershell.rs (PowerShellExtractor) — functions/filters, PS5+
    classes with inheritance, both call forms, all 3 import forms, read/write
    references, Function scopes
  - eval/corpus/powershell/scoped_call/ golden fixture
  - docs/supported-languages.md PowerShell row moved 🟠 → 🟢
  - powershell feature flipped into default; bindings/node and
    bindings/python Cargo.toml feature parity (BIND-01); verified no-op napi
    diff (BIND-02)
affects: [02-02-astro, future language-addition phases needing the bindings-parity practice]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "PowerShell call detection: two collect_call_references passes (cmdlet-style CMDLET_CALL_QUERY + member/expression-style MEMBER_CALL_QUERY) concatenated, with a byte-offset HashSet double-count guard excluding command_name nodes already classified as imports"
    - "Import classification via raw Query/QueryCursor grouping matches by command-node byte offset (a query capture repeated per sibling produces one match per sibling, not one match with all captures together — the grouping step is required to reassemble ordered argument lists)"
    - "Manual node-child walk for class name + optional base-class (not a combined query with an optional capture — verified duplicate-match gotcha)"
    - "Read/Write detection via ancestor-walk (has_ancestor_kind) rather than immediate-parent check, since the grammar wraps every sub-expression in a full operator-precedence chain"

key-files:
  created:
    - src/extract/powershell.rs
    - eval/corpus/powershell/scoped_call/deploy.ps1
    - eval/corpus/powershell/scoped_call/expected.edges
  modified:
    - src/lang.rs
    - Cargo.toml
    - bindings/node/Cargo.toml
    - bindings/python/Cargo.toml
    - src/extract/mod.rs
    - src/extract/dispatch.rs
    - docs/supported-languages.md

key-decisions:
  - "Query grouping fix: IMPORT_ARG_QUERY's (generic_token) @arg capture matches once per sibling generic_token, not once per command with all args together — grouped matches by the command node's start_byte to reassemble ordered [module, MyModule]-style argument pairs for using module"
  - "Read/Write classification uses a full ancestor-chain walk (has_ancestor_kind), not an immediate-parent check as the plan's prose literally described, because the pinned grammar wraps every sub-expression (even a bare variable) in a full precedence-operator chain (unary_expression > array_literal_expression > range_expression > ... > left_assignment_expression)"
  - "PowerShell row relocated from the 🟠-planned block into the 🟢-supported block in docs/supported-languages.md (not just re-marked in place), keeping the doc's status grouping honest"

patterns-established:
  - "Query-capture-per-sibling grouping pattern: when a tree-sitter query captures a repeated child directly (no wrapping list node), group QueryCursor matches by a stable anchor node's byte offset before assuming multiple captures share one match"

requirements-completed: [LANG-08, BIND-01, BIND-02]

# Metrics
duration: 25min
completed: 2026-07-05
---

# Phase 2 Plan 1: PowerShell Extractor Summary

**PowerShellExtractor emitting function/filter/class/method symbols, both cmdlet-style and member-style call references, all three import forms with a double-count guard, and ancestor-walk-based Read/Write references — closing LANG-08, BIND-01, BIND-02.**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-07-05T19:38 (init+context read)
- **Completed:** 2026-07-05T19:59
- **Tasks:** 3 completed (Task 2 executed as RED→GREEN TDD, 2 commits)
- **Files modified:** 11 (2 new source/test-bearing, 2 new corpus fixture files, 1 new deferred-items doc, 6 modified)

## Accomplishments
- `Language::PowerShell` wired end-to-end (enum, extensions, `as_str`, exhaustiveness guard, dispatch)
- `PowerShellExtractor` emits real SCIP-id symbols for functions/filters, PS5+ classes, and methods, with `IsImplementation` inheritance edges via a manual node-walk (avoiding a verified duplicate-match query pitfall)
- Both call forms detected (parenless cmdlet-style and `.`/`::` member-style with qualifier capture), with a byte-offset guard so `Import-Module`/`using module` commands never also emit a spurious `Call`
- All three import forms (`Import-Module`, `using module`, dot-sourcing) correctly classified as `Import` references
- Read/Write references and real `Function` scopes (replacing the Task-2 placeholder Module-only scope tree)
- `eval/corpus/powershell/scoped_call/` golden fixture resolves its one same-file `Call` edge under the eval harness
- `docs/supported-languages.md` row moved into the supported (🟢) section with honest capability columns
- `bindings/node` and `bindings/python` Cargo.toml feature parity; verified `npx napi build --release --platform` produces a no-op diff against committed `index.js`/`index.d.ts`

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire Language::PowerShell + Cargo feature flip + bindings feature lists** - `44cdac1` (feat)
2. **Task 2: PowerShellExtractor — symbols, both call forms, 3 import forms** - `5882253` (test, RED) + `aafed64` (feat, GREEN)
3. **Task 3: Read/Write refs + Function scopes + corpus case + docs row + bindings verify** - `6a4792b` (feat)

_Task 2 followed the TDD flow (tdd="true"): RED commit added the stub extractor + 9 failing tests, GREEN commit implemented the extractor logic until all 9 passed._

## Files Created/Modified
- `src/extract/powershell.rs` - PowerShellExtractor: symbols, both call forms, imports, inheritance, Read/Write, Function scopes
- `eval/corpus/powershell/scoped_call/deploy.ps1` / `expected.edges` - golden same-file Call fixture
- `src/lang.rs` - `Language::PowerShell` variant, extensions, `as_str`, exhaustiveness guard
- `Cargo.toml` - `powershell` feature gains `_extractors`; joins `default`
- `bindings/node/Cargo.toml`, `bindings/python/Cargo.toml` - `powershell` added to explicit feature lists
- `src/extract/mod.rs`, `src/extract/dispatch.rs` - module/dispatch wiring
- `docs/supported-languages.md` - PowerShell row moved 🟠 → 🟢, relocated into the supported block

## Decisions Made
- Grouped `IMPORT_ARG_QUERY` matches by the command node's byte offset rather than assuming one match carries all `@arg` captures — the query's repeated-sibling capture pattern produces one match per `generic_token`, discovered when `using module MyModule`'s second-argument logic initially failed (both "using"+"module" and "using"+"MyModule" arrived as separate matches).
- Implemented Read/Write classification via a full ancestor-chain walk rather than an immediate-parent check, since the pinned `tree-sitter-powershell` 0.26.4 grammar wraps every sub-expression (including a bare `$x`) in a full operator-precedence chain — verified directly by dumping the AST for `$x = 1; $y = $x` before writing the logic.
- Reused `import_bindings` (not explicitly called out in the plan's Task 2 text but used by every other import-emitting extractor in this codebase) for Tier-B binding parity, since the docs row claims Tier-B (🟢) resolution including Imports.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed `using module` argument-pairing bug in import classification**
- **Found during:** Task 2 GREEN implementation, `using_module_is_import_not_call` test
- **Issue:** `IMPORT_ARG_QUERY`'s `(generic_token) @arg` capture matches once per generic_token sibling; for `using module MyModule` (2 generic_token children) this produced 2 separate matches (`cmd="using", arg="module"` and `cmd="using", arg="MyModule"`) rather than one match with both args, so the original code's `args.get(1)` (assuming both were in one match) never found the second argument.
- **Fix:** Grouped matches by the `@cmd` node's `start_byte()` into `Vec<(Node, Vec<Node>)>` before classification, reassembling the full ordered argument list per command.
- **Files modified:** `src/extract/powershell.rs`
- **Verification:** `using_module_is_import_not_call` passes; confirmed via a throwaway `ast_query_test_tmp.rs` example (deleted after verification) showing the 2-match shape directly.
- **Committed in:** `aafed64` (Task 2 GREEN commit)

**2. [Scope boundary — logged, not fixed] Pre-existing feature-isolation gap in resolver test modules**
- **Found during:** Task 2, running the plan's mandated `cargo test --no-default-features --features powershell extract::powershell::tests` verify step
- **Issue:** `src/resolve/symbol_table.rs` and `src/resolve/scope_graph.rs` test modules import `RustExtractor`/`JavaExtractor`/`PythonExtractor`/`TypeScriptExtractor`/`GoExtractor`/`RubyExtractor` unconditionally (module-level or un-gated per-test local `use`s), so the whole lib-test binary fails to compile under any single, unrelated language feature. Confirmed pre-existing and unrelated to this plan (`cargo test --no-default-features --features lua`, via `git stash`, fails identically with 17 `E0432` errors before any 02-01 edits).
- **Why not fixed:** Scope boundary — a cross-cutting refactor spanning dozens of test functions across two ~900-line files, well beyond a bounded 3-attempt auto-fix and orthogonal to adding the PowerShell extractor. Phase 1 fixed the same category of bug for 4 SQL/HCL tests specifically; the remaining Rust/Java/Python/TypeScript/Go/Ruby cases are a much larger, standalone follow-up.
- **Logged to:** `.planning/phases/02-quick-win-extractors-astro-powershell/deferred-items.md`
- **Verified equivalently via:** `cargo test --all-features` (785 tests pass, including all 12 `extract::powershell::tests`) and `cargo check --no-default-features --features powershell` (compiles standalone — confirms the extractor's *production* code has no accidental cross-feature dependency; only the pre-existing test-only import gap blocks the full `cargo test` invocation in isolation).

---

**Total deviations:** 2 (1 auto-fixed bug, 1 logged-and-deferred pre-existing gap)
**Impact on plan:** The bug fix was necessary for correctness (Rule 1). The deferred item means one of the plan's must-have truths — `cargo test --no-default-features --features powershell` passing standalone — could not be demonstrated exactly as worded; verified equivalently via `--all-features` instead, with the underlying pre-existing gap logged for a dedicated follow-up. No scope creep into unrelated resolver test files.

## Issues Encountered
- See deviation #1 (query-grouping bug) and #2 (pre-existing test-isolation gap) above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- PowerShell extractor fully lands LANG-08; bindings parity (BIND-01/02) practice demonstrated end-to-end and ready to repeat for Astro in 02-02.
- `deferred-items.md` flags a real, pre-existing project-wide gap (resolver test-module feature isolation) worth a dedicated follow-up plan — not blocking 02-02, which is unaffected (Astro's own isolated build would hit the same gap, to be verified via `--all-features` the same way).

---
*Phase: 02-quick-win-extractors-astro-powershell*
*Completed: 2026-07-05*

## Self-Check: PASSED

All created files verified present; all 4 task/RED-GREEN commits verified in `git log`.
