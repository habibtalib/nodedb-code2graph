# COMPAT-01 / COMPAT-02 Verdicts — All 15 Candidates

**Method:** Every row below is an *empirical* verdict — each candidate's grammar was wired as a
Cargo feature and run against the real `abi_versions_are_compatible` test in `src/grammar.rs`
(checks the compiled parser's `abi_version()` against this repo's resolved ABI window) **and** a
standalone `cargo check --no-default-features --features <lang>` isolation check. No verdict here
is inferred from a crate's declared semver (`tree-sitter = "^0.2x"` dev-dependency) — per D-01/D-03,
that dev-dependency is irrelevant to linking; the real gate is each crate's normal
`tree-sitter-language = "^0.1"` dependency, and the only way to be sure is to run the test.

**Date:** 2026-07-05

**Batches:**
- 01-01 (11 expected-compatible, per STACK.md): Zig, Julia, R, OCaml, Objective-C, Fortran, Groovy,
  PowerShell, SystemVerilog, Astro, F#
- 01-02 (4 disputed, STACK.md-vs-FEATURES.md conflict resolved here): Elixir, Erlang, Gleam, Haskell

| Language | Crate | Version | `tree-sitter-language` req | ABI Result | License |
|----------|-------|---------|------------------------------|------------|---------|
| Zig | tree-sitter-zig | 1.1.2 | ^0.1 | PASS | MIT |
| Julia | tree-sitter-julia | 0.23.1 | ^0.1 | PASS | MIT |
| R | tree-sitter-r | 1.3.0 | ^0.1 | PASS | MIT |
| OCaml | tree-sitter-ocaml | 0.25.0 | ^0.1 | PASS | MIT |
| Objective-C | tree-sitter-objc | 3.0.2 | ^0.1 | PASS | MIT |
| Fortran | tree-sitter-fortran | 0.6.0 | ^0.1 | PASS | MIT |
| Groovy | tree-sitter-groovy | 0.1.2 | ^0.1 | PASS | MIT |
| PowerShell | tree-sitter-powershell | 0.26.4 | ^0.1 | PASS | MIT |
| SystemVerilog | tree-sitter-systemverilog | 0.3.1 | ^0.1 | PASS | MIT |
| Astro | tree-sitter-astro-next | 0.1.1 | ^0.1 | PASS | MIT OR Apache-2.0 |
| F# | tree-sitter-fsharp | 0.3.1 | ^0.1 | PASS | MIT |
| Elixir | tree-sitter-elixir | 0.3.5 | ^0.1 | PASS | Apache-2.0 |
| Erlang | tree-sitter-erlang | 0.19.0 | ^0.1 | PASS | MIT |
| Gleam | tree-sitter-gleam | 1.0.0 | ^0.1 | PASS | Apache-2.0 |
| Haskell | tree-sitter-haskell | 0.23.1 | ^0.1 | PASS | MIT |

## Outcome

All 15 COMPAT-01 candidates pass both the ABI compatibility test
(`cargo test grammar::tests::abi_versions_are_compatible --features <lang>`) and the standalone
feature-isolation check (`cargo check --no-default-features --features <lang>`). Zero failures —
zero COMPAT-02 rows are needed for these 15; no candidate required a revert or an honest
no-usable-grammar note.

The STACK.md-vs-FEATURES.md dispute over Elixir/Erlang/Gleam/Haskell (D-03) is resolved: FEATURES.md
flagged their `tree-sitter = "^0.23"` **dev-dependency** as incompatible, but that dependency is
never linked into this crate — it only affects the disputed crates' own test suites. The actual
load-bearing contract is each crate's normal `tree-sitter-language = "^0.1"` dependency (the type
`src/grammar.rs`'s `.into()` conversion actually consumes), which all four declare, and which all
four's compiled parsers confirm via a passing `abi_versions_are_compatible` run. STACK.md's reading
was correct; FEATURES.md's precondition table conflated a dev-only version pin with the real ABI
gate.

All 15 are wired as grammar-only Cargo features (no `_extractors`) — none in `default` — ready for
the corresponding extractor phase to flip them from 🟠 to 🟢/⭐ once an extractor is written.
