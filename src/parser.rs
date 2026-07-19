use crate::{
    ast::{Identifier, Program},
    diagnostic::Diagnostic,
    span::Span,
    token::{Token, TokenKind},
};
use std::fmt;

mod declarations;
mod expressions;
mod queries;
mod statements;
mod types;

#[derive(Clone, Debug)]
pub struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    pending_types: Vec<crate::ast::ClassDeclaration>,
}

/// Stable categories for malformed raw token streams supplied to [`Parser`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenStreamErrorKind {
    Empty,
    MissingEof,
    InteriorEof,
    MixedSource,
    ReversedSpan,
    NonMonotonicSpan,
    OverlappingSpan,
}

/// Structural failure found before parser lookahead reads a raw token stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenStreamError {
    /// Machine-readable invariant that failed.
    pub kind: TokenStreamErrorKind,
    /// Real source span that exposed the failure, or `None` for an empty stream.
    pub offending_span: Option<Span>,
}

impl TokenStreamError {
    fn new(kind: TokenStreamErrorKind, offending_span: Option<Span>) -> Self {
        Self {
            kind,
            offending_span,
        }
    }

    pub fn message(&self) -> &'static str {
        match self.kind {
            TokenStreamErrorKind::Empty => "parser token stream is empty",
            TokenStreamErrorKind::MissingEof => {
                "parser token stream must end with exactly one EOF token"
            }
            TokenStreamErrorKind::InteriorEof => {
                "parser token stream contains an EOF token before its end"
            }
            TokenStreamErrorKind::MixedSource => {
                "parser token stream contains spans from multiple sources"
            }
            TokenStreamErrorKind::ReversedSpan => {
                "parser token stream contains a span whose start exceeds its end"
            }
            TokenStreamErrorKind::NonMonotonicSpan => {
                "parser token stream spans are not in source order"
            }
            TokenStreamErrorKind::OverlappingSpan => {
                "parser token stream contains overlapping spans"
            }
        }
    }
}

impl fmt::Display for TokenStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for TokenStreamError {}

impl Parser {
    /// Validates and accepts one ordered, single-source, terminal-EOF token stream.
    pub fn new(tokens: Vec<Token>) -> Result<Self, TokenStreamError> {
        validate_token_stream(&tokens)?;
        Ok(Self {
            tokens,
            cursor: 0,
            pending_types: Vec::new(),
        })
    }

    pub fn parse_program(mut self) -> Result<Program, Diagnostic> {
        let mut classes = Vec::new();
        let mut triggers = Vec::new();
        let mut methods = Vec::new();
        let mut statements = Vec::new();
        while !self.check(&TokenKind::Eof) {
            if self.check_keyword("trigger") {
                triggers.push(self.parse_trigger_declaration()?);
            } else if self.is_class_declaration_start() {
                classes.push(self.parse_class_declaration()?);
                classes.append(&mut self.pending_types);
            } else if self.is_method_declaration_start() {
                methods.push(self.parse_method_declaration()?);
            } else {
                statements.push(self.parse_statement()?);
            }
        }
        Ok(Program {
            classes,
            triggers,
            methods,
            statements,
        })
    }

    pub(crate) fn parse_soql_query(mut self) -> Result<crate::ast::SoqlQuery, Diagnostic> {
        let query = self.parse_soql_body()?;
        if !self.check(&TokenKind::Eof) {
            return Err(Diagnostic::new(
                "unexpected token after dynamic SOQL query",
                self.current().span,
            ));
        }
        Ok(query)
    }

    fn expect_identifier(&mut self, message: &str) -> Result<Identifier, Diagnostic> {
        let token = self.current().clone();
        if let TokenKind::Identifier(spelling) = token.kind {
            self.advance();
            Ok(Identifier::new(spelling, token.span))
        } else {
            Err(Diagnostic::new(message, token.span))
        }
    }

    fn expect_simple(&mut self, expected: TokenKind, message: &str) -> Result<Token, Diagnostic> {
        if self.check(&expected) {
            Ok(self.advance())
        } else {
            Err(Diagnostic::new(message, self.current().span))
        }
    }

    fn check(&self, expected: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(expected)
    }

    fn current(&self) -> &Token {
        self.peek(0)
    }

    fn peek(&self, offset: usize) -> &Token {
        self.token_at(self.cursor + offset)
    }

    fn token_at(&self, index: usize) -> &Token {
        &self.tokens[index.min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> Token {
        let token = self.current().clone();
        if self.cursor + 1 < self.tokens.len() {
            self.cursor += 1;
        }
        token
    }

    fn check_keyword(&self, expected: &str) -> bool {
        matches!(
            &self.current().kind,
            TokenKind::Identifier(spelling) if spelling.eq_ignore_ascii_case(expected)
        )
    }

    fn expect_keyword(&mut self, expected: &str, message: &str) -> Result<Token, Diagnostic> {
        if self.check_keyword(expected) {
            Ok(self.advance())
        } else {
            Err(Diagnostic::new(message, self.current().span))
        }
    }
}

fn validate_token_stream(tokens: &[Token]) -> Result<(), TokenStreamError> {
    let Some(first) = tokens.first() else {
        return Err(TokenStreamError::new(TokenStreamErrorKind::Empty, None));
    };

    let source_id = first.span.source_id;
    let mut previous: Option<Span> = None;
    for token in tokens {
        if token.span.source_id != source_id {
            return Err(TokenStreamError::new(
                TokenStreamErrorKind::MixedSource,
                Some(token.span),
            ));
        }
        if token.span.start > token.span.end {
            return Err(TokenStreamError::new(
                TokenStreamErrorKind::ReversedSpan,
                Some(token.span),
            ));
        }
        if let Some(previous_span) = previous {
            if token.span.start < previous_span.start {
                return Err(TokenStreamError::new(
                    TokenStreamErrorKind::NonMonotonicSpan,
                    Some(token.span),
                ));
            }
            if token.span.start < previous_span.end {
                return Err(TokenStreamError::new(
                    TokenStreamErrorKind::OverlappingSpan,
                    Some(token.span),
                ));
            }
        }
        previous = Some(token.span);
    }

    let Some(last) = tokens.last() else {
        return Err(TokenStreamError::new(TokenStreamErrorKind::Empty, None));
    };
    if !matches!(last.kind, TokenKind::Eof) {
        if let Some(interior) = tokens
            .iter()
            .find(|token| matches!(token.kind, TokenKind::Eof))
        {
            return Err(TokenStreamError::new(
                TokenStreamErrorKind::InteriorEof,
                Some(interior.span),
            ));
        }
        return Err(TokenStreamError::new(
            TokenStreamErrorKind::MissingEof,
            Some(last.span),
        ));
    }
    if let Some(interior) = tokens[..tokens.len() - 1]
        .iter()
        .find(|token| matches!(token.kind, TokenKind::Eof))
    {
        return Err(TokenStreamError::new(
            TokenStreamErrorKind::InteriorEof,
            Some(interior.span),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests;
