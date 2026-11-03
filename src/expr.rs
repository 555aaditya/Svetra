//! The expression language of docs/format.md: a lexer and a recursive-descent parser.
//! Type checking lives in `check`.

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
    /// `field`, `ref_field.field` or `auth.user`.
    Path(Vec<String>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    In(Box<Expr>, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
}

/// A parse error. `at` is a 0-based character offset into the expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub at: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Word(String),
    Sym(&'static str),
}

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, ParseError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            let n = text.parse().map_err(|_| ParseError {
                message: format!("bad number `{text}`"),
                at: start,
            })?;
            out.push((Tok::Num(n), start));
        } else if c.is_ascii_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push((Tok::Word(chars[start..i].iter().collect()), start));
        } else if c == '\'' {
            i += 1;
            let mut s = String::new();
            loop {
                match chars.get(i) {
                    None => {
                        return Err(ParseError {
                            message: "unclosed string".into(),
                            at: start,
                        });
                    }
                    Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                        s.push('\'');
                        i += 2;
                    }
                    Some('\'') => break,
                    Some(ch) => {
                        s.push(*ch);
                        i += 1;
                    }
                }
            }
            i += 1;
            out.push((Tok::Str(s), start));
        } else {
            let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            let sym = ["==", "!=", "<=", ">="].into_iter().find(|s| *s == two);
            let sym = sym.or_else(|| {
                ["<", ">", "+", "-", "*", "/", "(", ")", "[", "]", ",", "."]
                    .into_iter()
                    .find(|s| s.starts_with(c))
            });
            let Some(sym) = sym else {
                return Err(ParseError {
                    message: format!("unexpected `{c}`"),
                    at: start,
                });
            };
            i += sym.len();
            out.push((Tok::Sym(sym), start));
        }
    }
    Ok(out)
}

pub fn parse(src: &str) -> Result<Expr, ParseError> {
    let toks = lex(src)?;
    let mut p = Parser {
        toks,
        i: 0,
        end: src.chars().count(),
    };
    let e = p.or()?;
    match p.toks.get(p.i) {
        None => Ok(e),
        Some((_, at)) => Err(ParseError {
            message: "unexpected text after the expression".into(),
            at: *at,
        }),
    }
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    i: usize,
    end: usize,
}

impl Parser {
    fn at(&self) -> usize {
        self.toks.get(self.i).map_or(self.end, |t| t.1)
    }

    fn eat_word(&mut self, w: &str) -> bool {
        let hit = matches!(self.toks.get(self.i), Some((Tok::Word(x), _)) if x == w);
        self.i += hit as usize;
        hit
    }

    fn eat_sym(&mut self, s: &str) -> bool {
        let hit = matches!(self.toks.get(self.i), Some((Tok::Sym(x), _)) if *x == s);
        self.i += hit as usize;
        hit
    }

    fn expect(&mut self, s: &str) -> Result<(), ParseError> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            Err(ParseError {
                message: format!("expected `{s}`"),
                at: self.at(),
            })
        }
    }

    fn or(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.and()?;
        while self.eat_word("or") {
            e = Expr::Bin(Op::Or, Box::new(e), Box::new(self.and()?));
        }
        Ok(e)
    }

    fn and(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.not()?;
        while self.eat_word("and") {
            e = Expr::Bin(Op::And, Box::new(e), Box::new(self.not()?));
        }
        Ok(e)
    }

    fn not(&mut self) -> Result<Expr, ParseError> {
        if self.eat_word("not") {
            Ok(Expr::Not(Box::new(self.not()?)))
        } else {
            self.cmp()
        }
    }

    fn cmp(&mut self) -> Result<Expr, ParseError> {
        let left = self.sum()?;
        if self.eat_word("in") {
            self.expect("[")?;
            let mut items = vec![self.unary()?];
            while self.eat_sym(",") {
                items.push(self.unary()?);
            }
            self.expect("]")?;
            return Ok(Expr::In(Box::new(left), items));
        }
        let ops = [
            ("==", Op::Eq),
            ("!=", Op::Ne),
            ("<=", Op::Le),
            (">=", Op::Ge),
            ("<", Op::Lt),
            (">", Op::Gt),
        ];
        for (s, op) in ops {
            if self.eat_sym(s) {
                return Ok(Expr::Bin(op, Box::new(left), Box::new(self.sum()?)));
            }
        }
        Ok(left)
    }

    fn sum(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.term()?;
        loop {
            let op = if self.eat_sym("+") {
                Op::Add
            } else if self.eat_sym("-") {
                Op::Sub
            } else {
                return Ok(e);
            };
            e = Expr::Bin(op, Box::new(e), Box::new(self.term()?));
        }
    }

    fn term(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.unary()?;
        loop {
            let op = if self.eat_sym("*") {
                Op::Mul
            } else if self.eat_sym("/") {
                Op::Div
            } else {
                return Ok(e);
            };
            e = Expr::Bin(op, Box::new(e), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if self.eat_sym("-") {
            Ok(Expr::Neg(Box::new(self.unary()?)))
        } else {
            self.atom()
        }
    }

    fn atom(&mut self) -> Result<Expr, ParseError> {
        let at = self.at();
        let Some((tok, _)) = self.toks.get(self.i).cloned() else {
            return Err(ParseError {
                message: "expression ends too early".into(),
                at,
            });
        };
        self.i += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Sym("(") => {
                let e = self.or()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::Word(w) => Ok(match w.as_str() {
                "true" => Expr::Bool(true),
                "false" => Expr::Bool(false),
                "null" => Expr::Null,
                "and" | "or" | "not" | "in" => {
                    return Err(ParseError {
                        message: format!("unexpected `{w}`"),
                        at,
                    });
                }
                _ => {
                    let mut path = vec![w];
                    while self.eat_sym(".") {
                        match self.toks.get(self.i).cloned() {
                            Some((Tok::Word(next), _)) => {
                                self.i += 1;
                                path.push(next);
                            }
                            _ => {
                                return Err(ParseError {
                                    message: "expected a name after `.`".into(),
                                    at: self.at(),
                                });
                            }
                        }
                    }
                    Expr::Path(path)
                }
            }),
            Tok::Sym(s) => Err(ParseError {
                message: format!("unexpected `{s}`"),
                at,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_and_paths() {
        let e = parse("source.owner == auth.user or not a > 1 + 2 * 3").unwrap();
        let Expr::Bin(Op::Or, l, r) = e else {
            panic!("or at the top")
        };
        assert_eq!(
            *l,
            Expr::Bin(
                Op::Eq,
                Box::new(Expr::Path(vec!["source".into(), "owner".into()])),
                Box::new(Expr::Path(vec!["auth".into(), "user".into()]))
            )
        );
        assert!(matches!(*r, Expr::Not(_)));
    }

    #[test]
    fn strings_lists_and_errors() {
        assert_eq!(
            parse("s in ['a', 'it''s']").unwrap(),
            Expr::In(
                Box::new(Expr::Path(vec!["s".into()])),
                vec![Expr::Str("a".into()), Expr::Str("it's".into())]
            )
        );
        assert_eq!(parse("a ==").unwrap_err().at, 4);
        assert_eq!(parse("a = 1").unwrap_err().at, 2);
        assert!(parse("'open").is_err());
    }
}
