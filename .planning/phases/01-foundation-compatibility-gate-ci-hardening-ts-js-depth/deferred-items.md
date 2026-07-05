# Deferred Items — Phase 01

## From 01-03 (CI hardening / feature-isolation job)

### Broader test-module extractor-import gating gap in `src/resolve/symbol_table.rs`

**Found during:** Task 1 pre-check (fixing the SQL/HCL isolation gap flagged for this plan).

**Scope:** `src/resolve/symbol_table.rs`'s `#[cfg(test)] mod tests` unconditionally imports
`RustExtractor`, `JavaExtractor`, `PythonExtractor`, `GoExtractor`, `CExtractor`, and `RubyExtractor`
inside individual test functions, the same shape as the `SqlExtractor`/`HclExtractor` bug this plan
fixed. These only fail to compile under `cargo check --tests --no-default-features --features <lang>`
(or `cargo test --no-default-features --features <lang>`) for a language other than the one each test
happens to use — NOT under the plain `cargo check --no-default-features --features <lang>` that this
plan's new `feature-isolation` CI job actually runs (verified: all 37 current language features pass
the plain `cargo check` cleanly, with zero fixes).

**Why deferred:** Out of scope for COMPAT-03 as literally specified — the new CI job uses `cargo check`,
not `cargo check --tests`/`cargo test`, so this gap is invisible to the job being added. Fixing all of
it (6+ extractors across ~15 test functions) exceeds the "smallest honest fix" directive that scoped this
plan's fix to the two extractors (`SqlExtractor`, `HclExtractor`) confirmed by the 01-04 executor.

**Recommendation:** If a future plan adds a `cargo test --no-default-features --features <lang>`-style
CI check (deeper than compile-only isolation), gate every extractor import in this test module the same
way (`#[cfg(feature = "...")]` above `#[test]`, matching the pattern already used in
`src/extract/support.rs`'s test module).
