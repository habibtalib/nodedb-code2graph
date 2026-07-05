# Requirements: code2graph — Language Expansion Milestone

**Defined:** 2026-07-05
**Core Value:** Honest, deterministic structural facts for as many real-world languages as have compatible grammars — extraction depth, never "the file parses."

## v1 Requirements

Requirements for this milestone. Each maps to roadmap phases.

### Compatibility Gate (COMPAT)

- [x] **COMPAT-01**: Every candidate grammar crate is empirically gated before extractor work — added as an optional dep, compiled, and passing `abi_versions_are_compatible` in `src/grammar.rs` (candidates: Zig, Julia, R, OCaml, Objective-C, Fortran, Groovy, PowerShell, SystemVerilog, Astro via `tree-sitter-astro-next`, F# via `tree-sitter-fsharp`, plus recheck of Elixir, Erlang, Gleam, Haskell)
- [x] **COMPAT-02**: Candidates that fail the gate are documented honestly in `docs/supported-languages.md` (crate checked, version, exact tree-sitter requirement found) per CONTRIBUTING §"When a language has no usable grammar"
- [x] **COMPAT-03**: CI builds each newly added language as a standalone feature (`--no-default-features --features <lang>`) so feature-flag combinatorics can't silently break

### New Language Extractors (LANG)

Each language that passes COMPAT-01 follows the full CONTRIBUTING recipe: Cargo feature + grammar dep, `src/grammar.rs` registration + ABI arm, `src/lang.rs` variant + extension dispatch, extractor in `src/extract/<lang>.rs` (reusing `support.rs` helpers), `mod.rs`/`dispatch.rs` wiring, unit tests asserting real SCIP ids, ≥1 `eval/corpus/` case, and a `docs/supported-languages.md` row (sync-test guarded) — emitting at minimum symbols + calls + imports (table stakes per research FEATURES.md), with honest ceilings (macros/dynamic features stay unresolved rather than guessed).

- [ ] **LANG-01**: Zig extractor (template: C/Rust)
- [ ] **LANG-02**: Julia extractor (template: Lua; multiple-dispatch ceiling documented)
- [ ] **LANG-03**: R extractor (template: Lua; NSE/eval ceiling documented)
- [ ] **LANG-04**: OCaml extractor (template: Rust + C header/impl split for `.ml`/`.mli`)
- [ ] **LANG-05**: Objective-C extractor (template: C + Swift; `.h` collision with C dispatch resolved by explicit documented decision)
- [ ] **LANG-06**: Fortran extractor (template: Pascal/Go)
- [ ] **LANG-07**: Groovy extractor (template: Java/Kotlin; `.gradle` scoping decided explicitly)
- [x] **LANG-08**: PowerShell extractor (template: Shell)
- [ ] **LANG-09**: SystemVerilog extractor (template: C)
- [x] **LANG-10**: Astro extractor via embedded-SFC pattern (template: Svelte; `astro` feature transitively enables `typescript`)
- [ ] **LANG-11**: F# extractor via `tree-sitter-fsharp` (ionide) — newly unblocked; move out of blocked list
- [ ] **LANG-12**: Elixir, Erlang, Gleam, Haskell — implement those that pass the COMPAT-01 recheck; document any that fail (STACK vs FEATURES research disagreement resolved empirically)

### TypeScript Adapter Depth (TSADAPT)

- [x] **TSADAPT-01**: Entry-point detection for TS/JS via the shared `extract_ecmascript` path — call-terminal matching for Express/Fastify/Koa/Hono verb calls (Python `PY_ROUTE_VERBS` precedent) and decorator-terminal matching for NestJS `@Get`/`@Post`/`@Controller` (Java annotation precedent); `Entry-pts` column filled for TS and JS

### Bindings Parity (BIND)

- [x] **BIND-01**: Every new language feature string added to `bindings/node/Cargo.toml` and `bindings/python/Cargo.toml` explicit feature lists in the same change that adds the language (the one unguarded integration point)
- [x] **BIND-02**: Committed napi artifacts (`index.js`/`index.d.ts`) regenerated and verified drift-free (`npx napi build --release --platform`, expected no-op diff) before each language phase completes

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Depth & Quality

- **DEPTH-01**: Corpus cases backfilled for already-shipped 🟢 languages missing them (C#, Dart, Lua, Luau, Pascal, Scala, Svelte)
- **DEPTH-02**: Tier-B scopes/bindings upgrades for the new languages beyond table-stakes extraction
- **DEPTH-03**: Bindings CI job extended to the 3-OS matrix (scanner-heavy grammars risk platform-specific build breaks)
- **DEPTH-04**: FFI ABI spec for Objective-C (`src/ffi/` SPECS registry entry)

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Vue, Apex | Grammar crates pin `tree-sitter ~0.20` as a normal dependency — old incompatible `Language` type, unmaintained since 2022 |
| Liquid | No crate exists on crates.io under any name/variant (verified) |
| COBOL | Only crate (`tree-sitter-cobol 0.1.0`) declares zero dependencies, no repository — not a functioning grammar integration |
| Bridging incompatible tree-sitter versions / vendoring grammars | Violates project durability bar; explicitly rejected by CONTRIBUTING |
| New resolver capabilities (arity-based/multiple-dispatch Tier-B resolution) | Separate effort; this milestone is extraction breadth |
| HTML/CSS/prose, generic config as code graphs, binary artifacts | Deliberately never supported |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| COMPAT-01 | Phase 1 | Complete |
| COMPAT-02 | Phase 1 | Complete |
| COMPAT-03 | Phase 1 | Complete |
| TSADAPT-01 | Phase 1 | Complete |
| LANG-08 | Phase 2 | Complete |
| LANG-10 | Phase 2 | Complete |
| BIND-01 | Phase 2 | Complete |
| BIND-02 | Phase 2 | Complete |
| LANG-01 | Phase 3 | Pending |
| LANG-05 | Phase 3 | Pending |
| LANG-06 | Phase 3 | Pending |
| LANG-07 | Phase 3 | Pending |
| LANG-09 | Phase 3 | Pending |
| LANG-02 | Phase 4 | Pending |
| LANG-03 | Phase 4 | Pending |
| LANG-04 | Phase 4 | Pending |
| LANG-11 | Phase 4 | Pending |
| LANG-12 | Phase 4 | Pending |

**Coverage:**
- v1 requirements: 18 total
- Mapped to phases: 18
- Unmapped: 0 ✓

---
*Requirements defined: 2026-07-05*
*Last updated: 2026-07-05 after roadmap creation*
