//! Diagnostics. Each `Code` fixes its id, severity, category and remediation action,
//! so a rule can't be reported two different ways.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Svt001,
    Svt002,
    Svt003,
    Svt004,
    Svt005,
    Svt010,
    Svt019,
    Svt020,
    Svt021,
    Svt022,
    Svt023,
    Svt024,
    Svt025,
    Svt026,
    Svt027,
    Svt028,
    Svt029,
}

impl Code {
    /// (id, severity, category, remediation action), as listed in docs/format.md.
    fn row(self) -> (&'static str, Severity, &'static str, &'static str) {
        use Severity::*;
        match self {
            Code::Svt001 => ("SVT001", Error, "policy", "add_policy"),
            Code::Svt002 => ("SVT002", Error, "reference", "fix_reference"),
            Code::Svt003 => ("SVT003", Error, "expression", "fix_expression"),
            Code::Svt004 => ("SVT004", Warning, "policy", "add_write_path"),
            Code::Svt005 => ("SVT005", Error, "money", "use_money"),
            Code::Svt010 => ("SVT010", Info, "index", "none"),
            Code::Svt019 => ("SVT019", Error, "syntax", "fix_syntax"),
            Code::Svt020 => ("SVT020", Error, "syntax", "remove_key"),
            Code::Svt021 => ("SVT021", Error, "naming", "rename"),
            Code::Svt022 => ("SVT022", Error, "syntax", "set_primary"),
            Code::Svt023 => ("SVT023", Error, "type", "fix_type"),
            Code::Svt024 => ("SVT024", Error, "naming", "rename"),
            Code::Svt025 => ("SVT025", Error, "version", "set_version"),
            Code::Svt026 => ("SVT026", Error, "reference", "fix_field"),
            Code::Svt027 => ("SVT027", Warning, "policy", "remove_policy"),
            Code::Svt028 => ("SVT028", Error, "syntax", "add_key"),
            Code::Svt029 => ("SVT029", Error, "type", "fix_value"),
        }
    }

    pub fn id(self) -> &'static str {
        self.row().0
    }
    pub fn severity(self) -> Severity {
        self.row().1
    }
    pub fn category(self) -> &'static str {
        self.row().2
    }
    pub fn action(self) -> &'static str {
        self.row().3
    }
}

/// One finding. `path` is a dotted location such as `entities.Account.fields.balance`.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub code: Code,
    pub path: String,
    pub line: usize,
    pub col: usize,
    pub message: String,
    pub hint: String,
}

impl Diagnostic {
    pub fn severity(&self) -> Severity {
        self.code.severity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_their_variant_names() {
        for (code, n) in [(Code::Svt001, 1), (Code::Svt010, 10), (Code::Svt029, 29)] {
            assert_eq!(code.id(), format!("SVT{n:03}"));
        }
    }
}
