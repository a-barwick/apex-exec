use crate::span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    VariableDeclaration {
        ty: TypeName,
        name: Identifier,
        initializer: Expression,
        span: Span,
    },
    Assignment {
        name: Identifier,
        value: Expression,
        span: Span,
    },
    Debug {
        variable: Identifier,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expression {
    StringLiteral(String, Span),
    BooleanLiteral(bool, Span),
    IntegerLiteral(i64, Span),
    Variable(Identifier),
}

impl Expression {
    pub fn span(&self) -> Span {
        match self {
            Self::StringLiteral(_, span)
            | Self::BooleanLiteral(_, span)
            | Self::IntegerLiteral(_, span) => *span,
            Self::Variable(identifier) => identifier.span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identifier {
    pub spelling: String,
    pub canonical: String,
    pub span: Span,
}

impl Identifier {
    pub fn new(spelling: String, span: Span) -> Self {
        let canonical = spelling.to_ascii_lowercase();
        Self {
            spelling,
            canonical,
            span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeName {
    String,
    Boolean,
    Integer,
}

impl TypeName {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "string" => Some(Self::String),
            "boolean" => Some(Self::Boolean),
            "integer" => Some(Self::Integer),
            _ => None,
        }
    }

    pub const fn apex_name(self) -> &'static str {
        match self {
            Self::String => "String",
            Self::Boolean => "Boolean",
            Self::Integer => "Integer",
        }
    }
}

impl Statement {
    pub fn span(&self) -> Span {
        match self {
            Self::VariableDeclaration { span, .. }
            | Self::Assignment { span, .. }
            | Self::Debug { span, .. } => *span,
        }
    }
}
