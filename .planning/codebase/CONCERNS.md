# Codebase Concerns

**Analysis Date:** 2026-07-05

## Critical Issues

### Bindings API Drift (High Impact)

**Issue:** Node.js NAPI bindings (`index.js`, `index.d.ts`) can drift from the Rust API when `#[napi]` signatures change.

- **Files:** `bindings/node/src/lib.rs` (Rust definitions), `bindings/node/index.js` (committed loader), `bindings/node/index.d.ts` (TypeScript definitions)
- **Impact:** Stale committed files → broken npm package on release. Published npm tarball ships exact loader as committed, so regeneration without re-commit = version mismatch for consumers.
- **Current mitigation:** CI gate added (commit e6a67f9) — `napi build` regenerates files, then `git diff --exit-code` fails the gate if they differ. See `.github/workflows/test.yml` lines 89–95.
- **Workflow:** Developers must run `npm run build` in `bindings/node/` after touching `src/lib.rs`, then commit both `index.js` and `index.d.ts`. Miss either → CI fails.
- **Risk:** Human error on release. A breaking API change without regeneration gets into released npm package.

### Tree-Sitter Version Constraint (Medium-High Impact)

**Issue:** Hard-pinned tree-sitter dependency (`>=0.24, <0.27`) creates a grammar compatibility window that blocks many planned languages.

- **Files:** `Cargo.toml` line 56
- **Impact:** Grammar crates built against tree-sitter 0.20–0.23 or 0.27+ cannot be used. Vue, F#, and other languages in the "🔴 blocked" row of `docs/supported-languages.md` are blocked entirely because their only maintained grammars target incompatible tree-sitter versions.
- **Ceiling:** The 0.24–0.26 window is 3 minor versions wide. As tree-sitter releases progress, grammars drift out of compatibility. By tree-sitter 0.27 or later, this entire window closes.
- **Migration path:** Eventually (tree-sitter 0.27+), all grammar crates will need upgrade, but that's a coordinated cross-ecosystem change. Current approach: accept the window and wait for the ecosystem. No workaround without violating `CONTRIBUTING.md` § "When a Language Has No Usable Grammar" rule 3 (no transmuting incompatible `Language` types).
- **Documentation:** Enforced in `src/grammar.rs` via `abi_versions_are_compatible()` test; contributors must verify compatibility per `CONTRIBUTING.md` line 95.

## Language Support Gaps

### Incomplete Feature Coverage on Supported Languages

**Issue:** Several Tier-B supported languages have blank capability columns in `docs/supported-languages.md` — capabilities that are not extracted today despite being real language features.

- **Files:** `docs/supported-languages.md` (matrix rows 47–65), corresponding per-language extractors in `src/extract/<lang>.rs`
- **Gaps:**
  - **Ruby** (`src/extract/ruby.rs`): No imports (column blank) / no type-refs (column blank) — extracting Ruby's `require`/`require_relative` and dynamic type patterns is incomplete.
  - **C** (`src/extract/c.rs`): No imports (column blank) — C has preprocessor `#include` but no semantic imports; this is honest but limits C-only projects' reachability.
  - **Shell** (`src/extract/shell.rs`): No imports (column blank), no type-refs (blank), no read/write (blank) — extraction is calls and entry points only.
  - **Lua** (`src/extract/lua.rs`): No type-refs (blank), no inheritance (blank) — Lua's dynamic typing and metatable patterns aren't captured.
  - **Luau** (via Lua): same gaps as Lua.
- **Priority:** Low to Medium. These gaps are honest (the language semantics don't cleanly map to the schema), but they limit resolution depth for consumers of those languages.
- **Contribution:** Each blank is an open gap — see `CONTRIBUTING.md` line 89 ("Blank cells on supported rows are real gaps").

### Oracle Coverage Incomplete (Medium Impact)

**Issue:** Most Tier-B supported languages are not oracle-measured (external SCIP ground truth). Only 9 languages have `<lang>_oracle/` corpus directories with independently verified precision/recall.

- **Files:** `eval/corpus/` contains 9 oracle directories (`rust_oracle/`, `ts_oracle/`, `py_oracle/`, `java_oracle/`, `go_oracle/`, `kotlin_oracle/`, `cpp_oracle/`, `c_oracle/`, `ruby_oracle/`)
- **Covered (⭐ tier, oracle-measured):** Rust, TypeScript, Python, Java, Go, Kotlin, C++, C, Ruby (9 total)
- **Not oracle-measured (🟢 tier, expected-good, not proven):** PHP, Swift, C#, Scala, Dart, Solidity, Lua, Luau, Pascal, Shell, Svelte, and JavaScript (delegated to TS)
- **Impact:** The README claims Tier-B resolution "scope-aware: resolves through lexical scopes, imports, and qualified paths" with honest confidence, but precision/recall is unquantified for 12+ languages. A consumer relying on PHP or Swift Tier-B gets no external evidence of accuracy.
- **Current test:** 18 corpus cases with oracle scoring, 23 with hand-authored `expected.edges` (lower rigor). No regression gate on precision/recall — only that golden fixtures don't regress.
- **Why it matters:** "Best at code→graph" (README tagline) is unverified for 2/3 of the Tier-B set. Re-measuring against SCIP oracles would either confirm quality or expose gaps.

### Planned and Blocked Languages Need Verification (Low-Medium Impact)

**Issue:** 🟠 (planned) and 🔴 (blocked) rows in `docs/supported-languages.md` carry tree-sitter version compatibility risks that aren't pre-validated.

- **Files:** `docs/supported-languages.md` lines 68–86, corresponding grammar crates listed in comments
- **Planned languages (🟠):** Elixir, Erlang, Gleam, Zig, Julia, R, Haskell, OCaml, Objective-C, Fortran, Groovy, PowerShell, SystemVerilog, Astro (14 total). Each references a grammar (e.g., `tree-sitter-elixir`) but compatibility is not pre-verified.
- **Blocked languages (🔴):** Vue, Liquid, F#, Salesforce Apex, COBOL (5 total). These have no usable compatible grammar today.
- **Risk:** A contributor adds a 🟠 language with a grammar that targets tree-sitter 0.20 or 0.28, and only discovers incompatibility during CI via the `abi_versions_are_compatible()` test. This wastes a review cycle.
- **Workaround:** Pre-check crates.io before opening a PR, per `CONTRIBUTING.md` line 95. Not automated.

## Cross-Language FFI Gaps (Frontier)

### Most FFI Boundaries Not Yet Bridged (Medium Impact)

**Issue:** The FFI bridge resolver only links 5 ABI mechanisms today (C ABI, PyO3, Wasm, Node-API, JNI). All other standard FFI boundaries (Go cgo, C# P/Invoke, Rust `extern "C"` imports, etc.) are marked 🟠 (not yet bridged) in `docs/ffi-support-matrix.md`.

- **Files:** `src/ffi/` (5 spec files: `c.rs`, `python.rs`, `wasm.rs`, `node_api.rs`, `jni.rs`); FFI detection in per-language extractors (e.g., `src/extract/rust.rs` lines for `#[no_mangle]`)
- **Implemented bridges (🟢):** C ABI (C/C++ → Rust), PyO3 (Python → Rust), Wasm (JS/TS → Rust), Node-API (JS/TS → Rust), JNI (Java → Rust/C)
- **Not bridged (🟠, listed in `docs/ffi-support-matrix.md` lines 65–71):**
  - C / C++ as export side (Rust calling C; C++ calling C)
  - Go cgo (`//export` / `import "C"`)
  - C# P/Invoke (`[DllImport]`)
  - Kotlin/Native `@CName` ↔ C
  - Rustler NIFs (Elixir/Erlang → Rust)
  - Swift `@_cdecl`, pybind11 / Cython (C++ → Python)
  - Python `ctypes` / `cffi` (handle-based calls, not bare names)
  - WebAssembly component model / WIT imports (beyond `wasm-bindgen`)
- **Impact:** A Rust app calling a C library, or a Go app calling C, produces no cross-language edges. The call site is unlinked.
- **Architecture:** The resolver pattern is extensible (one `FfiAbi` enum variant + one `src/ffi/<abi>.rs` spec file + extractor changes). See `CONTRIBUTING.md` lines 100–105.
- **Priority:** Medium. Real and common boundaries, but incremental work to add each.

## Performance & Scalability Concerns

### Large Extractor Files (Low-Medium Impact)

**Issue:** Several per-language extractors exceed 2000 lines of tree-sitter walk code, creating maintenance and performance overhead.

- **Files:** Largest extractors in `src/extract/`:
  - `rust.rs` — 2932 lines
  - `java.rs` — 2164 lines
  - `solidity.rs` — 2101 lines
  - `kotlin.rs` — 2083 lines
  - `swift.rs` — 2048 lines
  - `cpp.rs` — 1905 lines
  - `go.rs` — 1746 lines
  - `php.rs` — 1706 lines
  - `python.rs` — 1685 lines
- **Risk:** Complex tree-sitter walks are hard to understand and audit. Finding edge cases or performance issues requires reading deep code paths. Adding new capabilities (e.g., FFI detection, entry-point markers) requires surgical edits that risk regression.
- **No current performance bottleneck reported,** but single-file extraction speed is not benchmarked in the test suite. Heavy tree-sitter traversals on large files could be slow.
- **Mitigation:** Extractors reuse `src/extract/support.rs` helpers, limiting duplication. No immediate refactor needed unless performance issues surface.

### No Incremental Rebuild Benchmarks (Low Impact)

**Issue:** The `IncrementalGraph` feature allows per-file resolution without re-extracting the whole workspace, but its performance characteristics are not validated.

- **Files:** `src/resolve/incremental/` (mod.rs, store.rs, subgraph.rs, stitch.rs); public API in `src/resolve/mod.rs`
- **Feature:** `IncrementalGraph` keeps a graph current as files change — only the changed file is re-extracted/re-resolved, and cross-file edges are stitched on-demand.
- **Concern:** No benchmark suite exists to validate that incremental updates are faster than re-running full resolution. For a consumer relying on incremental updates for fast IDE/tool responsiveness, a worst-case O(n) stitch operation could negate the benefit.
- **Evidence:** No test in `.github/workflows/test.yml` exercises incremental performance. Only correctness tests exist.

## Security & Stability Concerns

### serde_json Serialization Surface (Low-Medium Impact)

**Issue:** The Node and Python bindings serialize all `FileFacts` and `CodeGraph` via `serde_json::to_value()`. Large codebases could produce deeply nested JSON that exhausts memory or causes DoS.

- **Files:** `bindings/node/src/lib.rs` line 26, `bindings/python/src/lib.rs` (equivalent)
- **Risk:** No input bounds on file size or symbol count. Extracting a pathologically large file (100k+ symbols) serializes all at once. A malicious or misconfigured input could cause memory exhaustion.
- **Current mitigation:** None — the library is a primitive, not a server. Consumers are responsible for input validation and resource limits.
- **Recommendation:** Document this in API docs; suggest that tools processing untrusted input add extraction size limits upstream.

### Breadth of Grammar Dependencies (Low Impact)

**Issue:** The default feature set (`Cargo.toml` line 24) enables 23 language grammar crates, each with its own C parser and build dependencies. A vulnerability in one grammar's generated parser affects the whole build.

- **Files:** `Cargo.toml` lines 28–49 (23 feature flags for 23 languages), each pulling a grammar crate
- **Scope:** This is deliberate — every language is optional, and consumers can choose exactly which grammars to pull via feature flags. But the **default** enables all 23.
- **Risk:** Low in practice because tree-sitter parsers are generated C from formal grammars, not hand-written. Supply chain risk is isolated per grammar repo, not centralized. But the attack surface is large.
- **Mitigation:** Documented in `CONTRIBUTING.md`. Consumers who care can enable only the languages they need: `cargo add code2graph --no-default-features --features rust,python,typescript`.

### No Fuzzing or Property-Based Tests (Low Impact)

**Issue:** The extraction pipeline (tree-sitter walk → symbol/reference collection) and resolution (name matching, scope graph traversal) are untested against malformed or adversarial input.

- **Files:** Unit tests in each extractor (`src/extract/<lang>.rs` test modules) and resolver tests (`src/resolve/*.rs`); corpus cases in `eval/`
- **Current testing:** Golden fixtures (hand-authored code snippets with expected output) and oracle corpus cases (code2graph output vs. external SCIP indices). No fuzzing of tree-sitter parser output or pathological input (cyclic scope graphs, millions of symbols, etc.).
- **Impact:** Low, because tree-sitter parsing is bulletproof (it never crashes on malformed input), but extractor logic could have edge cases on unusual ASTs. E.g., a deeply nested generic type, or a scope graph with unexpected back-edges, could cause panic or incorrect output.
- **Not urgent:** The library is pre-0.1 and already marked as "Early, pre-0.1" in the README. Fuzz testing is a post-1.0 hardening task.

## Technical Debt

### Planned Entry-Point Detection Incomplete (Low Impact)

**Issue:** Entry-point detection (`EntryPoint::Main`, `EntryPoint::HttpRoute`) is implemented for some languages (Rust, Python, Java, Go, C, C++, Kotlin, Scala, Swift, C#, Shell), but not all Tier-B languages.

- **Files:** Entry-point detection logic in each extractor; registry in `docs/supported-languages.md` column "Entry-pts"
- **Gaps (blank cells):** TypeScript, JavaScript (via TS), Ruby, PHP, Lua, Luau, Pascal, Solidity, HCL, SQL, and all 🟠/🔴 languages have no entry-point detection yet.
- **Not critical:** Entry points are optional facts; consumers who don't need them simply ignore the column.
- **Contribution:** Each blank is a mechanical pattern match for the language (e.g., TypeScript: `function`, `export function`, `@Controller`).

### Documentation Drift Risk (Low Impact)

**Issue:** `docs/supported-languages.md` and `docs/ffi-support-matrix.md` are hand-maintained and guarded by sync tests, but the sync tests only verify that updates to language coverage / FFI specs are documented.

- **Files:** `docs/supported-languages.md` (matrix of supported languages), `docs/ffi-support-matrix.md` (FFI bridge matrix), sync tests in `src/ffi/sync_tests.rs` (checks ffi-support-matrix against `SPECS` registry)
- **Guard:** The sync test `ffi_markers_are_documented` ensures `src/ffi/` definitions don't drift from documentation, but the inverse is human-driven. If someone adds a capability to an extractor without updating the matrix, the PR review must catch it.
- **Low risk:** Small matrix, small team, review discipline in place. But a missed column update could silently lie to users.
- **Current mitigation:** Code review checklist (implicit in CONTRIBUTING.md line 93).

## Pre-0.1 Status & Evolution Risks (Medium Impact)

**Issue:** The library is early, pre-0.1. The SCIP identity scheme (`SymbolId` descriptor rendering) and the graph schema (`FileFacts`, `CodeGraph`, reference kinds) are explicitly marked as "may still evolve before 0.1" in `README.md` line 161 and the status badge.

- **Impact:** Consumers building on code2graph during pre-0.1 should expect breaking API changes. Once published to crates.io / PyPI / npm, upgrades will require adaptation.
- **Scope:** The library is already published (crates.io, PyPI, npm badges in `README.md` lines 32–36), so breaking changes affect real users.
- **Current policy:** Conventional Commits in PRs, clear changelog expected on version bump (not documented in `CONTRIBUTING.md`). No stable 0.1 milestone defined in the repository.
- **Recommendation:** Document a 0.1 feature freeze date or condition (e.g., "once 15+ languages reach oracle-verified Tier-B") to give consumers a target for stability.

## Test Coverage & Validation Gaps

### Corpus Coverage Uneven Across Languages (Low-Medium Impact)

**Issue:** The evaluation corpus under `eval/corpus/` has varying coverage depth: some languages have rich case diversity, others have minimal fixtures.

- **Files:** `eval/corpus/` subdirectories
- **Pattern:** Oracle cases (`<lang>_oracle/`) have multiple scenarios (e.g., `ts_oracle/` has `ambiguous_call/`, `scoped_call/`, `import_shadowing/`), but some languages (e.g., `ruby_oracle/`) have fewer
- **Impact:** Oracle languages with thin coverage may hide resolution edge cases. A language with only one corpus case may pass all tests but fail on real-world code patterns not in the fixture.
- **Not urgent:** The corpus is growing as extractors improve. Each new language PR is expected to add corpus cases (`CONTRIBUTING.md` line 215).

### No Regression Gate on Precision/Recall Invariants (Low Impact)

**Issue:** The test suite validates that golden and oracle scoring don't regress absolute numbers, but does not enforce Tier-A and Tier-B invariants (recall for A, no false positives for B).

- **Files:** `eval/src/` (scoring logic), `eval/tests/` (test harness)
- **Invariants (claimed, not gated):**
  - Tier-A (SymbolTableResolver): perfect recall (connects all same-named symbols), but high false-positive rate.
  - Tier-B (ScopeGraphResolver): zero false positives (never emits an edge it can't prove), lower recall.
  - Tier-B should beat Tier-A on precision where genuine ambiguity exists.
- **Current gate:** Literal score numbers must not decrease on successive runs (regression test). But no assertion that Tier-B.false_positives == 0 across all languages.
- **Risk:** A buggy resolver change could emit a false positive on language X and still pass the regression gate if it's the first time that language's corpus is being scored.
- **Not blocking:** The eval harness is sound (it scores correctly), but the gate could be stronger.

## Dependency & Ecosystem Risks

### Python Binding Maturin / PyO3 Stability (Low Impact)

**Issue:** The Python binding uses maturin (PyO3 wrapper) and serde for serialization. Both are stable, but breaking changes in major versions could require recompilation.

- **Files:** `bindings/python/Cargo.toml`, `bindings/python/pyproject.toml`
- **Current deps:** PyO3 (automatic, via maturin), serde, serde_json (core deps)
- **Risk:** Low. PyO3 is production-grade (used by projects like Polars). Serde is stable. The risk is upgrade-breaking on maturin version bumps, which are rare.

### Node Binding napi-rs Stability (Low Impact)

**Issue:** The Node binding uses `napi-rs` (Node-API, a stable low-level interface), but version updates could introduce breaking changes.

- **Files:** `bindings/node/Cargo.toml`, `bindings/node/package.json`, `bindings/node/src/lib.rs`
- **Current:** `napi-rs` provides a stable, language-agnostic binding layer. The `#[napi]` macro is the API contract.
- **Risk:** Same as PyO3: low, because the Node-API itself is a C standard. Macro changes would trigger regeneration (caught by the bindings CI gate).

### Grammar Maintenance Dependency (Low Impact)

**Issue:** Each language depends on a third-party tree-sitter grammar maintained outside the code2graph project. Grammars can be abandoned or ship breaking changes without warning.

- **Files:** Each grammar crate in `Cargo.toml` (lines 58–79)
- **Risk:** A grammar maintainer could release a version with a different ABI version or drop support for tree-sitter 0.24–0.26. This breaks the build.
- **Mitigation:** The `abi_versions_are_compatible()` test in `src/grammar.rs` catches ABI mismatches. A broken grammar is caught immediately on `cargo test`.
- **Long-term:** Language support is only as stable as the grammar's maintenance. This is a shared risk across all code-graph extractors.

## Summary of Priorities

**High-Impact:**
1. Bindings drift (already mitigated with CI gate, but process is manual)
2. Tree-sitter version window (will require action as ecosystem advances to 0.27+)

**Medium-Impact:**
1. FFI gap (most boundaries not bridged yet — known frontier)
2. Oracle coverage incomplete (resolution quality unmeasured for 2/3 of Tier-B languages)
3. Language feature gaps (blank cells on supported languages)

**Low-Impact:**
1. Large extractor files (maintainability, not blocking)
2. Grammar dependency risk (mitigated by CI gates)
3. Entry-point detection incomplete (optional feature)
4. No fuzzing (pre-0.1 codebase, lower priority)

---

*Concerns audit: 2026-07-05*
