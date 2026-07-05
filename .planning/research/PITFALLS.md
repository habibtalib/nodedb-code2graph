# Pitfalls Research

**Domain:** Adding tree-sitter grammar-backed language extractors to an existing quality-gated Rust code-graph library (code2graph)
**Researched:** 2026-07-05
**Confidence:** MEDIUM-HIGH (repo-verified structural claims are HIGH; per-grammar ecosystem claims for candidate languages not yet added are MEDIUM/LOW and should be re-verified against crates.io at the moment each language phase starts)

## Critical Pitfalls

### Pitfall 1: Grammar crate's declared `tree-sitter` version doesn't guarantee ABI compatibility

**What goes wrong:**
A candidate grammar crate (e.g. `tree-sitter-elixir`, `tree-sitter-haskell`, `tree-sitter-ocaml`) looks compatible because its `Cargo.toml` depends on `tree-sitter = "0.24"` or similar, but the crate's *generated parser* (the `LANGUAGE` const, compiled from a `grammar.js`/`src/parser.c` pinned at a specific tree-sitter-cli version) carries its own ABI version baked into the C output. A crate can declare a compatible Rust-level dependency while its generated parser's `abi_version()` sits outside `[MIN_COMPATIBLE_LANGUAGE_VERSION, LANGUAGE_VERSION]` — or the inverse: an old crate published years ago against tree-sitter 0.20 with a stale `Cargo.toml` that nobody bumped, which won't even compile against `tree-sitter >=0.24, <0.27` because the crate's own `Language`-returning API type comes from a different major version of the `tree-sitter` crate (a different Rust type, not just a version string).

**Why it happens:**
Grammar crates are maintained independently, often by single volunteers, with irregular release cadence. The published crate version and the underlying `tree-sitter-cli` version used to regenerate `parser.c` are two independent numbers that drift out of lockstep. `cargo add` / `cargo update` only checks semver on the Rust dependency graph — it cannot check ABI compatibility, because ABI version is a runtime property of the generated C, not a Cargo.toml field. This exact confusion is already flagged in `CONCERNS.md` ("Tree-Sitter Version Constraint") and `CONTRIBUTING.md` §"When a Language Has No Usable Grammar" rule 2, and is the reason `src/grammar.rs` has a dedicated runtime test rather than relying on Cargo resolution alone.

**How to avoid:**
- Never treat "crate depends on tree-sitter 0.2x" as sufficient evidence. Always add the grammar dependency, wire the one-line `pub fn <lang>()` in `src/grammar.rs`, add its `check("<lang>", super::<lang>())` arm to `abi_versions_are_compatible`, and run `cargo test grammar::tests::abi_versions_are_compatible --features <lang>` **before** writing a single line of extractor code.
- Do this gate check as a throwaway spike per candidate language, separate from and before the extractor PR — reject/park the language immediately if the ABI check fails, per CONTRIBUTING rule 3 ("do not bridge incompatible tree-sitter versions... will be rejected").
- Record the verified-compatible crate version in the phase notes so a later `cargo update` that bumps the grammar crate to a newer, ABI-incompatible release is caught by CI rather than discovered at release time.

**Warning signs:**
- `cargo build --features <lang>` fails to compile with a type mismatch on `tree_sitter::Language` (classic sign the grammar crate pulls a different major version of the `tree-sitter` crate transitively — check `cargo tree -e features -i tree-sitter`).
- `abi_versions_are_compatible` panics with an explicit `"grammar `<name>` ABI {v} outside [...]"` message — this is the intended, cheap detection point; treat any failure here as a hard stop, not a version bump to chase.

**Phase to address:**
A dedicated "grammar feasibility gate" step at the start of *each* new-language phase (not a separate phase of its own) — verify ABI compatibility before scoping extractor work for that language. Should run as literally the first commit of each language's phase so a rejection costs one throwaway commit, not a half-built extractor.

---

### Pitfall 2: Writing the extractor against the GitHub repo's `node-types.json` instead of the actual installed crate's grammar

**What goes wrong:**
The extractor is written by reading the grammar's GitHub repository (`grammar.js`, `node-types.json`, example queries) but the crate version actually pinned in `Cargo.toml` was cut from an older or newer commit. Field names, node kinds, or wrapper nodes differ — e.g. a "signature" node that used to be a single field is split into `name` + `parameters` fields, or a punctuation token is given a field label that looks like real content. The extractor compiles, tests pass on hand-written toy fixtures that happen to match the assumed shape, and then silently mis-extracts (or extracts nothing) for real-world code shaped slightly differently than the toy fixture.

**Why it happens:**
GitHub `main` branch node-types.json is a moving target; the published crate is a frozen snapshot. This is explicitly called out in CONTRIBUTING.md: "Published grammars frequently differ from the `node-types.json` in their GitHub repo." It's an easy trap because reading docs feels more efficient than running code, especially under time pressure to ship a new language quickly.

**How to avoid:**
- Follow the CONTRIBUTING tip literally: after wiring steps 1–2 (Cargo dep + `src/grammar.rs` registration), drop a throwaway `examples/dump_ast.rs` that parses 2–3 representative real-world snippets (not toy one-liners — pull a real file from a popular OSS repo in that language) and prints `tree.root_node().to_sexp()`. Read the actual tree from the actual pinned crate version. Delete the throwaway example before the PR (or keep it gated behind a doc-only note — check repo convention).
- Prefer snippets exercising the constructs the extractor most needs: function/method definitions with generics or decorators, member/qualified calls (to validate the receiver/qualifier field), imports (to validate the `from_path` shape), and at least one construct known to be unusual for the language (e.g. Elixir's `defmodule`/`def` distinct from Erlang's `-module`/function clauses; Haskell's cascading `where` clauses).
- Reuse `src/extract/support.rs` field-access helpers rather than raw `node.child(n)` positional access — field-based lookups (`field_text`) degrade more gracefully across minor grammar shape drift than positional indexing.

**Warning signs:**
- Extractor unit tests use only single-line, idealized snippets and never a multi-construct real-world file.
- A reviewer can't find a comment/commit showing the `to_sexp()` dump was actually run against the pinned crate version (no evidence of verification, just assumed shape from memory/docs).

**Phase to address:**
Every language phase, as a mandatory first sub-step before extractor implementation — should be an explicit acceptance criterion in each language's phase plan ("AST dumped against pinned crate version; extractor test fixtures pulled from real code, not synthesized").

---

### Pitfall 3: Bundled scanner C/C++ code breaks the build on a subset of platforms

**What goes wrong:**
Several tree-sitter grammars for structurally irregular languages (Haskell, OCaml, and historically others with complex lexing like heredocs/layout rules) ship an *external scanner* written in C or C++ (`scanner.c`/`scanner.cc`) compiled via the `cc` crate, in addition to the generated `parser.c`. These scanners are hand-written, not generated, and have shown real cross-platform build failures: `tree-sitter-haskell` has open/historical issues failing to build on Windows (MSVC rejecting `to_string`/certain C++ stdlib usage in `scanner.cc`) and on macOS (C++11 initializer-list syntax rejected under the default C++ standard the `cc` crate selects). A language extractor that works and passes CI on the contributor's own machine (typically Linux or a recent macOS with a modern toolchain) can fail to build entirely for Windows consumers, or vice versa.

**Why it happens:**
Scanner code is maintained by the grammar author, often tested only on their own OS, and doesn't go through the same generated-code rigor as `parser.c`. The `cc` crate's default C/C++ standard selection varies by platform and compiler (MSVC vs. Clang vs. GCC), and scanner authors write against whichever compiler they use locally.

**How to avoid:**
- Confirm the target grammar's scanner language (C vs. C++) and check its issue tracker for open Windows/macOS build reports before selecting it as the "feasible" grammar for that language — this is part of the feasibility gate (Pitfall 1), not a separate step.
- Rely on the existing 3-OS CI matrix (`ubuntu-latest`, `macos-latest`, `windows-latest`, `--all-features`, in `.github/workflows/test.yml`) to catch this — but note the *bindings* job (Python maturin wheel + Node napi build) currently only runs on `ubuntu-latest`, so a scanner that builds fine in the main 3-OS test job could still surface an issue specifically in the maturin/napi build step on Windows/macOS that the bindings job never exercises. Flag this gap if a Windows/macOS-fragile grammar (Haskell, OCaml, or any other C++-scanner grammar) is added — extend the bindings job to the same 3-OS matrix for that PR, or explicitly document the known gap.
- If a candidate language's only maintained grammar has known unresolved cross-platform scanner bugs, treat it the same as an ABI mismatch: document the blocker honestly (CONTRIBUTING §"no usable grammar" step 5) rather than merging a Linux-only-green PR.

**Warning signs:**
- The grammar crate's repo has a `scanner.c` or `scanner.cc` file (check `cargo tree`/crate source) rather than being pure generated `parser.c`.
- Open GitHub issues on the grammar repo mentioning "Windows", "MSVC", "undefined reference", or "C++11"/"C++17" build failures.
- CI green on the PR author's fork (often Linux-only local testing) but the full 3-OS matrix hasn't actually run yet at review time.

**Phase to address:**
Feasibility-gate step for any candidate language known to use an external C/C++ scanner (Haskell, OCaml are the two flagged candidates here); explicitly run the full 3-OS CI matrix before merging, and extend the bindings job's OS matrix for that specific language's PR if the scanner is C++-based.

---

### Pitfall 4: Feature-flag combinatorics — CI only tests `--all-features`, never a single language in isolation

**What goes wrong:**
CONTRIBUTING.md documents `cargo test --no-default-features --features rust` as the supported way to work on one language in isolation, and the `Cargo.toml` feature model requires each language to be independently compilable (`lang = ["dep:tree-sitter-lang", "_extractors"]`, with composite cases like `svelte = ["dep:tree-sitter-svelte-ng", "typescript", "_extractors"]` for embedded-language extractors). But the actual CI (`.github/workflows/test.yml`) only ever runs `cargo test --workspace --all-features` and `cargo clippy --workspace --all-targets --all-features`. There is **no CI job that builds any single-feature or no-default-features combination**. A new extractor can silently gain an undeclared dependency on another language's feature (e.g. reusing a helper only compiled under `#[cfg(feature = "typescript")]` without adding `"typescript"` to its own feature list, or a shared support-module change that only compiles when multiple grammars are present) and this will never be caught until a downstream consumer runs `cargo build --no-default-features --features <that-lang>` and gets a compile error.

**Why it happens:**
`--all-features` is the fast, obvious CI choice and catches the vast majority of real bugs; testing the full feature powerset (23+ languages → 2^23 combinations) is infeasible, so no one built even a minimal "each language solo" matrix. This is a real, currently-existing gap (verified: no `no-default-features` string appears anywhere in `.github/workflows/`), not a hypothetical.

**How to avoid:**
- For embedded/composite languages (Astro, following the Svelte pattern per PROJECT.md), explicitly declare the transitive feature dependency in `Cargo.toml` (`astro = ["dep:tree-sitter-astro", "typescript", "_extractors"]`) — mirror the existing Svelte precedent exactly.
- Add a lightweight CI matrix job (or at minimum a documented local pre-merge check) that runs `cargo check --no-default-features --features <lang>` for every language touched in a PR, not the full powerset — this is cheap (one `cargo check` per touched language, not per combination) and closes the gap CONTRIBUTING already assumes exists.
- When adding a new language, actually run the isolation command from CONTRIBUTING locally before opening the PR, not just `cargo test --workspace` (which is `--all-features` by default in this repo's setup and would mask the gap).

**Warning signs:**
- A language's extractor imports or calls into another extractor's module or a shared type gated behind a different feature without a corresponding `Cargo.toml` feature edge.
- PR only ran `cargo test --workspace` / relied on CI green, no evidence the contributor ran the isolated-feature build locally.

**Phase to address:**
Should be raised as a cross-cutting infra fix at the start of the language-expansion milestone (a "harden CI" or "phase 0" task), not deferred — otherwise every subsequent language phase inherits the same undetected risk, and it compounds with 8–13 new languages being added.

---

### Pitfall 5: Objective-C extension dispatch collides with existing C-family conventions

**What goes wrong:**
`src/lang.rs`'s `Language::extensions()` is a flat, first-match extension → language map (verified: `Language::C => &["c", "h"]`, `Language::Cpp => &["cc","cpp","cxx","hh","hpp","hxx"]` — note C++ headers using bare `.h` are *already* dispatched to the C extractor today, a pre-existing, accepted ambiguity). Objective-C's canonical implementation extension is `.m` and Objective-C++ is `.mm`, but Objective-C **headers are conventionally `.h`** — identical to C and (today) to any C++ file using a `.h` header. If Objective-C is added, its header files will silently be extracted as C (wrong symbol kinds, missed `@interface`/`@implementation`/`@protocol` constructs, no Objective-C message-send references) with no error — `from_extension` has no ambiguity signal, it just returns the first (only) match in the list. Separately, `.m` is also the canonical source extension for MATLAB and (less commonly) Mercury — not currently in scope, but worth noting for any future roadmap that adds either, since `from_extension` has no per-directory/content-sniffing fallback to disambiguate.

**Why it happens:**
`Language::from_extension` is deliberately simple (single source of truth, no configuration, no sniffing) — appropriate for the current language set where extensions are unique, but Objective-C is the first candidate whose *header* extension is not unique to it.

**How to avoid:**
- Decide explicitly, and document in the Objective-C phase plan, what happens to `.h` files when Objective-C is enabled: either (a) leave `.h` mapped to C only (Objective-C header content — interfaces, protocols — goes unextracted, same honest-gap pattern already accepted for other blank-cell capabilities per CONCERNS.md), or (b) accept that enabling both `c` and `objc` features makes `.h` dispatch ambiguous and pick one deterministically, documenting the tradeoff in `docs/supported-languages.md`.
- Do not attempt content-sniffing (parsing the file with both grammars and picking the one that parses "better") — this contradicts the project's simple, deterministic dispatch model and adds hidden cost per file.
- Map only `.m`/`.mm` to Objective-C/Objective-C++ explicitly; leave `.h` out of the Objective-C extension list to avoid silently reassigning existing C/C++ header handling. State this limitation plainly in the docs row rather than silently under-extracting.

**Warning signs:**
- A PR adds `"h"` to Objective-C's `extensions()` list, which would make `Language::from_extension` behavior depend on iteration order over `Language::ALL` (first match wins) — this should fail review immediately since it silently changes existing C/C++ dispatch behavior.

**Phase to address:**
The Objective-C language phase specifically — must be an explicit design decision documented in that phase's plan before extractor work starts, not discovered during implementation.

---

### Pitfall 6: Groovy `.gradle` build scripts are real-world Groovy but violate every fixture/corpus assumption

**What goes wrong:**
If Groovy support is scoped to include `.gradle` build files (a very common real-world source of Groovy code — arguably more Groovy code exists in Gradle build scripts than in application `.groovy` files), the extractor faces a corpus with extreme structural variance: heavy use of the Groovy Builder DSL pattern (nested closures as pseudo-declarative blocks), dynamic method dispatch that looks like keywords (`dependencies { implementation 'foo:bar:1.0' }`), and Gradle-specific DSL extensions (Kotlin DSL `.gradle.kts` is a *different* language — Kotlin — not Groovy, adding a second extension-mapping decision). An extractor written and tested against typical class-based Groovy (`.groovy` application code, structurally similar to Java) will badly under- or mis-extract `.gradle` files because the dominant construct in a build script is closures-as-arguments, not classes/methods.

**Why it happens:**
Extractors are usually templated off "the freshest structurally-similar extractor" per CONTRIBUTING guidance (Java-like for Groovy makes sense for class-based `.groovy` code) — but build scripts are a fundamentally different usage pattern of the same grammar, not covered by a Java-shaped template.

**How to avoid:**
- Explicitly scope the Groovy phase: decide upfront whether `.gradle` is in scope at all for this milestone, or deferred as a follow-up once basic `.groovy` extraction is proven. If in scope, treat it as its own corpus category (`eval/corpus/groovy_oracle/gradle_dsl/` or similar) rather than assuming `.groovy` test cases generalize.
- If `.gradle` is included, ensure `Language::extensions()` for Groovy explicitly lists `gradle` separately from `groovy` (do not conflate `.gradle.kts` — that must dispatch to Kotlin, already an existing `Language::Kotlin` variant with `.kt`/`.kts` — verify `.gradle.kts` file names don't get truncated by the `rsplit('.')` extension logic in `from_path`, which takes only the text after the *last* dot: `"build.gradle.kts".rsplit('.').next()` yields `"kts"`, which already correctly routes to Kotlin today, so this is actually safe — confirm this in a unit test rather than assuming).
- Add at least one real-world `build.gradle` snippet (not synthesized) to the corpus if `.gradle` is in scope, specifically exercising the DSL-closure pattern, to catch under-extraction early.

**Warning signs:**
- Groovy extractor unit tests only cover class/method definitions, no closure-as-argument or builder-DSL patterns.
- No corpus case sourced from an actual Gradle build file.

**Phase to address:**
Groovy language phase — scope decision (`.gradle` in vs. out) must be made explicit in that phase's plan, not left implicit.

---

### Pitfall 7: Sync tests fail the build when docs matrices lag the code change — but only if forgotten in the *same* PR

**What goes wrong:**
Two guard tests already exist and are verified in this repo: `supported_languages_doc_lists_each_primary_extension` (in `src/lang.rs`, checks every `Language::ALL` variant's primary extension appears as a backticked token in `docs/supported-languages.md`) and the FFI matrix sync test (`ffi_markers_are_documented` per CONTRIBUTING, checking `docs/ffi-support-matrix.md` against the `SPECS` registry in `src/ffi/`). These are strict positive-membership checks — they fail loudly if a new `Language` variant's extension doc row is missing. But they only catch *missing* additions, not *stale* content: e.g. if a language's capability column (imports, type-refs, entry-points) changes in the extractor but the doc table's cell isn't updated, no test fails, because the sync test only checks that the primary extension string exists somewhere in the doc — not that every column/claim is accurate. This is the exact gap already flagged in CONCERNS.md ("Documentation Drift Risk... the inverse is human-driven").
Additionally, per CONCERNS.md this project also carries a *committed-artifact* drift gate that's easy to conflate with docs drift: the Node napi bindings CI gate (`git diff --exit-code -- index.js index.d.ts` after `napi build`) fails if a `#[napi]` signature change wasn't regenerated and recommitted — a *different* mechanism (build-output diffing, not doc-content-string checking) that a contributor might assume is covered by "the sync tests" generically, but isn't the same test and doesn't run in the same job as the language-doc sync test.

**Why it happens:**
Sync tests are cheap to write as "does X appear in Y" existence checks; writing a test that validates the *semantic accuracy* of every table cell (e.g. "if extractor emits entry-point facts, the Entry-pts column for this language must not be blank") requires structured, machine-readable doc data (not free-form markdown prose), which this repo doesn't have.

**How to avoid:**
- Treat the two existing sync tests as necessary-but-not-sufficient. When adding a language, manually update every column in `docs/supported-languages.md` for that row (imports, type-refs, entry-points, read/write, inheritance) to match exactly what the extractor emits — don't rely on any test to catch a stale/optimistic cell.
- When a language's binding-affecting API surface changes (new `Language` variant exposed through Node/Python bindings), run the napi regeneration step locally (`npm run build` in `bindings/node/`) and commit the diff in the *same* PR as the Rust change — this is a separate, mechanically-enforced gate (CI `git diff --exit-code`) from the docs sync test, and missing it fails a different CI job than missing a docs update.
- Add a PR checklist item (mirroring CONTRIBUTING's own guidance) enumerating: `Language` variant added + `ALL` + `as_str` + `extensions`; `docs/supported-languages.md` row (all columns, not just the extension); Python binding exposure; Node binding exposure + regenerated `index.js`/`index.d.ts` committed; `eval/corpus/` case added.

**Warning signs:**
- A docs table cell claims a capability (e.g. "Entry-pts: ✓") that isn't actually backed by a corresponding code path in that language's extractor — only caught by manual review, not CI.
- A PR touches `bindings/node/src/lib.rs` `#[napi]` signatures but the diff doesn't include changes to `index.js`/`index.d.ts` — this **will** fail CI (verified gate exists), but only if the contributor pushes; a local-only build that "looks fine" without running the exact `napi build --release --platform` command from the workflow can miss it until CI runs.

**Phase to address:**
Every language phase's Definition of Done should explicitly include "all supported-languages.md columns for this row reviewed line-by-line against the extractor's actual emitted facts" — not just "row exists." Bindings-parity phase (Python + Node) should include "napi build regenerated and committed" as an explicit gate check, distinct from the docs check.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|-----------------|-----------------|
| Skip the `to_sexp()` AST dump and write the extractor from GitHub's `node-types.json`/docs memory | Faster to start coding | Silent mis-extraction on real code shaped differently than assumed; costly review round-trips (explicitly warned against in CONTRIBUTING) | Never |
| Add a new language without a corresponding no-default-features CI check | Ships faster, no infra work | Undetected feature-flag coupling surfaces only for downstream consumers using `--no-default-features` | Only if the "harden CI" cross-cutting task (Pitfall 4) is explicitly deferred and tracked, not silently skipped |
| Ship a language with only hand-authored golden fixtures, no SCIP oracle corpus | Faster (oracle regeneration requires an external indexer, off-by-default per `eval/ORACLE.md`) | Resolution quality (Tier-B precision/recall) stays unverified against ground truth — already true for 12+ existing languages per CONCERNS.md | Acceptable for initial phase of each new language; should be tracked as a known gap, matching existing project precedent, not treated as "done" |
| Leave capability columns (imports/type-refs/entry-points) blank for a new language rather than attempting incomplete extraction | Avoids shipping wrong facts | Consumer reachability/resolution depth limited for that language, same honest tradeoff already made for Ruby/C/Shell/Lua per CONCERNS.md | Always acceptable if honestly documented — this is the project's stated philosophy ("blank cells are real gaps," not silently faked) |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|--------------|-----------------|-------------------|
| Grammar crate → `src/grammar.rs` | Importing `tree_sitter_<lang>` directly in the extractor module (violates the single-chokepoint invariant) | Register the grammar function in `src/grammar.rs` only; extractors call `crate::grammar::<lang>()` |
| Embedded-language extractor (Astro, following Svelte pattern) | Re-implementing TS/JS parsing inside the new host extractor instead of reusing the existing engine | Parse the host document, locate the embedded script node, call `super::typescript::extract_ecmascript`, remap offsets via `support::shift_offsets` — exactly as Svelte does |
| Node bindings (napi-rs) | Changing a `#[napi]` signature and only running `cargo build`/`cargo test`, never `npx napi build --release --platform` | Regenerate via the exact command CI runs, commit `index.js`/`index.d.ts` in the same PR |
| Python bindings (PyO3/maturin) | Assuming Python binding parity is automatic once the Rust `Language` enum gains a variant | Explicitly verify the new variant is exposed through `bindings/python/src/lib.rs`'s language mapping — no CI gate currently enforces this the way napi drift is enforced (asymmetric risk: Python binding drift is not caught by an equivalent `git diff --exit-code` gate) |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|-----------------|
| Extractor grows past ~2000 lines by accretion (existing precedent: `rust.rs` 2932 lines, `java.rs` 2164 lines) | Hard to review, hard to find the relevant walk branch when adding a capability later, higher regression risk on edits | Split into submodules from the start for structurally rich languages (definitions/references/entry-points as separate files under a `src/extract/<lang>/` directory) rather than one flat file — CONTRIBUTING's "keep files focused... use a directory from the start" guidance applies especially to new large languages like Haskell/OCaml with rich construct sets | Once a single extractor file exceeds ~1500–2000 lines, per existing project pattern |
| No single-file extraction speed benchmark exists (per CONCERNS.md) | A pathological real-world file (e.g. an enormous generated `.gradle` file, or a deeply nested Haskell `where`-clause file) could be slow with no regression alarm | Not urgent for this milestone but worth a note: if a new language's typical real-world corpus includes very large generated files (Gradle multi-module build scripts can be huge), consider adding a basic timing smoke test for that language's corpus case | Not yet — flag as a watch item, not a blocker |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Adding a grammar crate without checking its license | Apache-2.0 project incompatibility (GPL-licensed grammar crate, or a crate with an incompatible or absent license) | Check the grammar crate's license on crates.io/GitHub before adding the dependency — CONTRIBUTING and PROJECT.md both flag "grammar crate licenses must be compatible" as a constraint; most tree-sitter grammars are MIT, but this is not universal and should be verified per-candidate, not assumed |
| Enabling all languages by default (current pattern: `default = [...]` lists all 23 languages) continuing unchanged as more are added | Larger default attack surface / build time as more third-party C parsers are pulled in by default | Already deliberately mitigated (each language is an independent optional feature); for the new batch, keep following the same pattern — add each new language to `default` only if the project intends it to ship as source-available by default, matching existing precedent, and remind consumers who care to use `--no-default-features --features <specific langs>` |

## "Looks Done But Isn't" Checklist

- [ ] **Grammar registered:** Often missing the `abi_versions_are_compatible` check arm in `src/grammar.rs` — verify the new `check("<lang>", super::<lang>())` line was actually added, not just the `pub fn <lang>()` accessor.
- [ ] **Extension dispatch:** Often missing a check for extension collisions with existing languages (see Pitfall 5, Pitfall 6) — verify `Language::from_extension` unit tests explicitly assert the new language's extensions don't silently shadow or get shadowed by an existing variant.
- [ ] **Docs table row:** Often has the extension listed (satisfies the sync test) but stale/optimistic capability columns — verify every column against the actual extractor code, not just presence of the row.
- [ ] **Bindings parity:** Often the Rust `Language` variant is added and Node bindings regenerated (CI-enforced), but Python binding exposure is missed since there's no equivalent automated drift gate for PyO3 — verify manually.
- [ ] **Feature isolation:** Often compiles fine under `--all-features` (the only thing CI checks) but not under `--no-default-features --features <lang>` alone — verify manually per Pitfall 4 until a CI job exists.
- [ ] **Corpus case:** Often ships with only synthesized, idealized snippets, not a real-world source file — verify at least one corpus case is derived from actual OSS code in that language.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|-----------------|-----------------|
| Grammar ABI incompatible, discovered after some extractor work started | LOW (if caught early via the feasibility gate) / MEDIUM (if discovered mid-implementation) | Abandon the language for now; document the blocker (crate name, tree-sitter version it targets) in an issue per CONTRIBUTING step 5 so it's not re-discovered; move it from "candidate" to "blocked" in `docs/supported-languages.md` |
| Extractor written against wrong node shape (Pitfall 2), discovered via corpus/oracle scoring after merge | MEDIUM | Add the missing `to_sexp()` dump against the pinned version now, diff against the extractor's assumed shape, patch the specific mismatched field/node accesses, add a regression corpus case exercising the exact construct that was wrong |
| `.h`/extension collision (Pitfall 5) discovered after Objective-C ships | MEDIUM–HIGH (changes user-visible dispatch behavior for existing C/C++ users) | Requires a deliberate, documented breaking-change decision (which language "wins" `.h`), not a silent patch — treat as a design change needing its own review, not a quick fix |
| Bindings drift (napi or PyO3) discovered post-release | MEDIUM (napi: CI would have caught it pre-merge if run; if somehow bypassed, requires a patch release) / MEDIUM-HIGH (PyO3: no automated gate, could ship silently) | For napi: re-run `napi build`, commit, patch release. For PyO3: add the missing binding exposure, patch release, and treat the absence of an equivalent automated gate as a follow-up infra task |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|-------------------|----------------|
| Grammar ABI/version mismatch (Pitfall 1) | Feasibility-gate sub-step at the start of each language phase | `abi_versions_are_compatible` test passes with the specific candidate crate version pinned |
| Wrong node shape from stale docs (Pitfall 2) | Every language phase, before extractor implementation | PR includes evidence (commit message or note) that `to_sexp()` was dumped against the pinned crate version using real-world snippets |
| Scanner C/C++ cross-platform build breakage (Pitfall 3) | Feasibility gate for Haskell/OCaml specifically (and any other scanner-based candidate) | Full 3-OS CI matrix green; bindings job OS matrix extended for that language if C++-scanner-based |
| Feature-flag combinatorics gap (Pitfall 4) | Cross-cutting infra task at milestone start (phase 0), reinforced per language phase | `cargo check --no-default-features --features <lang>` run locally (or in an added CI job) for every touched language |
| Objective-C `.h`/`.m`/`.mm` extension collision (Pitfall 5) | Objective-C language phase specifically | Explicit written decision in that phase's plan on `.h` handling, plus a dispatch unit test asserting no silent reassignment of existing C/C++ extensions |
| Groovy `.gradle` corpus variance (Pitfall 6) | Groovy language phase specifically | Explicit in/out-of-scope decision for `.gradle`; if in scope, at least one real-world Gradle-DSL corpus case exists and passes |
| Docs/bindings drift not caught in the same PR (Pitfall 7) | Every language phase's Definition of Done + the bindings-parity requirement (already a milestone-level requirement per PROJECT.md) | Manual line-by-line review of the language's docs row against actual extractor output; napi bindings regenerated-and-committed check; Python binding exposure manually verified (no automated gate exists yet) |

## Sources

- Repo-verified (HIGH confidence): `.planning/PROJECT.md`, `.planning/codebase/CONCERNS.md`, `CONTRIBUTING.md`, `src/grammar.rs`, `src/lang.rs`, `Cargo.toml`, `.github/workflows/test.yml` — all read directly from `/Users/habib/Git/NodeDB-Lab/nodedb-code2graph` on 2026-07-05.
- [tree-sitter-haskell Issue #34 — Unable to setup on mac](https://github.com/tree-sitter/tree-sitter-haskell/issues/34) — MEDIUM confidence (single GitHub issue; illustrates known scanner build fragility class, not necessarily current/unresolved state).
- [tree-sitter-haskell Issue #37 — Fails to build on Windows](https://github.com/tree-sitter/tree-sitter-haskell/issues/37) — MEDIUM confidence, same caveat.
- [tree-sitter/tree-sitter Issue #1246 — tree-sitter-cli does not allow C++11 language features in external scanners on macOS](https://github.com/tree-sitter/tree-sitter/issues/1246) — MEDIUM confidence, illustrates the general external-scanner/C++-standard class of problem across the ecosystem, not language-specific.
- [tree-sitter-ocaml on crates.io](https://crates.io/crates/tree-sitter-ocaml) — LOW-MEDIUM confidence (existence/description only; specific ABI/version compatibility with this project's `>=0.24, <0.27` pin not independently re-verified in this pass and must be re-checked at the start of the OCaml phase).
- [tree-sitter-objc on crates.io](https://crates.io/crates/tree-sitter-objc/2.1.0) — LOW-MEDIUM confidence, same caveat; version and ABI compatibility must be re-verified via the `abi_versions_are_compatible` check at feasibility-gate time, not assumed from this research pass.
- [tree-sitter-groovy on GitHub (murtaza64/tree-sitter-groovy)](https://github.com/murtaza64/tree-sitter-groovy) — LOW confidence on current maintenance status/ABI compatibility; last observed crates.io update November 2024 per search snapshot, re-verify freshness at feasibility-gate time.
- General ecosystem context on ABI/version mismatch as a known class of problem: [tree-sitter-language crate](https://crates.io/crates/tree-sitter-language) (built specifically to paper over tree-sitter crate version mismatches) — MEDIUM confidence, corroborates that this is a recognized, recurring ecosystem issue rather than a one-off risk.

---
*Pitfalls research for: tree-sitter grammar/extractor expansion (code2graph)*
*Researched: 2026-07-05*
