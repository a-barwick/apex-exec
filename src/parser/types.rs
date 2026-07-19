use super::Parser;
use crate::{
    ast::{NamedType, ReturnType, TypeArgument, TypeName},
    diagnostic::Diagnostic,
    token::TokenKind,
};

impl Parser {
    pub(super) fn parse_type_name(&mut self) -> Result<(TypeName, crate::span::Span), Diagnostic> {
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

    pub(super) fn parse_return_type(
        &mut self,
    ) -> Result<(ReturnType, crate::span::Span), Diagnostic> {
        if self.check(&TokenKind::Void) {
            let token = self.advance();
            return Ok((ReturnType::Void, token.span));
        }
        let (ty, span) = self.parse_type_name()?;
        Ok((ReturnType::Value(ty), span))
    }

    pub(super) fn parse_base_type_name(
        &mut self,
    ) -> Result<(TypeName, crate::span::Span), Diagnostic> {
        let identifier = self.expect_identifier("expected a type name")?;
        let mut spelling = identifier.spelling.clone();
        let mut canonical = identifier.canonical.clone();
        let mut qualified_span = identifier.span;
        if self.check(&TokenKind::Dot) {
            self.advance();
            let nested = self.expect_identifier("expected a type name after `.`")?;
            spelling.push('.');
            spelling.push_str(&nested.spelling);
            canonical.push('.');
            canonical.push_str(&nested.canonical);
            qualified_span = qualified_span.merge(nested.span);
        }
        match canonical.as_str() {
            "list" | "set" => {
                self.expect_simple(TokenKind::Less, "expected `<` after collection type name")?;
                let (element, _) = self.parse_type_name()?;
                let end = self.expect_type_greater("expected `>` after collection element type")?;
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
                let end = self.expect_type_greater("expected `>` after map value type")?;
                Ok((
                    TypeName::Map(Box::new(key), Box::new(value)),
                    identifier.span.merge(end.span),
                ))
            }
            _ if TypeName::from_apex_name(&canonical).is_some() => Ok((
                TypeName::from_apex_name(&canonical).expect("type presence was checked"),
                qualified_span,
            )),
            _ => Ok((
                TypeName::Custom(NamedType::new(spelling, qualified_span)),
                qualified_span,
            )),
        }
    }

    pub(super) fn parse_named_type(&mut self) -> Result<NamedType, Diagnostic> {
        let identifier = self.expect_identifier("expected a type name")?;
        let mut spelling = identifier.spelling;
        let mut span = identifier.span;
        if self.check(&TokenKind::Dot) {
            self.advance();
            let nested = self.expect_identifier("expected an interface name after `.`")?;
            spelling.push('.');
            spelling.push_str(&nested.spelling);
            span = span.merge(nested.span);
        }
        let mut type_arguments = Vec::new();
        if self.check(&TokenKind::Less) {
            self.advance();
            loop {
                let (ty, argument_span) = self.parse_type_name()?;
                type_arguments.push(TypeArgument {
                    ty,
                    span: argument_span,
                });
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
            let end = self.expect_type_greater("expected `>` after interface type argument")?;
            span = span.merge(end.span);
        }
        Ok(NamedType::with_type_arguments(
            spelling,
            type_arguments,
            span,
        ))
    }

    pub(super) fn is_declaration_start(&self) -> bool {
        let mut probe = self.clone();
        match probe.parse_type_name() {
            Ok(_) => matches!(probe.current().kind, TokenKind::Identifier(_)),
            Err(error) if error.message == "only one array suffix is supported" => {
                while probe.check(&TokenKind::LeftBracket)
                    && matches!(probe.peek(1).kind, TokenKind::RightBracket)
                {
                    probe.advance();
                    probe.advance();
                }
                matches!(probe.current().kind, TokenKind::Identifier(_))
            }
            Err(_) => false,
        }
    }

    pub(super) fn is_method_declaration_start(&self) -> bool {
        let mut probe = self.clone();
        probe.parse_return_type().is_ok_and(|_| {
            matches!(probe.current().kind, TokenKind::Identifier(_))
                && matches!(probe.peek(1).kind, TokenKind::LeftParen)
        })
    }

    pub(super) fn is_class_declaration_start(&self) -> bool {
        let mut cursor = self.annotation_prefix_end_at(self.cursor);
        loop {
            match self.token_at(cursor).kind {
                TokenKind::Public
                | TokenKind::Private
                | TokenKind::Protected
                | TokenKind::Global
                | TokenKind::Static
                | TokenKind::Virtual
                | TokenKind::Abstract
                | TokenKind::Override
                | TokenKind::Final => cursor += 1,
                TokenKind::Identifier(ref spelling)
                    if ["with", "without", "inherited"]
                        .iter()
                        .any(|candidate| spelling.eq_ignore_ascii_case(candidate))
                        && matches!(&self.token_at(cursor + 1).kind, TokenKind::Identifier(next) if next.eq_ignore_ascii_case("sharing")) =>
                {
                    cursor += 2;
                }
                _ => break,
            }
        }
        matches!(
            self.token_at(cursor).kind,
            TokenKind::Class | TokenKind::Interface
        )
    }

    pub(super) fn annotation_prefix_end_at(&self, mut cursor: usize) -> usize {
        while matches!(self.token_at(cursor).kind, TokenKind::At) {
            cursor += 1;
            if !matches!(self.token_at(cursor).kind, TokenKind::Identifier(_)) {
                return cursor;
            }
            cursor += 1;
            if matches!(self.token_at(cursor).kind, TokenKind::LeftParen) {
                let mut depth = 0usize;
                loop {
                    match self.token_at(cursor).kind {
                        TokenKind::LeftParen => depth += 1,
                        TokenKind::RightParen => {
                            depth -= 1;
                            cursor += 1;
                            if depth == 0 {
                                break;
                            }
                            continue;
                        }
                        TokenKind::Eof => return cursor,
                        _ => {}
                    }
                    cursor += 1;
                }
            }
        }
        cursor
    }

    pub(super) fn is_cast_start(&self) -> bool {
        if !self.check(&TokenKind::LeftParen) {
            return false;
        }
        let mut probe = self.clone();
        probe.advance();
        let Ok((candidate, _)) = probe.parse_type_name() else {
            return false;
        };
        if !matches!(probe.current().kind, TokenKind::RightParen)
            || matches!(&candidate, TypeName::Custom(name) if name.canonical.contains('.'))
        {
            // Arbitrary qualified custom types are outside the current grammar,
            // so `(receiver.member)` remains an expression rather than a cast.
            return false;
        }
        probe.advance();
        let numeric_scalar = matches!(
            candidate,
            TypeName::Integer | TypeName::Long | TypeName::Decimal
        );
        match &probe.current().kind {
            // Signed unary operands disambiguate only numeric scalar casts;
            // reference-shaped candidates remain grouped binary expressions.
            TokenKind::Plus | TokenKind::Minus => numeric_scalar,
            TokenKind::PlusPlus | TokenKind::MinusMinus => {
                numeric_scalar && is_unary_expression_start(&probe.peek(1).kind)
            }
            // `[` continues a grouped value as indexing unless it starts the
            // supported SOQL/SOSL primary-expression shape.
            TokenKind::LeftBracket => probe.is_query_expression_start_at(probe.cursor),
            operand => is_unary_expression_start(operand),
        }
    }

    pub(super) fn is_for_each_start(&self) -> bool {
        let mut probe = self.clone();
        probe.parse_type_name().is_ok_and(|_| {
            matches!(probe.current().kind, TokenKind::Identifier(_))
                && matches!(probe.peek(1).kind, TokenKind::Colon)
        })
    }

    fn is_query_expression_start_at(&self, cursor: usize) -> bool {
        matches!(
            &self.token_at(cursor + 1).kind,
            TokenKind::Identifier(spelling)
                if spelling.eq_ignore_ascii_case("select")
                    || spelling.eq_ignore_ascii_case("find")
        )
    }

    fn expect_type_greater(&mut self, message: &str) -> Result<crate::token::Token, Diagnostic> {
        let token = self.current().clone();
        if !token.lexeme.starts_with('>') {
            return Err(Diagnostic::new(message, token.span));
        }

        let consumed = crate::token::Token {
            kind: TokenKind::Greater,
            span: crate::span::Span::new_in(
                token.span.source_id,
                token.span.start,
                token.span.start + 1,
            ),
            lexeme: ">".to_owned(),
        };
        let remainder = &token.lexeme[1..];
        if remainder.is_empty() {
            self.advance();
        } else {
            let kind = match remainder {
                ">" => TokenKind::Greater,
                ">>" => TokenKind::ShiftRight,
                "=" => TokenKind::Equal,
                ">=" => TokenKind::GreaterEqual,
                ">>=" => TokenKind::ShiftRightEqual,
                _ => {
                    return Err(Diagnostic::new(
                        "invalid token after generic type closer",
                        token.span,
                    ));
                }
            };
            self.tokens[self.cursor] = crate::token::Token {
                kind,
                span: crate::span::Span::new_in(
                    token.span.source_id,
                    token.span.start + 1,
                    token.span.end,
                ),
                lexeme: remainder.to_owned(),
            };
        }
        Ok(consumed)
    }
}

fn is_unary_expression_start(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier(_)
            | TokenKind::StringLiteral(_)
            | TokenKind::BooleanLiteral(_)
            | TokenKind::IntegerLiteral(_)
            | TokenKind::LongLiteral(_)
            | TokenKind::DecimalLiteral(_)
            | TokenKind::Null
            | TokenKind::New
            | TokenKind::LeftBracket
            | TokenKind::LeftParen
            | TokenKind::Plus
            | TokenKind::PlusPlus
            | TokenKind::Minus
            | TokenKind::MinusMinus
            | TokenKind::Bang
            | TokenKind::Tilde
    )
}
