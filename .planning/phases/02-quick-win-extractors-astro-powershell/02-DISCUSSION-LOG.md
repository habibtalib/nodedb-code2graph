# Phase 2: Quick-Win Extractors — Astro & PowerShell - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-05
**Phase:** 2 — Quick-Win Extractors: Astro & PowerShell
**Areas discussed:** PowerShell extraction scope, Astro extraction scope, default-feature flip, bindings parity mechanics
**Mode:** `--auto` — all gray areas auto-selected; recommended option chosen per question and logged below.

---

## PowerShell extraction scope

| Option | Description | Selected |
|--------|-------------|----------|
| Table stakes + honest ceilings | functions/classes/imports/both call forms/read-write; Visibility Unknown; dynamic invocation documented as ceiling | ✓ |
| Functions + calls only | Narrowest useful slice | |
| Infer visibility from Export-ModuleMember | Cross-artifact visibility inference | |

`[auto]` Q: "How deep should PowerShell extraction go?" → Selected: "Table stakes + honest ceilings" (matches roadmap success criterion 1 verbatim; Export-ModuleMember inference rejected — cross-artifact visibility is a documented v2 ceiling per FEATURES research). Extensions locked to `.ps1`/`.psm1`; `.psd1` excluded as data.

## Astro extraction scope

| Option | Description | Selected |
|--------|-------------|----------|
| Frontmatter + script tags via TS engine | Svelte-pattern embedded extraction with shift_offsets | ✓ |
| Frontmatter only | Skip `<script>` tags | |
| Full template expressions | Also extract `{expr}` in markup | |

`[auto]` Q: "What Astro content gets extracted?" → Selected: "Frontmatter + script tags via TS engine" (both are real TS/JS; matches Svelte reference implementation; template expressions deferred). `astro` feature transitively enables `typescript` per existing svelte precedent.

## Default-feature flip

| Option | Description | Selected |
|--------|-------------|----------|
| Flip into default with extractor | Supported languages are on by default (matrix convention) | ✓ |
| Keep non-default until milestone end | Batch the flip | |

`[auto]` Q: "When do powershell/astro join the default feature list?" → Selected: "Flip into default with extractor" (docs matrix states all supported languages on by default; sync tests expect enum+docs+feature coherence per language change).

## Bindings parity mechanics

| Option | Description | Selected |
|--------|-------------|----------|
| Same-change feature lists + napi no-op verify | Add to both bindings Cargo.tomls in the flip commit; regen napi and assert no-op diff | ✓ |
| Trailing bindings commit at phase end | Batch bindings updates | |

`[auto]` Q: "How is BIND-01/02 satisfied?" → Selected: "Same-change feature lists + napi no-op verify" (BIND-01 wording requires same-change; the napi drift CI gate makes no-op verification mandatory anyway).

## Claude's Discretion

- Real AST node names via `to_sexp()` dump before extractor code (mandatory CONTRIBUTING tip)
- Corpus case content (small, role-typed)
- PowerShell class Inherit edges only if AST-unambiguous

## Deferred Ideas

- `.psd1` manifest parsing (package enrichment, not extraction)
- Astro template-expression depth
- 3-OS bindings CI matrix (DEPTH-03, v2)
