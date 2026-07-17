use super::Parser;
use crate::{
    ast::{
        AssignmentTarget, BinaryOperator, CollectionInitializer, Expression, Identifier, MapEntry,
        PostfixOperator, TypeName, UnaryOperator,
    },
    diagnostic::Diagnostic,
    token::TokenKind,
};

impl Parser {
    pub(super) fn parse_expression(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_assignment()
    }

    pub(super) fn parse_assignment(&mut self) -> Result<Expression, Diagnostic> {
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
            Expression::MemberAccess {
                receiver,
                member,
                span,
            } => AssignmentTarget::Member {
                receiver,
                member,
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

    pub(super) fn parse_or(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(Self::parse_and, &[(TokenKind::OrOr, BinaryOperator::Or)])
    }

    pub(super) fn parse_and(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_equality,
            &[(TokenKind::AndAnd, BinaryOperator::And)],
        )
    }

    pub(super) fn parse_equality(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_comparison,
            &[
                (TokenKind::EqualEqual, BinaryOperator::Equal),
                (TokenKind::BangEqual, BinaryOperator::NotEqual),
            ],
        )
    }

    pub(super) fn parse_comparison(&mut self) -> Result<Expression, Diagnostic> {
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

    pub(super) fn parse_term(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_factor,
            &[
                (TokenKind::Plus, BinaryOperator::Add),
                (TokenKind::Minus, BinaryOperator::Subtract),
            ],
        )
    }

    pub(super) fn parse_factor(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_unary,
            &[
                (TokenKind::Star, BinaryOperator::Multiply),
                (TokenKind::Slash, BinaryOperator::Divide),
                (TokenKind::Percent, BinaryOperator::Remainder),
            ],
        )
    }

    pub(super) fn parse_binary_level(
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

    pub(super) fn parse_unary(&mut self) -> Result<Expression, Diagnostic> {
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

    pub(super) fn parse_postfix(&mut self) -> Result<Expression, Diagnostic> {
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
                    let member = self.expect_identifier("expected a member name after `.`")?;
                    if self.check(&TokenKind::LeftParen) {
                        let (arguments, end) = self.parse_argument_list()?;
                        let span = expression.span().merge(end);
                        expression = Expression::MethodCall {
                            receiver: Box::new(expression),
                            method: member,
                            arguments,
                            span,
                        };
                    } else {
                        let span = expression.span().merge(member.span);
                        expression = Expression::MemberAccess {
                            receiver: Box::new(expression),
                            member,
                            span,
                        };
                    }
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

    pub(super) fn parse_primary(&mut self) -> Result<Expression, Diagnostic> {
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

    pub(super) fn parse_new_expression(&mut self) -> Result<Expression, Diagnostic> {
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

        if matches!(ty, TypeName::Custom(_)) {
            if !self.check(&TokenKind::LeftParen) {
                return Err(Diagnostic::new(
                    "expected `(` after class name",
                    self.current().span,
                ));
            }
            let (arguments, end) = self.parse_argument_list()?;
            return Ok(Expression::NewObject {
                ty,
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

    pub(super) fn parse_collection_initializer(
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

    pub(super) fn parse_argument_list(
        &mut self,
    ) -> Result<(Vec<Expression>, crate::span::Span), Diagnostic> {
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
}
