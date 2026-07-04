/// Native Syrup parser + C-style frontend (C++ / C#).
///
/// ## Native Syrup syntax
///
/// ```text
/// # comment
/// section_name {
///     key = "string value"
///     count = 42
///     ratio = 3.14
///     flag  = true
///     arr   = [1.0, 2.0, 3.0, 4.0]
/// }
/// ```
///
/// ## C-style syntax (cpp / csharp)
///
/// ```cpp
/// // comment
/// section_name {
///     string  key   = "string value";  // type keyword ignored
///     int     count = 42;
///     float   ratio = 3.14;
///     bool    flag  = true;
///     auto    arr   = [1.0, 2.0, 3.0];
/// }
/// ```
use super::{SyrupDoc, SyrupSection, SyrupValue};
use std::collections::HashMap;

// ── Public entry points ───────────────────────────────────────────────────────

/// Parse native Syrup source.
pub fn parse(src: &str) -> SyrupDoc {
    parse_inner(src, false)
}

/// Parse C++ / C# config source (strips leading type keyword before `key`).
pub fn parse_c_style(src: &str) -> SyrupDoc {
    parse_inner(src, true)
}

// ── Shared parser ─────────────────────────────────────────────────────────────

fn parse_inner(src: &str, strip_type: bool) -> SyrupDoc {
    let mut doc: HashMap<String, SyrupSection> = HashMap::new();
    let mut current_section: Option<String> = None;

    for raw_line in src.lines() {
        let line = strip_comment(raw_line).trim().to_string();
        if line.is_empty() { continue; }

        // Skip lang directive
        if line.starts_with("!lang") { continue; }

        // Open block: `section_name {`
        if line.ends_with('{') {
            let name = line.trim_end_matches('{').trim().to_string();
            current_section = Some(name.clone());
            doc.entry(name).or_default();
            continue;
        }

        // Close block
        if line == "}" {
            current_section = None;
            continue;
        }

        // Key = value assignment
        if let Some(ref section) = current_section {
            // Strip trailing semicolon for C-style
            let assignment = line.trim_end_matches(';').trim();

            if let Some((k, v)) = split_assignment(assignment, strip_type) {
                if let Some(val) = parse_value(&v) {
                    doc.entry(section.clone()).or_default().insert(k, val);
                }
            }
        }
    }

    SyrupDoc(doc)
}

// ── Assignment parsing ────────────────────────────────────────────────────────

fn split_assignment(line: &str, strip_type: bool) -> Option<(String, String)> {
    // Find the `=` separator
    let eq = line.find('=')?;
    let mut key_part = line[..eq].trim().to_string();
    let val_part = line[eq + 1..].trim().to_string();

    if strip_type {
        // Remove the leading type keyword if present:
        // `std::string key` → `key`   |  `int count` → `count`
        // Split by whitespace; last token is the identifier
        let parts: Vec<&str> = key_part.split_whitespace().collect();
        if parts.len() > 1 {
            key_part = parts.last().unwrap().to_string();
        }
    }

    // Strip potential inline comments from key (shouldn't happen but defensive)
    let key = key_part.split_whitespace().next()?.to_string();
    Some((key, val_part))
}

// ── Value parsing ─────────────────────────────────────────────────────────────

fn parse_value(s: &str) -> Option<SyrupValue> {
    let s = s.trim();

    // Array  [1, 2, 3]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        let items: Vec<SyrupValue> = inner
            .split(',')
            .filter_map(|item| parse_scalar(item.trim()))
            .collect();
        return Some(SyrupValue::Array(items));
    }

    parse_scalar(s)
}

fn parse_scalar(s: &str) -> Option<SyrupValue> {
    let s = s.trim();
    if s.is_empty() { return None; }

    // Quoted string
    if (s.starts_with('"') && s.ends_with('"')) ||
       (s.starts_with('\'') && s.ends_with('\''))
    {
        return Some(SyrupValue::String(s[1..s.len() - 1].to_string()));
    }

    // Bool
    if s == "true"  { return Some(SyrupValue::Bool(true)); }
    if s == "false" { return Some(SyrupValue::Bool(false)); }

    // Float (must contain `.` or `e`)
    if s.contains('.') || s.contains('e') || s.contains('E') {
        if let Ok(f) = s.parse::<f64>() {
            return Some(SyrupValue::Float(f));
        }
    }

    // Integer
    if let Ok(i) = s.parse::<i64>() {
        return Some(SyrupValue::Int(i));
    }

    // Bare string (unquoted identifier or path)
    Some(SyrupValue::String(s.to_string()))
}

// ── Comment stripping ─────────────────────────────────────────────────────────

fn strip_comment(line: &str) -> &str {
    // Handle `#` and `//` comments, but not inside quoted strings
    let mut in_str = false;
    let mut chars = line.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => in_str = !in_str,
            '#' if !in_str => return &line[..i],
            '/' if !in_str => {
                if chars.peek().map(|&(_, c)| c) == Some('/') {
                    return &line[..i];
                }
            }
            _ => {}
        }
    }
    line
}
