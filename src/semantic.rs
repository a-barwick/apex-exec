use crate::{
    ast::{
        AccessorKind, AssignmentTarget, BinaryOperator, CatchClause, ClassDeclaration, ClassKind,
        ClassMember, CollectionInitializer, ConstructorDeclaration, Expression, Identifier,
        MethodDeclaration, Modifier, PostfixOperator, Program, ReturnType, Statement, TypeName,
        UnaryOperator,
    },
    diagnostic::Diagnostic,
    hir::{self, CallTarget, ClassMemberId, ExpressionType, MemberTarget, ReferenceTarget},
    span::Span,
};
use std::collections::HashMap;

pub fn check(program: &Program) -> Result<hir::Program, Diagnostic> {
    Checker::new().check_program(program)
}

#[derive(Clone)]
struct MethodSignature {
    id: usize,
    parameter_types: Vec<TypeName>,
    return_type: ReturnType,
}

#[derive(Clone)]
struct ClassMethodSignature {
    target: ClassMemberId,
    name: String,
    parameter_types: Vec<TypeName>,
    return_type: ReturnType,
    modifiers: Vec<Modifier>,
}

#[derive(Clone)]
struct ClassValueMember {
    target: ClassMemberId,
    ty: TypeName,
    modifiers: Vec<Modifier>,
    read_access: Vec<Modifier>,
    write_access: Vec<Modifier>,
    readable: bool,
    writable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClassCallKind {
    Static,
    Instance,
    Super,
}

struct Checker {
    scopes: Vec<HashMap<String, TypeName>>,
    loop_depth: usize,
    return_type: Option<ReturnType>,
    methods: HashMap<String, Vec<MethodSignature>>,
    expression_types: HashMap<Span, ExpressionType>,
    calls: HashMap<Span, CallTarget>,
    references: HashMap<Span, ReferenceTarget>,
    members: HashMap<Span, MemberTarget>,
    classes: Vec<ClassDeclaration>,
    class_ids: HashMap<String, usize>,
    current_class: Option<usize>,
    current_static: bool,
}

impl Checker {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            loop_depth: 0,
            return_type: None,
            methods: HashMap::new(),
            expression_types: HashMap::new(),
            calls: HashMap::new(),
            references: HashMap::new(),
            members: HashMap::new(),
            classes: Vec::new(),
            class_ids: HashMap::new(),
            current_class: None,
            current_static: false,
        }
    }

    fn check_program(mut self, program: &Program) -> Result<hir::Program, Diagnostic> {
        self.collect_classes(program)?;
        self.collect_method_signatures(program)?;
        self.validate_class_hierarchy()?;
        for class_id in 0..self.classes.len() {
            self.check_class(class_id)?;
        }
        for method in &program.methods {
            self.check_method(method)?;
        }
        for statement in &program.statements {
            self.check_statement(statement)?;
        }
        Ok(hir::Program::new(
            program.clone(),
            self.expression_types,
            self.calls,
            self.references,
            self.members,
        ))
    }

    fn collect_classes(&mut self, program: &Program) -> Result<(), Diagnostic> {
        self.classes = program.classes.clone();
        for (class_id, class) in self.classes.iter().enumerate() {
            if let Some(previous) = self
                .class_ids
                .insert(class.name.canonical.clone(), class_id)
            {
                let original = &self.classes[previous];
                return Err(Diagnostic::new(
                    format!(
                        "duplicate type `{}`; first declared as `{}`",
                        class.name.spelling, original.name.spelling
                    ),
                    class.name.span,
                ));
            }
        }
        Ok(())
    }

    fn validate_class_hierarchy(&self) -> Result<(), Diagnostic> {
        for (class_id, class) in self.classes.iter().enumerate() {
            validate_modifier_set(&class.modifiers, class.name.span, "type")?;
            reject_modifiers(
                &class.modifiers,
                &[
                    Modifier::Private,
                    Modifier::Protected,
                    Modifier::Static,
                    Modifier::Override,
                ],
                class.name.span,
                "top-level type",
            )?;
            if class.modifiers.iter().any(|modifier| {
                matches!(
                    modifier,
                    Modifier::WithSharing | Modifier::WithoutSharing | Modifier::InheritedSharing
                )
            }) {
                return Err(Diagnostic::new(
                    "sharing modifiers are parsed but not supported by the active compatibility profile",
                    class.name.span,
                ));
            }
            if let Some(superclass) = &class.superclass {
                let parent_id = self
                    .class_ids
                    .get(&superclass.canonical)
                    .copied()
                    .ok_or_else(|| {
                        Diagnostic::new(
                            format!("unknown superclass `{}`", superclass.spelling),
                            superclass.span,
                        )
                    })?;
                let parent = &self.classes[parent_id];
                if class.kind == ClassKind::Class && parent.kind != ClassKind::Class {
                    return Err(Diagnostic::new(
                        format!("class cannot extend interface `{}`", superclass.spelling),
                        superclass.span,
                    ));
                }
                if class.kind == ClassKind::Interface && parent.kind != ClassKind::Interface {
                    return Err(Diagnostic::new(
                        format!("interface cannot extend class `{}`", superclass.spelling),
                        superclass.span,
                    ));
                }
                if parent.modifiers.contains(&Modifier::Final) {
                    return Err(Diagnostic::new(
                        format!("cannot extend final class `{}`", superclass.spelling),
                        superclass.span,
                    ));
                }
                if class.kind == ClassKind::Class
                    && !(parent.modifiers.contains(&Modifier::Virtual)
                        || parent.modifiers.contains(&Modifier::Abstract))
                {
                    return Err(Diagnostic::new(
                        format!("cannot extend non-virtual class `{}`", superclass.spelling),
                        superclass.span,
                    ));
                }
            }
            for interface in &class.interfaces {
                let interface_id = self
                    .class_ids
                    .get(&interface.canonical)
                    .copied()
                    .ok_or_else(|| {
                        Diagnostic::new(
                            format!("unknown interface `{}`", interface.spelling),
                            interface.span,
                        )
                    })?;
                if self.classes[interface_id].kind != ClassKind::Interface {
                    return Err(Diagnostic::new(
                        format!("`{}` is not an interface", interface.spelling),
                        interface.span,
                    ));
                }
            }

            let mut seen = vec![false; self.classes.len()];
            let mut cursor = Some(class_id);
            while let Some(id) = cursor {
                if seen[id] {
                    return Err(Diagnostic::new(
                        format!("cyclic inheritance involving `{}`", class.name.spelling),
                        class.name.span,
                    ));
                }
                seen[id] = true;
                cursor = self.classes[id]
                    .superclass
                    .as_ref()
                    .and_then(|name| self.class_ids.get(&name.canonical).copied());
            }
        }
        Ok(())
    }

    fn check_class(&mut self, class_id: usize) -> Result<(), Diagnostic> {
        self.validate_class_member_declarations(class_id)?;
        self.validate_class_contracts(class_id)?;

        let saved_class = self.current_class.replace(class_id);
        let saved_static = self.current_static;
        let members = self.classes[class_id].members.clone();
        let result = (|| {
            for member in &members {
                match member {
                    ClassMember::Field(field) => {
                        self.validate_type(&field.ty, field.name.span)?;
                        self.current_static = field.modifiers.contains(&Modifier::Static);
                        if let Some(initializer) = &field.initializer {
                            let actual = self.expression_type(initializer)?;
                            self.require_assignable(&field.ty, &actual, initializer.span())?;
                        }
                    }
                    ClassMember::Constructor(constructor) => {
                        self.current_static = false;
                        self.check_constructor(constructor)?;
                    }
                    ClassMember::Method(method) => {
                        self.current_static = method.modifiers.contains(&Modifier::Static);
                        self.check_method(method)?;
                    }
                    ClassMember::Property(property) => {
                        self.validate_type(&property.ty, property.name.span)?;
                        self.current_static = property.modifiers.contains(&Modifier::Static);
                        self.check_property_accessors(property)?;
                    }
                }
            }
            Ok(())
        })();
        self.current_class = saved_class;
        self.current_static = saved_static;
        result
    }

    fn validate_class_member_declarations(&self, class_id: usize) -> Result<(), Diagnostic> {
        let class = &self.classes[class_id];
        let mut values = HashMap::<String, Span>::new();
        let mut methods = HashMap::<(String, Vec<TypeName>), Span>::new();
        let mut constructors = HashMap::<Vec<TypeName>, Span>::new();
        for member in &class.members {
            match member {
                ClassMember::Field(field) => {
                    validate_modifier_set(&field.modifiers, field.name.span, "field")?;
                    reject_modifiers(
                        &field.modifiers,
                        &[Modifier::Virtual, Modifier::Abstract, Modifier::Override],
                        field.name.span,
                        "field",
                    )?;
                    if values
                        .insert(field.name.canonical.clone(), field.name.span)
                        .is_some()
                    {
                        return Err(Diagnostic::new(
                            format!("duplicate member `{}`", field.name.spelling),
                            field.name.span,
                        ));
                    }
                }
                ClassMember::Property(property) => {
                    validate_modifier_set(&property.modifiers, property.name.span, "property")?;
                    reject_modifiers(
                        &property.modifiers,
                        &[
                            Modifier::Virtual,
                            Modifier::Abstract,
                            Modifier::Override,
                            Modifier::Final,
                        ],
                        property.name.span,
                        "property",
                    )?;
                    if values
                        .insert(property.name.canonical.clone(), property.name.span)
                        .is_some()
                    {
                        return Err(Diagnostic::new(
                            format!("duplicate member `{}`", property.name.spelling),
                            property.name.span,
                        ));
                    }
                    let mut get = false;
                    let mut set = false;
                    for accessor in &property.accessors {
                        let duplicate = match accessor.kind {
                            AccessorKind::Get => std::mem::replace(&mut get, true),
                            AccessorKind::Set => std::mem::replace(&mut set, true),
                        };
                        if duplicate {
                            return Err(Diagnostic::new(
                                "duplicate property accessor",
                                accessor.span,
                            ));
                        }
                    }
                    if property.accessors.is_empty() {
                        return Err(Diagnostic::new(
                            "property requires at least one accessor",
                            property.name.span,
                        ));
                    }
                    if class.kind == ClassKind::Interface
                        && (property.modifiers.contains(&Modifier::Private)
                            || property.modifiers.contains(&Modifier::Protected))
                    {
                        return Err(Diagnostic::new(
                            "interface properties must be public or global",
                            property.name.span,
                        ));
                    }
                }
                ClassMember::Constructor(constructor) => {
                    validate_modifier_set(
                        &constructor.modifiers,
                        constructor.name.span,
                        "constructor",
                    )?;
                    reject_modifiers(
                        &constructor.modifiers,
                        &[
                            Modifier::Static,
                            Modifier::Virtual,
                            Modifier::Abstract,
                            Modifier::Override,
                            Modifier::Final,
                        ],
                        constructor.name.span,
                        "constructor",
                    )?;
                    let signature = constructor
                        .parameters
                        .iter()
                        .map(|parameter| parameter.ty.clone())
                        .collect::<Vec<_>>();
                    if constructors
                        .insert(signature, constructor.name.span)
                        .is_some()
                    {
                        return Err(Diagnostic::new(
                            "duplicate constructor overload",
                            constructor.name.span,
                        ));
                    }
                    if class.kind == ClassKind::Interface {
                        return Err(Diagnostic::new(
                            "interfaces cannot declare constructors",
                            constructor.name.span,
                        ));
                    }
                }
                ClassMember::Method(method) => {
                    validate_modifier_set(&method.modifiers, method.name.span, "method")?;
                    if method.modifiers.contains(&Modifier::Static) {
                        reject_modifiers(
                            &method.modifiers,
                            &[Modifier::Virtual, Modifier::Abstract, Modifier::Override],
                            method.name.span,
                            "static method",
                        )?;
                    }
                    if method.modifiers.contains(&Modifier::Final) {
                        reject_modifiers(
                            &method.modifiers,
                            &[Modifier::Virtual, Modifier::Abstract],
                            method.name.span,
                            "final method",
                        )?;
                    }
                    let signature = (
                        method.name.canonical.clone(),
                        method
                            .parameters
                            .iter()
                            .map(|parameter| parameter.ty.clone())
                            .collect::<Vec<_>>(),
                    );
                    if methods.insert(signature, method.name.span).is_some() {
                        return Err(Diagnostic::new(
                            format!("duplicate method overload `{}`", method.name.spelling),
                            method.name.span,
                        ));
                    }
                    let is_abstract = method.modifiers.contains(&Modifier::Abstract)
                        || class.kind == ClassKind::Interface;
                    if is_abstract && method.body.is_some() {
                        return Err(Diagnostic::new(
                            "abstract and interface methods cannot have a body",
                            method.name.span,
                        ));
                    }
                    if !is_abstract && method.body.is_none() {
                        return Err(Diagnostic::new(
                            "method without a body must be abstract",
                            method.name.span,
                        ));
                    }
                    if class.kind == ClassKind::Interface
                        && method.modifiers.contains(&Modifier::Static)
                    {
                        return Err(Diagnostic::new(
                            "interface methods cannot be static in the supported profile",
                            method.name.span,
                        ));
                    }
                    if class.kind == ClassKind::Interface
                        && (method.modifiers.contains(&Modifier::Private)
                            || method.modifiers.contains(&Modifier::Protected))
                    {
                        return Err(Diagnostic::new(
                            "interface methods must be public or global",
                            method.name.span,
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_class_contracts(&self, class_id: usize) -> Result<(), Diagnostic> {
        let class = &self.classes[class_id];
        for signature in self.own_class_methods(class_id) {
            let method = self.method_declaration(signature.target);
            let inherited = self
                .parent_class_id(class_id)
                .and_then(|parent| self.find_matching_method(parent, method, true));
            if method.modifiers.contains(&Modifier::Override) {
                let Some(base) = inherited else {
                    return Err(Diagnostic::new(
                        format!(
                            "method `{}` does not override an inherited method",
                            method.name.spelling
                        ),
                        method.name.span,
                    ));
                };
                let base_method = self.method_declaration(base.target);
                if base_method.modifiers.contains(&Modifier::Static)
                    || !(base_method.modifiers.contains(&Modifier::Virtual)
                        || base_method.modifiers.contains(&Modifier::Abstract)
                        || self.classes[base.target.class_id].kind == ClassKind::Interface)
                {
                    return Err(Diagnostic::new(
                        format!(
                            "method `{}` overrides a non-virtual method",
                            method.name.spelling
                        ),
                        method.name.span,
                    ));
                }
                if method.return_type != base_method.return_type {
                    return Err(Diagnostic::new(
                        "override return type must match the inherited method",
                        method.name.span,
                    ));
                }
                if access_rank(&method.modifiers) < access_rank(&base_method.modifiers) {
                    return Err(Diagnostic::new(
                        "override cannot reduce inherited method visibility",
                        method.name.span,
                    ));
                }
            } else if inherited.is_some()
                && !method.modifiers.contains(&Modifier::Static)
                && class.kind == ClassKind::Class
            {
                return Err(Diagnostic::new(
                    format!("method `{}` must use `override`", method.name.spelling),
                    method.name.span,
                ));
            }
        }

        if class.kind == ClassKind::Class && !class.modifiers.contains(&Modifier::Abstract) {
            for required in self.required_abstract_methods(class_id) {
                if self
                    .find_concrete_implementation(class_id, &required)
                    .is_none()
                {
                    let method = self.method_declaration(required.target);
                    return Err(Diagnostic::new(
                        format!(
                            "non-abstract class `{}` must implement method `{}`",
                            class.name.spelling, method.name.spelling
                        ),
                        class.name.span,
                    ));
                }
            }
        }
        Ok(())
    }

    fn parent_class_id(&self, class_id: usize) -> Option<usize> {
        self.classes[class_id]
            .superclass
            .as_ref()
            .and_then(|name| self.class_ids.get(&name.canonical).copied())
    }

    fn own_class_methods(&self, class_id: usize) -> Vec<ClassMethodSignature> {
        self.classes[class_id]
            .members
            .iter()
            .enumerate()
            .filter_map(|(member_id, member)| {
                let ClassMember::Method(method) = member else {
                    return None;
                };
                let mut modifiers = method.modifiers.clone();
                if self.classes[class_id].kind == ClassKind::Interface
                    && access_rank(&modifiers) == 0
                {
                    modifiers.push(Modifier::Public);
                }
                Some(ClassMethodSignature {
                    target: ClassMemberId {
                        class_id,
                        member_id,
                    },
                    name: method.name.canonical.clone(),
                    parameter_types: method
                        .parameters
                        .iter()
                        .map(|parameter| parameter.ty.clone())
                        .collect(),
                    return_type: method.return_type.clone(),
                    modifiers,
                })
            })
            .collect()
    }

    fn class_methods_named(&self, class_id: usize, canonical: &str) -> Vec<ClassMethodSignature> {
        let mut methods = Vec::new();
        let mut signatures = Vec::<Vec<TypeName>>::new();
        let mut cursor = Some(class_id);
        while let Some(id) = cursor {
            for signature in self.own_class_methods(id).into_iter().filter(|signature| {
                self.method_declaration(signature.target).name.canonical == canonical
            }) {
                if !signatures.contains(&signature.parameter_types) {
                    signatures.push(signature.parameter_types.clone());
                    methods.push(signature);
                }
            }
            cursor = self.parent_class_id(id);
        }
        methods
    }

    fn method_declaration(&self, target: ClassMemberId) -> &MethodDeclaration {
        let ClassMember::Method(method) = &self.classes[target.class_id].members[target.member_id]
        else {
            unreachable!("method target must refer to a method")
        };
        method
    }

    fn find_matching_method(
        &self,
        class_id: usize,
        method: &MethodDeclaration,
        include_abstract: bool,
    ) -> Option<ClassMethodSignature> {
        self.class_methods_named(class_id, &method.name.canonical)
            .into_iter()
            .find(|candidate| {
                candidate.parameter_types
                    == method
                        .parameters
                        .iter()
                        .map(|parameter| parameter.ty.clone())
                        .collect::<Vec<_>>()
                    && (include_abstract
                        || self.method_declaration(candidate.target).body.is_some())
            })
    }

    fn required_abstract_methods(&self, class_id: usize) -> Vec<ClassMethodSignature> {
        let mut required = Vec::new();
        let mut cursor = Some(class_id);
        while let Some(id) = cursor {
            for method in self.own_class_methods(id) {
                if self.method_declaration(method.target).body.is_none() {
                    push_unique_signature(&mut required, method);
                }
            }
            for interface in &self.classes[id].interfaces {
                let interface_id = self.class_ids[&interface.canonical];
                self.collect_interface_methods(interface_id, &mut required);
            }
            cursor = self.parent_class_id(id);
        }
        required
    }

    fn collect_interface_methods(
        &self,
        interface_id: usize,
        required: &mut Vec<ClassMethodSignature>,
    ) {
        for method in self.own_class_methods(interface_id) {
            push_unique_signature(required, method);
        }
        if let Some(parent) = self.parent_class_id(interface_id) {
            self.collect_interface_methods(parent, required);
        }
        for interface in &self.classes[interface_id].interfaces {
            self.collect_interface_methods(self.class_ids[&interface.canonical], required);
        }
    }

    fn find_concrete_implementation(
        &self,
        class_id: usize,
        required: &ClassMethodSignature,
    ) -> Option<ClassMethodSignature> {
        let required_method = self.method_declaration(required.target);
        self.class_methods_named(class_id, &required_method.name.canonical)
            .into_iter()
            .find(|candidate| {
                candidate.parameter_types == required.parameter_types
                    && candidate.return_type == required.return_type
                    && self.method_declaration(candidate.target).body.is_some()
                    && !candidate.modifiers.contains(&Modifier::Abstract)
                    && access_rank(&candidate.modifiers) >= access_rank(&required.modifiers)
            })
    }

    fn class_value_member(&self, class_id: usize, canonical: &str) -> Option<ClassValueMember> {
        let mut cursor = Some(class_id);
        while let Some(id) = cursor {
            for (member_id, member) in self.classes[id].members.iter().enumerate() {
                let target = ClassMemberId {
                    class_id: id,
                    member_id,
                };
                match member {
                    ClassMember::Field(field) if field.name.canonical == canonical => {
                        return Some(ClassValueMember {
                            target,
                            ty: field.ty.clone(),
                            modifiers: field.modifiers.clone(),
                            read_access: field.modifiers.clone(),
                            write_access: field.modifiers.clone(),
                            readable: true,
                            writable: !field.modifiers.contains(&Modifier::Final),
                        });
                    }
                    ClassMember::Property(property) if property.name.canonical == canonical => {
                        return Some(ClassValueMember {
                            target,
                            ty: property.ty.clone(),
                            modifiers: property.modifiers.clone(),
                            read_access: property
                                .accessors
                                .iter()
                                .find(|accessor| accessor.kind == AccessorKind::Get)
                                .and_then(|accessor| accessor.modifier)
                                .map_or_else(
                                    || property.modifiers.clone(),
                                    |modifier| vec![modifier],
                                ),
                            write_access: property
                                .accessors
                                .iter()
                                .find(|accessor| accessor.kind == AccessorKind::Set)
                                .and_then(|accessor| accessor.modifier)
                                .map_or_else(
                                    || property.modifiers.clone(),
                                    |modifier| vec![modifier],
                                ),
                            readable: property
                                .accessors
                                .iter()
                                .any(|accessor| accessor.kind == AccessorKind::Get),
                            writable: property
                                .accessors
                                .iter()
                                .any(|accessor| accessor.kind == AccessorKind::Set),
                        });
                    }
                    _ => {}
                }
            }
            cursor = self.parent_class_id(id);
        }
        None
    }

    fn validate_type(&self, ty: &TypeName, span: Span) -> Result<(), Diagnostic> {
        match ty {
            TypeName::Custom(name) if !self.class_ids.contains_key(&name.canonical) => Err(
                Diagnostic::new(format!("unknown type `{}`", name.spelling), span),
            ),
            TypeName::List(element) | TypeName::Set(element) => self.validate_type(element, span),
            TypeName::Map(key, value) => {
                self.validate_type(key, span)?;
                self.validate_type(value, span)
            }
            _ => Ok(()),
        }
    }

    fn is_assignable(&self, expected: &TypeName, actual: &ExpressionType) -> bool {
        match actual {
            ExpressionType::Value(actual) => self.is_subtype(actual, expected),
            ExpressionType::Null => true,
            ExpressionType::Void => false,
        }
    }

    fn is_subtype(&self, actual: &TypeName, expected: &TypeName) -> bool {
        if actual == expected || *expected == TypeName::Object {
            return true;
        }
        if *expected == TypeName::Exception && actual.is_exception() {
            return true;
        }
        let (TypeName::Custom(actual), TypeName::Custom(expected)) = (actual, expected) else {
            return false;
        };
        let Some(actual_id) = self.class_ids.get(&actual.canonical).copied() else {
            return false;
        };
        let Some(expected_id) = self.class_ids.get(&expected.canonical).copied() else {
            return false;
        };
        self.class_is_or_inherits(actual_id, expected_id)
    }

    fn class_is_or_inherits(&self, actual_id: usize, expected_id: usize) -> bool {
        if actual_id == expected_id {
            return true;
        }
        if self
            .parent_class_id(actual_id)
            .is_some_and(|parent| self.class_is_or_inherits(parent, expected_id))
        {
            return true;
        }
        self.classes[actual_id].interfaces.iter().any(|interface| {
            self.class_is_or_inherits(self.class_ids[&interface.canonical], expected_id)
        })
    }

    fn require_assignable(
        &self,
        expected: &TypeName,
        actual: &ExpressionType,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if self.is_assignable(expected, actual) {
            Ok(())
        } else {
            Err(Diagnostic::new(
                format!(
                    "cannot assign {} to {}",
                    actual.name(),
                    expected.apex_name()
                ),
                span,
            ))
        }
    }

    fn check_constructor(
        &mut self,
        constructor: &ConstructorDeclaration,
    ) -> Result<(), Diagnostic> {
        let saved_scopes = std::mem::replace(&mut self.scopes, vec![HashMap::new()]);
        let saved_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let saved_return_type = self.return_type.replace(ReturnType::Void);
        let result = (|| {
            self.bind_parameters(&constructor.parameters)?;
            self.check_method_body(&constructor.body)
        })();
        self.scopes = saved_scopes;
        self.loop_depth = saved_loop_depth;
        self.return_type = saved_return_type;
        result
    }

    fn check_property_accessors(
        &mut self,
        property: &crate::ast::PropertyDeclaration,
    ) -> Result<(), Diagnostic> {
        for accessor in &property.accessors {
            let Some(body) = &accessor.body else {
                continue;
            };
            let saved_scopes = std::mem::replace(&mut self.scopes, vec![HashMap::new()]);
            let saved_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
            let saved_return_type = self.return_type.replace(match accessor.kind {
                AccessorKind::Get => ReturnType::Value(property.ty.clone()),
                AccessorKind::Set => ReturnType::Void,
            });
            if accessor.kind == AccessorKind::Set {
                self.current_scope_mut()
                    .insert("value".to_owned(), property.ty.clone());
            }
            let result = self.check_method_body(body);
            self.scopes = saved_scopes;
            self.loop_depth = saved_loop_depth;
            self.return_type = saved_return_type;
            result?;
            if accessor.kind == AccessorKind::Get && !statement_definitely_returns_or_throws(body) {
                return Err(Diagnostic::new(
                    format!(
                        "getter `{}` must return a value on every path",
                        property.name.spelling
                    ),
                    accessor.span,
                ));
            }
        }
        Ok(())
    }

    fn bind_parameters(&mut self, parameters: &[crate::ast::Parameter]) -> Result<(), Diagnostic> {
        for parameter in parameters {
            self.validate_type(&parameter.ty, parameter.span)?;
            if self.current_scope().contains_key(&parameter.name.canonical) {
                return Err(Diagnostic::new(
                    format!("duplicate parameter `{}`", parameter.name.spelling),
                    parameter.name.span,
                ));
            }
            self.current_scope_mut()
                .insert(parameter.name.canonical.clone(), parameter.ty.clone());
        }
        Ok(())
    }

    fn collect_method_signatures(&mut self, program: &Program) -> Result<(), Diagnostic> {
        for (id, method) in program.methods.iter().enumerate() {
            let parameter_types = method
                .parameters
                .iter()
                .map(|parameter| parameter.ty.clone())
                .collect::<Vec<_>>();
            let overloads = self
                .methods
                .entry(method.name.canonical.clone())
                .or_default();
            if overloads
                .iter()
                .any(|overload| overload.parameter_types == parameter_types)
            {
                return Err(Diagnostic::new(
                    format!(
                        "duplicate method overload `{}`({})",
                        method.name.spelling,
                        parameter_types
                            .iter()
                            .map(TypeName::apex_name)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    method.name.span,
                ));
            }
            overloads.push(MethodSignature {
                id,
                parameter_types,
                return_type: method.return_type.clone(),
            });
        }
        Ok(())
    }

    fn check_method(&mut self, method: &MethodDeclaration) -> Result<(), Diagnostic> {
        self.validate_return_type(&method.return_type, method.name.span)?;
        let Some(body) = method.body.as_ref() else {
            return Ok(());
        };
        let saved_scopes = std::mem::replace(&mut self.scopes, vec![HashMap::new()]);
        let saved_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let saved_return_type = self.return_type.replace(method.return_type.clone());

        let result = (|| {
            self.bind_parameters(&method.parameters)?;

            self.check_method_body(body)?;
            if matches!(method.return_type, ReturnType::Value(_))
                && !statement_definitely_returns_or_throws(body)
            {
                return Err(Diagnostic::new(
                    format!(
                        "method `{}` must return a value on every path",
                        method.name.spelling
                    ),
                    method.name.span,
                ));
            }
            Ok(())
        })();

        self.scopes = saved_scopes;
        self.loop_depth = saved_loop_depth;
        self.return_type = saved_return_type;
        result
    }

    fn validate_return_type(&self, ty: &ReturnType, span: Span) -> Result<(), Diagnostic> {
        match ty {
            ReturnType::Void => Ok(()),
            ReturnType::Value(ty) => self.validate_type(ty, span),
        }
    }

    fn check_method_body(&mut self, body: &Statement) -> Result<(), Diagnostic> {
        if let Statement::Block { statements, .. } = body {
            for statement in statements {
                self.check_statement(statement)?;
            }
            Ok(())
        } else {
            self.check_statement(body)
        }
    }

    fn check_statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match statement {
            Statement::VariableDeclaration {
                ty,
                name,
                initializer,
                ..
            } => {
                self.validate_type(ty, name.span)?;
                if self.current_scope().contains_key(&name.canonical) {
                    return Err(Diagnostic::new(
                        format!("duplicate variable `{}`", name.spelling),
                        name.span,
                    ));
                }
                let initializer_type = self.expression_type(initializer)?;
                self.require_assignable(ty, &initializer_type, initializer.span())?;
                self.current_scope_mut()
                    .insert(name.canonical.clone(), ty.clone());
                Ok(())
            }
            Statement::Expression { expression, .. } => {
                if !is_statement_expression(expression) {
                    return Err(Diagnostic::new(
                        "only assignment, method-call, and increment/decrement expressions may be statements",
                        expression.span(),
                    ));
                }
                self.expression_type(expression)?;
                Ok(())
            }
            Statement::Block { statements, .. } => self.with_scope(|checker| {
                for statement in statements {
                    checker.check_statement(statement)?;
                }
                Ok(())
            }),
            Statement::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.require_boolean(condition)?;
                self.check_statement(then_branch)?;
                if let Some(else_branch) = else_branch {
                    self.check_statement(else_branch)?;
                }
                Ok(())
            }
            Statement::While {
                condition, body, ..
            } => {
                self.require_boolean(condition)?;
                self.with_loop(|checker| checker.check_statement(body))
            }
            Statement::DoWhile {
                body, condition, ..
            } => {
                self.with_loop(|checker| checker.check_statement(body))?;
                self.require_boolean(condition)
            }
            Statement::For {
                initializer,
                condition,
                update,
                body,
                ..
            } => self.with_scope(|checker| {
                if let Some(initializer) = initializer {
                    checker.check_statement(initializer)?;
                }
                if let Some(condition) = condition {
                    checker.require_boolean(condition)?;
                }
                checker.with_loop(|checker| {
                    checker.check_statement(body)?;
                    if let Some(update) = update {
                        checker.check_statement(update)?;
                    }
                    Ok(())
                })
            }),
            Statement::ForEach {
                element_type,
                name,
                iterable,
                body,
                ..
            } => {
                let iterable_type = self.expression_type(iterable)?;
                let actual_element_type = match iterable_type {
                    ExpressionType::Value(TypeName::List(element))
                    | ExpressionType::Value(TypeName::Set(element)) => *element,
                    other => {
                        return Err(Diagnostic::new(
                            format!(
                                "enhanced for-loop requires List or Set, found {}",
                                other.name()
                            ),
                            iterable.span(),
                        ));
                    }
                };
                self.require_assignable(
                    element_type,
                    &ExpressionType::Value(actual_element_type),
                    iterable.span(),
                )?;
                self.with_scope(|checker| {
                    checker
                        .current_scope_mut()
                        .insert(name.canonical.clone(), element_type.clone());
                    checker.with_loop(|checker| checker.check_statement(body))
                })
            }
            Statement::Break { span } => {
                if self.loop_depth == 0 {
                    Err(Diagnostic::new(
                        "`break` is only valid inside a loop",
                        *span,
                    ))
                } else {
                    Ok(())
                }
            }
            Statement::Continue { span } => {
                if self.loop_depth == 0 {
                    Err(Diagnostic::new(
                        "`continue` is only valid inside a loop",
                        *span,
                    ))
                } else {
                    Ok(())
                }
            }
            Statement::Try {
                try_block,
                catches,
                finally_block,
                ..
            } => {
                self.check_statement(try_block)?;
                self.check_catches(catches)?;
                if let Some(finally_block) = finally_block {
                    self.check_statement(finally_block)?;
                }
                Ok(())
            }
            Statement::Throw { value, .. } => {
                let actual = self.expression_type(value)?;
                if matches!(&actual, ExpressionType::Value(ty) if ty.is_exception())
                    || actual == ExpressionType::Null
                {
                    Ok(())
                } else {
                    Err(Diagnostic::new(
                        format!("`throw` requires an Exception, found {}", actual.name()),
                        value.span(),
                    ))
                }
            }
            Statement::Return { value, span } => self.check_return(value.as_ref(), *span),
        }
    }

    fn check_catches(&mut self, catches: &[CatchClause]) -> Result<(), Diagnostic> {
        let mut catches_everything = false;
        let mut seen = Vec::new();
        for catch in catches {
            if !catch.exception_type.is_exception() {
                return Err(Diagnostic::new(
                    format!(
                        "catch type must be an Exception, found {}",
                        catch.exception_type.apex_name()
                    ),
                    catch.span,
                ));
            }
            if catches_everything || seen.contains(&catch.exception_type) {
                return Err(Diagnostic::new(
                    format!("unreachable catch for {}", catch.exception_type.apex_name()),
                    catch.span,
                ));
            }
            catches_everything = catch.exception_type == TypeName::Exception;
            seen.push(catch.exception_type.clone());

            self.with_scope(|checker| {
                checker
                    .current_scope_mut()
                    .insert(catch.name.canonical.clone(), catch.exception_type.clone());
                checker.check_method_body(&catch.body)
            })?;
        }
        Ok(())
    }

    fn check_return(
        &mut self,
        value: Option<&Expression>,
        return_span: Span,
    ) -> Result<(), Diagnostic> {
        let return_type = self.return_type.clone();
        match (return_type, value) {
            (None, None) | (Some(ReturnType::Void), None) => Ok(()),
            (None, Some(value)) => Err(Diagnostic::new(
                "anonymous execution does not support returning a value",
                value.span(),
            )),
            (Some(ReturnType::Void), Some(value)) => Err(Diagnostic::new(
                "void method cannot return a value",
                value.span(),
            )),
            (Some(ReturnType::Value(expected)), None) => Err(Diagnostic::new(
                format!("return requires a {} value", expected.apex_name()),
                return_span,
            )),
            (Some(ReturnType::Value(expected)), Some(value)) => {
                let actual = self.expression_type(value)?;
                if self.is_assignable(&expected, &actual) {
                    Ok(())
                } else {
                    Err(Diagnostic::new(
                        format!(
                            "cannot return {} from a method returning {}",
                            actual.name(),
                            expected.apex_name()
                        ),
                        value.span(),
                    ))
                }
            }
        }
    }

    fn expression_type(&mut self, expression: &Expression) -> Result<ExpressionType, Diagnostic> {
        let ty = self.expression_type_inner(expression)?;
        self.expression_types.insert(expression.span(), ty.clone());
        Ok(ty)
    }

    fn expression_type_inner(
        &mut self,
        expression: &Expression,
    ) -> Result<ExpressionType, Diagnostic> {
        match expression {
            Expression::StringLiteral(..) => Ok(ExpressionType::Value(TypeName::String)),
            Expression::BooleanLiteral(..) => Ok(ExpressionType::Value(TypeName::Boolean)),
            Expression::IntegerLiteral(..) => Ok(ExpressionType::Value(TypeName::Integer)),
            Expression::NullLiteral(..) => Ok(ExpressionType::Null),
            Expression::Variable(identifier) => self.variable_type(identifier),
            Expression::Assignment { target, value, .. } => {
                let expected = self.assignment_target_type(target)?;
                let actual = self.expression_type(value)?;
                self.require_assignable(&expected, &actual, value.span())?;
                Ok(ExpressionType::Value(expected))
            }
            Expression::NewCollection {
                ty, initializer, ..
            } => self.new_collection_type(ty, initializer),
            Expression::NewException {
                exception_type,
                arguments,
                ..
            } => self.new_exception_type(exception_type, arguments),
            Expression::NewObject {
                ty,
                arguments,
                span,
            } => self.new_object_type(ty, arguments, *span),
            Expression::Index {
                collection, index, ..
            } => self
                .index_type(collection, index)
                .map(ExpressionType::Value),
            Expression::FunctionCall {
                name,
                arguments,
                span,
            } => self.function_call_type(name, arguments, *span),
            Expression::MethodCall {
                receiver,
                method,
                arguments,
                span,
            } => self.method_call_type(receiver, method, arguments, *span),
            Expression::MemberAccess {
                receiver,
                member,
                span,
            } => self.member_access_type(receiver, member, *span, false),
            Expression::Cast { ty, expression, .. } => self.cast_type(ty, expression),
            Expression::Unary {
                operator,
                operand,
                operator_span,
                ..
            } => self.unary_type(*operator, operand, *operator_span),
            Expression::Postfix {
                operand,
                operator,
                operator_span,
                ..
            } => self.postfix_type(operand, *operator, *operator_span),
            Expression::Binary {
                left,
                operator,
                right,
                operator_span,
                ..
            } => self.binary_type(left, *operator, right, *operator_span),
        }
    }

    fn variable_type(&mut self, identifier: &Identifier) -> Result<ExpressionType, Diagnostic> {
        if let Some(ty) = self.lookup(&identifier.canonical).cloned() {
            self.references
                .insert(identifier.span, ReferenceTarget::Local);
            return Ok(ExpressionType::Value(ty));
        }
        let Some(class_id) = self.current_class else {
            return Err(unknown_variable(identifier));
        };
        if identifier.canonical == "this" {
            if self.current_static {
                return Err(Diagnostic::new(
                    "`this` is unavailable in a static context",
                    identifier.span,
                ));
            }
            self.references
                .insert(identifier.span, ReferenceTarget::This);
            return Ok(ExpressionType::Value(self.class_type(class_id)));
        }
        if identifier.canonical == "super" {
            if self.current_static {
                return Err(Diagnostic::new(
                    "`super` is unavailable in a static context",
                    identifier.span,
                ));
            }
            let parent = self
                .parent_class_id(class_id)
                .ok_or_else(|| Diagnostic::new("class has no superclass", identifier.span))?;
            self.references
                .insert(identifier.span, ReferenceTarget::Super(parent));
            return Ok(ExpressionType::Value(self.class_type(parent)));
        }
        let Some(member) = self.class_value_member(class_id, &identifier.canonical) else {
            return Err(unknown_variable(identifier));
        };
        self.ensure_member_access(member.target, &member.read_access, identifier.span)?;
        if !member.readable {
            return Err(Diagnostic::new(
                format!("property `{}` is write-only", identifier.spelling),
                identifier.span,
            ));
        }
        let is_static = member.modifiers.contains(&Modifier::Static);
        if self.current_static && !is_static {
            return Err(Diagnostic::new(
                format!(
                    "instance member `{}` is unavailable in a static context",
                    identifier.spelling
                ),
                identifier.span,
            ));
        }
        self.references.insert(
            identifier.span,
            if is_static {
                ReferenceTarget::StaticMember(member.target)
            } else {
                ReferenceTarget::InstanceMember(member.target)
            },
        );
        Ok(ExpressionType::Value(member.ty))
    }

    fn new_object_type(
        &mut self,
        ty: &TypeName,
        arguments: &[Expression],
        span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        self.validate_type(ty, span)?;
        let TypeName::Custom(name) = ty else {
            return Err(Diagnostic::new(
                "object construction requires a class type",
                span,
            ));
        };
        let class_id = self.class_ids[&name.canonical];
        let class = &self.classes[class_id];
        if class.kind == ClassKind::Interface || class.modifiers.contains(&Modifier::Abstract) {
            return Err(Diagnostic::new(
                format!("cannot construct abstract type `{}`", name.spelling),
                span,
            ));
        }
        let constructors = class
            .members
            .iter()
            .enumerate()
            .filter_map(|(member_id, member)| {
                let ClassMember::Constructor(constructor) = member else {
                    return None;
                };
                Some((
                    ClassMemberId {
                        class_id,
                        member_id,
                    },
                    constructor.clone(),
                ))
            })
            .collect::<Vec<_>>();
        if constructors.is_empty() {
            for argument in arguments {
                self.expression_type(argument)?;
            }
            if !arguments.is_empty() {
                return Err(Diagnostic::new(
                    format!(
                        "default constructor for `{}` expects no arguments",
                        name.spelling
                    ),
                    arguments[0].span(),
                ));
            }
            self.calls.insert(
                span,
                CallTarget::Constructor {
                    class_id,
                    member_id: None,
                },
            );
            return Ok(ExpressionType::Value(ty.clone()));
        }
        let argument_types = arguments
            .iter()
            .map(|argument| self.expression_type(argument))
            .collect::<Result<Vec<_>, _>>()?;
        let applicable = constructors
            .iter()
            .filter(|(_, constructor)| {
                constructor.parameters.len() == argument_types.len()
                    && constructor
                        .parameters
                        .iter()
                        .zip(&argument_types)
                        .all(|(parameter, actual)| self.is_assignable(&parameter.ty, actual))
            })
            .collect::<Vec<_>>();
        let selected = self.select_constructor(&applicable).ok_or_else(|| {
            Diagnostic::new(
                if applicable.is_empty() {
                    format!("no matching constructor for `{}`", name.spelling)
                } else {
                    format!("ambiguous constructor for `{}`", name.spelling)
                },
                span,
            )
        })?;
        self.ensure_member_access(selected.0, &selected.1.modifiers, span)?;
        self.calls.insert(
            span,
            CallTarget::Constructor {
                class_id,
                member_id: Some(selected.0.member_id),
            },
        );
        Ok(ExpressionType::Value(ty.clone()))
    }

    fn select_constructor<'a>(
        &self,
        applicable: &[&'a (ClassMemberId, ConstructorDeclaration)],
    ) -> Option<&'a (ClassMemberId, ConstructorDeclaration)> {
        let most_specific = applicable
            .iter()
            .copied()
            .filter(|candidate| {
                !applicable.iter().copied().any(|other| {
                    other.0 != candidate.0
                        && self.parameter_types_more_specific(
                            &other
                                .1
                                .parameters
                                .iter()
                                .map(|parameter| parameter.ty.clone())
                                .collect::<Vec<_>>(),
                            &candidate
                                .1
                                .parameters
                                .iter()
                                .map(|parameter| parameter.ty.clone())
                                .collect::<Vec<_>>(),
                        )
                })
            })
            .collect::<Vec<_>>();
        let [selected] = most_specific.as_slice() else {
            return None;
        };
        Some(*selected)
    }

    fn member_access_type(
        &mut self,
        receiver: &Expression,
        name: &Identifier,
        span: Span,
        for_write: bool,
    ) -> Result<ExpressionType, Diagnostic> {
        let (class_id, static_access) = if let Expression::Variable(identifier) = receiver {
            if let Some(class_id) = self.class_ids.get(&identifier.canonical).copied()
                && (self
                    .class_value_member(class_id, &name.canonical)
                    .is_some_and(|member| member.modifiers.contains(&Modifier::Static))
                    || (self.lookup(&identifier.canonical).is_none()
                        && self
                            .current_class
                            .and_then(|id| self.class_value_member(id, &identifier.canonical))
                            .is_none()))
            {
                (class_id, true)
            } else {
                let receiver_type = self.expression_type(receiver)?;
                (
                    self.class_id_from_expression(&receiver_type, receiver.span())?,
                    false,
                )
            }
        } else {
            let receiver_type = self.expression_type(receiver)?;
            (
                self.class_id_from_expression(&receiver_type, receiver.span())?,
                false,
            )
        };
        let member = self
            .class_value_member(class_id, &name.canonical)
            .ok_or_else(|| {
                Diagnostic::new(
                    format!(
                        "unknown member `{}` on {}",
                        name.spelling, self.classes[class_id].name.spelling
                    ),
                    name.span,
                )
            })?;
        self.ensure_member_access(
            member.target,
            if for_write {
                &member.write_access
            } else {
                &member.read_access
            },
            name.span,
        )?;
        let is_static = member.modifiers.contains(&Modifier::Static);
        if static_access != is_static {
            return Err(Diagnostic::new(
                if static_access {
                    format!("instance member `{}` requires an object", name.spelling)
                } else {
                    format!(
                        "static member `{}` must be accessed through its class",
                        name.spelling
                    )
                },
                name.span,
            ));
        }
        if for_write && !member.writable {
            return Err(Diagnostic::new(
                format!("member `{}` is read-only", name.spelling),
                name.span,
            ));
        }
        if !for_write && !member.readable {
            return Err(Diagnostic::new(
                format!("member `{}` is write-only", name.spelling),
                name.span,
            ));
        }
        self.members.insert(
            span,
            if is_static {
                MemberTarget::Static(member.target)
            } else {
                MemberTarget::Instance(member.target)
            },
        );
        Ok(ExpressionType::Value(member.ty))
    }

    fn class_id_from_expression(
        &self,
        ty: &ExpressionType,
        span: Span,
    ) -> Result<usize, Diagnostic> {
        let ExpressionType::Value(TypeName::Custom(name)) = ty else {
            return Err(Diagnostic::new(
                format!(
                    "member access requires a class instance, found {}",
                    ty.name()
                ),
                span,
            ));
        };
        Ok(self.class_ids[&name.canonical])
    }

    fn class_type(&self, class_id: usize) -> TypeName {
        let class = &self.classes[class_id];
        TypeName::Custom(crate::ast::NamedType::new(
            class.name.spelling.clone(),
            class.name.span,
        ))
    }

    fn ensure_member_access(
        &self,
        target: ClassMemberId,
        modifiers: &[Modifier],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some(accessing) = self.current_class else {
            if access_rank(modifiers) >= access_rank(&[Modifier::Public]) {
                return Ok(());
            }
            return Err(Diagnostic::new("member is not accessible", span));
        };
        if accessing == target.class_id
            || access_rank(modifiers) >= access_rank(&[Modifier::Public])
            || (modifiers.contains(&Modifier::Protected)
                && self.class_is_or_inherits(accessing, target.class_id))
        {
            Ok(())
        } else {
            Err(Diagnostic::new("member is not accessible", span))
        }
    }

    fn parameter_types_more_specific(&self, left: &[TypeName], right: &[TypeName]) -> bool {
        let mut strict = false;
        for (left, right) in left.iter().zip(right) {
            if left == right {
                continue;
            }
            if self.is_subtype(left, right) {
                strict = true;
            } else {
                return false;
            }
        }
        strict
    }

    fn new_exception_type(
        &mut self,
        exception_type: &TypeName,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        if !exception_type.is_exception() {
            return Err(Diagnostic::new(
                format!("{} is not an Exception type", exception_type.apex_name()),
                arguments.first().map_or(Span::new(0, 0), Expression::span),
            ));
        }
        if arguments.len() > 1 {
            return Err(Diagnostic::new(
                "exception constructor expects zero or one argument",
                arguments[1].span(),
            ));
        }
        if let Some(message) = arguments.first() {
            let actual = self.expression_type(message)?;
            self.require_assignable(&TypeName::String, &actual, message.span())?;
        }
        Ok(ExpressionType::Value(exception_type.clone()))
    }

    fn function_call_type(
        &mut self,
        name: &Identifier,
        arguments: &[Expression],
        span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let argument_types = arguments
            .iter()
            .map(|argument| self.expression_type(argument))
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(class_id) = self.current_class {
            let candidates = self.class_methods_named(class_id, &name.canonical);
            if !candidates.is_empty() {
                let kind = if self.current_static
                    || candidates
                        .iter()
                        .all(|candidate| candidate.modifiers.contains(&Modifier::Static))
                {
                    ClassCallKind::Static
                } else {
                    ClassCallKind::Instance
                };
                return self.select_class_method_call(
                    class_id,
                    name,
                    &argument_types,
                    candidates,
                    kind,
                    span,
                );
            }
        }
        let Some(overloads) = self.methods.get(&name.canonical) else {
            return Err(Diagnostic::new(
                format!("unknown method `{}`", name.spelling),
                name.span,
            ));
        };

        let applicable = overloads
            .iter()
            .filter(|overload| overload.parameter_types.len() == argument_types.len())
            .filter(|overload| {
                overload
                    .parameter_types
                    .iter()
                    .zip(&argument_types)
                    .all(|(expected, actual)| self.is_assignable(expected, actual))
            })
            .collect::<Vec<_>>();

        if applicable.is_empty() {
            return Err(Diagnostic::new(
                format!(
                    "no matching overload for method `{}` with argument types ({})",
                    name.spelling,
                    argument_types
                        .iter()
                        .map(ExpressionType::name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                name.span,
            ));
        }

        let most_specific = applicable
            .iter()
            .copied()
            .filter(|candidate| {
                !applicable
                    .iter()
                    .copied()
                    .any(|other| other.id != candidate.id && method_more_specific(other, candidate))
            })
            .collect::<Vec<_>>();
        let [best] = most_specific.as_slice() else {
            return Err(Diagnostic::new(
                format!("ambiguous overload for method `{}`", name.spelling),
                name.span,
            ));
        };

        self.calls.insert(span, CallTarget::TopLevelMethod(best.id));
        Ok(match &best.return_type {
            ReturnType::Void => ExpressionType::Void,
            ReturnType::Value(ty) => ExpressionType::Value(ty.clone()),
        })
    }

    fn cast_type(
        &mut self,
        target: &TypeName,
        expression: &Expression,
    ) -> Result<ExpressionType, Diagnostic> {
        let actual = self.expression_type(expression)?;
        let allowed = match &actual {
            ExpressionType::Null => true,
            ExpressionType::Value(source) => {
                source == target
                    || *source == TypeName::Object
                    || *target == TypeName::Object
                    || (*source == TypeName::Exception && target.is_exception())
                    || (*target == TypeName::Exception && source.is_exception())
                    || self.is_subtype(source, target)
                    || self.is_subtype(target, source)
            }
            ExpressionType::Void => false,
        };
        if allowed {
            Ok(ExpressionType::Value(target.clone()))
        } else {
            Err(Diagnostic::new(
                format!("cannot cast {} to {}", actual.name(), target.apex_name()),
                expression.span(),
            ))
        }
    }

    fn new_collection_type(
        &mut self,
        ty: &TypeName,
        initializer: &CollectionInitializer,
    ) -> Result<ExpressionType, Diagnostic> {
        match initializer {
            CollectionInitializer::Arguments(arguments) => {
                self.check_collection_constructor(ty, arguments)?;
            }
            CollectionInitializer::Elements(elements) => {
                let element_type = match ty {
                    TypeName::List(element) | TypeName::Set(element) => element.as_ref(),
                    _ => {
                        return Err(Diagnostic::new(
                            format!("{} does not support an element initializer", ty.apex_name()),
                            elements.first().map_or(Span::new(0, 0), Expression::span),
                        ));
                    }
                };
                for element in elements {
                    let actual = self.expression_type(element)?;
                    self.require_assignable(element_type, &actual, element.span())?;
                }
            }
            CollectionInitializer::MapEntries(entries) => {
                let TypeName::Map(key_type, value_type) = ty else {
                    return Err(Diagnostic::new(
                        format!("{} does not support a map initializer", ty.apex_name()),
                        entries.first().map_or(Span::new(0, 0), |entry| entry.span),
                    ));
                };
                for entry in entries {
                    let actual_key = self.expression_type(&entry.key)?;
                    self.require_assignable(key_type, &actual_key, entry.key.span())?;
                    let actual_value = self.expression_type(&entry.value)?;
                    self.require_assignable(value_type, &actual_value, entry.value.span())?;
                }
            }
            CollectionInitializer::SizedArray(size) => {
                if !matches!(ty, TypeName::List(_)) {
                    return Err(Diagnostic::new(
                        format!("{} cannot be allocated with an array size", ty.apex_name()),
                        size.span(),
                    ));
                }
                self.require_operand(size, &TypeName::Integer, size.span())?;
            }
        }
        Ok(ExpressionType::Value(ty.clone()))
    }

    fn check_collection_constructor(
        &mut self,
        ty: &TypeName,
        arguments: &[Expression],
    ) -> Result<(), Diagnostic> {
        match ty {
            TypeName::List(element) | TypeName::Set(element) => {
                require_arity(ty, "constructor", arguments.len(), &[0, 1], arguments)?;
                if let Some(argument) = arguments.first() {
                    self.require_list_or_set_argument(ty, "constructor", 0, argument, element)?;
                }
                Ok(())
            }
            TypeName::Map(..) => {
                require_arity(ty, "constructor", arguments.len(), &[0, 1], arguments)?;
                if let Some(argument) = arguments.first() {
                    self.require_argument(ty, "constructor", 0, argument, ty)?;
                }
                Ok(())
            }
            _ => Err(Diagnostic::new(
                format!("{} is not constructible in this milestone", ty.apex_name()),
                arguments.first().map_or(Span::new(0, 0), Expression::span),
            )),
        }
    }

    fn assignment_target_type(
        &mut self,
        target: &AssignmentTarget,
    ) -> Result<TypeName, Diagnostic> {
        match target {
            AssignmentTarget::Variable(identifier) => {
                if let Some(ty) = self.lookup(&identifier.canonical).cloned() {
                    self.references
                        .insert(identifier.span, ReferenceTarget::Local);
                    return Ok(ty);
                }
                let class_id = self
                    .current_class
                    .ok_or_else(|| unknown_variable(identifier))?;
                let member = self
                    .class_value_member(class_id, &identifier.canonical)
                    .ok_or_else(|| unknown_variable(identifier))?;
                self.ensure_member_access(member.target, &member.write_access, identifier.span)?;
                if !member.writable {
                    return Err(Diagnostic::new(
                        format!("member `{}` is read-only", identifier.spelling),
                        identifier.span,
                    ));
                }
                let is_static = member.modifiers.contains(&Modifier::Static);
                if self.current_static && !is_static {
                    return Err(Diagnostic::new(
                        format!(
                            "instance member `{}` is unavailable in a static context",
                            identifier.spelling
                        ),
                        identifier.span,
                    ));
                }
                self.references.insert(
                    identifier.span,
                    if is_static {
                        ReferenceTarget::StaticMember(member.target)
                    } else {
                        ReferenceTarget::InstanceMember(member.target)
                    },
                );
                Ok(member.ty)
            }
            AssignmentTarget::Index {
                collection, index, ..
            } => self.index_type(collection, index),
            AssignmentTarget::Member {
                receiver,
                member,
                span,
            } => match self.member_access_type(receiver, member, *span, true)? {
                ExpressionType::Value(ty) => Ok(ty),
                _ => unreachable!("member access always has a value type"),
            },
        }
    }

    fn index_type(
        &mut self,
        collection: &Expression,
        index: &Expression,
    ) -> Result<TypeName, Diagnostic> {
        let collection_type = self.expression_type(collection)?;
        self.require_operand(index, &TypeName::Integer, index.span())?;
        match collection_type {
            ExpressionType::Value(TypeName::List(element)) => Ok(*element),
            other => Err(Diagnostic::new(
                format!("cannot index {}", other.name()),
                collection.span(),
            )),
        }
    }

    fn method_call_type(
        &mut self,
        receiver: &Expression,
        method: &Identifier,
        arguments: &[Expression],
        span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let result = if let Expression::Variable(identifier) = receiver {
            if let Some(receiver_type) = self.lookup(&identifier.canonical).cloned() {
                let static_candidates =
                    self.class_ids
                        .get(&identifier.canonical)
                        .copied()
                        .map(|class_id| {
                            (
                                class_id,
                                self.class_methods_named(class_id, &method.canonical)
                                    .into_iter()
                                    .filter(|candidate| {
                                        candidate.modifiers.contains(&Modifier::Static)
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        });
                if let Some((class_id, candidates)) = static_candidates
                    && !candidates.is_empty()
                {
                    let argument_types = arguments
                        .iter()
                        .map(|argument| self.expression_type(argument))
                        .collect::<Result<Vec<_>, _>>()?;
                    self.select_class_method_call(
                        class_id,
                        method,
                        &argument_types,
                        candidates,
                        ClassCallKind::Static,
                        span,
                    )
                } else {
                    self.references
                        .insert(identifier.span, ReferenceTarget::Local);
                    self.instance_method_type(&receiver_type, method, arguments, span, false)
                }
            } else if let Some(class_id) = self.class_ids.get(&identifier.canonical).copied()
                && self
                    .current_class
                    .and_then(|id| self.class_value_member(id, &identifier.canonical))
                    .is_none()
            {
                let argument_types = arguments
                    .iter()
                    .map(|argument| self.expression_type(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                let candidates = self.class_methods_named(class_id, &method.canonical);
                self.select_class_method_call(
                    class_id,
                    method,
                    &argument_types,
                    candidates,
                    ClassCallKind::Static,
                    span,
                )
            } else {
                match identifier.canonical.as_str() {
                    "string" => self.static_string_method_type(method, arguments),
                    "math" => self.static_math_method_type(method, arguments),
                    "system" => self.static_system_method_type(method, arguments),
                    _ => {
                        let receiver_type = self.variable_type(identifier)?;
                        let ExpressionType::Value(receiver_type) = receiver_type else {
                            unreachable!("variables always have value types")
                        };
                        self.instance_method_type(
                            &receiver_type,
                            method,
                            arguments,
                            span,
                            identifier.canonical == "super",
                        )
                    }
                }
            }
        } else {
            match self.expression_type(receiver)? {
                ExpressionType::Value(receiver_type) => {
                    self.instance_method_type(&receiver_type, method, arguments, span, false)
                }
                other => Err(Diagnostic::new(
                    format!(
                        "cannot call method `{}` on {}",
                        method.spelling,
                        other.name()
                    ),
                    method.span,
                )),
            }
        };

        result.map_err(|mut error| {
            if error.span == Span::new(0, 0) {
                error.span = method.span;
            }
            error
        })
    }

    fn instance_method_type(
        &mut self,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
        span: Span,
        super_call: bool,
    ) -> Result<ExpressionType, Diagnostic> {
        match receiver_type {
            TypeName::List(element) => {
                self.list_method_type(receiver_type, element, method, arguments)
            }
            TypeName::Set(element) => {
                self.set_method_type(receiver_type, element, method, arguments)
            }
            TypeName::Map(key, value) => {
                self.map_method_type(receiver_type, key, value, method, arguments)
            }
            TypeName::String => self.string_instance_method_type(method, arguments),
            ty if ty.is_exception() => {
                self.exception_instance_method_type(receiver_type, method, arguments)
            }
            TypeName::Custom(name) => {
                let class_id = self.class_ids[&name.canonical];
                let argument_types = arguments
                    .iter()
                    .map(|argument| self.expression_type(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                let candidates = self.class_methods_named(class_id, &method.canonical);
                self.select_class_method_call(
                    class_id,
                    method,
                    &argument_types,
                    candidates,
                    if super_call {
                        ClassCallKind::Super
                    } else {
                        ClassCallKind::Instance
                    },
                    span,
                )
            }
            _ => Err(unknown_method(receiver_type, method)),
        }
    }

    fn select_class_method_call(
        &mut self,
        receiver_class_id: usize,
        method: &Identifier,
        argument_types: &[ExpressionType],
        candidates: Vec<ClassMethodSignature>,
        kind: ClassCallKind,
        span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let static_call = kind == ClassCallKind::Static;
        let candidates = candidates
            .into_iter()
            .filter(|candidate| candidate.modifiers.contains(&Modifier::Static) == static_call)
            .collect::<Vec<_>>();
        let applicable = candidates
            .iter()
            .filter(|candidate| candidate.parameter_types.len() == argument_types.len())
            .filter(|candidate| {
                candidate
                    .parameter_types
                    .iter()
                    .zip(argument_types)
                    .all(|(expected, actual)| self.is_assignable(expected, actual))
            })
            .cloned()
            .collect::<Vec<_>>();
        if applicable.is_empty() {
            return Err(Diagnostic::new(
                format!(
                    "no matching {}method `{}` on {}",
                    if static_call { "static " } else { "" },
                    method.spelling,
                    self.classes[receiver_class_id].name.spelling
                ),
                method.span,
            ));
        }
        let most_specific = applicable
            .iter()
            .filter(|candidate| {
                !applicable.iter().any(|other| {
                    other.target != candidate.target
                        && self.parameter_types_more_specific(
                            &other.parameter_types,
                            &candidate.parameter_types,
                        )
                })
            })
            .collect::<Vec<_>>();
        let [best] = most_specific.as_slice() else {
            return Err(Diagnostic::new(
                format!("ambiguous overload for method `{}`", method.spelling),
                method.span,
            ));
        };
        self.ensure_member_access(best.target, &best.modifiers, method.span)?;
        self.calls.insert(
            span,
            match kind {
                ClassCallKind::Static => CallTarget::StaticMethod(best.target),
                ClassCallKind::Instance => CallTarget::InstanceMethod(best.target),
                ClassCallKind::Super => CallTarget::SuperMethod(best.target),
            },
        );
        Ok(match &best.return_type {
            ReturnType::Void => ExpressionType::Void,
            ReturnType::Value(ty) => ExpressionType::Value(ty.clone()),
        })
    }

    fn exception_instance_method_type(
        &mut self,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        match method.canonical.as_str() {
            "getmessage" | "gettypename" | "getstacktracestring" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::String))
            }
            _ => Err(unknown_method(receiver_type, method)),
        }
    }

    fn list_method_type(
        &mut self,
        receiver_type: &TypeName,
        element: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        match method.canonical.as_str() {
            "add" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1, 2],
                    arguments,
                )?;
                if arguments.len() == 2 {
                    self.require_argument(
                        receiver_type,
                        &method.spelling,
                        0,
                        &arguments[0],
                        &TypeName::Integer,
                    )?;
                    self.require_argument(
                        receiver_type,
                        &method.spelling,
                        1,
                        &arguments[1],
                        element,
                    )?;
                } else {
                    self.require_argument(
                        receiver_type,
                        &method.spelling,
                        0,
                        &arguments[0],
                        element,
                    )?;
                }
                Ok(ExpressionType::Void)
            }
            "addall" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_list_or_set_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    element,
                )?;
                Ok(ExpressionType::Void)
            }
            "clear" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Void)
            }
            "clone" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(receiver_type.clone()))
            }
            "contains" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], element)?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            "get" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::Integer,
                )?;
                Ok(ExpressionType::Value(element.clone()))
            }
            "indexof" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], element)?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            "isempty" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            "remove" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::Integer,
                )?;
                Ok(ExpressionType::Value(element.clone()))
            }
            "set" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[2],
                    arguments,
                )?;
                self.require_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::Integer,
                )?;
                self.require_argument(receiver_type, &method.spelling, 1, &arguments[1], element)?;
                Ok(ExpressionType::Void)
            }
            "size" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            "sort" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                if !matches!(element, TypeName::String | TypeName::Integer) {
                    return Err(Diagnostic::new(
                        format!(
                            "method `sort` requires List<String> or List<Integer>, found {}",
                            receiver_type.apex_name()
                        ),
                        method.span,
                    ));
                }
                Ok(ExpressionType::Void)
            }
            _ => Err(unknown_method(receiver_type, method)),
        }
    }

    fn set_method_type(
        &mut self,
        receiver_type: &TypeName,
        element: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        match method.canonical.as_str() {
            "add" | "contains" | "remove" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], element)?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            "addall" | "containsall" | "removeall" | "retainall" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_list_or_set_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    element,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            "clear" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Void)
            }
            "clone" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(receiver_type.clone()))
            }
            "isempty" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            "size" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            _ => Err(unknown_method(receiver_type, method)),
        }
    }

    fn map_method_type(
        &mut self,
        receiver_type: &TypeName,
        key: &TypeName,
        value: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        match method.canonical.as_str() {
            "clear" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Void)
            }
            "clone" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(receiver_type.clone()))
            }
            "containskey" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], key)?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            "get" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], key)?;
                Ok(ExpressionType::Value(value.clone()))
            }
            "isempty" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            "keyset" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Set(Box::new(key.clone()))))
            }
            "put" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[2],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], key)?;
                self.require_argument(receiver_type, &method.spelling, 1, &arguments[1], value)?;
                Ok(ExpressionType::Value(value.clone()))
            }
            "putall" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    receiver_type,
                )?;
                Ok(ExpressionType::Void)
            }
            "remove" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], key)?;
                Ok(ExpressionType::Value(value.clone()))
            }
            "size" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            "values" => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::List(Box::new(
                    value.clone(),
                ))))
            }
            _ => Err(unknown_method(receiver_type, method)),
        }
    }

    fn static_string_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        match method.canonical.as_str() {
            "valueof" => {
                require_static_arity("String", method, arguments.len(), &[1], arguments)?;
                self.require_non_void_argument("String", &method.spelling, 0, &arguments[0])?;
                Ok(ExpressionType::Value(TypeName::String))
            }
            "join" => {
                require_static_arity("String", method, arguments.len(), &[2], arguments)?;
                self.require_list_or_set_argument(
                    &TypeName::String,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                self.require_argument(
                    &TypeName::String,
                    &method.spelling,
                    1,
                    &arguments[1],
                    &TypeName::String,
                )?;
                Ok(ExpressionType::Value(TypeName::String))
            }
            "isblank" | "isnotblank" | "isempty" | "isnotempty" => {
                require_static_arity("String", method, arguments.len(), &[1], arguments)?;
                self.require_argument(
                    &TypeName::String,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            _ => Err(unknown_static_method("String", method)),
        }
    }

    fn string_instance_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        let receiver_type = TypeName::String;
        match method.canonical.as_str() {
            "length" => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            "contains" | "startswith" | "endswith" | "equals" | "equalsignorecase" => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    &receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            "indexof" => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    &receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            "substring" => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1, 2],
                    arguments,
                )?;
                for (index, argument) in arguments.iter().enumerate() {
                    self.require_argument(
                        &receiver_type,
                        &method.spelling,
                        index,
                        argument,
                        &TypeName::Integer,
                    )?;
                }
                Ok(ExpressionType::Value(TypeName::String))
            }
            "trim" | "tolowercase" | "touppercase" => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::String))
            }
            "replace" => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[2],
                    arguments,
                )?;
                for (index, argument) in arguments.iter().enumerate() {
                    self.require_argument(
                        &receiver_type,
                        &method.spelling,
                        index,
                        argument,
                        &TypeName::String,
                    )?;
                }
                Ok(ExpressionType::Value(TypeName::String))
            }
            _ => Err(unknown_method(&receiver_type, method)),
        }
    }

    fn static_math_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        let arity = match method.canonical.as_str() {
            "abs" => 1,
            "max" | "min" | "mod" => 2,
            _ => return Err(unknown_static_method("Math", method)),
        };
        require_static_arity("Math", method, arguments.len(), &[arity], arguments)?;
        for (index, argument) in arguments.iter().enumerate() {
            self.require_named_argument(
                "Math",
                &method.spelling,
                index,
                argument,
                &TypeName::Integer,
            )?;
        }
        Ok(ExpressionType::Value(TypeName::Integer))
    }

    fn static_system_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        match method.canonical.as_str() {
            "debug" => {
                require_static_arity("System", method, arguments.len(), &[1], arguments)?;
                self.require_non_void_argument("System", &method.spelling, 0, &arguments[0])?;
                Ok(ExpressionType::Void)
            }
            _ => Err(unknown_static_method("System", method)),
        }
    }

    fn require_argument(
        &mut self,
        receiver_type: &TypeName,
        method: &str,
        position: usize,
        argument: &Expression,
        expected: &TypeName,
    ) -> Result<(), Diagnostic> {
        self.require_named_argument(
            &receiver_type.apex_name(),
            method,
            position,
            argument,
            expected,
        )
    }

    fn require_named_argument(
        &mut self,
        owner: &str,
        method: &str,
        position: usize,
        argument: &Expression,
        expected: &TypeName,
    ) -> Result<(), Diagnostic> {
        let actual = self.expression_type(argument)?;
        if self.is_assignable(expected, &actual) {
            Ok(())
        } else {
            Err(argument_type_error(
                owner,
                method,
                position,
                &expected.apex_name(),
                &actual,
                argument.span(),
            ))
        }
    }

    fn require_list_or_set_argument(
        &mut self,
        receiver_type: &TypeName,
        method: &str,
        position: usize,
        argument: &Expression,
        expected_element: &TypeName,
    ) -> Result<(), Diagnostic> {
        let actual = self.expression_type(argument)?;
        let is_compatible = match &actual {
            ExpressionType::Value(TypeName::List(element))
            | ExpressionType::Value(TypeName::Set(element)) => element.as_ref() == expected_element,
            ExpressionType::Null => true,
            ExpressionType::Value(_) | ExpressionType::Void => false,
        };
        if is_compatible {
            Ok(())
        } else {
            Err(argument_type_error(
                &receiver_type.apex_name(),
                method,
                position,
                &format!(
                    "List<{}> or Set<{}>",
                    expected_element.apex_name(),
                    expected_element.apex_name()
                ),
                &actual,
                argument.span(),
            ))
        }
    }

    fn require_non_void_argument(
        &mut self,
        owner: &str,
        method: &str,
        position: usize,
        argument: &Expression,
    ) -> Result<(), Diagnostic> {
        let actual = self.expression_type(argument)?;
        if actual != ExpressionType::Void {
            Ok(())
        } else {
            Err(argument_type_error(
                owner,
                method,
                position,
                "a value",
                &actual,
                argument.span(),
            ))
        }
    }

    fn unary_type(
        &mut self,
        operator: UnaryOperator,
        operand: &Expression,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        match operator {
            UnaryOperator::Positive | UnaryOperator::Negate => {
                self.require_operand(operand, &TypeName::Integer, operator_span)?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            UnaryOperator::Not => {
                self.require_operand(operand, &TypeName::Boolean, operator_span)?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            UnaryOperator::PrefixIncrement | UnaryOperator::PrefixDecrement => {
                self.require_mutable_integer(operand, operator_span)
            }
        }
    }

    fn postfix_type(
        &mut self,
        operand: &Expression,
        _operator: PostfixOperator,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        self.require_mutable_integer(operand, operator_span)
    }

    fn binary_type(
        &mut self,
        left: &Expression,
        operator: BinaryOperator,
        right: &Expression,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let left_type = self.expression_type(left)?;
        let right_type = self.expression_type(right)?;
        match operator {
            BinaryOperator::Add => {
                if left_type == ExpressionType::Value(TypeName::Integer)
                    && right_type == ExpressionType::Value(TypeName::Integer)
                {
                    Ok(ExpressionType::Value(TypeName::Integer))
                } else if (left_type == ExpressionType::Value(TypeName::String)
                    || right_type == ExpressionType::Value(TypeName::String))
                    && left_type != ExpressionType::Void
                    && right_type != ExpressionType::Void
                {
                    Ok(ExpressionType::Value(TypeName::String))
                } else {
                    Err(invalid_binary_operands(
                        operator,
                        &left_type,
                        &right_type,
                        operator_span,
                    ))
                }
            }
            BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => {
                if left_type == ExpressionType::Value(TypeName::Integer)
                    && right_type == ExpressionType::Value(TypeName::Integer)
                {
                    if matches!(
                        operator,
                        BinaryOperator::Less
                            | BinaryOperator::LessEqual
                            | BinaryOperator::Greater
                            | BinaryOperator::GreaterEqual
                    ) {
                        Ok(ExpressionType::Value(TypeName::Boolean))
                    } else {
                        Ok(ExpressionType::Value(TypeName::Integer))
                    }
                } else {
                    Err(invalid_binary_operands(
                        operator,
                        &left_type,
                        &right_type,
                        operator_span,
                    ))
                }
            }
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                let comparable = match (&left_type, &right_type) {
                    (ExpressionType::Value(left), ExpressionType::Value(right)) => left == right,
                    (ExpressionType::Null, ExpressionType::Value(_))
                    | (ExpressionType::Value(_), ExpressionType::Null)
                    | (ExpressionType::Null, ExpressionType::Null) => true,
                    (ExpressionType::Void, _) | (_, ExpressionType::Void) => false,
                };
                if comparable {
                    Ok(ExpressionType::Value(TypeName::Boolean))
                } else {
                    Err(invalid_binary_operands(
                        operator,
                        &left_type,
                        &right_type,
                        operator_span,
                    ))
                }
            }
            BinaryOperator::And | BinaryOperator::Or => {
                if left_type == ExpressionType::Value(TypeName::Boolean)
                    && right_type == ExpressionType::Value(TypeName::Boolean)
                {
                    Ok(ExpressionType::Value(TypeName::Boolean))
                } else {
                    Err(invalid_binary_operands(
                        operator,
                        &left_type,
                        &right_type,
                        operator_span,
                    ))
                }
            }
        }
    }

    fn require_boolean(&mut self, expression: &Expression) -> Result<(), Diagnostic> {
        self.require_operand(expression, &TypeName::Boolean, expression.span())
    }

    fn require_operand(
        &mut self,
        expression: &Expression,
        expected: &TypeName,
        error_span: Span,
    ) -> Result<(), Diagnostic> {
        let actual = self.expression_type(expression)?;
        if actual == ExpressionType::Value(expected.clone()) {
            Ok(())
        } else {
            Err(Diagnostic::new(
                format!("expected {}, found {}", expected.apex_name(), actual.name()),
                error_span,
            ))
        }
    }

    fn require_mutable_integer(
        &mut self,
        operand: &Expression,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let actual = match operand {
            Expression::Variable(identifier) => {
                self.assignment_target_type(&AssignmentTarget::Variable(identifier.clone()))?
            }
            Expression::Index {
                collection, index, ..
            } => self.index_type(collection, index)?,
            Expression::MemberAccess {
                receiver,
                member,
                span,
            } => self.assignment_target_type(&AssignmentTarget::Member {
                receiver: receiver.clone(),
                member: member.clone(),
                span: *span,
            })?,
            _ => {
                return Err(Diagnostic::new(
                    "increment/decrement operand must be a variable",
                    operator_span,
                ));
            }
        };
        if actual != TypeName::Integer {
            return Err(Diagnostic::new(
                format!(
                    "increment/decrement requires Integer, found {}",
                    actual.apex_name()
                ),
                operator_span,
            ));
        }
        Ok(ExpressionType::Value(TypeName::Integer))
    }

    fn lookup(&self, canonical: &str) -> Option<&TypeName> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(canonical))
    }

    fn current_scope(&self) -> &HashMap<String, TypeName> {
        self.scopes.last().expect("checker always has a scope")
    }

    fn current_scope_mut(&mut self) -> &mut HashMap<String, TypeName> {
        self.scopes.last_mut().expect("checker always has a scope")
    }

    fn with_scope<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        self.scopes.push(HashMap::new());
        let result = operation(self);
        self.scopes.pop();
        result
    }

    fn with_loop<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        self.loop_depth += 1;
        let result = operation(self);
        self.loop_depth -= 1;
        result
    }
}

fn validate_modifier_set(
    modifiers: &[Modifier],
    span: Span,
    subject: &str,
) -> Result<(), Diagnostic> {
    let mut seen = Vec::new();
    for modifier in modifiers {
        if seen.contains(modifier) {
            return Err(Diagnostic::new(
                format!("duplicate modifier on {subject}"),
                span,
            ));
        }
        seen.push(*modifier);
    }
    let access_count = [
        Modifier::Private,
        Modifier::Protected,
        Modifier::Public,
        Modifier::Global,
    ]
    .iter()
    .filter(|modifier| modifiers.contains(modifier))
    .count();
    if access_count > 1 {
        return Err(Diagnostic::new(
            format!("conflicting access modifiers on {subject}"),
            span,
        ));
    }
    if modifiers.contains(&Modifier::Abstract) && modifiers.contains(&Modifier::Final) {
        return Err(Diagnostic::new(
            format!("{subject} cannot be both abstract and final"),
            span,
        ));
    }
    Ok(())
}

fn reject_modifiers(
    modifiers: &[Modifier],
    rejected: &[Modifier],
    span: Span,
    subject: &str,
) -> Result<(), Diagnostic> {
    if let Some(modifier) = rejected
        .iter()
        .find(|modifier| modifiers.contains(modifier))
    {
        Err(Diagnostic::new(
            format!(
                "modifier `{}` is not valid on {subject}",
                modifier_name(*modifier)
            ),
            span,
        ))
    } else {
        Ok(())
    }
}

fn modifier_name(modifier: Modifier) -> &'static str {
    match modifier {
        Modifier::Public => "public",
        Modifier::Private => "private",
        Modifier::Protected => "protected",
        Modifier::Global => "global",
        Modifier::Static => "static",
        Modifier::Virtual => "virtual",
        Modifier::Abstract => "abstract",
        Modifier::Override => "override",
        Modifier::Final => "final",
        Modifier::WithSharing => "with sharing",
        Modifier::WithoutSharing => "without sharing",
        Modifier::InheritedSharing => "inherited sharing",
    }
}

fn access_rank(modifiers: &[Modifier]) -> u8 {
    if modifiers.contains(&Modifier::Global) {
        3
    } else if modifiers.contains(&Modifier::Public) {
        2
    } else if modifiers.contains(&Modifier::Protected) {
        1
    } else {
        0
    }
}

fn push_unique_signature(
    signatures: &mut Vec<ClassMethodSignature>,
    signature: ClassMethodSignature,
) {
    if !signatures.iter().any(|existing| {
        existing.name == signature.name && existing.parameter_types == signature.parameter_types
    }) {
        signatures.push(signature);
    }
}

fn method_more_specific(left: &MethodSignature, right: &MethodSignature) -> bool {
    let mut strictly_more_specific = false;
    for (left, right) in left.parameter_types.iter().zip(&right.parameter_types) {
        if left == right {
            continue;
        }
        if type_more_specific(left, right) {
            strictly_more_specific = true;
        } else {
            return false;
        }
    }
    strictly_more_specific
}

fn type_more_specific(left: &TypeName, right: &TypeName) -> bool {
    *right == TypeName::Object
        || (*right == TypeName::Exception && left.is_exception() && *left != TypeName::Exception)
}

fn is_statement_expression(expression: &Expression) -> bool {
    matches!(
        expression,
        Expression::Assignment { .. }
            | Expression::FunctionCall { .. }
            | Expression::MethodCall { .. }
            | Expression::Unary {
                operator: UnaryOperator::PrefixIncrement | UnaryOperator::PrefixDecrement,
                ..
            }
            | Expression::Postfix { .. }
    )
}

fn statement_definitely_returns_or_throws(statement: &Statement) -> bool {
    let completions = statement_completions(statement);
    !completions.normal
        && !completions.breaks
        && !completions.continues
        && (completions.returns || completions.throws)
}

#[derive(Clone, Copy, Debug, Default)]
struct Completions {
    normal: bool,
    returns: bool,
    throws: bool,
    breaks: bool,
    continues: bool,
}

impl Completions {
    fn normal() -> Self {
        Self {
            normal: true,
            ..Self::default()
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            normal: self.normal || other.normal,
            returns: self.returns || other.returns,
            throws: self.throws || other.throws,
            breaks: self.breaks || other.breaks,
            continues: self.continues || other.continues,
        }
    }

    fn then(self, next: Self) -> Self {
        Self {
            normal: self.normal && next.normal,
            returns: self.returns || (self.normal && next.returns),
            throws: self.throws || (self.normal && next.throws),
            breaks: self.breaks || (self.normal && next.breaks),
            continues: self.continues || (self.normal && next.continues),
        }
    }

    fn without_throw(self) -> Self {
        Self {
            throws: false,
            ..self
        }
    }
}

fn statement_completions(statement: &Statement) -> Completions {
    match statement {
        Statement::Return { .. } => Completions {
            returns: true,
            ..Completions::default()
        },
        Statement::Throw { .. } => Completions {
            throws: true,
            ..Completions::default()
        },
        Statement::Break { .. } => Completions {
            breaks: true,
            ..Completions::default()
        },
        Statement::Continue { .. } => Completions {
            continues: true,
            ..Completions::default()
        },
        Statement::Block { statements, .. } => statements
            .iter()
            .fold(Completions::normal(), |current, statement| {
                current.then(statement_completions(statement))
            }),
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => statement_completions(then_branch).union(
            else_branch
                .as_deref()
                .map_or_else(Completions::normal, statement_completions),
        ),
        Statement::While { body, .. }
        | Statement::For { body, .. }
        | Statement::ForEach { body, .. } => {
            let body = statement_completions(body);
            Completions {
                normal: true,
                returns: body.returns,
                throws: body.throws,
                breaks: false,
                continues: false,
            }
        }
        Statement::DoWhile { body, .. } => {
            let body = statement_completions(body);
            Completions {
                normal: body.normal || body.breaks || body.continues,
                returns: body.returns,
                throws: body.throws,
                breaks: false,
                continues: false,
            }
        }
        Statement::Try {
            try_block,
            catches,
            finally_block,
            ..
        } => {
            let try_completions = statement_completions(try_block);
            let mut pending = try_completions.without_throw();
            if catches.is_empty() {
                pending.throws = try_completions.throws;
            } else {
                for catch in catches {
                    pending = pending.union(statement_completions(&catch.body));
                }
                if !catches
                    .iter()
                    .any(|catch| catch.exception_type == TypeName::Exception)
                {
                    pending.throws = true;
                }
            }

            let Some(finally_block) = finally_block else {
                return pending;
            };
            let finally = statement_completions(finally_block);
            let mut result = Completions {
                normal: false,
                returns: finally.returns,
                throws: finally.throws,
                breaks: finally.breaks,
                continues: finally.continues,
            };
            if finally.normal {
                result = result.union(pending);
            }
            result
        }
        Statement::VariableDeclaration { .. } | Statement::Expression { .. } => {
            Completions::normal()
        }
    }
}

fn invalid_binary_operands(
    operator: BinaryOperator,
    left: &ExpressionType,
    right: &ExpressionType,
    span: Span,
) -> Diagnostic {
    Diagnostic::new(
        format!(
            "operator `{}` cannot be applied to {} and {}",
            binary_operator_spelling(operator),
            left.name(),
            right.name()
        ),
        span,
    )
}

fn binary_operator_spelling(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Remainder => "%",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::Equal => "==",
        BinaryOperator::NotEqual => "!=",
        BinaryOperator::And => "&&",
        BinaryOperator::Or => "||",
    }
}

fn unknown_variable(identifier: &Identifier) -> Diagnostic {
    Diagnostic::new(
        format!("unknown variable `{}`", identifier.spelling),
        identifier.span,
    )
}

fn unknown_method(receiver_type: &TypeName, method: &Identifier) -> Diagnostic {
    Diagnostic::new(
        format!(
            "unknown method `{}` on {}",
            method.spelling,
            receiver_type.apex_name()
        ),
        method.span,
    )
}

fn unknown_static_method(owner: &str, method: &Identifier) -> Diagnostic {
    Diagnostic::new(
        format!("unknown static method `{}` on {}", method.spelling, owner),
        method.span,
    )
}

fn require_arity(
    receiver_type: &TypeName,
    method: &str,
    actual: usize,
    expected: &[usize],
    arguments: &[Expression],
) -> Result<(), Diagnostic> {
    require_arity_named(
        &receiver_type.apex_name(),
        method,
        actual,
        expected,
        arguments,
    )
}

fn require_static_arity(
    owner: &str,
    method: &Identifier,
    actual: usize,
    expected: &[usize],
    arguments: &[Expression],
) -> Result<(), Diagnostic> {
    require_arity_named(owner, &method.spelling, actual, expected, arguments).map_err(
        |mut error| {
            if arguments.is_empty() {
                error.span = method.span;
            }
            error
        },
    )
}

fn require_arity_named(
    owner: &str,
    method: &str,
    actual: usize,
    expected: &[usize],
    arguments: &[Expression],
) -> Result<(), Diagnostic> {
    if expected.contains(&actual) {
        return Ok(());
    }
    let expected = expected
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(" or ");
    Err(Diagnostic::new(
        format!("method `{method}` on {owner} expects {expected} arguments, found {actual}"),
        arguments.first().map_or(Span::new(0, 0), Expression::span),
    ))
}

fn argument_type_error(
    owner: &str,
    method: &str,
    position: usize,
    expected: &str,
    actual: &ExpressionType,
    span: Span,
) -> Diagnostic {
    Diagnostic::new(
        format!(
            "argument {} to `{}` on {} expects {}, found {}",
            position + 1,
            method,
            owner,
            expected,
            actual.name()
        ),
        span,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_source(source: &str) -> Result<(), Diagnostic> {
        let program = crate::parse(source)?;
        check(&program).map(|_| ())
    }

    #[test]
    fn permits_nested_shadowing_but_rejects_same_scope_duplicates() {
        check_source("Integer value = 1; { Integer VALUE = 2; }").unwrap();

        let error = check_source("Integer value = 1; Integer VALUE = 2;").unwrap_err();
        assert_eq!(error.message, "duplicate variable `VALUE`");
    }

    #[test]
    fn short_circuit_rhs_is_still_checked_statically() {
        let error = check_source("Boolean result = true || missing;").unwrap_err();
        assert_eq!(error.message, "unknown variable `missing`");
    }

    #[test]
    fn null_assignment_does_not_permit_cross_type_equality() {
        check_source("Integer number = null; number = null;").unwrap();

        let error = check_source("Integer number = 1; Boolean same = number == true;").unwrap_err();
        assert_eq!(
            error.message,
            "operator `==` cannot be applied to Integer and Boolean"
        );
    }

    #[test]
    fn loop_control_is_validated_against_lexical_loop_depth() {
        check_source("while (true) { if (false) continue; break; }").unwrap();

        let error = check_source("{ break; }").unwrap_err();
        assert_eq!(error.message, "`break` is only valid inside a loop");
    }

    #[test]
    fn checks_collection_construction_indexing_and_generic_invariance() {
        check_source(
            "List<String> values = new List<String>{'a', null}; \
             Set<String> unique = new Set<String>(values); \
             Map<String, List<String>> grouped = new Map<String, List<String>>{ \
                 'all' => values \
             }; \
             String first = values[0]; \
             values[0] = 'b'; \
             Integer[] numbers = new Integer[2]; \
             numbers[0]++;",
        )
        .unwrap();

        let error = check_source("List<String> values = new List<String>{1};").unwrap_err();
        assert_eq!(error.message, "cannot assign Integer to String");

        let error = check_source(
            "List<Integer> values = new List<Integer>(); String value = values['zero'];",
        )
        .unwrap_err();
        assert_eq!(error.message, "expected Integer, found String");
    }

    #[test]
    fn checks_enhanced_for_types_scope_and_loop_control() {
        check_source(
            "Set<String> values = new Set<String>{'a'}; \
             for (String value : values) { if (value == 'a') continue; }",
        )
        .unwrap();

        let error = check_source(
            "List<String> values = new List<String>(); \
             for (Integer value : values) {}",
        )
        .unwrap_err();
        assert_eq!(error.message, "cannot assign String to Integer");

        let error = check_source(
            "Map<String, String> values = new Map<String, String>(); \
             for (String value : values) {}",
        )
        .unwrap_err();
        assert_eq!(
            error.message,
            "enhanced for-loop requires List or Set, found Map<String,String>"
        );
    }

    #[test]
    fn checks_collection_method_signatures_and_return_types() {
        check_source(
            "List<String> values = new List<String>{'b'}; \
             values.add('c'); values.add(0, 'a'); values.addAll(new Set<String>{'d'}); \
             Boolean hasA = values.contains('a'); Integer position = values.indexOf('a'); \
             String first = values.get(0); String removed = values.remove(0); \
             values.set(0, 'z'); Integer count = values.size(); Boolean listEmpty = values.isEmpty(); \
             values.sort(); List<String> copy = values.clone(); copy.clear(); \
             Set<String> unique = new Set<String>(values); \
             Boolean changed = unique.add('q'); changed = unique.addAll(values); \
             changed = unique.containsAll(values); changed = unique.removeAll(values); \
             changed = unique.retainAll(copy); changed = unique.remove('q'); \
             Boolean setEmpty = unique.isEmpty(); Integer setSize = unique.size(); \
             Set<String> uniqueCopy = unique.clone(); uniqueCopy.clear(); \
             Map<String, String> labels = new Map<String, String>{'a' => 'A'}; \
             String prior = labels.put('b', 'B'); String found = labels.get('a'); \
             Boolean hasKey = labels.containsKey('a'); Set<String> keys = labels.keySet(); \
             String removedLabel = labels.remove('b'); Boolean mapEmpty = labels.isEmpty(); \
             Integer mapSize = labels.size(); List<String> labelValues = labels.values(); \
             Map<String, String> labelsCopy = labels.clone(); \
             Map<String, String> constructedCopy = new Map<String, String>(labelsCopy); \
             labels.putAll(constructedCopy); labels.clear();",
        )
        .unwrap();

        let error =
            check_source("List<String> values = new List<String>(); values.add(1);").unwrap_err();
        assert_eq!(
            error.message,
            "argument 1 to `add` on List<String> expects String, found Integer"
        );

        let error =
            check_source("Set<String> values = new Set<String>(); values.add();").unwrap_err();
        assert_eq!(
            error.message,
            "method `add` on Set<String> expects 1 arguments, found 0"
        );
    }

    #[test]
    fn checks_string_math_and_system_signatures() {
        check_source(
            "List<String> values = new List<String>{String.valueOf(1)}; \
             String joined = String.join(values, ','); \
             String joinedSet = String.join(new Set<String>(values), ','); \
             Boolean blank = String.isBlank(null); Boolean notBlank = String.isNotBlank(joined); \
             Boolean empty = String.isEmpty(''); Boolean notEmpty = String.isNotEmpty(joined); \
             Integer length = joined.length(); Boolean contains = joined.contains('1'); \
             Boolean starts = joined.startsWith('1'); Boolean ends = joined.endsWith('1'); \
             Boolean exact = joined.equals('1'); Boolean same = joined.equalsIgnoreCase('1'); \
             Integer index = joined.indexOf('1'); \
             String piece = joined.substring(0, 1).trim().toUpperCase().toLowerCase(); \
             String replaced = joined.replace('1', 'one'); \
             Integer absolute = Math.abs(-1); Integer maximum = Math.max(1, 2); \
             Integer minimum = Math.min(1, 2); Integer remainder = Math.mod(5, 2); \
             System.debug(String.join(values, ''));",
        )
        .unwrap();

        let error = check_source("String value = Math.abs('wrong');").unwrap_err();
        assert_eq!(
            error.message,
            "argument 1 to `abs` on Math expects Integer, found String"
        );

        let error = check_source("Boolean value = System.debug('no');").unwrap_err();
        assert_eq!(error.message, "cannot assign void to Boolean");
    }

    #[test]
    fn method_receivers_resolve_variables_before_static_types() {
        let error = check_source("String String = 'value'; String converted = String.valueOf(1);")
            .unwrap_err();
        assert_eq!(error.message, "unknown method `valueOf` on String");
    }

    #[test]
    fn collects_method_signatures_before_checking_bodies_and_resolves_recursion() {
        check_source(
            "Integer first(Integer value) { return second(value); } \
             Integer second(Integer value) { \
                 if (value <= 0) return 0; \
                 return first(value - 1); \
             } \
             System.debug(first(2));",
        )
        .unwrap();

        let error = check_source(
            "Integer choose(Integer value) { return value; } \
             String CHOOSE(Integer value) { return 'duplicate'; }",
        )
        .unwrap_err();
        assert!(error.message.contains("duplicate method overload"));
    }

    #[test]
    fn ranks_exact_object_and_null_overloads() {
        check_source(
            "String kind(String value) { return 'String'; } \
             String kind(Object value) { return 'Object'; } \
             String exact = kind('value'); Object boxed = 1; String broad = kind(boxed); \
             String specificNull = kind(null);",
        )
        .unwrap();

        check_source(
            "String kind(Exception value) { return 'Exception'; } \
             String kind(Object value) { return 'Object'; } \
             NullPointerException error = new NullPointerException(); \
             String specificException = kind(error);",
        )
        .unwrap();

        let error = check_source(
            "String choose(String value) { return 'String'; } \
             String choose(Integer value) { return 'Integer'; } \
             String result = choose(null);",
        )
        .unwrap_err();
        assert!(error.message.contains("ambiguous overload"));

        let error = check_source(
            "String choose(String value) { return 'String'; } \
             String choose(Exception value) { return 'Exception'; } \
             String result = choose(null);",
        )
        .unwrap_err();
        assert!(error.message.contains("ambiguous overload"));

        let error = check_source(
            "String choose(Object left, MathException right) { return 'first'; } \
             String choose(Exception left, Object right) { return 'second'; } \
             MathException error = new MathException(); \
             String result = choose(error, error);",
        )
        .unwrap_err();
        assert!(error.message.contains("ambiguous overload"));
    }

    #[test]
    fn validates_method_return_types_and_definite_completion() {
        check_source(
            "Integer complete(Boolean branch) { \
                 if (branch) return 1; else throw new MathException('failed'); \
             } \
             Integer once() { do { return 1; } while (false); } \
             void done() { return; }",
        )
        .unwrap();

        let error = check_source("Integer incomplete(Boolean branch) { if (branch) return 1; }")
            .unwrap_err();
        assert!(error.message.contains("every path"));

        let error = check_source(
            "Integer broken(Boolean stop) { \
                 do { if (stop) break; return 1; } while (false); \
             }",
        )
        .unwrap_err();
        assert!(error.message.contains("every path"));

        let error = check_source("void wrong() { return 1; }").unwrap_err();
        assert_eq!(error.message, "void method cannot return a value");
    }

    #[test]
    fn validates_exception_catches_accessors_and_casts() {
        check_source(
            "String recover(Object value) { \
                 try { \
                     MathException error = (MathException) value; \
                     throw error; \
                 } catch (MathException error) { \
                     return error.getTypeName() + error.getMessage() \
                         + error.getStackTraceString(); \
                 } finally { System.debug('done'); } \
             } \
             Exception emptyMessage = new Exception(null); \
             throw null;",
        )
        .unwrap();

        let error = check_source(
            "void fail() { \
                 try { throw new MathException(); } \
                 catch (Exception error) {} \
                 catch (MathException specific) {} \
             }",
        )
        .unwrap_err();
        assert!(error.message.contains("unreachable catch"));

        let error = check_source("void fail() { throw 'not an exception'; }").unwrap_err();
        assert!(error.message.contains("requires an Exception"));

        let error = check_source("try {} catch (String problem) {}").unwrap_err();
        assert!(error.message.contains("catch type must be an Exception"));

        let error = check_source("throw new Exception('first', 'second');").unwrap_err();
        assert!(error.message.contains("zero or one argument"));

        let error = check_source(
            "MathException math = new MathException(); \
             IllegalArgumentException unrelated = (IllegalArgumentException) math;",
        )
        .unwrap_err();
        assert!(error.message.contains("cannot cast"));
    }

    #[test]
    fn method_local_names_do_not_resolve_anonymous_or_other_frame_locals() {
        let error = check_source(
            "Integer read() { return outside; } \
             Integer outside = 1; System.debug(read());",
        )
        .unwrap_err();
        assert_eq!(error.message, "unknown variable `outside`");

        let error =
            check_source("Integer same(Integer value) { Integer VALUE = 2; return value; }")
                .unwrap_err();
        assert_eq!(error.message, "duplicate variable `VALUE`");
    }
}
