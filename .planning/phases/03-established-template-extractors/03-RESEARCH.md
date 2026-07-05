# Phase 3: Established-Template Extractors - Research

**Researched:** 2026-07-05
**Domain:** tree-sitter extractor implementation for Zig, Objective-C, Fortran, Groovy, SystemVerilog
**Confidence:** HIGH for AST node/field shapes (all verified via real `to_sexp()` / raw-child dumps against the exact pinned crate versions, then deleted per CONTRIBUTING); HIGH for wiring/template/SCIP conventions (read directly from existing extractor source); MEDIUM for Groovy's mature-usage patterns given confirmed grammar immaturity (see Pitfall 1 below).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Objective-C `.h` dispatch (the phase's flagged decision)
- **D-01:** Objective-C claims `.m` and `.mm` ONLY. Bare `.h` stays mapped to C — no content-sniffing, no dual dispatch. This is a documented honest gap (ObjC declarations in headers are extracted as C facts), recorded in the ObjC docs-row note and the extractor's module doc. Rationale: dispatch is extension-based by design; content-sniffing violates the determinism bar; C already owns `.h` as an accepted pre-existing ambiguity (C++ precedent).

#### Groovy `.gradle` scoping (the phase's second flagged decision)
- **D-02:** `.gradle` files ARE dispatched to the Groovy extractor, parsed as plain Groovy — closures/method calls extracted as ordinary facts, with NO Gradle-DSL semantic modeling (no dependency-coordinate interpretation, no task-graph semantics). Documented ceiling in the docs-row note. Rationale: the docs matrix already lists `.gradle` under Groovy's planned extensions; plain-Groovy parse is honest and useful; DSL semantics would be guessing.

#### Per-language extraction targets (table stakes + honest ceilings)
- **D-03:** Zig (template: C + Rust): `fn` definitions (incl. `pub` visibility — Zig has a real public/private signal), struct/enum/union declarations with member fns, `@import("x")` → Imports, call refs with receiver qualifiers on member calls, Read/Write. `comptime` constructs capped at table stakes (extract the declaration, don't evaluate).
- **D-04:** Objective-C (template: C + Swift): `@interface`/`@implementation`/`@protocol`/categories → class-kind symbols with inheritance (`: Base` and `<Protocol>` → IsImplementation), method declarations (+/- selectors as symbol names in selector form e.g. `doThing:withArg:`), message sends `[recv sel:arg]` → Calls with receiver qualifier, `#import`/`@import` → Imports, properties, C functions shared via C-like handling.
- **D-05:** Fortran (template: Pascal/Go): `module`/`program` → module symbols, `subroutine`/`function` (incl. `contains` nesting) → functions, `use` statements → Imports, `call` statements + function refs → Calls, explicit `public`/`private` statements → real Visibility (roadmap criterion). Free-form `.f90` is the target; fixed-form `.f` dispatches but is honestly capped at whatever the grammar yields (documented).
- **D-06:** Groovy (template: Java/Kotlin): classes/interfaces/traits/enums, methods (incl. `def`), fields/properties, `import` statements, calls (incl. paren-less command-expression calls where the AST is unambiguous), inheritance (`extends`/`implements`). Dynamic dispatch/`methodMissing` is a documented ceiling; visibility from modifiers, default-package-visibility honestly `Unknown` where Groovy's implicit-public rule is ambiguous — pick per Java template consistency. **[Research update: `trait` is verified NOT parseable by the pinned tree-sitter-groovy 0.1.2 grammar — see Language-by-Language AST Evidence and Pitfall 1. Treat as a hard grammar-version ceiling, not a gap to close in this phase.]**
- **D-07:** SystemVerilog (template: C): `module`/`interface`/`package`/`class` → symbols, functions/tasks, `import pkg::*` → Imports, `` `include `` → Imports (file-level), module instantiations → TypeRef, function/task calls → Calls. Extensions `.sv`/`.svh`. Simulation/synthesis semantics are out of scope.

#### Wiring, bindings, sequencing (Phase 2 practice repeated per language)
- **D-08:** Full recipe per language: enum variant + `as_str()` + extension dispatch, extractor file reusing `support.rs`, `mod.rs`/`dispatch.rs` wiring, feature gains `_extractors` + flips into `default`, unit tests with real SCIP ids, ≥1 corpus case, docs row 🟠→🟢 (sync-tested), BOTH bindings feature lists in the same change, napi no-op diff verified per plan.
- **D-09:** One plan per language, sequential waves (all plans touch the shared wiring files: Cargo.toml, lang.rs, dispatch.rs, mod.rs, docs, bindings Cargo.tomls). Order by template proximity/risk: Zig → SystemVerilog → Fortran → Groovy → Objective-C (ObjC last — largest surface).
- **D-10:** Verification pattern per plan (established in Phase 2, resolver test-isolation gap deferred): `cargo check --no-default-features --features <lang>` + `cargo test --all-features` + fmt/clippy gates + napi no-op. The roadmap's literal "`cargo test --no-default-features --features <lang>`" criterion is satisfied the same way Phase 2's was judged: production code isolation via cargo check, tests via --all-features, with the pre-existing resolver-test-import gap explicitly referenced (deferred-items.md), unless a plan chooses to fix that gap once for all languages (planner's discretion if cheap).

### Claude's Discretion
- Real AST node names per grammar — MUST come from `to_sexp()` dumps against the exact pinned crate versions (research step), never guessed. **[Done — see Language-by-Language AST Evidence below.]**
- Corpus case content per language (small, role-typed, `scoped_call` shape like Phase 2).
- Whether Fortran fixed-form `.f` gets its own corpus coverage (not required).
- ObjC category naming convention in SCIP descriptors.

### Deferred Ideas (OUT OF SCOPE)
- ObjC `.h` content-sniffing or dual-dispatch — rejected for determinism; revisit only as a project-level decision
- Gradle DSL semantic modeling (dependency coordinates, task graph) — potential future `src/package/` enrichment, not extraction
- Fixing the pre-existing resolver test-module isolation gap for all languages (tracked in Phase 2 deferred-items.md; planner MAY pull it in if trivial)
- SystemVerilog elaboration/parameterization semantics
</user_constraints>

## Summary

All five grammars parse their core constructs cleanly with one real exception: **tree-sitter-groovy 0.1.2 does not implement the `trait` keyword** — `trait Greeter { ... }` parses as a paren-less function call `trait(Greeter)` followed by an unrelated closure block, not a `trait_declaration` node. This directly contradicts CONTEXT.md's D-06 assumption that traits are extractable; the plan must either drop trait support from Groovy's must-haves or explicitly document it as a hard grammar-version ceiling (recommend the latter, worded honestly in the docs row and module doc, matching the project's "never fake it" bar). Additionally, Groovy's grammar requires explicit statement terminators (`;` or real newline handling isn't fully modeled) — omitting a `;` after an untyped field or a `return` statement produces a recoverable `MISSING(";")` node (parsing continues, symbols are still extractable, but `tree.root_node().has_error()` returns `true`). This means realistic idiomatic Groovy source (which relies on newline-as-terminator) will very often have `has_error() == true` per file even though extraction still succeeds — worth a one-line honest note in the Groovy module doc, not a blocker.

The other four grammars (Zig 1.1.2, Objective-C via `tree-sitter-objc` 3.0.2, Fortran 0.6.0, SystemVerilog via `tree-sitter-systemverilog` 0.3.1) parsed every required construct with **zero ERROR nodes**, including multi-part Objective-C selectors, Zig's `@import`/`comptime`/`test` forms, Fortran's `module`/`use only:`/`public`/`private` visibility statements, and SystemVerilog's `module`/`interface`/`package`/`class`/instantiation forms. One Fortran snippet initially produced an ERROR node purely because it violated Fortran's own language rule (an executable statement before a variable declaration in the same scope) — reordering the snippet to declare-then-execute parsed cleanly. This is a real-language-rule finding, not a grammar limitation, and is worth noting in the extractor's doc comment so nobody re-discovers it as a "bug."

**Primary recommendation:** Build all five per D-09's stated order (Zig → SystemVerilog → Fortran → Groovy → Objective-C), following the shared def-shape below; for Groovy, ship class/interface/enum/method/field extraction at full depth and explicitly document trait support as unavailable in this grammar version (not a blank cell to silently fill later — a `Notes` column caveat).

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| LANG-01 | Zig extractor (template: C/Rust) | §Zig AST Evidence — all D-03 constructs verified parseable; `pub` visibility confirmed as an anonymous keyword token requiring a `c.rs`-style `is_pub()` helper (not a named field) |
| LANG-05 | Objective-C extractor (template: C + Swift) | §Objective-C AST Evidence — `@interface`/`@implementation`/`@protocol`/category/property/message-send/import all verified; compound-selector reassembly pattern confirmed via repeated `method:` fields |
| LANG-06 | Fortran extractor (template: Pascal/Go) | §Fortran AST Evidence — module/program/subroutine/function/use/call/public/private all verified; `.f` fixed-form dispatches and parses without modules (no visibility, no `use`) |
| LANG-07 | Groovy extractor (template: Java/Kotlin) | §Groovy AST Evidence — class/interface/enum/method/field/import/call/closure verified; **trait is NOT parseable in this grammar version** — documented ceiling, not a gap to fix |
| LANG-09 | SystemVerilog extractor (template: C) | §SystemVerilog AST Evidence — module/interface/package/`` `include``/class/function/task/instantiation all verified; calls wrapped in `tf_call`/`subroutine_call` nodes |
</phase_requirements>

## Standard Stack

### Core (already pinned in `Cargo.toml` — Phase 1 ABI-verified)

| Feature | Crate | Version | Grammar fn (already in `src/grammar.rs`) |
|---------|-------|---------|-------------------------------------------|
| `zig` | `tree-sitter-zig` | `1.1.2` | `crate::grammar::zig()` |
| `objc` | `tree-sitter-objc` | `3.0.2` | `crate::grammar::objc()` |
| `fortran` | `tree-sitter-fortran` | `0.6.0` | `crate::grammar::fortran()` |
| `groovy` | `tree-sitter-groovy` | `0.1.2` | `crate::grammar::groovy()` |
| `systemverilog` | `tree-sitter-systemverilog` | `0.3.1` | `crate::grammar::systemverilog()` |

All five features currently declare only `dep:tree-sitter-<lang>` (no `_extractors`) in `Cargo.toml` — the plan must add `"_extractors"` to each feature string (matching every shipped extractor) AND add each to the `default = [...]` list, per the recipe.

**Version verification:** re-confirmed directly from the checked-in `Cargo.toml` (Phase 1's pinned, ABI-gated versions) — no registry drift check was needed since these are the exact versions already compiled against in this repo.

### Alternatives Considered

None — Phase 1 already selected these as the sole ABI-compatible crate per language; no alternative grammar exists on crates.io for any of the five that is also `tree-sitter >=0.24, <0.27` compatible.

## Architecture Patterns

### Template mapping (confirmed against existing extractor source, not just CONTEXT.md's guess)

| Language | Primary template file(s) | What to copy structurally |
|----------|---------------------------|----------------------------|
| Zig | `src/extract/c.rs` (declarator/visibility walk shape) + light borrowing from `rust.rs` structure (module-per-file, no package decl) | C's `is_static`-style boolean-keyword-scan pattern (for `pub`), C's file-path namespace derivation, C's aggregate-type-with-body detection pattern (for `const X = struct {...}` → Type symbol) |
| Objective-C | `src/extract/swift.rs` (class/protocol/inheritance/visibility shape) reusing `src/extract/c.rs`'s C-subset handling directly (same `collect_symbols`/`declarator_name` helpers for plain C functions in `.m`/`.mm`) | Swift's `class_declaration`-with-body pattern for `@interface`/`@implementation`, its `IsImplementation` inheritance-ref pattern for `superclass:`/protocol list, PLUS a **new** compound-selector-name-join step with no existing precedent in this codebase |
| Fortran | `src/extract/pascal.rs` (visibility-section / `declType`-body shape) + `src/extract/go.rs` (file="one namespace" simplicity, no nested types) | Pascal's `section_visibility`-style explicit keyword-to-`Visibility` mapping (for `public`/`private` statements), Go's flat top-level-decl walk (Fortran has no nested-type member walk to mirror) |
| Groovy | `src/extract/java.rs` (class/interface/enum/method/field/import shape) — closest structural sibling in the whole codebase | Java's `collect_members`/`read_visibility`/`collect_imports` almost verbatim, with **two required deltas**: (1) default visibility is `Public` when unmarked (opposite of Java's package-private default — CONTEXT.md D-06 already flags this, now empirically confirmed harmless to assume since `modifiers` node is simply absent when no keyword is present), (2) calls need TWO query patterns like PowerShell's dual cmdlet/member style — `method_invocation` (paren calls) AND `juxt_function_call` (paren-less command calls) |
| SystemVerilog | `src/extract/c.rs` (top-level declaration walk, simple namespace-from-path) | C's flat top-level walk and namespace-from-path pattern; SV has no inheritance concept in the table-stakes scope (`class` has no `extends` requirement exercised here — v1 can treat all classes as flat `Type` symbols, matching Go's `Inherit: —` honest precedent) |

### Recommended per-language `Language` enum additions (naming precedent check)

`src/lang.rs` uses PascalCase variants with an `as_str()` matching the Cargo feature name exactly (e.g. `CSharp => "csharp"`, `Cpp => "cpp"`, `PowerShell => "powershell"`). Following that precedent exactly:

```rust
Language::Zig,           // as_str() "zig",           extensions: ["zig"]
Language::ObjC,          // as_str() "objc",           extensions: ["m", "mm"]  (per D-01: NOT "h")
Language::Fortran,       // as_str() "fortran",        extensions: ["f90", "f"]
Language::Groovy,        // as_str() "groovy",         extensions: ["groovy", "gradle"]  (per D-02)
Language::SystemVerilog, // as_str() "systemverilog",  extensions: ["sv", "svh"]
```

Extractor file names (matching feature name, per every existing file): `src/extract/zig.rs`, `src/extract/objc.rs`, `src/extract/fortran.rs`, `src/extract/groovy.rs`, `src/extract/systemverilog.rs`.

### Pattern: Anonymous keyword tokens are NOT named fields — verified for all five

Every visibility/qualifier keyword checked (Zig `pub`, Objective-C `+`/`-` method-type marker) is an **unnamed** grammar token — it does not appear in `to_sexp()` output and is not reachable via `child_by_field_name`. It must be found by scanning `node.children(&mut node.walk())` for a child whose `.kind()` equals the literal token text, exactly like `support::is_static()` already does for C's `static` keyword. Fortran and Groovy's visibility/import keywords ARE exposed as distinct named statement nodes (`public_statement`, `private_statement`) or a `modifiers` wrapper node, so those two don't need this raw-token-scan pattern.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Call-callee query | A custom AST walk for every call shape | `support::collect_call_references()` + a tree-sitter query string (`CALL_QUERY` const), same as every existing extractor | Handles `MIN_REF_LEN`, qualifier capture, and `Reference` construction uniformly |
| Scope tree / bindings | Custom scope-index bookkeeping | `support::push_scope`, `support::innermost_scope`, `support::attach_reference_scopes`, `support::push_binding`, `support::definition_bindings`, `support::import_bindings` | Every Tier-B extractor already shares this; reinventing risks off-by-one scope-index bugs |
| SCIP identity rendering | Manual string formatting of symbol ids | `symbol::Descriptor` variants + `SymbolId::global()` | Handles simple-vs-backtick-quoted identifier escaping automatically (critical for ObjC selectors — see below) |
| Multi-part ObjC selector joining | A one-off string-join scattered in the extractor | A small dedicated helper (no existing precedent — must write new, but keep it a pure `Vec<&str> -> String` function unit-testable in isolation) | Selector reassembly is genuinely novel in this codebase (no existing extractor does compound-name joining); isolating it makes the SCIP-string test assertions readable |

**Key insight:** every one of the five languages' definition/reference/scope mechanics maps onto helpers that already exist in `support.rs` — the only genuinely new code is (1) ObjC's compound-selector name assembly and (2) Groovy's dual call-query (paren + paren-less), both scoped, small, and testable in isolation.

## Language-by-Language AST Evidence

All snippets below were parsed with a throwaway `examples/dump_ast.rs` (deleted after use, per CONTRIBUTING) against the exact pinned crate versions. `to_sexp()` output only shows **named** nodes/fields; anonymous keyword tokens are called out separately where relevant (verified via a raw child-walk that also visits unnamed nodes).

### Zig 1.1.2 — zero ERROR nodes across all constructs

```
pub fn add(a: i32, b: i32) i32 { return a + b; }
→ (source_file (function_declaration name: (identifier)
    (parameters (parameter name: (identifier) type: (builtin_type)) (parameter name: (identifier) type: (builtin_type)))
    type: (builtin_type)
    body: (block (expression_statement (return_expression (binary_expression left: (identifier) right: (identifier)))))))
```
- `pub` is an **anonymous token** (unnamed child, literal text `"pub"`), positioned before `fn` — NOT a named field. Confirmed via raw dump: `function_declaration` for `pub fn add(...)` has a direct unnamed child `pub "pub"`; the non-`pub` variant (`fn helper() void {}`) has no such child. Visibility detection = scan `node.children()` for an unnamed child with kind `"pub"` (mirrors `support::is_static`).
- `const Point = struct { x: i32, y: i32, pub fn magnitude(self: Point) i32 {...} };` → `(variable_declaration (identifier) (struct_declaration (container_field name: (identifier) type: (builtin_type)) (container_field ...) (function_declaration name: (identifier) (parameters (parameter name: (identifier) type: (identifier))) type: (builtin_type) body: (block ...))))`. **Zig has no dedicated `struct` keyword for declaration** — confirmed: the top-level node is `variable_declaration`, and the struct/enum/union body is the RHS. Detection rule: a top-level `variable_declaration` whose value child is `struct_declaration`/`enum_declaration`/`union_declaration` is a Type definition (name = the `variable_declaration`'s `identifier`); its `container_field` children are struct fields, its `function_declaration` children are methods.
- `const Color = enum { Red, Green, Blue };` → `(variable_declaration (identifier) (enum_declaration (container_field name: (identifier)) (container_field name: (identifier)) (container_field name: (identifier))))`. `const Value = union { i: i32, f: f32 };` → same shape with `union_declaration`. Enum/union members are `container_field` (same node kind as struct fields — enum members simply lack a `type:` field).
- `@import("std")` / `@import("./helper.zig")` → `(variable_declaration (identifier) (builtin_function (builtin_identifier) (arguments (string (string_content)))))`. `@import` is a `builtin_function` node whose `builtin_identifier` child's text is literally `@import` — detection: check `builtin_identifier` text equals `"@import"`, then the single string argument's `string_content` is the import path (relative-path resolution: `./x.zig` needs the same "resolves to a sibling file" handling Rust's `mod` gets, per D-03).
- `p.magnitude()` (member call) → `(call_expression function: (field_expression object: (identifier) member: (identifier)))`. Receiver = `field_expression`'s `object:` field, callee = `member:` field — a distinct query shape from a free call `foo()` → `(call_expression function: (identifier))`. Both must be covered in `CALL_QUERY` (mirrors the Swift/Pascal dual free-vs-member-call pattern already in this codebase).
- `test "add works" { ... }` → `(test_declaration (string (string_content)) (block ...))`. The string content is the test's display name (use as the symbol name, not a `SymbolKind` this codebase has a dedicated variant for — recommend `SymbolKind::Function` with a `Descriptor::Method` using the string as name, same shape as any other function).
- `comptime { const x = 5; }` → `(comptime_declaration (block (variable_declaration (identifier) (integer))))` — parses cleanly; per D-03 cap at table stakes: emit nothing special for the comptime body's contents beyond what a normal block would produce (no comptime-specific `SymbolKind` needed).
- **Confirmed via raw child-walk (isolated, re-verified twice): Zig has NO separate `assignment_expression` node kind.** `var count: i32 = 0; count = count + 1; const total = count;` → **all three** statements — the `var` declaration, the bare reassignment, AND the `const` declaration — parse as the SAME node kind `variable_declaration`. The only distinguishing signal is an **anonymous keyword token** (`var` or `const`) as the declaration's first child; the bare reassignment has no such token — its first child is directly the target `identifier`, followed by the RHS expression, with no field labels on either. **This is a real, load-bearing pitfall** (see Pitfall 6 below): a naive "every `variable_declaration` is a new binding" rule would misclassify every reassignment as a redeclaration. Detection rule: `variable_declaration` has a `var`/`const` anonymous-token child ⇒ real definition (emit a `Local`/`Definition` Binding, per D-03's Zig `var`/`const` handling); no such token ⇒ a plain reassignment ⇒ emit a `RefRole::Write` reference for the target identifier (same conceptual role as every other extractor's assignment-LHS handling, just riding a differently-shaped node).

### Objective-C (`tree-sitter-objc` 3.0.2) — zero ERROR nodes across all constructs

```
@interface Base : NSObject
- (void)run;
@end
→ (translation_unit (class_interface (identifier) superclass: (identifier)
    (method_declaration (method_type (type_name (primitive_type))) (identifier))))
```
- `@interface Sub : Base <Proto>` → adds `(parameterized_arguments (type_name (type_identifier)))` sibling to the `superclass:` field — protocol-conformance list. Both `superclass:` (single) and `parameterized_arguments` (protocol list, possibly multiple `type_name` children) map to `RefRole::IsImplementation`, exactly mirroring Swift's `collect_inheritance` pattern.
- `@implementation Sub { ... }` → `(class_implementation (identifier) (implementation_definition (method_definition ...)))`. Method bodies live under `method_definition` (not `method_declaration`, which is interface-only) — table-stakes v1 should emit the symbol from whichever of `@interface`'s `method_declaration` OR `@implementation`'s `method_definition` is present, with the interface declaration preferred as the definition site when both exist (matches D-04's "declared in `@interface`" visibility heuristic).
- `@interface Base (Cat) ... @end` → `(class_interface (identifier) category: (identifier) (method_declaration ...))`. **`category:` IS a named field** — directly usable for category-name capture (CONTEXT.md's "Claude's discretion: ObjC category naming convention" — recommend `Descriptor::Type("Base+Cat")` or a nested namespace segment; either is defensible, but the field itself is trivially extractable).
- `@protocol Proto ... @optional ... @end` → `(protocol_declaration (identifier) (method_declaration ...) (qualified_protocol_interface_declaration (method_declaration ...)))`. Required methods are direct `method_declaration` children; `@optional`-marked methods are wrapped in `qualified_protocol_interface_declaration` — both should still emit a Method symbol (v1 does not need to distinguish required-vs-optional as a separate fact).
- **Multi-part selector — the critical finding.** `[obj compute:1 with:2]` → `(message_expression receiver: (identifier) method: (identifier) (number_literal) method: (identifier) (number_literal))`. The selector is **NOT** one node — it is a **repeated `method:` field**, each followed by its argument expression as an unlabeled sibling. Confirmed identically on the declaration side: `- (int)compute:(int)x with:(int)y;` → `(method_declaration (method_type ...) (identifier) (method_parameter (method_type ...) (identifier)) (identifier) (method_parameter (method_type ...) (identifier)))` — i.e. the declaration's selector pieces are the (unlabeled) `identifier` children interleaved with `method_parameter` nodes, one `identifier` per colon-part. **Extraction rule: iterate all children (not just fields), collect every `identifier` that is a *direct* child (not nested inside `method_type`/`method_parameter`) in order, join with `":"`, append a trailing `":"` — this reconstructs `"compute:with:"`.** Zero-arg selectors (`- (void)run;`) have exactly one such `identifier` and no trailing colon (confirmed: `run` renders bare, not `run:`).
- `@interface Foo\n+ (instancetype)shared;\n- (void)run;\n@end` → raw dump confirms **`+`/`-` are anonymous single-character tokens**, first child of `method_declaration`/`method_definition`, kind literally `"+"` or `"-"`. Class-vs-instance-method distinction requires a raw unnamed-child scan (same pattern as Zig's `pub`).
- `@property (nonatomic, strong) NSString *token;` → `(property_declaration (property_attributes_declaration (property_attribute (identifier)) (property_attribute (identifier))) (struct_declaration (type_identifier) (struct_declarator (pointer_declarator declarator: (identifier)))))`. The property name is the innermost `pointer_declarator`'s `declarator:` field — reuse `c.rs`'s `declarator_name()` helper directly (same declarator-chain shape as C).
- `#import "Session.h"` / `#import <Foundation/Foundation.h>` / `@import ModuleName;` → three distinct node kinds: `(preproc_include path: (string_literal (string_content)))`, `(preproc_include path: (system_lib_string))`, `(module_import path: (identifier))`. All three map to `RefRole::Import`; per D-01/existing C precedent, `Imports: —` in the docs matrix column is **not** required to change — CONTEXT.md D-04 explicitly says `#import`/`@import` → Imports, so this phase DOES fill that cell for ObjC (unlike C's honest `—`), since `@import ModuleName` gives a genuine symbolic module reference distinct from C's purely textual `#include`.
- Plain C function (`int add(int a, int b) { return a + b; }`) → identical `function_definition`/`declarator`/`parameter_list` shape as `c.rs` already handles — direct reuse confirmed, zero deltas needed for the C subset.

### Fortran 0.6.0 — zero ERROR nodes once Fortran's own declare-before-execute rule is respected

```
module mymod
  implicit none
  public :: add
  private :: helper
contains
  function add(a, b) result(c)
    integer, intent(in) :: a, b
    integer :: c
    c = a + b
  end function add
  subroutine helper(x)
    integer, intent(in) :: x
  end subroutine helper
end module mymod
→ (translation_unit (module (module_statement (name)) (implicit_statement (none))
    (public_statement (identifier)) (private_statement (identifier))
    (internal_procedures (contains_statement)
      (function (function_statement name: (name) parameters: (parameters (identifier) (identifier)) (function_result (identifier))) ... (end_function_statement (name)))
      (subroutine (subroutine_statement name: (name) parameters: (parameters (identifier))) ... (end_subroutine_statement (name))))
    (end_module_statement (name))))
```
- **`module_statement`'s module-name child is NOT a named field** — confirmed via raw dump: it's a bare `(name)` child with no field label, unlike `function_statement`/`subroutine_statement` where `name:` **is** a named field. Namespace derivation for Fortran: scan `module_statement`/`program_statement` children for a child of kind `"name"` directly (positional), but use `child_by_field_name("name")` for `function_statement`/`subroutine_statement` (this asymmetry is a real gotcha worth a code comment).
- `public :: add` / `private :: helper` → `(public_statement (identifier))` / `(private_statement (identifier))` — clean, unambiguous, **exactly** the real Visibility signal CONTEXT.md's D-05/Specific Ideas calls for. Confirmed multi-name form: `public :: add, sub` → `(public_statement (identifier) (identifier))` (multiple identifiers, comma-separated — iterate all `identifier` children, not just the first).
- `use mymod` / `use other_mod, only: thing` → `(use_statement (module_name))` / `(use_statement (module_name) (included_items (identifier)))`. Bare `use` has no import-list (whole-module import, like Rust's glob or Java's wildcard — emit one `Import` ref for the module name); `use ..., only: ...` has an `included_items` wrapper with one `identifier` per named import — genuinely precise, symbol-level imports (matches FEATURES.md's "genuine differentiator" claim).
- `call helper(1)` (subroutine invocation) → `(subroutine_call subroutine: (identifier) (argument_list (number_literal)))` — **a distinct node kind from function calls**. `r = add(1, 2)` (function invocation, used as an expression) → `(assignment_statement left: (identifier) right: (call_expression (identifier) (argument_list (number_literal) (number_literal))))`. Confirms D-05/FEATURES.md's "syntactically different call-site shapes" — the extractor needs TWO call queries: one for `subroutine_call`'s `subroutine:` field, one for `call_expression`'s callee (its first, unlabeled `identifier` child — NOT a named field, confirmed by the sexp showing `(call_expression (identifier) (argument_list ...))` with no field prefix on the identifier).
- `program main ... end program main` → same `program`/`program_statement`/`end_program_statement` shape as `module`, minus `internal_procedures` wrapping (statements are direct children of `program`).
- **Fixed-form `.f`** (`      PROGRAM MAIN\n      INTEGER I\n      I = 1\n      CALL FOO(I)\n      END`) parses **cleanly, zero errors**, producing the identical node shape as free-form (`program`, `variable_declaration`, `assignment_statement`, `subroutine_call`, `end_program_statement` — the only visible difference is `end_program_statement` here has no trailing `(name)` child since `END` alone was written, vs. `END PROGRAM MAIN`). Per D-05, fixed-form gets whatever the grammar yields "as is" — confirmed this is a fully usable subset (declarations, assignment, subroutine calls), not a degraded parse. No module/`use`/visibility concepts exist in this snippet (correctly absent, matching the honest fixed-form ceiling).
- **The one ERROR encountered was a genuine Fortran-rule violation in the first test snippet**, not a grammar bug: placing `integer :: r` (a declaration) *after* `call helper(1)` (an executable statement) inside the same subroutine body produced `(ERROR (block_label_start_expression) (number_literal))`. Reordering to declare-then-execute parsed with zero errors (re-verified). **Document this in the extractor module doc**: Fortran requires all declarations before the first executable statement in a scoping unit; a real-world file violating this is itself invalid Fortran, so an ERROR node here should be treated as "genuinely malformed input," not a signal to loosen the extractor.

### Groovy (`tree-sitter-groovy` 0.1.2) — real gaps found; grammar is genuinely immature

```
class Foo extends Base implements Iface1, Iface2 { ... }
→ (class_declaration name: (identifier) superclass: (superclass (type_identifier))
    interfaces: (super_interfaces (type_list (type_identifier) (type_identifier)))
    body: (class_body ...))
```
- Class/superclass/interfaces shape is clean and Java-like, as expected — `superclass:` and `interfaces:` are both named fields, directly portable from `java.rs`'s `collect_inheritance`.
- `public class Foo {}` → `(class_declaration (modifiers) name: (identifier) body: (class_body))`; `private def secret() {...}` on a method → `(method_declaration (modifiers) ...)`; `public int x;` field → `(field_declaration (modifiers) ...)`. **`modifiers` is present as a wrapper node only when an explicit keyword exists** — absent entirely for unmarked members (confirmed: unmarked `def name` / `String typed` fields have NO `modifiers` child at all). This directly confirms CONTEXT.md D-06's note: Groovy's default-unmarked visibility should map to `Visibility::Public` (the *absence* of `modifiers` is the "no explicit modifier" signal, same detection shape as Java's `read_visibility` but with the opposite fallback value).
- **`trait` keyword is NOT implemented by this grammar version.** `trait Greeter { def greet() {} }` parses as: `(juxt_function_call name: (identifier "trait") args: (argument_list (identifier "Greeter"))) (expression_statement (closure ...))` — i.e., the parser reads `trait Greeter` as a paren-less command call to a function literally named `trait` with one argument `Greeter`, and the following `{ ... }` block is parsed as a **separate, unrelated closure expression statement**, not the trait's body. This is a hard grammar-version ceiling, not an extraction-logic gap: **there is no `trait_declaration` node to walk.** Recommendation: drop `trait` from Groovy's must-haves for this phase (contradicts CONTEXT.md D-06's listed target, which was written before this grammar was verified) and add an explicit docs-row note ("`trait` keyword unsupported by tree-sitter-groovy 0.1.2 — parses as an unrelated call+closure pair, not extracted"). `interface Greeter { def greet() }`, by contrast, **works cleanly**: `(interface_declaration name: (identifier) body: (interface_body (method_declaration ...)))`.
- **Missing-semicolon recovery, not a hard failure.** `def name` (untyped field, no trailing `;`, followed by a newline and the next member) parses as `(field_declaration type: (type_identifier "def") declarator: (variable_declarator name: (identifier)) (MISSING ";"))` — the parser inserts a synthetic MISSING token and **keeps going**; subsequent siblings (another field, a method) still parse correctly and are still walkable via the normal tree — but `tree.root_node().has_error()` becomes `true` for the whole file. Confirmed the same MISSING(";") pattern on an unterminated `return 1` inside a method body. **This means realistic Groovy source without semicolons will report `has_error() == true` per-file even when every definition/reference is still successfully extracted** — worth a one-line honest note in the extractor's module doc so a future contributor doesn't treat `has_error()` as "extraction failed."
- `println "hi"` (paren-less call) at any position → `(juxt_function_call name: (identifier) args: (argument_list (string_literal (string_fragment))))`. `println("hi")` (parenthesized) → `(method_invocation name: (identifier) arguments: (argument_list (string_literal (string_fragment))))`. **These are two distinct node kinds** requiring two `CALL_QUERY` patterns (mirrors PowerShell's dual cmdlet/expression-call pattern) — confirmed both work standalone; the earlier full-file test's spurious ERROR near a `println "hi"` call was caused by the *preceding* unterminated field declarations' MISSING-token recovery cascading, not by `println` itself (isolated retest confirms `println "hi"` alone parses with zero errors).
- `import pkg.Thing` / `import static pkg.Util.helper` / `import pkg.other.*` → all three render as `(import_declaration (scoped_identifier scope: ... name: ...))`, the wildcard form additionally carrying a trailing `(asterisk)` sibling — directly portable from `java.rs`'s `collect_imports` wildcard-detection pattern (`asterisk` sibling ⇒ skip, same as Java). **Static imports are NOT distinguishable via a named node** in the sexp — the `static` keyword's own token wasn't visible as a named child in this grammar's import shape; if distinguishing static imports matters, verify with a raw unnamed-child scan before assuming Java's `"static"` keyword detection pattern transfers unchanged.
- `def run() { def c = { x -> x + 1 }; [1,2].each { it * 2 } }` → closures parse cleanly as `(closure (lambda_expression parameters: (identifier) body: ...))` for an explicit-param closure, and a bare `(closure (binary_expression ...))` for an implicit-`it` closure (`each { it * 2 }`) — a trailing-closure argument to `.each(...)` appears as a `body:` field on the enclosing `method_invocation`, confirming FEATURES.md's prediction that Gradle-DSL-style trailing closures (`foo { ... }`) are call-with-trailing-closure-argument, not a special construct — same shape applies to `.gradle` files per D-02.
- `enum Color { RED, GREEN, BLUE }` → clean `(enum_declaration name: (identifier) body: (enum_body (enum_constant name: (identifier)) ...))`, directly portable from Java's enum handling.

### SystemVerilog (`tree-sitter-systemverilog` 0.3.1) — zero ERROR nodes across all constructs

```
module adder(input logic [7:0] a, input logic [7:0] b, output logic [7:0] sum);
  assign sum = a + b;
endmodule
→ (source_file (module_declaration (module_ansi_header (module_keyword) name: (simple_identifier)
    (list_of_port_declarations (ansi_port_declaration (variable_port_header (port_direction) (variable_port_type ...)) port_name: (simple_identifier)) ...))
    (continuous_assign ...)))
```
- Module name is `module_ansi_header`'s `name:` field (a `simple_identifier`), NOT on `module_declaration` directly — must descend one level. Port direction (`input`/`output`/`inout`) is a `port_direction` node (its text is the keyword) inside `variable_port_header`/`net_port_header` — table-stakes v1 does not need port direction for the required facts (symbols + calls + imports), but it's there if a future phase wants port-level Type-ref depth.
- `interface bus_if; logic clk; logic [7:0] data; endinterface` → `(interface_declaration (interface_ansi_header name: (simple_identifier)) (data_declaration ...) ...)` — same `name:`-one-level-down pattern as module.
- `package mypkg; parameter int WIDTH = 8; endpackage` → `(package_declaration name: (simple_identifier) (package_item (parameter_declaration ...)))` — **`name:` IS a direct field on `package_declaration` itself** (unlike module/interface, which nest it under an `_ansi_header`). `import mypkg::*;` inside a module → `(data_declaration (package_import_declaration (package_import_item (simple_identifier))))` — the package name is the `simple_identifier` inside `package_import_item`; a real `Import` ref (package-qualified, `::*` wildcard confirmed present in the raw source but not surfaced as a distinct wildcard node — the whole `pkg::*` collapses to one `simple_identifier` capture, so wildcard-vs-named import is not distinguishable from this node alone; treat all `import pkg::*` the same as a whole-package import, matching Fortran's bare `use` semantics).
- `` `include "defs.svh" `` → `(include_compiler_directive (quoted_string (quoted_string_item)))` — file-level, textual, same honest "no symbol-level import list" ceiling as C's `#include` (per D-07, this still maps to `RefRole::Import` at the file-path granularity, matching how ObjC's `#import` is handled).
- `class Packet; int data; function new(); ... endfunction function int get_data(); ... endfunction task run(); ... endtask endclass` → `(class_declaration name: (simple_identifier) (class_item (class_property ...)) (class_item (class_method (class_constructor_declaration ...))) (class_item (class_method (function_declaration (function_body_declaration ... name: (simple_identifier) ...)))) (class_item (class_method (task_declaration (task_body_declaration name: (simple_identifier) ...)))))`. **Constructors (`function new()`) are a distinct node kind (`class_constructor_declaration`) with NO `name:` field** (its name is implicitly `new` — hardcode it, same as ObjC/Swift's `init`). Regular functions/tasks both expose `name:` as a direct field on their inner `*_body_declaration` node.
- Function/task calls (`get_data()` called from inside `run()`) → `(function_subroutine_call (subroutine_call (tf_call (hierarchical_identifier (simple_identifier)))))` — the callee name is the `simple_identifier` inside `hierarchical_identifier` inside `tf_call`; this three-level wrapper is consistent for both function and task invocations (no separate node kind distinguishing "calling a function" vs "calling a task" at the call site — matches ordinary SV semantics where task/function calls are syntactically identical in this context).
- Module instantiation (`adder u_adder(.a(a), .b(b), .sum(sum));`) → `(module_instantiation instance_type: (simple_identifier) (hierarchical_instance (name_of_instance instance_name: (simple_identifier)) (list_of_port_connections (named_port_connection port_name: (simple_identifier) connection: (expression ...)) ...)))`. `instance_type:` is the field to use for a `RefRole::TypeRef` (per D-07, "module instantiations → TypeRef") — the module being instantiated (`adder`) is directly captured there; `instance_name:` is the local instance's own name (not itself a new symbol definition in v1 scope, matches the "no elaboration/parameterization semantics" deferred item).

## Common Pitfalls

### Pitfall 1: Groovy grammar immaturity causes real, not cosmetic, gaps
**What goes wrong:** Assuming a mainstream Java-like construct (traits) "obviously" works because Java-family languages usually support it.
**Why it happens:** `tree-sitter-groovy` 0.1.2 is a young (per FEATURES.md, already-flagged-as-risky) crate; its grammar coverage is incomplete relative to the real language.
**How to avoid:** Verify every construct against a real `to_sexp()` dump before writing extraction logic for it (done here) — never assume Java-template parity holds 1:1 for Groovy.
**Warning signs:** A construct that "should" produce a dedicated node kind instead produces a generic `juxt_function_call`/`method_invocation` plus an unrelated trailing `closure` — that shape is this grammar's signature for "keyword not recognized."

### Pitfall 2: Anonymous keyword tokens are invisible to `to_sexp()`
**What goes wrong:** Concluding a language "has no visibility signal in the AST" because `pub`/`+`/`-` don't show up in a plain `to_sexp()` dump.
**Why it happens:** `to_sexp()` only prints named nodes and named fields; unnamed keyword tokens are silently omitted.
**How to avoid:** For any construct where a keyword's presence/absence is semantically load-bearing (Zig `pub`, ObjC `+`/`-`), do a raw child walk (`node.children()` including unnamed, checking `.kind()` against the literal token text) before concluding the signal is unavailable — confirmed present for both Zig and ObjC in this research.
**Warning signs:** A `to_sexp()` dump for two semantically-different snippets (e.g. `pub fn x` vs `fn x`) looks byte-identical at the named-node level.

### Pitfall 3: Fortran's positional vs. field-labeled `name` child is asymmetric
**What goes wrong:** Writing one `field_text(node, "name", ...)` helper call and applying it uniformly across `module_statement`/`program_statement`/`function_statement`/`subroutine_statement`.
**Why it happens:** `function_statement`/`subroutine_statement` expose `name:` as a genuine named field, but `module_statement`/`program_statement` do NOT — their name is a bare positional `(name)` child with no field label.
**How to avoid:** Use `child_by_field_name("name")` for function/subroutine statements; scan children for `kind() == "name"` for module/program statements. Verified via raw dump.
**Warning signs:** `field_text(module_stmt, "name", bytes)` silently returns `None` for a syntactically valid `module Foo` statement.

### Pitfall 4: ObjC selector reconstruction must NOT rely on named fields
**What goes wrong:** Trying to find a single "selector" field on `message_expression`/`method_declaration`.
**Why it happens:** There isn't one — SCIP-worthy compound selectors (`compute:with:`) are reassembled from a **repeated, unlabeled `method:` field** interleaved with argument/parameter nodes.
**How to avoid:** Iterate all children of `message_expression`/`method_declaration` in document order, collect every node occupying the `method:` field slot (there can be 0, 1, or N), join their texts with `":"`, and append a trailing `":"` only when N ≥ 1. Verified against both a zero-arg (`run`) and two-arg (`compute:with:`) case.
**Warning signs:** Only the first selector piece gets captured, silently truncating multi-keyword selectors to their first segment.

### Pitfall 5: SCIP disambiguator/name escaping applies automatically — don't fight it
**What goes wrong:** Worrying that a colon-containing selector name (`compute:with:`) will produce an invalid SCIP string.
**Why it happens:** `Descriptor::render`'s `push_ident` helper backtick-quotes any non-"simple" identifier (colons are not in `is_simple_ident_char`'s alphanumeric/`_`/`+`/`-`/`$` set) automatically.
**How to avoid:** Just pass the joined selector string as the `Descriptor::Method { name, .. }`'s `name` field — the renderer will produce `` `compute:with:`(). `` automatically; no manual escaping needed in the extractor. Verified by reading `src/symbol/descriptor.rs`'s `push_ident`/`is_simple_ident_char`.
**Warning signs:** Writing custom quoting logic in the ObjC extractor that duplicates what `Descriptor`/`SymbolId` already do.

### Pitfall 6: Zig has no `assignment_expression` node — declaration and reassignment share one node kind
**What goes wrong:** Treating every `variable_declaration` node as a new binding/definition, which would misclassify plain reassignments (`count = count + 1;`) as redeclarations, or conversely writing a Write-reference collector that only looks for a (nonexistent) `assignment_expression` kind and silently emits nothing for Zig.
**Why it happens:** Confirmed via isolated raw child-walk (re-verified twice, once mixed with other statements and once alone): `tree-sitter-zig` uses the SAME `variable_declaration` node kind for `var x = 1;`, `const x = 1;`, AND a bare `x = 2;` reassignment with no keyword at all. The only distinguishing feature is the presence/absence of an anonymous `var`/`const` keyword token as the first child (same "anonymous token" shape as Pitfall 2's `pub` check).
**How to avoid:** In the Zig extractor's binding/reference walk, branch on `variable_declaration`: has a `var`/`const` first-child token ⇒ Definition/Local Binding (+ TypeRef if a `type:` field is present); no such token ⇒ `RefRole::Write` for the target identifier, with the RHS still walked normally for `RefRole::Read`s.
**Warning signs:** Zig's Local-binding count matches its reassignment count 1:1 in a test file that has more reassignments than declarations (an obvious duplicate-binding bug).

## Open Questions

1. **Groovy static-import keyword detection**
   - What we know: `import static pkg.Util.helper` renders identically (at the named-node level) to a plain multi-segment import — the `static` keyword itself wasn't visible in the `to_sexp()` output.
   - What's unclear: whether `static` is an anonymous token (needs a raw-child scan, per Pitfall 2) or genuinely absent from the parse tree entirely (meaning static imports can't be distinguished from instance imports in this grammar version).
   - Recommendation: if the plan wants to distinguish static imports (not required by D-06's stated must-haves — Groovy's target list only says "import statements", not static-vs-instance), do the raw-child check first. Otherwise, table-stakes v1 can treat all Groovy imports identically (matches Java's existing precedent of also just emitting `Import` refs uniformly regardless of `static`).

## Corpus Case Format Reminder

`eval/corpus/powershell/scoped_call/` shape (copy directly):
```
eval/corpus/<lang>/<case_name>/
  <source-file>.<ext>
  expected.edges
```
`expected.edges` format: one edge per non-comment line, `<file>:<line> <EdgeRole> <file>:<line>` (role matches `EdgeKind`/`RefRole` names, e.g. `Call`), with `#`-prefixed comment lines explaining what the case proves. Comment style: state what's being proven and why (e.g. "Proves cmdlet-style (parenless) call detection resolves via Tier-A same-file matching"). Recommend one small `scoped_call`-shaped case per language for this phase (per CONTEXT.md's "Claude's Discretion"), sized similarly to PowerShell's (~10 lines of source, one same-file resolved edge).

## Docs Matrix Column Semantics (per `docs/supported-languages.md`)

Current rows for all five are 🟠 (planned) with no filled capability columns. Per this research, the honest target fills for each language's row (🟠→🟢 transition):

| Language | Calls | Imports | Inherit | Type-ref | Read/Write | Notes to add |
|----------|:-----:|:-------:|:-------:|:--------:|:----------:|---------------|
| Zig | ✓ | ✓ | — | ✓ | ✓ | no inheritance concept (like Go); `comptime`-generic Type-ref capped short of Exact |
| Objective-C | ✓ | ✓ | ✓ | ✓ | ✓ | `.h` stays dispatched to C (D-01, documented gap); categories emitted as distinct symbols, not merged; dynamic dispatch (`performSelector:`) unresolved by design |
| Fortran | ✓ | ✓ | ✓ (F90+ `extends`) | ✓ | ✓ | modern (F90+) modules get real `public`/`private` Visibility; legacy fixed-form `.f` caps at Unknown/no Read-Write on COMMON (not exercised in this phase's corpus per CONTEXT.md discretion) |
| Groovy | ✓ | ✓ | ✓ | ✓ | ✓ | `.gradle` parsed as plain Groovy, no DSL semantics (D-02); **`trait` keyword unsupported by tree-sitter-groovy 0.1.2 — not extracted, documented ceiling**; default unmarked visibility is Public (opposite of Java) |
| SystemVerilog | ✓ | ✓ | — | ✓ | — | `.sv`/`.svh`; no elaboration/parameterization semantics (deferred); Read/Write on module signals is out of table-stakes scope this phase (module-level dataflow, not simple var read/write — recommend leaving `—` unless the plan's discretion wants to add simple blocking-assignment LHS/RHS tracking, which the AST supports via `operator_assignment`'s `variable_lvalue`/`expression` fields) |

## Project Constraints (from CLAUDE.md)

- Repo-root `CLAUDE.md` (NodeDB-Lab container) applies: work in `nodedb-code2graph/` only; it is a standalone repo with its own `CLAUDE.md`-equivalent conventions (`CONTRIBUTING.md`), not GSD-managed via the container-level markers (no `.planning/` workflow note applies from the container doc — this project's own `.planning/` IS the GSD workflow for this repo).
- Conventional Commits required (`feat`, `fix`, `test`, `docs`, `chore`, scoped `extract`/`grammar`/`lang`/`symbol`/`graph`).
- No `.unwrap()`/`.expect()`/`panic!` in library code — `Result` + `?` + typed `CodegraphError` (test code may unwrap; all research example code above was deleted, none shipped).
- `mod.rs`/`lib.rs` are wiring-only (no logic) — applies to `src/extract/mod.rs` edits for these five languages.
- Grammars imported ONLY in `src/grammar.rs` — already done in Phase 1 for all five; extractors call `crate::grammar::<lang>()`.
- `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace` must be green.

## Sources

### Primary (HIGH confidence)
- Real `to_sexp()` and raw-child dumps against the exact pinned crate versions (`tree-sitter-zig` 1.1.2, `tree-sitter-objc` 3.0.2, `tree-sitter-fortran` 0.6.0, `tree-sitter-groovy` 0.1.2, `tree-sitter-systemverilog` 0.3.1), produced via a throwaway `examples/dump_ast.rs` + follow-up probe files, all deleted after use per CONTRIBUTING's own recommended workflow.
- `src/extract/c.rs`, `src/extract/swift.rs`, `src/extract/pascal.rs`, `src/extract/java.rs`, `src/extract/powershell.rs`, `src/extract/support.rs` — read in full for template/helper precedent.
- `src/symbol/descriptor.rs` — SCIP rendering/escaping rules (`push_ident`, `is_simple_ident_char`).
- `src/lang.rs`, `Cargo.toml`, `bindings/node/Cargo.toml`, `bindings/python/Cargo.toml` — naming precedent and wiring points.
- `docs/supported-languages.md`, `.planning/research/FEATURES.md`, `CONTRIBUTING.md` — capability targets, docs-matrix semantics, recipe.

### Secondary (MEDIUM confidence)
- `.planning/research/FEATURES.md`'s training-knowledge sections on Groovy/Fortran/ObjC semantics (explicitly flagged MEDIUM by its own author) — cross-verified against this research's real AST dumps where overlapping; agreement was high except FEATURES.md did not anticipate the `trait`-keyword grammar gap (a genuinely new finding from this research, not previously documented).

## Metadata

**Confidence breakdown:**
- AST node/field shapes: HIGH — all directly observed against pinned crate versions, not inferred from training data or upstream docs.
- Architecture/template mapping: HIGH — read directly from existing shipped extractor source.
- Groovy maturity ceiling (trait gap): HIGH — directly reproduced and isolated (three independent test snippets all confirm the same `juxt_function_call`+unrelated-`closure` shape).
- Fortran declare-before-execute requirement: HIGH — reproduced the ERROR, then reproduced the fix, confirming causation.

**Research date:** 2026-07-05
**Valid until:** effectively permanent for the pinned crate versions (grammar shapes don't change without a version bump); re-verify if any of the five `tree-sitter-*` versions in `Cargo.toml` are bumped before this phase executes.
