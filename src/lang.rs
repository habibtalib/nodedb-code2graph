// SPDX-License-Identifier: Apache-2.0

//! The set of languages code2graph can parse, plus extension dispatch.
//!
//! This is the single place that enumerates language coverage; extraction and
//! resolution dispatch off it.

/// A source language code2graph knows how to parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    TypeScript, // .ts, .tsx
    JavaScript, // .js, .jsx, .mjs, .cjs
    Python,     // .py, .pyi
    Go,         // .go
    Shell,      // .sh, .bash, .zsh
    C,          // .c, .h
    Cpp,        // .cc, .cpp, .cxx, .hh, .hpp, .hxx
    Java,       // .java
    Ruby,       // .rb
    Php,        // .php
    Swift,      // .swift
    Kotlin,     // .kt, .kts
    Solidity,   // .sol
    Sql,        // .sql
    Hcl,        // .tf, .hcl, .tfvars
    CSharp,     // .cs
    Scala,      // .scala, .sc
    Dart,       // .dart
    Lua,        // .lua
    Luau,       // .luau
    Pascal,     // .pas, .dpr, .dpk, .lpr
    Svelte,     // .svelte
    PowerShell, // .ps1, .psm1
    Astro,      // .astro
    Zig,        // .zig
}

impl Language {
    /// Every `Language` variant. Kept exhaustive by the `all_is_exhaustive` test.
    pub const ALL: &[Language] = &[
        Language::Rust,
        Language::TypeScript,
        Language::JavaScript,
        Language::Python,
        Language::Go,
        Language::Shell,
        Language::C,
        Language::Cpp,
        Language::Java,
        Language::Ruby,
        Language::Php,
        Language::Swift,
        Language::Kotlin,
        Language::Solidity,
        Language::Sql,
        Language::Hcl,
        Language::CSharp,
        Language::Scala,
        Language::Dart,
        Language::Lua,
        Language::Luau,
        Language::Pascal,
        Language::Svelte,
        Language::PowerShell,
        Language::Astro,
        Language::Zig,
    ];

    /// This variant's file extensions (without the leading dot); the first entry
    /// is the primary/canonical extension. Single source of truth for
    /// `from_extension` — keep in sync with the extensions accepted there.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["rs"],
            Language::TypeScript => &["ts", "tsx"],
            Language::JavaScript => &["js", "jsx", "mjs", "cjs"],
            Language::Python => &["py", "pyi"],
            Language::Go => &["go"],
            Language::Shell => &["sh", "bash", "zsh"],
            Language::C => &["c", "h"],
            Language::Cpp => &["cc", "cpp", "cxx", "hh", "hpp", "hxx"],
            Language::Java => &["java"],
            Language::Ruby => &["rb"],
            Language::Php => &["php"],
            Language::Swift => &["swift"],
            Language::Kotlin => &["kt", "kts"],
            Language::Solidity => &["sol"],
            Language::Sql => &["sql"],
            Language::Hcl => &["tf", "hcl", "tfvars"],
            Language::CSharp => &["cs"],
            Language::Scala => &["scala", "sc"],
            Language::Dart => &["dart"],
            Language::Lua => &["lua"],
            Language::Luau => &["luau"],
            Language::Pascal => &["pas", "dpr", "dpk", "lpr"],
            Language::Svelte => &["svelte"],
            Language::PowerShell => &["ps1", "psm1"],
            Language::Astro => &["astro"],
            Language::Zig => &["zig"],
        }
    }

    /// Canonical lowercase language tag (stable; used in `SymbolKey.lang`).
    pub fn as_str(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::TypeScript => "typescript",
            Language::JavaScript => "javascript",
            Language::Python => "python",
            Language::Go => "go",
            Language::Shell => "shell",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::Java => "java",
            Language::Ruby => "ruby",
            Language::Php => "php",
            Language::Swift => "swift",
            Language::Kotlin => "kotlin",
            Language::Solidity => "solidity",
            Language::Sql => "sql",
            Language::Hcl => "hcl",
            Language::CSharp => "csharp",
            Language::Scala => "scala",
            Language::Dart => "dart",
            Language::Lua => "lua",
            Language::Luau => "luau",
            Language::Pascal => "pascal",
            Language::Svelte => "svelte",
            Language::PowerShell => "powershell",
            Language::Astro => "astro",
            Language::Zig => "zig",
        }
    }

    /// Map a lowercase file extension to a `Language`, or `None` if unknown.
    /// Derives from `extensions()` so the mapping has a single source of truth.
    pub fn from_extension(ext: &str) -> Option<Self> {
        Language::ALL
            .iter()
            .copied()
            .find(|l| l.extensions().contains(&ext))
    }

    /// Map a file path to a `Language` via its extension.
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = path.rsplit('.').next()?;
        Self::from_extension(&ext.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_dispatch() {
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::from_path("src/auth/mod.rs"), Some(Language::Rust));
        assert_eq!(Language::from_path("a/b/Main.KT"), Some(Language::Kotlin));
        assert_eq!(Language::from_extension("bin"), None);
    }

    #[test]
    fn tag_is_stable_lowercase() {
        assert_eq!(Language::TypeScript.as_str(), "typescript");
    }

    /// Compile-time guard: a new `Language` variant forces a new arm here (no
    /// wildcard), where the contributor is reminded to also add it to `Language::ALL`.
    fn assert_variant_in_all(l: Language) -> bool {
        let listed = match l {
            // EVERY variant must be listed here AND in `Language::ALL`.
            Language::Rust
            | Language::TypeScript
            | Language::JavaScript
            | Language::Python
            | Language::Go
            | Language::Shell
            | Language::C
            | Language::Cpp
            | Language::Java
            | Language::Ruby
            | Language::Php
            | Language::Swift
            | Language::Kotlin
            | Language::Solidity
            | Language::Sql
            | Language::Hcl
            | Language::CSharp
            | Language::Scala
            | Language::Dart
            | Language::Lua
            | Language::Luau
            | Language::Pascal
            | Language::Svelte
            | Language::PowerShell
            | Language::Astro
            | Language::Zig => true,
        };
        listed && Language::ALL.contains(&l)
    }

    #[test]
    fn all_is_exhaustive() {
        for &l in Language::ALL {
            assert!(
                assert_variant_in_all(l),
                "variant missing from Language::ALL"
            );
        }
    }

    #[test]
    fn supported_languages_doc_lists_each_primary_extension() {
        let path = format!("{}/docs/supported-languages.md", env!("CARGO_MANIFEST_DIR"));
        let doc =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
        for &lang in Language::ALL {
            let primary = lang.extensions()[0];
            let token = format!("`.{primary}`"); // doc cells are backticked: `.rs`
            assert!(
                doc.contains(&token),
                "language {lang:?} (primary ext .{primary}) is not listed in \
                 docs/supported-languages.md — add its row and update the doc",
            );
        }
    }
}
