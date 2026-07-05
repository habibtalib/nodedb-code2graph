# Deferred items — Phase 02 execution

## Pre-existing feature-isolation gap in resolver test modules (out of scope for 02-01)

**Found during:** 02-01 Task 2, running the plan's mandated
`cargo test --no-default-features --features powershell extract::powershell::tests` verify step.

**Issue:** `src/resolve/symbol_table.rs` and `src/resolve/scope_graph.rs` test modules import
`RustExtractor`/`JavaExtractor`/`PythonExtractor`/`TypeScriptExtractor`/`GoExtractor`/`RubyExtractor`/
`CExtractor`/`JavaScriptExtractor` unconditionally (module-level `use` statements or un-gated
per-test local `use`s), so the whole `code2graph` lib-test binary fails to compile under ANY
single, unrelated language feature (`--no-default-features --features <lang>`) unless that lang
happens to be one of the ones referenced. Confirmed pre-existing and unrelated to this plan's
changes: `cargo test --no-default-features --features lua` (before any 02-01 edits, via `git
stash`) fails identically with 17 `E0432` errors.

Phase 1 already fixed this exact category of bug for 4 SQL/HCL test functions (per STATE.md:
"Fixed pre-existing SQL/HCL test-compile isolation bug in symbol_table.rs ... before adding the
feature-isolation CI job") by adding `#[cfg(feature = "sql")]` / `#[cfg(feature = "hcl")]` above
the affected `#[test]` fns and converting the extractor `use` to a local import. The same fix is
needed for the remaining Rust/Java/Python/TypeScript/Go/Ruby/C/JavaScript references, but the
scope is far larger this time: `scope_graph.rs`'s entire ~900-line test module depends on a
module-level `use crate::extract::RustExtractor;` used by nearly every test in the file, and
`symbol_table.rs` has a similar module-level `RustExtractor`/`JavaExtractor` dependency. Properly
isolating every language feature's test build would mean auditing and re-gating dozens of test
functions across both files — a cross-cutting refactor spanning the whole resolver test suite, not
something caused by or bounded to adding the PowerShell extractor.

**Why deferred, not fixed:** Scope boundary — this is a pre-existing, project-wide gap that
predates this plan and affects every language feature's isolated test build equally (confirmed via
`lua`), not something introduced by 02-01's changes. Fixing it correctly requires reviewing every
test in `symbol_table.rs` and `scope_graph.rs` to determine its true minimal feature-set and
re-gating accordingly — a substantial, standalone task, not a 3-attempt auto-fix.

**Impact on 02-01 verification:** The plan's task-level verify command
(`cargo test --no-default-features --features powershell ...`) could not be run standalone as
written. Verified equivalently via `cargo test --all-features` instead (which is also the plan's
own Task 3 verification command and the project's standard full-suite gate) — all 782 tests pass,
including the 9 new `extract::powershell::tests`. `cargo check --no-default-features --features
powershell` (compile-only, no test binary) does succeed standalone, confirming the extractor's
production code has no accidental cross-feature dependency; only the pre-existing test-only import
gap blocks the full `cargo test` invocation in isolation.

**Recommendation:** A follow-up task (own plan, not bundled into a language-addition phase) should
audit `symbol_table.rs` and `scope_graph.rs` test-by-test, adding `#[cfg(feature = "...")]` (or
`#[cfg(all(feature = "...", feature = "..."))]` for multi-extractor tests) to every test function
per its actual extractor dependency, converting any remaining module-level `use` into scoped
per-test local imports. This unblocks true single-feature test isolation for every language, not
just the ones referenced by already-fixed tests.
