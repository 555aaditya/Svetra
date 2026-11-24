//! `svetra patch` (docs/patch.md): checked edits to `svetra.yaml`, made on the text so that
//! comments, blank lines and key order stay. All edits apply to a copy; the result is validated
//! as a whole and written only when it is valid.

use crate::yaml::{self, Node, Value};
use std::path::Path;

/// One edit, on a path of map keys such as `entities.Booking.fields.notes`.
#[derive(Debug, Clone)]
pub enum Edit {
    Set { path: Vec<String>, value: Node },
    Remove { path: Vec<String> },
    Rename { path: Vec<String>, to: String },
}

/// Something wrong: with one edit (`at` is `ops[2]`, code `edit`) or with the result (`at` is a
/// position in the file as it would be written, code `SVT…`).
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub at: String,
    pub code: String,
    pub message: String,
    pub hint: String,
}

type Fail = (String, String);

fn fail(message: impl Into<String>, hint: impl Into<String>) -> Fail {
    (message.into(), hint.into())
}

/// Read the edit list: a JSON array of `{op, path, value | to}` objects. Anything that isn't a
/// well-formed list is a usage error, reported with the offending index.
pub fn parse_ops(text: &str) -> Result<Vec<Edit>, String> {
    if !text.trim_start().starts_with('[') {
        return Err("the edits must be a JSON array, such as [{\"op\": \"remove\", \"path\": \"entities.Old\"}]".into());
    }
    let root = yaml::parse(text).map_err(|e| {
        format!(
            "the edits aren't valid JSON: {} (line {}, column {})",
            e.message,
            e.line,
            e.col + 1
        )
    })?;
    let Value::Seq(items) = &root.value else {
        return Err("the edits must be a JSON array".into());
    };
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let Value::Map(entries) = &item.value else {
            return Err(format!(
                "ops[{i}] must be an object with \"op\" and \"path\""
            ));
        };
        for (k, _) in entries {
            if !matches!(k.as_str(), Some("op" | "path" | "value" | "to")) {
                return Err(format!(
                    "ops[{i}] has an unknown key {:?}: use op, path, value or to",
                    k.as_str().unwrap_or("?")
                ));
            }
        }
        let get = |name: &str| {
            entries
                .iter()
                .find(|(k, _)| k.as_str() == Some(name))
                .map(|(_, v)| v)
        };
        let text_of = |name: &str| -> Result<String, String> {
            get(name)
                .and_then(Node::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("ops[{i}] needs a string {name:?}"))
        };
        let op = text_of("op")?;
        let path_text = text_of("path")?;
        let path: Vec<String> = path_text.split('.').map(str::to_string).collect();
        if path.iter().any(String::is_empty) {
            return Err(format!(
                "ops[{i}]: the path {path_text:?} has an empty part: write names separated by single dots"
            ));
        }
        let only = |extra: &str| -> Result<(), String> {
            for key in ["value", "to"] {
                if key != extra && get(key).is_some() {
                    return Err(format!("ops[{i}] ({op}) doesn't take {key:?}"));
                }
            }
            Ok(())
        };
        out.push(match op.as_str() {
            "set" => {
                only("value")?;
                let value = get("value")
                    .ok_or_else(|| format!("ops[{i}] (set) needs a \"value\""))?
                    .clone();
                Edit::Set { path, value }
            }
            "remove" => {
                only("")?;
                Edit::Remove { path }
            }
            "rename" => {
                only("to")?;
                Edit::Rename {
                    path,
                    to: text_of("to")?,
                }
            }
            other => {
                return Err(format!(
                    "ops[{i}] has an unknown op {other:?}: use set, remove or rename"
                ));
            }
        });
    }
    Ok(out)
}

/// Apply every edit, in order, each to the result of the one before. An edit that can't be
/// applied is reported and skipped, so one call lists everything that is wrong.
pub fn apply(src: &str, ops: &[Edit]) -> (String, Vec<Problem>) {
    let mut text = src.to_string();
    let mut problems = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        match apply_one(&text, op) {
            Ok(next) => text = next,
            Err((message, hint)) => problems.push(Problem {
                at: format!("ops[{i}]"),
                code: "edit".into(),
                message,
                hint,
            }),
        }
    }
    (text, problems)
}

fn apply_one(src: &str, op: &Edit) -> Result<String, Fail> {
    let root = parse_for_edit(src)?;
    let mut t = Text::new(src);
    match op {
        Edit::Set { path, value } => set(&mut t, &root, path, value)?,
        Edit::Remove { path } => remove(&mut t, &root, path)?,
        Edit::Rename { path, to } => {
            rename(&mut t, &root, path, to)?;
            let out = t.render();
            let Some(old) = was_needed(&root, path) else {
                return Ok(out);
            };
            // A rename of an entity or a field declares what it was, so `plan` keeps the data.
            let root = parse_for_edit(&out)?;
            let mut t = Text::new(&out);
            let mut renamed = path.clone();
            if let Some(last) = renamed.last_mut() {
                *last = to.clone();
            }
            renamed.push("was".into());
            set(&mut t, &root, &renamed, &scalar_node(&old))?;
            return Ok(t.render());
        }
    }
    Ok(t.render())
}

fn parse_for_edit(src: &str) -> Result<Node, Fail> {
    yaml::parse(src).map_err(|e| {
        fail(
            format!(
                "the file doesn't parse (line {}, column {}): {}",
                e.line,
                e.col + 1,
                e.message
            ),
            "Fix the file first: svetra validate shows the problem.",
        )
    })
}

// ---------------------------------------------------------------------------------------------
// The text, as lines of characters. Columns are character positions, as the parser reports them.

struct Text {
    lines: Vec<Vec<char>>,
    crlf: bool,
    final_newline: bool,
}

type Pos = (usize, usize);

impl Text {
    fn new(src: &str) -> Text {
        let mut lines: Vec<Vec<char>> = src
            .split('\n')
            .map(|l| l.trim_end_matches('\r').chars().collect())
            .collect();
        if src.ends_with('\n') || src.is_empty() {
            lines.pop();
        }
        Text {
            lines,
            crlf: src.contains("\r\n"),
            final_newline: src.is_empty() || src.ends_with('\n'),
        }
    }

    fn render(&self) -> String {
        let eol = if self.crlf { "\r\n" } else { "\n" };
        let mut out = self
            .lines
            .iter()
            .map(|l| l.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join(eol);
        if self.final_newline && !self.lines.is_empty() {
            out += eol;
        }
        out
    }

    fn line(&self, l: usize) -> &[char] {
        self.lines.get(l).map_or(&[], Vec::as_slice)
    }

    fn insert_lines(&mut self, at: usize, new: Vec<String>) {
        let at = at.min(self.lines.len());
        let new: Vec<Vec<char>> = new.iter().map(|l| l.chars().collect()).collect();
        self.lines.splice(at..at, new);
    }

    /// Delete lines `from..=to`.
    fn delete_lines(&mut self, from: usize, to: usize) {
        if from < self.lines.len() {
            self.lines.drain(from..=to.min(self.lines.len() - 1));
        }
    }

    /// Delete the text from `start` up to, not including, `end`; the two lines join.
    fn delete_range(&mut self, start: Pos, end: Pos) {
        if start.0 >= self.lines.len() {
            return;
        }
        let last = end.0.min(self.lines.len() - 1);
        let head: Vec<char> = self.line(start.0).iter().take(start.1).copied().collect();
        let tail: Vec<char> = self.line(last).iter().skip(end.1).copied().collect();
        let joined: Vec<char> = head.into_iter().chain(tail).collect();
        self.lines.splice(start.0..=last, [joined]);
    }

    fn insert_str(&mut self, l: usize, c: usize, s: &str) {
        if let Some(line) = self.lines.get_mut(l) {
            let at = c.min(line.len());
            line.splice(at..at, s.chars());
        }
    }

    /// Drop trailing blanks after `from` when nothing else follows on the line.
    fn trim_blank_tail(&mut self, l: usize, from: usize) {
        if let Some(line) = self.lines.get_mut(l)
            && line.iter().skip(from).all(|c| *c == ' ')
        {
            line.truncate(from.min(line.len()));
        }
    }
}

fn indent(line: &[char]) -> usize {
    line.iter().take_while(|c| **c == ' ').count()
}

fn is_blank(line: &[char]) -> bool {
    line.iter().all(|c| c.is_whitespace())
}

fn is_comment(line: &[char]) -> bool {
    line.iter().find(|c| !c.is_whitespace()) == Some(&'#')
}

/// The end of a quoted scalar that starts at `start`: just after the closing quote.
fn quoted_end(line: &[char], start: usize) -> usize {
    let quote = line.get(start).copied().unwrap_or('"');
    let mut i = start + 1;
    while i < line.len() {
        if quote == '"' && line[i] == '\\' {
            i += 2;
            continue;
        }
        if line[i] == quote {
            if quote == '\'' && line.get(i + 1) == Some(&'\'') {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    line.len()
}

/// The end of a plain scalar: before a ` #` comment, and in a flow collection before `,`, `}` or `]`.
fn plain_end(line: &[char], start: usize, flow: bool) -> usize {
    let mut i = start;
    while i < line.len() {
        let c = line[i];
        if c == '#' && i > start && line[i - 1] == ' ' {
            break;
        }
        if flow && matches!(c, ',' | '}' | ']') {
            break;
        }
        i += 1;
    }
    while i > start + 1 && line[i - 1] == ' ' {
        i -= 1;
    }
    i
}

/// Where the key that starts at `col` ends: after its closing quote, or before its colon.
fn key_end(line: &[char], col: usize) -> usize {
    if matches!(line.get(col), Some('"' | '\'')) {
        return quoted_end(line, col);
    }
    let mut i = col;
    while i < line.len() {
        if line[i] == ':' && line.get(i + 1).is_none_or(|c| *c == ' ') {
            break;
        }
        i += 1;
    }
    while i > col && line[i - 1] == ' ' {
        i -= 1;
    }
    i
}

/// The position of the bracket that closes the one at `(l, c)`, skipping quotes and comments.
fn flow_end(t: &Text, l: usize, c: usize) -> Option<Pos> {
    let (mut depth, mut quote) = (0i32, None::<char>);
    let mut ci = c;
    for li in l..t.lines.len() {
        let line = t.line(li);
        while ci < line.len() {
            let ch = line[ci];
            if let Some(q) = quote {
                if q == '"' && ch == '\\' {
                    ci += 2;
                    continue;
                }
                if ch == q {
                    if q == '\'' && line.get(ci + 1) == Some(&'\'') {
                        ci += 2;
                        continue;
                    }
                    quote = None;
                }
                ci += 1;
                continue;
            }
            match ch {
                '"' | '\'' => quote = Some(ch),
                '#' if ci == 0 || line[ci - 1] == ' ' => break,
                '{' | '[' => depth += 1,
                '}' | ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((li, ci));
                    }
                }
                _ => {}
            }
            ci += 1;
        }
        ci = 0;
    }
    None
}

/// What follows a key's colon.
enum Slot {
    /// Nothing on the key's line (or only a comment): the value is on the lines below, or null.
    Below,
    /// A value on the key's line, from `start` up to, not including, `end`.
    Inline { start: Pos, end: Pos },
}

/// The slot after `key`, and the column of its colon.
fn slot_after(t: &Text, key: &Node, in_flow: bool) -> (Slot, usize) {
    let l = key.line.saturating_sub(1);
    let line = t.line(l);
    let kend = key_end(line, key.col);
    let mut colon = kend;
    while line.get(colon) == Some(&' ') {
        colon += 1;
    }
    if line.get(colon) != Some(&':') {
        return (Slot::Below, kend);
    }
    let mut s = colon + 1;
    while line.get(s) == Some(&' ') {
        s += 1;
    }
    let Some(&first) = line.get(s) else {
        return (Slot::Below, colon);
    };
    if first == '#' {
        return (Slot::Below, colon);
    }
    let end = match first {
        '{' | '[' => match flow_end(t, l, s) {
            Some((el, ec)) => (el, ec + 1),
            None => (l, line.len()),
        },
        '"' | '\'' => (l, quoted_end(line, s)),
        _ => (l, plain_end(line, s, in_flow)),
    };
    (Slot::Inline { start: (l, s), end }, colon)
}

/// The last line of the entry whose key is at `(key_line, key_col)`: every following line that is
/// indented deeper, plus blank and comment lines among them, without the ones at the end (those
/// belong to what follows).
fn entry_extent(t: &Text, key_line: usize, key_col: usize) -> usize {
    let mut last = key_line;
    let mut j = key_line + 1;
    while j < t.lines.len() {
        let line = t.line(j);
        if is_blank(line) || is_comment(line) {
            j += 1;
            continue;
        }
        let i = indent(line);
        let dash = line.get(i) == Some(&'-') && line.get(i + 1).is_none_or(|c| *c == ' ');
        if i > key_col || (i == key_col && dash) {
            last = j;
            j += 1;
        } else {
            break;
        }
    }
    last
}

// ---------------------------------------------------------------------------------------------
// Finding a path in the parsed tree.

struct Walk<'a> {
    /// The root, then one `(key, value)` per resolved segment.
    chain: Vec<(Option<&'a Node>, &'a Node)>,
    outcome: Outcome,
}

enum Outcome {
    Hit,
    /// The next segment isn't there (in a map, or in a null that stands for an empty one).
    Missing,
    /// The next segment would go under a value that isn't a map.
    NotMap(&'static str),
}

fn walk<'a>(root: &'a Node, path: &[String]) -> Walk<'a> {
    let mut chain: Vec<(Option<&Node>, &Node)> = vec![(None, root)];
    for seg in path {
        let Some(&(_, cur)) = chain.last() else {
            break;
        };
        let outcome = match &cur.value {
            Value::Map(entries) => match entries.iter().find(|(k, _)| k.as_str() == Some(seg)) {
                Some((k, v)) => {
                    chain.push((Some(k), v));
                    continue;
                }
                None => Outcome::Missing,
            },
            Value::Null if chain.len() > 1 => Outcome::Missing,
            _ => Outcome::NotMap(cur.kind()),
        };
        return Walk { chain, outcome };
    }
    Walk {
        chain,
        outcome: Outcome::Hit,
    }
}

fn join(path: &[String]) -> String {
    path.join(".")
}

/// Whether a map node is written in flow style (`{ … }`).
fn is_flow(t: &Text, node: &Node) -> bool {
    t.line(node.line.saturating_sub(1)).get(node.col) == Some(&'{')
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    /// No sibling to follow.
    Auto,
    Flow,
    Block,
}

/// The style of the first sibling that is a map, so a new map is written like the ones around it.
fn sibling_style(t: &Text, parent: &Node, skip: Option<&str>) -> Style {
    let Value::Map(entries) = &parent.value else {
        return Style::Auto;
    };
    for (k, v) in entries {
        if k.as_str() == skip && skip.is_some() {
            continue;
        }
        if matches!(&v.value, Value::Map(m) if !m.is_empty()) {
            return if is_flow(t, v) {
                Style::Flow
            } else {
                Style::Block
            };
        }
    }
    Style::Auto
}

// ---------------------------------------------------------------------------------------------
// Writing values.

fn scalar_node(s: &str) -> Node {
    Node {
        value: Value::Str(s.to_string()),
        line: 0,
        col: 0,
    }
}

fn reserved(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "y" | "n" | "~"
    )
}

/// A string written plain when it is a simple name or dotted name, and double-quoted otherwise.
fn text(s: &str, dotted: bool) -> String {
    let mut chars = s.chars();
    let plain = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || (dotted && c == '.'))
        && !reserved(s);
    if plain {
        return s.to_string();
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out += "\\\"",
            '\\' => out += "\\\\",
            '\n' => out += "\\n",
            '\r' => out += "\\r",
            '\t' => out += "\\t",
            c if (c as u32) < 0x20 => out += &format!("\\u{:04x}", c as u32),
            c => out.push(c),
        }
    }
    out + "\""
}

fn render_key(name: &str) -> String {
    text(name, false)
}

fn scalar(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => format!("{f:?}"),
        Value::Str(s) => text(s, true),
        Value::Seq(_) | Value::Map(_) => String::new(),
    }
}

fn key_text(k: &Node) -> String {
    match &k.value {
        Value::Str(s) => render_key(s),
        other => scalar(other),
    }
}

/// A value on one line: `{ type: text }`, `[a, b]` or a scalar.
fn flow(v: &Node) -> String {
    match &v.value {
        Value::Map(m) if m.is_empty() => "{}".into(),
        Value::Map(m) => format!(
            "{{ {} }}",
            m.iter()
                .map(|(k, c)| format!("{}: {}", key_text(k), flow(c)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Seq(items) => format!(
            "[{}]",
            items.iter().map(flow).collect::<Vec<_>>().join(", ")
        ),
        other => scalar(other),
    }
}

/// A map none of whose values is a map.
fn is_leaf(v: &Node) -> bool {
    matches!(&v.value, Value::Map(m) if m.iter().all(|(_, c)| !matches!(c.value, Value::Map(_))))
}

enum Rendered {
    Inline(String),
    /// The lines under the key.
    Block(Vec<String>),
}

fn render_value(v: &Node, child_indent: usize, style: Style, force_flow: bool) -> Rendered {
    match &v.value {
        Value::Map(m) if !m.is_empty() && !force_flow => {
            if style != Style::Block && is_leaf(v) {
                Rendered::Inline(flow(v))
            } else {
                Rendered::Block(
                    m.iter()
                        .flat_map(|(k, c)| entry_lines(&key_text(k), c, child_indent, Style::Auto))
                        .collect(),
                )
            }
        }
        _ => Rendered::Inline(flow(v)),
    }
}

/// `key: value` as lines at `indent`; `key` is already written.
fn entry_lines(key: &str, v: &Node, indent: usize, style: Style) -> Vec<String> {
    let pad = " ".repeat(indent);
    match render_value(v, indent + 2, style, false) {
        Rendered::Inline(s) => vec![format!("{pad}{key}: {s}")],
        Rendered::Block(mut lines) => {
            lines.insert(0, format!("{pad}{key}:"));
            lines
        }
    }
}

/// `a.b = v` as nested maps, for a path whose first parts don't exist yet.
fn nest(path: &[String], v: &Node) -> Node {
    match path.split_first() {
        None => v.clone(),
        Some((first, rest)) => Node {
            value: Value::Map(vec![(scalar_node(first), nest(rest, v))]),
            line: 0,
            col: 0,
        },
    }
}

// ---------------------------------------------------------------------------------------------
// The three edits.

fn internal() -> Fail {
    fail(
        "the edit couldn't be placed in the file",
        "Send the edits again, or edit svetra.yaml by hand.",
    )
}

fn set(t: &mut Text, root: &Node, path: &[String], value: &Node) -> Result<(), Fail> {
    let w = walk(root, path);
    let done = w.chain.len() - 1;
    match w.outcome {
        Outcome::NotMap(kind) => Err(fail(
            format!(
                "`{}` is a {kind}, not a map, so `{}` can't go under it",
                join(&path[..done]),
                path[done]
            ),
            "Set the whole value at that path instead.",
        )),
        Outcome::Hit => replace_value(t, &w, value),
        Outcome::Missing => {
            let rest = &path[done..];
            let nested = nest(&rest[1..], value);
            add_key(t, &w, &rest[0], &nested)
        }
    }
}

fn replace_value(t: &mut Text, w: &Walk, new: &Node) -> Result<(), Fail> {
    let [.., (_, parent), (Some(key), _)] = w.chain.as_slice() else {
        return Err(internal());
    };
    let in_flow = is_flow(t, parent);
    let style = if in_flow {
        Style::Flow
    } else {
        sibling_style(t, parent, key.as_str())
    };
    let kl = key.line - 1;
    let (slot, colon) = slot_after(t, key, in_flow);
    let (insert_at, inline_before) = match slot {
        Slot::Inline { start, end } => {
            t.delete_range(start, end);
            (start, true)
        }
        Slot::Below => ((kl, colon + 1), false),
    };
    if !in_flow {
        let ext = entry_extent(t, kl, key.col);
        if ext > kl {
            t.delete_lines(kl + 1, ext);
        }
    }
    match render_value(new, key.col + 2, style, in_flow) {
        Rendered::Inline(s) => {
            let s = if inline_before { s } else { format!(" {s}") };
            t.insert_str(insert_at.0, insert_at.1, &s);
        }
        Rendered::Block(lines) => {
            t.trim_blank_tail(kl, colon + 1);
            t.insert_lines(kl + 1, lines);
        }
    }
    Ok(())
}

/// Where the entry for `key` ends on its line: after its value, or after the key without one.
fn entry_end(t: &Text, key: &Node, in_flow: bool) -> Pos {
    match slot_after(t, key, in_flow) {
        (Slot::Inline { end, .. }, _) => end,
        (Slot::Below, _) => (
            key.line - 1,
            key_end(t.line(key.line.saturating_sub(1)), key.col),
        ),
    }
}

fn add_key(t: &mut Text, w: &Walk, name: &str, val: &Node) -> Result<(), Fail> {
    let Some(&(pkey, pval)) = w.chain.last() else {
        return Err(internal());
    };
    let key = render_key(name);
    match &pval.value {
        Value::Map(entries) if !entries.is_empty() => {
            let Some((last_key, _)) = entries.last() else {
                return Err(internal());
            };
            if is_flow(t, pval) {
                let (l, c) = entry_end(t, last_key, true);
                t.insert_str(l, c, &format!(", {key}: {}", flow(val)));
            } else {
                let style = sibling_style(t, pval, None);
                let ext = entry_extent(t, last_key.line - 1, last_key.col);
                let col = entries.first().map_or(0, |(k, _)| k.col);
                t.insert_lines(ext + 1, entry_lines(&key, val, col, style));
            }
        }
        _ => match pkey {
            None => {
                let at = t.lines.len();
                t.insert_lines(at, entry_lines(&key, val, 0, Style::Auto));
            }
            Some(pk) => {
                let parent_flow = w
                    .chain
                    .len()
                    .checked_sub(2)
                    .and_then(|i| w.chain.get(i))
                    .is_some_and(|(_, p)| is_flow(t, p));
                let (slot, colon) = slot_after(t, pk, parent_flow);
                let kl = pk.line - 1;
                match slot {
                    Slot::Below => {
                        t.insert_lines(kl + 1, entry_lines(&key, val, pk.col + 2, Style::Auto));
                    }
                    Slot::Inline { start, end } => {
                        // `{}` or `null`: written again with the new key in it.
                        t.delete_range(start, end);
                        if parent_flow {
                            let one = nest(&[name.to_string()], val);
                            t.insert_str(start.0, start.1, &flow(&one));
                        } else {
                            t.trim_blank_tail(kl, colon + 1);
                            t.insert_lines(kl + 1, entry_lines(&key, val, pk.col + 2, Style::Auto));
                        }
                    }
                }
            }
        },
    }
    Ok(())
}

fn missing(path: &[String]) -> Fail {
    fail(
        format!("`{}` isn't there", join(path)),
        "Check the path against svetra context.",
    )
}

/// The found entry for `path`: its key, and the map it is in.
fn found<'a>(w: &Walk<'a>, path: &[String]) -> Result<(&'a Node, &'a Node), Fail> {
    match w.outcome {
        Outcome::Hit => {}
        Outcome::Missing => return Err(missing(path)),
        Outcome::NotMap(kind) => {
            let done = w.chain.len() - 1;
            return Err(fail(
                format!(
                    "`{}` is a {kind}, not a map, so `{}` isn't under it",
                    join(&path[..done]),
                    path[done]
                ),
                "Paths go through maps only.",
            ));
        }
    }
    match w.chain.as_slice() {
        [.., (_, parent), (Some(key), _)] => Ok((key, parent)),
        _ => Err(internal()),
    }
}

fn remove(t: &mut Text, root: &Node, path: &[String]) -> Result<(), Fail> {
    let w = walk(root, path);
    let (key, parent) = found(&w, path)?;
    let Value::Map(entries) = &parent.value else {
        return Err(internal());
    };
    let at = entries
        .iter()
        .position(|(k, _)| (k.line, k.col) == (key.line, key.col))
        .ok_or_else(internal)?;
    if is_flow(t, parent) {
        let here = (key.line - 1, key.col);
        if entries.len() == 1 {
            let open = (parent.line - 1, parent.col + 1);
            let close = flow_end(t, parent.line - 1, parent.col).ok_or_else(internal)?;
            t.delete_range(open, close);
        } else if let Some((next, _)) = entries.get(at + 1) {
            t.delete_range(here, (next.line - 1, next.col));
        } else {
            let (prev, _) = &entries[at - 1];
            let from = entry_end(t, prev, true);
            let to = entry_end(t, key, true);
            t.delete_range(from, to);
        }
        return Ok(());
    }
    // The comment lines directly above go with it; a blank line ends them.
    let mut first = key.line - 1;
    while first > 0 && is_comment(t.line(first - 1)) {
        first -= 1;
    }
    let last = entry_extent(t, key.line - 1, key.col);
    t.delete_lines(first, last);
    // A map left with nothing in it stays a map: `policies: {}`, not a bare `policies:`.
    if entries.len() == 1
        && let [.., (Some(pk), _), _] = w.chain.as_slice()
    {
        let in_flow = false;
        if let (Slot::Below, colon) = slot_after(t, pk, in_flow) {
            t.insert_str(pk.line - 1, colon + 1, " {}");
        }
    }
    Ok(())
}

fn rename(t: &mut Text, root: &Node, path: &[String], to: &str) -> Result<(), Fail> {
    let w = walk(root, path);
    let (key, parent) = found(&w, path)?;
    if let Value::Map(entries) = &parent.value
        && entries
            .iter()
            .any(|(k, _)| k.as_str() == Some(to) && (k.line, k.col) != (key.line, key.col))
    {
        return Err(fail(
            format!(
                "can't rename `{}` to `{to}`: there is already a `{to}` there",
                join(path)
            ),
            "Choose another name, or remove the existing one first.",
        ));
    }
    let l = key.line - 1;
    let end = key_end(t.line(l), key.col);
    t.delete_range((l, key.col), (l, end));
    t.insert_str(l, key.col, &render_key(to));
    Ok(())
}

/// The old name to record as `was`, for an entity or a field that has none yet.
fn was_needed(root: &Node, path: &[String]) -> Option<String> {
    let known = match path {
        [a, _] => a == "entities",
        [a, _, c, _] => a == "entities" && c == "fields",
        _ => false,
    };
    if !known {
        return None;
    }
    let w = walk(root, path);
    let Outcome::Hit = w.outcome else {
        return None;
    };
    match w.chain.last() {
        Some((_, v)) => match &v.value {
            Value::Map(m) if !m.iter().any(|(k, _)| k.as_str() == Some("was")) => {
                path.last().cloned()
            }
            _ => None,
        },
        None => None,
    }
}

// ---------------------------------------------------------------------------------------------
// The whole command.

/// What `svetra patch` did, or refused to do.
pub struct Report {
    pub file: String,
    pub dry_run: bool,
    pub applied: bool,
    /// The file really was written.
    pub written: bool,
    /// Another revision than the one the edits were written against: `(expected, now)`.
    pub conflict: Option<(String, String)>,
    pub was: String,
    pub now: Option<String>,
    pub problems: Vec<Problem>,
    /// Warnings and info about the result. They never stop the write.
    pub notes: Vec<Problem>,
    /// The system's name and what changed, when the file as it was is valid to compare.
    pub plan: Option<(String, crate::change::Plan)>,
}

fn diag_problem(file: &str, d: &crate::diag::Diagnostic) -> Problem {
    Problem {
        at: format!("{file}:{}:{}", d.line, d.col + 1),
        code: d.code.id().to_string(),
        message: d.message.clone(),
        hint: d.hint.clone(),
    }
}

/// Apply `ops` to `file`, check the result, and write it unless `dry_run`. An `Err` is a file
/// that couldn't be read or written.
pub fn run(
    file: &Path,
    ops: &[Edit],
    expected: Option<&str>,
    dry_run: bool,
) -> Result<Report, String> {
    let name = file.display().to_string();
    let read = || {
        std::fs::read_to_string(file).map_err(|e| format!("cannot read {}: {e}", file.display()))
    };
    let src = read()?;
    let was = crate::compile::revision(&src);
    let mut report = Report {
        file: name.clone(),
        dry_run,
        applied: false,
        written: false,
        conflict: None,
        was: was.clone(),
        now: None,
        problems: Vec::new(),
        notes: Vec::new(),
        plan: None,
    };
    if let Some(e) = expected
        && e != was
    {
        report.conflict = Some((e.to_string(), was));
        return Ok(report);
    }
    let (new_src, problems) = apply(&src, ops);
    if !problems.is_empty() {
        report.problems = problems;
        return Ok(report);
    }
    let diags = crate::report::validate(&new_src);
    // Notes are only what the edits brought: what the file already said, it says again on
    // `validate`, and an agent shouldn't pay for it on every patch.
    let before = crate::report::validate(&src);
    for d in &diags {
        let p = diag_problem(&name, d);
        if d.severity() == crate::diag::Severity::Error {
            report.problems.push(p);
        } else if !before
            .iter()
            .any(|o| o.code == d.code && o.message == d.message)
        {
            report.notes.push(p);
        }
    }
    if !report.problems.is_empty() {
        return Ok(report);
    }
    report.applied = true;
    let now = crate::compile::revision(&new_src);
    report.now = Some(now.clone());
    if new_src == src {
        return Ok(report);
    }
    if !crate::report::has_errors(&before) {
        let (old, _) = crate::load::load(&src);
        let (new, _) = crate::load::load(&new_src);
        let plan = crate::change::plan(&old, &src, &new, &new_src);
        report.plan = Some((new.system.name.clone(), plan));
    }
    if !dry_run {
        // The file may have changed while the edits were checked.
        let latest = crate::compile::revision(&read()?);
        if latest != was {
            report.applied = false;
            report.now = None;
            report.conflict = Some((was.clone(), latest));
            report.plan = None;
            return Ok(report);
        }
        crate::project::write_atomic(file, &new_src)
            .map_err(|e| format!("cannot write {}: {e}", file.display()))?;
        report.written = true;
    }
    Ok(report)
}

const CONFLICT_HINT: &str = "svetra.yaml changed since you read it. Read it again with svetra context, redo your edits against it, and send them with the new revision.";

fn problem_json(p: &Problem) -> serde_json::Value {
    serde_json::json!({ "at": p.at, "code": p.code, "message": p.message, "hint": p.hint })
}

/// The answer as JSON (docs/patch.md, Output and Conflicts).
pub fn json(r: &Report) -> serde_json::Value {
    use serde_json::json;
    if let Some((expected, now)) = &r.conflict {
        return json!({
            "applied": false,
            "dry_run": r.dry_run,
            "conflict": true,
            "revision": { "expected": expected, "now": now },
            "hint": CONFLICT_HINT,
        });
    }
    let mut revision = json!({ "was": r.was });
    if let Some(now) = &r.now {
        revision["now"] = json!(now);
    }
    let mut out = json!({
        "applied": r.applied,
        "dry_run": r.dry_run,
        "revision": revision,
        "problems": r.problems.iter().map(problem_json).collect::<Vec<_>>(),
        "notes": r.notes.iter().map(problem_json).collect::<Vec<_>>(),
    });
    match &r.plan {
        Some((_, plan)) => {
            let v = crate::change::json(plan, "", &[]);
            out["changes"] = v["changes"].clone();
            out["needs_yes"] = v["needs_yes"].clone();
        }
        None if r.applied => {
            out["changes"] = json!([]);
            out["needs_yes"] = json!([]);
            if r.now.as_deref() != Some(r.was.as_str()) {
                out["comparison"] =
                    json!("The file as it was doesn't validate, so what changed isn't compared.");
            }
        }
        None => {}
    }
    out
}

/// The answer for a person.
pub fn human(r: &Report, edits: usize) -> String {
    if let Some((expected, now)) = &r.conflict {
        return format!(
            "svetra patch: conflict, nothing written. {} is at revision {now}, not {expected}.\n{CONFLICT_HINT}\n",
            r.file
        );
    }
    let mut out = String::new();
    if !r.problems.is_empty() {
        out += &format!(
            "svetra patch: refused, nothing written ({} problem{})\n",
            r.problems.len(),
            if r.problems.len() == 1 { "" } else { "s" }
        );
        for p in &r.problems {
            out += &format!(
                "  {} {}: {}\n    hint: {}\n",
                p.at, p.code, p.message, p.hint
            );
        }
        return out;
    }
    let now = r.now.as_deref().unwrap_or(&r.was);
    let verb = match (r.dry_run, r.written) {
        (true, _) => "would apply",
        (_, true) => "applied",
        _ => "applied, nothing to write",
    };
    let edits = format!("{edits} edit{}", if edits == 1 { "" } else { "s" });
    if now == r.was {
        out += &format!(
            "svetra patch: {edits} {verb} to {}: no change (revision {})\n",
            r.file, r.was
        );
    } else {
        out += &format!(
            "svetra patch: {edits} {verb} to {} (revision {} -> {now})\n",
            r.file, r.was
        );
    }
    match &r.plan {
        Some((system, plan)) if !plan.changes.is_empty() => {
            let body = crate::change::human(plan, system, "before");
            out += "\n";
            out += body.split_once("\n\n").map_or("", |(_, rest)| rest);
            if !plan.needs_yes().is_empty() {
                out += "\nShow the destructive and wider lines to the person before relying on this. Next: svetra plan.\n";
            } else {
                out += "\nNext: svetra plan.\n";
            }
        }
        None if r.applied && now != r.was => {
            out += "\nThe file as it was doesn't validate, so what changed isn't compared.\n";
        }
        _ => {}
    }
    for n in &r.notes {
        out += &format!("  note {} {}: {}\n", n.at, n.code, n.message);
    }
    out
}
