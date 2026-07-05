# Stack Research — Grammar Crate Compatibility for Language Expansion

**Domain:** tree-sitter grammar crate availability/compatibility for code2graph's `tree-sitter >=0.24, <0.27` pin
**Researched:** 2026-07-05
**Confidence:** HIGH (every verdict below is a direct crates.io API lookup, not training data)

## Methodology

For each candidate, queried `https://crates.io/api/v1/crates/<name>` (metadata: version, downloads, maintainer, last update) and `https://crates.io/api/v1/crates/<name>/<version>/dependencies` (the actual declared `tree-sitter`/`tree-sitter-language` requirement) with a proper User-Agent header (crates.io's data-access policy blocks anonymous UAs).

**Critical mechanism found:** code2graph's existing grammar registrations (`src/grammar.rs`) all call `tree_sitter_<lang>::LANGUAGE.into()`. `LANGUAGE` is a `tree_sitter_language::LanguageFn` constant; the `.into()` conversion to `tree_sitter::Language` is implemented by the **host** `tree-sitter` crate. Every grammar crate published in the last ~2 years (including 100% of the candidates below) declares its real, load-bearing dependency as `tree-sitter-language = "^0.1"` (a `normal` dependency) — **not** a direct `tree-sitter` dependency. Their `tree-sitter = "^0.2x"` entry is a `dev-dependency` only (used for the grammar's own test suite), and is irrelevant to whether it links into a consumer.

This repo's own `Cargo.lock` currently resolves the host `tree-sitter` to **0.26.9**, and that crate itself depends on `tree-sitter-language = "^0.1"` (locked at `0.1.7`). Since every candidate below also depends on `tree-sitter-language ^0.1`, Cargo's resolver unifies to one compatible version and the `.into()` conversion compiles for all of them — this is the real compatibility gate, and it is satisfied uniformly.

The remaining risk is the **runtime ABI version** of the compiled parser, checked by this repo's own `abi_versions_are_compatible` test (`src/grammar.rs`) against `tree_sitter::{MIN_COMPATIBLE_LANGUAGE_VERSION, LANGUAGE_VERSION}`. Verified via docs.rs:

| tree-sitter version | LANGUAGE_VERSION (max ABI) | MIN_COMPATIBLE_LANGUAGE_VERSION (min ABI) |
|---|---|---|
| 0.24.7 | 14 | 13 |
| 0.26.10 | 15 | 13 |

At the repo's currently-resolved 0.26.9, the supported ABI window is **[13, 15]**. Every candidate grammar was generated/published in 2024–2026 (modern tree-sitter-cli era, ABI 14 or 15) — all fall inside this window. **Old-style crates that declare `tree-sitter` as a normal (non-dev) dependency at `~0.20.x` — e.g. `tree-sitter-vue`, `tree-sitter-apex` — expose the pre-`tree-sitter-language` `Language` type directly and are genuinely incompatible** (confirmed below); this is the pattern CONTRIBUTING.md's "no bridging" rule exists to reject.

## Verdict Table — 🟠 Planned Candidates

| Language | Crate | Version to pin | tree-sitter compat evidence | License | Maintainer | Downloads | Last updated | Verdict | Confidence |
|---|---|---|---|---|---|---|---|---|---|
| Elixir | `tree-sitter-elixir` | `0.3.5` | `tree-sitter-language ^0.1.0` (normal); `tree-sitter ^0.23.0` (dev, tests) | Apache-2.0 | `elixir-lang` (official org) | 2,480,127 | 2026-03-02 | **COMPATIBLE** | HIGH |
| Erlang | `tree-sitter-erlang` | `0.19.0` | `tree-sitter-language ^0.1.0` (normal); `tree-sitter ^0.23` (dev) | MIT | `WhatsApp` org | 208,104 | 2026-06-04 | **COMPATIBLE** | HIGH |
| Gleam | `tree-sitter-gleam` | `1.0.0` | `tree-sitter-language ^0.1.0` (normal); `tree-sitter ^0.23` (dev) | Apache-2.0 | `tree-sitter` org (official grammars collection) | 66,668 | 2025-01-15 | **COMPATIBLE** | HIGH |
| Zig | `tree-sitter-zig` | `1.1.2` | `tree-sitter-language ^0.1` (normal); `tree-sitter ^0.24.5` (dev) | MIT | `tree-sitter-grammars` org | 413,750 | 2024-12-22 | **COMPATIBLE** | HIGH |
| Julia | `tree-sitter-julia` | `0.23.1` | `tree-sitter-language ^0.1` (normal); `tree-sitter ^0.24` (dev) | MIT | `tree-sitter` org | 179,788 | 2024-11-11 | **COMPATIBLE** | HIGH |
| R | `tree-sitter-r` | `1.3.0` | `tree-sitter-language ^0.1` (normal); `tree-sitter ^0.24.7` (dev) | MIT | `r-lib` org | 380,144 | 2026-06-19 | **COMPATIBLE** | HIGH |
| Haskell | `tree-sitter-haskell` | `0.23.1` | `tree-sitter-language ^0.1` (normal); `tree-sitter ^0.23` (dev) | MIT | `tree-sitter` org | 2,011,621 | 2024-11-10 | **COMPATIBLE** | HIGH |
| OCaml | `tree-sitter-ocaml` | `0.25.0` | `tree-sitter-language ^0.1` (normal); `tree-sitter ^0.26` (dev) | MIT | `tree-sitter` org | 314,791 | 2026-05-09 | **COMPATIBLE** | HIGH |
| Objective-C | `tree-sitter-objc` | `3.0.2` | `tree-sitter-language ^0.1` (normal); `tree-sitter ^0.24` (dev) | MIT | `tree-sitter-grammars` org | 356,921 | 2024-12-16 | **COMPATIBLE** | HIGH |
| Fortran | `tree-sitter-fortran` | `0.6.0` | `tree-sitter-language ^0.1.0` (normal); `tree-sitter ^0.26.3` (dev) | MIT | `stadelmanma` (individual, long-running maintainer) | 113,358 | 2026-04-24 | **COMPATIBLE** | HIGH |
| Groovy | `tree-sitter-groovy` | `0.1.2` | `tree-sitter-language ^0.1` (normal); `tree-sitter ^0.24` (dev) | MIT | `amaanq` (individual; prolific grammar author across the ecosystem) | 142,364 | 2024-11-19 | **COMPATIBLE** | HIGH |
| PowerShell | `tree-sitter-powershell` | `0.26.4` | `tree-sitter-language ^0.1` (normal); `tree-sitter ^0.26.5` (dev) | MIT | `airbus-cert` org | 1,685,518 | 2026-05-04 | **COMPATIBLE** | HIGH |
| SystemVerilog | `tree-sitter-systemverilog` | `0.3.1` | `tree-sitter-language ^0.1` (normal); `tree-sitter ^0.25.3` (dev) | MIT | `gmlarumbe` (individual) | 19,207 | 2025-10-08 | **COMPATIBLE** | HIGH |
| Astro | `tree-sitter-astro-next` | `0.1.1` | `tree-sitter-language ^0.1.7` (normal); `tree-sitter ^0.26.5` (dev); description states "compatible with tree-sitter 0.25+" | MIT OR Apache-2.0 | `PRRPCHT` (individual) | 38,742 | 2026-02-14 (single release, no track record yet) | **COMPATIBLE, but MEDIUM confidence on maturity** | MEDIUM |

**All 14 candidates from the 🟠 planned list have a usable, compatible grammar right now.** None are gated out by tree-sitter version.

### Note on `tree-sitter-astro-next`

No crate named `tree-sitter-astro` exists on crates.io (checked directly — 404). The only Astro grammar targeting the modern ABI/dependency pattern is `tree-sitter-astro-next`, a single-version (0.1.1), ~5-months-old crate from an individual maintainer. It is technically compatible (confirmed dependency structure) but has no multi-release track record, so treat it as **higher execution risk than the other 13** — dump the real AST (`to_sexp()`) early per CONTRIBUTING's guidance, and budget extra time for surprises given the embedded-SFC extraction path (Svelte pattern) it needs to support.

## Verdict Table — 🔴 Blocked Re-check

| Language | Crate checked | Result | Evidence | Verdict |
|---|---|---|---|---|
| Vue | `tree-sitter-vue` | Still 0.0.3, last updated 2022-09-24 | `tree-sitter ~0.20.3` declared as a **normal** (non-dev) dependency — exposes the old, pre-`tree-sitter-language` `Language` type directly; incompatible with the pinned range and cannot be bridged without vendoring | **STILL BLOCKED — no change** |
| Liquid | `tree-sitter-liquid` (and `-ng` variant) | No crate found at all (crates.io search for `tree-sitter-liquid` returns zero results) | N/A | **STILL BLOCKED — no crate exists** |
| F# | `tree-sitter-fsharp` | **NEW FINDING**: 0.3.1, maintained by `ionide` org (the standard F# tooling org), 6 published versions since 2024-09-05, most recent **2026-07-01** (4 days before this research) | `tree-sitter-language ^0.1` (normal dep, modern pattern); `tree-sitter ^0.26.8` (dev); license MIT; 112,768 downloads (71,703 recent — active adoption) | **UNBLOCKED — COMPATIBLE now.** A maintained, compatible grammar has appeared since the project's last check. Recommend moving F# from Out of Scope to the 🟠 planned/candidate set for this milestone. |
| Apex | `tree-sitter-apex` | Still 1.0.0, last updated 2022-04-08 | `tree-sitter ~0.20.0` declared as a **normal** dependency — same old-ABI pattern as Vue | **STILL BLOCKED — no change** |
| COBOL | `tree-sitter-cobol` | 0.1.0, single version, published 2025-08-22 | **Zero declared dependencies at all** (`dependencies` endpoint returns an empty list) — no `tree-sitter`, no `tree-sitter-language`, no `cc` build-dep for the C parser. No `repository` field either. This is not a functioning tree-sitter grammar integration; it cannot be linked as one. 549 total downloads (59 recent). | **STILL BLOCKED — the one crate that exists is not usable; treat as no crate** |

**Key result: F# is no longer blocked.** This is a genuine ecosystem change since the project's last assessment and should be surfaced to roadmap planning as a new candidate, not left in Out of Scope.

## Recommended Priority Order

Ranked by ecosystem demand (real-world usage code2graph would plausibly index) × grammar maturity/maintenance signal:

1. **Haskell** — `tree-sitter-haskell 0.23.1`. Highest download count of any candidate (2M+), official `tree-sitter` org grammar, long track record. Real production Haskell codebases exist to extract from.
2. **PowerShell** — `tree-sitter-powershell 0.26.4`. Extremely high downloads (1.68M), actively maintained (May 2026), backed by a security-tooling org (`airbus-cert`) with strong incentive to keep it correct — PowerShell scripting is common in DevOps/infra repos.
3. **Elixir** — `tree-sitter-elixir 0.3.5`. Official `elixir-lang` org grammar (as authoritative as a grammar gets), 2.48M downloads, actively updated March 2026.
4. **F#** — `tree-sitter-fsharp 0.3.1` (newly unblocked). Official `ionide` org, most-recently-updated crate of the entire batch (2026-07-01), fills a real .NET-ecosystem gap next to the existing C# support.
5. **OCaml** — `tree-sitter-ocaml 0.25.0`. Official `tree-sitter` org, actively updated (May 2026), meaningful adoption (compilers/tooling, Jane Street-style codebases).
6. **Zig** — `tree-sitter-zig 1.1.2`. High downloads (413K), growing systems-language adoption, `tree-sitter-grammars` org maintained.
7. **Julia** — `tree-sitter-julia 0.23.1`. Official `tree-sitter` org, strong scientific-computing niche demand.
8. **Objective-C** — `tree-sitter-objc 3.0.2`. `tree-sitter-grammars` org, meaningful legacy-iOS/macOS codebase relevance (pairs naturally with existing Swift support).
9. **R** — `tree-sitter-r 1.3.0`. `r-lib` org (same group behind tidyverse tooling), actively updated (June 2026), strong data-science niche.
10. **Erlang** — `tree-sitter-erlang 0.19.0`. `WhatsApp` org (production Erlang at scale), actively updated (June 2026); pairs naturally with Elixir (shared BEAM ecosystem, shared resolver opportunities later).
11. **Groovy** — `tree-sitter-groovy 0.1.2`. Real demand from Gradle build scripts/Jenkinsfiles, but crate itself is a lower version number (0.1.2) — reasonable maturity for a niche grammar.
12. **Fortran** — `tree-sitter-fortran 0.6.0`. Individual-maintained but long-running and recently updated (April 2026); real demand in scientific/HPC codebases.
13. **Gleam** — `tree-sitter-gleam 1.0.0`. Official `tree-sitter` org, but Gleam itself is a small, young language — lower real-world corpus size than the above.
14. **SystemVerilog** — `tree-sitter-systemverilog 0.3.1`. Real niche (hardware design) but lowest download count (19K) and single-maintainer; fine to build but expect a smaller eval corpus.
15. **Astro** — `tree-sitter-astro-next 0.1.1`. Do this last among the batch: real ecosystem demand (Astro is a popular JS meta-framework) but the grammar crate itself is brand-new (single release, ~5 months old) — validate it thoroughly (AST dump, embedded-SFC extraction via the Svelte pattern) before committing scope to it, and don't be surprised if node/field names shift in a future release.

## What NOT to Use and Why

| Crate | Why avoid | What to do instead |
|---|---|---|
| `tree-sitter-vue` (0.0.3) | Declares `tree-sitter ~0.20.3` as a **normal** dependency — the pre-`tree-sitter-language` ABI/type era. Cannot compile against this repo's `tree-sitter >=0.24, <0.27` pin without an incompatible-type bridge, which CONTRIBUTING.md explicitly forbids ("Do not bridge incompatible tree-sitter versions"). No newer release since 2022-09-24 — effectively unmaintained. | Keep Vue in Out of Scope; revisit only if a `tree-sitter-vue-ng`-style community fork adopts the `tree-sitter-language` pattern (none exists today — checked). |
| `tree-sitter-apex` (1.0.0) | Same `tree-sitter ~0.20.0` normal-dependency pattern as Vue. Unmaintained since 2022-04-08, very low downloads (3,485 total). | Keep Apex in Out of Scope. |
| `tree-sitter-cobol` (0.1.0) | Declares **zero dependencies** — no `tree-sitter`/`tree-sitter-language` binding, no `cc` build-dependency to compile a C parser. This is not a working tree-sitter integration regardless of version compatibility; there is nothing to link against. No repository link to inspect either. Do not attempt to depend on this crate as-is. | Keep COBOL in Out of Scope; if COBOL support becomes a priority, this needs a from-scratch grammar (crosses the "grammars come from crates.io only" line — requires a project-level Discussion per CONTRIBUTING §4, not a drive-by dependency add). |
| `tree-sitter-liquid` | Does not exist on crates.io under this name or any `-ng`/community variant (search returned zero results). | Keep Liquid in Out of Scope; nothing to gate on, there's no crate at all. |

## Version Compatibility

| Package A | Compatible With | Notes |
|---|---|---|
| `tree-sitter >=0.24, <0.27` (this repo's pin, currently resolving to `0.26.9`) | `tree-sitter-language ^0.1` (any 0.1.x) | The load-bearing compatibility contract for every modern grammar crate (candidates above and existing 22 registered grammars alike). Cargo unifies to one `tree-sitter-language` version across the whole dependency graph; as long as a grammar crate's `Cargo.toml` declares `tree-sitter-language = "^0.1"` as a normal dependency (not `tree-sitter` directly), it will compile against this repo regardless of what `tree-sitter` version its own dev-dependencies/tests reference. |
| ABI window at resolved `tree-sitter 0.26.9` | ABI 13–15 (`MIN_COMPATIBLE_LANGUAGE_VERSION=13`, `LANGUAGE_VERSION=15`, both verified via docs.rs) | Every candidate crate above was published 2024–2026 using a modern `tree-sitter-cli` generation (ABI 14 or 15) — comfortably inside the window. The repo's own `abi_versions_are_compatible` test in `src/grammar.rs` will still catch any surprise at build time; add the `check("<lang>", super::<lang>())` arm per CONTRIBUTING's recipe for each new grammar. |
| Old-style grammar crates declaring `tree-sitter ~0.20.x` as a **normal** dependency (Vue, Apex) | Nothing in the `>=0.24, <0.27` window | These predate the `tree-sitter-language` indirection; their public `Language` type comes from a different, incompatible version of the `tree-sitter` crate and cannot be passed to this repo's parser. This is a hard incompatibility, not a semver nuance — do not attempt to force it. |

## Sources

- crates.io API (`GET /api/v1/crates/<name>` and `/api/v1/crates/<name>/<version>/dependencies`) — direct lookups for all 19 crates checked (13 planned candidates + Astro + 5 blocked re-checks), 2026-07-05. HIGH confidence — this is the authoritative registry, not a mirror or training-data recollection.
- docs.rs `tree-sitter` crate pages (0.24.7 and 0.26.10/0.26.9) — verified `LANGUAGE_VERSION` (14 → 15) and `MIN_COMPATIBLE_LANGUAGE_VERSION` (13, unchanged) constants directly from generated documentation. HIGH confidence.
- This repo's own `Cargo.lock` — verified the currently-resolved `tree-sitter = 0.26.9` and `tree-sitter-language = 0.1.7`, grounding the compatibility analysis in the actual resolved graph rather than the abstract semver range. HIGH confidence.
- `src/grammar.rs` and `CONTRIBUTING.md` (§"Adding a Language", §"When a Language Has No Usable Grammar") — read directly to confirm the `LanguageFn.into()` mechanism and the project's existing compatibility-gating discipline. HIGH confidence.

---
*Stack research for: code2graph language-expansion milestone*
*Researched: 2026-07-05*
