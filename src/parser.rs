use crate::{
    ast::{
        AssignmentTarget, BinaryOperator, CatchClause, CollectionInitializer, Expression,
        Identifier, MapEntry, MethodDeclaration, Parameter, PostfixOperator, Program, ReturnType,
        Statement, TypeName, UnaryOperator,
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
        let mut methods = Vec::new();
        let mut statements = Vec::new();
        while !self.check(&TokenKind::Eof) {
            if self.is_method_declaration_start() {
                methods.push(self.parse_method_declaration()?);
            } else {
                statements.push(self.parse_statement()?);
            }
        }
        Ok(Program {
            methods,
            statements,
        })
    }

    fn parse_method_declaration(&mut self) -> Result<MethodDeclaration, Diagnostic> {
        let (return_type, start) = self.parse_return_type()?;
        let name = self.expect_identifier("expected a method name")?;
        self.expect_simple(TokenKind::LeftParen, "expected `(` after method name")?;

        let mut parameters = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                let (ty, type_span) = self.parse_type_name()?;
                let name = self.expect_identifier("expected a parameter name")?;
                let span = type_span.merge(name.span);
                parameters.push(Parameter { ty, name, span });
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
        }
        self.expect_simple(
            TokenKind::RightParen,
            "expected `)` after method parameters",
        )?;
        let body = self.parse_block()?;
        let span = start.merge(body.span());
        Ok(MethodDeclaration {
            return_type,
            name,
            parameters,
            body,
            span,
        })
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
            TokenKind::Try => self.parse_try(),
            TokenKind::Throw => self.parse_throw(),
            TokenKind::Return => self.parse_return(),
            _ if self.is_declaration_start() => self.parse_variable_declaration(true),
            _ => self.parse_expression_statement(true),
        }
    }

    fn parse_variable_declaration(
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

    fn parse_try(&mut self) -> Result<Statement, Diagnostic> {
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

    fn parse_throw(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect_simple(TokenKind::Throw, "expected `throw`")?;
        let value = self.parse_expression()?;
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after `throw`")?;
        Ok(Statement::Throw {
            value,
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
        let target = match expression {
            Expression::Variable(identifier) => AssignmentTarget::Variable(identifier),
            Expression::Index {
                collection,
                index,
                span,
            } => AssignmentTarget::Index {
                collection,
                index,
                span,
            },
            _ => return Err(Diagnostic::new("invalid assignment target", equals.span)),
        };
        let span = target.span().merge(value.span());
        Ok(Expression::Assignment {
            target,
            value: Box::new(value),
            span,
        })
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
            match self.current().kind {
                TokenKind::LeftBracket => {
                    self.advance();
                    let index = self.parse_expression()?;
                    let end = self.expect_simple(
                        TokenKind::RightBracket,
                        "expected `]` after index expression",
                    )?;
                    let span = expression.span().merge(end.span);
                    expression = Expression::Index {
                        collection: Box::new(expression),
                        index: Box::new(index),
                        span,
                    };
                }
                TokenKind::Dot => {
                    self.advance();
                    let method = self.expect_identifier("expected a method name after `.`")?;
                    if !self.check(&TokenKind::LeftParen) {
                        return Err(Diagnostic::new(
                            "expected `(` after method name",
                            self.current().span,
                        ));
                    }
                    let (arguments, end) = self.parse_argument_list()?;
                    let span = expression.span().merge(end);
                    expression = Expression::MethodCall {
                        receiver: Box::new(expression),
                        method,
                        arguments,
                        span,
                    };
                }
                TokenKind::PlusPlus | TokenKind::MinusMinus => {
                    let operator = if self.check(&TokenKind::PlusPlus) {
                        PostfixOperator::Increment
                    } else {
                        PostfixOperator::Decrement
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
                _ => break,
            }
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
                let name = Identifier::new(spelling, token.span);
                self.advance();
                if self.check(&TokenKind::LeftParen) {
                    let (arguments, end) = self.parse_argument_list()?;
                    return Ok(Expression::FunctionCall {
                        name,
                        arguments,
                        span: token.span.merge(end),
                    });
                }
                return Ok(Expression::Variable(name));
            }
            TokenKind::New => return self.parse_new_expression(),
            TokenKind::LeftParen => {
                if self.is_cast_start() {
                    let start = self.advance();
                    let (ty, _) = self.parse_type_name()?;
                    self.expect_simple(TokenKind::RightParen, "expected `)` after cast type")?;
                    let expression = self.parse_unary()?;
                    let span = start.span.merge(expression.span());
                    return Ok(Expression::Cast {
                        ty,
                        expression: Box::new(expression),
                        span,
                    });
                }
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

    fn parse_new_expression(&mut self) -> Result<Expression, Diagnostic> {
        let start = self.expect_simple(TokenKind::New, "expected `new`")?;
        let (mut ty, _) = self.parse_base_type_name()?;

        if ty.is_exception() {
            if !self.check(&TokenKind::LeftParen) {
                return Err(Diagnostic::new(
                    "expected `(` after exception type",
                    self.current().span,
                ));
            }
            let (arguments, end) = self.parse_argument_list()?;
            return Ok(Expression::NewException {
                exception_type: ty,
                arguments,
                span: start.span.merge(end),
            });
        }

        let (initializer, end) = if self.check(&TokenKind::LeftBracket) {
            self.advance();
            if self.check(&TokenKind::RightBracket) {
                self.advance();
                ty = TypeName::List(Box::new(ty));
                if !self.check(&TokenKind::LeftBrace) {
                    return Err(Diagnostic::new(
                        "expected an array initializer after `[]`",
                        self.current().span,
                    ));
                }
                self.parse_collection_initializer(&ty)?
            } else {
                let size = self.parse_expression()?;
                let end =
                    self.expect_simple(TokenKind::RightBracket, "expected `]` after array size")?;
                ty = TypeName::List(Box::new(ty));
                (CollectionInitializer::SizedArray(Box::new(size)), end.span)
            }
        } else if self.check(&TokenKind::LeftParen) {
            let (arguments, end) = self.parse_argument_list()?;
            (CollectionInitializer::Arguments(arguments), end)
        } else if self.check(&TokenKind::LeftBrace) {
            self.parse_collection_initializer(&ty)?
        } else {
            return Err(Diagnostic::new(
                "expected constructor arguments, collection initializer, or array size",
                self.current().span,
            ));
        };

        Ok(Expression::NewCollection {
            ty,
            initializer,
            span: start.span.merge(end),
        })
    }

    fn parse_collection_initializer(
        &mut self,
        ty: &TypeName,
    ) -> Result<(CollectionInitializer, crate::span::Span), Diagnostic> {
        self.expect_simple(TokenKind::LeftBrace, "expected `{`")?;
        if matches!(ty, TypeName::Map(..)) {
            let mut entries = Vec::new();
            if !self.check(&TokenKind::RightBrace) {
                loop {
                    let key = self.parse_expression()?;
                    self.expect_simple(TokenKind::FatArrow, "expected `=>` after map key")?;
                    let value = self.parse_expression()?;
                    let span = key.span().merge(value.span());
                    entries.push(MapEntry { key, value, span });
                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance();
                }
            }
            let end =
                self.expect_simple(TokenKind::RightBrace, "expected `}` after map initializer")?;
            Ok((CollectionInitializer::MapEntries(entries), end.span))
        } else {
            let mut elements = Vec::new();
            if !self.check(&TokenKind::RightBrace) {
                loop {
                    elements.push(self.parse_expression()?);
                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance();
                }
            }
            let end = self.expect_simple(
                TokenKind::RightBrace,
                "expected `}` after collection initializer",
            )?;
            Ok((CollectionInitializer::Elements(elements), end.span))
        }
    }

    fn parse_argument_list(&mut self) -> Result<(Vec<Expression>, crate::span::Span), Diagnostic> {
        self.expect_simple(TokenKind::LeftParen, "expected `(`")?;
        let mut arguments = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                arguments.push(self.parse_expression()?);
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
        }
        let end = self.expect_simple(TokenKind::RightParen, "expected `)` after arguments")?;
        Ok((arguments, end.span))
    }

    fn parse_type_name(&mut self) -> Result<(TypeName, crate::span::Span), Diagnostic> {
        let (mut ty, mut span) = self.parse_base_type_name()?;
        if self.check(&TokenKind::LeftBracket) {
            self.advance();
            let end = self.expect_simple(TokenKind::RightBracket, "expected `]` in array type")?;
            ty = TypeName::List(Box::new(ty));
            span = span.merge(end.span);
            if self.check(&TokenKind::LeftBracket) {
                return Err(Diagnostic::new(
                    "only one array suffix is supported",
                    self.current().span,
                ));
            }
        }
        Ok((ty, span))
    }

    fn parse_return_type(&mut self) -> Result<(ReturnType, crate::span::Span), Diagnostic> {
        if self.check(&TokenKind::Void) {
            let token = self.advance();
            return Ok((ReturnType::Void, token.span));
        }
        let (ty, span) = self.parse_type_name()?;
        Ok((ReturnType::Value(ty), span))
    }

    fn parse_base_type_name(&mut self) -> Result<(TypeName, crate::span::Span), Diagnostic> {
        let identifier = self.expect_identifier("expected a type name")?;
        match identifier.canonical.as_str() {
            "list" | "set" => {
                self.expect_simple(TokenKind::Less, "expected `<` after collection type name")?;
                let (element, _) = self.parse_type_name()?;
                let end = self.expect_simple(
                    TokenKind::Greater,
                    "expected `>` after collection element type",
                )?;
                let ty = if identifier.canonical == "list" {
                    TypeName::List(Box::new(element))
                } else {
                    TypeName::Set(Box::new(element))
                };
                Ok((ty, identifier.span.merge(end.span)))
            }
            "map" => {
                self.expect_simple(TokenKind::Less, "expected `<` after `Map`")?;
                let (key, _) = self.parse_type_name()?;
                self.expect_simple(TokenKind::Comma, "expected `,` after map key type")?;
                let (value, _) = self.parse_type_name()?;
                let end =
                    self.expect_simple(TokenKind::Greater, "expected `>` after map value type")?;
                Ok((
                    TypeName::Map(Box::new(key), Box::new(value)),
                    identifier.span.merge(end.span),
                ))
            }
            _ if TypeName::from_apex_name(&identifier.canonical).is_some() => Ok((
                TypeName::from_apex_name(&identifier.canonical).expect("type presence was checked"),
                identifier.span,
            )),
            _ => Err(Diagnostic::new(
                format!("unsupported type `{}`", identifier.spelling),
                identifier.span,
            )),
        }
    }

    fn is_declaration_start(&self) -> bool {
        self.type_end_at(self.cursor)
            .is_some_and(|end| matches!(self.token_at(end).kind, TokenKind::Identifier(_)))
    }

    fn is_method_declaration_start(&self) -> bool {
        self.return_type_end_at(self.cursor).is_some_and(|end| {
            matches!(self.token_at(end).kind, TokenKind::Identifier(_))
                && matches!(self.token_at(end + 1).kind, TokenKind::LeftParen)
        })
    }

    fn is_cast_start(&self) -> bool {
        if !self.check(&TokenKind::LeftParen) {
            return false;
        }
        self.type_end_at(self.cursor + 1)
            .is_some_and(|end| matches!(self.token_at(end).kind, TokenKind::RightParen))
    }

    fn is_for_each_start(&self) -> bool {
        self.type_end_at(self.cursor).is_some_and(|end| {
            matches!(self.token_at(end).kind, TokenKind::Identifier(_))
                && matches!(self.token_at(end + 1).kind, TokenKind::Colon)
        })
    }

    fn type_end_at(&self, cursor: usize) -> Option<usize> {
        let TokenKind::Identifier(spelling) = &self.token_at(cursor).kind else {
            return None;
        };
        let canonical = spelling.to_ascii_lowercase();
        let mut end = cursor + 1;
        match canonical.as_str() {
            "string"
            | "boolean"
            | "integer"
            | "object"
            | "exception"
            | "nullpointerexception"
            | "listexception"
            | "mathexception"
            | "typeexception"
            | "stringexception"
            | "illegalargumentexception"
            | "finalexception" => {}
            "list" | "set" => {
                if !matches!(self.token_at(end).kind, TokenKind::Less) {
                    return None;
                }
                end = self.type_end_at(end + 1)?;
                if !matches!(self.token_at(end).kind, TokenKind::Greater) {
                    return None;
                }
                end += 1;
            }
            "map" => {
                if !matches!(self.token_at(end).kind, TokenKind::Less) {
                    return None;
                }
                end = self.type_end_at(end + 1)?;
                if !matches!(self.token_at(end).kind, TokenKind::Comma) {
                    return None;
                }
                end = self.type_end_at(end + 1)?;
                if !matches!(self.token_at(end).kind, TokenKind::Greater) {
                    return None;
                }
                end += 1;
            }
            _ => return None,
        }
        while matches!(self.token_at(end).kind, TokenKind::LeftBracket)
            && matches!(self.token_at(end + 1).kind, TokenKind::RightBracket)
        {
            end += 2;
        }
        Some(end)
    }

    fn return_type_end_at(&self, cursor: usize) -> Option<usize> {
        if matches!(&self.token_at(cursor).kind, TokenKind::Void) {
            Some(cursor + 1)
        } else {
            self.type_end_at(cursor)
        }
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
        let AssignmentTarget::Variable(target) = target else {
            panic!("expected variable assignment target");
        };

        assert_eq!(target.canonical, "left");
        assert!(matches!(
            value.as_ref(),
            Expression::Assignment {
                target: AssignmentTarget::Variable(target),
                ..
            } if target.canonical == "right"
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

    #[test]
    fn parses_nested_generic_types_and_canonicalizes_array_syntax() {
        let program = parse(
            "Map<String, List<Set<Integer>>> grouped = new Map<String, List<Set<Integer>>>(); \
             Integer[] numbers = new Integer[3];",
        );

        let Statement::VariableDeclaration {
            ty, initializer, ..
        } = &program.statements[0]
        else {
            panic!("expected map declaration");
        };
        assert_eq!(
            ty,
            &TypeName::Map(
                Box::new(TypeName::String),
                Box::new(TypeName::List(Box::new(TypeName::Set(Box::new(
                    TypeName::Integer
                )))))
            )
        );
        assert!(matches!(
            initializer,
            Expression::NewCollection {
                initializer: CollectionInitializer::Arguments(arguments),
                ..
            } if arguments.is_empty()
        ));

        let Statement::VariableDeclaration {
            ty, initializer, ..
        } = &program.statements[1]
        else {
            panic!("expected array declaration");
        };
        assert_eq!(ty, &TypeName::List(Box::new(TypeName::Integer)));
        assert!(matches!(
            initializer,
            Expression::NewCollection {
                ty: TypeName::List(element),
                initializer: CollectionInitializer::SizedArray(size),
                ..
            } if element.as_ref() == &TypeName::Integer
                && matches!(size.as_ref(), Expression::IntegerLiteral(3, _))
        ));
    }

    #[test]
    fn parses_element_and_map_collection_initializers() {
        let program = parse(
            "List<String> names = new List<String>{'Ada', 'Grace'}; \
             Map<String, Integer> counts = new Map<String, Integer>{'one' => 1, 'two' => 2}; \
             Set<String> copied = new Set<String>(names); \
             String[] aliases = new String[]{'one', 'two'};",
        );

        let Statement::VariableDeclaration { initializer, .. } = &program.statements[0] else {
            panic!("expected list declaration");
        };
        assert!(matches!(
            initializer,
            Expression::NewCollection {
                initializer: CollectionInitializer::Elements(elements),
                ..
            } if elements.len() == 2
        ));

        let Statement::VariableDeclaration { initializer, .. } = &program.statements[1] else {
            panic!("expected map declaration");
        };
        let Expression::NewCollection {
            initializer: CollectionInitializer::MapEntries(entries),
            ..
        } = initializer
        else {
            panic!("expected map entries");
        };
        assert_eq!(entries.len(), 2);
        assert!(matches!(
            &entries[0].key,
            Expression::StringLiteral(value, _) if value == "one"
        ));
        assert!(matches!(
            &entries[0].value,
            Expression::IntegerLiteral(1, _)
        ));

        let Statement::VariableDeclaration { initializer, .. } = &program.statements[2] else {
            panic!("expected set declaration");
        };
        assert!(matches!(
            initializer,
            Expression::NewCollection {
                initializer: CollectionInitializer::Arguments(arguments),
                ..
            } if matches!(arguments.as_slice(), [Expression::Variable(name)] if name.canonical == "names")
        ));

        let Statement::VariableDeclaration { initializer, .. } = &program.statements[3] else {
            panic!("expected array literal declaration");
        };
        assert!(matches!(
            initializer,
            Expression::NewCollection {
                ty: TypeName::List(element),
                initializer: CollectionInitializer::Elements(elements),
                ..
            } if element.as_ref() == &TypeName::String && elements.len() == 2
        ));
    }

    #[test]
    fn parses_index_assignment_and_chained_method_calls() {
        let program = parse(
            "List<String> values = new List<String>{'zero'}; \
             values[0] = String.VaLuEOf(1); \
             values.add(values[0]); \
             System.debug(String.join(values, ''));",
        );

        let Statement::Expression { expression, .. } = &program.statements[1] else {
            panic!("expected index assignment");
        };
        assert!(matches!(
            expression,
            Expression::Assignment {
                target: AssignmentTarget::Index { .. },
                value,
                ..
            } if matches!(
                value.as_ref(),
                Expression::MethodCall { method, arguments, .. }
                    if method.spelling == "VaLuEOf"
                        && method.canonical == "valueof"
                        && arguments.len() == 1
            )
        ));

        let Statement::Expression { expression, .. } = &program.statements[2] else {
            panic!("expected add call");
        };
        assert!(matches!(
            expression,
            Expression::MethodCall { method, arguments, .. }
                if method.canonical == "add"
                    && matches!(arguments.as_slice(), [Expression::Index { .. }])
        ));

        let Statement::Expression { expression, .. } = &program.statements[3] else {
            panic!("System.debug should be an ordinary expression statement");
        };
        assert!(matches!(
            expression,
            Expression::MethodCall { method, arguments, .. }
                if method.canonical == "debug"
                    && matches!(arguments.as_slice(), [Expression::MethodCall { method, .. }] if method.canonical == "join")
        ));
    }

    #[test]
    fn distinguishes_enhanced_and_traditional_for_statements() {
        let program = parse(
            "List<String> values = new List<String>(); \
             for (String value : values) System.debug(value); \
             for (Integer index = 0; index < 1; index++) {}",
        );

        assert!(matches!(
            &program.statements[1],
            Statement::ForEach {
                element_type: TypeName::String,
                name,
                iterable: Expression::Variable(iterable),
                ..
            } if name.canonical == "value" && iterable.canonical == "values"
        ));
        assert!(matches!(&program.statements[2], Statement::For { .. }));
    }

    #[test]
    fn collection_postfix_nodes_preserve_full_source_spans() {
        let source = "List<String> values = new List<String>{'zero'}; values[0].toUpperCase();";
        let program = parse(source);
        let Statement::VariableDeclaration { initializer, .. } = &program.statements[0] else {
            panic!("expected declaration");
        };
        assert_eq!(
            &source[initializer.span().start..initializer.span().end],
            "new List<String>{'zero'}"
        );

        let Statement::Expression { expression, .. } = &program.statements[1] else {
            panic!("expected method call");
        };
        let Expression::MethodCall {
            receiver, method, ..
        } = expression
        else {
            panic!("expected method call expression");
        };
        assert_eq!(
            &source[expression.span().start..expression.span().end],
            "values[0].toUpperCase()"
        );
        assert_eq!(
            &source[receiver.span().start..receiver.span().end],
            "values[0]"
        );
        assert_eq!(&source[method.span.start..method.span.end], "toUpperCase");
        assert_eq!(method.canonical, "touppercase");
    }

    #[test]
    fn rejects_more_than_one_array_suffix_explicitly() {
        let error = Parser::new(
            Lexer::new("Integer[][] values = new Integer[1];")
                .tokenize()
                .unwrap(),
        )
        .parse_program()
        .unwrap_err();

        assert_eq!(error.message, "only one array suffix is supported");
    }

    #[test]
    fn parses_methods_separately_from_executable_statements() {
        let source = "Integer add(Integer left, Integer right) { return left + right; } \
                      void report(String value) { System.debug(value); } \
                      Integer total = add(1, 2);";
        let program = parse(source);

        assert_eq!(program.methods.len(), 2);
        assert_eq!(program.statements.len(), 1);

        let add = &program.methods[0];
        assert_eq!(add.return_type, ReturnType::Value(TypeName::Integer));
        assert_eq!(add.name.canonical, "add");
        assert_eq!(add.parameters.len(), 2);
        assert_eq!(add.parameters[0].ty, TypeName::Integer);
        assert_eq!(add.parameters[0].name.canonical, "left");
        assert!(matches!(
            add.body,
            Statement::Block {
                ref statements,
                ..
            } if matches!(statements.as_slice(), [Statement::Return { value: Some(_), .. }])
        ));

        let report = &program.methods[1];
        assert_eq!(report.return_type, ReturnType::Void);
        assert_eq!(report.parameters[0].ty, TypeName::String);
        assert_eq!(
            &source[report.span.start..report.span.end],
            "void report(String value) { System.debug(value); }"
        );

        let Statement::VariableDeclaration { initializer, .. } = &program.statements[0] else {
            panic!("expected executable declaration");
        };
        assert!(matches!(
            initializer,
            Expression::FunctionCall {
                name,
                arguments,
                ..
            } if name.canonical == "add"
                && arguments.len() == 2
        ));
    }

    #[test]
    fn parses_function_calls_as_postfix_receivers() {
        let program =
            parse("List<String> make() { return new List<String>(); } make().add('value');");
        let Statement::Expression { expression, .. } = &program.statements[0] else {
            panic!("expected call statement");
        };
        assert!(matches!(
            expression,
            Expression::MethodCall { receiver, method, .. }
                if method.canonical == "add"
                    && matches!(receiver.as_ref(), Expression::FunctionCall { name, .. } if name.canonical == "make")
        ));
    }

    #[test]
    fn parses_exception_construction_throw_and_handlers() {
        let source = "try { throw new IllegalArgumentException('bad input'); } \
                      catch (IllegalArgumentException problem) { throw problem; } \
                      catch (Exception ignored) {} \
                      finally { System.debug('cleanup'); }";
        let program = parse(source);
        let Statement::Try {
            try_block,
            catches,
            finally_block,
            ..
        } = &program.statements[0]
        else {
            panic!("expected try statement");
        };

        assert!(matches!(
            try_block.as_ref(),
            Statement::Block { statements, .. }
                if matches!(
                    statements.as_slice(),
                    [Statement::Throw {
                        value: Expression::NewException {
                            exception_type: TypeName::IllegalArgumentException,
                            arguments,
                            ..
                        },
                        ..
                    }] if matches!(arguments.as_slice(), [Expression::StringLiteral(value, _)] if value == "bad input")
                )
        ));
        assert_eq!(catches.len(), 2);
        assert_eq!(
            catches[0].exception_type,
            TypeName::IllegalArgumentException
        );
        assert_eq!(catches[0].name.canonical, "problem");
        assert_eq!(catches[1].exception_type, TypeName::Exception);
        assert!(finally_block.is_some());
    }

    #[test]
    fn parses_try_finally_without_a_catch() {
        let program = parse("try { System.debug('work'); } finally { System.debug('done'); }");
        assert!(matches!(
            &program.statements[0],
            Statement::Try {
                catches,
                finally_block: Some(_),
                ..
            } if catches.is_empty()
        ));
    }

    #[test]
    fn requires_a_catch_or_finally_after_try() {
        let error = Parser::new(Lexer::new("try {}").tokenize().unwrap())
            .parse_program()
            .unwrap_err();

        assert_eq!(
            error.message,
            "expected at least one `catch` or a `finally` after try block"
        );
    }

    #[test]
    fn preserves_non_exception_catch_types_for_semantic_validation() {
        let program = parse("try {} catch (String problem) {}");
        assert!(matches!(
            &program.statements[0],
            Statement::Try { catches, .. }
                if catches[0].exception_type == TypeName::String
        ));
    }

    #[test]
    fn preserves_exception_constructor_arguments_for_semantic_validation() {
        let program = parse("throw new Exception('first', 'second');");
        assert!(matches!(
            &program.statements[0],
            Statement::Throw {
                value: Expression::NewException { arguments, .. },
                ..
            } if arguments.len() == 2
        ));
    }

    #[test]
    fn distinguishes_casts_from_grouped_expressions() {
        let program = parse(
            "Object boxed = 1; Integer casted = (Integer) boxed; \
             Integer grouped = (1 + 2) * 3;",
        );

        let Statement::VariableDeclaration { initializer, .. } = &program.statements[1] else {
            panic!("expected cast declaration");
        };
        assert!(matches!(
            initializer,
            Expression::Cast {
                ty: TypeName::Integer,
                expression,
                ..
            } if matches!(expression.as_ref(), Expression::Variable(name) if name.canonical == "boxed")
        ));

        let Statement::VariableDeclaration { initializer, .. } = &program.statements[2] else {
            panic!("expected grouped declaration");
        };
        assert!(matches!(
            initializer,
            Expression::Binary {
                left,
                operator: BinaryOperator::Multiply,
                ..
            } if matches!(left.as_ref(), Expression::Binary { operator: BinaryOperator::Add, .. })
        ));
    }

    #[test]
    fn supports_object_and_core_exception_type_names_case_insensitively() {
        let program = parse(
            "oBjEcT identity(oBjEcT value) { return value; } \
             throw new nUlLpOiNtErExCePtIoN();",
        );

        assert_eq!(
            program.methods[0].return_type,
            ReturnType::Value(TypeName::Object)
        );
        assert_eq!(program.methods[0].parameters[0].ty, TypeName::Object);
        assert!(matches!(
            &program.statements[0],
            Statement::Throw {
                value: Expression::NewException {
                    exception_type: TypeName::NullPointerException,
                    arguments,
                    ..
                },
                ..
            } if arguments.is_empty()
        ));
    }
}
