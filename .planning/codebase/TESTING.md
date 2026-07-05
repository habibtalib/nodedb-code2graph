// SPDX-License-Identifier: Apache-2.0

# Testing Patterns

**Analysis Date:** 2026-07-05

## Test Framework

**Runner:**
- Built-in `cargo test` (standard Rust test harness)
- No external test framework (no nextest, no pytest, no jest)
- Dev-dependency: `serde_json` for test fixtures

**Run Commands:**
```bash
cargo test --workspace                  # Run all tests (library + eval)
cargo test --all-features               # Include all language extractors
cargo test --no-default-features        # Minimal, no extractors (used to test framework separation)
cargo test --features rust              # Single language test
cargo test -p code2graph-eval           # Eval harness only (scoring, corpus validation)
cargo test --lib                        # Library unit tests only
cargo test --doc                        # Doc tests only
```

**Feature-Gated Tests:**
- Each language test is compiled only when its feature is enabled
- The eval crate tests are standalone and always compiled
- Binding tests (Python/Node) are separate from the core

## Test File Organization

**Location:**
- Co-located with production code via `#[cfg(test)] mod tests { ... }`
- Each extractor file (`src/extract/rust.rs`, `src/extract/python.rs`, etc.) contains a `tests` module at the end
- Resolver tests in `src/resolve/<tier>.rs` near their implementations
- Integration tests in `eval/tests/` for cross-language and corpus validation

**Naming:**
- Test functions prefixed with `test_` or named descriptively: `#[test] fn extracts_defs_with_scip_ids() { ... }`
- Test modules follow the feature gate: `#[cfg(test)] mod tests { ... }`

**Structure:**
```
src/extract/rust.rs
├── [production code: RustExtractor, helper functions]
└── #[cfg(test)]
    mod tests {
        use super::*;
        
        #[test]
        fn test_name() { … }
    }
```

## Test Structure

**Setup Pattern:**
Each test creates minimal test source and calls `Extractor::extract()`:
```rust
#[test]
fn extracts_call_references() {
    let src = "pub fn main() { validate_token(\"t\"); helper(); }";
    let facts = RustExtractor.extract(src, "src/main.rs").unwrap();
    // assertions
}
```

**Teardown:**
- None needed; tests are pure functions
- No state persistence between tests
- Each test is independent

**Assertion Pattern:**
- Filter vectors for the aspect being tested: `facts.symbols.iter().filter(...).map(|s| s.name.as_str()).collect()`
- Assert against rendered SCIP strings for identity validation: `assert_eq!(sym.id.to_scip_string(), "codegraph . . . auth/session/validate_token().")`
- Assert enum variants and structural properties: `assert_eq!(sym.kind, SymbolKind::Function)`, `assert_eq!(sym.visibility, Visibility::Public)`

## Test Types

**Unit Tests (Per-Language Extractors):**
- Location: `src/extract/<lang>.rs` in `#[cfg(test)] mod tests`
- Scope: Assert that an extractor correctly identifies definitions, references, and their properties
- Examples from `src/extract/rust.rs`:
  - `extracts_defs_with_scip_ids()` — validates symbol identity rendering
  - `extracts_call_references()` — validates reference capture
  - `trait_impl_emits_inherit_ref_and_inherent_impl_does_not()` — validates role-specific references
  - `import_scoped_identifier_emits_leaf()` — validates import reference names
  - `supertrait_bounds_emit_inherit_refs()` — validates inheritance references

**Scope/Binding Tests:**
- Extractors that emit scopes and bindings (Rust, Python, TypeScript) test lexical-scope capture
- Verify `Reference.scope` is correctly attached to enclosing `Scope`
- Verify `Binding` records match definitions in their scope

**Resolver Tests:**
- Location: `src/resolve/<tier>.rs` (e.g., `src/resolve/symbol_table.rs` has no tests in the code file itself; resolver tests live in eval/)
- Scope: Cross-file edge resolution, confidence labeling, provenance attribution
- Tested via corpus fixtures in `eval/corpus/`

**Integration Tests (Eval):**
- Location: `eval/tests/` and `eval/corpus/`
- Scope: End-to-end extraction + resolution against ground truth
- Data-driven: each `eval/corpus/<lang>_oracle/<case>/` or `eval/corpus/<lang>/<case>/` directory is automatically picked up
- Scored via `cargo run -p code2graph-eval` (prints precision/recall per language and tier)

**Doc Tests:**
- Location: Module-level `///` doc comments
- Example from `src/lib.rs`:
```rust
//! ```
//! use code2graph::{extract_path, resolve::{Resolver, SymbolTableResolver}};
//!
//! let a = extract_path("src/util.rs", "pub fn helper() {}").unwrap();
//! let b = extract_path("src/main.rs", "pub fn run() { helper() }").unwrap();
//! let graph = SymbolTableResolver.resolve(&[a, b]);
//! assert_eq!(graph.edges.len(), 1); // run --calls--> helper
//! ```
```
- Run via `cargo test --doc --all-features`

## Mocking

**No Mocking Framework:**
- code2graph has no dependencies on mock libraries
- Tree-sitter is used directly; no mocking of the parser
- Tests pass pre-built source strings to extractors

**Test Doubles:**
- Minimal test fixtures: 3–10 lines of source code per test
- Example: `"pub fn validate_token(tok: &str) -> bool { helper() }"` tests call references and signatures

**What NOT to Mock:**
- Never mock tree-sitter — the real grammar is essential to test
- Never mock the `Extractor` trait — test real extractors with real source
- Never stub SCIP identifier rendering — always test against real rendered strings

## Fixtures and Factories

**Test Data Location:**
- Corpus fixtures: `eval/corpus/<lang>_oracle/` and `eval/corpus/<lang>/`
- No separate fixture factory crates
- Inline test source directly in test functions (3–20 lines is typical)

**Golden Fixtures:**
- `eval/corpus/<lang>/<case>/expected.edges` — hand-authored ref→def location pairs
- Format: `<ref>:<line> <def>:<line>` (location-only, role-agnostic scoring)
- Used for language extractors that don't have an external oracle

**Oracle Fixtures:**
- `eval/corpus/<lang>_oracle/<case>/index.scip` — committed binary index from external tool
- `eval/corpus/<lang>_oracle/<case>/oracle.edges` — derived text file (location-only ref→def pairs)
- Source files included: `.rs`, `.py`, `.ts`, etc. — the exact code the oracle indexed
- Used to validate Tier-B precision against an independent source of truth

**Adding a Fixture:**
```
eval/corpus/rust_oracle/ambiguous_call/
├── src/
│   ├── lib.rs    # source file with call site
│   └── types.rs  # source file with definitions
├── index.scip    # committed binary from rust-analyzer
└── oracle.edges  # committed text: one location pair per line
```

## Conformance Testing (src/resolve/conformance.rs)

**Purpose:**
The `ConformanceResolver` implements inherited-member recall — when a call qualifies the type (`Foo::bar()`), it resolves unqualified definitions to inherited members up the type hierarchy.

**Test Patterns (in eval corpus):**
- Fixture with inheritance hierarchy (trait/superclass definitions)
- Type-qualified member references
- Assert edges exist to inherited definitions
- Location-only scoring: oracle can't distinguish direct vs. inherited without type knowledge

**Oracle Workflow:**
- External tool (rust-analyzer, scip-java, etc.) produces type-aware reference edges
- code2graph's output is compared location-to-location against the oracle
- A reference and a definition at the same locations match, regardless of how code2graph derived the edge

## Eval Harness (eval/ crate)

**Structure:**
```
eval/
├── Cargo.toml                    # Feature: oracle-regen (optional)
├── src/
│   ├── main.rs                   # Scorecard binary
│   ├── lib.rs                    # Public harness API
│   ├── corpus.rs                 # Case discovery and loading
│   ├── runner.rs                 # Test runner
│   ├── score.rs                  # Precision/recall scoring
│   ├── oracle.rs                 # Oracle index parsing (behind oracle-regen feature)
│   └── bin/gen_oracle.rs         # Oracle regeneration tool (behind oracle-regen feature)
├── corpus/                       # All corpus cases
│   ├── rust/
│   │   ├── simple_call/          # Golden fixture
│   │   └── nested_call/
│   ├── rust_oracle/
│   │   └── scoped_call/          # Oracle fixture
│   ├── python/
│   ├── python_oracle/
│   └── …
└── tests/                        # Integration tests
```

**Running the Harness:**
```bash
cargo run -p code2graph-eval            # Print scorecard (precision/recall per language/tier)
cargo test -p code2graph-eval           # Run regression tests (invariants locked in)
cargo test -p code2graph-eval -- --test  # Verbose test output
```

**Scoring Model:**
- Per-case: extract all symbols/references, resolve via all tiers, compare against oracle
- Precision: (true positives) / (all edges emitted)
- Recall: (true positives) / (all expected edges)
- Invariants locked in tests:
  - Tier-A maintains full recall in name-matching
  - Tier-B never emits false positives (precision = 1.0 when it emits)
  - Tier-B beats Tier-A on precision where genuine ambiguity exists

**Output Example:**
```
code2graph eval scorecard
═══════════════════════════════════════════════════════════════════

LANGUAGE          CASES       TIER-A                  TIER-B
                              Precision Recall       Precision Recall
─────────────────────────────────────────────────────────────────────
rust              10          1.00      1.00         1.00      1.00
python            8           0.95      1.00         0.98      0.95
typescript        6           0.92      1.00         0.97      0.93
…
```

## CI Gates

**GitHub Actions Workflow** (`.github/workflows/test.yml`):

**Job: lint**
- Runs on: ubuntu-latest
- Steps:
  1. `cargo fmt --all --check` — formatting is required
  2. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — no linting warnings
  3. `cargo doc --no-deps --all-features` with `RUSTDOCFLAGS = -D warnings` — doc build must succeed
  4. `cargo test --doc --all-features` — all doc-tests must pass

**Job: test**
- Runs on: ubuntu-latest, macos-latest, windows-latest (3 OSes in parallel)
- Steps:
  1. `cargo test --workspace --all-features --exclude code2graph-py --exclude code2graph-node`
  - Excludes Python/Node bindings (they're built separately via maturin/napi)
  - Tests all Rust extractors and resolvers

**Job: bindings**
- Runs on: ubuntu-latest
- Steps:
  1. Build Python wheel: `maturin build --release -m bindings/python/Cargo.toml`
  2. Build Node addon: `npx napi build --release --platform` in `bindings/node`
  3. **Verify committed napi bindings are in sync:**
     - `napi build` regenerates `bindings/node/index.js` and `index.d.ts`
     - These files are committed (so the published npm package ships with its loader)
     - CI fails if `git diff --exit-code -- index.js index.d.ts` shows differences
     - This enforces: changes to `src/ffi/node_api.rs` must be followed by `npm run build` and commit

**Pre-Commit Checklist (local):**
```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Binding Tests

**Python Binding (`bindings/python/`):**
- Built via `maturin` (Rust → PyO3 extension module)
- Tests: via `pytest` or unittest in Python test suite (location: `bindings/python/tests/` if present)
- CI: `maturin build --release` verifies it compiles

**Node Binding (`bindings/node/`):**
- Built via `napi-rs` (Rust → Node.js native addon)
- Tests: via Jest or Node test runner (location: `bindings/node/tests/` if present)
- CI gates:
  1. `npx napi build --release --platform` compiles the addon
  2. Verifies `index.js` and `index.d.ts` match what's committed — enforces `npm run build` discipline

## Error Cases & Edge Cases

**Extractor Tests:**
- Parse failures: `CodegraphError::Parse` on unparseable input
- Unsupported language: `CodegraphError::UnsupportedLanguage` when language feature disabled
- Invalid tree-sitter query: caught by `abi_versions_are_compatible` test in `src/grammar.rs`

**Resolver Tests:**
- Empty file list: returns empty `CodeGraph`
- Unresolvable references: Tier-A matches by name (may fan out); Tier-B emits nothing if scope doesn't permit
- Circular dependencies: handled correctly (no infinite loops; edges are deterministic)

## Regression Testing

**Corpus Cases as Regression Gates:**
- Each `eval/corpus/<lang>/<case>/expected.edges` is a regression test
- New language: include at least one hand-authored fixture to lock the initial precision/recall
- Resolver improvement: add a fixture exercising the improvement, confirm regression gate locks the gain
- Corpus discovery is automatic: `runner.rs` finds all `*/<case>/expected.edges`

**Example: Adding Tier-B for Rust**
1. Implement `ScopeGraphResolver::resolve()` for Rust
2. Add `eval/corpus/rust_oracle/scoped_call/` with oracle index and cases
3. Run `cargo test -p code2graph-eval` — confirms Tier-B beats Tier-A on precision
4. Commit the corpus case and oracle index
5. Future Tier-B breaks are caught immediately

## Test Coverage

**Target:** 
- No explicit coverage percentage enforced
- Extractors: all symbol kinds, all reference roles, visibility variants
- Resolvers: cross-file edges, ambiguity handling, confidence assignment
- Error paths: tested implicitly (returning `Err` is visible in test output)

## Doc Tests

**When to Use:**
- Public API examples in type/function doc comments
- Examples should be short (5–15 lines) and runnable
- Use `?` operator in doc tests (no explicit `.unwrap()` — let `?` handle it)

**Example from `src/lib.rs`:**
```rust
//! ## Pipeline
//!
//! ```
//! use code2graph::{extract_path, resolve::{Resolver, SymbolTableResolver}};
//!
//! let a = extract_path("src/util.rs", "pub fn helper() {}").unwrap();
//! let b = extract_path("src/main.rs", "pub fn run() { helper() }").unwrap();
//! let graph = SymbolTableResolver.resolve(&[a, b]);
//! assert_eq!(graph.edges.len(), 1); // run --calls--> helper
//! ```
```

**Run:**
```bash
cargo test --doc --all-features
```

## Special Testing Patterns

**AST Dumping (for grammar verification):**
When adding a language or debugging a grammar:
1. Create a throwaway `examples/dump_ast.rs`:
```rust
use code2graph::grammar;
use tree_sitter::Parser;

fn main() {
    let lang = grammar::rust();
    let mut parser = Parser::new();
    parser.set_language(&lang).unwrap();
    let tree = parser.parse("pub fn foo() {}", None).unwrap();
    println!("{}", tree.root_node().to_sexp());
}
```
2. Run: `cargo run --example dump_ast`
3. Verify node kinds and field names against the grammar crate docs
4. Delete the example after verification

**Sync Tests (for docs consistency):**
- Location: `src/ffi/sync_tests.rs` and similar
- Purpose: Ensure docs/supported-languages.md stays in sync with `Language` enum, docs/ffi-support-matrix.md stays in sync with FFI specs
- Prevents doc rot via compile-time assertions

## Bindings Integration Testing

**Python (code2graph-py):**
- Excluded from main `cargo test` (different build process via maturin)
- Wheel built separately in CI
- If tests exist in `bindings/python/tests/`, they run via `pytest` post-build

**Node (code2graph-node):**
- Excluded from main `cargo test` (different build process via napi-rs)
- Built separately in CI: `npx napi build --release --platform`
- Generated files (`index.js`, `index.d.ts`) verified against committed versions
- If tests exist in `bindings/node/tests/`, they run via Jest post-build

---

*Testing analysis: 2026-07-05*
