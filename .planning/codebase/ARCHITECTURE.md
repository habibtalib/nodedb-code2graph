# Architecture

**Analysis Date:** 2026-07-05

## Pattern Overview

**Overall:** Two-stage extraction-then-resolution pipeline

**Key Characteristics:**
- **Extraction** (per-file, language-specific): Tree-sitter AST walk → `FileFacts` (symbols + references)
- **Resolution** (cross-file, tier-stacked): Link references to definitions → `CodeGraph` (symbols + confidence-tagged edges)
- **Identity** (SCIP-aligned): Symbols use descriptor paths; string equality = cross-file matching
- **Tier seam**: Multiple resolver implementations (recall-first → precision-first) emit the same output shape
- **No storage, no source bodies**: Symbols carry byte spans; consumers slice their own text

## Layers

**Extraction (`src/extract/`):**
- Purpose: Parse a single source file and emit symbol definitions + references in one tree-sitter walk
- Location: `src/extract/` (one module per language, plus `dispatch.rs` and `support.rs`)
- Contains: Language-specific AST visitors; each implements `Extractor` trait
- Depends on: Tree-sitter grammar (loaded via `src/grammar.rs` chokepoint), shared extraction helpers in `support.rs`
- Used by: Public `extract_file()` and `extract_path()` entry points
- Characteristics:
  - Pure and deterministic (no I/O, no storage, no resolution)
  - Each extractor is a struct (`RustExtractor`, `PythonExtractor`, `TypeScriptExtractor`, etc.) with a single `extract()` method
  - Returns `FileFacts`: symbols, references, scopes, bindings, FFI exports
  - No cross-file linking at this stage

**Resolution (`src/resolve/`):**
- Purpose: Link references to definitions across files; emit edges tagged with confidence and provenance
- Location: `src/resolve/` (multiple resolver implementations)
- Contains: Trait `Resolver`, implementations (`SymbolTableResolver`, `ScopeGraphResolver`, `FfiBridgeResolver`, `LayeredResolver`, others)
- Depends on: `FileFacts` from extraction, symbol identity system (`src/symbol/`)
- Used by: Consumers call `resolver.resolve(&[file_facts])` to get `CodeGraph`
- Characteristics:
  - **Tier A (fast, broad):** `SymbolTableResolver` — name/scope matching, all languages, `NameOnly` edges
  - **Tier B (precise):** `ScopeGraphResolver` — lexical-scope + import + qualified-path resolution (Rust/Python/TypeScript), `Exact`/`Scoped` edges
  - **Bridge:** `FfiBridgeResolver` — cross-language FFI links (Rust `#[no_mangle]` → C)
  - **Composable:** `LayeredResolver` stacks multiple resolvers, deduplicates edges by confidence
  - **Default stack:** `SymbolTableResolver` → `ScopeGraphResolver` → `FfiBridgeResolver` → `ConformanceResolver` → `ExternalResolver` → `NormalizedNameResolver`

**Identity (`src/symbol/`):**
- Purpose: SCIP-aligned symbol identity; cross-file matching by string equality
- Location: `src/symbol/` (`id.rs`, `descriptor.rs`)
- Contains: `SymbolId` (global or local), `Descriptor` (Namespace/Type/Term/Method/etc.), `Package`
- Characteristics:
  - Global symbol: scheme + package + language + descriptor path
  - Descriptor path renders to stable, human-readable SCIP string (e.g., `codegraph . . rust std/slice#.(len).`)
  - Local symbols (parameters, locals) scoped to file
  - Language tag carried per-symbol

**Graph Data Model (`src/graph/`):**
- Purpose: Neutral structural facts — no storage opinion, no embeddings
- Location: `src/graph/types.rs`
- Contains:
  - `Symbol` — definition: name, kind (Function/Method/Class/etc.), visibility, entry points, span
  - `Reference` — usage: name, occurrence, role (Call/Import/TypeRef/Read/Write), scope, qualifier
  - `Edge` — resolved: from symbol, to symbol, role, confidence, provenance, occurrence
  - `FileFacts` — per-file: symbols, references, scopes, bindings
  - `CodeGraph` — per-workspace: symbols, edges
  - `Binding` — name introduced in scope: kind (Local/Param/Import/Definition), target (Local/Import/Def)
  - `Scope` — lexical region: kind (Module/Function/Block/Type), parent, span

**Dispatch & Grammar (`src/lang.rs`, `src/grammar.rs`):**
- Purpose: Single source of truth for language coverage and tree-sitter grammar loading
- Location: `src/lang.rs` (Language enum), `src/grammar.rs` (grammar functions)
- Contains:
  - 23 language variants with extension mappings
  - Grammar loading functions (one per language, feature-gated)
- Characteristics:
  - `Language::from_path()` and `Language::from_extension()` for dispatch
  - `Language::ALL` array for iteration
  - Grammar chokepoint: only `src/grammar.rs` imports tree-sitter grammar crates

## Data Flow

**End-to-end pipeline:**

```
Source file
    ↓
[Extraction] src/extract/{lang}.rs + support.rs
    ↓ extract_file(lang, source, file) or extract_path(file, source)
FileFacts {
  file, lang, symbols, references, scopes, bindings, ffi_exports
}
    ↓
[Resolution] src/resolve/{resolver_impl}.rs
    ↓ resolver.resolve(&[FileFacts, …])
CodeGraph {
  symbols, edges
}
```

**For a call-reference flow (example):**

1. **Extract stage** (Python example):
   - File `src/auth/jwt.py` contains `def validate(token): …` and calls `helper()`
   - Python extractor walks tree-sitter AST
   - Emits `Symbol(id=validate, kind=Function, file=src/auth/jwt.py, …)`
   - Emits `Reference(name=helper, role=Call, occ=(file, line, col, byte), …)`
   - Both carry descriptors via namespace derivation (`auth/jwt/`)

2. **Resolve stage**:
   - `SymbolTableResolver` scans all symbols for leaf name `helper`
   - Finds `Symbol(id=helper, …)` in same or different file
   - Emits `Edge(from=validate, to=helper, role=Call, confidence=NameOnly/Scoped, …)`
   - `ScopeGraphResolver` optionally narrows via scope/import facts to `Exact` confidence

**State management:**
- **No cross-file state mutation:** Each resolver is pure
- **Extraction is isolated:** One file's facts don't affect another's
- **Incremental updates:** `IncrementalGraph` re-extracts changed files and stitches edges on demand

## Key Abstractions

**Extractor Trait (`src/extract/dispatch.rs`):**
- Purpose: Per-language source-to-facts transformation
- Pattern: Struct (e.g., `PythonExtractor`) implementing `fn extract(&self, source: &str, file: &str) -> Result<FileFacts>`
- Examples: `src/extract/python.rs`, `src/extract/typescript.rs`, `src/extract/rust.rs`
- Responsibilities:
  - Parse with tree-sitter grammar
  - Walk AST collecting definitions and references
  - Derive qualified identities from file path (namespaces)
  - Mark entry points (HTTP routes, `main`, decorators)
  - Return neutral `FileFacts` (no storage, no bodies)

**Resolver Trait (`src/resolve/resolver.rs`):**
- Purpose: Cross-file reference linking
- Pattern: Struct implementing `fn resolve(&self, files: &[FileFacts]) -> CodeGraph`
- Examples: `SymbolTableResolver`, `ScopeGraphResolver`, `FfiBridgeResolver`, `LayeredResolver`
- Responsibilities:
  - Accept per-file facts
  - Link references to symbols
  - Tag edges with confidence level
  - Return unified `CodeGraph`

**Language Dispatch:**
- `Language` enum in `src/lang.rs` — 23 variants, one per supported language
- `Language::from_path(file)` infers language from extension
- `extract_file(lang, source, file)` dispatches to language-specific extractor
- Feature gating: Each language behind a Cargo feature (e.g., `python`, `rust`, `typescript`)

**Entry Points (`src/graph/types.rs::EntryPoint`):**
- `Main` — language entry point (function/method named `main`, or Python module guard)
- `HttpRoute(String)` — HTTP framework marker (raw identifier from decorator, e.g., `"app.route"`)
- Emitted by extractors when syntax unambiguously present; consumer decides policy

## Entry Points

**Public library API (`src/lib.rs`):**
- `extract_file(lang, source, file) -> Result<FileFacts>` — extract with explicit language
- `extract_path(file, source) -> Result<FileFacts>` — extract inferring language from extension
- `Resolver` trait — call `.resolve(&[FileFacts])` on any resolver instance
- Built-in resolvers: `SymbolTableResolver`, `ScopeGraphResolver`, `FfiBridgeResolver`, `LayeredResolver`, etc.
- Example:
  ```rust
  let facts = extract_path("src/main.rs", source)?;
  let graph = SymbolTableResolver.resolve(&[facts]);
  ```

**FFI entry points (`src/ffi/`, `bindings/`):**
- Python: `bindings/python/src/lib.rs` — PyO3 wrapper
- Node: `bindings/node/src/lib.rs` — Node-API wrapper
- C: `src/ffi/c.rs` — raw C ABI (Extern-C functions)

## Error Handling

**Strategy:** Typed errors via `thiserror`

**Patterns:**
- `CodegraphError::Parse { path }` — tree-sitter parse failure
- `CodegraphError::UnsupportedLanguage(…)` — language not available (feature disabled)
- All public APIs return `Result<T> = std::result::Result<T, CodegraphError>`
- No panics in extraction/resolution (all preconditions validated or handle gracefully)

**Location:** `src/error.rs`

## Cross-Cutting Concerns

**Logging:** Not implemented. Errors propagate; consumers log as needed.

**Validation:** 
- Language features checked at compile-time (Cargo features)
- Grammar ABI version checked at runtime (`src/grammar.rs::tests::abi_versions_are_compatible`)

**Authentication:** Not applicable (no I/O, no network).

**Entry Point Detection:**
- Syntactic markers only — name-based (`main`), decorator-based (HTTP routes)
- No framework reflection or conventions
- Extractors emit markers; consumers apply policy

**FFI Specification:**
- Per-ABI spec files in `src/ffi/` (`c.rs`, `python.rs`, `node_api.rs`, etc.)
- `FfiBridgeResolver` reads specs and matches across language boundaries
- Today: Rust `#[no_mangle]` → C ABI consumers

---

*Architecture analysis: 2026-07-05*
