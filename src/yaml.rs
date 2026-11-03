//! A small owned YAML tree with positions, so the rest of the crate never sees saphyr types.

use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, YamlData};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Seq(Vec<Node>),
    /// Keys keep source order. Duplicate keys never get here: the parser rejects them.
    Map(Vec<(Node, Node)>),
}

/// A YAML node. `line` is 1-based and `col` is 0-based, as reported by the parser.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub value: Value,
    pub line: usize,
    pub col: usize,
}

impl Node {
    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self.value {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "integer",
            Value::Float(_) => "number",
            Value::Str(_) => "string",
            Value::Seq(_) => "list",
            Value::Map(_) => "map",
        }
    }
}

/// A YAML syntax error with its position.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

/// Parse the first YAML document in `src`. An empty input is an empty map.
pub fn parse(src: &str) -> Result<Node, SyntaxError> {
    let docs = MarkedYaml::load_from_str(src).map_err(|e| SyntaxError {
        message: e.info().to_string(),
        line: e.marker().line(),
        col: e.marker().col(),
    })?;
    Ok(match docs.first() {
        Some(doc) => convert(doc),
        None => Node {
            value: Value::Map(Vec::new()),
            line: 1,
            col: 0,
        },
    })
}

fn convert(node: &MarkedYaml) -> Node {
    let value = match &node.data {
        YamlData::Value(scalar) => match scalar {
            Scalar::Null => Value::Null,
            Scalar::Boolean(b) => Value::Bool(*b),
            Scalar::Integer(i) => Value::Int(*i),
            Scalar::FloatingPoint(f) => Value::Float(f.into_inner()),
            Scalar::String(s) => Value::Str(s.to_string()),
        },
        YamlData::Sequence(items) => Value::Seq(items.iter().map(convert).collect()),
        YamlData::Mapping(map) => {
            Value::Map(map.iter().map(|(k, v)| (convert(k), convert(v))).collect())
        }
        YamlData::Tagged(_, inner) => return convert(inner),
        // Aliases, raw representations and bad values are not part of the format.
        _ => Value::Null,
    };
    Node {
        value,
        line: node.span.start.line(),
        col: node.span.start.col(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_positions_and_order() {
        let n = parse("b: 1\na:\n  - x\n").unwrap();
        let Value::Map(entries) = n.value else {
            panic!("map")
        };
        assert_eq!(entries[0].0.as_str(), Some("b"));
        assert_eq!(entries[1].0.as_str(), Some("a"));
        assert_eq!((entries[1].0.line, entries[1].0.col), (2, 0));
        let Value::Seq(items) = &entries[1].1.value else {
            panic!("seq")
        };
        assert_eq!((items[0].line, items[0].col), (3, 4));
    }

    #[test]
    fn quoted_version_is_a_string() {
        let n = parse("svetra: \"0.1\"\n").unwrap();
        let Value::Map(entries) = n.value else {
            panic!("map")
        };
        assert_eq!(entries[0].1.value, Value::Str("0.1".into()));
    }

    #[test]
    fn duplicate_keys_are_syntax_errors() {
        let e = parse("a: 1\na: 2\n").unwrap_err();
        assert_eq!((e.line, e.col), (2, 0));
    }

    #[test]
    fn reports_syntax_errors_with_position() {
        let e = parse("a: [1, 2\n").unwrap_err();
        assert!(e.line >= 1);
    }
}
