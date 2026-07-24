use crate::span::Span;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackFrame {
    pub method: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExceptionKind {
    NullPointer,
    List,
    Math,
    Type,
    String,
    IllegalArgument,
    Final,
    Assert,
    Query,
    Dml,
    SObject,
    Async,
    Callout,
    UserDefined(String),
}

impl ExceptionKind {
    pub fn from_apex_name(name: impl Into<String>) -> Self {
        let name = name.into();
        match name.to_ascii_lowercase().as_str() {
            "nullpointerexception" => Self::NullPointer,
            "listexception" => Self::List,
            "mathexception" => Self::Math,
            "typeexception" => Self::Type,
            "stringexception" => Self::String,
            "illegalargumentexception" => Self::IllegalArgument,
            "finalexception" => Self::Final,
            "assertexception" => Self::Assert,
            "queryexception" => Self::Query,
            "dmlexception" => Self::Dml,
            "sobjectexception" => Self::SObject,
            "asyncexception" => Self::Async,
            "calloutexception" => Self::Callout,
            _ => Self::UserDefined(name),
        }
    }

    pub fn apex_name(&self) -> &str {
        match self {
            Self::NullPointer => "NullPointerException",
            Self::List => "ListException",
            Self::Math => "MathException",
            Self::Type => "TypeException",
            Self::String => "StringException",
            Self::IllegalArgument => "IllegalArgumentException",
            Self::Final => "FinalException",
            Self::Assert => "AssertException",
            Self::Query => "QueryException",
            Self::Dml => "DmlException",
            Self::SObject => "SObjectException",
            Self::Async => "AsyncException",
            Self::Callout => "CalloutException",
            Self::UserDefined(name) => name,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    pub exception_type: Option<String>,
    pub exception_kind: Option<ExceptionKind>,
    pub stack_trace: Vec<StackFrame>,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            exception_type: None,
            exception_kind: None,
            stack_trace: Vec::new(),
        }
    }

    pub fn runtime_exception(
        exception_type: impl Into<String>,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        let exception_kind = ExceptionKind::from_apex_name(exception_type);
        Self {
            message: message.into(),
            span,
            exception_type: Some(exception_kind.apex_name().to_owned()),
            exception_kind: Some(exception_kind),
            stack_trace: Vec::new(),
        }
    }

    pub fn push_frame(&mut self, method: impl Into<String>, span: Span) {
        if self.exception_type.is_some() {
            self.stack_trace.push(StackFrame {
                method: method.into(),
                span,
            });
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

        let heading = self.exception_type.as_ref().map_or_else(
            || self.message.clone(),
            |exception_type| format!("{exception_type}: {}", self.message),
        );
        let mut rendered = format!(
            "error: {}\n --> {}:{}:{}\n  |\n{} | {}\n  | {}{}",
            heading,
            file_name,
            line,
            column,
            line,
            source_line,
            " ".repeat(column - 1),
            "^".repeat(width),
        );

        if !self.stack_trace.is_empty() {
            rendered.push_str("\nApex stack trace:");
            for frame in &self.stack_trace {
                let (line, column) = source_location(source, frame.span.start);
                rendered.push_str(&format!(
                    "\n  at {} ({}:{}:{})",
                    frame.method, file_name, line, column
                ));
            }
        }

        rendered
    }
}

fn source_location(source: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line = source[..offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let column = source[line_start..offset].chars().count() + 1;
    (line, column)
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Diagnostic {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_the_correct_line_column_and_highlight() {
        let source = "String message = 'ok';\nSystem.debug(mesage);";
        let start = source.find("mesage").unwrap();
        let diagnostic = Diagnostic::new("unknown variable `mesage`", Span::new(start, start + 6));
        let rendered = diagnostic.render("script.apex", source);

        assert!(rendered.contains(" --> script.apex:2:14"));
        assert!(rendered.contains("2 | System.debug(mesage);"));
        assert!(rendered.ends_with("^^^^^^"));
    }

    #[test]
    fn reports_character_columns_when_prior_text_is_multibyte() {
        let source = "String é = value;";
        let start = source.find("value").unwrap();
        let diagnostic = Diagnostic::new("example", Span::new(start, start + 5));

        assert!(diagnostic.render("unicode.apex", source).contains(":1:12"));
    }

    #[test]
    fn renders_runtime_exception_types_and_source_mapped_frames() {
        let source = "Integer fail() { return 1 / 0; }\nInteger value = fail();";
        let origin = source.find('/').unwrap();
        let call = source.rfind("fail").unwrap();
        let mut diagnostic = Diagnostic::runtime_exception(
            "MathException",
            "division by zero",
            Span::new(origin, origin + 1),
        );
        diagnostic.push_frame("fail", Span::new(call, call + 4));

        let rendered = diagnostic.render("script.apex", source);
        assert!(rendered.contains("error: MathException: division by zero"));
        assert!(rendered.contains("Apex stack trace:"));
        assert!(rendered.contains("at fail (script.apex:2:17)"));
    }
}
