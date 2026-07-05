# Codebase Structure

**Analysis Date:** 2026-07-05

## Directory Layout

```
nodedb-code2graph/
├── src/                      # Rust library source
│   ├── lib.rs               # Public API re-exports, module layout
│   ├── lang.rs              # Language enum, extension dispatch
│   ├── grammar.rs           # Tree-sitter grammar chokepoint
│   ├── error.rs             # Error types (CodegraphError)
│   ├── extract/             # Language-specific extractors (23 languages)
│   │   ├── mod.rs           # Extractor module layout, feature gating
│   │   ├── dispatch.rs      # Extractor trait, extract_file/extract_path
│   │   ├── support.rs       # Shared extraction toolkit (tree-sitter helpers)
│   │   ├── python.rs        # Python extractor
│   │   ├── typescript.rs    # TypeScript extractor (shared with JS)
│   │   ├── javascript.rs    # JavaScript extractor (thin wrapper)
│   │   ├── rust.rs          # Rust extractor (largest, 116KB)
│   │   ├── go.rs            # Go extractor
│   │   ├── java.rs          # Java extractor
│   │   ├── cpp.rs           # C++ extractor
│   │   ├── c.rs             # C extractor
│   │   ├── [20 more languages...]
│   ├── resolve/             # Resolution / edge linking
│   │   ├── mod.rs           # Resolver exports
│   │   ├── resolver.rs      # Resolver trait definition
│   │   ├── symbol_table.rs  # Tier A: name/scope resolver (fast, broad)
│   │   ├── scope_graph.rs   # Tier B: scope-aware resolver (precise)
│   │   ├── ffi_bridge/      # Cross-language FFI resolver
│   │   │   ├── mod.rs       # FfiBridgeResolver
│   │   │   └── resolver.rs  # FFI resolution logic
│   │   ├── layered.rs       # LayeredResolver: compose multiple resolvers
│   │   ├── incremental/     # Incremental graph updates
│   │   │   ├── mod.rs       # IncrementalGraph, FileSubgraph
│   │   │   ├── store.rs     # Incremental symbol/edge store
│   │   │   ├── subgraph.rs  # Per-file subgraph extraction
│   │   │   └── stitch.rs    # Cross-file edge stitching
│   │   ├── conformance.rs   # Inherited/implemented-member recall resolver
│   │   ├── external.rs      # SCA reachability resolver
│   │   ├── normalized_name.rs # Case-folded name matching resolver
│   │   ├── scope_graph.rs   # Lexical scope binding resolution
│   │   ├── support.rs       # Shared resolver utilities
│   ├── graph/               # Neutral data model
│   │   ├── mod.rs           # Graph module exports
│   │   └── types.rs         # Symbol, Reference, Edge, FileFacts, CodeGraph, etc.
│   ├── symbol/              # SCIP-aligned symbol identity
│   │   ├── mod.rs           # Symbol module exports
│   │   ├── id.rs            # SymbolId (global/local), Package
│   │   ├── descriptor.rs    # Descriptor (Namespace/Type/Term/Method/etc.)
│   │   └── serde_impl.rs    # Serialization support (behind "serde" feature)
│   ├── ffi/                 # FFI specification & binding
│   │   ├── mod.rs           # FFI module, spec registry
│   │   ├── spec.rs          # FFI spec trait, SPECS array
│   │   ├── c.rs             # C ABI spec
│   │   ├── python.rs        # Python ABI spec
│   │   ├── node_api.rs      # Node-API spec
│   │   ├── jni.rs           # Java Native Interface spec
│   │   ├── wasm.rs          # WebAssembly spec
│   │   └── sync_tests.rs    # FFI spec consistency tests
│   ├── package/             # Manifest parsing (behind "manifest" feature)
│   │   └── [manifest loaders for Cargo.toml, package.json, etc.]
│
├── bindings/                # Language-specific bindings
│   ├── python/              # Python binding (PyO3)
│   │   ├── Cargo.toml       # Rust crate for Python binding
│   │   ├── src/lib.rs       # PyO3 module definition
│   │   ├── pyproject.toml   # Python package metadata
│   │   └── [Python wrapper code]
│   ├── node/                # Node.js binding (Node-API / NAPI)
│   │   ├── Cargo.toml       # Rust crate for Node binding
│   │   ├── src/lib.rs       # NAPI module definition
│   │   ├── package.json     # npm package metadata
│   │   ├── index.js         # JavaScript wrapper (load NAPI binary)
│   │   ├── index.d.ts       # TypeScript definitions
│   │   └── build.rs         # Build script
│
├── eval/                    # Evaluation scripts (benchmarks, test data)
│
├── docs/                    # Documentation
│   └── supported-languages.md  # Coverage table (updated per Language enum)
│
├── assets/                  # Project assets (logo, diagrams, etc.)
│
├── Cargo.toml               # Workspace manifest (single crate + bindings)
├── Cargo.lock               # Dependency lock
├── README.md                # Project overview
├── CONTRIBUTING.md          # Contribution guide
├── CODE_OF_CONDUCT.md       # Community guidelines
├── LICENSE                  # Apache-2.0
└── .github/                 # CI/CD workflows
    └── workflows/           # GitHub Actions

.planning/                    # GSD planning documents
└── codebase/                # This directory
    ├── ARCHITECTURE.md      # Architecture patterns & data flow
    └── STRUCTURE.md         # This file
```

## Directory Purposes

**`src/extract/`:**
- Purpose: Language-specific AST walkers (one per language)
- Contains: Per-language extractor modules (23 total)
- Shared helpers: `support.rs` (tree-sitter queries, node text slicing, signature extraction)
- Dispatch: `dispatch.rs` (Extractor trait, extract_file, extract_path)
- Compilation: Each language behind a Cargo feature; disabled languages error at runtime

**`src/resolve/`:**
- Purpose: Multi-tier reference resolution (name matching → scope aware → FFI → conformance)
- Implementations: `SymbolTableResolver`, `ScopeGraphResolver`, `FfiBridgeResolver`, `LayeredResolver`
- Composition: `LayeredResolver` stacks multiple resolvers, confidence-deduplicates edges
- Incremental: `IncrementalGraph` maintains resolved graph, re-extracts only changed files

**`src/symbol/`:**
- Purpose: SCIP-aligned symbol identity for cross-file matching
- SymbolId: Global (scheme + package + lang + descriptors) or Local (file-scoped)
- Descriptor: Namespace `/`, Type `#`, Term `.`, Method `(…).`, etc. (SCIP grammar)
- Rendering: Descriptors join to stable strings; cross-file matching is string equality

**`src/graph/`:**
- Purpose: Neutral structural-fact data model
- Symbol: Definition with span, visibility, entry points, kind
- Reference: Usage with name, role (Call/Import/TypeRef/Read/Write), occurrence, scope
- Edge: Resolved reference (from Symbol → to Symbol, confidence, provenance)
- FileFacts: Per-file extraction output
- CodeGraph: Workspace-level resolved graph

**`src/ffi/`:**
- Purpose: FFI ABI specifications and cross-language linking
- Specs: Per-ABI definition files (c.rs, python.rs, node_api.rs, jni.rs, wasm.rs)
- Bridge: FfiBridgeResolver reads specs and matches exports across language boundaries
- Extractors: Mark FFI exports (Rust `#[no_mangle]`) for consumption by other languages

**`bindings/python/` and `bindings/node/`:**
- Purpose: Language-native API for consuming code2graph
- Python: PyO3 binding; Python users call `code2graph.extract_path(…)`, `resolver.resolve(…)`
- Node: Node-API binding; JavaScript/TypeScript users call `code2graph.extractPath(…)`
- Both wrap the core Rust library and expose the same data types

## Key File Locations

**Entry Points:**

| File | Role |
|------|------|
| `src/lib.rs` | Public library API: re-exports `extract_file`, `extract_path`, `Resolver` trait, all public types |
| `src/extract/dispatch.rs` | `extract_file(lang, source, file)` and `extract_path(file, source)` entry points |
| `src/resolve/mod.rs` | Resolver re-exports; consumer calls `.resolve(&[FileFacts])` on any resolver |
| `bindings/python/src/lib.rs` | Python entry: `PyModule::add_function(m, …)` for PyO3 exports |
| `bindings/node/src/lib.rs` | Node entry: `napi_create_function(…)` for NAPI exports |

**Configuration:**

| File | Purpose |
|------|---------|
| `Cargo.toml` | Workspace manifest; per-language features (rust, python, typescript, …) |
| `src/lang.rs` | Language enum; primary source of truth for supported languages |
| `src/grammar.rs` | Grammar imports; single chokepoint for tree-sitter crates |
| `docs/supported-languages.md` | Coverage table (tested to stay in sync with Language::ALL) |

**Core Logic:**

| File | Purpose |
|------|---------|
| `src/extract/python.rs` | Example extractor; ~64KB |
| `src/extract/typescript.rs` | Shared TS/JS extractor; ~79KB |
| `src/extract/rust.rs` | Most complex extractor; ~116KB |
| `src/extract/support.rs` | Shared extraction toolkit: tree-sitter helpers, scope walking, binding collection |
| `src/resolve/symbol_table.rs` | Tier A resolver: name-table matching, `NameOnly` edges |
| `src/resolve/scope_graph.rs` | Tier B resolver: scope-aware, lexical lookup, `Exact`/`Scoped` edges |
| `src/resolve/layered.rs` | Compose multiple resolvers; edge dedup by confidence |
| `src/symbol/id.rs` | SymbolId: SCIP string rendering, parsing |
| `src/symbol/descriptor.rs` | Descriptor: path element (Namespace/Type/Term/etc.) |
| `src/graph/types.rs` | Data model: Symbol, Reference, Edge, FileFacts, CodeGraph, etc. |

**Testing:**

Tests are co-located in the same files as code (behind `#[cfg(test)]` modules). Test patterns:
- Extension dispatch: `src/lang.rs::tests::extension_dispatch`
- Grammar ABI: `src/grammar.rs::tests::abi_versions_are_compatible`
- Extraction: Each extractor has examples and extraction test functions
- FFI sync: `src/ffi/sync_tests.rs` ensures specs match producer/consumer

## Naming Conventions

**Files:**
- Language extractors: `{language}.rs` (e.g., `python.rs`, `typescript.rs`)
- No underscores in extractor names
- Resolver implementations: `{resolver_type}.rs` (e.g., `symbol_table.rs`, `scope_graph.rs`)
- Support modules: `support.rs` (shared utilities)
- Traits: Named in `mod.rs` or eponymous file (`resolver.rs`)

**Directories:**
- Hierarchical by subsystem: `extract/`, `resolve/`, `symbol/`, `graph/`, `ffi/`, `package/`
- Submodules organized by responsibility: `incremental/`, `ffi_bridge/`

**Rust Items:**
- Extractors: `{Language}Extractor` (e.g., `PythonExtractor`, `TypeScriptExtractor`)
- Resolvers: `{Type}Resolver` (e.g., `SymbolTableResolver`, `ScopeGraphResolver`)
- Data types: Descriptive (e.g., `Symbol`, `Reference`, `Edge`, `FileFacts`)
- Enums: Exhaustive variants, test guards ensure new variants update all match arms

## Where to Add New Code

### Adding a New Language Extractor

**Steps:**

1. **Add Language variant** (`src/lang.rs`):
   - Add enum variant to `Language` (e.g., `Language::NewLang`)
   - Add to `Language::ALL` constant
   - Add extensions in `extensions()` method
   - Add language tag string in `as_str()` method
   - Compiler will force updates to `assert_variant_in_all()` test

2. **Add Cargo feature** (`Cargo.toml`):
   - Add feature: `newlang = ["dep:tree-sitter-newlang", "_extractors"]`
   - Add dependency: `tree-sitter-newlang = { version = "X.Y.Z", optional = true }`

3. **Add grammar function** (`src/grammar.rs`):
   - Add function:
     ```rust
     #[cfg(feature = "newlang")]
     pub fn newlang() -> Language {
         tree_sitter_newlang::LANGUAGE.into()
     }
     ```
   - Add to ABI test `abi_versions_are_compatible()`

4. **Create extractor** (`src/extract/newlang.rs`):
   - Implement `Extractor` trait:
     ```rust
     pub struct NewLangExtractor;
     
     impl Extractor for NewLangExtractor {
         fn lang(&self) -> Language { Language::NewLang }
         fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
             // Parse with crate::grammar::newlang()
             // Walk AST collecting definitions and references
             // Return FileFacts { file, lang, symbols, references, scopes, bindings, ffi_exports }
         }
     }
     ```
   - Reuse helpers from `support.rs`: `make_symbol()`, `push_ref()`, `collect_call_references()`, etc.
   - Define tree-sitter queries for calls, definitions, imports (language-specific)

5. **Wire extractor dispatch** (`src/extract/mod.rs` and `dispatch.rs`):
   - Add conditional import: `#[cfg(feature = "newlang")] pub mod newlang;`
   - Add conditional re-export: `#[cfg(feature = "newlang")] pub use newlang::NewLangExtractor;`
   - Add arm to `extract_file()` match:
     ```rust
     #[cfg(feature = "newlang")]
     Language::NewLang => NewLangExtractor.extract(source, file),
     ```

6. **Update docs** (`docs/supported-languages.md`):
   - Add row for new language with primary extension (tested by `Language::supported_languages_doc_lists_each_primary_extension()`)

7. **Test**:
   - Run: `cargo test --all-features --lib`
   - Test extraction: `cargo nextest run --features newlang`
   - Verify dispatch: `Language::from_extension("newext")` resolves correctly

**File locations:**
- Language enum and extension dispatch: `src/lang.rs`
- Grammar loading: `src/grammar.rs`
- Extractor trait and dispatch: `src/extract/dispatch.rs`
- Language-specific extractor: `src/extract/newlang.rs` (new file)
- Shared extraction toolkit: `src/extract/support.rs` (reuse, don't modify)

### Adding a New Resolver

**Steps:**

1. **Create resolver file** (`src/resolve/{resolver_name}.rs`):
   - Implement `Resolver` trait:
     ```rust
     pub struct {Name}Resolver;
     
     impl Resolver for {Name}Resolver {
         fn resolve(&self, files: &[FileFacts]) -> CodeGraph {
             // Link references to definitions
             // Return CodeGraph { symbols, edges }
         }
     }
     ```

2. **Wire into module** (`src/resolve/mod.rs`):
   - Add module: `pub mod {resolver_name};`
   - Add re-export: `pub use {resolver_name}::{Name}Resolver;`

3. **Optional: Add to default stack** (`src/resolve/layered.rs`):
   - If general-purpose, add to `LayeredResolver::default_dense()` stack
   - Execution order matters: faster/broader first, then precise, then additive

4. **Test**:
   - `cargo test --lib` to verify trait impl and no regressions
   - Add unit tests in the resolver module

**File locations:**
- New resolver: `src/resolve/{name}.rs` (new file)
- Trait: `src/resolve/resolver.rs`
- Composable stacking: `src/resolve/layered.rs`

### Adding FFI Support for a New ABI

**Steps:**

1. **Create ABI spec** (`src/ffi/{abi_name}.rs`):
   - Implement FfiAbiSpec (details in `src/ffi/spec.rs`)
   - Define how Rust exports map to the target ABI

2. **Wire into registry** (`src/ffi/mod.rs`):
   - Add module: `mod {abi_name};`
   - Add to `SPECS` array in `spec.rs`

3. **Update FfiBridgeResolver** (`src/resolve/ffi_bridge/resolver.rs`):
   - Add logic to match producer (Rust) exports to consumer (target language) expectations

**File locations:**
- ABI specs: `src/ffi/{abi_name}.rs` (new file per ABI)
- Spec registry: `src/ffi/spec.rs`
- Bridge resolver: `src/resolve/ffi_bridge/resolver.rs`

### Modifying an Existing Extractor

**When to modify:**
- Fix bugs in reference detection or definition collection
- Add new entry-point markers (HTTP route variants, decorator patterns)
- Improve namespace derivation or signature capture
- Handle edge cases in scope/binding extraction

**Guidelines:**
- Always emit `FileFacts` — never break the contract
- Use helpers from `support.rs` — don't duplicate logic
- Keep under 100KB if possible (largest is Rust at 116KB)
- Add scope/binding support for Tier-B resolution readiness
- No I/O, no storage, no resolution — extraction is pure

**File locations:**
- Extractor core: `src/extract/{lang}.rs`
- Shared helpers: `src/extract/support.rs` (shared by all extractors)

## Special Directories

**`.planning/`:**
- Purpose: GSD (Goal-Scope-Design) planning documents
- Contents: `codebase/` subdirectory with ARCHITECTURE.md, STRUCTURE.md, etc.
- Generated: No; authored by analysis
- Committed: Yes

**`eval/`:**
- Purpose: Benchmark and evaluation scripts
- Contents: Test corpora, performance benchmarks, validation tooling
- Generated: No; hand-authored
- Committed: Yes

**`bindings/`:**
- Purpose: Language-native bindings (Python, Node, etc.)
- Each binding is a separate crate but shares the core Rust library
- Generated: Partially (compiled binaries); source committed
- Committed: Yes (source and build artifacts)

**`.github/workflows/`:**
- Purpose: CI/CD pipelines
- Tests: `cargo fmt --all`, `cargo clippy`, `cargo nextest run --all-features`
- Committed: Yes

---

*Structure analysis: 2026-07-05*
