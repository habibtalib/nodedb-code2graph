# Phase 1: Foundation — Compatibility Gate, CI Hardening & TS/JS Depth - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-05
**Phase:** 1 — Foundation: Compatibility Gate, CI Hardening & TS/JS Depth
**Areas discussed:** Compat-spike mechanics, Verdict recording, CI hardening shape, TS/JS entry-point design
**Mode:** `--auto` — all gray areas auto-selected; recommended option chosen per question and logged below.

---

## Compat-spike mechanics

| Option | Description | Selected |
|--------|-------------|----------|
| In-repo incremental wire-ups | Add optional dep + feature + grammar fn + ABI arm per candidate; passing ones stay (foundation for Phases 2–4), failing ones reverted with verdict recorded | ✓ |
| Throwaway scratch crate | Verify outside the repo; re-wire later | |
| Spike branch, discarded | Wire everything on a branch, keep only the report | |

`[auto]` Q: "Where do the 15 grammar wire-ups live?" → Selected: "In-repo incremental wire-ups" (matches the roadmap success criterion of running the repo's own ABI test per feature; avoids re-doing work in Phases 2–4). Follow-up locked: candidate features stay out of `default` until their extractor lands.

## Verdict recording

| Option | Description | Selected |
|--------|-------------|----------|
| Phase artifact + docs notes | Full 15-row verdict table in 01-COMPAT-VERDICTS.md; failures also documented in docs/supported-languages.md per CONTRIBUTING | ✓ |
| Docs only | Only update the public matrix | |

`[auto]` Q: "Where do verdicts live?" → Selected: "Phase artifact + docs notes" (COMPAT-02 requires the docs entries; the phase artifact gives Phases 2–4 a single authoritative gate report). Includes research-verified status corrections: F# unblocked; Vue/Apex/Liquid/COBOL precise blocked reasons.

## CI hardening shape

| Option | Description | Selected |
|--------|-------------|----------|
| Loop over ALL language features | `cargo check --no-default-features --features <lang>` per feature, ubuntu-only, in test.yml | ✓ |
| New languages only | Check only candidates added this milestone | |
| Full per-language matrix job | One matrix entry per language, heavier | |

`[auto]` Q: "Scope of the isolated-build CI check?" → Selected: "Loop over ALL language features" (closes the pre-existing gap for shipped languages too; `cargo check` keeps cost low).

## TS/JS entry-point design

| Option | Description | Selected |
|--------|-------------|----------|
| Verb-calls + NestJS decorators | Call-terminal matching (Express/Fastify/Koa/Hono) + decorator-terminal matching (NestJS), `HttpRoute(raw marker)`, syntax-only | ✓ |
| Express only | Narrowest scope | |
| Broader heuristics | Framework-config parsing, type-aware detection | |

`[auto]` Q: "Which entry-point detection patterns?" → Selected: "Verb-calls + NestJS decorators" (both mirror proven in-repo precedents — Python route verbs, Java annotations; broader heuristics would fake precision, rejected by project policy). Follow-up locked: implement in shared `extract_ecmascript`; no `Main` detection for TS/JS.

## Claude's Discretion

- Exact verb list (err toward precision; `all`/`use` only if unambiguous)
- CI loop implementation style (shell loop vs small matrix)
- Entry-point helper placement/naming in the TS extractor

## Deferred Ideas

- Bindings CI 3-OS matrix (DEPTH-03, v2)
- Python-side bindings drift gate (v2)
- Corpus backfill for shipped 🟢 languages (DEPTH-01, v2)
