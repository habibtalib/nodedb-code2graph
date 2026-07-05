# Phase 2: Quick-Win Extractors — Astro & PowerShell - Context

**Gathered:** 2026-07-05
**Status:** Ready for planning
**Mode:** `--auto` (recommended defaults selected; choices logged in 02-DISCUSSION-LOG.md)

<domain>
## Phase Boundary

Ship the Astro and PowerShell extractors end-to-end — grammar (already ABI-verified in Phase 1), `Language` enum + dispatch, extractor, unit tests, corpus case, docs row, and both bindings feature lists with a verified no-op napi diff (LANG-08, LANG-10, BIND-01, BIND-02). No other languages; no resolver changes.

</domain>

<decisions>
## Implementation Decisions

### PowerShell extractor (LANG-08)
- **D-01:** Template: `src/extract/shell.rs` (near-1:1 per research). Extensions: `.ps1`, `.psm1` only — `.psd1` is a data manifest, not code (documented exclusion).
- **D-02:** Emit: function definitions (`function Verb-Noun { }`, including `filter`), PS 5+ `class` definitions with methods/properties, imports (`Import-Module`, `using module`, dot-sourcing `. ./file.ps1`), calls in BOTH forms — cmdlet-style command invocation (`Verb-Noun -Arg x`) and expression-style member calls (`$obj.Method()`, receiver captured as qualifier) — plus variable Read/Write.
- **D-03:** `Visibility` is honestly `Unknown` (no in-language public/private signal); do NOT infer from `Export-ModuleMember`. `Invoke-Expression` and `& $scriptBlock` are a documented unresolved dynamic-invocation ceiling — never guessed.

### Astro extractor (LANG-10)
- **D-04:** Embedded-SFC pattern, reference implementation `src/extract/svelte.rs`: parse the host `.astro` document with tree-sitter-astro-next, locate the frontmatter fence (`---` … `---`) AND any `<script>` tag contents, run `super::typescript::extract_ecmascript` on each, remap offsets via `support::shift_offsets`.
- **D-05:** The `astro` Cargo feature transitively enables `typescript` (exactly like the existing `svelte = [..., "typescript", ...]`).

### Wiring & docs (both languages)
- **D-06:** Full recipe per language: `Language::PowerShell` / `Language::Astro` enum variants, `as_str()` arms, extension dispatch, `src/extract/mod.rs` + `dispatch.rs` wiring, unit tests asserting real rendered SCIP id strings (derive from an existing extractor's tests), ≥1 `eval/corpus/<lang>/` golden case with `expected.edges`, and the `docs/supported-languages.md` row moved 🟠→🟢 with capability columns filled honestly (sync-test guarded).
- **D-07:** Flip `powershell` and `astro` INTO the `default` feature list when their extractors land (matrix convention: supported languages on by default). Their features gain `_extractors` and the enum/dispatch code in the same change.

### Bindings parity (BIND-01, BIND-02)
- **D-08:** Add `"powershell"` and `"astro"` to the explicit `features = [...]` lists in `bindings/node/Cargo.toml` AND `bindings/python/Cargo.toml` in the same change that flips each language into default (the one unguarded integration point — research-confirmed).
- **D-09:** Regenerate napi artifacts (`npx napi build --release --platform` in `bindings/node`) and verify the committed `index.js`/`index.d.ts` diff is a no-op (enum-generic bindings — signatures don't change); run whatever bindings CI check exists locally before completing the phase.

### Claude's Discretion
- Exact tree-sitter node names for both grammars — MUST be verified with a real `to_sexp()` AST dump against the exact crate versions before writing extractor code (CONTRIBUTING tip; published grammars differ from repo node-types.json).
- Corpus case content (keep it small but role-typed).
- Whether PowerShell classes emit Inherit edges (`class B : A`) — include if the AST makes it unambiguous, else leave the column blank honestly.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Recipe & templates
- `CONTRIBUTING.md` §"Adding a Language" — the 6-step recipe + AST-dump tip + embedded-SFC pattern
- `src/extract/shell.rs` — PowerShell's structural template
- `src/extract/svelte.rs` — Astro's reference implementation (embedded pattern, `shift_offsets`)
- `src/extract/support.rs` — mandatory shared helpers
- `src/lang.rs`, `src/extract/dispatch.rs`, `src/extract/mod.rs` — wiring seams

### Phase 1 outputs (already landed)
- `.planning/phases/01-foundation-compatibility-gate-ci-hardening-ts-js-depth/01-COMPAT-VERDICTS.md` — both grammars ABI-verified PASS with pinned versions
- `Cargo.toml` — `powershell`/`astro` features exist as grammar-only, non-default
- `src/grammar.rs` — grammar fns + ABI arms already registered

### Validation & docs
- `eval/corpus/` — golden-fixture layout (auto-discovered); `CONTRIBUTING.md` §Validation
- `docs/supported-languages.md` — rows to move 🟠→🟢 (sync-tested against `src/lang.rs`)

### Bindings
- `bindings/node/Cargo.toml`, `bindings/python/Cargo.toml` — explicit feature lists (BIND-01)
- `.github/workflows/test.yml` — bindings job + napi drift gate to mirror locally (BIND-02)
- `.planning/research/ARCHITECTURE.md` — bindings parity mechanics

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- Grammar registration + ABI verification done in Phase 1 — extractor phases start at the enum/extractor steps
- `extract_ecmascript` (now with entry-point detection from Phase 1) is the engine Astro reuses
- Feature-isolation CI job (Phase 1) will automatically cover `powershell`/`astro` once flipped

### Established Patterns
- Sync tests fire when enum variants land without docs rows — docs must change in the same commits
- Bindings are enum-generic: only Cargo.toml feature lists change (no binding source code)

### Integration Points
- `Cargo.toml` (default list + feature defs), `src/lang.rs`, `src/grammar.rs` (already done), `src/extract/{mod,dispatch}.rs`, new `src/extract/{powershell,astro}.rs`, `eval/corpus/`, `docs/supported-languages.md`, `bindings/{node,python}/Cargo.toml`

</code_context>

<specifics>
## Specific Ideas

- Roadmap success criteria are explicit: PS cmdlet-style AND expression-style calls; Astro offsets shifted back to host file; isolated-feature tests pass; napi no-op diff; corpus + docs rows present.

</specifics>

<deferred>
## Deferred Ideas

- PowerShell `.psd1` manifest parsing for package enrichment — potential future `src/package/` work, not extraction
- Astro template-expression extraction beyond script/frontmatter (e.g. `{expr}` in markup) — depth work for a later milestone
- 3-OS bindings CI matrix (DEPTH-03, v2)

</deferred>

---

*Phase: 02-quick-win-extractors-astro-powershell*
*Context gathered: 2026-07-05*
