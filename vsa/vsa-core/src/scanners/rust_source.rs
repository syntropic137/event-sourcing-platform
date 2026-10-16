//! A light reading of Rust domain code for the manifest.
//!
//! A Rust domain usually models its commands and its events as one enum each
//! (`enum OrderCommand { Place { .. }, Cancel }`) and decides in a `match` over
//! the command enum, often handing each arm to a helper function that builds
//! the events. This module reads, from Rust source with comments, literals and
//! test-only items blanked:
//!
//! - the enums a file declares and their variants (name, fields, line);
//! - for each `CommandEnum::Variant =>` arm, the `EventEnum::Variant`s the arm
//!   builds, directly or through functions it calls (followed transitively,
//!   among the functions given).
//!
//! It is a reading, not a parser: macros, trait dispatch and events built
//! outside the given files are not followed.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

use regex::Regex;

pub(crate) use crate::validation::source_refs::rust_code_only;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid regex"))
}

/// One enum variant: its name, its fields (`name`, `type`; tuple fields are
/// named by position) and its 1-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RustVariant {
    pub name: String,
    pub fields: Vec<(String, String)>,
    pub line: usize,
}

/// An enum and its variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RustEnum {
    pub name: String,
    pub variants: Vec<RustVariant>,
}

/// Index of the matching close bracket for the open bracket at `open`.
fn matching_close(chars: &[char], open: usize) -> Option<usize> {
    let (o, c) = match chars[open] {
        '{' => ('{', '}'),
        '(' => ('(', ')'),
        '[' => ('[', ']'),
        _ => return None,
    };
    let mut depth = 0usize;
    for (i, &ch) in chars.iter().enumerate().skip(open) {
        if ch == o {
            depth += 1;
        } else if ch == c {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Split `text` at top-level commas (outside (), {}, [] and <>).
fn split_top_level(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '(' | '{' | '[' | '<' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            // `->` in a fn type is not a closing angle bracket.
            '>' if i == 0 || chars[i - 1] != '-' => depth -= 1,
            _ => {}
        }
        if ch == ',' && depth == 0 {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    parts.push(current);
    parts.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

/// Drop leading `#[...]` attributes and a visibility from an item fragment.
fn strip_attributes(text: &str) -> &str {
    let mut rest = text.trim_start();
    while rest.starts_with("#[") {
        let chars: Vec<char> = rest.chars().collect();
        match matching_close(&chars, 1) {
            Some(end) => {
                let byte_end: usize = chars[..=end].iter().map(|c| c.len_utf8()).sum();
                rest = rest[byte_end..].trim_start();
            }
            None => break,
        }
    }
    rest.strip_prefix("pub ").map(str::trim_start).unwrap_or(rest)
}

fn line_of(code: &str, byte: usize) -> usize {
    code[..byte].matches('\n').count() + 1
}

fn parse_fields(body: &str, named: bool) -> Vec<(String, String)> {
    split_top_level(body)
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let field = strip_attributes(field);
            if named {
                match field.split_once(':') {
                    Some((name, ty)) => (name.trim().to_string(), ty.trim().to_string()),
                    None => (field.to_string(), String::new()),
                }
            } else {
                (i.to_string(), field.to_string())
            }
        })
        .collect()
}

/// The enums declared in `code` (already passed through [`rust_code_only`]).
pub(crate) fn enums(code: &str) -> Vec<RustEnum> {
    static ENUM: OnceLock<Regex> = OnceLock::new();
    let chars: Vec<char> = code.chars().collect();
    // Char index for each byte offset the regex reports.
    let char_at = |byte: usize| code[..byte].chars().count();
    let mut found = Vec::new();
    for m in re(&ENUM, r"\benum\s+([A-Za-z_][A-Za-z0-9_]*)").captures_iter(code) {
        let name = m[1].to_string();
        let after = m.get(0).map(|g| g.end()).unwrap_or(0);
        let Some(open_rel) = code[after..].find('{') else { continue };
        let open = char_at(after + open_rel);
        let Some(close) = matching_close(&chars, open) else { continue };
        let body: String = chars[open + 1..close].iter().collect();
        let body_byte = after + open_rel + 1;
        let mut variants = Vec::new();
        let mut offset = 0;
        for part in split_top_level(&body) {
            let start = body[offset..].find(part.as_str()).map(|p| p + offset).unwrap_or(offset);
            offset = start + part.len();
            let item = strip_attributes(&part);
            let ident_end =
                item.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(item.len());
            let vname = &item[..ident_end];
            if vname.is_empty() {
                continue;
            }
            let rest = item[ident_end..].trim_start();
            // The variant's own `{ .. }` / `( .. )`, by its matching close, so a
            // nested `((u64, u64))` keeps its inner tuple.
            let inner = |rest: &str| {
                let rc: Vec<char> = rest.chars().collect();
                matching_close(&rc, 0).map(|c| rc[1..c].iter().collect::<String>())
            };
            let fields = match rest.chars().next() {
                Some('{') => inner(rest).map(|b| parse_fields(&b, true)).unwrap_or_default(),
                Some('(') => inner(rest).map(|b| parse_fields(&b, false)).unwrap_or_default(),
                _ => Vec::new(),
            };
            variants.push(RustVariant {
                name: vname.to_string(),
                fields,
                line: line_of(code, body_byte + start),
            });
        }
        found.push(RustEnum { name, variants });
    }
    found
}

/// The variants of every enum in `source` whose name ends with `suffix`
/// (`"Command"`, `"Event"`), each with its qualified name `Enum::Variant`.
/// Empty when the file declares no such enum.
pub(crate) fn rust_enum_variants(source: &str, suffix: &str) -> Vec<(String, RustVariant)> {
    enums(&rust_code_only(source))
        .into_iter()
        .filter(|e| e.name.ends_with(suffix))
        .flat_map(|e| {
            let name = e.name;
            e.variants.into_iter().map(move |v| (format!("{name}::{}", v.name), v))
        })
        .collect()
}

/// Whether a field type is `Option<_>`, however written (`Option <T>`,
/// `std::option::Option<T>`, `core::option::Option<T>`).
pub(crate) fn is_optional_type(field_type: &str) -> bool {
    let compact: String = field_type.chars().filter(|c| !c.is_whitespace()).collect();
    let path = compact.trim_start_matches("::");
    let path = path
        .strip_prefix("std::option::")
        .or_else(|| path.strip_prefix("core::option::"))
        .unwrap_or(path);
    path.starts_with("Option<")
}

/// Function name -> bodies (several when the name repeats), from code already
/// passed through [`rust_code_only`].
pub(crate) fn functions(code: &str) -> Vec<(String, String, usize)> {
    static FN: OnceLock<Regex> = OnceLock::new();
    let chars: Vec<char> = code.chars().collect();
    let char_at = |byte: usize| code[..byte].chars().count();
    let mut found = Vec::new();
    for m in re(&FN, r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)").captures_iter(code) {
        let after = m.get(0).map(|g| g.end()).unwrap_or(0);
        // The body starts at the first `{` outside the signature's brackets;
        // a `;` first means a declaration without a body.
        let mut depth = 0i32;
        let mut open = None;
        for (i, ch) in code[after..].char_indices() {
            match ch {
                '(' | '[' | '<' => depth += 1,
                ')' | ']' => depth -= 1,
                '>' if !code[after..after + i].ends_with('-') => depth -= 1,
                '{' if depth <= 0 => {
                    open = Some(after + i);
                    break;
                }
                ';' if depth <= 0 => break,
                _ => {}
            }
        }
        let Some(open_byte) = open else { continue };
        let open = char_at(open_byte);
        let Some(close) = matching_close(&chars, open) else { continue };
        let body: String = chars[open + 1..close].iter().collect();
        found.push((m[1].to_string(), body, line_of(code, open_byte)));
    }
    found
}

/// One decision arm: `CommandEnum::Variant` handled in function `function` at
/// `line`, and the `EventEnum::Variant`s it builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecisionArm {
    pub command: String,
    pub function: String,
    pub line: usize,
    pub events: Vec<String>,
}

/// A Rust source file for [`decision_arms`]: its module name (the file stem,
/// or the folder name for `mod.rs`) and its code, already passed through
/// [`rust_code_only`].
#[derive(Debug, Clone)]
pub(crate) struct SourceFile {
    pub module: String,
    pub code: String,
}

/// Helper functions by name: `(file index, body)` for each definition. The
/// index identifies the file (two files may share a module name, such as
/// `aggregate_a/decide.rs` and `aggregate_b/decide.rs`).
type Helpers = HashMap<String, Vec<(usize, String)>>;

/// Every `Command::Variant =>` match arm in `arm_files`, with the events each
/// builds, directly or through the functions it calls. A call resolves to a
/// function of the same name in the module it names (`orders::create()`), else
/// in the caller's own file, else to the only function of that name among
/// `arm_files` and `helper_files`; an ambiguous call is not followed.
/// `commands` and `events` map an enum name to its variant names; only those
/// count.
pub(crate) fn decision_arms(
    arm_files: &[SourceFile],
    helper_files: &[SourceFile],
    commands: &BTreeMap<String, BTreeSet<String>>,
    events: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<DecisionArm> {
    let files: Vec<&SourceFile> = arm_files.iter().chain(helper_files).collect();
    let mut helpers: Helpers = HashMap::new();
    for (index, file) in files.iter().enumerate() {
        for (name, body, _) in functions(&file.code) {
            helpers.entry(name).or_default().push((index, body));
        }
    }
    let ctx = Lookup { events, helpers: &helpers, files: &files };

    let mut arms = Vec::new();
    for (index, file) in arm_files.iter().enumerate() {
        for (function, body, _) in functions(&file.code) {
            let fn_start = file.code.find(&body).unwrap_or(0);
            for (command, start, (from, to)) in arm_patterns(&body, commands) {
                let mut seen = BTreeSet::new();
                seen.insert((index, function.clone()));
                let mut found = BTreeSet::new();
                ctx.collect(&body[from..to], index, &mut seen, &mut found);
                arms.push(DecisionArm {
                    command,
                    function: function.clone(),
                    line: line_of(&file.code, fn_start + start),
                    events: found.into_iter().collect(),
                });
            }
        }
    }
    arms
}

/// Char index just past the end of the arm body starting at char `start`:
/// a `{ .. }` block, else an expression running to the next comma, or to the
/// `}` that closes the `match`, at depth 0.
fn arm_body_end(chars: &[char], start: usize) -> usize {
    let mut i = start;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    let mut depth = 0i32;
    // Generic arguments opened by a turbofish (`Ok::<_, E>(..)`): their commas
    // do not end the arm.
    let mut generics = 0i32;
    while i < chars.len() {
        match chars[i] {
            ':' if chars.get(i + 1) == Some(&':') && chars.get(i + 2) == Some(&'<') => {
                generics += 1;
                i += 3;
                continue;
            }
            '<' if generics > 0 => generics += 1,
            '>' if generics > 0 && (i == 0 || chars[i - 1] != '-') => generics -= 1,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    return i;
                }
                depth -= 1;
                // A block-like body (`if .. { } else { }`, `match .. { }`)
                // needs no comma: once its last block closes, what follows is
                // the next arm, unless the expression goes on (`else`, `.x()`,
                // `?`, an operator).
                if depth == 0 && chars[i] == '}' {
                    let mut j = i + 1;
                    while j < chars.len() && chars[j].is_whitespace() {
                        j += 1;
                    }
                    let next: String = chars[j..chars.len().min(j + 5)].iter().collect();
                    let continues = next.starts_with("else")
                        && !next[4..].starts_with(|c: char| c.is_alphanumeric() || c == '_');
                    let joins = chars.get(j).is_some_and(|c| ".?+-*/%&|^<>=!,;)]}".contains(*c));
                    if !continues && (!joins || starts_arm(chars, j)) {
                        return i + 1;
                    }
                }
            }
            ',' if depth == 0 && generics == 0 => return i,
            _ => {}
        }
        i += 1;
    }
    chars.len()
}

/// Whether the text at `j` is a match-arm pattern: it reaches `=>` at depth 0
/// before a `,` or `;` at depth 0 (`&Cmd::X(_) =>`, `| Cmd::Y =>`).
fn starts_arm(chars: &[char], j: usize) -> bool {
    let mut depth = 0i32;
    let mut k = j;
    while k < chars.len() {
        match chars[k] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            '=' if depth == 0 && chars.get(k + 1) == Some(&'>') => return true,
            ',' | ';' if depth == 0 => return false,
            _ => {}
        }
        k += 1;
    }
    false
}

/// `(Enum::Variant, pattern_start, (body_start, body_end))` (byte offsets in
/// `body`) for every match arm whose pattern names a known command, in order.
/// `A | B =>` gives one entry per variant, sharing one body.
fn arm_patterns(
    body: &str,
    commands: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<(String, usize, (usize, usize))> {
    static PATH: OnceLock<Regex> = OnceLock::new();
    let chars: Vec<char> = body.chars().collect();
    let byte_of = |ci: usize| chars[..ci].iter().map(|c| c.len_utf8()).sum::<usize>();
    let char_at = |byte: usize| body[..byte].chars().count();
    let mut out = Vec::new();
    let mut pending: Vec<(String, usize)> = Vec::new();
    let mut resume = 0usize;
    for m in re(&PATH, r"\b([A-Z][A-Za-z0-9_]*)::([A-Z][A-Za-z0-9_]*)\b").captures_iter(body) {
        let whole = m.get(0).expect("match");
        if whole.start() < resume {
            continue;
        }
        let (enum_name, variant) = (&m[1], &m[2]);
        if !commands.get(enum_name).is_some_and(|v| v.contains(variant)) {
            continue;
        }
        // Skip a `{ .. }` / `( .. )` binding, then expect `=>`, `|` or `if`.
        let mut i = char_at(whole.end());
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i < chars.len() && (chars[i] == '{' || chars[i] == '(') {
            match matching_close(&chars, i) {
                Some(close) => i = close + 1,
                None => continue,
            }
        }
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
        let name = format!("{enum_name}::{variant}");
        if rest.starts_with('|') {
            pending.push((name, whole.start()));
            resume = byte_of(i + 1);
        } else if rest == "=>" || rest.starts_with("if") {
            // A guard runs to the arrow.
            let Some(arrow) = body[byte_of(i)..].find("=>").map(|p| p + byte_of(i)) else {
                continue;
            };
            let body_start = arrow + 2;
            let body_end = byte_of(arm_body_end(&chars, char_at(body_start)));
            for (pending_name, start) in pending.drain(..) {
                out.push((pending_name, start, (body_start, body_end)));
            }
            out.push((name, whole.start(), (body_start, body_end)));
            resume = body_end;
        } else {
            // A use of the command (a constructor, a call argument), not an arm.
            pending.clear();
        }
    }
    out
}

/// What an arm can reach: the known events and the helper functions.
struct Lookup<'a> {
    events: &'a BTreeMap<String, BTreeSet<String>>,
    helpers: &'a Helpers,
    files: &'a [&'a SourceFile],
}

impl Lookup<'_> {
    fn collect(
        &self,
        code: &str,
        caller: usize,
        seen: &mut BTreeSet<(usize, String)>,
        found: &mut BTreeSet<String>,
    ) {
        static PATH: OnceLock<Regex> = OnceLock::new();
        static CALL: OnceLock<Regex> = OnceLock::new();
        let path = re(&PATH, r"\b([A-Z][A-Za-z0-9_]*)::([A-Z][A-Za-z0-9_]*)\b");
        for m in path.captures_iter(code) {
            if self.events.get(&m[1]).is_some_and(|v| v.contains(&m[2])) {
                found.insert(format!("{}::{}", &m[1], &m[2]));
            }
        }
        let call = re(
            &CALL,
            r"(\.)?\b(?:([a-z_][A-Za-z0-9_]*)::)?([a-z_][A-Za-z0-9_]*)\s*(?:::\s*<[^()]*?>\s*)?\(",
        );
        for m in call.captures_iter(code) {
            if m.get(1).is_some() {
                continue; // a method call
            }
            let qualifier =
                m.get(2).map(|q| q.as_str()).filter(|q| !matches!(*q, "self" | "super" | "crate"));
            let name = &m[3];
            let Some((target, body)) = self.resolve(name, qualifier, caller) else {
                continue;
            };
            if !seen.insert((target, name.to_string())) {
                continue;
            }
            self.collect(&body, target, seen, found);
        }
    }

    fn resolve(
        &self,
        name: &str,
        qualifier: Option<&str>,
        caller: usize,
    ) -> Option<(usize, String)> {
        let defs = self.helpers.get(name)?;
        let only =
            |matching: Vec<&(usize, String)>| (matching.len() == 1).then(|| matching[0].clone());
        let in_module = |module: &str| {
            only(defs.iter().filter(|(i, _)| self.files[*i].module == module).collect())
        };
        let in_caller = || only(defs.iter().filter(|(i, _)| *i == caller).collect());
        // The only definition of that name, when nothing narrower matched (a
        // module imported under another name: `use x as containment`).
        let unique = || (defs.len() == 1).then(|| defs[0].clone());
        match qualifier {
            Some(module) => in_module(module).or_else(unique),
            None => in_caller().or_else(unique),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMANDS: &str = r#"
/// What a caller can ask an order to do.
#[derive(Debug, Clone)]
pub enum OrderCommand {
    /// Place it.
    Place { id: u64, lines: Vec<(String, u32)> },
    Cancel(Reason),
    #[serde(rename = "x")]
    Archive,
}
"#;

    const EVENTS: &str = r"
pub enum OrderEvent {
    Placed { id: u64 },
    Cancelled,
    Archived,
    Noted(String),
}
";

    const DECIDE: &str = r#"
pub fn decide(state: &State, cmd: OrderCommand) -> Result<Vec<OrderEvent>, Error> {
    match cmd {
        OrderCommand::Place { id, lines } if !lines.is_empty() => decide_place(id),
        OrderCommand::Cancel(reason) => {
            let _ = "OrderEvent::Archived"; // a literal, not an event
            Ok(vec![OrderEvent::Cancelled, note(reason)])
        }
        OrderCommand::Archive => Ok(vec![]),
    }
}

fn decide_place(id: u64) -> Result<Vec<OrderEvent>, Error> {
    Ok(vec![OrderEvent::Placed { id }])
}

fn note(r: Reason) -> OrderEvent { OrderEvent::Noted(r.0) }

#[cfg(test)]
mod tests {
    fn t() { let _ = decide(&s, OrderCommand::Archive); let _ = OrderEvent::Archived; }
}
"#;

    fn variants(code: &str, name: &str) -> BTreeSet<String> {
        enums(&rust_code_only(code))
            .into_iter()
            .find(|e| e.name == name)
            .map(|e| e.variants.into_iter().map(|v| v.name).collect())
            .unwrap_or_default()
    }
    fn file(module: &str, code: &str) -> SourceFile {
        SourceFile { module: module.to_string(), code: rust_code_only(code) }
    }

    fn maps() -> (BTreeMap<String, BTreeSet<String>>, BTreeMap<String, BTreeSet<String>>) {
        (
            BTreeMap::from([("OrderCommand".to_string(), variants(COMMANDS, "OrderCommand"))]),
            BTreeMap::from([("OrderEvent".to_string(), variants(EVENTS, "OrderEvent"))]),
        )
    }

    fn by_command(arms: &[DecisionArm]) -> BTreeMap<&str, Vec<&str>> {
        arms.iter()
            .map(|a| (a.command.as_str(), a.events.iter().map(String::as_str).collect()))
            .collect()
    }

    #[test]
    fn reads_variants_with_their_fields() {
        let found = enums(&rust_code_only(COMMANDS));
        assert_eq!(found.len(), 1);
        let e = &found[0];
        assert_eq!(e.name, "OrderCommand");
        let names: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["Place", "Cancel", "Archive"]);
        assert_eq!(
            e.variants[0].fields,
            vec![
                ("id".to_string(), "u64".to_string()),
                ("lines".to_string(), "Vec<(String, u32)>".to_string())
            ]
        );
        assert_eq!(e.variants[1].fields, vec![("0".to_string(), "Reason".to_string())]);
        assert!(e.variants[2].fields.is_empty());
        assert_eq!(e.variants[0].line, 6);
    }

    #[test]
    fn optional_types_however_written() {
        for ty in [
            "Option<String>",
            "Option <String>",
            "std::option::Option<u8>",
            "::core::option::Option<T>",
        ] {
            assert!(is_optional_type(ty), "{ty}");
        }
        for ty in ["String", "Vec<Option<u8>>", "MyOption<u8>"] {
            assert!(!is_optional_type(ty), "{ty}");
        }
    }

    #[test]
    fn nested_tuple_fields_keep_their_brackets() {
        let found = enums(&rust_code_only("enum E { Pair((u64, u64)), Map { m: Vec<(A, B)> } }"));
        assert_eq!(found[0].variants[0].fields, vec![("0".to_string(), "(u64, u64)".to_string())]);
        assert_eq!(found[0].variants[1].fields, vec![("m".to_string(), "Vec<(A, B)>".to_string())]);
    }

    #[test]
    fn maps_each_arm_to_the_events_it_builds_through_helpers() {
        let (commands, events) = maps();
        let arms = decision_arms(&[file("order", DECIDE)], &[], &commands, &events);
        let by = by_command(&arms);
        assert_eq!(by.len(), 3, "{arms:?}");
        assert_eq!(by["OrderCommand::Place"], ["OrderEvent::Placed"]);
        assert_eq!(by["OrderCommand::Cancel"], ["OrderEvent::Cancelled", "OrderEvent::Noted"]);
        assert!(by["OrderCommand::Archive"].is_empty(), "test code and literals do not count");
        assert!(arms.iter().all(|a| a.function == "decide"));
    }

    #[test]
    fn or_patterns_share_an_arm_and_constructors_are_not_arms() {
        let code = "fn decide(c: OrderCommand) -> Vec<OrderEvent> { match c {\n\
               OrderCommand::Cancel(_) | OrderCommand::Archive => vec![OrderEvent::Archived],\n\
               OrderCommand::Place { .. } => vec![],\n\
             } }\n\
             fn build() -> OrderCommand { OrderCommand::Archive }\n";
        let (commands, events) = maps();
        let arms = decision_arms(&[file("order", code)], &[], &commands, &events);
        assert_eq!(arms.len(), 3, "{arms:?}");
        assert_eq!(arms[0].command, "OrderCommand::Cancel");
        assert_eq!(arms[1].command, "OrderCommand::Archive");
        assert_eq!(arms[0].events, ["OrderEvent::Archived"]);
        assert_eq!(arms[1].events, ["OrderEvent::Archived"]);
        assert!(arms[2].events.is_empty());
    }

    #[test]
    fn an_arm_ends_at_its_own_body_not_at_the_next_known_command() {
        let code = "fn decide(c: OrderCommand) -> Vec<OrderEvent> {\n\
               let events = match c {\n\
                   OrderCommand::Archive => vec![],\n\
                   _ => vec![OrderEvent::Archived],\n\
               };\n\
               let _after = vec![OrderEvent::Cancelled];\n\
               match c { OrderCommand::Cancel(_) => { match x { _ => vec![OrderEvent::Cancelled] } } _ => vec![] }\n\
               events\n\
             }\n";
        let (commands, events) = maps();
        let arms = decision_arms(&[file("order", code)], &[], &commands, &events);
        let by = by_command(&arms);
        assert!(
            by["OrderCommand::Archive"].is_empty(),
            "the wildcard arm and trailing code are not Archive's: {arms:?}"
        );
        assert_eq!(
            by["OrderCommand::Cancel"],
            ["OrderEvent::Cancelled"],
            "a nested match stays in its arm"
        );
    }

    #[test]
    fn a_block_like_arm_body_needs_no_comma() {
        let code = "fn decide(c: OrderCommand, ok: bool) -> Vec<OrderEvent> { match c {\n\
               OrderCommand::Place { .. } => if ok { vec![OrderEvent::Placed { id: 1 }] } else { vec![] }\n\
               OrderCommand::Cancel(_) => match ok { true => vec![OrderEvent::Cancelled], false => vec![] }\n\
               OrderCommand::Archive => { vec![] }.into_iter().chain(archive()).collect(),\n\
             } }\n\
             fn archive() -> Vec<OrderEvent> { vec![OrderEvent::Archived] }\n";
        let (commands, events) = maps();
        let arms = decision_arms(&[file("order", code)], &[], &commands, &events);
        let by = by_command(&arms);
        assert_eq!(by.len(), 3, "{arms:?}");
        assert_eq!(by["OrderCommand::Place"], ["OrderEvent::Placed"]);
        assert_eq!(by["OrderCommand::Cancel"], ["OrderEvent::Cancelled"]);
        assert_eq!(by["OrderCommand::Archive"], ["OrderEvent::Archived"]);
    }

    #[test]
    fn turbofish_commas_and_reference_patterns_do_not_split_arms() {
        let code = "fn decide(c: &OrderCommand) -> Result<Vec<OrderEvent>, Error> { match c {\n\
               &OrderCommand::Archive => { Ok(vec![]) }\n\
               &OrderCommand::Cancel(_) => Ok::<_, Error>(vec![OrderEvent::Cancelled]),\n\
               &OrderCommand::Place { .. } => place::<Error>(),\n\
             } }\n\
             fn place<E>() -> Result<Vec<OrderEvent>, E> { Ok(vec![OrderEvent::Placed { id: 1 }]) }\n";
        let (commands, events) = maps();
        let arms = decision_arms(&[file("order", code)], &[], &commands, &events);
        let by = by_command(&arms);
        assert_eq!(by.len(), 3, "{arms:?}");
        assert!(by["OrderCommand::Archive"].is_empty(), "{arms:?}");
        assert_eq!(by["OrderCommand::Cancel"], ["OrderEvent::Cancelled"]);
        assert_eq!(by["OrderCommand::Place"], ["OrderEvent::Placed"]);
    }

    #[test]
    fn same_named_files_in_different_folders_keep_their_own_helpers() {
        // `aggregate_order/decide.rs` and `aggregate_customer/decide.rs`: one module
        // name, two files; a local call resolves to the caller's own file.
        let order = file(
            "decide",
            "fn decide(c: OrderCommand) -> Vec<OrderEvent> { match c { OrderCommand::Place { .. } => create(), } }\n\
             fn create() -> Vec<OrderEvent> { vec![OrderEvent::Placed { id: 1 }] }\n",
        );
        let customer =
            file("decide", "fn create() -> Vec<OrderEvent> { vec![OrderEvent::Noted(x)] }\n");
        let (commands, events) = maps();
        let arms = decision_arms(&[order], &[customer], &commands, &events);
        assert_eq!(arms[0].events, ["OrderEvent::Placed"], "{arms:?}");
    }

    #[test]
    fn calls_resolve_by_module_then_own_file_then_a_unique_name() {
        let decide = "fn decide(c: OrderCommand) -> Vec<OrderEvent> { match c {\n\
               OrderCommand::Place { .. } => placing::create(),\n\
               OrderCommand::Cancel(_) => create(),\n\
               OrderCommand::Archive => alias::archive(),\n\
             } }\n\
             fn create() -> Vec<OrderEvent> { vec![OrderEvent::Cancelled] }\n";
        let placing = file(
            "placing",
            "pub fn create() -> Vec<OrderEvent> { vec![OrderEvent::Placed { id: 1 }] }\n",
        );
        let payments =
            file("payments", "pub fn create() -> Vec<OrderEvent> { vec![OrderEvent::Noted(x)] }\n");
        let archiving = file(
            "archiving",
            "pub fn archive() -> Vec<OrderEvent> { vec![OrderEvent::Archived] }\n",
        );
        let (commands, events) = maps();
        let arms = decision_arms(
            &[file("order", decide)],
            &[placing, payments, archiving],
            &commands,
            &events,
        );
        let by = by_command(&arms);
        assert_eq!(by["OrderCommand::Place"], ["OrderEvent::Placed"], "{arms:?}");
        assert_eq!(by["OrderCommand::Cancel"], ["OrderEvent::Cancelled"], "own file wins");
        assert_eq!(
            by["OrderCommand::Archive"],
            ["OrderEvent::Archived"],
            "a unique name resolves, even under a module alias"
        );

        // Two `create`s and neither in the caller's file: ambiguous, not followed.
        let decide2 = file("order", "fn decide(c: OrderCommand) -> Vec<OrderEvent> { match c { OrderCommand::Place { .. } => create(), } }\n");
        let a =
            file("a", "fn create() -> Vec<OrderEvent> { vec![OrderEvent::Placed { id: 1 }] }\n");
        let b = file("b", "fn create() -> Vec<OrderEvent> { vec![OrderEvent::Noted(x)] }\n");
        let arms = decision_arms(&[decide2], &[a, b], &commands, &events);
        assert!(arms[0].events.is_empty(), "{arms:?}");
    }
}
