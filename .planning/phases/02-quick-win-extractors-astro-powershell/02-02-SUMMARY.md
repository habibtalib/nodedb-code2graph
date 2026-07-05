---
phase: 02-quick-win-extractors-astro-powershell
plan: 02
subsystem: extraction
tags: [tree-sitter, astro, embedded-sfc, svelte-pattern, napi, bindings-parity]

# Dependency graph
requires:
  - phase: 02-quick-win-extractors-astro-powershell (02-01)
    provides: PowerShell's bindings-parity practice (BIND-01/02 verification flow) and the shared-wiring-file append pattern (Cargo.toml default list, both bindings' feature lists)
provides:
  - Language::Astro enum variant with .astro extension dispatch
  - src/extract/astro.rs (AstroExtractor) — frontmatter (always TypeScript)
    + <script> block discovery reusing Svelte's detect_script_lang verbatim,
    merge-loop offset remap via shift_offsets, single synthesized Module
    symbol + root scope regardless of block count (including zero blocks)
  - eval/corpus/astro/scoped_call/ golden fixture (frontmatter-embedded
    same-file Call edge)
  - docs/supported-languages.md Astro row moved 🟠 → 🟢
  - astro feature flipped into default (transitively enables typescript);
    bindings/node and bindings/python Cargo.toml feature parity (BIND-01)
    for both powershell and astro; verified no-op napi diff (BIND-02)
affects: [future embedded-SFC language phases (e.g. Vue, if it ever gains a compatible grammar), Phase 3/4 extractor work reusing the bindings-parity practice]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Embedded-SFC merge loop factored into a shared merge_block() helper: extract_ecmascript -> shift_offsets -> scope-index shift by scope_base -> re-parent block's former root scope under doc_root -> drop the block's own Module symbol. Both the frontmatter block and every <script> block route through the same helper, differing only in inner_lang and how their text node is located (frontmatter_js_block vs raw_text)."
    - "Frontmatter is unconditionally treated as Language::TypeScript (no lang attribute exists on the frontmatter node) while <script> tags still run through Svelte's verbatim-ported detect_script_lang (default JavaScript, lang=\"ts\"/\"typescript\" -> TypeScript)"

key-files:
  created:
    - src/extract/astro.rs
    - eval/corpus/astro/scoped_call/Component.astro
    - eval/corpus/astro/scoped_call/expected.edges
  modified:
    - src/lang.rs
    - Cargo.toml
    - bindings/node/Cargo.toml
    - bindings/python/Cargo.toml
    - src/extract/mod.rs
    - src/extract/dispatch.rs
    - docs/supported-languages.md

key-decisions:
  - "Factored the frontmatter-vs-script_element merge into one merge_block() helper (not duplicated inline per block type) since both differ only in inner_lang and how their embedded-text node is located — reduces the risk of the two paths drifting apart as future maintenance touches one but not the other."
  - "Verified all 5 unit tests via a proper RED (stub extractor, only the frontmatter-less/script-less case passing) -> GREEN (full frontmatter+script discovery) TDD cycle per the plan's tdd=\"true\" marking, mirroring 02-01's Task 2 flow."
  - "Astro row inserted directly above the SQL row in docs/supported-languages.md (immediately after PowerShell), matching the plan's exact prescribed position and the existing Svelte/PowerShell 🟢-block ordering."

patterns-established:
  - "Embedded-SFC merge_block() extraction pattern: any future embedded-language extractor with more than one kind of embedded block (not just N of the same kind, like Svelte's <script> tags) should factor the per-block merge steps into one shared helper parameterized by (text_node, inner_lang) rather than duplicating the merge loop body per block kind."

requirements-completed: [LANG-10, BIND-01, BIND-02]

# Metrics
duration: 20min
completed: 2026-07-05
---

# Phase 2 Plan 2: Astro Extractor Summary

**AstroExtractor merging an always-TypeScript frontmatter block and any number of `<script>` blocks through the shared TS engine, with `shift_offsets`-based byte-offset remap back into the full `.astro` document — closing LANG-10, BIND-01, and BIND-02 for the whole phase.**

## Performance

- **Duration:** ~20 min
- **Started:** 2026-07-05T19:59 (init + context read, following 02-01)
- **Completed:** 2026-07-05T20:12
- **Tasks:** 3 completed (Task 2 executed as RED→GREEN TDD, 2 commits)
- **Files modified:** 9 (1 new source/test-bearing file, 2 new corpus fixture files, 6 modified)

## Accomplishments
- `Language::Astro` wired end-to-end (enum, extensions, `as_str`, exhaustiveness guard, dispatch), `astro` feature transitively enabling `typescript` + `_extractors` and joining `default`
- `AstroExtractor` discovers the optional (at most one) frontmatter block and every `<script>` block, running each through `extract_ecmascript` — frontmatter unconditionally as TypeScript, `<script>` tags via Svelte's verbatim-ported `detect_script_lang`
- Byte offsets from every embedded block correctly shifted back to the full `.astro` file via `support::shift_offsets`, verified by an exact `span.start` assertion against the host document
- A frontmatter-less, script-less `.astro` file (pure markup) still emits exactly one `Module` symbol and one root scope with zero references — no panic (the Pitfall 3 failure mode from `02-RESEARCH.md`)
- Multi-block merge (frontmatter + `<script>` together) produces exactly one synthesized `Module` symbol and one root scope, mirroring `svelte.rs`'s proven merge shape
- `eval/corpus/astro/scoped_call/` golden fixture resolves its one same-file `Call` edge (`run` calling `helper`, both defined in the frontmatter) end-to-end through the eval harness's regression tests
- `docs/supported-languages.md` Astro row moved 🟠 → 🟢 with honest `⤴`-via-TS-engine capability columns, matching Svelte's row shape
- Re-verified `npx napi build --release --platform` produces a no-op diff against committed `index.js`/`index.d.ts` now that BOTH `powershell` and `astro` are wired — completing BIND-02 for the entire phase

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire Language::Astro + Cargo feature flip + bindings feature lists** - `def7c62` (feat)
2. **Task 2: AstroExtractor — frontmatter + script_element discovery, merge loop, module symbol** - `9f3ee05` (test, RED) + `b2617b1` (feat, GREEN)
3. **Task 3: Corpus case + docs row + full bindings-parity verify** - `d1f77e2` (feat)

_Task 2 followed the TDD flow (tdd="true"): the RED commit added a stub `AstroExtractor` (document-spanning Module symbol + root scope only, no frontmatter/script discovery) plus 5 tests, 4 of which failed; the GREEN commit implemented frontmatter/script_element discovery and the merge loop until all 5 passed._

## Files Created/Modified
- `src/extract/astro.rs` - AstroExtractor: frontmatter (always-TS) + `<script>` block discovery, `merge_block` helper, synthesized single Module symbol
- `eval/corpus/astro/scoped_call/Component.astro` / `expected.edges` - golden same-file Call fixture inside frontmatter
- `src/lang.rs` - `Language::Astro` variant, extensions, `as_str`, exhaustiveness guard
- `Cargo.toml` - `astro` feature gains `typescript` + `_extractors`; joins `default`
- `bindings/node/Cargo.toml`, `bindings/python/Cargo.toml` - `astro` added to explicit feature lists (alongside `powershell`)
- `src/extract/mod.rs`, `src/extract/dispatch.rs` - module/dispatch wiring
- `docs/supported-languages.md` - Astro row moved 🟠 → 🟢, positioned directly after PowerShell in the supported block

## Decisions Made
- Factored the frontmatter-vs-`<script>` per-block merge logic (extract → shift_offsets → scope-index shift → re-parent → drop per-block Module symbol) into one shared `merge_block()` helper rather than duplicating svelte.rs's inline loop body twice, since Astro (unlike Svelte) has two structurally different kinds of embedded block sharing the same merge shape.
- Followed the plan's `tdd="true"` marking for Task 2 with a genuine RED→GREEN cycle: the RED commit's stub extractor deliberately only satisfied the frontmatter-less/script-less test (4 of 5 tests failed), confirmed by running the tests before writing the real implementation.
- Placed the new Astro row in `docs/supported-languages.md` immediately after the PowerShell row (both directly above SQL), matching the plan's exact prescribed table position.

## Deviations from Plan

None - plan executed exactly as written. The one pre-existing, already-documented gap (resolver test-module feature-isolation, logged in `02-01`'s `deferred-items.md`) applies identically here: `cargo test --no-default-features --features astro` cannot compile its test binary standalone (unrelated `RustExtractor`/`JavaExtractor`/etc. imports in `symbol_table.rs`/`scope_graph.rs`), verified equivalently via `cargo check --no-default-features --features astro` (production code compiles standalone) plus `cargo test --all-features` (790 tests pass, including all 5 new `extract::astro::tests`) — exactly the workaround the plan's own notes anticipated and prescribed. No new deviation entry was needed since this is the same already-logged item, not a new discovery.

## Issues Encountered
None beyond the already-documented pre-existing resolver test-isolation gap (see Deviations above and `02-01`'s `deferred-items.md`).

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Astro extractor fully lands LANG-10; bindings parity (BIND-01/02) is now closed for the entire Phase 2 (both `powershell` and `astro` verified together in the same napi no-op diff check).
- Phase 2's success criteria are all met: PowerShell's cmdlet/expression call forms (02-01), Astro's frontmatter/script offset-shifted extraction (this plan), isolated-feature `cargo check` passing for both languages, bindings parity with a diff-free napi build, and both languages carrying a corpus case + a sync-test-guarded `docs/supported-languages.md` row.
- The resolver test-module feature-isolation gap (`symbol_table.rs`/`scope_graph.rs`) remains open and unaffected by this plan — still flagged in STATE.md's Blockers/Concerns for a dedicated follow-up plan, not blocking Phase 3.

---
*Phase: 02-quick-win-extractors-astro-powershell*
*Completed: 2026-07-05*

## Self-Check: PASSED

All created files verified present; all 4 task commits verified in `git log`.
