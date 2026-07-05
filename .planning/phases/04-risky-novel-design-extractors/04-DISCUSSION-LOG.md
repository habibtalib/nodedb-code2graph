# Phase 4: Risky & Novel-Design Extractors - Discussion Log

> **Audit trail only.** Decisions live in CONTEXT.md.

**Date:** 2026-07-05
**Phase:** 4 — Risky & Novel-Design Extractors (Julia, R, OCaml, F#, Elixir, Erlang, Gleam, Haskell)
**Areas discussed:** LANG-12 scope, per-language ceilings, sequencing, arity identity
**Mode:** `--auto` — recommended options selected and logged.

---

## LANG-12 scope

| Option | Description | Selected |
|--------|-------------|----------|
| All four BEAM/Haskell members | Elixir, Erlang, Gleam, Haskell all passed Phase 1's empirical gate | ✓ |
| Subset | Defer some members | |

`[auto]` Q: "Which LANG-12 members are attempted?" → Selected: "All four" (01-COMPAT-VERDICTS.md records PASS for every candidate; roadmap criterion 5 conditions inclusion only on the gate).

## Per-language ceilings

`[auto]` Selected the honest-ceiling defaults per research FEATURES.md and roadmap criteria: Julia multiple dispatch stays NameOnly fan-out; R visibility Unknown + NSE never attempted; OCaml `.mli` correlation out of scope; F# reuses ML template; real visibility extracted where the language has a clean signal (Elixir def/defp, Erlang -export, Gleam pub, Haskell export lists).

## Sequencing

| Option | Description | Selected |
|--------|-------------|----------|
| OCaml → F# → Gleam → Elixir → Erlang → Haskell → Julia → R | ML template first (F# depends on it), cleaner BEAM next, novel-dispatch last | ✓ |
| Roadmap listing order | Julia first | |

`[auto]` Q: "Order?" → Selected: ML-first ordering (roadmap criterion 4 mandates F# reuse the OCaml template validated in this phase).

## Arity identity (Elixir/Erlang)

| Option | Description | Selected |
|--------|-------------|----------|
| Claude's discretion, documented | Include /arity in SCIP descriptor only if scheme accommodates cleanly, else name-only + arity in signature | ✓ |
| Force /arity in descriptors | May break SCIP rendering conventions | |

`[auto]` Q: "Is arity part of symbol identity?" → Selected: "Claude's discretion, documented" (novel territory; the implementer sees the real SCIP renderer behavior).

## Deferred Ideas

- `.ml`↔`.mli` correlation; R NAMESPACE visibility; Julia dispatch modeling; Haskell point-free depth; 3-OS bindings CI.
