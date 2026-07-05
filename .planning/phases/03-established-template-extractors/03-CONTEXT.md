# Phase 3: Established-Template Extractors - Context

**Gathered:** 2026-07-05
**Status:** Ready for planning
**Mode:** `--auto` (recommended defaults selected; choices logged in 03-DISCUSSION-LOG.md)

<domain>
## Phase Boundary

Ship five new language extractors end-to-end — Zig, Objective-C, Fortran, Groovy, SystemVerilog (LANG-01, LANG-05, LANG-06, LANG-07, LANG-09) — each on its mapped in-repo template, with the phase's two explicit scope decisions (`.h` dispatch, `.gradle` inclusion) resolved below, not assumed. Grammars are already ABI-verified and registered (Phase 1). Bindings parity per language (same pattern Phase 2 established). No resolver changes.

</domain>

<decisions>
## Implementation Decisions

### Objective-C `.h` dispatch (the phase's flagged decision)
- **D-01:** Objective-C claims `.m` and `.mm` ONLY. Bare `.h` stays mapped to C — no content-sniffing, no dual dispatch. This is a documented honest gap (ObjC declarations in headers are extracted as C facts), recorded in the ObjC docs-row note and the extractor's module doc. Rationale: dispatch is extension-based by design; content-sniffing violates the determinism bar; C already owns `.h` as an accepted pre-existing ambiguity (C++ precedent).

### Groovy `.gradle` scoping (the phase's second flagged decision)
- **D-02:** `.gradle` files ARE dispatched to the Groovy extractor, parsed as plain Groovy — closures/method calls extracted as ordinary facts, with NO Gradle-DSL semantic modeling (no dependency-coordinate interpretation, no task-graph semantics). Documented ceiling in the docs-row note. Rationale: the docs matrix already lists `.gradle` under Groovy's planned extensions; plain-Groovy parse is honest and useful; DSL semantics would be guessing.

### Per-language extraction targets (table stakes + honest ceilings)
- **D-03:** Zig (template: C + Rust): `fn` definitions (incl. `pub` visibility — Zig has a real public/private signal), struct/enum/union declarations with member fns, `@import("x")` → Imports, call refs with receiver qualifiers on member calls, Read/Write. `comptime` constructs capped at table stakes (extract the declaration, don't evaluate).
- **D-04:** Objective-C (template: C + Swift): `@interface`/`@implementation`/`@protocol`/categories → class-kind symbols with inheritance (`: Base` and `<Protocol>` → IsImplementation), method declarations (+/- selectors as symbol names in selector form e.g. `doThing:withArg:`), message sends `[recv sel:arg]` → Calls with receiver qualifier, `#import`/`@import` → Imports, properties, C functions shared via C-like handling.
- **D-05:** Fortran (template: Pascal/Go): `module`/`program` → module symbols, `subroutine`/`function` (incl. `contains` nesting) → functions, `use` statements → Imports, `call` statements + function refs → Calls, explicit `public`/`private` statements → real Visibility (roadmap criterion). Free-form `.f90` is the target; fixed-form `.f` dispatches but is honestly capped at whatever the grammar yields (documented).
- **D-06:** Groovy (template: Java/Kotlin): classes/interfaces/traits/enums, methods (incl. `def`), fields/properties, `import` statements, calls (incl. paren-less command-expression calls where the AST is unambiguous), inheritance (`extends`/`implements`). Dynamic dispatch/`methodMissing` is a documented ceiling; visibility from modifiers, default-package-visibility honestly `Unknown` where Groovy's implicit-public rule is ambiguous — pick per Java template consistency.
- **D-07:** SystemVerilog (template: C): `module`/`interface`/`package`/`class` → symbols, functions/tasks, `import pkg::*` → Imports, `` `include `` → Imports (file-level), module instantiations → TypeRef, function/task calls → Calls. Extensions `.sv`/`.svh`. Simulation/synthesis semantics are out of scope.

### Wiring, bindings, sequencing (Phase 2 practice repeated per language)
- **D-08:** Full recipe per language: enum variant + `as_str()` + extension dispatch, extractor file reusing `support.rs`, `mod.rs`/`dispatch.rs` wiring, feature gains `_extractors` + flips into `default`, unit tests with real SCIP ids, ≥1 corpus case, docs row 🟠→🟢 (sync-tested), BOTH bindings feature lists in the same change, napi no-op diff verified per plan.
- **D-09:** One plan per language, sequential waves (all plans touch the shared wiring files: Cargo.toml, lang.rs, dispatch.rs, mod.rs, docs, bindings Cargo.tomls). Order by template proximity/risk: Zig → SystemVerilog → Fortran → Groovy → Objective-C (ObjC last — largest surface).
- **D-10:** Verification pattern per plan (established in Phase 2, resolver test-isolation gap deferred): `cargo check --no-default-features --features <lang>` + `cargo test --all-features` + fmt/clippy gates + napi no-op. The roadmap's literal "`cargo test --no-default-features --features <lang>`" criterion is satisfied the same way Phase 2's was judged: production code isolation via cargo check, tests via --all-features, with the pre-existing resolver-test-import gap explicitly referenced (deferred-items.md), unless a plan chooses to fix that gap once for all languages (planner's discretion if cheap).

### Claude's Discretion
- Real AST node names per grammar — MUST come from `to_sexp()` dumps against the exact pinned crate versions (research step), never guessed.
- Corpus case content per language (small, role-typed, `scoped_call` shape like Phase 2).
- Whether Fortran fixed-form `.f` gets its own corpus coverage (not required).
- ObjC category naming convention in SCIP descriptors.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Recipe & templates
- `CONTRIBUTING.md` §"Adding a Language" — recipe + AST-dump tip
- `src/extract/c.rs`, `src/extract/rust.rs` — Zig + SystemVerilog + ObjC(C side) templates
- `src/extract/swift.rs` — ObjC class/protocol shape template
- `src/extract/pascal.rs`, `src/extract/go.rs` — Fortran templates
- `src/extract/java.rs`, `src/extract/kotlin.rs` — Groovy templates
- `src/extract/powershell.rs` — freshest extractor (Phase 2), current best-practice shape
- `src/extract/support.rs` — mandatory shared helpers

### Phase 1/2 outputs
- `.planning/phases/01-foundation-compatibility-gate-ci-hardening-ts-js-depth/01-COMPAT-VERDICTS.md` — all five grammars ABI-verified PASS with pinned versions
- `.planning/phases/02-quick-win-extractors-astro-powershell/02-01-SUMMARY.md` — the end-to-end wiring pattern + bindings verify commands that worked
- `.planning/phases/02-quick-win-extractors-astro-powershell/deferred-items.md` — resolver test-isolation gap (pre-existing, referenced by D-10)

### Validation & docs & bindings
- `eval/corpus/powershell/scoped_call/` — corpus case shape to copy
- `docs/supported-languages.md` — rows to move 🟠→🟢; ObjC/Groovy notes carry D-01/D-02 decisions
- `bindings/node/Cargo.toml`, `bindings/python/Cargo.toml` — feature lists (BIND practice)
- `.planning/research/FEATURES.md`, `.planning/research/ARCHITECTURE.md` — capability targets and template mapping

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- Grammar fns + ABI arms for all five languages already in `src/grammar.rs` (Phase 1)
- `feature-isolation` CI job auto-covers the five features (matrix already lists them)
- Phase 2's napi verify flow (`npm ci` done; `npx napi build --release --platform` + `git diff --exit-code`)

### Established Patterns
- TDD RED→GREEN commits per extractor (Phase 2 practice)
- Append-only edits to shared wiring files, sequential waves
- Docs sync tests fire on enum variants without matching docs rows — same-change docs updates

### Integration Points
- `Cargo.toml` (feature defs + default list), `src/lang.rs`, `src/extract/{mod,dispatch}.rs`, new `src/extract/{zig,objc,fortran,groovy,systemverilog}.rs` (match existing file-naming: check how features were named in Phase 1 — `objc` feature name), `eval/corpus/`, `docs/supported-languages.md`, `bindings/{node,python}/Cargo.toml`

</code_context>

<specifics>
## Specific Ideas

- Roadmap success criteria per language are explicit (see ROADMAP.md Phase 3) — plans' must_haves derive from them 1:1.
- Fortran is the one language with REAL visibility extraction required (public/private statements) — don't cap it at Unknown.

</specifics>

<deferred>
## Deferred Ideas

- ObjC `.h` content-sniffing or dual-dispatch — rejected for determinism; revisit only as a project-level decision
- Gradle DSL semantic modeling (dependency coordinates, task graph) — potential future `src/package/` enrichment, not extraction
- Fixing the pre-existing resolver test-module isolation gap for all languages (tracked in Phase 2 deferred-items.md; planner MAY pull it in if trivial)
- SystemVerilog elaboration/parameterization semantics

</deferred>

---

*Phase: 03-established-template-extractors*
*Context gathered: 2026-07-05*
