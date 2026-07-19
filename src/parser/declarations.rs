use super::Parser;
use crate::{
    ast::{
        AccessorKind, Annotation, AnnotationKind, ClassDeclaration, ClassKind, ClassMember,
        ConstructorDeclaration, FieldDeclaration, Identifier, MethodDeclaration, Modifier,
        Parameter, PropertyAccessor, PropertyDeclaration, ReturnType, TriggerDeclaration,
        TriggerEvent, TypeName,
    },
    diagnostic::Diagnostic,
    span::Span,
    token::TokenKind,
};

impl Parser {
    pub(super) fn parse_trigger_declaration(&mut self) -> Result<TriggerDeclaration, Diagnostic> {
        let start = self.expect_keyword("trigger", "expected `trigger`")?;
        let name = self.expect_identifier("expected a trigger name")?;
        self.expect_keyword("on", "expected `on` after trigger name")?;
        let object = self.parse_named_type()?;
        self.expect_simple(TokenKind::LeftParen, "expected `(` before trigger events")?;
        let mut events = Vec::new();
        loop {
            let phase = self.expect_identifier("expected `before` or `after`")?;
            let operation = self.expect_identifier("expected a trigger DML event")?;
            let event = match (phase.canonical.as_str(), operation.canonical.as_str()) {
                ("before", "insert") => TriggerEvent::BeforeInsert,
                ("before", "update") => TriggerEvent::BeforeUpdate,
                ("before", "delete") => TriggerEvent::BeforeDelete,
                ("before", "undelete") => TriggerEvent::BeforeUndelete,
                ("after", "insert") => TriggerEvent::AfterInsert,
                ("after", "update") => TriggerEvent::AfterUpdate,
                ("after", "delete") => TriggerEvent::AfterDelete,
                ("after", "undelete") => TriggerEvent::AfterUndelete,
                _ => {
                    return Err(Diagnostic::new(
                        format!(
                            "unsupported trigger event `{} {}`",
                            phase.spelling, operation.spelling
                        ),
                        phase.span.merge(operation.span),
                    ));
                }
            };
            events.push(event);
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance();
        }
        self.expect_simple(TokenKind::RightParen, "expected `)` after trigger events")?;
        let body = self.parse_block()?;
        let span = start.span.merge(body.span());
        Ok(TriggerDeclaration {
            name,
            object,
            events,
            body,
            span,
        })
    }

    pub(super) fn parse_class_declaration(&mut self) -> Result<ClassDeclaration, Diagnostic> {
        let annotations = self.parse_annotations()?;
        if let Some(annotation) = annotations
            .iter()
            .find(|annotation| annotation.kind.is_future())
        {
            return Err(Diagnostic::new(
                "`@future` is only valid on methods",
                annotation.span,
            ));
        }
        let modifiers = self.parse_modifiers()?;
        let start = self.current().span;
        let kind = if self.check(&TokenKind::Class) {
            self.advance();
            ClassKind::Class
        } else {
            self.expect_simple(TokenKind::Interface, "expected `class` or `interface`")?;
            ClassKind::Interface
        };
        let name = self.expect_identifier("expected a type name")?;
        let (superclass, interfaces) = self.parse_hierarchy_edges(kind)?;
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after type declaration")?;
        let mut members = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            members.push(self.parse_class_member(&name)?);
        }
        let end = self.expect_simple(TokenKind::RightBrace, "expected `}` after type body")?;
        Ok(ClassDeclaration {
            annotations,
            kind,
            modifiers,
            name,
            superclass,
            interfaces,
            members,
            span: start.merge(end.span),
        })
    }

    fn parse_hierarchy_edges(
        &mut self,
        kind: ClassKind,
    ) -> Result<(Option<crate::ast::NamedType>, Vec<crate::ast::NamedType>), Diagnostic> {
        let superclass = if self.check(&TokenKind::Extends) {
            self.advance();
            Some(self.parse_named_type()?)
        } else {
            None
        };

        if kind == ClassKind::Interface && self.check(&TokenKind::Implements) {
            return Err(Diagnostic::new(
                "interfaces extend other interfaces; `implements` is invalid here",
                self.current().span,
            ));
        }

        let mut interfaces = Vec::new();
        if kind == ClassKind::Class && self.check(&TokenKind::Implements) {
            self.advance();
            loop {
                interfaces.push(self.parse_named_type()?);
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
        }
        Ok((superclass, interfaces))
    }

    pub(super) fn parse_class_member(
        &mut self,
        class_name: &Identifier,
    ) -> Result<ClassMember, Diagnostic> {
        let annotations = self.parse_annotations()?;
        let modifiers = self.parse_modifiers()?;
        if matches!(&self.current().kind, TokenKind::Identifier(spelling)
            if spelling.eq_ignore_ascii_case(&class_name.spelling))
            && matches!(self.peek(1).kind, TokenKind::LeftParen)
        {
            if let Some(annotation) = annotations.first() {
                return Err(Diagnostic::new(
                    "annotations are not supported on constructors",
                    annotation.span,
                ));
            }
            let name = self.expect_identifier("expected constructor name")?;
            let parameters = self.parse_parameters()?;
            let body = self.parse_block()?;
            let span = name.span.merge(body.span());
            return Ok(ClassMember::Constructor(ConstructorDeclaration {
                modifiers,
                name,
                parameters,
                body,
                span,
            }));
        }

        let (return_type, start) = self.parse_return_type()?;
        let name = self.expect_identifier("expected a member name")?;
        if self.check(&TokenKind::LeftParen) {
            let parameters = self.parse_parameters()?;
            let (body, end) = if self.check(&TokenKind::Semicolon) {
                (None, self.advance().span)
            } else {
                let body = self.parse_block()?;
                let end = body.span();
                (Some(body), end)
            };
            return Ok(ClassMember::Method(MethodDeclaration {
                annotations,
                modifiers,
                return_type,
                name,
                parameters,
                body,
                span: start.merge(end),
            }));
        }

        let ReturnType::Value(ty) = return_type else {
            return Err(Diagnostic::new(
                "fields and properties cannot have type void",
                start,
            ));
        };
        if let Some(annotation) = annotations.first() {
            return Err(Diagnostic::new(
                "annotations are only supported on classes and methods",
                annotation.span,
            ));
        }
        if self.check(&TokenKind::LeftBrace) {
            return self
                .parse_property(modifiers, ty, name, start)
                .map(ClassMember::Property);
        }
        let initializer = if self.check(&TokenKind::Equal) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after field")?;
        Ok(ClassMember::Field(FieldDeclaration {
            modifiers,
            ty,
            name,
            initializer,
            span: start.merge(end.span),
        }))
    }

    pub(super) fn parse_property(
        &mut self,
        modifiers: Vec<Modifier>,
        ty: TypeName,
        name: Identifier,
        start: crate::span::Span,
    ) -> Result<PropertyDeclaration, Diagnostic> {
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after property name")?;
        let mut accessors = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let accessor_modifier = if self.check(&TokenKind::Public)
                || self.check(&TokenKind::Private)
                || self.check(&TokenKind::Protected)
                || self.check(&TokenKind::Global)
            {
                Some(self.parse_modifier()?)
            } else {
                None
            };
            let accessor_start = self.current().span;
            let kind = if matches!(&self.current().kind, TokenKind::Identifier(spelling) if spelling.eq_ignore_ascii_case("get"))
            {
                self.advance();
                AccessorKind::Get
            } else if matches!(&self.current().kind, TokenKind::Identifier(spelling) if spelling.eq_ignore_ascii_case("set"))
            {
                self.advance();
                AccessorKind::Set
            } else {
                return Err(Diagnostic::new(
                    "expected `get` or `set` property accessor",
                    self.current().span,
                ));
            };
            let (body, end) = if self.check(&TokenKind::Semicolon) {
                (None, self.advance().span)
            } else {
                let body = self.parse_block()?;
                let end = body.span();
                (Some(body), end)
            };
            accessors.push(PropertyAccessor {
                kind,
                modifier: accessor_modifier,
                body,
                span: accessor_start.merge(end),
            });
        }
        let end = self.expect_simple(TokenKind::RightBrace, "expected `}` after property")?;
        Ok(PropertyDeclaration {
            modifiers,
            ty,
            name,
            accessors,
            span: start.merge(end.span),
        })
    }

    pub(super) fn parse_method_declaration(&mut self) -> Result<MethodDeclaration, Diagnostic> {
        let (return_type, start) = self.parse_return_type()?;
        let name = self.expect_identifier("expected a method name")?;
        let parameters = self.parse_parameters()?;
        let body = self.parse_block()?;
        let span = start.merge(body.span());
        Ok(MethodDeclaration {
            annotations: Vec::new(),
            modifiers: Vec::new(),
            return_type,
            name,
            parameters,
            body: Some(body),
            span,
        })
    }

    pub(super) fn parse_parameters(&mut self) -> Result<Vec<Parameter>, Diagnostic> {
        self.expect_simple(TokenKind::LeftParen, "expected `(`")?;
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
        Ok(parameters)
    }

    pub(super) fn parse_modifiers(&mut self) -> Result<Vec<Modifier>, Diagnostic> {
        let mut modifiers = Vec::new();
        loop {
            if self.is_simple_modifier() {
                modifiers.push(self.parse_modifier()?);
                continue;
            }
            let sharing = match &self.current().kind {
                TokenKind::Identifier(spelling)
                    if spelling.eq_ignore_ascii_case("with")
                        && matches!(&self.peek(1).kind, TokenKind::Identifier(next) if next.eq_ignore_ascii_case("sharing")) =>
                {
                    Some(Modifier::WithSharing)
                }
                TokenKind::Identifier(spelling)
                    if spelling.eq_ignore_ascii_case("without")
                        && matches!(&self.peek(1).kind, TokenKind::Identifier(next) if next.eq_ignore_ascii_case("sharing")) =>
                {
                    Some(Modifier::WithoutSharing)
                }
                TokenKind::Identifier(spelling)
                    if spelling.eq_ignore_ascii_case("inherited")
                        && matches!(&self.peek(1).kind, TokenKind::Identifier(next) if next.eq_ignore_ascii_case("sharing")) =>
                {
                    Some(Modifier::InheritedSharing)
                }
                _ => None,
            };
            let Some(sharing) = sharing else {
                break;
            };
            self.advance();
            self.advance();
            modifiers.push(sharing);
        }
        Ok(modifiers)
    }

    pub(super) fn parse_annotations(&mut self) -> Result<Vec<Annotation>, Diagnostic> {
        let mut annotations = Vec::new();
        while self.check(&TokenKind::At) {
            let start = self.advance().span;
            let name = self.expect_identifier("expected an annotation name after `@`")?;
            let kind = match name.canonical.as_str() {
                "istest" => {
                    let see_all_data = if self.check(&TokenKind::LeftParen) {
                        self.advance();
                        let argument = self
                            .expect_identifier("expected `SeeAllData` in `@IsTest` annotation")?;
                        if argument.canonical != "seealldata" {
                            return Err(Diagnostic::new(
                                "only `SeeAllData` is supported in `@IsTest`",
                                argument.span,
                            ));
                        }
                        self.expect_simple(TokenKind::Equal, "expected `=` after `SeeAllData`")?;
                        let value = match self.current().kind {
                            TokenKind::BooleanLiteral(value) => {
                                self.advance();
                                value
                            }
                            _ => {
                                return Err(Diagnostic::new(
                                    "`SeeAllData` requires a Boolean literal",
                                    self.current().span,
                                ));
                            }
                        };
                        self.expect_simple(
                            TokenKind::RightParen,
                            "expected `)` after `@IsTest` arguments",
                        )?;
                        Some(value)
                    } else {
                        None
                    };
                    AnnotationKind::IsTest { see_all_data }
                }
                "testsetup" => {
                    if self.check(&TokenKind::LeftParen) {
                        return Err(Diagnostic::new(
                            "`@TestSetup` does not accept arguments",
                            self.current().span,
                        ));
                    }
                    AnnotationKind::TestSetup
                }
                "future" => {
                    if self.check(&TokenKind::LeftParen) {
                        return Err(Diagnostic::new(
                            "`@future` options are not supported",
                            self.current().span,
                        ));
                    }
                    AnnotationKind::Future
                }
                _ => {
                    return Err(Diagnostic::new(
                        format!("unsupported annotation `@{}`", name.spelling),
                        name.span,
                    ));
                }
            };
            let end = self.peek(0).span.start;
            annotations.push(Annotation {
                kind,
                span: Span::new_in(start.source_id, start.start, end),
            });
        }
        Ok(annotations)
    }

    pub(super) fn parse_modifier(&mut self) -> Result<Modifier, Diagnostic> {
        let token = self.advance();
        match token.kind {
            TokenKind::Public => Ok(Modifier::Public),
            TokenKind::Private => Ok(Modifier::Private),
            TokenKind::Protected => Ok(Modifier::Protected),
            TokenKind::Global => Ok(Modifier::Global),
            TokenKind::Static => Ok(Modifier::Static),
            TokenKind::Virtual => Ok(Modifier::Virtual),
            TokenKind::Abstract => Ok(Modifier::Abstract),
            TokenKind::Override => Ok(Modifier::Override),
            TokenKind::Final => Ok(Modifier::Final),
            _ => Err(Diagnostic::new("expected a modifier", token.span)),
        }
    }

    pub(super) fn is_simple_modifier(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Public
                | TokenKind::Private
                | TokenKind::Protected
                | TokenKind::Global
                | TokenKind::Static
                | TokenKind::Virtual
                | TokenKind::Abstract
                | TokenKind::Override
                | TokenKind::Final
        )
    }
}
