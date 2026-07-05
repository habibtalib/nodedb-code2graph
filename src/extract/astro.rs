// SPDX-License-Identifier: Apache-2.0

//! Astro single-file component extractor.
//!
//! Parses the `.astro` file with tree-sitter-astro-next, locates the frontmatter
//! fence (`---` … `---`, at most one per document) and every `<script>` block,
//! delegates each embedded block's inner source to the existing TypeScript /
//! JavaScript extraction core (`extract_ecmascript`), then remaps all byte
//! offsets back into the full `.astro` file via `shift_offsets`.
//!
//! The frontmatter block is always compiled as TypeScript — Astro compiles
//! frontmatter unconditionally as TS and there is no `lang` attribute to detect
//! on it (unlike `<script>` tags). `<script>` block discovery, `raw_text`
//! lookup, and `lang="ts"`/`"typescript"` detection are ported verbatim from
//! `svelte.rs` (confirmed identical node-kind literals by direct AST dump).
//!
//! The merge shape mirrors `svelte.rs`: one document-spanning root
//! [`ScopeKind::Module`] scope is pushed first (`doc_root`), then each embedded
//! block (frontmatter, if present, and every `<script>` block) is extracted,
//! offset-shifted, and re-parented under `doc_root`. Per-block
//! [`SymbolKind::Module`] symbols are filtered out during the merge; a single
//! Module symbol spanning the whole document is synthesized once, after the
//! loop, giving the component a stable SCIP identity regardless of how many
//! embedded blocks it contains — including zero (a pure-markup document with
//! no frontmatter and no `<script>` tag still emits exactly one Module symbol
//! and one root scope; frontmatter absence must never panic, see Pitfall 3 in
//! `02-RESEARCH.md`).

use tree_sitter::{Node, Parser};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{ByteSpan, FileFacts, ScopeId, ScopeKind, SymbolKind};
use crate::lang::Language;

use super::Extractor;
use super::module_symbol;
use super::push_scope;
use super::shift_offsets;
use super::typescript::{extract_ecmascript, module_namespaces};

/// Extracts facts from an Astro single-file component.
pub struct AstroExtractor;

impl Extractor for AstroExtractor {
    fn lang(&self) -> Language {
        Language::Astro
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_lang = crate::grammar::astro();
        let mut parser = Parser::new();
        parser
            .set_language(&ts_lang)
            .map_err(|_| CodegraphError::Parse {
                path: file.to_owned(),
            })?;

        let tree = parser
            .parse(source.as_bytes(), None)
            .ok_or_else(|| CodegraphError::Parse {
                path: file.to_owned(),
            })?;

        let root = tree.root_node();
        let bytes = source.as_bytes();

        // Collect all script_element nodes anywhere in the document.
        let mut script_nodes = Vec::new();
        collect_script_elements(&root, &mut script_nodes);

        let mut merged = FileFacts {
            file: file.to_owned(),
            lang: "astro".to_owned(),
            symbols: Vec::new(),
            references: Vec::new(),
            scopes: Vec::new(),
            bindings: Vec::new(),
            ffi_exports: Vec::new(),
        };

        // A single document-spanning root scope. Each embedded block's former
        // root scope is re-parented under this so the merged file has exactly
        // one root (`parent == None`) — mirroring svelte.rs and every other
        // language's one-module-per-file shape.
        let doc_root: ScopeId = push_scope(
            &mut merged.scopes,
            None,
            ByteSpan {
                start: 0,
                end: source.len(),
            },
            ScopeKind::Module,
        );

        // Frontmatter is always TypeScript — Astro compiles it unconditionally
        // as TS, and the frontmatter node carries no `start_tag`/attributes at
        // all (no `lang` to detect, unlike `<script>` tags below). At most one
        // frontmatter node exists per document, entirely absent when there's
        // no fenced `---`...`---` block (Pitfall 3 — never assume it's there).
        if let Some(frontmatter) = find_frontmatter(&root) {
            if let Some(js_block) = frontmatter_js_block(&frontmatter) {
                merge_block(
                    &mut merged,
                    doc_root,
                    js_block,
                    Language::TypeScript,
                    file,
                    bytes,
                )?;
            }
        }

        for script_el in script_nodes {
            // Find the raw_text child — skip empty script blocks.
            let raw_text = match find_raw_text(&script_el) {
                Some(n) => n,
                None => continue,
            };
            let inner_lang = detect_script_lang(&script_el, bytes);
            merge_block(&mut merged, doc_root, raw_text, inner_lang, file, bytes)?;
        }

        // Exactly one module symbol spanning the whole document — its SCIP id
        // is identical regardless of how many embedded blocks contributed.
        let namespaces = module_namespaces(file);
        merged.symbols.push(module_symbol(
            Language::Astro,
            &namespaces,
            file,
            source.len(),
        ));

        Ok(merged)
    }
}

/// Extract, offset-shift, and merge one embedded block (the frontmatter's
/// `frontmatter_js_block` or a `<script>`'s `raw_text`) whose inner source
/// spans `text_node`'s byte range, into `merged`. Mirrors `svelte.rs::extract`'s
/// per-block merge steps: `extract_ecmascript` → `shift_offsets` → shift scope
/// indices by `scope_base` → re-parent the block's former root scope under
/// `doc_root` → drop the block's own Module symbol (a single document Module
/// symbol is synthesized once, after every block has merged).
fn merge_block(
    merged: &mut FileFacts,
    doc_root: ScopeId,
    text_node: Node<'_>,
    inner_lang: Language,
    file: &str,
    bytes: &[u8],
) -> Result<()> {
    let delta = text_node.start_byte();
    let inner_source = std::str::from_utf8(&bytes[text_node.byte_range()]).unwrap_or_default();

    let mut block_facts = extract_ecmascript(inner_source, file, inner_lang)?;
    shift_offsets(&mut block_facts, delta, file, "astro", bytes);

    // Merge: fix up ScopeId indices before extending. `scope_base` is >= 1 for
    // every block (the doc root occupies index 0), so the shifts apply
    // uniformly.
    let scope_base: ScopeId = merged.scopes.len();
    for b in &mut block_facts.bindings {
        b.scope += scope_base;
    }
    for r in &mut block_facts.references {
        if let Some(s) = r.scope.as_mut() {
            *s += scope_base;
        }
    }
    for sc in &mut block_facts.scopes {
        if let Some(p) = sc.parent.as_mut() {
            *p += scope_base;
        }
    }
    // Re-parent the block's former root scope (local index 0, parent None,
    // untouched by the Some(p) shift above) under the document root.
    if let Some(first) = block_facts.scopes.first_mut() {
        first.parent = Some(doc_root);
    }

    // Drop the per-block module symbol — the single document module symbol is
    // synthesized once, after every block has merged.
    merged.symbols.extend(
        block_facts
            .symbols
            .into_iter()
            .filter(|s| s.kind != SymbolKind::Module),
    );
    merged.references.extend(block_facts.references);
    merged.scopes.extend(block_facts.scopes);
    merged.bindings.extend(block_facts.bindings);
    // ffi_exports: Astro frontmatter/scripts don't emit FFI exports.

    Ok(())
}

/// Direct child of `document` with kind `frontmatter`, if present. Astro
/// documents have AT MOST ONE frontmatter node, entirely absent when there's
/// no fenced frontmatter block (Pitfall 3 — treat as `Option`, never
/// `.unwrap()`).
fn find_frontmatter<'a>(root: &Node<'a>) -> Option<Node<'a>> {
    root.children(&mut root.walk())
        .find(|n| n.kind() == "frontmatter")
}

/// The single `frontmatter_js_block` raw-text child of a `frontmatter` node.
fn frontmatter_js_block<'a>(frontmatter: &Node<'a>) -> Option<Node<'a>> {
    frontmatter
        .children(&mut frontmatter.walk())
        .find(|n| n.kind() == "frontmatter_js_block")
}

/// Walk the tree recursively and collect every `script_element` node. Ported
/// verbatim from `svelte.rs` (confirmed identical node-kind literal).
fn collect_script_elements<'a>(node: &Node<'a>, out: &mut Vec<Node<'a>>) {
    if node.kind() == "script_element" {
        out.push(*node);
        return; // script_element children are its own internals, not nested scripts
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_script_elements(&child, out);
    }
}

/// Return the `raw_text` child of a `script_element`, if one exists. Ported
/// verbatim from `svelte.rs`.
fn find_raw_text<'a>(script_el: &Node<'a>) -> Option<Node<'a>> {
    let mut cursor = script_el.walk();
    script_el
        .children(&mut cursor)
        .find(|n| n.kind() == "raw_text")
}

/// Detect whether `<script lang="ts">` or `<script lang="typescript">` is
/// present on the script element. Falls back to [`Language::JavaScript`].
/// Ported verbatim from `svelte.rs` (confirmed identical attribute-node
/// shape by direct AST dump).
fn detect_script_lang(script_el: &Node<'_>, bytes: &[u8]) -> Language {
    let mut cursor = script_el.walk();
    for child in script_el.children(&mut cursor) {
        if child.kind() == "start_tag" {
            let mut tag_cursor = child.walk();
            for attr in child.children(&mut tag_cursor) {
                if attr.kind() != "attribute" {
                    continue;
                }
                let name_matches = {
                    let mut c = attr.walk();
                    attr.children(&mut c)
                        .any(|n| n.kind() == "attribute_name" && &bytes[n.byte_range()] == b"lang")
                };
                if !name_matches {
                    continue;
                }
                let mut attr_cursor = attr.walk();
                for child2 in attr.children(&mut attr_cursor) {
                    if child2.kind() == "quoted_attribute_value" {
                        let mut qav_cursor = child2.walk();
                        for av in child2.children(&mut qav_cursor) {
                            if av.kind() == "attribute_value" {
                                let val = &bytes[av.byte_range()];
                                if val == b"ts" || val == b"typescript" {
                                    return Language::TypeScript;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Language::JavaScript
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::SymbolKind;

    fn astro_source_with_frontmatter_and_template() -> &'static str {
        r#"---
function helper() {}
function run() { helper(); }
---
<div>{run()}</div>"#
    }

    #[test]
    fn extracts_helper_and_run_symbols_lang_is_astro() {
        let source = astro_source_with_frontmatter_and_template();
        let facts = AstroExtractor
            .extract(source, "src/Component.astro")
            .expect("extraction should succeed");

        assert_eq!(facts.lang, "astro");
        assert_eq!(facts.file, "src/Component.astro");

        let helper_sym = facts
            .symbols
            .iter()
            .find(|s| s.name == "helper" && s.kind == SymbolKind::Function);
        assert!(
            helper_sym.is_some(),
            "expected `helper` function symbol; got: {:?}",
            facts.symbols
        );
        let run_sym = facts
            .symbols
            .iter()
            .find(|s| s.name == "run" && s.kind == SymbolKind::Function);
        assert!(
            run_sym.is_some(),
            "expected `run` function symbol; got: {:?}",
            facts.symbols
        );

        assert!(
            !facts.references.is_empty(),
            "expected at least one reference"
        );
    }

    #[test]
    fn offset_remap_is_correct() {
        let source = astro_source_with_frontmatter_and_template();
        let facts = AstroExtractor
            .extract(source, "src/Component.astro")
            .expect("extraction should succeed");

        let run_sym = facts
            .symbols
            .iter()
            .find(|s| s.name == "run" && s.kind == SymbolKind::Function)
            .expect("`run` symbol must be present");

        let expected_start = source
            .find("function run")
            .expect("`function run` must appear in source");
        assert_eq!(
            run_sym.span.start, expected_start,
            "span.start should be the byte offset of the `run` declaration in the full .astro source"
        );
        assert!(
            source[run_sym.span.start..run_sym.span.end].contains("run"),
            "remapped span must slice the run declaration out of the .astro source"
        );
    }

    #[test]
    fn script_lang_ts_is_detected_no_frontmatter() {
        let source = r#"<script lang="ts">
export function greet(name: string) { return name; }
</script>
<p>Hi</p>"#;
        let facts = AstroExtractor
            .extract(source, "src/Greet.astro")
            .expect("extraction should succeed");

        assert_eq!(facts.lang, "astro", "lang should always be 'astro'");
        let greet = facts.symbols.iter().find(|s| s.name == "greet");
        assert!(
            greet.is_some(),
            "expected `greet` symbol from lang=\"ts\" script; got: {:?}",
            facts.symbols
        );
    }

    #[test]
    fn no_frontmatter_no_script_emits_single_module_symbol_and_root_scope() {
        let source = r#"<main><p>Hello</p></main>"#;
        let facts = AstroExtractor
            .extract(source, "src/NoScript.astro")
            .expect("extraction should succeed even with no frontmatter and no script");

        assert_eq!(facts.lang, "astro");
        assert_eq!(facts.file, "src/NoScript.astro");

        assert_eq!(facts.symbols.len(), 1, "expected exactly one symbol");
        assert_eq!(facts.symbols[0].kind, SymbolKind::Module);
        assert_eq!(facts.symbols[0].span.start, 0);
        assert_eq!(facts.symbols[0].span.end, source.len());
        assert!(facts.references.is_empty(), "expected no references");

        assert_eq!(facts.scopes.len(), 1, "expected exactly one (root) scope");
        assert_eq!(facts.scopes[0].parent, None);
        assert_eq!(facts.scopes[0].span.start, 0);
        assert_eq!(facts.scopes[0].span.end, source.len());
    }

    #[test]
    fn frontmatter_and_script_both_contribute_one_module_symbol_one_root_scope() {
        let source = r#"---
function preload() {}
---
<script>
function setup() {}
</script>
<div>content</div>"#;
        let facts = AstroExtractor
            .extract(source, "src/Page.astro")
            .expect("extraction should succeed");

        let has_preload = facts.symbols.iter().any(|s| s.name == "preload");
        let has_setup = facts.symbols.iter().any(|s| s.name == "setup");
        assert!(has_preload, "expected `preload` from frontmatter block");
        assert!(has_setup, "expected `setup` from script block");

        let module_syms: Vec<_> = facts
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module)
            .collect();
        assert_eq!(
            module_syms.len(),
            1,
            "expected exactly one Module symbol, got {module_syms:?}"
        );
        assert_eq!(module_syms[0].span.start, 0, "module span must start at 0");
        assert_eq!(
            module_syms[0].span.end,
            source.len(),
            "module span must cover the whole document"
        );

        let root_scopes: Vec<_> = facts.scopes.iter().filter(|s| s.parent.is_none()).collect();
        assert_eq!(
            root_scopes.len(),
            1,
            "expected exactly one root scope, got {root_scopes:?}"
        );
        assert_eq!(root_scopes[0].span.start, 0, "root scope must start at 0");
        assert_eq!(
            root_scopes[0].span.end,
            source.len(),
            "root scope must cover the whole document"
        );

        for b in &facts.bindings {
            assert!(
                b.scope < facts.scopes.len() || facts.scopes.is_empty(),
                "binding scope {} out of range (scopes.len={})",
                b.scope,
                facts.scopes.len()
            );
        }
        for r in &facts.references {
            if let Some(s) = r.scope {
                assert!(
                    s < facts.scopes.len(),
                    "reference scope {} out of range (scopes.len={})",
                    s,
                    facts.scopes.len()
                );
            }
        }
    }
}
