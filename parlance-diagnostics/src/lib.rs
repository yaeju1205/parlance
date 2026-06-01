#[derive(Debug, Default, Clone)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug)]
pub enum Origin {
    Compiler,
    Parser,
}

#[derive(Debug)]
pub enum Severity {
    Error,
}

#[derive(Debug)]
pub struct Diagnostics {
    pub message: String,
    pub severity: Severity,
    pub origin: Origin,
    pub span: Span,
}

impl ToString for Diagnostics {
    fn to_string(&self) -> String {
        format!(
            "Parlance {:?} {:?}\n> {} ({}:{})",
            self.origin, self.severity, self.message, self.span.start, self.span.end
        )
    }
}

impl Diagnostics {
    pub fn parser_error(message: String, span: Span) -> Self {
        Self {
            message,
            span,
            severity: Severity::Error,
            origin: Origin::Parser,
        }
    }

    pub fn compiler_error(message: String) -> Self {
        Self {
            message,
            span: Span::default(),
            severity: Severity::Error,
            origin: Origin::Compiler,
        }
    }
}
