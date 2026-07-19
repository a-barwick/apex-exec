use super::Parser;
use crate::{
    ast::{
        AssignmentOperator, AssignmentTarget, BinaryOperator, CollectionInitializer, Expression,
        Identifier, MapEntry, PostfixOperator, TypeName, UnaryOperator,
    },
    diagnostic::Diagnostic,
    token::TokenKind,
};

impl Parser {
    pub(super) fn parse_expression(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_assignment()
    }

    pub(super) fn parse_assignment(&mut self) -> Result<Expression, Diagnostic> {
        let expression = self.parse_conditional()?;
        let operator = match self.current().kind {
            TokenKind::Equal => AssignmentOperator::Assign,
            TokenKind::PlusEqual => AssignmentOperator::Add,
            TokenKind::MinusEqual => AssignmentOperator::Subtract,
            TokenKind::StarEqual => AssignmentOperator::Multiply,
            TokenKind::SlashEqual => AssignmentOperator::Divide,
            TokenKind::PercentEqual => AssignmentOperator::Remainder,
            TokenKind::AmpersandEqual => AssignmentOperator::BitwiseAnd,
            TokenKind::PipeEqual => AssignmentOperator::BitwiseOr,
            TokenKind::CaretEqual => AssignmentOperator::BitwiseXor,
            TokenKind::ShiftLeftEqual => AssignmentOperator::ShiftLeft,
            TokenKind::ShiftRightEqual => AssignmentOperator::ShiftRight,
            TokenKind::UnsignedShiftRightEqual => AssignmentOperator::UnsignedShiftRight,
            _ => return Ok(expression),
        };
        let operator_token = self.advance();
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
                safe_navigation: false,
                span,
                ..
            } => AssignmentTarget::Member {
                receiver,
                member,
                span,
            },
            Expression::MemberAccess {
                navigation_span, ..
            } => {
                return Err(Diagnostic::new(
                    "safe-navigation access cannot be an assignment target",
                    navigation_span,
                ));
            }
            _ => {
                return Err(Diagnostic::new(
                    "invalid assignment target",
                    operator_token.span,
                ));
            }
        };
        let span = target.span().merge(value.span());
        Ok(Expression::Assignment {
            target,
            operator,
            operator_span: operator_token.span,
            value: Box::new(value),
            span,
        })
    }

    pub(super) fn parse_conditional(&mut self) -> Result<Expression, Diagnostic> {
        let condition = self.parse_null_coalescing()?;
        if !self.check(&TokenKind::Question) {
            return Ok(condition);
        }

        let question = self.advance();
        let when_true = self.parse_expression()?;
        self.expect_simple(
            TokenKind::Colon,
            "expected `:` after the true branch of conditional expression",
        )?;
        let when_false = self.parse_conditional()?;
        let span = condition.span().merge(when_false.span());
        Ok(Expression::Conditional {
            condition: Box::new(condition),
            when_true: Box::new(when_true),
            when_false: Box::new(when_false),
            question_span: question.span,
            span,
        })
    }

    pub(super) fn parse_null_coalescing(&mut self) -> Result<Expression, Diagnostic> {
        let mut expression = self.parse_or()?;
        while self.check(&TokenKind::NullCoalesce) {
            let operator = self.advance();
            let right = self.parse_or()?;
            let span = expression.span().merge(right.span());
            expression = Expression::NullCoalesce {
                left: Box::new(expression),
                right: Box::new(right),
                operator_span: operator.span,
                span,
            };
        }
        Ok(expression)
    }

    pub(super) fn parse_or(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(Self::parse_and, &[(TokenKind::OrOr, BinaryOperator::Or)])
    }

    pub(super) fn parse_and(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_bitwise_or,
            &[(TokenKind::AndAnd, BinaryOperator::And)],
        )
    }

    pub(super) fn parse_bitwise_or(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_bitwise_xor,
            &[(TokenKind::Pipe, BinaryOperator::BitwiseOr)],
        )
    }

    pub(super) fn parse_bitwise_xor(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_bitwise_and,
            &[(TokenKind::Caret, BinaryOperator::BitwiseXor)],
        )
    }

    pub(super) fn parse_bitwise_and(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_equality,
            &[(TokenKind::Ampersand, BinaryOperator::BitwiseAnd)],
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
        let mut expression = self.parse_shift()?;
        loop {
            if self.check(&TokenKind::Instanceof) {
                let operator = self.advance();
                let (target, target_span) = self.parse_type_name()?;
                let span = expression.span().merge(target_span);
                expression = Expression::Instanceof {
                    value: Box::new(expression),
                    target,
                    target_span,
                    operator_span: operator.span,
                    span,
                };
                continue;
            }
            let operator = [
                (TokenKind::Less, BinaryOperator::Less),
                (TokenKind::LessEqual, BinaryOperator::LessEqual),
                (TokenKind::Greater, BinaryOperator::Greater),
                (TokenKind::GreaterEqual, BinaryOperator::GreaterEqual),
            ]
            .iter()
            .find(|(kind, _)| self.check(kind))
            .map(|(_, operator)| *operator);
            let Some(operator) = operator else {
                break;
            };
            let operator_token = self.advance();
            let right = self.parse_shift()?;
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

    pub(super) fn parse_shift(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_binary_level(
            Self::parse_term,
            &[
                (TokenKind::ShiftLeft, BinaryOperator::ShiftLeft),
                (TokenKind::ShiftRight, BinaryOperator::ShiftRight),
                (
                    TokenKind::UnsignedShiftRight,
                    BinaryOperator::UnsignedShiftRight,
                ),
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
            TokenKind::Tilde => Some(UnaryOperator::BitwiseNot),
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
        let mut safe_navigation_chain = false;
        loop {
            match self.current().kind {
                TokenKind::LeftBracket => {
                    if safe_navigation_chain {
                        return Err(Diagnostic::new(
                            "indexed access in a safe-navigation chain is unsupported",
                            self.current().span,
                        ));
                    }
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
                TokenKind::Dot | TokenKind::SafeNavigation => {
                    let navigation = self.advance();
                    if navigation.kind == TokenKind::SafeNavigation {
                        safe_navigation_chain = true;
                    }
                    let safe_navigation = safe_navigation_chain;
                    let member = if self.check(&TokenKind::New) {
                        let token = self.advance();
                        Identifier::new(token.lexeme, token.span)
                    } else {
                        self.expect_identifier("expected a member name after `.`")?
                    };
                    if self.check(&TokenKind::LeftParen) {
                        let (arguments, end) = self.parse_argument_list()?;
                        let span = expression.span().merge(end);
                        expression = Expression::MethodCall {
                            receiver: Box::new(expression),
                            method: member,
                            arguments,
                            safe_navigation,
                            navigation_span: navigation.span,
                            span,
                        };
                    } else {
                        let span = expression.span().merge(member.span);
                        expression = Expression::MemberAccess {
                            receiver: Box::new(expression),
                            member,
                            safe_navigation,
                            navigation_span: navigation.span,
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
            TokenKind::LongLiteral(value) => Expression::LongLiteral(value, token.span),
            TokenKind::DecimalLiteral(value) => Expression::DecimalLiteral(value, token.span),
            TokenKind::Null => Expression::NullLiteral(token.span),
            TokenKind::Identifier(spelling) => {
                let mut probe = self.clone();
                if probe.parse_type_name().is_ok()
                    && probe.check(&TokenKind::Dot)
                    && matches!(probe.peek(1).kind, TokenKind::Class)
                {
                    let (ty, type_span) = self.parse_type_name()?;
                    self.advance();
                    let end = self.advance();
                    return Ok(Expression::TypeLiteral {
                        ty,
                        span: type_span.merge(end.span),
                    });
                }
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
            TokenKind::LeftBracket => return self.parse_query_expression(),
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
        let (ty, _) = self.parse_type_name()?;

        if self.check(&TokenKind::LeftBracket) {
            return self.parse_new_array_expression(start, ty);
        }

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

        if matches!(
            ty,
            TypeName::Custom(_) | TypeName::Http | TypeName::HttpRequest | TypeName::HttpResponse
        ) {
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

        let (initializer, end) = if self.check(&TokenKind::LeftParen) {
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

    fn parse_new_array_expression(
        &mut self,
        start: crate::token::Token,
        element_type: TypeName,
    ) -> Result<Expression, Diagnostic> {
        self.expect_simple(TokenKind::LeftBracket, "expected `[`")?;
        let ty = TypeName::List(Box::new(element_type));
        let (initializer, end) = if self.check(&TokenKind::RightBracket) {
            self.advance();
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
            (CollectionInitializer::SizedArray(Box::new(size)), end.span)
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
