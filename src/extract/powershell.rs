// SPDX-License-Identifier: Apache-2.0

//! PowerShell extractor — one tree-sitter pass yielding definitions and references.
//!
//! Definitions: `function`/`filter` statements (both parse to `function_statement`
//! — the `filter` keyword doesn't change shape) and PS5+ `class` statements
//! (with their nested methods and, when present, a base-class reference).
//! References: calls in both cmdlet-style (`Verb-Noun -Arg x`) and
//! member/expression-style (`$obj.Method()`, `[Type]::Static()`) forms; all
//! three import forms (`Import-Module`, `using module`, dot-sourcing); variable
//! read/write.
//!
//! `Visibility` is always [`Visibility::Unknown`] — the language has no
//! in-source public/private signal (`Export-ModuleMember` is a runtime
//! convention, not syntax, and is deliberately not used to infer visibility).
//! Dynamic invocation (`Invoke-Expression`, `&$scriptBlock`) is an unresolved
//! ceiling — never guessed at. Names are emitted as written; PowerShell's
//! case-insensitivity is not normalized at the extraction layer (matches every
//! other extractor's as-written behavior — see 02-RESEARCH.md Open Question 1).
//!
//! Emits neutral [`FileFacts`] — no storage entries, no source bodies.

use std::collections::HashSet;

use tree_sitter::{Language as TsLanguage, Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::error::{CodegraphError, Result};
use crate::graph::types::{
    ByteSpan, FileFacts, RefRole, Reference, Scope, ScopeKind, Symbol, SymbolKind, Visibility,
};
use crate::lang::Language;
use crate::symbol::Descriptor;

use super::{
    ExtractCtx, Extractor, attach_reference_scopes, collect_call_references, definition_bindings,
    import_bindings, make_symbol, node_text, one_line_signature, push_import_ref, push_ref,
    push_scope,
};

/// Cmdlet-style call query — parenless, space-separated commands
/// (`Get-Process -Name notepad`). Verified against the pinned grammar in
/// 02-RESEARCH.md "Pattern 3".
const CMDLET_CALL_QUERY: &str = r#"(command command_name: (command_name) @callee)"#;

/// Member/expression-style call query — `.NET` interop and PS-object method
/// calls (`$obj.Method()`, `[System.IO.File]::ReadAllText($path)`). The
/// receiver (a `variable` or `type_literal`) is captured as `@qualifier`.
const MEMBER_CALL_QUERY: &str = r#"(invokation_expression [(variable) (type_literal)] @qualifier (member_name (simple_name) @callee))"#;

/// `Import-Module Foo` / `using module Foo` — both are generic `command`
/// nodes; classify by `command_name` text (see 02-RESEARCH.md "Pattern 4").
const IMPORT_ARG_QUERY: &str = r#"(command command_name: (command_name) @cmd command_elements: (command_elements (generic_token) @arg))"#;

/// Dot-sourcing (`. .\lib\helpers.ps1`) — distinguished by the presence of a
/// `command_invokation_operator` sibling and a wrapped `command_name_expr`.
const DOT_SOURCE_QUERY: &str = r#"(command (command_invokation_operator) command_name: (command_name_expr (command_name) @path))"#;

/// Extracts PowerShell symbols and references.
pub struct PowerShellExtractor;

impl Extractor for PowerShellExtractor {
    fn lang(&self) -> Language {
        Language::PowerShell
    }

    fn extract(&self, source: &str, file: &str) -> Result<FileFacts> {
        let ts_language = crate::grammar::powershell();
        let mut parser = Parser::new();
        parser
            .set_language(&ts_language)
            .map_err(|_| CodegraphError::Parse {
                path: file.to_owned(),
            })?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| CodegraphError::Parse {
                path: file.to_owned(),
            })?;

        let root = tree.root_node();
        let bytes = source.as_bytes();
        let namespaces = powershell_namespaces(file);
        let ctx = ExtractCtx {
            bytes,
            file,
            lang: Language::PowerShell,
        };

        // Stub for RED: no symbols/references/imports collected yet.
        let mod_sym = super::module_symbol(Language::PowerShell, &namespaces, file, source.len());
        let symbols = vec![mod_sym];
        let references: Vec<Reference> = Vec::new();

        let mut scopes = Vec::new();
        push_scope(
            &mut scopes,
            None,
            ByteSpan {
                start: 0,
                end: source.len(),
            },
            ScopeKind::Module,
        );

        let mut references = references;
        attach_reference_scopes(&mut references, &scopes);

        Ok(FileFacts {
            file: file.to_owned(),
            lang: Language::PowerShell.as_str().to_owned(),
            symbols,
            references,
            scopes,
            bindings: Vec::new(),
            ffi_exports: Vec::new(),
        })
    }
}

/// Derive the PowerShell namespace path from a file path.
///
/// Strips a `.ps1`/`.psm1` extension; strips a leading `src/`, `bin/`, or
/// `scripts/` prefix (each tried in order); then splits on `/`.
fn powershell_namespaces(file: &str) -> Vec<String> {
    let p = file.strip_suffix(".ps1").or_else(|| file.strip_suffix(".psm1")).unwrap_or(file);
    let p = p
        .strip_prefix("src/")
        .or_else(|| p.strip_prefix("bin/"))
        .or_else(|| p.strip_prefix("scripts/"))
        .unwrap_or(p);
    p.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::RefRole;

    #[test]
    fn extracts_function_statement() {
        let src = "function Get-Helper { return $true }\n";
        let facts = PowerShellExtractor.extract(src, "scripts/deploy.ps1").unwrap();
        let helper = facts
            .symbols
            .iter()
            .find(|s| s.name == "Get-Helper")
            .expect("expected a Function symbol named 'Get-Helper'");
        assert_eq!(helper.kind, SymbolKind::Function);
        assert_eq!(helper.visibility, Visibility::Unknown);
        assert_eq!(
            helper.id.to_scip_string(),
            "codegraph . . . deploy/Get-Helper()."
        );
    }

    #[test]
    fn extracts_filter_statement() {
        let src = "filter Get-Bar { $_ }\n";
        let facts = PowerShellExtractor.extract(src, "scripts/deploy.ps1").unwrap();
        assert!(facts.symbols.iter().any(|s| s.name == "Get-Bar"));
    }

    #[test]
    fn extracts_class_and_method() {
        let src = "class Animal { [string]$Name Speak() { return \"hi\" } }\n";
        let facts = PowerShellExtractor.extract(src, "scripts/models.ps1").unwrap();
        let animal = facts
            .symbols
            .iter()
            .find(|s| s.name == "Animal")
            .expect("expected a Class symbol 'Animal'");
        assert_eq!(animal.kind, SymbolKind::Class);
        assert_eq!(animal.id.to_scip_string(), "codegraph . . . models/Animal#");

        let speak = facts
            .symbols
            .iter()
            .find(|s| s.name == "Speak")
            .expect("expected a Method symbol 'Speak'");
        assert_eq!(speak.kind, SymbolKind::Method);
        assert_eq!(
            speak.id.to_scip_string(),
            "codegraph . . . models/Animal#Speak()."
        );
    }

    #[test]
    fn class_inheritance_emits_is_implementation() {
        let src = "class Dog : Animal { Speak() { } }\n";
        let facts = PowerShellExtractor.extract(src, "scripts/models.ps1").unwrap();
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::IsImplementation && r.name == "Animal"),
            "expected an IsImplementation reference to 'Animal'"
        );
    }

    #[test]
    fn cmdlet_style_calls_both_pipeline_stages() {
        let src = "Get-Process -Name notepad | Stop-Process\n";
        let facts = PowerShellExtractor.extract(src, "scripts/run.ps1").unwrap();
        let names: Vec<&str> = facts
            .references
            .iter()
            .filter(|r| r.role == RefRole::Call)
            .map(|r| r.name.as_str())
            .collect();
        assert!(names.contains(&"Get-Process"));
        assert!(names.contains(&"Stop-Process"));
    }

    #[test]
    fn member_style_calls_capture_qualifier() {
        let src = "$obj.Method()\n[System.IO.File]::ReadAllText($path)\n";
        let facts = PowerShellExtractor.extract(src, "scripts/run.ps1").unwrap();

        let method_call = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "Method")
            .expect("expected a Call reference 'Method'");
        assert_eq!(method_call.qualifier.as_deref(), Some("$obj"));

        let static_call = facts
            .references
            .iter()
            .find(|r| r.role == RefRole::Call && r.name == "ReadAllText")
            .expect("expected a Call reference 'ReadAllText'");
        assert_eq!(
            static_call.qualifier.as_deref(),
            Some("[System.IO.File]")
        );
    }

    #[test]
    fn import_module_is_import_not_call() {
        let src = "Import-Module MyModule\n";
        let facts = PowerShellExtractor.extract(src, "scripts/run.ps1").unwrap();
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Import && r.name == "MyModule")
        );
        assert!(
            !facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Call && r.name.eq_ignore_ascii_case("Import-Module")),
            "Import-Module must not also emit a spurious Call reference"
        );
    }

    #[test]
    fn using_module_is_import_not_call() {
        let src = "using module MyModule\n";
        let facts = PowerShellExtractor.extract(src, "scripts/run.ps1").unwrap();
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Import && r.name == "MyModule")
        );
        assert!(
            !facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Call && r.name.eq_ignore_ascii_case("using")),
            "using module must not also emit a spurious Call reference"
        );
    }

    #[test]
    fn dot_sourcing_is_import_not_call() {
        let src = ". .\\lib\\helpers.ps1\n";
        let facts = PowerShellExtractor.extract(src, "scripts/run.ps1").unwrap();
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.role == RefRole::Import && r.name == ".\\lib\\helpers.ps1")
        );
        assert!(
            !facts.references.iter().any(|r| r.role == RefRole::Call),
            "dot-sourcing must not also emit a spurious Call reference"
        );
    }
}
