//! What a source file refers to, read well enough for slice-shape rules.
//!
//! The slice rules ask "does this file reach the domain / a port / a
//! command?". A line-based import parser misses grouped imports
//! (`use crate::{domain::decide, ports::EventStorePort};`) and imported names
//! (`from ... import domain, ports`), and a plain text search counts comments,
//! strings and test-only code. This module strips those first:
//!
//! - Rust: comments, string and char literals, and test-only items
//!   (`#[cfg(test)]`, `#[cfg(all(test, …))]`, `#[test]`, `#[tokio::test]`) are
//!   blanked. Then every path segment rooted at `crate` / `super` / `self` is
//!   collected, from `use` trees and from qualified paths in code. Names bound
//!   with `as` and paths into other crates do not count.
//! - Python: comments and string literals are blanked; module segments and
//!   imported names of every `from … import …` / `import …` are collected.
//! - TypeScript/JavaScript: comments and template literals are blanked; the
//!   path segments of every `import … from '…'`, `export … from '…'`,
//!   `import '…'` and `require('…')` are collected.

use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid regex"))
}

/// Path segments and names a file refers to through imports or (Rust)
/// qualified paths. Empty for unreadable or unsupported files.
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

/// Identifiers of a `use` tree or path, without names introduced by `as`.
fn path_identifiers(text: &str) -> Vec<String> {
    static ALIAS: OnceLock<Regex> = OnceLock::new();
    let without_aliases = re(&ALIAS, r"\bas\s+[A-Za-z_][A-Za-z0-9_]*").replace_all(text, "");
    identifiers(&without_aliases).collect()
}

// ---------------------------------------------------------------- Rust

fn rust_segments(code: &str) -> HashSet<String> {
    static USE: OnceLock<Regex> = OnceLock::new();
    static PATH: OnceLock<Regex> = OnceLock::new();
    let mut segments = HashSet::new();
    for cap in re(&USE, r"\buse\s+([^;]+);").captures_iter(code) {
        let tree = cap[1].trim().trim_start_matches("::");
        let ids = path_identifiers(tree);
        if matches!(ids.first().map(String::as_str), Some("crate" | "super" | "self")) {
            segments.extend(ids);
        }
    }
    let path_re = re(&PATH, r"\b(?:crate|super|self)(?:\s*::\s*[A-Za-z_][A-Za-z0-9_]*)+");
    for m in path_re.find_iter(code) {
        segments.extend(identifiers(m.as_str()));
    }
    segments
}

/// Rust source with comments, string/char literals and test-only items
/// blanked (newlines kept).
pub(crate) fn rust_code_only(source: &str) -> String {
    remove_test_items(&blank_rust_comments_and_literals(source))
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

/// Is this attribute body test-only? `test`, `tokio::test`,
/// `tokio::test(flavor = "multi_thread")`, `cfg(test)` and
/// `cfg(all(test, ...))` are. `cfg(any(test, ...))` and `cfg(not(test))` are
/// not: they also compile outside tests.
fn is_test_attribute(body: &str) -> bool {
    static TEST_WORD: OnceLock<Regex> = OnceLock::new();
    let body: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some(cfg) = body.strip_prefix("cfg(").and_then(|b| b.strip_suffix(')')) {
        return cfg == "test"
            || (cfg.starts_with("all(")
                && !cfg.contains("not(")
                && !cfg.contains("any(")
                && re(&TEST_WORD, r"\btest\b").is_match(cfg));
    }
    let name = body.split('(').next().unwrap_or("");
    name == "test" || name.ends_with("::test")
}

/// Blank each item under a test-only attribute: up to its `;`, or through its
/// matching `{ … }` block. The text is rescanned after each item, so a marker
/// inside an already blanked item is never processed.
fn remove_test_items(code: &str) -> String {
    static ATTR: OnceLock<Regex> = OnceLock::new();
    let attr = re(&ATTR, r"#\s*\[([^\]]*)\]");
    let mut out: Vec<char> = code.chars().collect();
    let mut from = 0;
    loop {
        let text: String = out.iter().collect();
        let Some((start, mut i)) = attr
            .captures_iter(&text)
            .filter(|c| c.get(0).is_some_and(|m| m.start() >= from))
            .find(|c| is_test_attribute(&c[1]))
            .map(|c| {
                let m = c.get(0).expect("whole match");
                (text[..m.start()].chars().count(), text[..m.end()].chars().count())
            })
        else {
            break;
        };
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
        from = text.char_indices().nth(end).map_or(text.len(), |(b, _)| b);
    }
    out.into_iter().collect()
}

// ---------------------------------------------------------------- Python

/// Python source with comments and string literals blanked.
pub(crate) fn python_code_only(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let blank = |out: &mut String, c: char| out.push(if c == '\n' { '\n' } else { ' ' });
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                blank(&mut out, chars[i]);
                i += 1;
            }
        } else if c == '"' || c == '\'' {
            let triple = chars.get(i + 1) == Some(&c) && chars.get(i + 2) == Some(&c);
            let quote_len = if triple { 3 } else { 1 };
            for _ in 0..quote_len {
                blank(&mut out, chars[i]);
                i += 1;
            }
            while i < chars.len() {
                if chars[i] == '\\' {
                    blank(&mut out, chars[i]);
                    i += 1;
                    if i < chars.len() {
                        blank(&mut out, chars[i]);
                        i += 1;
                    }
                    continue;
                }
                let closes = if triple {
                    chars[i] == c && chars.get(i + 1) == Some(&c) && chars.get(i + 2) == Some(&c)
                } else {
                    chars[i] == c || chars[i] == '\n'
                };
                if closes {
                    for _ in 0..quote_len.min(chars.len() - i) {
                        blank(&mut out, chars[i]);
                        i += 1;
                    }
                    break;
                }
                blank(&mut out, chars[i]);
                i += 1;
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn python_segments(source: &str) -> HashSet<String> {
    static FROM: OnceLock<Regex> = OnceLock::new();
    static IMPORT: OnceLock<Regex> = OnceLock::new();
    let code = python_code_only(source);
    let mut segments = HashSet::new();
    let from_re = re(&FROM, r"(?m)^\s*from\s+([\w\.]+)\s+import\s+(\([^)]*\)|[^\n]+)");
    for cap in from_re.captures_iter(&code) {
        segments.extend(identifiers(&cap[1]));
        segments.extend(path_identifiers(&cap[2]));
    }
    for cap in re(&IMPORT, r"(?m)^\s*import\s+([^\n]+)").captures_iter(&code) {
        segments.extend(path_identifiers(&cap[1]));
    }
    segments
}

// ---------------------------------------------------------------- TypeScript

/// JavaScript/TypeScript source with comments blanked. With `keep_strings`,
/// '…' and "…" literals are kept (imports live in them) and template
/// literals are blanked; without it, every literal is blanked.
fn js_blank(source: &str, keep_strings: bool) -> String {
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
            blank(&mut out, chars[i]);
            blank(&mut out, chars[i + 1]);
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                blank(&mut out, chars[i]);
                i += 1;
            }
            for _ in 0..2.min(chars.len() - i) {
                blank(&mut out, chars[i]);
                i += 1;
            }
        } else if c == '"' || c == '\'' || c == '`' {
            let keep = keep_strings && c != '`';
            let push =
                |out: &mut String, ch: char| if keep { out.push(ch) } else { blank(out, ch) };
            push(&mut out, c);
            i += 1;
            while i < chars.len() && chars[i] != c {
                if chars[i] == '\\' {
                    push(&mut out, chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    push(&mut out, chars[i]);
                    i += 1;
                }
            }
            if i < chars.len() {
                push(&mut out, chars[i]);
                i += 1;
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn typescript_segments(source: &str) -> HashSet<String> {
    static SPEC: OnceLock<Regex> = OnceLock::new();
    let code = js_blank(source, true);
    let spec = re(
        &SPEC,
        r#"(?m)(?:(?:^|;)\s*(?:import|export)\b[^;'"]*?\bfrom\s*|(?:^|;)\s*import\s*|\brequire\s*\(\s*|\bimport\s*\(\s*)['"]([^'"]+)['"]"#,
    );
    let mut segments = HashSet::new();
    for cap in spec.captures_iter(&code) {
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
    fn rust_qualified_paths_keep_every_segment() {
        let s = rust(
            "fn f(s: &mut dyn crate::orders::ports::Store) { crate::orders::domain::decide(); }\n",
        );
        assert!(s.contains("ports") && s.contains("domain"));
        let s = rust("fn f() { super::super::domain::decide(); }\n");
        assert!(s.contains("domain"));
    }

    #[test]
    fn rust_aliases_and_other_crates_do_not_count() {
        let s = rust("use std::{fmt as domain, io as ports};\nuse crate::infra as ports2;\n");
        assert!(!s.contains("domain") && !s.contains("ports"), "{s:?}");
    }

    #[test]
    fn rust_comments_strings_and_tests_do_not_count() {
        let s = rust(
            "// use crate::domain::decide;\n\
             /* crate::ports::X */\n\
             fn f() { let x = \"crate::domain::decide crate::ports::EventStorePort\"; let c = '\"'; }\n\
             fn g<'a>(s: &'a str) -> &'a str { s }\n\
             #[cfg(test)]\n\
             mod tests {\n    use crate::domain::decide;\n    #[cfg(test)]\n    use crate::ports::Inner;\n    fn h() { if true { } }\n}\n\
             #[cfg(test)]\nuse crate::ports::Other;\n\
             #[cfg(all(test, feature = \"x\"))]\nmod more { use crate::domain::x; }\n\
             #[test]\nfn t() { crate::domain::decide(); }\n\
             #[tokio::test]\nasync fn t2() { crate::ports::Store::new(); }\n\
             #[tokio::test(flavor = \"multi_thread\")]\nasync fn t3() { crate::domain::x(); }\n",
        );
        assert!(!s.contains("domain"), "{s:?}");
        assert!(!s.contains("ports"), "{s:?}");
    }

    #[test]
    fn rust_code_after_a_test_module_still_counts() {
        let s = rust(
            "#[cfg(test)]\nmod tests {\n    #[cfg(test)]\n    mod inner { }\n    fn h() {}\n}\n\
             use crate::domain::decide;\n#[cfg(not(test))]\nuse crate::ports::Store;\n\
             #[cfg(any(test, feature = \"prod\"))]\nuse crate::application::Svc;\n",
        );
        assert!(s.contains("domain") && s.contains("ports") && s.contains("application"), "{s:?}");
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
            "\"\"\"from crate import nothing\"\"\"\n# from x import ports\nfrom ... import domain, ports\nfrom ..domain.order import (\n    decide,\n)\nx = 'from y import strings'\n",
        );
        assert!(s.contains("domain") && s.contains("ports") && s.contains("decide"));
        assert!(!s.contains("nothing") && !s.contains("strings"));
        let s = python_segments("from os import path as domain\n");
        assert!(!s.contains("domain"));
    }

    #[test]
    fn typescript_module_paths() {
        let s = typescript_segments(
            "// import x from '../ports/Nope';\nimport { decide } from '../../domain/OrderAggregate';\nconst p = require('../ports/EventStorePort');\n",
        );
        assert!(s.contains("domain") && s.contains("ports"));
        assert!(!s.contains("Nope"));
    }

    #[test]
    fn typescript_literals_do_not_import() {
        let s = typescript_segments(
            "const doc = `import x from \"../domain/order\"; require(\"../ports/store\")`;\nconst u = \"https://example.com\"; import { a } from './real';\n",
        );
        assert!(!s.contains("domain") && !s.contains("ports"), "{s:?}");
        assert!(s.contains("real"), "code after a URL string is still read: {s:?}");
    }
}
