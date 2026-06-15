// SPDX-License-Identifier: Apache-2.0

//! Loads golden evaluation cases from the on-disk corpus.
//!
//! Layout: `corpus/<lang>/<case>/` holds the case's source files (at any depth)
//! plus one `expected.edges` manifest at the case root. Each case resolves in
//! isolation, so cases never collide. Source files are stored under their path
//! relative to the case root (`/`-separated); for a flat case that path is just
//! the basename.
//!
//! `expected.edges` lists one ground-truth ref→def edge per line:
//!
//! ```text
//! # comments and blank lines are ignored
//! main.rs:5 Call util.rs:1
//! main.rs:6 TypeRef types.rs:3
//! ```
//!
//! Fields are `<ref_file>:<ref_line> <ROLE> <def_file>:<def_line>`, where `ROLE`
//! is a [`RefRole`] variant name (`Call`, `IsImplementation`, `Import`,
//! `TypeRef`, `Read`, `Write`).

use crate::score::ExpectedEdge;
use code2graph::RefRole;
use std::fs;
use std::io;
use std::path::Path;

/// One evaluation case: a self-contained set of source files plus the
/// ground-truth edges they should resolve to.
#[derive(Debug, Clone)]
pub struct Case {
    /// Language directory the case lives under (`rust`, `python`, `sql`, …).
    pub lang: String,
    /// Case directory name (used only for reporting).
    pub name: String,
    /// `(case_relative_path, source)` for every source file in the case, where
    /// the path uses `/` separators (e.g. `main.rs`, `alpha/alpha.go`). For a
    /// flat case the relative path equals the basename.
    pub files: Vec<(String, String)>,
    /// Ground-truth located ref→def edges (hand-authored, role-typed).
    pub expected: Vec<ExpectedEdge>,
    /// SCIP-oracle location pairs `(ref_file, ref_line, def_file, def_line)`.
    /// Non-empty only when the case dir contains `oracle.edges`.
    pub oracle: Vec<(String, u32, String, u32)>,
}

/// Load every case under `root` (the corpus directory), sorted by `lang` then
/// `name` for deterministic reporting.
pub fn load_corpus(root: &Path) -> io::Result<Vec<Case>> {
    let mut cases = Vec::new();
    for lang_entry in sorted_dirs(root)? {
        let lang = file_name(&lang_entry);
        for case_entry in sorted_dirs(&lang_entry)? {
            let name = file_name(&case_entry);
            cases.push(load_case(&lang, &name, &case_entry)?);
        }
    }
    Ok(cases)
}

fn load_case(lang: &str, name: &str, dir: &Path) -> io::Result<Case> {
    let mut files = Vec::new();
    let mut expected = Vec::new();
    let mut oracle = Vec::new();

    // Collect every file at any depth, sorted by full path for stable output.
    let mut all_files = Vec::new();
    collect_files(dir, &mut all_files)?;
    all_files.sort();

    for path in all_files {
        // The case-relative path with `/` separators (e.g. `alpha/alpha.go`,
        // `main.rs`). For a flat case this equals the basename.
        let Some(rel) = path
            .strip_prefix(dir)
            .ok()
            .and_then(|p| p.to_str())
            .map(|s| s.replace(std::path::MAIN_SEPARATOR, "/"))
        else {
            // Non-UTF-8 path — skip rather than panic.
            continue;
        };

        // Control files are recognised ONLY at the case root.
        let is_root = !rel.contains('/');
        if is_root && (rel == "index.scip" || rel == "oracle.edges" || rel == "expected.edges") {
            if rel == "expected.edges" {
                let text = fs::read_to_string(&path)?;
                expected = parse_expected(&text).map_err(|msg| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{}/{}/expected.edges: {msg}", lang, name),
                    )
                })?;
            } else if rel == "oracle.edges" {
                let text = fs::read_to_string(&path)?;
                oracle = parse_oracle(&text).map_err(|msg| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{}/{}/oracle.edges: {msg}", lang, name),
                    )
                })?;
            }
            continue;
        }

        let text = fs::read_to_string(&path)?;
        files.push((rel, text));
    }
    Ok(Case {
        lang: lang.to_string(),
        name: name.to_string(),
        files,
        expected,
        oracle,
    })
}

/// Recursively collect every regular file under `dir` into `out`.
fn collect_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// Parse an `oracle.edges` file into location-only pairs.
///
/// Format (one non-comment line per edge):
/// ```text
/// # oracle: scip-typescript — location pairs (ref -> def), role-agnostic
/// alpha.ts:1 main.ts:4
/// ```
fn parse_oracle(text: &str) -> Result<Vec<(String, u32, String, u32)>, String> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        out.push(
            parse_oracle_line(line)
                .ok_or_else(|| format!("line {}: bad oracle edge `{raw}`", i + 1))?,
        );
    }
    Ok(out)
}

/// Split one `ref_file:ref_line def_file:def_line` line into a location pair.
fn parse_oracle_line(line: &str) -> Option<(String, u32, String, u32)> {
    let mut parts = line.split_whitespace();
    let (ref_file, ref_line) = parse_loc(parts.next()?)?;
    let (def_file, def_line) = parse_loc(parts.next()?)?;
    if parts.next().is_some() {
        return None; // trailing garbage
    }
    Some((ref_file, ref_line, def_file, def_line))
}

/// Parse an `expected.edges` manifest into located edges.
fn parse_expected(text: &str) -> Result<Vec<ExpectedEdge>, String> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        out.push(parse_edge_line(line).ok_or_else(|| format!("line {}: bad edge `{raw}`", i + 1))?);
    }
    Ok(out)
}

fn parse_edge_line(line: &str) -> Option<ExpectedEdge> {
    let mut parts = line.split_whitespace();
    let (ref_file, ref_line) = parse_loc(parts.next()?)?;
    let role = parse_role(parts.next()?)?;
    let (def_file, def_line) = parse_loc(parts.next()?)?;
    if parts.next().is_some() {
        return None; // trailing garbage
    }
    Some(ExpectedEdge {
        ref_file,
        ref_line,
        role,
        def_file,
        def_line,
    })
}

/// Split a `file.ext:line` location.
fn parse_loc(s: &str) -> Option<(String, u32)> {
    let (file, line) = s.rsplit_once(':')?;
    if file.is_empty() {
        return None;
    }
    Some((file.to_string(), line.parse().ok()?))
}

fn parse_role(s: &str) -> Option<RefRole> {
    Some(match s {
        "Call" => RefRole::Call,
        "IsImplementation" => RefRole::IsImplementation,
        "Import" => RefRole::Import,
        "TypeRef" => RefRole::TypeRef,
        "Read" => RefRole::Read,
        "Write" => RefRole::Write,
        _ => return None,
    })
}

/// Subdirectories of `dir`, sorted by path.
fn sorted_dirs(dir: &Path) -> io::Result<Vec<std::path::PathBuf>> {
    let mut dirs: Vec<_> = fs::read_dir(dir)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    Ok(dirs)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_manifest() {
        let text = "# header\nmain.rs:5 Call util.rs:1\n\ntypes.rs:3 TypeRef types.rs:9  # trailing comment\n";
        let edges = parse_expected(text).unwrap();
        assert_eq!(edges.len(), 2);
        assert_eq!(
            edges[0],
            ExpectedEdge {
                ref_file: "main.rs".into(),
                ref_line: 5,
                role: RefRole::Call,
                def_file: "util.rs".into(),
                def_line: 1,
            }
        );
        assert_eq!(edges[1].role, RefRole::TypeRef);
    }

    #[test]
    fn rejects_unknown_role() {
        assert!(parse_expected("main.rs:1 Frobnicate util.rs:1").is_err());
    }

    #[test]
    fn rejects_missing_field() {
        assert!(parse_expected("main.rs:1 Call").is_err());
    }

    #[test]
    fn rejects_non_numeric_line() {
        assert!(parse_expected("main.rs:x Call util.rs:1").is_err());
    }

    #[test]
    fn load_case_stores_case_relative_paths() {
        use std::io::Write as _;

        let tmp = std::env::temp_dir().join(format!("c2g_corpus_{}", std::process::id()));
        let case = tmp.join("rust").join("nested");
        fs::create_dir_all(case.join("sub")).unwrap();

        // Flat file at root → basename.
        let mut f = fs::File::create(case.join("x.rs")).unwrap();
        f.write_all(b"// flat\n").unwrap();
        // Nested file → `sub/y.rs`.
        let mut f = fs::File::create(case.join("sub").join("y.rs")).unwrap();
        f.write_all(b"// nested\n").unwrap();
        // Control file at root only — parsed, not stored as a source file.
        let mut f = fs::File::create(case.join("expected.edges")).unwrap();
        f.write_all(b"x.rs:1 Call sub/y.rs:1\n").unwrap();

        let loaded = load_case("rust", "nested", &case).unwrap();
        let paths: Vec<&str> = loaded.files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, vec!["sub/y.rs", "x.rs"]);
        assert_eq!(loaded.expected.len(), 1);
        assert_eq!(loaded.expected[0].def_file, "sub/y.rs");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn handles_colon_in_filename_path() {
        // rsplit_once on ':' keeps any earlier colons in the file portion.
        let edges = parse_expected("a/b.rs:7 Call a/c.rs:2").unwrap();
        assert_eq!(edges[0].ref_file, "a/b.rs");
        assert_eq!(edges[0].ref_line, 7);
    }
}
