# Phase 2: Quick-Win Extractors — Astro & PowerShell - Research

**Researched:** 2026-07-05
**Domain:** tree-sitter code-graph extraction — PowerShell (novel parenless-call grammar) and Astro (embedded-SFC pattern reusing the TS engine)
**Confidence:** HIGH — every node/field name below was verified by parsing real snippets with the exact pinned crate versions and printing the field-annotated tree (throwaway `examples/ast_dump.rs` + `examples/ast_query_test.rs`, both deleted after this research). Grammar registration (`src/grammar.rs`) and ABI compatibility were already verified in Phase 1.

## Summary

Both extractors follow existing templates near-1:1, and both grammars are already registered in `src/grammar.rs` with correct crate names (`tree_sitter_powershell::LANGUAGE`, `tree_sitter_astro_next::LANGUAGE`) and ABI-verified. The remaining work is exactly the CONTRIBUTING 6-step recipe for each language, plus bindings parity.

**PowerShell** (`src/extract/shell.rs` as template): the grammar is genuinely novel among existing extractors because cmdlet-style calls are **parenless space-separated commands** (`Get-Process -Name foo`), a materially different `CALL_QUERY` shape than every other language. I verified the exact query patterns below by executing them against the real grammar — they work as written. Function/class definitions, imports (`Import-Module`, `using module`, dot-sourcing), and member calls (`$obj.Method()`, `[Type]::Static()`) all parse into distinct, confirmed node shapes.

**Astro** (`src/extract/svelte.rs` as template): the frontmatter (`---`...`---`) and `<script>` blocks are structurally *closer* to Svelte than expected — the HTML-ish attribute grammar (`attribute` → `attribute_name` / `quoted_attribute_value` → `attribute_value`) is node-for-node identical to Svelte's, so `svelte.rs`'s `detect_script_lang` logic ports with **zero changes** to the node-kind literals. The one structural difference from Svelte: Astro documents have **at most one `frontmatter` node** (not a `script_element`), containing a single `frontmatter_js_block` raw-text child — always treat frontmatter as TypeScript (Astro compiles frontmatter as TS unconditionally; there is no `lang` attribute to detect).

**Primary recommendation:** Implement PowerShell's cmdlet-call detection via `command_name` text-matching against import-keywords (`Import-Module`, `using` + `module`) before falling through to a generic `Call` reference — the grammar does **not** distinguish these as different node types, so the extractor must do it, exactly the same "call-shaped import" pattern already established for Lua's `require()`. Implement Astro by copying `svelte.rs`'s structure directly: replace `script_element` discovery with (a) at most one `frontmatter` node → `extract_ecmascript(..., Language::TypeScript)`, and (b) `script_element` nodes (same discovery/raw_text/detect_script_lang logic as Svelte, verbatim) → `extract_ecmascript` with the same lang-detection.

## User Constraints (from CONTEXT.md)

### Locked Decisions

**PowerShell extractor (LANG-08)**
- D-01: Template `src/extract/shell.rs` (near-1:1). Extensions `.ps1`, `.psm1` only — `.psd1` is a data manifest, documented exclusion.
- D-02: Emit function definitions (`function Verb-Noun {}`, including `filter`), PS5+ `class` definitions with methods/properties, imports (`Import-Module`, `using module`, dot-sourcing `. ./file.ps1`), calls in BOTH forms (cmdlet-style AND expression-style member calls, receiver captured as qualifier), plus variable Read/Write.
- D-03: `Visibility` is honestly `Unknown` — do NOT infer from `Export-ModuleMember`. `Invoke-Expression` / `& $scriptBlock` are a documented unresolved dynamic-invocation ceiling — never guessed.

**Astro extractor (LANG-10)**
- D-04: Embedded-SFC pattern, reference implementation `src/extract/svelte.rs`: parse the host `.astro` document with tree-sitter-astro-next, locate the frontmatter fence AND any `<script>` tag contents, run `super::typescript::extract_ecmascript` on each, remap offsets via `support::shift_offsets`.
- D-05: The `astro` Cargo feature transitively enables `typescript` (exactly like `svelte = [..., "typescript", ...]`).

**Wiring & docs (both languages)**
- D-06: Full recipe: `Language::PowerShell` / `Language::Astro` enum variants, `as_str()` arms, extension dispatch, `src/extract/mod.rs` + `dispatch.rs` wiring, unit tests with real rendered SCIP id strings, ≥1 `eval/corpus/<lang>/` golden case with `expected.edges`, `docs/supported-languages.md` row moved 🟠→🟢 with honest capability columns (sync-test guarded).
- D-07: Flip `powershell` and `astro` INTO the `default` feature list when their extractors land. `_extractors` gains their enum/dispatch code in the same change.

**Bindings parity (BIND-01, BIND-02)**
- D-08: Add `"powershell"` and `"astro"` to the explicit `features = [...]` lists in `bindings/node/Cargo.toml` AND `bindings/python/Cargo.toml` in the same change that flips each language into default.
- D-09: Regenerate napi artifacts (`npx napi build --release --platform` in `bindings/node`) and verify the committed `index.js`/`index.d.ts` diff is a no-op; run whatever bindings CI check exists locally before completing the phase.

### Claude's Discretion
- Exact tree-sitter node names for both grammars — **resolved below**, verified against the exact pinned crate versions.
- Corpus case content (keep it small but role-typed).
- Whether PowerShell classes emit Inherit edges (`class B : A`) — the AST makes it unambiguous (confirmed below); recommend including it.

### Deferred Ideas (OUT OF SCOPE)
- PowerShell `.psd1` manifest parsing for package enrichment.
- Astro template-expression extraction beyond script/frontmatter (`{expr}` in markup) — depth work for a later milestone.
- 3-OS bindings CI matrix (DEPTH-03, v2).

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| LANG-08 | PowerShell extractor (template: Shell) | Verified node/field names for function/filter defs, class+method+inheritance, all 3 import forms, both call forms, variable read/write — see "PowerShell: Verified AST Shapes" below. Verified working tree-sitter queries provided. |
| LANG-10 | Astro extractor via embedded-SFC pattern (template: Svelte; `astro` feature transitively enables `typescript`) | Verified frontmatter + script_element node shapes; confirmed `detect_script_lang`'s attribute-node shape is identical to Svelte's; confirmed no-frontmatter and multi-block cases parse cleanly. See "Astro: Verified AST Shapes" below. |
| BIND-01 | Add `powershell`/`astro` to bindings Cargo.toml feature lists | Confirmed exact current feature-list strings in both `bindings/node/Cargo.toml` and `bindings/python/Cargo.toml` (both list all 22 current languages identically) — insertion point identified. |
| BIND-02 | Regenerate + verify napi artifacts no-op diff | Confirmed `node`/`npm`/`npx` (v22.23.0/10.9.8) are available locally; `bindings/node/node_modules` is NOT yet installed (`npm ci` must run first); exact CI command mirrored from `.github/workflows/test.yml`'s `bindings` job. |

</phase_requirements>

## Standard Stack

### Core

| Component | Version (pinned in `Cargo.toml`) | Purpose | Status |
|---|---|---|---|
| `tree-sitter-powershell` | 0.26.4 | PowerShell grammar | Already an optional dep + feature (`powershell = ["dep:tree-sitter-powershell"]`), grammar-only, non-default. ABI-verified in Phase 1 (`src/grammar.rs::powershell()` + its `check(...)` arm already exist). |
| `tree-sitter-astro-next` | 0.1.1 | Astro grammar (single-maintainer, independent project — **not** the official `withastro` org) | Same status: optional dep + feature, grammar-only, non-default, ABI-verified. `src/grammar.rs::astro()` already exists. |
| `tree-sitter` | `>=0.24, <0.27` (workspace pin) | Core parser | No change needed — both grammars already compile against this range (confirmed by Phase 1's `abi_versions_are_compatible` test, which already contains `check("powershell", ...)` and `check("astro", ...)` arms). |

**No new dependency work is needed in Phase 2** — Phase 1 already added both crates as optional deps and registered+ABI-verified the grammar functions. Phase 2's Cargo.toml work is limited to: (1) moving `powershell` and `astro` from their current grammar-only feature definitions into gaining `"_extractors"` (mirroring every other language, e.g. `shell = ["dep:tree-sitter-bash", "_extractors"]`), (2) adding `"typescript"` to `astro`'s feature list (mirroring `svelte = ["dep:tree-sitter-svelte-ng", "typescript", "_extractors"]`), and (3) adding both language names to the `default = [...]` list.

**Version verification:** `Cargo.toml` currently pins exactly these versions; no registry check needed since Phase 1 already verified them against crates.io and the ABI gate. Re-running `cargo metadata` is unnecessary — the versions are locked and this phase must not bump them (that would re-open the compat question Phase 1 already closed).

### Supporting

No new supporting libraries. Both extractors are built entirely from `src/extract/support.rs` helpers already in use by every other extractor (`ExtractCtx`, `make_symbol`, `node_text`, `field_text`, `child_text`, `one_line_signature`, `collect_call_references`, `push_ref`/`push_import_ref`/`push_type_ref`, `push_scope`/`push_binding`/`innermost_scope`/`attach_reference_scopes`, `shift_offsets`, `module_symbol`, `definition_bindings`). For Astro specifically: `super::typescript::extract_ecmascript` and `super::typescript::module_namespaces` (both already used by `svelte.rs`, imported the same way).

### Alternatives Considered

None — both languages have exactly one viable grammar crate each (confirmed in Phase 1 / `.planning/research/FEATURES.md`), and both templates (`shell.rs`, `svelte.rs`) are the correct, only reasonable structural matches per the locked decisions.

**Installation:** No new `cargo add` needed — deps already present as optional. The only `Cargo.toml` edits are feature-list membership changes (see "Architecture Patterns" below for exact diffs).

## Architecture Patterns

### Recommended Project Structure

```
src/
├── extract/
│   ├── powershell.rs   # new — struct PowerShellExtractor, mirrors shell.rs's shape
│   ├── astro.rs        # new — struct AstroExtractor, mirrors svelte.rs's shape
│   ├── mod.rs           # + 2 `#[cfg(feature = "...")] pub mod` / `pub use` pairs (wiring only)
│   └── dispatch.rs      # + 2 `use super::...Extractor;` + match arms
├── lang.rs               # + Language::PowerShell, Language::Astro variants (+ ALL, extensions, as_str, from_extension test coverage)
└── grammar.rs            # UNCHANGED — Phase 1 already added powershell()/astro() + ABI check arms
eval/corpus/
├── powershell/<case>/    # new — ≥1 golden case + expected.edges
└── astro/<case>/         # new — ≥1 golden case + expected.edges
docs/supported-languages.md   # PowerShell + Astro rows: 🟠 → 🟢, capability columns filled honestly
bindings/node/Cargo.toml      # + "powershell", "astro" to the explicit features list
bindings/python/Cargo.toml    # + "powershell", "astro" to the explicit features list
```

### Cargo.toml diff shape (exact, both languages)

```toml
[features]
default = [ ..., "svelte", "powershell", "astro" ]   # append both
# ...
powershell = ["dep:tree-sitter-powershell", "_extractors"]     # currently: ["dep:tree-sitter-powershell"]
astro = ["dep:tree-sitter-astro-next", "typescript", "_extractors"]  # currently: ["dep:tree-sitter-astro-next"]
```

### bindings/{node,python}/Cargo.toml diff shape (identical in both files today)

Current (both files, verified byte-identical feature list):
```
features = ["serde", "rust", "python", "typescript", "go", "java", "c", "cpp", "ruby", "php", "shell", "swift", "kotlin", "solidity", "sql", "hcl", "csharp", "scala", "dart", "lua", "luau", "pascal", "svelte"]
```
New (append `"powershell", "astro"`):
```
features = [..., "svelte", "powershell", "astro"]
```

### Pattern 1: PowerShell function/filter definitions (near-1:1 with `shell.rs::collect_symbols`)

**What:** Both `function Name { ... }` and `filter Name { ... }` parse to the same `function_statement` node kind — `filter` is a leading keyword token, not a different node kind. The name is a **positional child of kind `function_name`, with NO named field** (unlike shell.rs's `field_text(&child, "name", ...)` which relies on bash's `name:` field — that pattern does NOT port; use `child_text` by kind instead).

**Verified AST** (`function Get-Foo { param([string]$Name) return $Name }` / `filter Get-Bar { $_ }`):
```
function_statement [0..62]
  function (keyword token, no field)
  function_name [9..16] "Get-Foo"        ← NO field name; find via child_text(node, "function_name", bytes)
  { 
  script_block
    param_block                          ← present only when `param(...)` is used
      param_block > parameter_list > script_parameter
        attribute_list > attribute > type_literal > type_spec > type_name > type_identifier   ← [string] type annotation
        variable                          ← $Name (the parameter itself; PS has no dedicated "parameter" field name)
    script_block_body: script_block_body   ← THIS one IS a named field ("script_block_body")
      statement_list: statement_list       ← also a named field
        ... (body statements)
  }
```
`filter Get-Bar { ... }` is identical except the leading keyword is `filter` and there is no `param_block`.

**When to use:** Definition collection — walk `root`'s direct children (or all descendants, since PowerShell allows nested functions) for `function_statement` nodes; extract name via `child_text(&child, "function_name", bytes)`.

**Example:**
```rust
// Source: examples/ast_dump.rs throwaway dump against tree-sitter-powershell 0.26.4
for child in root.children(&mut root.walk()) {
    if child.kind() != "function_statement" { continue; }
    let Some(name) = child_text(&child, "function_name", ctx.bytes) else { continue; };
    // descriptors: namespaces + Descriptor::Method { name, disambiguator: String::new() }
}
```

### Pattern 2: PowerShell class + method + inheritance

**Verified AST** (`class Animal { [string]$Name Speak() { return "..." } }` / `class Dog : Animal { Speak() { return "Woof" } }`):
```
class_statement [0..63]                     # class Animal { ... }
  simple_name [6..12] "Animal"               ← class name, NO field, first simple_name child
  { 
  class_property_definition
    type_literal > type_spec > type_name > type_identifier   # [string]
    variable                                  # $Name
  class_method_definition
    simple_name [37..42] "Speak"              ← method name, NO field, first simple_name child of class_method_definition
    ( ) { 
    script_block
      script_block_body: ...  statement_list: ...
  }

class_statement [65..117]                   # class Dog : Animal { ... }
  simple_name [71..74] "Dog"                  ← class name (1st simple_name child)
  :                                            ← inheritance marker (anonymous token; presence of `:` distinguishes base-class case)
  simple_name [77..83] "Animal"               ← BASE class name (2nd simple_name child, only present when `:` token present)
  { class_method_definition ... }
```

**Critical gotcha (query-writing):** a single tree-sitter query combining an optional `(simple_name)? @base_name` capture with the other captures produces **spurious duplicate matches** (verified: querying `(class_statement (simple_name) @class_name (simple_name)? @base_name (class_method_definition (simple_name) @method_name))` against two classes returned 3 matches instead of 2, including one duplicate). **Do not use one combined query with an optional capture for this.** Instead, walk `class_statement`'s direct named children manually (matching `shell.rs`'s manual-walk style, not a query): the first `simple_name` child is always the class name; if a second `simple_name` child exists, it is the base class (emit `RefRole::IsImplementation` per D-`Claude's Discretion` — confirmed unambiguous); `class_method_definition` children each have their own first `simple_name` as the method name.

**When to use:** Definition collection for PS5+ classes (D-02), and the Inherit column (Claude's discretion — confirmed achievable).

### Pattern 3: PowerShell calls — TWO verified query patterns (cmdlet-style + member/expression-style)

**Cmdlet-style** (no parens, space-separated, e.g. `Get-Process -Name notepad | Stop-Process`):
```
command
  command_name: (command_name)    ← THIS field IS named "command_name" — reuse directly
  command_elements: (command_elements
    command_argument_sep
    command_parameter               ← e.g. "-Name" (a dash-prefixed flag, not a value)
    command_argument_sep
    generic_token                   ← bare positional argument text, e.g. "notepad"
    ...)
```
Pipeline chains (`cmd1 | cmd2`) are multiple sibling `command` nodes directly under `pipeline_chain`, joined by an anonymous `|` token — a tree-sitter query naturally matches each `command` independently, no special pipeline handling needed for call detection.

**Verified working query:**
```
(command command_name: (command_name) @callee)
```
Run against `Get-Process -Name notepad | Stop-Process` → captures BOTH `"Get-Process"` and `"Stop-Process"` correctly, confirmed via `QueryCursor`.

**Member/expression-style** (`.NET` interop and PS-object method calls, e.g. `$obj.Method()`, `[System.IO.File]::ReadAllText($path)`):
```
invokation_expression
  variable  |  type_literal          ← receiver; NO field name — a plain positional child, either kind
  .  |  ::                            ← anonymous operator token
  member_name (simple_name)           ← NO field name either
  argument_list
```
**Verified working query** (captures receiver as qualifier, method name as callee, for BOTH `.` and `::` forms in one pattern):
```
(invokation_expression
  [(variable) (type_literal)] @qualifier
  (member_name (simple_name) @callee))
```
Confirmed output against `$obj.Method()` / `[System.IO.File]::ReadAllText($path)`:
```
@qualifier = "$obj" (kind=variable)          @callee = "Method"
@qualifier = "[System.IO.File]" (kind=type_literal)   @callee = "ReadAllText"
```
This matches CONTRIBUTING's "receiver captured as qualifier" recipe step directly — `collect_call_references`'s existing `@qualifier`-optional-capture support (in `support.rs`) handles this with **zero changes** to the shared helper.

**Recommendation:** run TWO `collect_call_references` passes (one per query above) and concatenate, exactly the two-pattern-per-call-style approach the phase's decisions (D-02) already call for.

### Pattern 4: PowerShell imports — all three forms are `command` nodes; distinguish by `command_name` TEXT

The grammar has **no dedicated import/using-statement node** — `Import-Module Foo`, `using module Foo`, and `. .\file.ps1` (dot-sourcing) are ALL parsed as generic `command` nodes. The extractor must classify by inspecting the command's structure/text, not by node kind:

| Form | Verified shape | Detection rule |
|---|---|---|
| `Import-Module MyModule` | `command` → `command_name:` = `"Import-Module"` (case matters for text compare — see Open Questions on case-insensitivity), `command_elements` contains a `generic_token` = `"MyModule"` | `command_name` text equals `"Import-Module"` (case-insensitive compare recommended) → `Import` ref, name = first `generic_token` in `command_elements` |
| `using module MyModule` | `command` → `command_name:` = `"using"`, `command_elements` = `[generic_token("module"), generic_token("MyModule")]` | `command_name` text equals `"using"` AND first `generic_token` text equals `"module"` → `Import` ref, name = second `generic_token` |
| `. .\lib\helpers.ps1` (dot-sourcing) | `command` → has a `command_invokation_operator` child (`.` token, NO field name) AND `command_name:` = `(command_name_expr (command_name))` (note: **wrapped** in `command_name_expr`, unlike the plain-call case where `command_name` is bare) | Presence of `command_invokation_operator` child distinguishes this from a plain call; verified query `(command (command_invokation_operator) command_name: (command_name_expr (command_name) @path))` correctly captures `.\lib\helpers.ps1` as the path |

All three are call-shaped-import patterns — the exact "Lua `require()` re-tagged as `Import`" precedent FEATURES.md already identifies. **Order matters:** check dot-sourcing (has `command_invokation_operator`) and `using module` (name-text match) BEFORE falling through to the generic `Call` query, or these will be double-counted as calls.

### Pattern 5: Astro — frontmatter + script blocks (near-1:1 with `svelte.rs`)

**Verified AST shape** (document with frontmatter, a `<script>` tag, and a template interpolation):
```
document
  frontmatter                          ← AT MOST ONE per document; absent entirely if no `---` fence (verified: no-frontmatter case has no `frontmatter` node at all, just `element` directly under `document`)
    ---
    frontmatter_js_block                ← the raw TS/JS source, analogous to svelte's `raw_text` under `script_element`
    ---
  script_element                        ← zero or more; IDENTICAL shape to Svelte's `script_element`
    start_tag
      tag_name ("script")
      attribute                          ← e.g. lang="ts" — SAME node kinds as svelte.rs's detect_script_lang expects:
        attribute_name                   #   attribute_name == "lang"
        quoted_attribute_value             #   quoted_attribute_value > attribute_value == "ts"/"typescript"
    raw_text                             ← inner JS/TS source (find via same `find_raw_text` logic as svelte.rs)
    end_tag
  element                                ← template markup
    html_interpolation                   ← `{expr}` (Astro's equivalent of Svelte's mustache; NOT in scope per Deferred Ideas — do not extract)
```

**Confirmed via direct AST dump: `svelte.rs`'s `detect_script_lang` function ports to `astro.rs` with literally zero changes to its node-kind string literals** (`start_tag`, `attribute`, `attribute_name`, `quoted_attribute_value`, `attribute_value` all match exactly). `find_raw_text` (looks for a `raw_text` child) and `collect_script_elements`/stop-at-`script_element`-boundary recursion also port unchanged.

**Astro-specific difference from Svelte:** there is no `lang` attribute on the frontmatter fence (frontmatter has no `start_tag`/attributes at all — it's just `--- frontmatter_js_block ---`). **Always treat the frontmatter block as `Language::TypeScript`** when calling `extract_ecmascript` (Astro frontmatter is unconditionally TS-capable; there is no plain-JS frontmatter mode to detect). `<script>` tags use the same `detect_script_lang` (default `JavaScript`, `lang="ts"`/`"typescript"` → `TypeScript`) as Svelte.

**No-frontmatter documents parse cleanly** (confirmed: `<div>hi {1+1}</div>` produces a `document` with only an `element` child, no `frontmatter` node) — mirror svelte.rs's `no_script_block_emits_single_module_symbol_and_root_scope` test shape: a script-less/frontmatter-less Astro file still emits exactly one Module symbol spanning the document.

**Multiple `<script>` blocks + frontmatter together:** merge exactly like svelte.rs merges multiple `<script>` blocks — one document-spanning root `Module` scope (index 0), each block's own scopes shifted by `scope_base` and re-parented under the doc root, one synthesized `Module` symbol after the loop (filter out per-block `SymbolKind::Module` symbols the same way). The frontmatter block is just one more "block" in this same merge loop, processed identically to a `script_element` block except for the always-TypeScript language choice and locating its inner text via `frontmatter_js_block` (find by kind) instead of `raw_text`.

### Anti-Patterns to Avoid

- **Using `field_text(&child, "name", bytes)` for PowerShell function/class/method names** — bash's `name:` field convention does NOT exist in `tree-sitter-powershell`; use `child_text(&child, "function_name"/"simple_name", bytes)` (find-by-kind) instead. Verified: `field_name_for_child` returns `None` for these positions.
- **One combined tree-sitter query with an optional capture (`(simple_name)? @base_name`) for PowerShell class+base+method** — produces spurious duplicate matches (verified 3 matches for 2 classes). Use a manual node-child walk instead, exactly like `shell.rs`'s manual `collect_symbols` walk.
- **Treating `Import-Module`/`using module`/dot-sourcing as distinct node kinds** — they are all plain `command` nodes; the grammar gives no structural discriminator besides `command_name` text and the presence/absence of `command_invokation_operator`. Text-matching must happen before the generic call-query, in a specific order (see Pattern 4).
- **Detecting a `lang` attribute on Astro frontmatter** — frontmatter has no attributes at all (no `start_tag`); always treat it as `Language::TypeScript`, don't try to reuse `detect_script_lang` on the `frontmatter` node.
- **Assuming PowerShell name matching should be case-insensitive at the extractor level** — FEATURES.md correctly flags PowerShell as fully case-insensitive (commands, params, variables), but no locked decision in CONTEXT.md calls for case-normalizing symbol names or references. Recommend emitting names **as written** in v1 (matches every other extractor's behavior) and treating case-insensitive resolution as a resolver-level concern out of this phase's scope — flag this explicitly in the extractor's doc comment so it isn't silently wrong.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---|---|---|---|
| Call-reference query execution + `@qualifier` capture | A custom query-matching loop | `support::collect_call_references` (already supports optional `@qualifier`) | Zero changes needed to the shared helper; both PowerShell query patterns (cmdlet-style, member-style) work with it directly, verified. |
| Embedded TS/JS extraction (both Astro blocks) | A parallel mini-JS extractor | `super::typescript::extract_ecmascript` + `super::shift_offsets` | Exactly the Svelte precedent; Astro's frontmatter/script content is plain TS/JS, no Astro-specific syntax inside those blocks. |
| Scope-tree merging across multiple embedded blocks (frontmatter + N script tags) | A new merge algorithm | `svelte.rs`'s merge loop shape (doc-root scope at index 0, `scope_base` shift, re-parent block roots) | Proven, tested pattern; Astro's block set (frontmatter + scripts) is a strict superset of Svelte's (scripts only) — same merge shape applies. |
| Module symbol construction | Custom symbol-building logic | `support::module_symbol` + `super::typescript::module_namespaces` | Identical to every other extractor's file-level identity. |

**Key insight:** Neither extractor needs a single new shared helper in `support.rs` — everything routes through helpers already exercised by `shell.rs` and `svelte.rs`. The only genuinely new code is (1) PowerShell's two call-query strings + the import-classification text-matching logic, and (2) Astro's frontmatter-block discovery (one extra node-kind lookup beyond what `svelte.rs` already does for `script_element`).

## Common Pitfalls

### Pitfall 1: Assuming bash-style named fields exist in the PowerShell grammar
**What goes wrong:** Copying `shell.rs`'s `field_text(&child, "name", ctx.bytes)` verbatim for `function_statement`/`class_statement`/`class_method_definition` silently returns `None` for every definition (since none of `function_name`, class/method `simple_name` are named fields in this grammar).
**Why it happens:** `shell.rs` is the template but its bash grammar happens to expose a `name:` field; PowerShell's grammar does not.
**How to avoid:** Use `child_text(node, "<kind>", bytes)` (find-by-kind) for every PowerShell name extraction. Verified: `function_name`, `simple_name` (used for both class names and method names) have no field labels.
**Warning signs:** Definitions silently missing from `FileFacts.symbols` with no panic (a `let Some(name) = ... else { continue }` guard swallows the failure) — write a unit test asserting `facts.symbols` is non-empty before writing any resolver-facing logic.

### Pitfall 2: Double-counting import commands as calls
**What goes wrong:** If the generic `(command command_name: (command_name) @callee)` call query runs over the whole tree without first filtering out `Import-Module`/`using module`/dot-sourcing commands, every import also produces a spurious `Call` reference to `"Import-Module"`/`"using"`/the dot-sourced path.
**Why it happens:** The grammar does not distinguish these at the node-kind level (Pattern 4) — only text and the `command_invokation_operator` marker do.
**How to avoid:** Run import classification first, and either (a) exclude matched command nodes from the call-collection pass, or (b) generate the call list from the same tree walk and skip nodes already classified as imports. The `expected.edges` corpus case (if it includes an import) should assert the import does NOT also produce a spurious `Call` edge.
**Warning signs:** Symbol table resolver would show suspicious edges pointing at nonexistent functions named `"Import-Module"` or `"using"`.

### Pitfall 3: Astro's frontmatter is optional; the extractor must not assume it exists
**What goes wrong:** Code that unconditionally looks for a `frontmatter` child and unwraps it will panic/error on a frontmatter-less `.astro` file (a legitimate, common case — pure-template components).
**Why it happens:** `svelte.rs`'s pattern for `script_element` already handles zero-occurrence gracefully (`Vec` + loop), but a naive Astro port might reach for `.child_by_field_name("frontmatter")` or similar and `.unwrap()` it.
**How to avoid:** Treat frontmatter discovery the same as `svelte.rs` treats script blocks — `Option`, and the "found 0 embedded blocks" case (the `no_script_block_emits_single_module_symbol_and_root_scope` test shape in `svelte.rs`) must still emit exactly one Module symbol + one root scope.
**Warning signs:** A corpus case or unit test using a `.astro` file with only markup (no `---` fence) failing to parse or panicking.

### Pitfall 4: PowerShell's `param()` block parameters are NOT distinguishable from local variables without walking `param_block` specifically
**What goes wrong:** If the Read/Write reference walk (mirroring `shell.rs`'s `collect_read_references`/`collect_write_references`) is copied naively, `$Name` inside `param([string]$Name)` may be misclassified — it's a `variable` node under `script_parameter`, not under `simple_expansion`/`expansion` (bash's read-detection trigger) or `variable_assignment` (bash's write-detection trigger), so it should naturally NOT match either bash-derived rule (correct — it's a Param binding, not a Read/Write). Confirm this with a unit test rather than assuming; the shapes are different enough grammars that copy-paste risk is real.
**Why it happens:** the AST shape differs enough between bash and PowerShell that blind copy-paste of `collect_read_references`/`collect_write_references` logic (which pattern-matches on bash-specific parent-node kinds `simple_expansion`/`expansion`/`variable_assignment`) will not fire at all for PowerShell — PowerShell's plain variable reads are just bare `variable` nodes wherever they appear syntactically (no special "expansion" wrapper — string interpolation is a different, unverified shape not covered by this research spike), and assignment is `assignment_expression` with `left_assignment_expression`/`value:` fields, not `variable_assignment`.
**How to avoid:** Write PowerShell-specific Read/Write detection: **Write** = the `variable` under `left_assignment_expression` of an `assignment_expression`; **Read** = any other bare `variable` node NOT under `left_assignment_expression` and not the parameter-declaration `variable` under `script_parameter`. Verify with a unit test using the confirmed shape from Pattern 1/3 dumps above.
**Warning signs:** Zero Read/Write references emitted at all (silent no-op, since the bash-specific node kinds simply never appear) — this is the most likely failure mode if the extractor is copy-pasted too literally from `shell.rs`.

### Pitfall 5: `command_name` field is reused for two different wrapped shapes
**What goes wrong:** Code that assumes `command_name:` field is always a bare `(command_name)` node will fail to find the dot-sourced path, since in that one case the field's value is `(command_name_expr (command_name))` — one level deeper.
**Why it happens:** the grammar wraps the dot-sourcing target differently from a plain cmdlet name, but both use the same field name `command_name`.
**How to avoid:** When classifying a `command` node, check whether `command_invokation_operator` is present as a sibling FIRST; if so, expect `command_name_expr` wrapping; otherwise expect a bare `command_name`.
**Warning signs:** Dot-sourcing produces no `Import` reference at all (silent `None` from a `field_text` call expecting the wrong shape).

## Code Examples

### PowerShell: verified working call query (both forms)
```rust
// Cmdlet-style — verified against "Get-Process -Name notepad | Stop-Process" → captures BOTH commands.
const CMDLET_CALL_QUERY: &str = r#"
(command
  command_name: (command_name) @callee)
"#;

// Member/expression-style — verified against "$obj.Method()" and "[System.IO.File]::ReadAllText($path)".
const MEMBER_CALL_QUERY: &str = r#"
(invokation_expression
  [(variable) (type_literal)] @qualifier
  (member_name (simple_name) @callee))
"#;
```

### PowerShell: verified import-classification query fragments
```rust
// Import-Module / using module — command_name text match, then first/second generic_token.
// Verified: for "Import-Module MyModule" and "using module MyModule" this query captures
// @cmd = "Import-Module" | "using", @arg = "MyModule" | "module" then "MyModule".
const IMPORT_ARG_QUERY: &str = r#"
(command
  command_name: (command_name) @cmd
  command_elements: (command_elements (generic_token) @arg))
"#;

// Dot-sourcing — verified against ". .\lib\helpers.ps1" → @path = ".\lib\helpers.ps1".
const DOT_SOURCE_QUERY: &str = r#"
(command
  (command_invokation_operator)
  command_name: (command_name_expr (command_name) @path))
"#;
```

### Astro: block discovery shape (mirrors `svelte.rs::collect_script_elements` / `find_raw_text`)
```rust
// Source: verified AST — document has AT MOST ONE `frontmatter` node (direct child),
// zero-or-more `script_element` nodes (found by the same recursive walk svelte.rs uses).
fn find_frontmatter<'a>(root: &Node<'a>) -> Option<Node<'a>> {
    root.children(&mut root.walk()).find(|n| n.kind() == "frontmatter")
}

fn frontmatter_js_block<'a>(frontmatter: &Node<'a>) -> Option<Node<'a>> {
    frontmatter.children(&mut frontmatter.walk()).find(|n| n.kind() == "frontmatter_js_block")
}
// script_element discovery + raw_text + detect_script_lang: reuse svelte.rs's functions verbatim
// (node kinds `script_element`, `raw_text`, `start_tag`, `attribute`, `attribute_name`,
// `quoted_attribute_value`, `attribute_value` all confirmed identical).
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|---|---|---|---|
| N/A — both languages new to this project | Both grammars already registered + ABI-verified in Phase 1 | Phase 1 (2026-07-05) | Phase 2 starts directly at the extractor/enum/dispatch steps — no grammar-compat work remains. |

**Deprecated/outdated:** None applicable — this is greenfield extractor work on already-vetted grammars.

## Open Questions

1. **PowerShell case-insensitivity — extractor-level normalization or not?**
   - What we know: PowerShell is fully case-insensitive (cmdlet names, parameters, variables) per FEATURES.md; no other extractor in this codebase normalizes case.
   - What's unclear: whether Tier-A/Tier-B resolution should case-fold PowerShell names, and if so, whether that belongs in the extractor (normalize `name` at emission) or the resolver (a language-aware compare).
   - Recommendation: emit names as-written for v1 (matches every existing extractor's behavior, and matches the phase's stated scope of "table-stakes" extraction); do not silently normalize. Document the gap in the extractor's module doc comment and `docs/supported-languages.md`'s Notes column, so it's an honest, visible ceiling rather than an invisible under-match. Do not treat as blocking — this is a resolver-tier enhancement, not an extraction-tier requirement.

2. **PowerShell `using namespace System.Text` (not `using module`) — is it in scope?**
   - What we know: D-02 explicitly lists `using module Name` as an import form to emit; `using namespace X` (a .NET-namespace import, distinct from a module import) was not in the tested snippet set and its exact AST shape (whether `command_name` = "using" + `generic_token`="namespace" behaves identically to the "module" case) was not directly verified, though the grammar shape strongly suggests it mirrors `using module` (same `command` structure, different second `generic_token` value).
   - What's unclear: whether `using namespace` should also emit an `Import` reference, or is out of scope (CONTEXT.md's D-02 only names `using module`).
   - Recommendation: treat `using namespace` as out of scope for this phase (D-02 does not list it) unless the planner wants to extend the same detection rule (trivial to add: same query, checking for `generic_token` text `"namespace"` instead of `"module"`) — flag as a one-line follow-up, not a blocker.

3. **Astro `<script>` tags with `is:inline` or other Astro-specific directive attributes**
   - What we know: the verified `detect_script_lang` shape only checks for a `lang` attribute; Astro's real-world `<script>` tags sometimes carry `is:inline`, `define:vars`, etc.
   - What's unclear: whether these other attributes affect whether the script content is still plain JS to extract, or whether some (`define:vars`) inject template-expression-like content that isn't pure JS.
   - Recommendation: out of scope for this phase (Deferred Ideas explicitly excludes template-expression depth); treat every `<script>` block's `raw_text` as extractable JS/TS regardless of other attributes, matching Svelte's existing behavior of ignoring non-`lang` attributes.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|---|---|---|---|---|
| Rust stable / cargo | Building/testing both extractors | ✓ | (project MSRV 1.85, edition 2024) | — |
| `tree-sitter-powershell` 0.26.4 | LANG-08 | ✓ | Already in `Cargo.lock`/`Cargo.toml` as optional dep | — |
| `tree-sitter-astro-next` 0.1.1 | LANG-10 | ✓ | Already in `Cargo.toml` as optional dep | — |
| Node.js | BIND-02 (napi build) | ✓ | v22.23.0 (via nvm/Herd) | — |
| npm / npx | BIND-02 (napi build) | ✓ | 10.9.8 | — |
| `bindings/node/node_modules` (incl. `@napi-rs/cli`) | BIND-02 (`npx napi build`) | ✗ (not yet installed) | — | Run `npm ci` in `bindings/node` before `npx napi build --release --platform`; this is a one-time setup step, not a blocker. |
| Python + maturin | Python binding parity (manual verification, no automated gate per PROJECT.md blocker note) | not probed (out of scope — BIND-02 is napi-specific per CONTEXT.md D-09; Python has no automated drift gate) | — | Manual `maturin build --release -m bindings/python/Cargo.toml` if the planner wants an extra manual check, per PROJECT.md's known infra gap. |

**Missing dependencies with no fallback:** None.

**Missing dependencies with fallback:** `bindings/node/node_modules` — install via `npm ci` as the first step of any BIND-02 task; this exactly mirrors `.github/workflows/test.yml`'s `bindings` job (`npm ci` then `npx napi build --release --platform`, working directory `bindings/node`), so the local verification step is a direct, faithful mirror of CI.

## Validation Architecture

`workflow.nyquist_validation` is explicitly `false` in `.planning/config.json` — this section is skipped per the skip condition.

## Sources

### Primary (HIGH confidence)
- Direct execution: throwaway `examples/ast_dump.rs` (field-annotated recursive tree printer) run via `cargo run --example ast_dump --no-default-features --features powershell,astro` against `tree-sitter-powershell` 0.26.4 and `tree-sitter-astro-next` 0.1.1 (the exact pinned `Cargo.toml` versions) — 9 representative snippets covering every D-02/D-04 requirement (function/filter, class+method+inheritance, all 3 import forms, both call forms, variable assignment/read, frontmatter-only, script tag with/without `lang="ts"`, template `{expr}`, no-frontmatter). File deleted after research per CONTRIBUTING's throwaway-example convention.
- Direct execution: throwaway `examples/ast_query_test.rs` (`Query`/`QueryCursor` runner) verifying 6 proposed query patterns against the same grammar/crate versions — confirmed correct captures for cmdlet-calls, member-calls-with-qualifier, Import-Module/using-module argument extraction, dot-sourcing path extraction, and variable nodes; also confirmed the class-query duplicate-match gotcha. File deleted after research.
- `CONTRIBUTING.md` §"Adding a Language", §"Embedded / single-file-component languages", §"Tip: dump the real AST" — repo, read in full.
- `src/extract/shell.rs`, `src/extract/svelte.rs`, `src/extract/support.rs`, `src/lang.rs`, `src/extract/dispatch.rs`, `src/extract/mod.rs`, `src/extract/csharp.rs` (inheritance/`base_list` pattern precedent) — repo source, read in full.
- `Cargo.toml`, `src/grammar.rs`, `bindings/node/Cargo.toml`, `bindings/python/Cargo.toml`, `.github/workflows/test.yml` — repo, read in full; confirmed exact current feature-list strings and CI command sequence.
- `eval/src/corpus.rs` (doc comment + `parse_expected`/`Case` struct) — repo, read in full; confirmed `expected.edges` format (`<ref_file>:<ref_line> <ROLE> <def_file>:<def_line>`, `ROLE` = `RefRole` variant name) and corpus directory naming (`eval/corpus/<lang>/<case>/`, `<lang>` = `Language::as_str()`).
- `eval/corpus/shell/scoped_call/`, `eval/corpus/hcl/scoped_call/`, `eval/corpus/go/scoped_call/` — repo, read as format examples; confirmed a small same-file-call golden case is the established minimal shape. Note: `eval/corpus/shell/` already has a case (`scoped_call`) but `eval/corpus/svelte/` has NO corpus case yet (a pre-existing gap tracked separately as DEPTH-01, not this phase's concern) — Astro's new corpus case has no direct Svelte precedent to copy structurally, but the `shell`/`hcl`/`go` cases are sufficient templates for the format.
- `docs/supported-languages.md`, `src/lang.rs`'s `supported_languages_doc_lists_each_primary_extension` test — repo, read in full; confirmed the sync-test only checks that each language's primary extension string (backticked) appears somewhere in the doc, not full row structure.
- Local environment probe: `node --version` (v22.23.0), `npm --version` / `npx --version` (10.9.8), confirmed via Herd-managed nvm; `bindings/node/node_modules` confirmed absent (not yet run).
- `.planning/research/FEATURES.md`, `.planning/phases/02.../02-CONTEXT.md`, `.planning/REQUIREMENTS.md`, `.planning/STATE.md` — repo planning docs, read in full.

### Secondary (MEDIUM confidence)
- None — no WebSearch was needed; all grammar-shape questions were resolved directly and more reliably by parsing real source against the exact pinned crate version (higher-confidence than any external doc, per CONTRIBUTING's own explicit guidance that published grammars diverge from documentation).

### Tertiary (LOW confidence)
- None.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — both crates already pinned, registered, and ABI-verified in Phase 1; no new dependency decisions in this phase.
- Architecture (extractor patterns): HIGH — every node/field name claim was verified by direct execution against the pinned crate versions, not inferred from training data or external docs.
- Pitfalls: HIGH for the 5 documented above (all derived directly from the verified AST dumps and query test failures/successes); MEDIUM for the 3 Open Questions (genuinely undertested edge cases, explicitly flagged as such rather than guessed).

**Research date:** 2026-07-05
**Valid until:** Effectively indefinite for the node/field-name findings (pinned exact crate versions, `Cargo.toml` must not bump them without re-verifying); 30 days for the bindings/CI-environment findings (Node/npm versions, CI workflow shape) in case tooling changes.
