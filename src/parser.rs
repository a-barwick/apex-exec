use crate::{
    ast::{Identifier, Program},
    diagnostic::Diagnostic,
    token::{Token, TokenKind},
};

mod declarations;
mod expressions;
mod queries;
mod statements;
mod types;

pub struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, cursor: 0 }
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

#[cfg(test)]
mod tests;
