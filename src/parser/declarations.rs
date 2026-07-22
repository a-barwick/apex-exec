use super::Parser;
use crate::{
    ast::{
        AccessorKind, Annotation, AnnotationArgument, AnnotationKind, ClassDeclaration, ClassKind,
        ClassMember, ConstructorDeclaration, ConstructorDelegation, ConstructorDelegationKind,
        Expression, FieldDeclaration, FieldGroupDeclaration, Identifier, InitializerBlock,
        MethodDeclaration, Modifier, NamedType, Parameter, PropertyAccessor, PropertyDeclaration,
        ReturnType, Statement, TriggerDeclaration, TriggerEvent, TypeName, VariableDeclarator,
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
        self.parse_type_declaration(None)
    }

    fn parse_type_declaration(
        &mut self,
        enclosing_type: Option<NamedType>,
    ) -> Result<ClassDeclaration, Diagnostic> {
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
        let kind = self.parse_class_kind()?;
        let name = self.expect_identifier("expected a type name")?;
        let qualified_spelling = enclosing_type.as_ref().map_or_else(
            || name.spelling.clone(),
            |owner| format!("{}.{}", owner.spelling, name.spelling),
        );
        let qualified_name = NamedType::new(qualified_spelling, name.span);
        let (superclass, interfaces) = self.parse_hierarchy_edges(kind)?;
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after type declaration")?;
        let (enum_constants, members) = self.parse_type_body(kind, &name, &qualified_name)?;
        let end = self.expect_simple(TokenKind::RightBrace, "expected `}` after type body")?;
        Ok(ClassDeclaration {
            annotations,
            kind,
            modifiers,
            name,
            qualified_name,
            enclosing_type,
            superclass,
            interfaces,
            enum_constants,
            members,
            span: start.merge(end.span),
        })
    }

    fn parse_class_kind(&mut self) -> Result<ClassKind, Diagnostic> {
        if self.check(&TokenKind::Class) {
            self.advance();
            Ok(ClassKind::Class)
        } else if self.check(&TokenKind::Interface) {
            self.advance();
            Ok(ClassKind::Interface)
        } else if self.check_keyword("enum") {
            self.advance();
            Ok(ClassKind::Enum)
        } else {
            Err(Diagnostic::new(
                "expected `class`, `interface`, or `enum`",
                self.current().span,
            ))
        }
    }

    fn parse_type_body(
        &mut self,
        kind: ClassKind,
        name: &Identifier,
        qualified_name: &NamedType,
    ) -> Result<(Vec<Identifier>, Vec<ClassMember>), Diagnostic> {
        let enum_constants = if kind == ClassKind::Enum {
            self.parse_enum_constants()?
        } else {
            Vec::new()
        };
        let mut members = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            if self.is_class_declaration_start() {
                let nested = self.parse_type_declaration(Some(qualified_name.clone()))?;
                self.pending_types.push(nested);
            } else if self.is_initializer_block_start() {
                members.push(ClassMember::Initializer(self.parse_initializer_block()?));
            } else {
                members.push(self.parse_class_member(name)?);
            }
        }
        Ok((enum_constants, members))
    }

    fn is_initializer_block_start(&self) -> bool {
        self.check(&TokenKind::LeftBrace)
            || (self.check(&TokenKind::Static) && matches!(self.peek(1).kind, TokenKind::LeftBrace))
    }

    fn parse_enum_constants(&mut self) -> Result<Vec<Identifier>, Diagnostic> {
        let mut constants = Vec::new();
        if self.check(&TokenKind::RightBrace) || self.check(&TokenKind::Semicolon) {
            if self.check(&TokenKind::Semicolon) {
                self.advance();
            }
            return Ok(constants);
        }
        loop {
            constants.push(self.expect_identifier("expected an enum constant")?);
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance();
            if self.check(&TokenKind::Semicolon) || self.check(&TokenKind::RightBrace) {
                break;
            }
        }
        if self.check(&TokenKind::Semicolon) {
            self.advance();
        } else if !self.check(&TokenKind::RightBrace) {
            return Err(Diagnostic::new(
                "expected `;` after enum constants",
                self.current().span,
            ));
        }
        Ok(constants)
    }

    fn parse_initializer_block(&mut self) -> Result<InitializerBlock, Diagnostic> {
        let (is_static, start) = if self.check(&TokenKind::Static) {
            (true, self.advance().span)
        } else {
            (false, self.current().span)
        };
        let body = self.parse_block()?;
        Ok(InitializerBlock {
            is_static,
            span: start.merge(body.span()),
            body,
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
        if kind != ClassKind::Interface && self.check(&TokenKind::Implements) {
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
            return self.parse_constructor(annotations, modifiers);
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
        if self.check(&TokenKind::LeftBrace) {
            return self
                .parse_property(annotations, modifiers, ty, name, start)
                .map(ClassMember::Property);
        }
        self.parse_field_declaration(annotations, modifiers, ty, name, start)
    }

    fn parse_field_declaration(
        &mut self,
        annotations: Vec<Annotation>,
        modifiers: Vec<Modifier>,
        ty: TypeName,
        name: Identifier,
        start: Span,
    ) -> Result<ClassMember, Diagnostic> {
        let first = self.parse_variable_declarator(name)?;
        if self.check(&TokenKind::Comma) {
            let mut declarators = vec![first];
            while self.check(&TokenKind::Comma) {
                self.advance();
                let name = self.expect_identifier("expected a field name after `,`")?;
                declarators.push(self.parse_variable_declarator(name)?);
            }
            let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after fields")?;
            return Ok(ClassMember::FieldGroup(FieldGroupDeclaration {
                annotations,
                modifiers,
                ty,
                declarators,
                span: start.merge(end.span),
            }));
        }
        let end = self.expect_simple(TokenKind::Semicolon, "expected `;` after field")?;
        Ok(ClassMember::Field(FieldDeclaration {
            annotations,
            modifiers,
            ty,
            name: first.name,
            initializer: first.initializer,
            span: start.merge(end.span),
        }))
    }

    fn parse_variable_declarator(
        &mut self,
        name: Identifier,
    ) -> Result<VariableDeclarator, Diagnostic> {
        let initializer = if self.check(&TokenKind::Equal) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        let end = initializer
            .as_ref()
            .map_or(name.span, |initializer| initializer.span());
        Ok(VariableDeclarator {
            span: name.span.merge(end),
            name,
            initializer,
        })
    }

    fn parse_constructor(
        &mut self,
        annotations: Vec<Annotation>,
        modifiers: Vec<Modifier>,
    ) -> Result<ClassMember, Diagnostic> {
        let name = self.expect_identifier("expected constructor name")?;
        let parameters = self.parse_parameters()?;
        let mut body = self.parse_block()?;
        let delegation = take_constructor_delegation(&mut body);
        let span = name.span.merge(body.span());
        Ok(ClassMember::Constructor(ConstructorDeclaration {
            annotations,
            modifiers,
            name,
            parameters,
            delegation,
            body,
            span,
        }))
    }

    pub(super) fn parse_property(
        &mut self,
        annotations: Vec<Annotation>,
        modifiers: Vec<Modifier>,
        ty: TypeName,
        name: Identifier,
        start: crate::span::Span,
    ) -> Result<PropertyDeclaration, Diagnostic> {
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after property name")?;
        let mut accessors = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            accessors.push(self.parse_property_accessor()?);
        }
        let end = self.expect_simple(TokenKind::RightBrace, "expected `}` after property")?;
        Ok(PropertyDeclaration {
            annotations,
            modifiers,
            ty,
            name,
            accessors,
            span: start.merge(end.span),
        })
    }

    fn parse_property_accessor(&mut self) -> Result<PropertyAccessor, Diagnostic> {
        let modifier = if matches!(
            self.current().kind,
            TokenKind::Public | TokenKind::Private | TokenKind::Protected | TokenKind::Global
        ) {
            Some(self.parse_modifier()?)
        } else {
            None
        };
        let start = self.current().span;
        let kind = match &self.current().kind {
            TokenKind::Identifier(spelling) if spelling.eq_ignore_ascii_case("get") => {
                self.advance();
                AccessorKind::Get
            }
            TokenKind::Identifier(spelling) if spelling.eq_ignore_ascii_case("set") => {
                self.advance();
                AccessorKind::Set
            }
            _ => {
                return Err(Diagnostic::new(
                    "expected `get` or `set` property accessor",
                    start,
                ));
            }
        };
        let (body, end) = if self.check(&TokenKind::Semicolon) {
            (None, self.advance().span)
        } else {
            let body = self.parse_block()?;
            let end = body.span();
            (Some(body), end)
        };
        Ok(PropertyAccessor {
            kind,
            modifier,
            body,
            span: start.merge(end),
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
            let (arguments, parenthesized) = self.parse_annotation_arguments()?;
            let kind = self.classify_annotation(&name, &arguments, parenthesized)?;
            let end = self.token_at(self.cursor.saturating_sub(1)).span.end;
            annotations.push(Annotation {
                name,
                arguments,
                kind,
                span: Span::new_in(start.source_id, start.start, end),
            });
        }
        Ok(annotations)
    }

    fn classify_annotation(
        &self,
        name: &Identifier,
        arguments: &[AnnotationArgument],
        parenthesized: bool,
    ) -> Result<AnnotationKind, Diagnostic> {
        match name.canonical.as_str() {
            "istest" => self.classify_is_test_annotation(name, arguments, parenthesized),
            "testsetup" => self.classify_argumentless_annotation(
                name,
                parenthesized,
                "`@TestSetup` does not accept arguments",
                AnnotationKind::TestSetup,
            ),
            "future" => self.classify_argumentless_annotation(
                name,
                parenthesized,
                "`@future` options are not supported",
                AnnotationKind::Future,
            ),
            "auraenabled" => self.classify_aura_enabled_annotation(arguments),
            "suppresswarnings" => {
                self.classify_suppress_warnings_annotation(name, arguments, parenthesized)
            }
            "testvisible" => self.classify_argumentless_annotation(
                name,
                parenthesized,
                "`@TestVisible` does not accept arguments",
                AnnotationKind::TestVisible,
            ),
            _ => Ok(AnnotationKind::Other),
        }
    }

    fn classify_is_test_annotation(
        &self,
        name: &Identifier,
        arguments: &[AnnotationArgument],
        parenthesized: bool,
    ) -> Result<AnnotationKind, Diagnostic> {
        if parenthesized && arguments.is_empty() {
            return Err(Diagnostic::new(
                "expected a named `@IsTest` option",
                name.span,
            ));
        }
        let mut see_all_data = None;
        let mut is_parallel = None;
        for argument in arguments {
            let Some(argument_name) = &argument.name else {
                return Err(Diagnostic::new(
                    "`@IsTest` options must be named",
                    argument.span,
                ));
            };
            let Expression::BooleanLiteral(value, _) = argument.value else {
                return Err(Diagnostic::new(
                    format!("`{}` requires a Boolean literal", argument_name.spelling),
                    argument.value.span(),
                ));
            };
            let slot = match argument_name.canonical.as_str() {
                "seealldata" => &mut see_all_data,
                "isparallel" => &mut is_parallel,
                _ => {
                    return Err(Diagnostic::new(
                        "supported `@IsTest` options are `SeeAllData` and `IsParallel`",
                        argument_name.span,
                    ));
                }
            };
            if slot.replace(value).is_some() {
                return Err(Diagnostic::new(
                    format!("duplicate `@IsTest` option `{}`", argument_name.spelling),
                    argument_name.span,
                ));
            }
        }
        Ok(AnnotationKind::IsTest {
            see_all_data,
            is_parallel,
        })
    }

    fn classify_aura_enabled_annotation(
        &self,
        arguments: &[AnnotationArgument],
    ) -> Result<AnnotationKind, Diagnostic> {
        let mut cacheable = None;
        let mut continuation = None;
        for argument in arguments {
            let Some(argument_name) = &argument.name else {
                return Err(Diagnostic::new(
                    "`@AuraEnabled` options must be named",
                    argument.span,
                ));
            };
            let Expression::BooleanLiteral(value, _) = argument.value else {
                return Err(Diagnostic::new(
                    format!("`{}` requires a Boolean literal", argument_name.spelling),
                    argument.value.span(),
                ));
            };
            let slot = match argument_name.canonical.as_str() {
                "cacheable" => &mut cacheable,
                "continuation" => &mut continuation,
                _ => {
                    return Err(Diagnostic::new(
                        "supported `@AuraEnabled` options are `cacheable` and `continuation`",
                        argument_name.span,
                    ));
                }
            };
            if slot.replace(value).is_some() {
                return Err(Diagnostic::new(
                    format!(
                        "duplicate `@AuraEnabled` option `{}`",
                        argument_name.spelling
                    ),
                    argument_name.span,
                ));
            }
        }
        Ok(AnnotationKind::AuraEnabled {
            cacheable,
            continuation,
        })
    }

    fn classify_argumentless_annotation(
        &self,
        name: &Identifier,
        parenthesized: bool,
        message: &str,
        kind: AnnotationKind,
    ) -> Result<AnnotationKind, Diagnostic> {
        if parenthesized {
            Err(Diagnostic::new(message, name.span))
        } else {
            Ok(kind)
        }
    }

    fn classify_suppress_warnings_annotation(
        &self,
        name: &Identifier,
        arguments: &[AnnotationArgument],
        parenthesized: bool,
    ) -> Result<AnnotationKind, Diagnostic> {
        if !parenthesized
            || !matches!(
                arguments,
                [AnnotationArgument {
                    name: None,
                    value: Expression::StringLiteral(_, _),
                    ..
                }]
            )
        {
            return Err(Diagnostic::new(
                "`@SuppressWarnings` requires exactly one positional String literal",
                name.span,
            ));
        }
        Ok(AnnotationKind::SuppressWarnings)
    }

    fn parse_annotation_arguments(
        &mut self,
    ) -> Result<(Vec<AnnotationArgument>, bool), Diagnostic> {
        if !self.check(&TokenKind::LeftParen) {
            return Ok((Vec::new(), false));
        }
        self.advance();
        let mut arguments = Vec::new();
        while !self.check(&TokenKind::RightParen) {
            let start = self.current().span;
            let name = if matches!(self.current().kind, TokenKind::Identifier(_))
                && matches!(self.peek(1).kind, TokenKind::Equal)
            {
                let name = self.expect_identifier("expected an annotation argument name")?;
                self.advance();
                Some(name)
            } else {
                None
            };
            let value = self.parse_expression()?;
            arguments.push(AnnotationArgument {
                name,
                span: start.merge(value.span()),
                value,
            });
            if !self.check(&TokenKind::Comma) {
                if matches!(self.current().kind, TokenKind::Identifier(_))
                    && matches!(self.peek(1).kind, TokenKind::Equal)
                {
                    continue;
                }
                break;
            }
            self.advance();
        }
        self.expect_simple(
            TokenKind::RightParen,
            "expected `)` after annotation arguments",
        )?;
        Ok((arguments, true))
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
            TokenKind::Transient => Ok(Modifier::Transient),
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
                | TokenKind::Transient
        )
    }
}

fn take_constructor_delegation(body: &mut Statement) -> Option<ConstructorDelegation> {
    let Statement::Block { statements, .. } = body else {
        return None;
    };
    let Statement::Expression {
        expression:
            Expression::FunctionCall {
                name,
                arguments,
                span,
            },
        ..
    } = statements.first()?
    else {
        return None;
    };
    let kind = match name.canonical.as_str() {
        "this" => ConstructorDelegationKind::This,
        "super" => ConstructorDelegationKind::Super,
        _ => return None,
    };
    let delegation = ConstructorDelegation {
        kind,
        arguments: arguments.clone(),
        span: *span,
    };
    statements.remove(0);
    Some(delegation)
}
