---
phase: 01-foundation-compatibility-gate-ci-hardening-ts-js-depth
plan: 04
subsystem: extract
tags: [typescript, javascript, tree-sitter, entry-points, http-routes, nestjs, express]

# Dependency graph
requires: []
provides:
  - TS_ROUTE_VERBS + attach_call_entry_points — call-terminal HTTP verb detection (Express/Fastify/Koa/Hono) attaching EntryPoint::HttpRoute to named top-level handler Symbols
  - TS_ROUTE_DECORATORS + route_decorators_on — decorator-terminal detection (NestJS @Get/@Post/@Put/@Delete/@Patch/@Controller) aggregated onto the enclosing class Symbol
  - docs/supported-languages.md TypeScript row's Entry-pts cell filled in (blank → ✓)
affects: [phase-2-language-extractors, resolvers-consuming-entry-points]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Call-terminal verb match ported from Python's PY_ROUTE_VERBS to TS's call_expression/member_expression AST shape"
    - "Decorator-terminal match ported from Java's JAVA_ROUTE_ANNOTATIONS to TS decorator syntax, aggregated onto the class Symbol since TS has no per-method Symbol"

key-files:
  created: []
  modified:
    - src/extract/typescript.rs
    - docs/supported-languages.md

key-decisions:
  - "Aggregated both class-level (@Controller) and method-level (@Get/@Post/...) NestJS decorator markers onto the enclosing class's existing Symbol rather than fabricating a per-method Symbol — matches Phase 1's explicit boundary against new-language-extractor-scale changes"
  - "Deliberately excluded .use() from TS_ROUTE_VERBS (generic Express/Koa middleware registration, not HTTP routing) and inline/anonymous handlers (no per-call or per-inline-function Symbol exists to attach a marker to) — both are honest, documented ceilings, not gaps"

patterns-established:
  - "route_decorators_on(node, bytes) scans node's direct `decorator` children (works identically whether `node` is an export_statement, a bare class_declaration, or a class_body) — no sibling-pairing logic needed since aggregation targets the class Symbol regardless of which method a decorator precedes"

requirements-completed: [TSADAPT-01]

# Metrics
duration: 6min
completed: 2026-07-05
---

# Phase 01 Plan 04: TypeScript/JavaScript Entry-Point Detection Summary

**Ported Python's call-terminal route-verb match and Java's decorator-terminal annotation match to TypeScript's verified AST shapes, closing the last entry-point-detection gap on two ⭐/🟢 languages (TS and JS, via the shared `extract_ecmascript` core) with zero grammar risk.**

## Performance

- **Duration:** 6 min
- **Started:** 2026-07-05T18:23:31+08:00
- **Completed:** 2026-07-05T18:29:36+08:00
- **Tasks:** 3 completed
- **Files modified:** 2

## Accomplishments
- Express/Fastify/Koa/Hono named-handler verb calls (`router.get('/users', getUsers)`) now attach `EntryPoint::HttpRoute("router.get")` to the real handler `Symbol`, on both `.ts` and `.js` files via the shared `extract_ecmascript` path.
- NestJS class-level (`@Controller`) and method-level (`@Get`/`@Post`/`@Put`/`@Delete`/`@Patch`) decorators both resolve to markers aggregated onto the enclosing class's existing `Symbol` — no new per-method `Symbol` kind was introduced.
- `.use()` middleware registration and inline/anonymous handlers are deliberately, documentedly NOT detected — an honest ceiling captured both in code comments and in `docs/supported-languages.md`.
- TypeScript's `Entry-pts` docs cell flips from blank to `✓`; full workspace + doc test suite verified green (773 lib tests, 11 eval tests, 10 regression tests, 2 doc tests — all passing).

## Task Commits

Each task was committed atomically:

1. **Task 1: Call-terminal verb matching for Express/Fastify/Koa/Hono** - `becf780` (feat)
2. **Task 2: NestJS decorator matching — class-level and method-level, aggregated onto the class Symbol** - `b8acba6` (feat)
3. **Task 3: Fill the TypeScript Entry-pts docs cell and run full verification** - `5ccab67` (docs)

_Note: TDD tasks (Task 1 and Task 2) had their RED/GREEN cycles folded into single commits per task — each commit's test additions and implementation were verified together before committing (all 9 new tests + full regression passed before each commit), rather than splitting into separate test-then-feat commits._

## Files Created/Modified
- `src/extract/typescript.rs` - Added `TS_ROUTE_VERBS`, `attach_call_entry_points`, `call_route_match`, `last_arg_bare_identifier` (Task 1); `TS_ROUTE_DECORATORS`, `route_decorators_on`, and the `collect_symbols` wiring for both the `export_statement` and bare `BARE_DECL_KINDS` arms (Task 2); 9 new unit tests covering both detectors and their ceilings
- `docs/supported-languages.md` - TypeScript row's `Entry-pts` cell blank → `✓`; new note in the `## Entry-points` section documenting the two TS/JS detection patterns and their honest ceilings

## Decisions Made
- Ported Python's `PY_ROUTE_VERBS`/`entry_points_for` call-terminal pattern almost verbatim to TS's `call_expression`/`member_expression` shape, adding `route`/`websocket`/`ws`/`all` to match the verified terminal-verb set already used by the plan's interface spec.
- Ported Java's `JAVA_ROUTE_ANNOTATIONS`/`entry_points_for_java` decorator-terminal pattern to TS decorator syntax, but simplified the implementation: rather than pairing each `decorator` child with its immediately-following `method_definition` sibling (as the plan's action section described), `route_decorators_on(node, bytes)` scans `node`'s direct `decorator` children unconditionally — since all matches aggregate onto the same class `Symbol` regardless of which method a decorator precedes, no sibling-pairing was needed. Verified via an AST spike (`tree.root_node().to_sexp()`) that both export_statement's own `decorator:` field and class_body's `decorator` children are direct children reachable this way, for both exported and bare (non-exported) decorated classes.

## Deviations from Plan

None - plan executed exactly as written. The one implementation simplification (no sibling-pairing logic) achieves the same test-verified behavior as the plan's described approach with less code, since aggregation targets the class Symbol either way.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

TypeScript/JavaScript entry-point detection is complete and matches the coverage depth of Rust/Python/Go/Java. This closes requirement TSADAPT-01 and the last depth gap identified for Phase 1's two highest-trust rows. No blockers for subsequent phases (new-language-extractor work in Phase 2+).

---
*Phase: 01-foundation-compatibility-gate-ci-hardening-ts-js-depth*
*Completed: 2026-07-05*

## Self-Check: PASSED

- FOUND: src/extract/typescript.rs
- FOUND: docs/supported-languages.md
- FOUND: becf780 (Task 1 commit)
- FOUND: b8acba6 (Task 2 commit)
- FOUND: 5ccab67 (Task 3 commit)
