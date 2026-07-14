use crate::span::Span;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    pub fn render(&self, file_name: &str, source: &str) -> String {
        let start = self.span.start.min(source.len());
        let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
        let line_end = source[start..]
            .find('\n')
            .map_or(source.len(), |index| start + index);
        let line = source[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let column = source[line_start..start].chars().count() + 1;
        let width = source[start..self.span.end.min(line_end)]
            .chars()
            .count()
            .max(1);
        let source_line = &source[line_start..line_end];

        format!(
            "error: {}\n --> {}:{}:{}\n  |\n{} | {}\n  | {}{}",
            self.message,
            file_name,
            line,
            column,
            line,
            source_line,
            " ".repeat(column - 1),
            "^".repeat(width),
        )
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Diagnostic {}
