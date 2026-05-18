#[derive(Debug, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
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

pub struct Diagnostics {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
}

impl ToString for Diagnostics {
    fn to_string(&self) -> String {
        format!(
            "{:?}: {} ({})",
            self.severity,
            self.message,
            self.span.to_string()
        )
    }
}
