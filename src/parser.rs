use crate::{
    ast::{Expression, Identifier, Program, Statement, TypeName},
    diagnostic::Diagnostic,
    token::{Token, TokenKind},
};

pub struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, cursor: 0 }
    }

    pub fn parse_program(mut self) -> Result<Program, Diagnostic> {
        let mut statements = Vec::new();
        while !matches!(self.current().kind, TokenKind::Eof) {
            statements.push(self.parse_statement()?);
        }
        Ok(Program { statements })
    }

    fn parse_statement(&mut self) -> Result<Statement, Diagnostic> {
        let first = self.expect_identifier("expected a statement")?;
        if TypeName::from_apex_name(&first.spelling).is_some() {
            self.parse_variable_declaration(first)
        } else if first.canonical == "system" {
            self.parse_debug(first)
        } else if matches!(self.current().kind, TokenKind::Equal) {
            self.parse_assignment(first)
        } else {
            Err(Diagnostic::new(
                format!(
                    "unsupported or invalid statement starting with `{}`",
                    first.spelling
                ),
                first.span,
            ))
        }
    }

    fn parse_variable_declaration(
        &mut self,
        type_identifier: Identifier,
    ) -> Result<Statement, Diagnostic> {
        let ty = TypeName::from_apex_name(&type_identifier.spelling).expect("type checked above");
        let name = self.expect_identifier("expected a variable name")?;
        self.expect_simple(TokenKind::Equal, "expected `=` and an explicit initializer")?;
        let initializer = self.parse_expression()?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after declaration")?;
        Ok(Statement::VariableDeclaration {
            ty,
            name,
            initializer,
            span: type_identifier.span.merge(end.span),
        })
    }

    fn parse_assignment(&mut self, name: Identifier) -> Result<Statement, Diagnostic> {
        self.advance();
        let value = self.parse_expression()?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after assignment")?;
        Ok(Statement::Assignment {
            span: name.span.merge(end.span),
            name,
            value,
        })
    }

    fn parse_debug(&mut self, system: Identifier) -> Result<Statement, Diagnostic> {
        self.expect_simple(TokenKind::Dot, "expected `.` after `System`")?;
        let method = self.expect_identifier("expected `debug` after `System.`")?;
        if method.canonical != "debug" {
            return Err(Diagnostic::new(
                format!("unsupported System method `{}`", method.spelling),
                method.span,
            ));
        }
        self.expect_simple(TokenKind::LeftParen, "expected `(` after `System.debug`")?;
        let variable = self.expect_identifier("System.debug currently requires a variable")?;
        self.expect_simple(TokenKind::RightParen, "expected `)` after debug variable")?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after System.debug")?;
        Ok(Statement::Debug {
            variable,
            span: system.span.merge(end.span),
        })
    }

    fn parse_expression(&mut self) -> Result<Expression, Diagnostic> {
        let token = self.current().clone();
        let expression = match token.kind {
            TokenKind::StringLiteral(value) => Expression::StringLiteral(value, token.span),
            TokenKind::BooleanLiteral(value) => Expression::BooleanLiteral(value, token.span),
            TokenKind::IntegerLiteral(value) => Expression::IntegerLiteral(value, token.span),
            TokenKind::Identifier(spelling) => {
                Expression::Variable(Identifier::new(spelling, token.span))
            }
            _ => {
                return Err(Diagnostic::new(
                    "expected a primitive literal or variable",
                    token.span,
                ));
            }
        };
        self.advance();
        Ok(expression)
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
        let token = self.current().clone();
        if std::mem::discriminant(&token.kind) == std::mem::discriminant(&expected) {
            self.advance();
            Ok(token)
        } else {
            Err(Diagnostic::new(message, token.span))
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn advance(&mut self) {
        if self.cursor + 1 < self.tokens.len() {
            self.cursor += 1;
        }
    }
}
