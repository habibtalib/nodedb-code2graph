# Phase 1: Foundation — Compatibility Gate, CI Hardening & TS/JS Depth - Context

**Gathered:** 2026-07-05
**Status:** Ready for planning
**Mode:** `--auto` (recommended defaults selected; each choice logged in 01-DISCUSSION-LOG.md)

<domain>
## Phase Boundary

Empirically verify all 15 candidate grammar crates against the repo's ABI gate (COMPAT-01), document failures honestly (COMPAT-02), close the feature-flag-isolation CI gap (COMPAT-03), and add entry-point detection to the existing TS/JS extractor (TSADAPT-01). No new language extractors in this phase — those are Phases 2–4.

</domain>

<decisions>
## Implementation Decisions

### Compat-spike mechanics
- **D-01:** Wire candidates **in-repo, incrementally**: for each of the 15 candidates add the optional dep + Cargo feature + `src/grammar.rs` fn + ABI-test arm, then run `cargo test grammar::tests::abi_versions_are_compatible --features <lang>`. Passing wire-ups **stay in the tree** (they are the foundation Phases 2–4 build on); failing ones are reverted with the verdict recorded.
- **D-02:** New candidate features are **NOT added to `default`** until their extractor lands in a later phase. A grammar-only feature (no `Language` enum variant, no dispatch arm) must compile standalone — that's exactly what the new CI check verifies.
- **D-03:** Candidate order: the 11 expected-compatible first (Zig, Julia, R, OCaml, Objective-C, Fortran, Groovy, PowerShell, SystemVerilog, Astro via `tree-sitter-astro-next`, F# via `tree-sitter-fsharp`), then the 4 disputed (Elixir, Erlang, Gleam, Haskell) whose STACK-vs-FEATURES research conflict this gate resolves empirically.

### Verdict recording
- **D-04:** All 15 verdicts land in a phase artifact `.planning/phases/01-foundation-compatibility-gate-ci-hardening-ts-js-depth/01-COMPAT-VERDICTS.md` — one row per candidate: crate, version pinned, tree-sitter/tree-sitter-language requirement found, ABI test result, license.
- **D-05:** Failures additionally get the honest `docs/supported-languages.md` note per CONTRIBUTING §"When a language has no usable grammar" (crate checked, version, exact requirement). Passing candidates keep their 🟠 row until their extractor phase flips them to 🟢.
- **D-06:** Fold in the research's already-verified status corrections: F# moves out of 🔴 blocked (ionide `tree-sitter-fsharp` is compatible); Vue/Apex (tree-sitter ~0.20 as normal dep), Liquid (no crate), COBOL (non-functional crate) get precise blocked-reason notes.

### CI hardening shape
- **D-07:** One CI job that loops `cargo check --no-default-features --features <lang>` over **every** language feature (existing 22 + new candidates), ubuntu-only, using `cargo check` not `test` to keep it cheap. Closes the pre-existing gap, not just the new-language slice.
- **D-08:** Job lives in the existing `.github/workflows/test.yml` alongside the current jobs; a break in any isolated build fails the pipeline.

### TS/JS entry-point design
- **D-09:** Two detection patterns, both proven in-repo: call-terminal verb matching (Express/Fastify/Koa/Hono share the `x.get|post|put|delete|patch|...(path, handler)` shape — mirror Python's `PY_ROUTE_VERBS` approach in `src/extract/python.rs`) and decorator-terminal matching for NestJS `@Get`/`@Post`/`@Put`/`@Delete`/`@Patch`/`@Controller` (mirror Java's annotation detector in `src/extract/java.rs`).
- **D-10:** Emit the neutral `EntryPoint::HttpRoute("<raw marker as written>")` fact from unambiguous syntax only — no type resolution, no framework config parsing, no guessing. No `Main` detection for TS/JS (no unambiguous main construct).
- **D-11:** Implement once in the shared `extract_ecmascript` path so TS, TSX, JS, JSX and Svelte `<script>` blocks all inherit it; fill the `Entry-pts` column for the TypeScript row (JavaScript row shows ⤴ via the TS engine).

### Claude's Discretion
- Exact verb list for call-terminal matching (match Python's list plus `all`/`use` only if unambiguous — err toward precision).
- Whether the CI loop is a shell for-loop in one step or a small matrix — pick whichever keeps test.yml readable.
- Placement/naming of the entry-point helper within the TS extractor.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Recipe & policy
- `CONTRIBUTING.md` §"Adding a Language" — the 6-step recipe; §"When a Language Has No Usable Grammar" — the honest-failure protocol (steps 1–5); §"Validation" — corpus expectations
- `docs/supported-languages.md` — status matrix this phase updates; sync-tested against `src/lang.rs`

### Compat gate
- `src/grammar.rs` — grammar chokepoint + `abi_versions_are_compatible` test (the empirical gate)
- `Cargo.toml` — existing grammar pins and feature-flag pattern to copy
- `.planning/research/STACK.md` — per-candidate crate names, versions, dependency evidence, licenses
- `.planning/research/SUMMARY.md` — STACK/FEATURES conflict resolution protocol

### CI
- `.github/workflows/test.yml` — existing jobs incl. the napi bindings drift gate; where the isolation job lands
- `.planning/research/PITFALLS.md` — CI gaps found (no single-feature builds; bindings job ubuntu-only)

### TS/JS entry-points
- `src/extract/typescript.rs` — `extract_ecmascript` shared path to extend
- `src/extract/python.rs` — `PY_ROUTE_VERBS` call-terminal precedent
- `src/extract/java.rs` — annotation/decorator-terminal precedent
- `docs/supported-languages.md` §Entry-points — the neutral `EntryPoint` fact contract
- `.planning/research/FEATURES.md` — TS/JS entry-point pattern analysis

### Codebase maps
- `.planning/codebase/ARCHITECTURE.md`, `.planning/codebase/STRUCTURE.md`, `.planning/codebase/TESTING.md`, `.planning/codebase/CONVENTIONS.md`

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `abi_versions_are_compatible` test in `src/grammar.rs`: per-feature `check("<lang>", super::<lang>())` arms — the gate is already built; each candidate adds one `#[cfg(feature)]` fn + one arm
- Python route-verb detector and Java annotation detector: the two entry-point patterns to mirror
- `src/extract/support.rs` helpers (marker-walk pattern shared with FFI-export detection)

### Established Patterns
- Feature-per-language with `dep:` optional dependencies; community `-ng`/variant crates accepted (kotlin-ng, sequel, svelte-ng precedent)
- Sync tests: `docs/supported-languages.md` ↔ `Language` enum — grammar-only features (no enum variant) do NOT trip it, so candidate wire-ups are safe pre-extractor
- Bindings are enum-generic; nothing in this phase touches `bindings/` (no new `Language` variants yet)

### Integration Points
- `Cargo.toml` `[features]` + `[dependencies]`; `src/grammar.rs`; `.github/workflows/test.yml`; `src/extract/typescript.rs`; `docs/supported-languages.md`

</code_context>

<specifics>
## Specific Ideas

- The roadmap's success criterion is explicit about method: verdicts must come from actually running the ABI test per feature, never from a crate's declared semver.
- Research warns published grammars often differ from their repo `node-types.json` — irrelevant for this phase (no extractors), critical for Phases 2–4.

</specifics>

<deferred>
## Deferred Ideas

- Bindings CI 3-OS matrix extension (DEPTH-03, v2) — surfaced by pitfalls research; not this phase
- Python-side automated bindings drift gate (asymmetric with napi check) — note for v2
- Corpus backfill for shipped 🟢 languages missing cases (DEPTH-01, v2)

</deferred>

---

*Phase: 01-foundation-compatibility-gate-ci-hardening-ts-js-depth*
*Context gathered: 2026-07-05*
