# Phase 3: Established-Template Extractors - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-05
**Phase:** 3 — Established-Template Extractors (Zig, Objective-C, Fortran, Groovy, SystemVerilog)
**Areas discussed:** ObjC `.h` dispatch, Groovy `.gradle` scoping, per-language depth, plan sequencing
**Mode:** `--auto` — all gray areas auto-selected; recommended option chosen per question and logged below.

---

## Objective-C `.h` dispatch

| Option | Description | Selected |
|--------|-------------|----------|
| `.m`/`.mm` only, `.h` stays C | Documented honest gap; deterministic extension dispatch | ✓ |
| Content-sniff `.h` files | Route headers by content heuristics | |
| Dual-dispatch `.h` to both | Emit facts from both extractors | |

`[auto]` Q: "Who owns bare `.h`?" → Selected: "`.m`/`.mm` only, `.h` stays C" (content-sniffing violates the determinism bar; C already owns `.h` as an accepted ambiguity — C++ precedent; roadmap requires the decision be documented, not any particular answer).

## Groovy `.gradle` scoping

| Option | Description | Selected |
|--------|-------------|----------|
| In scope as plain Groovy | Dispatch `.gradle` → Groovy extractor, no DSL semantics (documented ceiling) | ✓ |
| Defer `.gradle` entirely | `.groovy` only this milestone | |
| Model Gradle DSL semantics | Dependency coordinates, task graph | |

`[auto]` Q: "Are `.gradle` files in scope?" → Selected: "In scope as plain Groovy" (docs matrix already lists `.gradle` under Groovy; plain parse is honest and useful; DSL modeling would be guessing and belongs to package enrichment if ever).

## Per-language depth

`[auto]` Q: "Depth per language?" → Selected: table stakes + real capabilities where the syntax is unambiguous (Zig pub visibility; Fortran REAL public/private visibility per roadmap criterion 3; ObjC selector-form method names + message-send qualifiers; Groovy inheritance; SV instantiations as TypeRef), honest ceilings documented (comptime, dynamic dispatch, fixed-form Fortran, SV elaboration).

## Plan sequencing

| Option | Description | Selected |
|--------|-------------|----------|
| One plan per language, sequential waves | Shared wiring files force ordering; Zig → SystemVerilog → Fortran → Groovy → ObjC | ✓ |
| One mega-plan | All five in one plan | |
| Parallel waves with worktrees | Isolation overhead | |

`[auto]` Q: "Plan structure?" → Selected: "One plan per language, sequential waves" (Phase 2 proved the append-only sequential pattern; ObjC last as largest surface).

## Claude's Discretion

- AST node names from `to_sexp()` dumps only
- Corpus case content; Fortran `.f` coverage optional
- ObjC category descriptor convention

## Deferred Ideas

- ObjC `.h` content-sniffing (rejected; project-level decision if ever)
- Gradle DSL semantics (package enrichment territory)
- Resolver test-module isolation fix (planner MAY pull in if trivial)
- SystemVerilog elaboration semantics
