// SPDX-License-Identifier: Apache-2.0

# Coding Conventions

**Analysis Date:** 2026-07-05

## Naming Patterns

**Files:**
- Language extractors: `<lang>.rs` (e.g., `src/extract/rust.rs`, `src/extract/python.rs`)
- Resolver types: `<tier>.rs` (e.g., `src/resolve/symbol_table.rs`, `src/resolve/scope_graph.rs`)
- Module wiring: `mod.rs` contains only re-exports; logic lives in sibling modules
- Dispatcher files: `dispatch.rs` for trait implementations and routing

**Functions:**
- Snake_case for all functions and methods
- Helper functions in module-private scope (not public API)
- Extractor methods: `collect_<facts>()` for symbol/reference collection
- Trait implementations: Named structurally, e.g., `RustExtractor`, `SymbolTableResolver`

**Variables:**
- Snake_case for local bindings and fields
- Single-letter loop counters acceptable only in tree-sitter tree walks (`i`, `root`, etc.)
- Descriptive names for extracted values: `defs`, `refs`, `symbols`, `references`, `edges`

**Types and Structs:**
- PascalCase for struct and enum names
- Extractors: `<Language>Extractor` (e.g., `RustExtractor`, `JavaExtractor`)
- Resolvers: Named after their resolution strategy (e.g., `SymbolTableResolver`, `ScopeGraphResolver`, `ConformanceResolver`)
- Enums for domain concepts: `SymbolKind`, `RefRole`, `EdgeKind`, `Visibility`, `Confidence`

## Code Style

**Formatting:**
- Standard Rust formatting via `cargo fmt --all` (no custom `.rustfmt.toml`)
- Run before every commit: enforced in CI

**Linting:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — all warnings must be eliminated
- No suppressions of linting rules except where explicitly documented (e.g., `#[allow(dead_code)]` in `src/extract/support.rs` where helpers are conditionally used per language feature)

**Line Length:**
- Standard Rust conventions (no hard limit enforced, but keep functions and types readable)

## Import Organization

**Order:**
1. Crate-internal imports (`use crate::...`)
2. Tree-sitter imports (`use tree_sitter::...`)
3. Standard library imports (`use std::...`)
4. Third-party crate imports (in alphabetical order)

**Path Aliases:**
- No path aliases used in the codebase
- Absolute paths from crate root preferred for clarity

**Module Re-exports:**
- All public types re-exported from `src/lib.rs` for public API
- Per-language extractors conditionally compiled via feature gates (`#[cfg(feature = "rust")]`)
- Grammar access centralized: `src/grammar.rs` is the sole importer of `tree_sitter_*` crates

## Error Handling

**Patterns:**
- No `.unwrap()` or `.expect()` in library code — only in tests and examples
- Typed errors using `thiserror` crate: `CodegraphError` enum in `src/error.rs`
- Error variants for parse failures, unsupported languages, and invalid tree-sitter queries
- Always return `Result<T>` from fallible operations
- Use `?` operator for error propagation
- Use `if let`/`let ... else` for recoverable errors where the error is not fatal
- Test code may unwrap (test failures are acceptable)

**Error Types:**
```rust
#[derive(Debug, thiserror::Error)]
pub enum CodegraphError {
    #[error("unsupported language for `{0}`")]
    UnsupportedLanguage(String),
    
    #[error("parse error in `{path}`")]
    Parse { path: String },
    
    #[error("invalid tree-sitter query for `{lang}`: {msg}")]
    Query { lang: String, msg: String },
}
```

- Never use `Result<T, String>` — use `CodegraphError` with proper variants
- Store only essential error context (never store large objects or source text)

## Logging

**Framework:** No logging dependencies
- Diagnostic output via `eprintln!` in CLI bindings only (not in library core)
- Library core is silent; consumers implement their own logging on top

## Comments

**When to Comment:**
- Explain the WHY, not the WHAT (code structure explains the what)
- Document invariants that aren't obvious from type signatures
- Explain assumptions about tree-sitter grammar structure (e.g., which fields are present on which node kinds)
- Prefix invariant comments with `//` for clarity — e.g., "// Only inline modules emit Module symbols"

**JSDoc/Doc Comments:**
- Use triple-slash `///` for public items and module docs
- Doc comments include examples for public types and trait methods
- Doc comments reference related types via intra-doc links: `[`Symbol`]`, `[`Resolver`]`
- Module-level doc comments (`//!`) at the top of every file explaining the module's role

**Example from `src/extract/rust.rs`:**
```rust
//! Rust extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: ALL top-level items (`fn/struct/enum/trait/type/const/static/mod`)
//! plus `impl` blocks, each tagged with its real [`Visibility`]. …
```

## Function Design

**Size:** 
- Keep functions to ~100 lines or fewer; break large collectors into smaller helpers
- Tree-sitter walks are the exception — collection loops can be longer if they're a single cohesive task
- Examples: `collect_symbols()` in `src/extract/rust.rs` (150 lines), `collect_call_references()` in support

**Parameters:**
- Pass `ExtractCtx` struct to avoid threading 3+ arguments through helpers
- Accept `&[String]` for namespaces rather than `Vec<String>` (borrows)
- Tree-sitter `Node` always by reference (`&Node`)

**Return Values:**
- Return `Vec<Symbol>` for symbol collections (owned vector, consumed by caller)
- Return `Option<String>` for optional text fields
- Always return `Result<T>` for fallible operations (parsing, tree-sitter setup)

## Module Design

**Exports:**
- `mod.rs` contains ONLY module declarations and re-exports
- No logic, no helper functions, no type definitions in `mod.rs`
- Example: `src/extract/mod.rs` declares `pub mod rust`, `pub mod python`, etc., then re-exports `RustExtractor`, `PythonExtractor`

**Barrel Files:**
- `src/lib.rs` is the public API barrel: re-exports all public types and functions
- Per-language extractors re-exported conditionally via feature gates
- Internal modules (e.g., `src/extract/support.rs`) not re-exported

**Feature Gating:**
- Every language feature gates its module: `#[cfg(feature = "rust")]`
- Grammar lookup behind the feature gate
- Extractors disabled by feature have `extract_file()` return `UnsupportedLanguage` at runtime

**Shared Code:**
- `src/extract/support.rs` contains language-agnostic tree-sitter helpers (text slicing, signature building, reference collection)
- Reuse `support::` functions rather than reimplementing: `node_text()`, `child_text()`, `one_line_signature()`, `collect_call_references()`, `push_ref()`, `push_scope()`, etc.
- `src/resolve/support.rs` contains resolver helpers (index building, symbol matching)

## Special Invariants (Enforced in Review)

These are non-negotiable design boundaries — they appear in the review checklist:

**No Storage/I/O:**
- Extractors and resolvers are pure functions — no file I/O, no network, no database
- No side effects beyond return values
- Deterministic: identical input always produces identical output

**No Source Bodies:**
- `Symbol` carries a `ByteSpan` (start/end bytes), never the source text
- Consumers slice what they need from the source they control
- Keeps symbols lightweight and independent of the source representation

**Purpose-Neutral Facts Only:**
- No scoring, ranking, embeddings, or confidence assignment in extractors
- No filtering of symbols based on visibility or naming convention
- Facts are neutral; policy is the consumer's responsibility
- Example: extract ALL definitions (public + private), tag visibility, let consumer filter

**Every Edge Carries Confidence + Provenance:**
- `Confidence` enum: `NameOnly`, `Scoped`, `Exact` — honest about resolution precision
- `Provenance` enum: `NameTable`, `ScopeGraph`, `Conformance`, `FfiBridge` — which analysis derived the edge
- Never emit a false positive; Tier-B never fakes precision
- Recall can be partial (e.g., ambiguous names fan out in Tier-A); precision must be honest

**Module-Root Files Are Wiring Only:**
- `mod.rs` and `lib.rs` contain ONLY module declarations and re-exports
- Never include trait implementations, type definitions, dispatch logic, or utility functions in a root file
- Put logic in named sibling modules and re-export them
- This keeps the module tree legible and the re-export surface small

**Grammars Imported in Exactly One Place:**
- `src/grammar.rs` is the sole importer of every `tree_sitter_*` crate
- No other file imports a grammar directly
- Extractors call `crate::grammar::<lang>()` to fetch the `Language` type
- This centralizes the dependency boundary and makes feature gating clear

## Commit Format

**Conventional Commits** ([https://www.conventionalcommits.org/](https://www.conventionalcommits.org/)):

```
<type>(<scope>): <subject>

<body (optional)>
```

**Types:** `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `chore`

**Common Scopes:** `extract`, `resolve`, `symbol`, `graph`, `lang`, `grammar`, `eval`, `ffi`, `node`, `py`

**Examples from Recent History:**
- `feat(extract): add Scala extractor` — new language support
- `fix(resolve): capture receiver as qualifier on qualified calls` — resolver bug
- `test(eval): add ruby_oracle/ambiguous_call corpus fixture` — new test case
- `docs(symbol): document SCIP descriptor rendering` — documentation
- `ci: fail the gate if committed napi bindings drift from the Rust API` — CI tooling
- `refactor(extract/typescript): adopt shared ExtractCtx + make_symbol` — internal refactor

**Commit Discipline:**
- One logical change per commit (e.g., add a language, fix a resolver edge case, add a corpus fixture)
- Each commit must build standalone — no broken bisect
- Do NOT include generated noise (formatting of untouched files, accidental lock-file changes)
- Language PRs: include the extractor, unit tests, AND at least one corpus fixture in one PR
- Draft PRs welcome for directional feedback before full implementation

## "Adding a Language" Recipe (from CONTRIBUTING.md)

This is the mechanical pattern for new language support:

**1. Add grammar dependency + feature** (`Cargo.toml`):
```toml
[features]
default = [ …, "foo" ]
foo = ["dep:tree-sitter-foo"]

[dependencies]
tree-sitter-foo = { version = "<x.y.z>", optional = true }
```

**2. Register grammar** in `src/grammar.rs` (the chokepoint):
```rust
#[cfg(feature = "foo")]
pub fn foo() -> Language {
    tree_sitter_foo::LANGUAGE.into()
}
// Add to abi_versions_are_compatible test:
#[cfg(feature = "foo")]
check("foo", super::foo());
```

**3. Register language** in `src/lang.rs`:
- Add `Foo` enum variant to `Language`
- Add `"foo"` arm to `as_str()` and `extensions()` methods
- Add extension dispatch in `from_extension()`

**4. Write extractor** in `src/extract/foo.rs`:
- Struct `FooExtractor` implementing `Extractor` trait
- Single tree-sitter walk collecting definitions and references
- Emit SCIP `SymbolId` with proper namespace descriptors
- Tag references with `RefRole` (Call, TypeRef, Import, etc.)
- Reuse `src/extract/support.rs` helpers, don't reinvent text extraction
- Structurally similar existing extractor is your template

**5. Wire it up:**
- `src/extract/mod.rs`: Add `#[cfg(feature = "foo")] pub mod foo;` and re-export `FooExtractor`
- `src/extract/dispatch.rs`: Add `Language::Foo => FooExtractor.extract(source, file)` match arm

**6. Add unit tests** in `foo.rs` (`#[cfg(test)] mod tests`):
- Assert definitions get expected SCIP ids and `SymbolKind`s
- Assert references (including qualifiers on member calls) are captured
- Test the actual rendered SCIP string: `assert_eq!(sym.id.to_scip_string(), "codegraph . . . <expected>")`

**7. Validate with eval corpus:**
- Add at least one `eval/corpus/foo_oracle/<case>/` or `eval/corpus/foo/<case>/` directory
- Include source files, `expected.edges` (hand-authored ref→def pairs), or oracle index for external validation
- Run `cargo run -p code2graph-eval` to confirm the language meets resolution targets

**Before Starting:**
- Check `docs/supported-languages.md` for what's covered (and guard docs against sync tests)
- Verify grammar exists and is compatible with `tree-sitter >=0.24, <0.27`
- Dump the AST for a few test cases: wire the grammar, drop an `examples/` program printing `tree.root_node().to_sexp()`, verify field names match the grammar crate version you depend on
- If no compatible grammar exists, document the blocker in an issue (don't ship a fragile FFI shim)

## Line-Length Guidance

While Rust has no strict line-length limit enforced by the formatter, keep lines readable:
- Tree-sitter query strings may exceed 100 characters (they're self-contained domain-specific syntax)
- Match arms and conditionals: try to keep under 120 characters; break long expressions onto new lines
- Type signatures: break at trait bounds if they exceed ~90 characters
- Function signatures: break parameters onto multiple lines if they exceed ~80 characters

## Use of `unsafe`

- Avoided everywhere in library code
- Tree-sitter bindings use `unsafe` internally; library code does not add more
- Test code may use `unsafe` for transmute-based testing (never in production paths)

## Panic / Early Exit

- No `panic!()`, `unwrap()`, or `expect()` in library code
- All fallible operations return `Result<T>`
- Test assertions may panic (that's the point of tests)

---

*Convention analysis: 2026-07-05*
