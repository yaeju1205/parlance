#[derive(Debug, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn error(self, message: impl Into<String>) -> Diagnostics {
        Diagnostics {
            severity: Severity::Error,
            span: self,
            message: message.into(),
        }
    }
}

impl ToString for Span {
    fn to_string(&self) -> String {
        format!("{}:{}", self.start, self.end)
    }
}

#[derive(Debug)]
pub enum Severity {
    Error,
}

#[derive(Debug)]
pub struct Diagnostics {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
}

impl ToString for Diagnostics {
    fn to_string(&self) -> String {
        format!(
            "Parlance::{:?}\n> {} ({})",
            self.severity,
            self.message,
            self.span.to_string()
        )
    }
}
