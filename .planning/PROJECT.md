# code2graph — Language Expansion Milestone

## What This Is

code2graph is a purpose-neutral, language-agnostic code-graph extraction library (Rust, tree-sitter based) that turns source files into symbols, references, and cross-file edges as plain data — with Python (PyO3) and Node (napi-rs) binding adapters. This milestone expands the set of supported source languages and strengthens the Python/TypeScript adapter surfaces.

## Core Value

Honest, deterministic structural facts for as many real-world languages as have compatible grammars — extraction depth (real symbols/references), never "the file parses."

## Requirements

### Validated

- ✓ 24 supported languages (Rust, TS/JS, Python, Go, Java, C, C++, Kotlin, Ruby, PHP, Swift, C#, Scala, Dart, Solidity, Lua, Luau, Pascal, Shell, Svelte, SQL, HCL) with Tier-A/Tier-B resolution — existing
- ✓ SCIP-aligned symbol identity, `Confidence`-tagged references, pluggable `Resolver` tiers — existing
- ✓ Eval harness with golden fixtures and SCIP oracles (`eval/corpus/`) scoring precision/recall per language — existing
- ✓ Python bindings (PyO3/maturin, PyPI `code2graph-rs`) and Node bindings (napi-rs, npm `@nodedb-lab/code2graph`) — existing
- ✓ Sync tests guarding `docs/supported-languages.md` ↔ `Language` enum and FFI matrix ↔ `SPECS` registry; CI gate on committed napi bindings drift — existing

### Active

- [ ] New language extractors from the 🟠 planned list, gated on a grammar crate compatible with `tree-sitter >=0.24, <0.27` (candidates: Elixir, Erlang, Gleam, Zig, Julia, R, Haskell, OCaml, Objective-C, Fortran, Groovy, PowerShell, Astro) — research determines the feasible set
- [ ] Each added language follows the full CONTRIBUTING recipe: Cargo feature + grammar dep, `src/grammar.rs` registration + ABI check, `src/lang.rs` variant + extension dispatch, extractor in `src/extract/`, dispatch wiring, unit tests, ≥1 `eval/corpus/` case, `docs/supported-languages.md` row
- [ ] Python and TypeScript/Node binding adapters stay in parity — new `Language` variants exposed through both bindings without drift (committed napi bindings regenerated)
- [ ] TypeScript adapter depth improvement: entry-point detection (`Entry-pts` column is blank for TS/JS — HTTP route markers like `app.get`)
- [ ] Blocked/infeasible languages documented honestly (which crates exist, what tree-sitter version they target) per CONTRIBUTING §"When a language has no usable grammar"

### Out of Scope

- Vue, Liquid, F#, Apex, COBOL — 🔴 blocked: no maintained grammar compatible with the pinned tree-sitter range; revisit when one appears
- Bridging incompatible tree-sitter versions or vendoring grammars — violates the project durability bar (CONTRIBUTING explicitly rejects it)
- HTML/CSS/prose, generic config (JSON/YAML/TOML) as code graphs, binary artifacts — deliberately never supported
- New resolver capabilities beyond what new extractors need — separate effort; this milestone is extraction breadth

## Context

- Brownfield: codebase map in `.planning/codebase/` (STACK, ARCHITECTURE, STRUCTURE, CONVENTIONS, TESTING, CONCERNS, INTEGRATIONS)
- The recipe is mechanical (CONTRIBUTING.md §"Adding a Language"); the resolver is language-agnostic so cross-file edges work once extraction emits correct facts
- Published grammars often differ from repo `node-types.json` — dump the real AST (`to_sexp()`) against the exact crate version before writing an extractor
- Embedded/SFC languages (Astro) follow the Svelte pattern: parse host, run TS extractor on script content, `shift_offsets` back
- `src/extract/support.rs` helpers are mandatory reuse; the freshest structurally-similar extractor is the template
- Pre-0.1: schema may still evolve; every PR runs fmt + clippy -D warnings + full test suite

## Constraints

- **Compatibility**: grammar crates must satisfy `tree-sitter >=0.24, <0.27` — ABI guarded by `abi_versions_are_compatible` in `src/grammar.rs`
- **Tech stack**: Rust edition 2024, MSRV 1.85; grammars come from crates.io only
- **Quality bar**: no `.unwrap()`/`.expect()` in non-test code; typed `thiserror` errors; Tier-B never emits a false positive
- **CI**: sync tests fail the build if docs matrices or committed napi bindings drift from the code
- **License**: Apache-2.0; grammar crate licenses must be compatible

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Gate every candidate language on verified grammar compat before planning it | Version window is the usual blocker; avoids wasted extractor work | — Pending |
| Keep bindings parity a requirement of each language phase, not a trailing phase | Committed napi bindings drift fails CI; drift compounds | — Pending |
| Astro via the embedded-SFC (Svelte) pattern rather than a bespoke extractor | Reuses the TS engine; matches CONTRIBUTING guidance | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd:transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd:complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-07-05 after initialization*
