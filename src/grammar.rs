// SPDX-License-Identifier: Apache-2.0

//! Grammar chokepoint — the **sole** importer of every `tree_sitter_*` grammar crate.
//!
//! Each public function returns a [`tree_sitter::Language`] for the requested grammar
//! and is gated on the corresponding Cargo feature (`rust`, `python`, `typescript`, …).
//! All grammar crate imports live here; no extractor module may import a grammar crate
//! directly.

use tree_sitter::Language;

#[cfg(feature = "rust")]
/// Returns the tree-sitter grammar for Rust.
pub fn rust() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}

#[cfg(feature = "python")]
/// Returns the tree-sitter grammar for Python.
pub fn python() -> Language {
    tree_sitter_python::LANGUAGE.into()
}

#[cfg(feature = "typescript")]
/// Returns the tree-sitter grammar for TypeScript.
pub fn typescript() -> Language {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
}

#[cfg(feature = "typescript")]
/// Returns the tree-sitter grammar for TSX (TypeScript + JSX).
pub fn tsx() -> Language {
    tree_sitter_typescript::LANGUAGE_TSX.into()
}

#[cfg(feature = "go")]
/// Returns the tree-sitter grammar for Go.
pub fn go() -> Language {
    tree_sitter_go::LANGUAGE.into()
}

#[cfg(feature = "java")]
/// Returns the tree-sitter grammar for Java.
pub fn java() -> Language {
    tree_sitter_java::LANGUAGE.into()
}

#[cfg(feature = "c")]
/// Returns the tree-sitter grammar for C.
pub fn c() -> Language {
    tree_sitter_c::LANGUAGE.into()
}

#[cfg(feature = "cpp")]
/// Returns the tree-sitter grammar for C++.
pub fn cpp() -> Language {
    tree_sitter_cpp::LANGUAGE.into()
}

#[cfg(feature = "ruby")]
/// Returns the tree-sitter grammar for Ruby.
pub fn ruby() -> Language {
    tree_sitter_ruby::LANGUAGE.into()
}

#[cfg(feature = "php")]
/// Returns the tree-sitter grammar for PHP.
pub fn php() -> Language {
    tree_sitter_php::LANGUAGE_PHP.into()
}

#[cfg(feature = "shell")]
/// Returns the tree-sitter grammar for Bash/Shell (via `tree-sitter-bash`).
pub fn shell() -> Language {
    tree_sitter_bash::LANGUAGE.into()
}

#[cfg(feature = "swift")]
/// Returns the tree-sitter grammar for Swift.
pub fn swift() -> Language {
    tree_sitter_swift::LANGUAGE.into()
}

#[cfg(feature = "kotlin")]
/// Returns the tree-sitter grammar for Kotlin (via `tree-sitter-kotlin-ng`).
pub fn kotlin() -> Language {
    tree_sitter_kotlin_ng::LANGUAGE.into()
}

#[cfg(feature = "solidity")]
/// Returns the tree-sitter grammar for Solidity.
pub fn solidity() -> Language {
    tree_sitter_solidity::LANGUAGE.into()
}

#[cfg(feature = "sql")]
/// Returns the tree-sitter grammar for SQL (via `tree-sitter-sequel`).
pub fn sql() -> Language {
    tree_sitter_sequel::LANGUAGE.into()
}

#[cfg(feature = "hcl")]
/// Returns the tree-sitter grammar for HCL (HashiCorp Configuration Language).
pub fn hcl() -> Language {
    tree_sitter_hcl::LANGUAGE.into()
}

#[cfg(feature = "csharp")]
/// Returns the tree-sitter grammar for C#.
pub fn csharp() -> Language {
    tree_sitter_c_sharp::LANGUAGE.into()
}

#[cfg(feature = "scala")]
/// Returns the tree-sitter grammar for Scala.
pub fn scala() -> Language {
    tree_sitter_scala::LANGUAGE.into()
}

#[cfg(feature = "dart")]
/// Returns the tree-sitter grammar for Dart.
pub fn dart() -> Language {
    tree_sitter_dart::LANGUAGE.into()
}

#[cfg(feature = "lua")]
/// Returns the tree-sitter grammar for Lua.
pub fn lua() -> Language {
    tree_sitter_lua::LANGUAGE.into()
}

#[cfg(feature = "luau")]
/// Returns the tree-sitter grammar for Luau.
pub fn luau() -> Language {
    tree_sitter_luau::LANGUAGE.into()
}

#[cfg(feature = "pascal")]
/// Returns the tree-sitter grammar for Pascal / Delphi.
pub fn pascal() -> Language {
    tree_sitter_pascal::LANGUAGE.into()
}

#[cfg(feature = "svelte")]
/// Returns the tree-sitter grammar for Svelte single-file components.
pub fn svelte() -> Language {
    tree_sitter_svelte_ng::LANGUAGE.into()
}

#[cfg(feature = "zig")]
/// Returns the tree-sitter grammar for Zig.
pub fn zig() -> Language {
    tree_sitter_zig::LANGUAGE.into()
}

#[cfg(feature = "julia")]
/// Returns the tree-sitter grammar for Julia.
pub fn julia() -> Language {
    tree_sitter_julia::LANGUAGE.into()
}

#[cfg(feature = "r")]
/// Returns the tree-sitter grammar for R.
pub fn r() -> Language {
    tree_sitter_r::LANGUAGE.into()
}

#[cfg(feature = "ocaml")]
/// Returns the tree-sitter grammar for OCaml (`.ml` implementation files).
/// The crate also exposes `LANGUAGE_OCAML_INTERFACE` (.mli) and
/// `LANGUAGE_OCAML_TYPE`, not wired here — Phase 1 gates the base grammar
/// only; `.mli` handling is Phase 4's concern (LANG-04).
pub fn ocaml() -> Language {
    tree_sitter_ocaml::LANGUAGE_OCAML.into()
}

#[cfg(feature = "objc")]
/// Returns the tree-sitter grammar for Objective-C.
pub fn objc() -> Language {
    tree_sitter_objc::LANGUAGE.into()
}

#[cfg(feature = "fortran")]
/// Returns the tree-sitter grammar for Fortran.
pub fn fortran() -> Language {
    tree_sitter_fortran::LANGUAGE.into()
}

#[cfg(feature = "groovy")]
/// Returns the tree-sitter grammar for Groovy.
pub fn groovy() -> Language {
    tree_sitter_groovy::LANGUAGE.into()
}

#[cfg(feature = "powershell")]
/// Returns the tree-sitter grammar for PowerShell.
pub fn powershell() -> Language {
    tree_sitter_powershell::LANGUAGE.into()
}

#[cfg(feature = "systemverilog")]
/// Returns the tree-sitter grammar for SystemVerilog.
pub fn systemverilog() -> Language {
    tree_sitter_systemverilog::LANGUAGE.into()
}

#[cfg(feature = "astro")]
/// Returns the tree-sitter grammar for Astro single-file components
/// (via the independently-maintained `tree-sitter-astro-next`).
pub fn astro() -> Language {
    tree_sitter_astro_next::LANGUAGE.into()
}

#[cfg(feature = "fsharp")]
/// Returns the tree-sitter grammar for F# (ionide `tree-sitter-fsharp`).
pub fn fsharp() -> Language {
    tree_sitter_fsharp::LANGUAGE_FSHARP.into()
}

#[cfg(test)]
mod tests {
    use tree_sitter::{LANGUAGE_VERSION, MIN_COMPATIBLE_LANGUAGE_VERSION};

    fn check(name: &str, lang: tree_sitter::Language) {
        let v = lang.abi_version();
        assert!(
            (MIN_COMPATIBLE_LANGUAGE_VERSION..=LANGUAGE_VERSION).contains(&v),
            "grammar `{name}` ABI {v} outside [{MIN_COMPATIBLE_LANGUAGE_VERSION}, {LANGUAGE_VERSION}]"
        );
    }

    #[test]
    fn abi_versions_are_compatible() {
        #[cfg(feature = "rust")]
        check("rust", super::rust());
        #[cfg(feature = "python")]
        check("python", super::python());
        #[cfg(feature = "typescript")]
        check("typescript", super::typescript());
        #[cfg(feature = "typescript")]
        check("tsx", super::tsx());
        #[cfg(feature = "go")]
        check("go", super::go());
        #[cfg(feature = "java")]
        check("java", super::java());
        #[cfg(feature = "c")]
        check("c", super::c());
        #[cfg(feature = "cpp")]
        check("cpp", super::cpp());
        #[cfg(feature = "ruby")]
        check("ruby", super::ruby());
        #[cfg(feature = "php")]
        check("php", super::php());
        #[cfg(feature = "shell")]
        check("shell", super::shell());
        #[cfg(feature = "swift")]
        check("swift", super::swift());
        #[cfg(feature = "kotlin")]
        check("kotlin", super::kotlin());
        #[cfg(feature = "solidity")]
        check("solidity", super::solidity());
        #[cfg(feature = "sql")]
        check("sql", super::sql());
        #[cfg(feature = "hcl")]
        check("hcl", super::hcl());
        #[cfg(feature = "csharp")]
        check("csharp", super::csharp());
        #[cfg(feature = "scala")]
        check("scala", super::scala());
        #[cfg(feature = "dart")]
        check("dart", super::dart());
        #[cfg(feature = "lua")]
        check("lua", super::lua());
        #[cfg(feature = "luau")]
        check("luau", super::luau());
        #[cfg(feature = "pascal")]
        check("pascal", super::pascal());
        #[cfg(feature = "zig")]
        check("zig", super::zig());
        #[cfg(feature = "julia")]
        check("julia", super::julia());
        #[cfg(feature = "r")]
        check("r", super::r());
        #[cfg(feature = "ocaml")]
        check("ocaml", super::ocaml());
        #[cfg(feature = "objc")]
        check("objc", super::objc());
        #[cfg(feature = "fortran")]
        check("fortran", super::fortran());
        #[cfg(feature = "groovy")]
        check("groovy", super::groovy());
        #[cfg(feature = "powershell")]
        check("powershell", super::powershell());
        #[cfg(feature = "systemverilog")]
        check("systemverilog", super::systemverilog());
        #[cfg(feature = "astro")]
        check("astro", super::astro());
        #[cfg(feature = "fsharp")]
        check("fsharp", super::fsharp());
    }
}
