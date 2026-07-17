use super::Parser;
use crate::{
    ast::{NamedType, ReturnType, TypeName},
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
        if self.check(&TokenKind::Less) {
            self.advance();
            let (_, argument_span) = self.parse_type_name()?;
            let end = self.expect_simple(
                TokenKind::Greater,
                "expected `>` after interface type argument",
            )?;
            span = span.merge(argument_span).merge(end.span);
        }
        Ok(NamedType::new(spelling, span))
    }

    pub(super) fn is_declaration_start(&self) -> bool {
        self.type_end_at(self.cursor)
            .is_some_and(|end| matches!(self.token_at(end).kind, TokenKind::Identifier(_)))
    }

    pub(super) fn is_method_declaration_start(&self) -> bool {
        self.return_type_end_at(self.cursor).is_some_and(|end| {
            matches!(self.token_at(end).kind, TokenKind::Identifier(_))
                && matches!(self.token_at(end + 1).kind, TokenKind::LeftParen)
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
        self.type_end_at(self.cursor + 1)
            .is_some_and(|end| matches!(self.token_at(end).kind, TokenKind::RightParen))
    }

    pub(super) fn is_for_each_start(&self) -> bool {
        self.type_end_at(self.cursor).is_some_and(|end| {
            matches!(self.token_at(end).kind, TokenKind::Identifier(_))
                && matches!(self.token_at(end + 1).kind, TokenKind::Colon)
        })
    }

    pub(super) fn type_end_at(&self, cursor: usize) -> Option<usize> {
        let TokenKind::Identifier(spelling) = &self.token_at(cursor).kind else {
            return None;
        };
        let canonical = spelling.to_ascii_lowercase();
        let mut end = cursor + 1;
        let mut qualified = canonical.clone();
        if matches!(self.token_at(end).kind, TokenKind::Dot)
            && matches!(self.token_at(end + 1).kind, TokenKind::Identifier(_))
        {
            let TokenKind::Identifier(nested) = &self.token_at(end + 1).kind else {
                unreachable!()
            };
            qualified.push('.');
            qualified.push_str(&nested.to_ascii_lowercase());
            end += 2;
        }
        match qualified.as_str() {
            "string"
            | "boolean"
            | "integer"
            | "decimal"
            | "date"
            | "datetime"
            | "time"
            | "id"
            | "blob"
            | "object"
            | "pattern"
            | "matcher"
            | "http"
            | "httprequest"
            | "httpresponse"
            | "queueablecontext"
            | "system.queueablecontext"
            | "batchablecontext"
            | "database.batchablecontext"
            | "schedulablecontext"
            | "system.schedulablecontext"
            | "sobjecttype"
            | "describesobjectresult"
            | "exception"
            | "nullpointerexception"
            | "listexception"
            | "mathexception"
            | "typeexception"
            | "stringexception"
            | "illegalargumentexception"
            | "finalexception"
            | "assertexception"
            | "queryexception"
            | "dmlexception"
            | "asyncexception"
            | "aggregateresult" => {}
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
            _ => {}
        }
        while matches!(self.token_at(end).kind, TokenKind::LeftBracket)
            && matches!(self.token_at(end + 1).kind, TokenKind::RightBracket)
        {
            end += 2;
        }
        Some(end)
    }

    pub(super) fn return_type_end_at(&self, cursor: usize) -> Option<usize> {
        if matches!(&self.token_at(cursor).kind, TokenKind::Void) {
            Some(cursor + 1)
        } else {
            self.type_end_at(cursor)
        }
    }
}
