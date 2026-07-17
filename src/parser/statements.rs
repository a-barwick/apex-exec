use super::Parser;
use crate::{
    ast::{CatchClause, DmlOperation, Statement},
    diagnostic::Diagnostic,
    token::TokenKind,
};

impl Parser {
    pub(super) fn parse_statement(&mut self) -> Result<Statement, Diagnostic> {
        match self.current().kind {
            TokenKind::LeftBrace => self.parse_block(),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::Do => self.parse_do_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Break => self.parse_break(),
            TokenKind::Continue => self.parse_continue(),
            TokenKind::Try => self.parse_try(),
            TokenKind::Throw => self.parse_throw(),
            TokenKind::Return => self.parse_return(),
            _ if self.is_dml_start() => self.parse_dml(),
            _ if self.is_declaration_start() => self.parse_variable_declaration(true),
            _ => self.parse_expression_statement(true),
        }
    }

    fn is_dml_start(&self) -> bool {
        matches!(
            &self.current().kind,
            TokenKind::Identifier(spelling)
                if ["insert", "update", "upsert", "delete", "undelete"]
                    .iter()
                    .any(|keyword| spelling.eq_ignore_ascii_case(keyword))
        )
    }

    fn parse_dml(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.advance();
        let operation = match start.lexeme.to_ascii_lowercase().as_str() {
            "insert" => DmlOperation::Insert,
            "update" => DmlOperation::Update,
            "upsert" => DmlOperation::Upsert,
            "delete" => DmlOperation::Delete,
            "undelete" => DmlOperation::Undelete,
            _ => unreachable!("DML start was checked"),
        };
        let value = self.parse_expression()?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after DML statement")?;
        Ok(Statement::Dml {
            operation,
            value,
            span: start.span.merge(end.span),
        })
    }

    pub(super) fn parse_variable_declaration(
        &mut self,
        consume_semicolon: bool,
    ) -> Result<Statement, Diagnostic> {
        let (ty, type_span) = self.parse_type_name()?;
        let name = self.expect_identifier("expected a variable name")?;
        self.expect_simple(TokenKind::Equal, "expected `=` and an explicit initializer")?;
        let initializer = self.parse_expression()?;
        let end = if consume_semicolon {
            self.expect_simple(TokenKind::Semicolon, "expected `;` after declaration")?
                .span
        } else {
            initializer.span()
        };
        Ok(Statement::VariableDeclaration {
            ty,
            name,
            initializer,
            span: type_span.merge(end),
        })
    }

    pub(super) fn parse_expression_statement(
        &mut self,
        consume_semicolon: bool,
    ) -> Result<Statement, Diagnostic> {
        let expression = self.parse_expression()?;
        let start = expression.span();
        let end = if consume_semicolon {
            self.expect_simple(TokenKind::Semicolon, "expected `;` after expression")?
                .span
        } else {
            start
        };
        Ok(Statement::Expression {
            expression,
            span: start.merge(end),
        })
    }

    pub(super) fn parse_block(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::LeftBrace, "expected `{`")?;
        let mut statements = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            statements.push(self.parse_statement()?);
        }
        let end = self.expect_simple(TokenKind::RightBrace, "expected `}` after block")?;
        Ok(Statement::Block {
            statements,
            span: start.span.merge(end.span),
        })
    }

    pub(super) fn parse_if(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::If, "expected `if`")?;
        self.expect_simple(TokenKind::LeftParen, "expected `(` after `if`")?;
        let condition = self.parse_expression()?;
        self.expect_simple(TokenKind::RightParen, "expected `)` after if condition")?;
        let then_branch = Box::new(self.parse_statement()?);
        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };
        let end = else_branch
            .as_deref()
            .unwrap_or(then_branch.as_ref())
            .span();
        Ok(Statement::If {
            condition,
            then_branch,
            else_branch,
            span: start.span.merge(end),
        })
    }

    pub(super) fn parse_while(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::While, "expected `while`")?;
        self.expect_simple(TokenKind::LeftParen, "expected `(` after `while`")?;
        let condition = self.parse_expression()?;
        self.expect_simple(TokenKind::RightParen, "expected `)` after while condition")?;
        let body = Box::new(self.parse_statement()?);
        let span = start.span.merge(body.span());
        Ok(Statement::While {
            condition,
            body,
            span,
        })
    }

    pub(super) fn parse_do_while(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::Do, "expected `do`")?;
        let body = Box::new(self.parse_statement()?);
        self.expect_simple(TokenKind::While, "expected `while` after do body")?;
        self.expect_simple(TokenKind::LeftParen, "expected `(` after `while`")?;
        let condition = self.parse_expression()?;
        self.expect_simple(
            TokenKind::RightParen,
            "expected `)` after do-while condition",
        )?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after do-while")?;
        Ok(Statement::DoWhile {
            body,
            condition,
            span: start.span.merge(end.span),
        })
    }

    pub(super) fn parse_for(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::For, "expected `for`")?;
        self.expect_simple(TokenKind::LeftParen, "expected `(` after `for`")?;

        if self.is_for_each_start() {
            let (element_type, _) = self.parse_type_name()?;
            let name = self.expect_identifier("expected an iteration variable")?;
            self.expect_simple(TokenKind::Colon, "expected `:` after enhanced for variable")?;
            let iterable = self.parse_expression()?;
            self.expect_simple(
                TokenKind::RightParen,
                "expected `)` after enhanced for iterable",
            )?;
            let body = Box::new(self.parse_statement()?);
            let span = start.span.merge(body.span());
            return Ok(Statement::ForEach {
                element_type,
                name,
                iterable,
                body,
                span,
            });
        }

        let initializer = if self.check(&TokenKind::Semicolon) {
            self.advance();
            None
        } else {
            let statement = if self.is_declaration_start() {
                self.parse_variable_declaration(false)?
            } else {
                self.parse_expression_statement(false)?
            };
            self.expect_simple(TokenKind::Semicolon, "expected `;` after for initializer")?;
            Some(Box::new(statement))
        };

        let condition = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect_simple(TokenKind::Semicolon, "expected `;` after for condition")?;

        let update = if self.check(&TokenKind::RightParen) {
            None
        } else {
            Some(Box::new(self.parse_expression_statement(false)?))
        };
        self.expect_simple(TokenKind::RightParen, "expected `)` after for clauses")?;
        let body = Box::new(self.parse_statement()?);
        let span = start.span.merge(body.span());
        Ok(Statement::For {
            initializer,
            condition,
            update,
            body,
            span,
        })
    }

    pub(super) fn parse_break(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::Break, "expected `break`")?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after `break`")?;
        Ok(Statement::Break {
            span: start.span.merge(end.span),
        })
    }

    pub(super) fn parse_continue(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::Continue, "expected `continue`")?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after `continue`")?;
        Ok(Statement::Continue {
            span: start.span.merge(end.span),
        })
    }

    pub(super) fn parse_try(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::Try, "expected `try`")?;
        let try_block = Box::new(self.parse_block()?);
        let mut catches = Vec::new();

        while self.check(&TokenKind::Catch) {
            let catch_start = self.advance();
            self.expect_simple(TokenKind::LeftParen, "expected `(` after `catch`")?;
            let (exception_type, _) = self.parse_type_name()?;
            let name = self.expect_identifier("expected a catch variable name")?;
            self.expect_simple(TokenKind::RightParen, "expected `)` after catch variable")?;
            let body = self.parse_block()?;
            let span = catch_start.span.merge(body.span());
            catches.push(CatchClause {
                exception_type,
                name,
                body,
                span,
            });
        }

        let finally_block = if self.check(&TokenKind::Finally) {
            self.advance();
            Some(Box::new(self.parse_block()?))
        } else {
            None
        };
        if catches.is_empty() && finally_block.is_none() {
            return Err(Diagnostic::new(
                "expected at least one `catch` or a `finally` after try block",
                self.current().span,
            ));
        }

        let end = finally_block.as_deref().map_or_else(
            || catches.last().expect("catch is present").body.span(),
            Statement::span,
        );
        Ok(Statement::Try {
            try_block,
            catches,
            finally_block,
            span: start.span.merge(end),
        })
    }

    pub(super) fn parse_throw(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::Throw, "expected `throw`")?;
        let value = self.parse_expression()?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after `throw`")?;
        Ok(Statement::Throw {
            value,
            span: start.span.merge(end.span),
        })
    }

    pub(super) fn parse_return(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::Return, "expected `return`")?;
        let value = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after `return`")?;
        Ok(Statement::Return {
            value,
            span: start.span.merge(end.span),
        })
    }
}
