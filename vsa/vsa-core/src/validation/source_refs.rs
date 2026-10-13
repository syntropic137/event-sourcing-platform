//! What a source file refers to, read well enough for slice-shape rules.
//!
//! The slice rules ask "does this file reach the domain / a port / a
//! command?". A line-based import parser misses grouped imports
//! (`use crate::{domain::decide, ports::EventStorePort};`) and imported names
//! (`from ... import domain, ports`), and a plain text search counts comments,
//! strings and test-only code. This module strips those first:
//!
//! - Rust: comments, string and char literals, and `#[cfg(test)]` items are
//!   blanked; then every identifier in a `use` statement and the first
//!   segment after each `crate::` / `super::` / `self::` path is collected.
//! - Python: comments and triple-quoted strings are dropped; module segments
//!   and imported names of every `from … import …` / `import …` are collected.
//! - TypeScript/JavaScript: comments are dropped; the path segments of every
//!   `from '…'`, `import '…'` and `require('…')` are collected.

use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid regex"))
}

/// Identifiers and path segments a file refers to through imports or
/// (Rust) qualified paths. Empty for unreadable or unsupported files.
pub(crate) fn referenced_segments(path: &Path) -> HashSet<String> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return HashSet::new();
    };
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => rust_segments(&rust_code_only(&source)),
        Some("py") => python_segments(&source),
        Some("ts" | "tsx" | "js" | "jsx") => typescript_segments(&source),
        _ => HashSet::new(),
    }
}

fn identifiers(text: &str) -> impl Iterator<Item = String> + '_ {
    static IDENT: OnceLock<Regex> = OnceLock::new();
    re(&IDENT, r"[A-Za-z_][A-Za-z0-9_]*").find_iter(text).map(|m| m.as_str().to_string())
}

// ---------------------------------------------------------------- Rust

fn rust_segments(code: &str) -> HashSet<String> {
    static USE: OnceLock<Regex> = OnceLock::new();
    static PATH: OnceLock<Regex> = OnceLock::new();
    let mut segments = HashSet::new();
    for cap in re(&USE, r"\buse\s+([^;]+);").captures_iter(code) {
        segments.extend(identifiers(&cap[1]));
    }
    for cap in
        re(&PATH, r"\b(?:crate|super|self)(?:\s*::\s*super)*\s*::\s*([A-Za-z_][A-Za-z0-9_]*)")
            .captures_iter(code)
    {
        segments.insert(cap[1].to_string());
    }
    segments
}

/// Rust source with comments, string/char literals and `#[cfg(test)]` items
/// blanked (newlines kept).
pub(crate) fn rust_code_only(source: &str) -> String {
    remove_cfg_test_items(&blank_rust_comments_and_literals(source))
}

fn blank_rust_comments_and_literals(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let blank = |out: &mut String, c: char| out.push(if c == '\n' { '\n' } else { ' ' });
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                blank(&mut out, chars[i]);
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    blank(&mut out, chars[i]);
                    i += 1;
                }
            }
        } else if let Some(end) = raw_string_end(&chars, i) {
            while i < end {
                blank(&mut out, chars[i]);
                i += 1;
            }
        } else if c == '"' {
            blank(&mut out, c);
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' {
                    blank(&mut out, chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    blank(&mut out, chars[i]);
                    i += 1;
                }
            }
            if i < chars.len() {
                blank(&mut out, chars[i]);
                i += 1;
            }
        } else if c == '\'' {
            // A char literal ('x', '\n', '\u{1F600}'), or a lifetime ('a), kept as is.
            let close = if next == Some('\\') {
                chars[i + 2..].iter().position(|&c| c == '\'').map(|p| i + 2 + p)
            } else if chars.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            match close {
                Some(end) => {
                    while i <= end {
                        blank(&mut out, chars[i]);
                        i += 1;
                    }
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// End (exclusive) of a raw string `r"…"`, `r#"…"#`, `br"…"` starting at `i`.
fn raw_string_end(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i;
    if chars.get(j) == Some(&'b') {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    if i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
        return None;
    }
    j += 1;
    let hashes = chars[j..].iter().take_while(|&&c| c == '#').count();
    j += hashes;
    if chars.get(j) != Some(&'"') {
        return None;
    }
    j += 1;
    while j < chars.len() {
        if chars[j] == '"'
            && chars[j + 1..].iter().take(hashes).filter(|&&c| c == '#').count() == hashes
        {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    Some(chars.len())
}

/// Blank each item that follows `#[cfg(test)]`: up to its `;`, or through its
/// matching `{ … }` block.
fn remove_cfg_test_items(code: &str) -> String {
    static CFG_TEST: OnceLock<Regex> = OnceLock::new();
    let marker = re(&CFG_TEST, r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]");
    let mut out: Vec<char> = code.chars().collect();
    let byte_to_char: Vec<usize> = {
        let mut map = vec![0; code.len() + 1];
        for (ci, (bi, _)) in code.char_indices().enumerate() {
            map[bi] = ci;
        }
        map[code.len()] = out.len();
        map
    };
    for m in marker.find_iter(code) {
        let start = byte_to_char[m.start()];
        let mut i = byte_to_char[m.end()];
        let mut depth = 0usize;
        while i < out.len() {
            match out[i] {
                ';' if depth == 0 => {
                    i += 1;
                    break;
                }
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        let end = i.min(out.len());
        for c in &mut out[start..end] {
            if *c != '\n' {
                *c = ' ';
            }
        }
    }
    out.into_iter().collect()
}

// ---------------------------------------------------------------- Python

fn python_code_only(source: &str) -> String {
    static TRIPLE: OnceLock<Regex> = OnceLock::new();
    let without_docstrings = re(&TRIPLE, r#"(?s)""".*?"""|'''.*?'''"#).replace_all(source, "");
    without_docstrings
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn python_segments(source: &str) -> HashSet<String> {
    static FROM: OnceLock<Regex> = OnceLock::new();
    static IMPORT: OnceLock<Regex> = OnceLock::new();
    let code = python_code_only(source);
    let mut segments = HashSet::new();
    for cap in
        re(&FROM, r"(?m)^\s*from\s+([\w\.]+)\s+import\s+(\([^)]*\)|[^\n]+)").captures_iter(&code)
    {
        segments.extend(identifiers(&cap[1]));
        segments.extend(identifiers(&cap[2]));
    }
    for cap in re(&IMPORT, r"(?m)^\s*import\s+([^\n]+)").captures_iter(&code) {
        segments.extend(identifiers(&cap[1]));
    }
    segments
}

// ---------------------------------------------------------------- TypeScript

fn c_like_without_comments(source: &str) -> String {
    static COMMENTS: OnceLock<Regex> = OnceLock::new();
    re(&COMMENTS, r"(?s)/\*.*?\*/|//[^\n]*").replace_all(source, "").into_owned()
}

fn typescript_segments(source: &str) -> HashSet<String> {
    static SPEC: OnceLock<Regex> = OnceLock::new();
    let code = c_like_without_comments(source);
    let mut segments = HashSet::new();
    for cap in
        re(&SPEC, r#"(?:\bfrom|\bimport|\brequire\s*\()\s*['"]([^'"]+)['"]"#).captures_iter(&code)
    {
        segments
            .extend(cap[1].split(['/', '\\', '.']).filter(|s| !s.is_empty()).map(str::to_string));
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust(src: &str) -> HashSet<String> {
        rust_segments(&rust_code_only(src))
    }

    #[test]
    fn rust_grouped_and_multiline_imports() {
        let s = rust("use crate::{domain::decide, ports::EventStorePort};\n");
        assert!(s.contains("domain") && s.contains("ports"));
        let s = rust("use crate::domain::{\n    decide,\n    ItemCommand,\n};\n");
        assert!(s.contains("domain") && s.contains("ItemCommand"));
    }

    #[test]
    fn rust_inline_paths() {
        let s = rust("fn f(s: &mut dyn crate::ports::Store) { super::super::domain::decide(); }\n");
        assert!(s.contains("ports") && s.contains("domain"));
    }

    #[test]
    fn rust_comments_strings_and_tests_do_not_count() {
        let s = rust(
            "// use crate::domain::decide;\n\
             /* crate::ports::X */\n\
             fn f() { let x = \"crate::domain::decide crate::ports::EventStorePort\"; let c = '\"'; }\n\
             fn g<'a>(s: &'a str) -> &'a str { s }\n\
             #[cfg(test)]\n\
             mod tests {\n    use crate::domain::decide;\n    use crate::ports::Store;\n    fn h() { if true { } }\n}\n\
             #[cfg(test)]\nuse crate::ports::Other;\n",
        );
        assert!(!s.contains("domain"), "{s:?}");
        assert!(!s.contains("ports"), "{s:?}");
    }

    #[test]
    fn rust_raw_strings_are_blanked() {
        let s = rust("fn f() { let x = r#\"crate::domain::decide\"#; }\nuse crate::ports::P;\n");
        assert!(!s.contains("domain"));
        assert!(s.contains("ports"));
    }

    #[test]
    fn python_imports_and_imported_names() {
        let s = python_segments(
            "\"\"\"from crate import nothing\"\"\"\n# from x import ports\nfrom ... import domain, ports\nfrom ..domain.order import (\n    decide,\n)\n",
        );
        assert!(s.contains("domain") && s.contains("ports") && s.contains("decide"));
        assert!(!s.contains("nothing"));
    }

    #[test]
    fn typescript_module_paths() {
        let s = typescript_segments(
            "// import x from '../ports/Nope';\nimport { decide } from '../../domain/OrderAggregate';\nconst p = require('../ports/EventStorePort');\n",
        );
        assert!(s.contains("domain") && s.contains("ports"));
        assert!(!s.contains("Nope"));
    }
}
