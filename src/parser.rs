use crate::{
    ast::{
        BinaryOperator, Expression, Identifier, PostfixOperator, Program, Statement, TypeName,
        UnaryOperator,
    },
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
        while !self.check(&TokenKind::Eof) {
            statements.push(self.parse_statement()?);
        }
        Ok(Program { statements })
    }

    fn parse_statement(&mut self) -> Result<Statement, Diagnostic> {
        match self.current().kind {
            TokenKind::LeftBrace => self.parse_block(),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::Do => self.parse_do_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Break => self.parse_break(),
            TokenKind::Continue => self.parse_continue(),
            TokenKind::Return => self.parse_return(),
            _ if self.is_declaration_start() => self.parse_variable_declaration(true),
            _ if self.is_debug_start() => self.parse_debug(),
            _ => self.parse_expression_statement(true),
        }
    }

    fn parse_variable_declaration(
        &mut self,
        consume_semicolon: bool,
    ) -> Result<Statement, Diagnostic> {
        let type_identifier = self.expect_identifier("expected a primitive type")?;
        let ty = TypeName::from_apex_name(&type_identifier.spelling)
            .expect("declaration start checked before parsing");
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
            span: type_identifier.span.merge(end),
        })
    }

    fn parse_expression_statement(
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

    fn parse_debug(&mut self) -> Result<Statement, Diagnostic> {
        let system = self.expect_identifier("expected `System`")?;
        self.expect_simple(TokenKind::Dot, "expected `.` after `System`")?;
        let method = self.expect_identifier("expected `debug` after `System.`")?;
        if method.canonical != "debug" {
            return Err(Diagnostic::new(
                format!("unsupported System method `{}`", method.spelling),
                method.span,
            ));
        }
        self.expect_simple(TokenKind::LeftParen, "expected `(` after `System.debug`")?;
        let expression = self.parse_expression()?;
        self.expect_simple(TokenKind::RightParen, "expected `)` after debug expression")?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after System.debug")?;
        Ok(Statement::Debug {
            expression,
            span: system.span.merge(end.span),
        })
    }

    fn parse_block(&mut self) -> Result<Statement, Diagnostic> {
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

    fn parse_if(&mut self) -> Result<Statement, Diagnostic> {
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

    fn parse_while(&mut self) -> Result<Statement, Diagnostic> {
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

    fn parse_do_while(&mut self) -> Result<Statement, Diagnostic> {
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

    fn parse_for(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::For, "expected `for`")?;
        self.expect_simple(TokenKind::LeftParen, "expected `(` after `for`")?;

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

    fn parse_break(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::Break, "expected `break`")?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after `break`")?;
        Ok(Statement::Break {
            span: start.span.merge(end.span),
        })
    }

    fn parse_continue(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::Continue, "expected `continue`")?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after `continue`")?;
        Ok(Statement::Continue {
            span: start.span.merge(end.span),
        })
    }

    fn parse_return(&mut self) -> Result<Statement, Diagnostic> {
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

    fn parse_expression(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expression, Diagnostic> {
        let expression = self.parse_or()?;
        if !self.check(&TokenKind::Equal) {
            return Ok(expression);
        }

        let equals = self.advance();
        let value = self.parse_assignment()?;
        if let Expression::Variable(target) = expression {
            let span = target.span.merge(value.span());
            Ok(Expression::Assignment {
                target,
                value: Box::new(value),
                span,
            })
        } else {
            Err(Diagnostic::new("invalid assignment target", equals.span))
        }
    }

    fn parse_or(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(Self::parse_and, &[(TokenKind::OrOr, BinaryOperator::Or)])
    }

    fn parse_and(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_equality,
            &[(TokenKind::AndAnd, BinaryOperator::And)],
        )
    }

    fn parse_equality(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_comparison,
            &[
                (TokenKind::EqualEqual, BinaryOperator::Equal),
                (TokenKind::BangEqual, BinaryOperator::NotEqual),
            ],
        )
    }

    fn parse_comparison(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_term,
            &[
                (TokenKind::Less, BinaryOperator::Less),
                (TokenKind::LessEqual, BinaryOperator::LessEqual),
                (TokenKind::Greater, BinaryOperator::Greater),
                (TokenKind::GreaterEqual, BinaryOperator::GreaterEqual),
            ],
        )
    }

    fn parse_term(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_factor,
            &[
                (TokenKind::Plus, BinaryOperator::Add),
                (TokenKind::Minus, BinaryOperator::Subtract),
            ],
        )
    }

    fn parse_factor(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_unary,
            &[
                (TokenKind::Star, BinaryOperator::Multiply),
                (TokenKind::Slash, BinaryOperator::Divide),
                (TokenKind::Percent, BinaryOperator::Remainder),
            ],
        )
    }

    fn parse_binary_level(
        &mut self,
        next: fn(&mut Self) -> Result<Expression, Diagnostic>,
        operators: &[(TokenKind, BinaryOperator)],
    ) -> Result<Expression, Diagnostic> {
        let mut expression = next(self)?;
        loop {
            let Some(operator) = operators
                .iter()
                .find(|(kind, _)| self.check(kind))
                .map(|(_, operator)| *operator)
            else {
                break;
            };
            let operator_token = self.advance();
            let right = next(self)?;
            let span = expression.span().merge(right.span());
            expression = Expression::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
                operator_span: operator_token.span,
                span,
            };
        }
        Ok(expression)
    }

    fn parse_unary(&mut self) -> Result<Expression, Diagnostic> {
        let operator = match self.current().kind {
            TokenKind::Plus => Some(UnaryOperator::Positive),
            TokenKind::Minus => Some(UnaryOperator::Negate),
            TokenKind::Bang => Some(UnaryOperator::Not),
            TokenKind::PlusPlus => Some(UnaryOperator::PrefixIncrement),
            TokenKind::MinusMinus => Some(UnaryOperator::PrefixDecrement),
            _ => None,
        };
        if let Some(operator) = operator {
            let token = self.advance();
            let operand = self.parse_unary()?;
            let span = token.span.merge(operand.span());
            return Ok(Expression::Unary {
                operator,
                operand: Box::new(operand),
                operator_span: token.span,
                span,
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expression, Diagnostic> {
        let mut expression = self.parse_primary()?;
        loop {
            let operator = match self.current().kind {
                TokenKind::PlusPlus => PostfixOperator::Increment,
                TokenKind::MinusMinus => PostfixOperator::Decrement,
                _ => break,
            };
            let token = self.advance();
            let span = expression.span().merge(token.span);
            expression = Expression::Postfix {
                operand: Box::new(expression),
                operator,
                operator_span: token.span,
                span,
            };
        }
        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expression, Diagnostic> {
        let token = self.current().clone();
        let expression = match token.kind {
            TokenKind::StringLiteral(value) => Expression::StringLiteral(value, token.span),
            TokenKind::BooleanLiteral(value) => Expression::BooleanLiteral(value, token.span),
            TokenKind::IntegerLiteral(value) => Expression::IntegerLiteral(value, token.span),
            TokenKind::Null => Expression::NullLiteral(token.span),
            TokenKind::Identifier(spelling) => {
                Expression::Variable(Identifier::new(spelling, token.span))
            }
            TokenKind::LeftParen => {
                self.advance();
                let expression = self.parse_expression()?;
                self.expect_simple(TokenKind::RightParen, "expected `)` after expression")?;
                return Ok(expression);
            }
            _ => return Err(Diagnostic::new("expected an expression", token.span)),
        };
        self.advance();
        Ok(expression)
    }

    fn is_declaration_start(&self) -> bool {
        matches!(
            &self.current().kind,
            TokenKind::Identifier(spelling) if TypeName::from_apex_name(spelling).is_some()
        ) && matches!(self.peek(1).kind, TokenKind::Identifier(_))
    }

    fn is_debug_start(&self) -> bool {
        matches!(
            &self.current().kind,
            TokenKind::Identifier(spelling) if spelling.eq_ignore_ascii_case("system")
        ) && matches!(self.peek(1).kind, TokenKind::Dot)
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
        &self.tokens[(self.cursor + offset).min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> Token {
        let token = self.current().clone();
        if self.cursor + 1 < self.tokens.len() {
            self.cursor += 1;
        }
        token
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(source: &str) -> Program {
        Parser::new(Lexer::new(source).tokenize().unwrap())
            .parse_program()
            .unwrap()
    }

    #[test]
    fn multiplication_binds_more_tightly_than_addition() {
        let program = parse("Integer result = 1 + 2 * 3;");
        let Statement::VariableDeclaration { initializer, .. } = &program.statements[0] else {
            panic!("expected variable declaration");
        };
        let Expression::Binary {
            operator: BinaryOperator::Add,
            left,
            right,
            ..
        } = initializer
        else {
            panic!("expected addition at the expression root");
        };

        assert!(matches!(left.as_ref(), Expression::IntegerLiteral(1, _)));
        assert!(matches!(
            right.as_ref(),
            Expression::Binary {
                operator: BinaryOperator::Multiply,
                ..
            }
        ));
    }

    #[test]
    fn assignment_parses_right_associatively() {
        let program = parse("Integer left = 0; Integer right = 0; left = right = 7;");
        let Statement::Expression { expression, .. } = &program.statements[2] else {
            panic!("expected assignment statement");
        };
        let Expression::Assignment { target, value, .. } = expression else {
            panic!("expected outer assignment");
        };

        assert_eq!(target.canonical, "left");
        assert!(matches!(
            value.as_ref(),
            Expression::Assignment { target, .. } if target.canonical == "right"
        ));
    }

    #[test]
    fn else_binds_to_the_nearest_if() {
        let program =
            parse("if (true) if (false) System.debug('inner'); else System.debug('else');");
        let Statement::If {
            then_branch,
            else_branch,
            ..
        } = &program.statements[0]
        else {
            panic!("expected outer if");
        };

        assert!(else_branch.is_none());
        assert!(matches!(
            then_branch.as_ref(),
            Statement::If {
                else_branch: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn for_statement_records_all_optional_clauses() {
        let program = parse("for (;;) { break; }");
        let Statement::For {
            initializer,
            condition,
            update,
            body,
            ..
        } = &program.statements[0]
        else {
            panic!("expected for statement");
        };

        assert!(initializer.is_none());
        assert!(condition.is_none());
        assert!(update.is_none());
        assert!(matches!(body.as_ref(), Statement::Block { .. }));
    }
}
