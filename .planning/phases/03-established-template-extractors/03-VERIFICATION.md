# Phase 3 Verification — Established-Template Extractors

**Phase goal:** Ship five new language extractors — Zig, Objective-C, Fortran, Groovy,
SystemVerilog — each mapped to a solid existing in-repo template, with the explicit scope
decisions each one raises (`.h` dispatch, `.gradle` inclusion) resolved and documented.

**Verified:** 2026-07-05, on `main` (HEAD `6816e72 feat(extract): add ObjC extractor end-to-end`),
clean working tree.
**Verdict: PASS** — all 6 roadmap success criteria satisfied; no fixes were required during
verification.

## Gate results (actual commands run)

| # | Gate | Command | Result |
|---|------|---------|--------|
| 1 | Full test suite | `cargo test --workspace --all-features` | **PASS** — 883 tests, 0 failed: 860 lib (incl. per-language extractor suites + docs sync tests), 11 eval unit, 10 `eval/tests/regression.rs` corpus regression, 2 doc-tests |
| 2 | Formatting | `cargo fmt --all -- --check` | **PASS** — no diffs |
| 3 | Lints | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **PASS** — exit 0 |
| 4 | Feature isolation | `cargo check --no-default-features --features <lang>` for each of `zig`, `systemverilog`, `fortran`, `groovy`, `objc` | **PASS** — all five compile standalone (`Finished dev profile` each) |
| 5 | Bindings parity | grep of both bindings Cargo.tomls; `npx napi build --release --platform` in `bindings/node`; `git diff --exit-code index.js index.d.ts` | **PASS** — all five feature strings present in `bindings/node/Cargo.toml:14` and `bindings/python/Cargo.toml:14`; napi build finished and produced a no-op diff against the committed `index.js`/`index.d.ts` |
| 6 | Sanity | `src/lang.rs` variants + `docs/supported-languages.md` rows | **PASS** — see criteria 2/4/6 below |

## Success criteria (goal-backward, per ROADMAP.md Phase 3)

### 1. Zig — functions/structs with real SCIP ids, `Calls`, `@import` Imports, `comptime` capped — SATISFIED

- 16 `extract::zig::tests` pass, covering exactly these behaviors:
  `struct_with_member_fn`, `free_call_and_member_call_with_qualifier`,
  `std_and_relative_imports`, `import_inside_usingnamespace_still_emits`,
  `pub_fn_is_public_private_fn_is_private` (real `pub` visibility), and
  `comptime_block_declarations_are_table_stakes` (the honest `comptime` cap:
  declarations extracted, never evaluated).
- Corpus case: `eval/corpus/zig/scoped_call` (guarded by the passing corpus regression suite).
- Docs row (`docs/supported-languages.md:68`) states the `comptime` cap and the
  `usingnamespace` limitation explicitly.

### 2. Objective-C — `.m` symbols distinct from C, `.h` dispatch decision documented — SATISFIED

- 11 `extract::objc::tests` pass: `interface_class_methods_property_and_conformance`,
  `message_sends_are_calls_with_receiver_qualifier` (selector-form names, receiver as
  qualifier), `category_is_distinct_class_symbol`, `protocol_with_optional_methods`,
  `implementation_dedupes_against_interface`, `c_functions_handled_c_style`.
- `dispatch_claims_m_and_mm_only` directly asserts the scope decision: `Language::ObjC`
  claims only `.m`/`.mm` (`src/lang.rs:112`), with the inline comment
  `// .m, .mm (D-01: bare .h stays mapped to C)` at `src/lang.rs:40`.
- **Documented scope decision confirmed:** docs row (`docs/supported-languages.md:72`)
  says verbatim: "bare `.h` stays mapped to C (locked decision D-01) — ObjC declarations
  in headers are extracted as C facts, no content sniffing".
- Corpus case: `eval/corpus/objc/scoped_call`.

### 3. Fortran — `.f90` modules/subroutines with real `public`/`private`; fixed-form capped — SATISFIED

- 16 `extract::fortran::tests` pass: `emits_module_symbol_named_after_the_module_unit`,
  `explicit_public_private_statements_set_real_visibility`,
  `bare_private_statement_flips_the_module_default`,
  `module_default_visibility_is_public`, `program_internal_procedures_are_private`,
  `subroutine_call_and_function_call_are_captured`, `use_only_imports_each_listed_name`.
- Fixed-form cap is tested honestly (`fixed_form_legacy_yields_program_and_calls`) and
  documented in the docs row (`docs/supported-languages.md:70`): legacy `.f` "caps at
  whatever the grammar yields", plus the `name(args)` array-access ambiguity ceiling.
- Corpus case: `eval/corpus/fortran/scoped_call`.

### 4. Groovy — symbols, `Calls`, `Imports`, explicit `.gradle` decision — SATISFIED

- 14 `extract::groovy::tests` pass: `extracts_class_and_members_with_visibility`,
  `paren_and_parenless_calls_are_captured`, `extracts_named_and_static_imports_skips_wildcard`,
  `import_refs_carry_source_module`, `static_main_gets_entry_point_marker`.
- **Documented scope decision confirmed:** `.gradle` is IN scope, claimed by
  `Language::Groovy` (`src/lang.rs:111` — `&["groovy", "gradle"]`), tested by
  `gradle_file_extracts_script_function_and_plain_calls`, and the docs row
  (`docs/supported-languages.md:71`) documents the boundary verbatim: "`.gradle` parsed
  as plain Groovy — no Gradle-DSL semantics (dependency coordinates, task graph)".
- Corpus case: `eval/corpus/groovy/scoped_call`.

### 5. SystemVerilog — `.sv` module/class symbols with `Calls`/`Imports`, C-template approach — SATISFIED

- 13 `extract::systemverilog::tests` pass: `extracts_module_symbol`,
  `extracts_class_with_ctor_method_task_and_visibility`,
  `hierarchical_and_scoped_calls_capture_qualifier`,
  `package_import_is_import_with_from_path`, and — mirroring the C template's `#include`
  handling — `include_directive_is_file_level_import`; `module_instantiation_is_type_ref`.
- Docs row (`docs/supported-languages.md:69`) states the ceilings honestly: no
  elaboration/parameterization semantics, generate blocks walked not expanded, class
  `extends` not emitted in v1.
- Corpus case: `eval/corpus/systemverilog/scoped_call`.

### 6. Per-language gates: isolated build, bindings parity, diff-free napi, corpus, docs row — SATISFIED

- **Feature entries:** both `bindings/node/Cargo.toml` and `bindings/python/Cargo.toml`
  list all five (`zig`, `systemverilog`, `fortran`, `groovy`, `objc`) in the
  `code2graph_core` features array (line 14 of each).
- **Diff-free napi build:** `npx napi build --release --platform` in `bindings/node`
  completed (`Finished release profile ... in 19.48s`) and
  `git diff --exit-code index.js index.d.ts` exited 0.
- **Corpus:** all five have `eval/corpus/<lang>/scoped_call`; the 10 corpus regression
  tests in `eval/tests/regression.rs` pass.
- **Docs:** all five have 🟢 rows in `docs/supported-languages.md` (lines 68-72), and the
  docs↔enum sync tests in the 860-test lib suite pass.
- **Isolated `cargo test` — satisfied via the documented equivalent, not literally:**
  the roadmap's literal `cargo test --no-default-features --features <lang>` cannot run
  standalone for ANY language because of the pre-existing resolver-test isolation gap
  (un-gated extractor imports in `src/resolve/symbol_table.rs` /
  `src/resolve/scope_graph.rs` test modules) recorded in
  `.planning/phases/02-quick-win-extractors-astro-powershell/deferred-items.md`. As in
  Phase 2, the criterion is satisfied equivalently by the combination of
  (a) `cargo check --no-default-features --features <lang>` passing for each of the five
  (proves no accidental cross-feature dependency in production code — the actual risk the
  criterion targets) and (b) the full `cargo test --workspace --all-features` run passing,
  which executes every one of the five languages' extractor test suites (70 tests total:
  zig 16, fortran 16, groovy 14, systemverilog 13, objc 11). The CI `feature-isolation`
  matrix job (`.github/workflows/test.yml`) includes all five languages and runs the same
  `cargo check` isolation on every push. The test-module re-gating remains a deferred
  standalone task per the Phase 2 recommendation.

## Documented scope decisions (both confirmed present in shipped docs)

1. **ObjC `.h` dispatch (D-01):** bare `.h` stays mapped to C; no content sniffing.
   Enforced in `src/lang.rs` (ObjC claims only `m`/`mm`), asserted by
   `extract::objc::tests::dispatch_claims_m_and_mm_only`, documented in
   `docs/supported-languages.md:72`.
2. **Groovy `.gradle` inclusion:** `.gradle` is in scope, parsed as plain Groovy with no
   Gradle-DSL semantics. Enforced in `src/lang.rs` (Groovy claims `groovy`/`gradle`),
   asserted by `extract::groovy::tests::gradle_file_extracts_script_function_and_plain_calls`,
   documented in `docs/supported-languages.md:71`.

## Issues found / fixed during verification

None. All gates passed on the first run; no fix commits were needed.
