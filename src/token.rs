use crate::span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Identifier(String),
    StringLiteral(String),
    IntegerLiteral(i64),
    BooleanLiteral(bool),
    Dot,
    Equal,
    LeftParen,
    RightParen,
    Semicolon,
    Eof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub lexeme: String,
}

impl TokenKind {
    pub fn description(&self) -> &'static str {
        match self {
            Self::Identifier(_) => "identifier",
            Self::StringLiteral(_) => "string literal",
            Self::IntegerLiteral(_) => "integer literal",
            Self::BooleanLiteral(_) => "boolean literal",
            Self::Dot => "`.`",
            Self::Equal => "`=`",
            Self::LeftParen => "`(`",
            Self::RightParen => "`)`",
            Self::Semicolon => "`;`",
            Self::Eof => "end of file",
        }
    }
}
