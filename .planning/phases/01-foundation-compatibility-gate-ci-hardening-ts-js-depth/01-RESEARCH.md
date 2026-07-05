# Phase 1: Foundation — Compatibility Gate, CI Hardening & TS/JS Depth - Research

**Researched:** 2026-07-05
**Domain:** Cargo feature-flag/tree-sitter grammar wiring (Rust), GitHub Actions CI, tree-sitter TS/TSX AST walking
**Confidence:** HIGH — every claim below was either read directly from this repo's source, or reproduced empirically with throwaway spikes (`cargo check`, `cargo test`, an AST-dump example) run against the real crates, then reverted. No training-data guessing was needed; this is a self-contained Rust monorepo with no external services.

## Summary

This phase has three independent workstreams (COMPAT-01/02/03 and TSADAPT-01), and the codebase already contains a **verbatim recipe** for two of the three (grammar wiring from `CONTRIBUTING.md` + `src/grammar.rs`; entry-point detection from `python.rs`/`java.rs`). The mechanical parts are genuinely mechanical. But direct experimentation surfaced **two concrete, previously-undocumented defects** that the plan must account for, not discover mid-execution:

1. **`src/grammar.rs`'s top-level `use tree_sitter::Language;` import is gated `#[cfg(feature = "_extractors")]`.** Every *existing* language feature enables `_extractors` itself, so this has never mattered. But D-02 requires the 15 new candidate features to be grammar-only (**no** `_extractors`, since they add no extractor/dispatch). Verified by spike: with the import gated as-is, `cargo check --no-default-features --features <any-new-candidate>` **fails to compile** (`error[E0425]: cannot find type Language in this scope`), for every single one of the 15 candidates, unconditionally. **Fix:** make the import unconditional (`tree_sitter::Language` is a mandatory, non-optional dependency of this crate, so there is no cost). Verified by spike: with the import unconditional, a grammar-only feature (no `_extractors`, no `Language` enum variant, no dispatch arm) compiles clean standalone and the ABI test passes. **This one-line fix must land before wiring candidate #1, not be discovered while wiring candidate #7.**
2. **The existing `luau` feature already fails feature isolation today, on `main`, before this phase touches anything.** `cargo check --no-default-features --features luau` currently fails (`error[E0432]: unresolved import super::lua`) because `src/extract/luau.rs` imports `super::lua::extract_lua_family`, but `Cargo.toml`'s `luau = ["dep:tree-sitter-luau", "_extractors"]` never declares a dependency on the `lua` feature. This is exactly the pre-existing gap COMPAT-03's new CI job (D-07/D-08) is designed to catch — and it *will* immediately catch it, turning the new job red on day one unless the plan includes fixing `luau`'s feature declaration (`luau = ["dep:tree-sitter-luau", "lua", "_extractors"]`) in the same change that adds the job.
3. **The TypeScript extractor does not emit per-method `Symbol`s for class members at all** — `collect_symbols` in `typescript.rs` only walks *top-level* declarations (`function_declaration`, `class_declaration`, `interface_declaration`, `type_alias_declaration`, `enum_declaration`, `lexical_declaration`/`variable_declaration`); it never descends into a `class_body` to emit one `Symbol` per `method_definition`, unlike `java.rs` (which does emit method-level symbols and attaches `entry_points_for_java` to them). Verified by an AST dump (`to_sexp()`) of a NestJS-shaped snippet: TS/TSX decorators on class methods (`@Get(':id')`) are real, individually-attachable nodes (a `decorator:` field on `class_body`, immediately preceding the `method_definition` it modifies) — the AST fully supports per-method decorator detection — **but there is currently no method-level `Symbol` in this extractor to attach the resulting `EntryPoint::HttpRoute` fact to.** This is the single biggest scope/architecture question the planner must resolve explicitly (see Open Questions below); it was not caught by CONTEXT.md, STACK.md, PITFALLS.md, or FEATURES.md, all of which assume TS "mirrors Java's annotation detector" without checking whether TS already has a method-level attachment point (it doesn't).

Everything else — the 15 grammar wire-ups, the CI isolation loop, the call-terminal Express/Fastify/Koa/Hono detection, the docs/verdicts artifacts — is low-risk, mechanical, and has a proven in-repo template.

**Primary recommendation:** Sequence Phase 1 as (a) one prep commit fixing the `_extractors` import gate + the pre-existing `luau` isolation bug, (b) 15 candidate wire-up commits each verified with *both* the additive ABI test *and* a local `--no-default-features` isolation check (not deferred to the CI job), (c) the CI isolation-loop job, (d) TS/JS entry-points — with an explicit, plan-level decision on the class-method-symbol question before writing any TS/JS code.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Compat-spike mechanics**
- D-01: Wire candidates in-repo, incrementally: for each of the 15 candidates add the optional dep + Cargo feature + `src/grammar.rs` fn + ABI-test arm, then run `cargo test grammar::tests::abi_versions_are_compatible --features <lang>`. Passing wire-ups stay in the tree; failing ones are reverted with the verdict recorded.
- D-02: New candidate features are **NOT** added to `default` until their extractor lands in a later phase. A grammar-only feature (no `Language` enum variant, no dispatch arm) must compile standalone — that's exactly what the new CI check verifies.
- D-03: Candidate order: the 11 expected-compatible first (Zig, Julia, R, OCaml, Objective-C, Fortran, Groovy, PowerShell, SystemVerilog, Astro via `tree-sitter-astro-next`, F# via `tree-sitter-fsharp`), then the 4 disputed (Elixir, Erlang, Gleam, Haskell) whose STACK-vs-FEATURES research conflict this gate resolves empirically.

**Verdict recording**
- D-04: All 15 verdicts land in a phase artifact `.planning/phases/01-foundation-compatibility-gate-ci-hardening-ts-js-depth/01-COMPAT-VERDICTS.md` — one row per candidate: crate, version pinned, tree-sitter/tree-sitter-language requirement found, ABI test result, license.
- D-05: Failures additionally get the honest `docs/supported-languages.md` note per CONTRIBUTING §"When a language has no usable grammar" (crate checked, version, exact requirement). Passing candidates keep their 🟠 row until their extractor phase flips them to 🟢.
- D-06: Fold in the research's already-verified status corrections: F# moves out of 🔴 blocked (ionide `tree-sitter-fsharp` is compatible); Vue/Apex (tree-sitter ~0.20 as normal dep), Liquid (no crate), COBOL (non-functional crate) get precise blocked-reason notes.

**CI hardening shape**
- D-07: One CI job that loops `cargo check --no-default-features --features <lang>` over **every** language feature (existing 22 + new candidates), ubuntu-only, using `cargo check` not `test` to keep it cheap. Closes the pre-existing gap, not just the new-language slice.
- D-08: Job lives in the existing `.github/workflows/test.yml` alongside the current jobs; a break in any isolated build fails the pipeline.

**TS/JS entry-point design**
- D-09: Two detection patterns, both proven in-repo: call-terminal verb matching (Express/Fastify/Koa/Hono share the `x.get|post|put|delete|patch|...(path, handler)` shape — mirror Python's `PY_ROUTE_VERBS` approach in `src/extract/python.rs`) and decorator-terminal matching for NestJS `@Get`/`@Post`/`@Put`/`@Delete`/`@Patch`/`@Controller` (mirror Java's annotation detector in `src/extract/java.rs`).
- D-10: Emit the neutral `EntryPoint::HttpRoute("<raw marker as written>")` fact from unambiguous syntax only — no type resolution, no framework config parsing, no guessing. No `Main` detection for TS/JS (no unambiguous main construct).
- D-11: Implement once in the shared `extract_ecmascript` path so TS, TSX, JS, JSX and Svelte `<script>` blocks all inherit it; fill the `Entry-pts` column for the TypeScript row (JavaScript row shows ⤴ via the TS engine).

### Claude's Discretion
- Exact verb list for call-terminal matching (match Python's list plus `all`/`use` only if unambiguous — err toward precision).
- Whether the CI loop is a shell for-loop in one step or a small matrix — pick whichever keeps test.yml readable.
- Placement/naming of the entry-point helper within the TS extractor.

### Deferred Ideas (OUT OF SCOPE)
- Bindings CI 3-OS matrix extension (DEPTH-03, v2) — surfaced by pitfalls research; not this phase.
- Python-side automated bindings drift gate (asymmetric with napi check) — note for v2.
- Corpus backfill for shipped 🟢 languages missing cases (DEPTH-01, v2).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| COMPAT-01 | Every candidate grammar crate empirically gated via `abi_versions_are_compatible` | Exact `src/grammar.rs` wire-up template below (Architecture Patterns §1); STACK.md's 15 verified crate/version pins (Standard Stack table); the `_extractors` import-gate fix required before *any* candidate can pass isolation (Common Pitfalls §1) |
| COMPAT-02 | Failures documented honestly in `docs/supported-languages.md` per CONTRIBUTING §"no usable grammar" | Confirmed all 15 candidates are compatible per STACK.md (verified same-day against crates.io dependency manifests) — expect zero COMPAT-02 rows for the 11 non-disputed candidates; the 4 disputed (Elixir/Erlang/Gleam/Haskell) must be *re-verified empirically*, not assumed, since FEATURES.md (same day) found them still on `tree-sitter ^0.23` while STACK.md found `tree-sitter-language ^0.1` compatible dependency structure for all — the ABI test is the actual tie-breaker, not either doc |
| COMPAT-03 | CI builds each language as a standalone feature | Exact `.github/workflows/test.yml` structure below (Architecture Patterns §3); the pre-existing `luau` isolation bug this job will immediately surface (Common Pitfalls §2) |
| TSADAPT-01 | Entry-point detection for TS/JS via shared `extract_ecmascript` path | Verbatim Python/Java precedent (Code Examples); verified AST shape for both Express-call and NestJS-decorator forms via `to_sexp()` spike (Architecture Patterns §4); the critical class-method-symbol architecture gap (Open Questions §1) |
</phase_requirements>

## Standard Stack

### Core — the 15 candidate grammar crates (STACK.md verdicts, crates.io-verified 2026-07-05)

All 15 declare `tree-sitter-language = "^0.1"` as a normal dependency (the actual load-bearing compat contract — see `src/grammar.rs`'s `.into()` conversion) and their own `tree-sitter = "^0.2x"` only as a dev-dependency (irrelevant to linking). All fall inside this repo's resolved ABI window `[13, 15]` (`tree-sitter 0.26.9`).

| Language | Crate | Version to pin | License | Verdict (STACK.md) |
|---|---|---|---|---|
| Zig | `tree-sitter-zig` | `1.1.2` | MIT | COMPATIBLE |
| Julia | `tree-sitter-julia` | `0.23.1` | MIT | COMPATIBLE |
| R | `tree-sitter-r` | `1.3.0` | MIT | COMPATIBLE |
| OCaml | `tree-sitter-ocaml` | `0.25.0` | MIT | COMPATIBLE |
| Objective-C | `tree-sitter-objc` | `3.0.2` | MIT | COMPATIBLE |
| Fortran | `tree-sitter-fortran` | `0.6.0` | MIT | COMPATIBLE |
| Groovy | `tree-sitter-groovy` | `0.1.2` | MIT | COMPATIBLE (young crate — v0.1.2) |
| PowerShell | `tree-sitter-powershell` | `0.26.4` | MIT | COMPATIBLE |
| SystemVerilog | `tree-sitter-systemverilog` | `0.3.1` | MIT | COMPATIBLE |
| Astro | `tree-sitter-astro-next` | `0.1.1` | MIT OR Apache-2.0 | COMPATIBLE, MEDIUM confidence (single release, ~5 months old, independent maintainer — not the `withastro` org) |
| F# | `tree-sitter-fsharp` | `0.3.1` | MIT | **UNBLOCKED** — `ionide` org, most recently updated of the whole batch (2026-07-01) |
| Elixir | `tree-sitter-elixir` | `0.3.5` | Apache-2.0 | COMPATIBLE per STACK.md (disputed vs. FEATURES.md — re-verify empirically, see below) |
| Erlang | `tree-sitter-erlang` | `0.19.0` | MIT | COMPATIBLE per STACK.md (disputed — re-verify) |
| Gleam | `tree-sitter-gleam` | `1.0.0` | Apache-2.0 | COMPATIBLE per STACK.md (disputed — re-verify) |
| Haskell | `tree-sitter-haskell` | `0.23.1` | MIT | COMPATIBLE per STACK.md (disputed — re-verify) |

**On the STACK.md vs. FEATURES.md disagreement for Elixir/Erlang/Gleam/Haskell:** FEATURES.md's precondition table (also written 2026-07-05) says these four are "NOT compatible" because their *declared* `tree-sitter` dependency is `^0.23` — but that's the **dev-dependency**, which STACK.md correctly identifies as irrelevant to linking (the real gate is `tree-sitter-language ^0.1`, a normal dependency all four declare). This is exactly why D-03/COMPAT-01 treats them as "disputed, resolve empirically": run the actual `abi_versions_are_compatible` test per CONTRIBUTING's own warning ("never treat 'crate depends on tree-sitter 0.2x' as sufficient evidence") — do not resolve this dispute by re-reading either doc, resolve it by running the test.

**Installation pattern** (repeat per candidate, grammar-only — no `_extractors`):
```toml
[dependencies]
tree-sitter-zig = { version = "1.1.2", optional = true }

[features]
zig = ["dep:tree-sitter-zig"]
```

**Version verification:** STACK.md's versions were pulled directly from the crates.io API the same day this research ran; treat them as current. If the plan executes more than a few days after 2026-07-05, re-check with `cargo info tree-sitter-<lang>` (or `cargo add tree-sitter-<lang> --dry-run`) before pinning, since these are independently-maintained crates that can publish a new (potentially ABI-incompatible) version at any time — this is precisely the risk class PITFALLS.md's Pitfall 1 describes.

### Feature naming

No fixed convention exists in this repo (`csharp` ≠ crate suffix `c-sharp`; `sql` → crate `tree-sitter-sequel`; `shell` → crate `tree-sitter-bash`) — feature names are canonical language tags, not crate-name mirrors. Recommended candidate tags (Claude's discretion, but keep them boring/predictable): `zig`, `julia`, `r`, `ocaml`, `objc`, `fortran`, `groovy`, `powershell`, `systemverilog`, `astro`, `fsharp`, `elixir`, `erlang`, `gleam`, `haskell`.

## Architecture Patterns

### 1. Exact `src/grammar.rs` wire-up for one candidate — the empirical gate

Current shape (verbatim, `src/grammar.rs`):
```rust
#[cfg(feature = "_extractors")]
use tree_sitter::Language;   // ← MUST become unconditional before wiring candidates (see Pitfall 1)

#[cfg(feature = "lua")]
/// Returns the tree-sitter grammar for Lua.
pub fn lua() -> Language {
    tree_sitter_lua::LANGUAGE.into()
}
```//
And the ABI test (bottom of the same file, inside `#[cfg(test)] mod tests`):
```rust
fn check(name: &str, lang: tree_sitter::Language) {   // note: fully-qualified, NOT affected by the import bug
    let v = lang.abi_version();
    assert!(
        (MIN_COMPATIBLE_LANGUAGE_VERSION..=LANGUAGE_VERSION).contains(&v),
        "grammar `{name}` ABI {v} outside [{MIN_COMPATIBLE_LANGUAGE_VERSION}, {LANGUAGE_VERSION}]"
    );
}

#[test]
fn abi_versions_are_compatible() {
    #[cfg(feature = "lua")]
    check("lua", super::lua());
    // … one arm per feature …
}
```

**Per-candidate wire-up (3 edits, matches CONTRIBUTING's recipe steps 1–2 exactly):**
1. `Cargo.toml`: add `tree-sitter-<lang> = { version = "<pinned>", optional = true }` under `[dependencies]`, and `<lang> = ["dep:tree-sitter-<lang>"]` under `[features]` (no `"_extractors"` — see D-02).
2. `src/grammar.rs`: add
   ```rust
   #[cfg(feature = "<lang>")]
   pub fn <lang>() -> Language {
       tree_sitter_<lang>::LANGUAGE.into()
   }
   ```
3. `src/grammar.rs` test mod: add `#[cfg(feature = "<lang>")] check("<lang>", super::<lang>());` inside `abi_versions_are_compatible`.

**Verification per candidate — run BOTH commands, not just the first:**
```bash
cargo test grammar::tests::abi_versions_are_compatible --features <lang>          # additive to defaults; proves ABI compat
cargo check --no-default-features --features <lang>                              # proves standalone isolation (D-02's actual requirement)
```
The first command alone is **not sufficient** to prove D-02's "must compile standalone" requirement, because it runs additively on top of the `default` feature list — `_extractors` is already satisfied by whatever default language pulled it in, masking the import-gate bug (Pitfall 1) entirely. Verified empirically: a grammar-only spike feature with the import bug present passed the additive ABI test cleanly while failing the isolation check with a hard compile error. Run the isolation check per-candidate as you go, not only once at the end via the new CI job — a batch of 15 candidates discovering the same root-cause compile error only when CI runs at the very end wastes 14 redundant debugging cycles.

### 2. Prep fix required before candidate #1 (not part of any single candidate's diff)

```rust
// src/grammar.rs — top of file
use tree_sitter::Language;   // unconditional; tree-sitter is a mandatory (non-optional) dependency
```
Verified via spike: removing the `#[cfg(feature = "_extractors")]` gate on this one line, then building a synthetic grammar-only feature (no `_extractors`, no `Language` enum variant, no dispatch arm — the exact shape every one of the 15 candidates will have), compiles clean under `cargo check --no-default-features --features <candidate>` and its ABI test passes. This is a one-line, low-risk fix; make it the first commit of the phase.

### 3. CI isolation job (COMPAT-03)

Current `.github/workflows/test.yml` has three jobs: `lint` (fmt/clippy/doc, `--all-features`), `test` (3-OS matrix, `--all-features`), `bindings` (ubuntu-only, maturin + napi, plus the existing napi-drift `git diff --exit-code` gate). **None of them build a single feature in isolation** — confirmed by `grep -r no-default-features .github/workflows/` returning nothing.

Add a fourth job (ubuntu-only per D-07, ~37 features after this phase: 22 existing + 15 new):
```yaml
  feature-isolation:
    name: Feature isolation (${{ matrix.lang }})
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        lang: [rust, python, typescript, go, java, c, cpp, ruby, php, shell, swift,
               kotlin, solidity, sql, hcl, csharp, scala, dart, lua, luau, pascal, svelte,
               zig, julia, r, ocaml, objc, fortran, groovy, powershell, systemverilog,
               astro, fsharp, elixir, erlang, gleam, haskell]
    steps:
      - uses: actions/checkout@v6
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          prefix-key: isolation-${{ matrix.lang }}
      - name: Build ${{ matrix.lang }} in isolation
        run: cargo check --no-default-features --features ${{ matrix.lang }}
```
A matrix (37 parallel, cheap `cargo check` jobs) is more idiomatic GitHub Actions than a shell for-loop in one step and gives per-language failure attribution in the PR UI for free — recommended over a for-loop (Claude's discretion, per CONTEXT.md). Cache key must vary per matrix value (`prefix-key: isolation-${{ matrix.lang }}`) or `Swatinem/rust-cache` will thrash across differently-featured builds sharing one cache.

**This job will fail immediately on `luau`** (pre-existing bug, verified — see Common Pitfalls §2) unless `Cargo.toml`'s `luau` feature is fixed in the same PR: `luau = ["dep:tree-sitter-luau", "lua", "_extractors"]`.

### 4. TS/JS entry-point detection — verified AST shapes

Spiked via a throwaway `examples/dump_ast_ts.rs` (written, run, deleted — per CONTRIBUTING's own recommended workflow) against `tree-sitter-typescript 0.23.2` (the pinned version):

**Express/Fastify/Koa/Hono call-terminal form** — `router.get('/users', getUsers)`:
```
(call_expression
  function: (member_expression object: (identifier) property: (property_identifier))
  arguments: (arguments (string (string_fragment)) (identifier)))
```
Structurally identical to Python's `PY_ROUTE_VERBS` shape (`call_expression`/`function:`/`member_expression`/terminal `property_identifier`) — the exact same terminal-match pattern applies almost verbatim, just walking `call_expression` nodes directly (found via a tree-sitter `Query`, same mechanism as the existing `CALL_QUERY` in `typescript.rs`) rather than unwrapping a `decorated_definition`.

**Bare identifier call form** — `app.post('/users', (req, res) => {...})` (inline handler) also parses as the same shape but with an `arrow_function` as the second argument instead of an `identifier` — see Open Questions §1 for why this matters.

**NestJS decorator form** — verified against:
```ts
@Controller('users')
export class UsersController {
  @Get(':id')
  findOne(@Param('id') id: string) { return id; }
}
```
produces (abbreviated):
```
(export_statement
  decorator: (decorator (call_expression function: (identifier) arguments: (arguments (string ...))))
  declaration: (class_declaration
    name: (type_identifier)
    body: (class_body
      decorator: (decorator (call_expression function: (identifier) arguments: (arguments (string ...))))
      (method_definition name: (property_identifier) parameters: (...) body: (...)))))
```
Two distinct attachment points:
- **Class-level** (`@Controller`): the `decorator:` field lives directly on `export_statement`, sibling to `declaration:` — exactly analogous to Java's class-level `@RestController`/`@Controller` detection, and TS already emits one `Symbol` per top-level class (via `emit_named` in `collect_symbols`), so this half is a drop-in port of `java.rs`'s pattern.
- **Method-level** (`@Get`/`@Post`/…): the `decorator:` field lives on `class_body`, positioned immediately before the `method_definition` node it modifies (tree-sitter allows a field name to repeat across multiple children — walk `class_body`'s children in order, and each `decorator` child applies to the next sibling `method_definition`). **The TS extractor currently emits no `Symbol` for `method_definition` at all** (see Open Questions §1) — this is the one place D-09's "mirror Java's annotation detector" cannot be a literal drop-in without first deciding what a method-level TS symbol even is.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| ABI compatibility checking | A custom crates.io dependency-manifest parser/version-range checker | The existing `abi_versions_are_compatible` runtime test in `src/grammar.rs` | It's the actual gate — it inspects the *compiled* parser's `abi_version()`, which is the only thing that matters; a manifest parser only tells you what the crate *claims*, which is exactly the trap PITFALLS.md's Pitfall 1 documents |
| Route/decorator detection for TS/JS | A framework-aware parser (Express/NestJS-specific AST library, or type-checking against `@types/express`) | Plain terminal-identifier matching on `call_expression`/`decorator` nodes (the `PY_ROUTE_VERBS`/`JAVA_ROUTE_ANNOTATIONS` pattern) | D-10 explicitly forbids type resolution/framework-config parsing; the existing pattern is proven, zero-dependency, and syntactic-only by design |
| CI feature-isolation matrix | A custom script computing the full feature powerset | `cargo check --no-default-features --features <lang>` per language, one at a time | The full powerset (2^37) is infeasible and unnecessary; PITFALLS.md's Pitfall 4 explicitly scopes the fix to "each language solo," not combinatorics |

**Key insight:** every mechanism this phase needs already exists in the repo in a proven, tested form (the ABI test, the CI job structure, the decorator/verb-matching pattern) — the actual engineering risk is in the two gaps that proven patterns don't cover (the `_extractors` import gate, and the missing TS method-symbol), not in reinventing anything.

## Common Pitfalls

### Pitfall 1: `_extractors` import gate blocks every grammar-only candidate feature (CONFIRMED, not hypothetical)

**What goes wrong:** `src/grammar.rs`'s `use tree_sitter::Language;` is gated `#[cfg(feature = "_extractors")]`. Every existing language feature bundles `_extractors` into its own feature list, so this has never surfaced. Per D-02, the 15 new candidates must NOT bundle `_extractors` (they have no extractor to gate). Reproduced directly: temporarily removing `"_extractors"` from an existing feature's declaration (`luau`, as a stand-in) and running `cargo check --no-default-features --features luau` produces `error[E0425]: cannot find type 'Language' in this scope` at the candidate's `pub fn <lang>() -> Language` accessor — a hard compile failure, not a warning.

**Why it happens:** the import was written when every grammar feature also happened to enable the extractor toolkit; nobody has added a grammar-only feature (one with no `Language` enum variant / no dispatch arm) before this phase.

**How to avoid:** make the import unconditional (`tree_sitter` is a mandatory, non-optional dependency of this crate — there is no cost to always importing it). Do this as the very first commit of the phase, before wiring candidate #1. Verified via spike: with the import unconditional, a synthetic grammar-only feature (mirroring exactly what all 15 candidates will look like) compiles clean in isolation and its ABI test passes.

**Warning signs:** `cargo check --no-default-features --features <candidate>` fails with `E0425: cannot find type Language in this scope` pointing at the candidate's own `pub fn` in `grammar.rs`.

**Phase to address:** Phase 1, as a prep step — must land before any of the 15 candidate wire-ups, or each candidate's isolation check will independently rediscover the same root cause.

### Pitfall 2: `luau` feature already fails isolation on `main` (CONFIRMED pre-existing bug, unrelated to this phase's new work)

**What goes wrong:** `cargo check --no-default-features --features luau` fails today (verified, unmodified `main`) with `error[E0432]: unresolved import 'super::lua'` — `src/extract/luau.rs` line 21 imports `super::{Extractor, lua::extract_lua_family}`, but `Cargo.toml`'s `luau = ["dep:tree-sitter-luau", "_extractors"]` never lists `"lua"` as a feature dependency, even though the Luau extractor structurally reuses the Lua-family extractor function.

**Why it happens:** `--all-features` (the only thing current CI runs) always has both `lua` and `luau` on together, so the missing cross-feature edge has never been exercised.

**How to avoid:** fix the declaration: `luau = ["dep:tree-sitter-luau", "lua", "_extractors"]`. This is not this phase's regression, but the new CI job (COMPAT-03) will immediately turn red on it unless it's fixed in the same PR that adds the job — plan for this as an explicit, small, in-scope fix, not a surprise CI failure to debug after the fact.

**Warning signs:** the new `feature-isolation` CI job (or a local `cargo check --no-default-features --features luau`) fails on `luau` specifically with an unresolved-import error naming `lua`.

**Phase to address:** Phase 1, bundled with the CI job addition (COMPAT-03) — one-line `Cargo.toml` fix, verify with the isolation command locally before relying on CI to catch it.

### Pitfall 3: TS extractor has no per-method `Symbol` — NestJS method-level decorators have nothing to attach to

**What goes wrong:** `java.rs`'s `entry_points_for_java` attaches `EntryPoint::HttpRoute` to a `method_sym` that already exists (Java extracts one `Symbol` per method). `typescript.rs`'s `collect_symbols` walks only top-level declarations — `class_body` is never descended into, so there is no equivalent per-method `Symbol` for a `@Get()`/`@Post()`-decorated NestJS method today. A literal "mirror Java's detector" port has no target to write to for the method-level half of D-09.

**Why it happens:** TS extraction depth for classes has never needed method-level identity before (Calls/Type-refs already work at whole-tree granularity via tree-sitter Queries, which don't require a per-method `Symbol` to exist — only entry-point *tagging*, which is a `Symbol` field, does).

**How to avoid:** make an explicit, documented scope decision before writing code (see Open Questions §1) — either (a) aggregate all route decorators found anywhere inside a class body onto that class's existing `Symbol` (lower scope, some precision loss vs. Java, no extractor architecture change), or (b) extend `collect_symbols` to emit one `Symbol` per `method_definition` inside `class_body` (larger scope, exact per-method parity with Java, but a real new capability beyond "add entry-point detection").

**Warning signs:** a task/PR description says "port Java's method-level annotation detector to TS" without first stating which of the two options above was chosen and why.

**Phase to address:** Phase 1 — must be an explicit, written decision in the TSADAPT-01 plan (same treatment PITFALLS.md's Pitfall 5 gives the Objective-C `.h` collision — a design call that needs a paper trail, not a silent implementation choice).

### Pitfall 4: The additive ABI test alone can mask the isolation bug

**What goes wrong:** `cargo test grammar::tests::abi_versions_are_compatible --features <lang>` (D-01's literal command) runs *additively* on top of `default` (which already includes 22 languages, all of which enable `_extractors`). A candidate with the Pitfall-1 bug present will pass this command cleanly — `_extractors` is already satisfied by an unrelated default feature — while still failing `cargo check --no-default-features --features <lang>` outright. Relying only on the D-01 command per candidate, then discovering the real isolation failure only once at the very end via the new CI job, means re-debugging the same root cause up to 15 times in CI instead of once locally.

**How to avoid:** run the isolation command locally per candidate as you go (see Architecture Patterns §1's "Verification per candidate"), not only via the eventual CI job.

**Phase to address:** Phase 1 — a workflow discipline note for whoever executes the 15 wire-ups, not a code change.

## Code Examples

### Python's call-terminal (decorator) verb match — the pattern to port for TS's call-expression form

```rust
// src/extract/python.rs (verbatim, lines ~143–232)
const PY_ROUTE_VERBS: &[&str] = &[
    "get", "post", "put", "delete", "patch", "head", "options", "trace",
    "route", "websocket", "ws",
];

fn entry_points_for(fn_name: &str, outer_node: &Node, bytes: &[u8]) -> Vec<EntryPoint> {
    let mut markers: Vec<EntryPoint> = Vec::new();
    if fn_name == "main" {
        markers.push(EntryPoint::Main);
    }
    if outer_node.kind() != "decorated_definition" {
        return markers;
    }
    for child in outer_node.children(&mut outer_node.walk()) {
        if child.kind() != "decorator" { continue; }
        let Some(call_node) = child.children(&mut child.walk()).find(|c| c.kind() == "call") else { continue; };
        let Some(func_node) = call_node.child_by_field_name("function") else { continue; };
        let (terminal, callee_text) = match func_node.kind() {
            "attribute" => {
                let terminal = func_node.child_by_field_name("attribute")
                    .map(|n| node_text(&n, bytes)).unwrap_or("");
                (terminal, node_text(&func_node, bytes))
            }
            "identifier" => { let text = node_text(&func_node, bytes); (text, text) }
            _ => continue,
        };
        if PY_ROUTE_VERBS.contains(&terminal) {
            markers.push(EntryPoint::HttpRoute(callee_text.to_owned()));
        }
    }
    markers
}
```

### Java's decorator/annotation-terminal match — the pattern to port for TS's `@Get`/`@Controller` form

```rust
// src/extract/java.rs (verbatim, lines ~57–135)
const JAVA_ROUTE_ANNOTATIONS: &[&str] = &[
    "RestController", "Controller", "RequestMapping", "GetMapping", "PostMapping",
    "PutMapping", "DeleteMapping", "PatchMapping",
    "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "Path",
];

fn entry_points_for_java(method_name: Option<&str>, node: &Node, bytes: &[u8]) -> Vec<EntryPoint> {
    let mut markers: Vec<EntryPoint> = Vec::new();
    if method_name == Some("main") && has_modifier(node, bytes, "static") {
        markers.push(EntryPoint::Main);
    }
    let Some(mods) = node.children(&mut node.walk()).find(|c| c.kind() == "modifiers") else {
        return markers;
    };
    for ann in mods.children(&mut mods.walk()) {
        if ann.kind() != "marker_annotation" && ann.kind() != "annotation" { continue; }
        let Some(name_node) = ann.child_by_field_name("name") else { continue; };
        let simple = simple_type_name(node_text(&name_node, bytes), ".");
        if JAVA_ROUTE_ANNOTATIONS.contains(&simple) {
            markers.push(EntryPoint::HttpRoute(simple.to_owned()));
        }
    }
    markers
}
```

### Verified AST shape for the two TS/JS forms (from the throwaway spike, `tree-sitter-typescript 0.23.2`)

```
// router.get('/users', getUsers);
(expression_statement (call_expression
  function: (member_expression object: (identifier) property: (property_identifier))
  arguments: (arguments (string (string_fragment)) (identifier))))

// @Controller('users') export class UsersController { @Get(':id') findOne(...) {...} }
(export_statement
  decorator: (decorator (call_expression function: (identifier) arguments: (arguments (string ...))))
  declaration: (class_declaration name: (type_identifier) body: (class_body
    decorator: (decorator (call_expression function: (identifier) arguments: (arguments (string ...))))
    (method_definition name: (property_identifier) parameters: (...) body: (...)))))
```

## Open Questions

1. **Where does a NestJS method-level route decorator's `EntryPoint::HttpRoute` fact attach, given TS emits no per-method `Symbol`?**
   - What we know: the AST fully supports detecting the decorator (verified via `to_sexp()` — `class_body`'s `decorator:` field precedes the `method_definition` it modifies); Java's precedent requires a method-level `Symbol` to exist, and TS's extractor doesn't currently produce one.
   - What's unclear: whether TSADAPT-01's intended scope includes adding method-level `Symbol` extraction to TS classes (a real new capability, arguably bigger than "entry-point detection"), or whether entry-point aggregation onto the existing class-level `Symbol` is an acceptable Phase-1 scope.
   - Recommendation: resolve explicitly in the plan, not implicitly in code. Two viable options:
     - **(A) Aggregate onto the class symbol** — walk the whole `class_body`, collect every route-decorator match (class-level `@Controller` AND all method-level `@Get`/`@Post`/…) into one `Vec<EntryPoint>` on the class's existing `Symbol`. Zero extractor-architecture change; some precision loss (a class with 5 routes shows 5 `HttpRoute` markers on one symbol, not attributed to individual methods) but stays honest (no fabricated per-method symbol id) and matches D-10's "unambiguous syntax only" framing since it's still a straight aggregation, not a guess.
     - **(B) Add method-level `Symbol` extraction to `collect_symbols`** for `class_body` → `method_definition`, giving TS full per-method identity (SCIP id needs a `Descriptor::Method` scoped under the class, mirroring how Java constructs qualified method ids) — exact parity with Java's detector, but a materially larger change than "wire entry-point detection," with knock-on effects on the `docs/supported-languages.md` Calls/Type-ref row semantics (methods would newly be independently addressable/resolvable targets).
   - Given the phase boundary explicitly excludes new-language-extractor-scale work ("No new language extractors in this phase"), Option A is the lower-risk fit for Phase 1's stated boundary; Option B is a legitimate, larger follow-on (flag it for a future phase/backlog item if not selected now, since it's real, not-yet-realized depth on an already-⭐ language).

2. **Exact call-terminal verb list — resolve the CONTEXT.md vs. FEATURES.md tension.**
   - What we know: CONTEXT.md's discretion note says "match Python's list plus `all`/`use` only if unambiguous — err toward precision." FEATURES.md's own analysis (same research pass) explicitly recommends *against* `.use()`: "deliberately excluded — too generic ... would produce a flood of false-positive `HttpRoute` markers," since `.use()` is Express/Koa's generic middleware-registration call (logging, auth, static files — not HTTP routes).
   - What's unclear: nothing, really — this is FEATURES.md correcting itself mid-document; there's no genuine ambiguity in the underlying fact (`.use()` matches on every Express/Koa app regardless of whether it's routing).
   - Recommendation: adopt Python's exact verb list (`get, post, put, delete, patch, head, options, trace, route, websocket, ws`) plus `all` (Express/Fastify's real "match any HTTP method" registration — genuinely unambiguous and route-shaped), and explicitly **exclude** `use` per FEATURES.md's own reasoning. Document the exclusion inline (mirroring how `python.rs`'s doc comment explains its own boundary) so a future contributor doesn't "helpfully" add it back.

3. **Should Svelte's `Entry-pts` doc column change as a side effect?**
   - What we know: Svelte's `<script>` block is parsed via the same shared `extract_ecmascript` path (D-11 confirms JS/TS/TSX/Svelte all inherit whatever lands there); Svelte's docs row currently shows `Entry-pts` blank, same as TS/JS before this phase.
   - What's unclear: whether a NestJS/Express-shaped pattern realistically ever appears inside a Svelte component's `<script>` block (very unlikely in practice — Svelte components aren't HTTP route handlers), so this is mostly a documentation-consistency question, not a functional one.
   - Recommendation: leave Svelte's docs row as-is (blank) unless a corpus case demonstrates the fact actually flows through in practice; note in the PR description that it's mechanically inherited but not a targeted capability for this phase, to preempt a reviewer asking "why didn't you update Svelte's row too."

## Environment Availability

Phase is Rust/Cargo + GitHub Actions YAML only — no external services, databases, or non-Cargo runtimes required.

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `cargo` / `rustc` | All of COMPAT-01/02/03, TSADAPT-01 | ✓ | 1.96.0 | — |
| Network access to crates.io | Fetching the 15 new grammar crates | ✓ (verified live during this research pass — all crates fetched successfully) | — | — |
| `git` | Verdict/isolation-bug reproduction, commits | ✓ | 2.50.1 | — |

No missing dependencies; nothing blocks execution.

## Sources

### Primary (HIGH confidence — read directly or reproduced empirically in this repo)
- `CONTRIBUTING.md` — §"Adding a Language" (6-step recipe), §"When a Language Has No Usable Grammar" — read directly.
- `src/grammar.rs` — read directly; the `_extractors` import-gate bug and its fix were reproduced with a live spike (`cargo check --no-default-features --features <candidate>`, both broken and fixed states), then reverted.
- `Cargo.toml` — read directly; the `luau` isolation bug was reproduced live (`cargo check --no-default-features --features luau` fails on unmodified `main`), then reverted.
- `.github/workflows/test.yml` — read directly; confirmed via `grep` that no existing job passes `--no-default-features`.
- `src/extract/python.rs`, `src/extract/java.rs`, `src/extract/typescript.rs`, `src/lang.rs`, `src/extract/support.rs`, `src/graph/types.rs` — read directly, including the full `entry_points_for`/`entry_points_for_java` implementations and the `Symbol`/`EntryPoint` schema.
- Live AST spike: a throwaway `examples/dump_ast_ts.rs` (written, `cargo run --example`, output captured, file deleted per CONTRIBUTING's own recommended workflow) against the pinned `tree-sitter-typescript 0.23.2`, dumping `to_sexp()` for both a NestJS-shaped class and an Express-shaped call sequence.
- `docs/supported-languages.md` — read directly; confirmed TS/JS/Svelte rows all currently show a blank `Entry-pts` cell, and that the doc's only automated sync test (`supported_languages_doc_lists_each_primary_extension` in `src/lang.rs`) checks extension-token presence only, not column accuracy.

### Secondary (HIGH confidence — same-day project research artifacts, cross-checked against live repo state above)
- `.planning/research/STACK.md` — per-candidate crate/version/license verdicts (crates.io API, 2026-07-05).
- `.planning/research/PITFALLS.md` — CI isolation gap (Pitfall 4), Objective-C `.h` precedent for "explicit written decision" framing (Pitfall 5), used as the template for how to present Open Question §1.
- `.planning/research/FEATURES.md` — TS/JS entry-point AST-shape hypothesis (§"TypeScript/JavaScript Entry-Points") — verified correct for the call-expression form and the class-level decorator form; found to be silent on the method-level-symbol gap, which this research fills in.

### Tertiary (not used — no unverified/low-confidence claims in this document)
None — every claim above is either a direct repo read or a reproduced, then-reverted, empirical result.

## Metadata

**Confidence breakdown:**
- Standard stack (15 crate versions): HIGH — inherited from STACK.md's same-day crates.io API verification; not independently re-queried since no time has elapsed.
- Architecture (grammar wiring, CI job, TS AST shapes): HIGH — grammar wiring and the `_extractors`/`luau` bugs were reproduced empirically with real `cargo check`/`cargo test` runs against the actual crates; TS AST shapes came from an actual `to_sexp()` dump against the pinned grammar version, not from memory or the grammar's GitHub `node-types.json` (avoiding PITFALLS.md's Pitfall 2 by construction).
- Pitfalls: HIGH — all four are either reproduced compile failures (Pitfalls 1, 2, 4) or a structural fact read directly from `typescript.rs`'s absence of per-method symbol emission (Pitfall 3).

**Research date:** 2026-07-05
**Valid until:** ~14 days for the architecture/pitfalls findings (repo-structural, won't drift); ~7 days for the 15 crate version pins (independently-maintained grammar crates can publish a new — possibly ABI-breaking — release at any time; re-verify with `cargo info` if execution starts more than a week out).
