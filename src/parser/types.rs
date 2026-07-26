use super::Parser;
use crate::{
    ast::{NamedType, ReturnType, TypeArgument, TypeName, TypeRef},
    diagnostic::Diagnostic,
    token::TokenKind,
};

impl Parser {
    pub(super) fn parse_type_name(&mut self) -> Result<(TypeName, crate::span::Span), Diagnostic> {
        let syntax = self.parse_type_ref()?;
        let span = syntax.span;
        Ok((type_name_from_ref(&syntax)?, span))
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

    pub(super) fn parse_named_type(&mut self) -> Result<NamedType, Diagnostic> {
        let syntax = self.parse_type_ref()?;
        if let Some(span) = syntax.array_suffixes.first() {
            return Err(Diagnostic::new(
                "hierarchy and trigger types cannot use array suffixes",
                *span,
            ));
        }
        named_type_from_ref(syntax)
    }

    pub(super) fn parse_type_ref(&mut self) -> Result<TypeRef, Diagnostic> {
        let first = self.expect_identifier("expected a type name")?;
        let mut span = first.span;
        let mut segments = vec![first];
        while self.check(&TokenKind::Dot) && matches!(self.peek(1).kind, TokenKind::Identifier(_)) {
            self.advance();
            let segment = self.expect_identifier("expected a type name after `.`")?;
            span = span.merge(segment.span);
            segments.push(segment);
        }

        let mut type_arguments = Vec::new();
        if self.check(&TokenKind::Less) {
            self.advance();
            loop {
                type_arguments.push(self.parse_type_ref()?);
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
            let end = self.expect_type_greater("expected `>` after type argument")?;
            span = span.merge(end.span);
        }

        let mut array_suffixes = Vec::new();
        while self.check(&TokenKind::LeftBracket)
            && matches!(self.peek(1).kind, TokenKind::RightBracket)
        {
            let start = self.advance().span;
            let end = self.advance().span;
            let suffix = start.merge(end);
            span = span.merge(suffix);
            array_suffixes.push(suffix);
        }
        Ok(TypeRef {
            segments,
            type_arguments,
            array_suffixes,
            span,
        })
    }

    pub(super) fn is_declaration_start(&self) -> bool {
        let mut probe = self.clone();
        while matches!(
            probe.current().kind,
            TokenKind::Final | TokenKind::Transient
        ) {
            probe.advance();
        }
        match probe.parse_type_name() {
            Ok(_) => matches!(probe.current().kind, TokenKind::Identifier(_)),
            Err(error) if error.message == "only one array suffix is supported" => {
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
                | TokenKind::Final
                | TokenKind::Transient => cursor += 1,
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
        ) || matches!(
            &self.token_at(cursor).kind,
            TokenKind::Identifier(spelling) if spelling.eq_ignore_ascii_case("enum")
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
        if !matches!(probe.current().kind, TokenKind::RightParen) {
            return false;
        }
        probe.advance();
        let numeric_scalar = matches!(
            candidate,
            TypeName::Integer | TypeName::Long | TypeName::Decimal | TypeName::Double
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

fn named_type_from_ref(syntax: TypeRef) -> Result<NamedType, Diagnostic> {
    let type_arguments = syntax
        .type_arguments
        .iter()
        .map(|argument| {
            Ok(TypeArgument {
                syntax: argument.clone(),
                ty: type_name_from_ref(argument)?,
                span: argument.span,
            })
        })
        .collect::<Result<Vec<_>, Diagnostic>>()?;
    Ok(NamedType::from_type_ref(syntax, type_arguments))
}

fn type_name_from_ref(syntax: &TypeRef) -> Result<TypeName, Diagnostic> {
    if syntax.array_suffixes.len() > 1 {
        return Err(Diagnostic::new(
            "only one array suffix is supported",
            syntax.array_suffixes[1],
        ));
    }
    let canonical = syntax.canonical();
    let arguments = syntax
        .type_arguments
        .iter()
        .map(type_name_from_ref)
        .collect::<Result<Vec<_>, _>>()?;
    let mut ty = match canonical.as_str() {
        "list" | "set" | "iterable" => {
            let [element] = arguments.as_slice() else {
                return Err(Diagnostic::new(
                    format!("{} requires exactly one type argument", syntax.spelling()),
                    syntax.span,
                ));
            };
            match canonical.as_str() {
                "list" => TypeName::List(Box::new(element.clone())),
                "set" => TypeName::Set(Box::new(element.clone())),
                _ => TypeName::Iterable(Box::new(element.clone())),
            }
        }
        "map" => {
            let [key, value] = arguments.as_slice() else {
                return Err(Diagnostic::new(
                    "Map requires exactly two type arguments",
                    syntax.span,
                ));
            };
            TypeName::Map(Box::new(key.clone()), Box::new(value.clone()))
        }
        _ if TypeName::from_apex_name(&canonical).is_some() => {
            if !arguments.is_empty() {
                return Err(Diagnostic::new(
                    format!("{} does not accept type arguments", syntax.spelling()),
                    syntax.span,
                ));
            }
            TypeName::from_apex_name(&canonical).expect("type presence was checked")
        }
        _ => TypeName::Custom(named_type_from_ref(syntax.clone())?),
    };
    for _ in &syntax.array_suffixes {
        ty = TypeName::List(Box::new(ty));
    }
    Ok(ty)
}
