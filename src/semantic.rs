use crate::{
    ast::{
        AccessorKind, Annotation, AnnotationKind, AssignmentOperator, AssignmentTarget,
        BinaryOperator, CatchClause, ClassDeclaration, ClassKind, ClassMember,
        CollectionInitializer, ConstructorDeclaration, ConstructorDelegationKind, Expression,
        Identifier, MethodDeclaration, Modifier, PostfixOperator, Program, ReturnType, Statement,
        TriggerDeclaration, TypeName, UnaryOperator,
    },
    diagnostic::Diagnostic,
    hir::{
        self, CallTarget, CheckedBinaryOperation, CheckedUnaryOperation, ClassId, ClassMemberId,
        ExpressionType, FieldId, MemberTarget, NumericKind, ObjectTypeId, PlaceTarget,
        PlatformConstructor, ReferenceTarget, TriggerContextVariable,
    },
    platform::{FieldType, SchemaCatalog},
    span::Span,
};
use std::collections::{HashMap, HashSet};

mod flow;
mod intrinsics;
mod overload;
mod queries;

use flow::statement_definitely_returns_or_throws;
use intrinsics::{require_arity, unknown_method};

pub fn check(program: &Program) -> Result<hir::Program, Diagnostic> {
    Checker::new(SchemaCatalog::new()).check_program(program)
}

pub fn check_with_schema(
    program: &Program,
    schema: &SchemaCatalog,
) -> Result<hir::Program, Diagnostic> {
    Checker::new(schema.clone()).check_program(program)
}

pub(crate) fn check_dynamic_soql(
    query: &crate::ast::SoqlQuery,
    schema: &SchemaCatalog,
    expected: Option<&TypeName>,
    bindings: HashMap<String, TypeName>,
) -> Result<hir::CheckedSoqlQuery, Diagnostic> {
    let mut checker = Checker::new(schema.clone());
    checker.scopes[0] = bindings;
    checker.dynamic_query = true;
    checker.soql_type(query, expected)?;
    match checker.queries.remove(&query.span) {
        Some(hir::CheckedQuery::Soql(query)) => Ok(*query),
        _ => Err(Diagnostic::new(
            "dynamic SOQL did not produce a checked plan",
            query.span,
        )),
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HierarchyVisit {
    Unvisited,
    Visiting,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HierarchyTraversal {
    nodes_started: usize,
    edges_examined: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InheritanceTraversal {
    matched: bool,
    nodes_visited: usize,
    edges_examined: usize,
}

struct HierarchyGraph {
    edges: Vec<Vec<usize>>,
}

impl HierarchyGraph {
    fn new(node_count: usize) -> Self {
        Self {
            edges: vec![Vec::new(); node_count],
        }
    }

    fn add_edges(&mut self, node: usize, edges: Vec<usize>) {
        self.edges[node] = edges;
    }

    fn edge_count(&self) -> usize {
        self.edges.iter().map(Vec::len).sum()
    }

    fn validate_acyclic(
        &self,
        classes: &[ClassDeclaration],
    ) -> Result<HierarchyTraversal, Diagnostic> {
        let mut visits = vec![HierarchyVisit::Unvisited; self.edges.len()];
        let mut traversal = HierarchyTraversal {
            nodes_started: 0,
            edges_examined: 0,
        };
        let mut stack = Vec::new();

        for root in 0..self.edges.len() {
            if visits[root] != HierarchyVisit::Unvisited {
                continue;
            }
            visits[root] = HierarchyVisit::Visiting;
            traversal.nodes_started += 1;
            stack.push((root, 0usize));
            while let Some((node, next_edge)) = stack.last_mut() {
                let Some(target) = self.edges[*node].get(*next_edge).copied() else {
                    visits[*node] = HierarchyVisit::Complete;
                    stack.pop();
                    continue;
                };
                *next_edge += 1;
                traversal.edges_examined += 1;
                match visits[target] {
                    HierarchyVisit::Unvisited => {
                        visits[target] = HierarchyVisit::Visiting;
                        traversal.nodes_started += 1;
                        stack.push((target, 0));
                    }
                    HierarchyVisit::Visiting => {
                        let class = &classes[target];
                        return Err(Diagnostic::new(
                            format!("cyclic inheritance involving `{}`", class.name.spelling),
                            class.name.span,
                        ));
                    }
                    HierarchyVisit::Complete => {}
                }
            }
        }
        Ok(traversal)
    }
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
    places: HashMap<Span, PlaceTarget>,
    binary_operations: HashMap<Span, CheckedBinaryOperation>,
    unary_operations: HashMap<Span, CheckedUnaryOperation>,
    type_literals: HashMap<Span, TypeName>,
    queries: HashMap<Span, hir::CheckedQuery>,
    null_aware_queries: HashSet<Span>,
    async_contracts: HashMap<usize, hir::AsyncClassContract>,
    classes: Vec<ClassDeclaration>,
    class_ids: HashMap<String, usize>,
    current_class: Option<usize>,
    current_static: bool,
    current_trigger_object: Option<usize>,
    schema: SchemaCatalog,
    dynamic_query: bool,
}

impl Checker {
    fn new(schema: SchemaCatalog) -> Self {
        Self {
            scopes: vec![HashMap::new()],
            loop_depth: 0,
            return_type: None,
            methods: HashMap::new(),
            expression_types: HashMap::new(),
            calls: HashMap::new(),
            references: HashMap::new(),
            members: HashMap::new(),
            places: HashMap::new(),
            binary_operations: HashMap::new(),
            unary_operations: HashMap::new(),
            type_literals: HashMap::new(),
            queries: HashMap::new(),
            null_aware_queries: HashSet::new(),
            async_contracts: HashMap::new(),
            classes: Vec::new(),
            class_ids: HashMap::new(),
            current_class: None,
            current_static: false,
            current_trigger_object: None,
            schema,
            dynamic_query: false,
        }
    }

    fn check_program(mut self, program: &Program) -> Result<hir::Program, Diagnostic> {
        self.collect_classes(program)?;
        self.collect_method_signatures(program)?;
        self.validate_class_hierarchy()?;
        for class_id in 0..self.classes.len() {
            self.check_class(class_id)?;
        }
        self.check_triggers(program)?;
        for method in &program.methods {
            self.check_method(method)?;
        }
        for statement in &program.statements {
            self.check_statement(statement)?;
        }
        Ok(hir::Program::new(
            program.clone(),
            hir::ProgramFacts {
                expression_types: self.expression_types,
                calls: self.calls,
                references: self.references,
                members: self.members,
                places: self.places,
                binary_operations: self.binary_operations,
                unary_operations: self.unary_operations,
                type_literals: self.type_literals,
                queries: self.queries,
                null_aware_queries: self.null_aware_queries,
                async_contracts: self.async_contracts,
            },
            self.schema,
        ))
    }

    fn check_triggers(&mut self, program: &Program) -> Result<(), Diagnostic> {
        let mut names = std::collections::HashSet::new();
        for trigger in &program.triggers {
            if !names.insert(trigger.name.canonical.clone()) {
                return Err(Diagnostic::new(
                    format!("duplicate trigger `{}`", trigger.name.spelling),
                    trigger.name.span,
                ));
            }
            self.check_trigger(trigger)?;
        }
        Ok(())
    }

    fn check_trigger(&mut self, trigger: &TriggerDeclaration) -> Result<(), Diagnostic> {
        if !trigger.object.type_arguments.is_empty() {
            return Err(Diagnostic::new(
                "trigger SObject types cannot have generic arguments",
                trigger.object.span,
            ));
        }
        let object_id = self
            .schema
            .object_index(&trigger.object.spelling)
            .ok_or_else(|| {
                Diagnostic::new(
                    format!("unknown trigger SObject `{}`", trigger.object.spelling),
                    trigger.object.span,
                )
            })?;
        let mut events = std::collections::HashSet::new();
        for event in &trigger.events {
            if !events.insert(*event) {
                return Err(Diagnostic::new(
                    "duplicate trigger event",
                    trigger.name.span,
                ));
            }
        }

        let saved_scopes = std::mem::replace(&mut self.scopes, vec![HashMap::new()]);
        let saved_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let saved_return_type = self.return_type.replace(ReturnType::Void);
        let saved_class = self.current_class.take();
        let saved_static = std::mem::replace(&mut self.current_static, true);
        let saved_trigger = self.current_trigger_object.replace(object_id);
        let result = self.check_method_body(&trigger.body);
        self.scopes = saved_scopes;
        self.loop_depth = saved_loop_depth;
        self.return_type = saved_return_type;
        self.current_class = saved_class;
        self.current_static = saved_static;
        self.current_trigger_object = saved_trigger;
        result
    }

    fn collect_classes(&mut self, program: &Program) -> Result<(), Diagnostic> {
        self.classes = program.classes.clone();
        let mut short_names = HashMap::<String, Vec<usize>>::new();
        for (class_id, class) in self.classes.iter().enumerate() {
            if let Some(previous) = self
                .class_ids
                .insert(class.qualified_name.canonical.clone(), class_id)
            {
                let original = &self.classes[previous];
                return Err(Diagnostic::new(
                    format!(
                        "duplicate type `{}`; first declared as `{}`",
                        class.qualified_name.spelling, original.qualified_name.spelling
                    ),
                    class.name.span,
                ));
            }
            short_names
                .entry(class.name.canonical.clone())
                .or_default()
                .push(class_id);
            if class.enclosing_type.is_none()
                && (self.schema.object(&class.name.spelling).is_ok()
                    || class.name.canonical == "sobject")
            {
                return Err(Diagnostic::new(
                    format!(
                        "type `{}` conflicts with an SObject schema type",
                        class.name.spelling
                    ),
                    class.name.span,
                ));
            }
        }
        for (canonical, ids) in short_names {
            if let [class_id] = ids.as_slice() {
                self.class_ids.entry(canonical).or_insert(*class_id);
            }
        }
        Ok(())
    }

    fn validate_class_hierarchy(&self) -> Result<(), Diagnostic> {
        let mut graph = HierarchyGraph::new(self.classes.len());
        for (class_id, class) in self.classes.iter().enumerate() {
            self.validate_type_declaration_header(class)?;
            graph.add_edges(class_id, self.validated_hierarchy_edges(class)?);
        }
        let traversal = graph.validate_acyclic(&self.classes)?;
        debug_assert!(traversal.nodes_started <= self.classes.len());
        debug_assert!(traversal.edges_examined <= graph.edge_count());
        Ok(())
    }

    fn validate_type_declaration_header(&self, class: &ClassDeclaration) -> Result<(), Diagnostic> {
        self.validate_test_class(class)?;
        validate_modifier_set(&class.modifiers, class.name.span, "type")?;
        let mut rejected = vec![Modifier::Override];
        let subject = if class.enclosing_type.is_some() {
            "nested type"
        } else {
            rejected.extend([Modifier::Protected, Modifier::Static]);
            if !class_is_test(class) {
                rejected.push(Modifier::Private);
            }
            "top-level type"
        };
        if class.kind == ClassKind::Enum {
            rejected.extend([Modifier::Virtual, Modifier::Abstract]);
            if class.superclass.is_some() || !class.interfaces.is_empty() {
                return Err(Diagnostic::new(
                    "enums cannot extend classes or implement interfaces",
                    class.name.span,
                ));
            }
        }
        reject_modifiers(&class.modifiers, &rejected, class.name.span, subject)?;
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
        Ok(())
    }

    fn validated_hierarchy_edges(
        &self,
        class: &ClassDeclaration,
    ) -> Result<Vec<usize>, Diagnostic> {
        let mut edges = Vec::new();
        let mut seen_interfaces = HashSet::new();
        if let Some(superclass) = &class.superclass {
            if let Some(parent_id) = self.validated_superclass(class, superclass)? {
                edges.push(parent_id);
            } else {
                return Ok(edges);
            }
        }

        for interface in &class.interfaces {
            if !seen_interfaces.insert(interface.canonical.clone()) {
                return Err(Diagnostic::new(
                    format!(
                        "a type cannot implement interface `{}` more than once",
                        interface.spelling
                    ),
                    interface.span,
                ));
            }
            if let Some(interface_id) = self.validated_interface(class, interface)? {
                edges.push(interface_id);
            }
        }
        Ok(edges)
    }

    fn validated_superclass(
        &self,
        class: &ClassDeclaration,
        superclass: &crate::ast::NamedType,
    ) -> Result<Option<usize>, Diagnostic> {
        if !superclass.type_arguments.is_empty() {
            return Err(Diagnostic::new(
                "generic arguments are unsupported on inherited user-defined types",
                superclass.span,
            ));
        }
        if superclass.canonical == "exception" {
            if class.kind != ClassKind::Class {
                return Err(Diagnostic::new(
                    "only classes can extend Exception",
                    superclass.span,
                ));
            }
            if !class.interfaces.is_empty() {
                return Err(Diagnostic::new(
                    "custom exception classes cannot implement interfaces in the supported profile",
                    class.interfaces[0].span,
                ));
            }
            return Ok(None);
        }
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
        self.validate_superclass_edge(class, superclass, parent_id)?;
        Ok(Some(parent_id))
    }

    fn validated_interface(
        &self,
        class: &ClassDeclaration,
        interface: &crate::ast::NamedType,
    ) -> Result<Option<usize>, Diagnostic> {
        if is_platform_async_interface(&interface.canonical) {
            self.validate_platform_interface_edge(class, interface)?;
            return Ok(None);
        }
        if !interface.type_arguments.is_empty() {
            return Err(Diagnostic::new(
                format!(
                    "generic arguments on user-defined interface `{}` are unsupported",
                    interface.spelling
                ),
                interface.span,
            ));
        }
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
        Ok(Some(interface_id))
    }

    fn validate_superclass_edge(
        &self,
        class: &ClassDeclaration,
        superclass: &crate::ast::NamedType,
        parent_id: usize,
    ) -> Result<(), Diagnostic> {
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
        Ok(())
    }

    fn validate_platform_interface_edge(
        &self,
        class: &ClassDeclaration,
        interface: &crate::ast::NamedType,
    ) -> Result<(), Diagnostic> {
        if class.kind != ClassKind::Class {
            return Err(Diagnostic::new(
                "platform async interfaces can only be implemented by classes",
                interface.span,
            ));
        }
        if is_batchable_interface(&interface.canonical) {
            let [argument] = interface.type_arguments.as_slice() else {
                return Err(Diagnostic::new(
                    "Database.Batchable requires exactly one type argument",
                    interface.span,
                ));
            };
            return self.validate_type(&argument.ty, argument.span);
        }
        if !interface.type_arguments.is_empty() {
            return Err(Diagnostic::new(
                format!("`{}` does not accept generic arguments", interface.spelling),
                interface.span,
            ));
        }
        Ok(())
    }

    fn validate_test_class(&self, class: &ClassDeclaration) -> Result<(), Diagnostic> {
        let mut saw_is_test = false;
        for annotation in &class.annotations {
            match annotation.kind {
                AnnotationKind::IsTest { see_all_data } => {
                    if saw_is_test {
                        return Err(Diagnostic::new(
                            "duplicate `@IsTest` annotation on class",
                            annotation.span,
                        ));
                    }
                    saw_is_test = true;
                    if see_all_data == Some(true) {
                        return Err(Diagnostic::new(
                            "`@IsTest(SeeAllData=true)` is unsupported without an org data host",
                            annotation.span,
                        ));
                    }
                }
                AnnotationKind::TestSetup => {
                    return Err(Diagnostic::new(
                        "`@TestSetup` is only valid on methods",
                        annotation.span,
                    ));
                }
                AnnotationKind::Future => {
                    return Err(Diagnostic::new(
                        "`@future` is only valid on methods",
                        annotation.span,
                    ));
                }
                AnnotationKind::Other => return Err(unsupported_annotation(annotation)),
            }
        }
        if saw_is_test && class.kind != ClassKind::Class {
            return Err(Diagnostic::new(
                "`@IsTest` is only valid on classes",
                class.name.span,
            ));
        }
        Ok(())
    }

    fn check_class(&mut self, class_id: usize) -> Result<(), Diagnostic> {
        self.validate_class_member_declarations(class_id)?;
        self.validate_class_contracts(class_id)?;
        self.validate_async_contract(class_id)?;

        let saved_class = self.current_class.replace(class_id);
        let saved_static = self.current_static;
        let members = self.classes[class_id].members.clone();
        let result = (|| {
            for member in &members {
                self.check_class_member(member)?;
            }
            self.validate_constructor_delegation_cycles(class_id)?;
            Ok(())
        })();
        self.current_class = saved_class;
        self.current_static = saved_static;
        result
    }

    fn check_class_member(&mut self, member: &ClassMember) -> Result<(), Diagnostic> {
        match member {
            ClassMember::Field(field) => {
                reject_unsupported_annotations(&field.annotations)?;
                self.validate_type(&field.ty, field.name.span)?;
                self.current_static = field.modifiers.contains(&Modifier::Static);
                if let Some(initializer) = &field.initializer {
                    let actual = self.expression_type_for_expected(initializer, &field.ty)?;
                    self.require_assignable(&field.ty, &actual, initializer.span())?;
                }
                Ok(())
            }
            ClassMember::FieldGroup(group) => Err(Diagnostic::new(
                "multi-declarator fields are parsed but unsupported by the active compatibility profile",
                group.span,
            )),
            ClassMember::Constructor(constructor) => {
                reject_unsupported_annotations(&constructor.annotations)?;
                self.current_static = false;
                self.check_constructor(constructor)
            }
            ClassMember::Method(method) => {
                self.current_static = method.modifiers.contains(&Modifier::Static);
                self.check_method(method)
            }
            ClassMember::Property(property) => {
                reject_unsupported_annotations(&property.annotations)?;
                self.validate_type(&property.ty, property.name.span)?;
                self.current_static = property.modifiers.contains(&Modifier::Static);
                self.check_property_accessors(property)
            }
            ClassMember::Initializer(initializer) => {
                self.current_static = initializer.is_static;
                self.check_method_body(&initializer.body)
            }
        }
    }

    fn validate_class_member_declarations(&self, class_id: usize) -> Result<(), Diagnostic> {
        let class = &self.classes[class_id];
        let mut values = HashMap::<String, Span>::new();
        let mut methods = HashMap::<(String, Vec<TypeName>), Span>::new();
        let mut constructors = HashMap::<Vec<TypeName>, Span>::new();
        self.validate_enum_constants(class, &mut values)?;
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
                ClassMember::FieldGroup(group) => {
                    return Err(Diagnostic::new(
                        "multi-declarator fields are parsed but unsupported by the active compatibility profile",
                        group.span,
                    ));
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
                    self.validate_constructor_declaration(class, constructor, &mut constructors)?;
                }
                ClassMember::Method(method) => {
                    self.validate_test_method(class, method)?;
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
                ClassMember::Initializer(initializer) => {
                    self.validate_initializer_declaration(class, initializer)?;
                }
            }
        }
        Ok(())
    }

    fn validate_enum_constants(
        &self,
        class: &ClassDeclaration,
        values: &mut HashMap<String, Span>,
    ) -> Result<(), Diagnostic> {
        for constant in &class.enum_constants {
            if values
                .insert(constant.canonical.clone(), constant.span)
                .is_some()
            {
                return Err(Diagnostic::new(
                    format!("duplicate enum constant `{}`", constant.spelling),
                    constant.span,
                ));
            }
        }
        Ok(())
    }

    fn validate_constructor_declaration(
        &self,
        class: &ClassDeclaration,
        constructor: &ConstructorDeclaration,
        constructors: &mut HashMap<Vec<TypeName>, Span>,
    ) -> Result<(), Diagnostic> {
        if class.kind == ClassKind::Enum {
            return Err(Diagnostic::new(
                "enum constructors are not supported",
                constructor.name.span,
            ));
        }
        validate_modifier_set(&constructor.modifiers, constructor.name.span, "constructor")?;
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
        Ok(())
    }

    fn validate_initializer_declaration(
        &self,
        class: &ClassDeclaration,
        initializer: &crate::ast::InitializerBlock,
    ) -> Result<(), Diagnostic> {
        if class.kind == ClassKind::Interface {
            return Err(Diagnostic::new(
                "interfaces cannot declare initializer blocks",
                initializer.span,
            ));
        }
        Ok(())
    }

    fn validate_test_method(
        &self,
        class: &ClassDeclaration,
        method: &MethodDeclaration,
    ) -> Result<(), Diagnostic> {
        let mut test_kind = None;
        let mut future = None;
        for annotation in &method.annotations {
            let kind_name = match annotation.kind {
                AnnotationKind::IsTest { see_all_data } => {
                    if see_all_data == Some(true) {
                        return Err(Diagnostic::new(
                            "`@IsTest(SeeAllData=true)` is unsupported without an org data host",
                            annotation.span,
                        ));
                    }
                    "@IsTest"
                }
                AnnotationKind::TestSetup => "@TestSetup",
                AnnotationKind::Future if future.replace(annotation.span).is_some() => {
                    return Err(Diagnostic::new(
                        "duplicate `@future` annotation",
                        annotation.span,
                    ));
                }
                AnnotationKind::Future => continue,
                AnnotationKind::Other => return Err(unsupported_annotation(annotation)),
            };
            if test_kind.is_some() {
                return Err(Diagnostic::new(
                    "test methods may have only one test annotation",
                    annotation.span,
                ));
            }
            test_kind = Some(kind_name);
        }
        if let Some(span) = future {
            if test_kind.is_some() {
                return Err(Diagnostic::new(
                    "`@future` cannot be combined with a test annotation",
                    span,
                ));
            }
            if !method.modifiers.contains(&Modifier::Static)
                || !(method.modifiers.contains(&Modifier::Public)
                    || method.modifiers.contains(&Modifier::Global))
                || method.return_type != ReturnType::Void
                || method.body.is_none()
            {
                return Err(Diagnostic::new(
                    "`@future` method must be public or global static void with a body",
                    method.name.span,
                ));
            }
            if let Some(parameter) = method
                .parameters
                .iter()
                .find(|parameter| !is_future_parameter_type(&parameter.ty))
            {
                return Err(Diagnostic::new(
                    format!(
                        "`@future` parameter `{}` has unsupported type {}; only primitive values and Lists or Sets of primitive values are supported",
                        parameter.name.spelling,
                        parameter.ty.apex_name()
                    ),
                    parameter.span,
                ));
            }
            return Ok(());
        }
        let Some(kind_name) = test_kind else {
            return Ok(());
        };
        if !class_is_test(class) {
            return Err(Diagnostic::new(
                format!("`{kind_name}` methods require an `@IsTest` class"),
                method.name.span,
            ));
        }
        if !method.modifiers.contains(&Modifier::Static)
            || method.return_type != ReturnType::Void
            || !method.parameters.is_empty()
            || method.body.is_none()
        {
            return Err(Diagnostic::new(
                format!("`{kind_name}` method must be static void with no parameters and a body"),
                method.name.span,
            ));
        }
        Ok(())
    }

    fn validate_class_contracts(&self, class_id: usize) -> Result<(), Diagnostic> {
        let class = &self.classes[class_id];
        self.validate_required_method_returns(class_id)?;
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

        self.validate_concrete_class_requirements(class_id)
    }

    fn validate_concrete_class_requirements(&self, class_id: usize) -> Result<(), Diagnostic> {
        let class = &self.classes[class_id];
        if class.kind != ClassKind::Class || class.modifiers.contains(&Modifier::Abstract) {
            return Ok(());
        }
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
        Ok(())
    }

    fn validate_required_method_returns(&self, class_id: usize) -> Result<(), Diagnostic> {
        let mut returns = HashMap::<(String, Vec<TypeName>), ReturnType>::new();
        let mut pending = vec![class_id];
        let mut visited = vec![false; self.classes.len()];
        while let Some(current) = pending.pop() {
            if std::mem::replace(&mut visited[current], true) {
                continue;
            }
            if self.classes[current].kind == ClassKind::Interface {
                for signature in self.own_class_methods(current) {
                    let key = (signature.name.clone(), signature.parameter_types.clone());
                    if let Some(previous) = returns.get(&key)
                        && previous != &signature.return_type
                    {
                        let method = self.method_declaration(signature.target);
                        return Err(Diagnostic::new(
                            format!(
                                "inherited interface method `{}` has conflicting return types {} and {}",
                                method.name.spelling,
                                previous.apex_name(),
                                signature.return_type.apex_name()
                            ),
                            method.name.span,
                        ));
                    }
                    returns.insert(key, signature.return_type);
                }
            }
            if let Some(parent) = self.parent_class_id(current) {
                pending.push(parent);
            }
            for interface in &self.classes[current].interfaces {
                if !is_platform_async_interface(&interface.canonical) {
                    pending.push(self.class_ids[&interface.canonical]);
                }
            }
        }
        Ok(())
    }

    fn validate_async_contract(&mut self, class_id: usize) -> Result<(), Diagnostic> {
        let mut contract = hir::AsyncClassContract::default();

        if self.classes[class_id]
            .interfaces
            .iter()
            .any(|interface| is_queueable_interface(&interface.canonical))
        {
            contract.queueable = Some(self.require_async_method(
                class_id,
                "execute",
                &[TypeName::QueueableContext],
                &ReturnType::Void,
                "Queueable",
            )?);
        }

        if self.classes[class_id]
            .interfaces
            .iter()
            .any(|interface| is_schedulable_interface(&interface.canonical))
        {
            contract.schedulable = Some(self.require_async_method(
                class_id,
                "execute",
                &[TypeName::SchedulableContext],
                &ReturnType::Void,
                "Schedulable",
            )?);
        }

        let batch_scope_type = self.classes[class_id]
            .interfaces
            .iter()
            .find(|interface| is_batchable_interface(&interface.canonical))
            .and_then(|interface| interface.type_arguments.first())
            .map(|argument| argument.ty.clone());
        if let Some(scope_type) = batch_scope_type {
            contract.batch = Some(self.validate_batch_contract(class_id, scope_type)?);
        }

        if contract != hir::AsyncClassContract::default() {
            self.async_contracts.insert(class_id, contract);
        }
        Ok(())
    }

    fn validate_batch_contract(
        &self,
        class_id: usize,
        scope_type: TypeName,
    ) -> Result<hir::BatchContract, Diagnostic> {
        let start_candidates = self
            .async_methods_named(class_id, "start")
            .collect::<Vec<_>>();
        let [(start, start_method)] = start_candidates.as_slice() else {
            return Err(async_contract_error(
                &self.classes[class_id],
                "Batchable requires exactly one public or global `start(Database.BatchableContext)` method",
            ));
        };
        if start_method.parameters.len() != 1
            || start_method.parameters[0].ty != TypeName::BatchableContext
        {
            return Err(async_contract_error(
                &self.classes[class_id],
                "Batchable `start` must accept Database.BatchableContext",
            ));
        }
        let expected_start_return = ReturnType::Value(TypeName::List(Box::new(scope_type.clone())));
        if start_method.return_type != expected_start_return
            && start_method.return_type != ReturnType::Value(TypeName::QueryLocator)
        {
            return Err(async_contract_error(
                &self.classes[class_id],
                format!(
                    "Batchable `start` must return {} or Database.QueryLocator to match the declared Database.Batchable type argument",
                    expected_start_return.apex_name()
                ),
            ));
        }
        let execute = self.require_async_method(
            class_id,
            "execute",
            &[
                TypeName::BatchableContext,
                TypeName::List(Box::new(scope_type.clone())),
            ],
            &ReturnType::Void,
            "Batchable",
        )?;
        let finish = self.require_async_method(
            class_id,
            "finish",
            &[TypeName::BatchableContext],
            &ReturnType::Void,
            "Batchable",
        )?;
        Ok(hir::BatchContract {
            start: *start,
            execute,
            finish,
            scope_type,
        })
    }

    fn async_methods_named<'a>(
        &'a self,
        class_id: usize,
        canonical: &'a str,
    ) -> impl Iterator<Item = (ClassMemberId, &'a MethodDeclaration)> + 'a {
        self.classes[class_id]
            .members
            .iter()
            .enumerate()
            .filter_map(move |(member_id, member)| {
                let ClassMember::Method(method) = member else {
                    return None;
                };
                (method.name.canonical == canonical
                    && !method.modifiers.contains(&Modifier::Static)
                    && (method.modifiers.contains(&Modifier::Public)
                        || method.modifiers.contains(&Modifier::Global))
                    && method.body.is_some())
                .then_some((
                    ClassMemberId {
                        class_id,
                        member_id,
                    },
                    method,
                ))
            })
    }

    fn require_async_method(
        &self,
        class_id: usize,
        name: &str,
        parameters: &[TypeName],
        return_type: &ReturnType,
        interface: &str,
    ) -> Result<ClassMemberId, Diagnostic> {
        let matches = self
            .async_methods_named(class_id, name)
            .filter(|(_, method)| {
                method.return_type == *return_type
                    && method
                        .parameters
                        .iter()
                        .map(|parameter| &parameter.ty)
                        .eq(parameters.iter())
            })
            .map(|(target, _)| target)
            .collect::<Vec<_>>();
        let [target] = matches.as_slice() else {
            let signature = parameters
                .iter()
                .map(TypeName::apex_name)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(async_contract_error(
                &self.classes[class_id],
                format!(
                    "{interface} requires exactly one public or global `{name}({signature})` method returning {}",
                    return_type.apex_name()
                ),
            ));
        };
        Ok(*target)
    }

    fn parent_class_id(&self, class_id: usize) -> Option<usize> {
        self.classes[class_id]
            .superclass
            .as_ref()
            .and_then(|name| self.class_ids.get(&name.canonical).copied())
    }

    fn class_is_custom_exception(&self, class_id: usize) -> bool {
        let mut current = Some(class_id);
        let mut visited = vec![false; self.classes.len()];
        while let Some(id) = current {
            if std::mem::replace(&mut visited[id], true) {
                return false;
            }
            let Some(superclass) = &self.classes[id].superclass else {
                return false;
            };
            if superclass.canonical == "exception" {
                return true;
            }
            current = self.class_ids.get(&superclass.canonical).copied();
        }
        false
    }

    fn is_exception_type(&self, ty: &TypeName) -> bool {
        ty.is_exception()
            || matches!(
                ty,
                TypeName::Custom(name)
                    if self.class_ids
                        .get(&name.canonical)
                        .is_some_and(|class_id| self.class_is_custom_exception(*class_id))
            )
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
        let mut visited_interfaces = vec![false; self.classes.len()];
        let mut cursor = Some(class_id);
        while let Some(id) = cursor {
            for method in self.own_class_methods(id) {
                if self.method_declaration(method.target).body.is_none() {
                    push_unique_signature(&mut required, method);
                }
            }
            for interface in &self.classes[id].interfaces {
                if is_platform_async_interface(&interface.canonical) {
                    continue;
                }
                let interface_id = self.class_ids[&interface.canonical];
                self.collect_interface_methods(
                    interface_id,
                    &mut required,
                    &mut visited_interfaces,
                );
            }
            cursor = self.parent_class_id(id);
        }
        required
    }

    fn collect_interface_methods(
        &self,
        interface_id: usize,
        required: &mut Vec<ClassMethodSignature>,
        visited: &mut [bool],
    ) {
        let mut pending = vec![interface_id];
        while let Some(current) = pending.pop() {
            if std::mem::replace(&mut visited[current], true) {
                continue;
            }
            for method in self.own_class_methods(current) {
                push_unique_signature(required, method);
            }
            if let Some(parent) = self.parent_class_id(current) {
                pending.push(parent);
            }
            for interface in self.classes[current].interfaces.iter().rev() {
                if !is_platform_async_interface(&interface.canonical) {
                    pending.push(self.class_ids[&interface.canonical]);
                }
            }
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
            TypeName::Custom(name)
                if !self.class_ids.contains_key(&name.canonical)
                    && self.schema.object(&name.spelling).is_err()
                    && name.canonical != "sobject" =>
            {
                Err(Diagnostic::new(
                    format!("unknown type `{}`", name.spelling),
                    span,
                ))
            }
            TypeName::Custom(name) => {
                if let Some(class_id) = self.class_ids.get(&name.canonical).copied() {
                    self.ensure_type_access(class_id, span)
                } else {
                    Ok(())
                }
            }
            TypeName::List(element) | TypeName::Set(element) | TypeName::Iterable(element) => {
                self.validate_type(element, span)
            }
            TypeName::Map(key, value) => {
                self.validate_type(key, span)?;
                self.validate_type(value, span)
            }
            _ => Ok(()),
        }
    }

    fn ensure_type_access(&self, class_id: usize, span: Span) -> Result<(), Diagnostic> {
        let class = &self.classes[class_id];
        if class.enclosing_type.is_none()
            || class.modifiers.contains(&Modifier::Public)
            || class.modifiers.contains(&Modifier::Global)
        {
            return Ok(());
        }
        let same_outer = self.current_class.is_some_and(|current| {
            outermost_type(&self.classes[current].qualified_name.canonical)
                == outermost_type(&class.qualified_name.canonical)
        });
        if same_outer
            || (class.modifiers.contains(&Modifier::Protected)
                && self
                    .current_class
                    .is_some_and(|current| self.class_is_or_inherits(current, class_id)))
        {
            Ok(())
        } else {
            Err(Diagnostic::new(
                format!(
                    "nested type `{}` is not accessible here",
                    class.qualified_name.spelling
                ),
                span,
            ))
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
        if (*actual == TypeName::Integer && matches!(expected, TypeName::Long | TypeName::Decimal))
            || (*actual == TypeName::Long && *expected == TypeName::Decimal)
        {
            return true;
        }
        if let (TypeName::List(actual) | TypeName::Set(actual), TypeName::Iterable(expected)) =
            (actual, expected)
        {
            return actual == expected;
        }
        if *expected == TypeName::Exception && self.is_exception_type(actual) {
            return true;
        }
        if self.is_sobject_type(actual) && self.is_dynamic_sobject_type(expected) {
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
        self.class_inheritance_traversal(actual_id, expected_id)
            .matched
    }

    fn class_inheritance_traversal(
        &self,
        actual_id: usize,
        expected_id: usize,
    ) -> InheritanceTraversal {
        let mut traversal = InheritanceTraversal {
            matched: false,
            nodes_visited: 0,
            edges_examined: 0,
        };
        let mut visited = vec![false; self.classes.len()];
        let mut pending = vec![actual_id];
        while let Some(class_id) = pending.pop() {
            if visited[class_id] {
                continue;
            }
            visited[class_id] = true;
            traversal.nodes_visited += 1;
            if class_id == expected_id {
                traversal.matched = true;
                break;
            }
            for interface in self.classes[class_id].interfaces.iter().rev() {
                if let Some(interface_id) = self.class_ids.get(&interface.canonical).copied() {
                    traversal.edges_examined += 1;
                    pending.push(interface_id);
                }
            }
            if let Some(parent_id) = self.parent_class_id(class_id) {
                traversal.edges_examined += 1;
                pending.push(parent_id);
            }
        }
        debug_assert!(traversal.nodes_visited <= self.classes.len());
        traversal
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
            if let Some(delegation) = &constructor.delegation {
                self.check_constructor_delegation(constructor, delegation)?;
            }
            self.check_method_body(&constructor.body)
        })();
        self.scopes = saved_scopes;
        self.loop_depth = saved_loop_depth;
        self.return_type = saved_return_type;
        result
    }

    fn check_constructor_delegation(
        &mut self,
        constructor: &ConstructorDeclaration,
        delegation: &crate::ast::ConstructorDelegation,
    ) -> Result<(), Diagnostic> {
        let current = self
            .current_class
            .expect("constructors have a declaring class");
        let target_class = match delegation.kind {
            ConstructorDelegationKind::This => current,
            ConstructorDelegationKind::Super => self.parent_class_id(current).ok_or_else(|| {
                Diagnostic::new(
                    "`super(...)` requires a user-defined superclass",
                    delegation.span,
                )
            })?,
        };
        let constructors =
            self.delegated_constructor_candidates(current, target_class, constructor.span);
        if constructors.is_empty() {
            return self.check_implicit_delegated_constructor(target_class, delegation);
        }
        let argument_types = delegation
            .arguments
            .iter()
            .map(|argument| self.expression_type(argument))
            .collect::<Result<Vec<_>, _>>()?;
        let applicable = constructors
            .iter()
            .filter(|(_, candidate)| {
                candidate.parameters.len() == argument_types.len()
                    && candidate
                        .parameters
                        .iter()
                        .zip(&argument_types)
                        .all(|(parameter, actual)| self.is_assignable(&parameter.ty, actual))
            })
            .collect::<Vec<_>>();
        let selected = self.select_constructor(&applicable).ok_or_else(|| {
            Diagnostic::new("no unique matching delegated constructor", delegation.span)
        })?;
        self.ensure_member_access(selected.0, &selected.1.modifiers, delegation.span)?;
        self.calls.insert(
            delegation.span,
            CallTarget::Constructor {
                class_id: target_class,
                member_id: Some(selected.0.member_id),
            },
        );
        Ok(())
    }

    fn delegated_constructor_candidates(
        &self,
        current: usize,
        target_class: usize,
        current_constructor_span: Span,
    ) -> Vec<(ClassMemberId, ConstructorDeclaration)> {
        self.classes[target_class]
            .members
            .iter()
            .enumerate()
            .filter_map(|(member_id, member)| {
                let ClassMember::Constructor(candidate) = member else {
                    return None;
                };
                if target_class == current && candidate.span == current_constructor_span {
                    return None;
                }
                Some((
                    ClassMemberId {
                        class_id: target_class,
                        member_id,
                    },
                    candidate.clone(),
                ))
            })
            .collect()
    }

    fn check_implicit_delegated_constructor(
        &mut self,
        target_class: usize,
        delegation: &crate::ast::ConstructorDelegation,
    ) -> Result<(), Diagnostic> {
        for argument in &delegation.arguments {
            self.expression_type(argument)?;
        }
        if delegation.arguments.is_empty() && delegation.kind == ConstructorDelegationKind::Super {
            self.calls.insert(
                delegation.span,
                CallTarget::Constructor {
                    class_id: target_class,
                    member_id: None,
                },
            );
            Ok(())
        } else {
            Err(Diagnostic::new(
                "no matching delegated constructor",
                delegation.span,
            ))
        }
    }

    fn validate_constructor_delegation_cycles(&self, class_id: usize) -> Result<(), Diagnostic> {
        let constructors = self.classes[class_id]
            .members
            .iter()
            .enumerate()
            .filter_map(|(member_id, member)| {
                matches!(member, ClassMember::Constructor(_)).then_some(member_id)
            })
            .collect::<Vec<_>>();
        for root in constructors {
            let mut seen = HashSet::new();
            let mut current = root;
            loop {
                if !seen.insert(current) {
                    let constructor = match &self.classes[class_id].members[current] {
                        ClassMember::Constructor(constructor) => constructor,
                        _ => unreachable!(),
                    };
                    return Err(Diagnostic::new(
                        "cyclic `this(...)` constructor delegation",
                        constructor.span,
                    ));
                }
                let ClassMember::Constructor(constructor) =
                    &self.classes[class_id].members[current]
                else {
                    break;
                };
                let Some(delegation) = &constructor.delegation else {
                    break;
                };
                let Some(CallTarget::Constructor {
                    class_id: target_class,
                    member_id: Some(target_member),
                }) = self.calls.get(&delegation.span).copied()
                else {
                    break;
                };
                if target_class != class_id || delegation.kind != ConstructorDelegationKind::This {
                    break;
                }
                current = target_member;
            }
        }
        Ok(())
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
            declaration @ Statement::VariableDeclaration { .. }
            | declaration @ Statement::LocalDeclaration { .. }
            | declaration @ Statement::Sequence { .. } => {
                self.check_declaration_statement(declaration)
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
            Statement::Switch { span, .. } => Err(Diagnostic::new(
                "`switch on`/`when` is parsed but unsupported by the active compatibility profile",
                *span,
            )),
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
                let actual_element_type = self.iterable_element_type(iterable)?;
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
            control @ Statement::Break { .. } | control @ Statement::Continue { .. } => {
                self.check_loop_control(control)
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
                if matches!(&actual, ExpressionType::Value(ty) if self.is_exception_type(ty))
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
            Statement::Dml {
                value, external_id, ..
            } => self.check_dml_statement(value, external_id.as_ref()),
            Statement::Return { value, span } => self.check_return(value.as_ref(), *span),
        }
    }

    fn check_declaration_statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match statement {
            Statement::VariableDeclaration {
                ty,
                name,
                initializer,
                ..
            } => {
                self.check_declarator(ty, name, Some(initializer))?;
                Ok(())
            }
            Statement::LocalDeclaration {
                modifiers,
                ty,
                declarators,
                ..
            } => {
                if let Some(modifier) = modifiers.first() {
                    return Err(Diagnostic::new(
                        format!(
                            "local modifier `{}` is parsed but unsupported by the active compatibility profile",
                            modifier_name(*modifier)
                        ),
                        declarators
                            .first()
                            .map_or_else(|| statement.span(), |declarator| declarator.span),
                    ));
                }
                for declarator in declarators {
                    self.check_declarator(ty, &declarator.name, declarator.initializer.as_ref())?;
                }
                Ok(())
            }
            Statement::Sequence { statements, .. } => {
                for statement in statements {
                    self.check_statement(statement)?;
                }
                Ok(())
            }
            _ => unreachable!("caller selects declarations and sequences"),
        }
    }

    fn check_declarator(
        &mut self,
        ty: &TypeName,
        name: &Identifier,
        initializer: Option<&Expression>,
    ) -> Result<(), Diagnostic> {
        self.validate_type(ty, name.span)?;
        if self.current_scope().contains_key(&name.canonical) {
            return Err(Diagnostic::new(
                format!("duplicate variable `{}`", name.spelling),
                name.span,
            ));
        }
        if let Some(initializer) = initializer {
            let actual = self.expression_type_for_expected(initializer, ty)?;
            self.require_assignable(ty, &actual, initializer.span())?;
        }
        self.current_scope_mut()
            .insert(name.canonical.clone(), ty.clone());
        Ok(())
    }

    fn check_loop_control(&self, statement: &Statement) -> Result<(), Diagnostic> {
        if self.loop_depth > 0 {
            return Ok(());
        }
        let keyword = if matches!(statement, Statement::Break { .. }) {
            "break"
        } else {
            "continue"
        };
        Err(Diagnostic::new(
            format!("`{keyword}` is only valid inside a loop"),
            statement.span(),
        ))
    }

    fn check_dml_statement(
        &mut self,
        value: &Expression,
        external_id: Option<&Identifier>,
    ) -> Result<(), Diagnostic> {
        if let Some(external_id) = external_id {
            return Err(Diagnostic::new(
                "external-ID DML syntax is parsed but unsupported by the active compatibility profile",
                external_id.span,
            ));
        }
        self.check_dml_value(value)
    }

    fn iterable_element_type(&mut self, iterable: &Expression) -> Result<TypeName, Diagnostic> {
        match self.expression_type(iterable)? {
            ExpressionType::Value(TypeName::List(element))
            | ExpressionType::Value(TypeName::Set(element))
            | ExpressionType::Value(TypeName::Iterable(element)) => Ok(*element),
            other => Err(Diagnostic::new(
                format!(
                    "enhanced for-loop requires List or Set, found {}",
                    other.name()
                ),
                iterable.span(),
            )),
        }
    }

    fn check_catches(&mut self, catches: &[CatchClause]) -> Result<(), Diagnostic> {
        let mut catches_everything = false;
        let mut seen = Vec::new();
        for catch in catches {
            if !self.is_exception_type(&catch.exception_type) {
                return Err(Diagnostic::new(
                    format!(
                        "catch type must be an Exception, found {}",
                        catch.exception_type.apex_name()
                    ),
                    catch.span,
                ));
            }
            if self.catch_is_unreachable(&catch.exception_type, &seen, catches_everything) {
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

    fn catch_is_unreachable(
        &self,
        exception_type: &TypeName,
        seen: &[TypeName],
        catches_everything: bool,
    ) -> bool {
        catches_everything
            || seen
                .iter()
                .any(|previous| self.is_subtype(exception_type, previous))
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
                let actual = self.expression_type_for_expected(value, &expected)?;
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
        if let Some(ty) = self.expression_types.get(&expression.span()).cloned() {
            return Ok(ty);
        }
        let ty = self.expression_type_inner(expression)?;
        self.expression_types.insert(expression.span(), ty.clone());
        Ok(ty)
    }

    fn expression_type_for_expected(
        &mut self,
        expression: &Expression,
        expected: &TypeName,
    ) -> Result<ExpressionType, Diagnostic> {
        if let Expression::MethodCall {
            receiver,
            method,
            arguments,
            span,
            ..
        } = expression
            && is_database_receiver(receiver)
            && method.canonical == "query"
        {
            self.database_method_type(method, arguments, *span, Some(expected))
        } else if let Expression::Soql(query) = expression {
            let ty = self.soql_type(query, Some(expected))?;
            self.expression_types.insert(expression.span(), ty.clone());
            Ok(ty)
        } else if let Expression::NullCoalesce {
            left,
            right,
            operator_span,
            ..
        } = expression
        {
            let left_type = self.expression_type_for_expected(left, expected)?;
            let right_type = self.expression_type_for_expected(right, expected)?;
            if matches!(left.as_ref(), Expression::Soql(_)) {
                self.null_aware_queries.insert(left.span());
            }
            let ty = self.join_expression_types(
                &left_type,
                &right_type,
                *operator_span,
                "null-coalescing operands",
            )?;
            self.expression_types.insert(expression.span(), ty.clone());
            Ok(ty)
        } else {
            self.expression_type(expression)
        }
    }

    fn expression_type_inner(
        &mut self,
        expression: &Expression,
    ) -> Result<ExpressionType, Diagnostic> {
        if let Some(ty) = literal_expression_type(expression) {
            return ty;
        }
        match expression {
            Expression::StringLiteral(..)
            | Expression::BooleanLiteral(..)
            | Expression::IntegerLiteral(..)
            | Expression::LongLiteral(..)
            | Expression::DecimalLiteral(..)
            | Expression::NullLiteral(..) => unreachable!("literal type handled above"),
            Expression::Soql(query) => self.soql_type(query, None),
            Expression::Sosl(query) => self.sosl_type(query),
            Expression::Variable(identifier) => self.variable_type(identifier),
            Expression::TypeLiteral { ty, span } => {
                self.validate_type(ty, *span)?;
                self.type_literals
                    .insert(*span, self.canonical_type_name(ty));
                Ok(ExpressionType::Value(TypeName::Type))
            }
            Expression::Assignment { .. } => self.assignment_expression_type(expression),
            Expression::NewCollection { .. } => self.new_collection_expression_type(expression),
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
            Expression::MethodCall { .. } | Expression::MemberAccess { .. } => {
                self.navigation_expression_type(expression)
            }
            Expression::Cast { ty, expression, .. } => self.cast_type(ty, expression),
            Expression::Conditional {
                condition,
                when_true,
                when_false,
                question_span,
                ..
            } => self.conditional_type(condition, when_true, when_false, *question_span),
            Expression::NullCoalesce { .. } => self.null_coalescing_expression_type(expression),
            Expression::Instanceof {
                value,
                target,
                target_span,
                operator_span,
                ..
            } => self.instanceof_type(value, target, *target_span, *operator_span),
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

    fn assignment_expression_type(
        &mut self,
        expression: &Expression,
    ) -> Result<ExpressionType, Diagnostic> {
        let Expression::Assignment {
            target,
            operator,
            operator_span,
            value,
            ..
        } = expression
        else {
            unreachable!("assignment helper requires an assignment expression");
        };
        let expected =
            self.assignment_target_type(target, *operator != AssignmentOperator::Assign)?;
        if *operator == AssignmentOperator::Assign {
            let actual = self.expression_type_for_expected(value, &expected)?;
            self.require_assignable(&expected, &actual, value.span())?;
        } else {
            let right = self.expression_type(value)?;
            let result = self.checked_binary_type(
                ExpressionType::Value(expected.clone()),
                compound_binary_operator(*operator),
                right,
                *operator_span,
            )?;
            self.require_assignable(&expected, &result, *operator_span)?;
        }
        Ok(ExpressionType::Value(expected))
    }

    fn navigation_expression_type(
        &mut self,
        expression: &Expression,
    ) -> Result<ExpressionType, Diagnostic> {
        match expression {
            Expression::MethodCall {
                receiver,
                method,
                arguments,
                safe_navigation,
                navigation_span,
                span,
            } => {
                if *safe_navigation {
                    self.prepare_null_aware_receiver(receiver)?;
                }
                let ty = self.method_call_type(receiver, method, arguments, *span)?;
                if *safe_navigation {
                    self.validate_safe_method_target(*span, *navigation_span)?;
                }
                Ok(ty)
            }
            Expression::MemberAccess {
                receiver,
                member,
                safe_navigation,
                navigation_span,
                span,
            } => {
                if *safe_navigation {
                    self.prepare_null_aware_receiver(receiver)?;
                }
                let ty = self.member_access_type(receiver, member, *span, false)?;
                if *safe_navigation {
                    self.validate_safe_member_target(*span, *navigation_span)?;
                }
                Ok(ty)
            }
            _ => unreachable!("navigation helper requires a member or method expression"),
        }
    }

    fn null_coalescing_expression_type(
        &mut self,
        expression: &Expression,
    ) -> Result<ExpressionType, Diagnostic> {
        let Expression::NullCoalesce {
            left,
            right,
            operator_span,
            ..
        } = expression
        else {
            unreachable!("null-coalescing helper requires a null-coalescing expression");
        };
        self.null_coalescing_type(left, right, *operator_span)
    }

    fn new_collection_expression_type(
        &mut self,
        expression: &Expression,
    ) -> Result<ExpressionType, Diagnostic> {
        let Expression::NewCollection {
            ty,
            initializer,
            span,
        } = expression
        else {
            unreachable!("collection expression helper requires a NewCollection node");
        };
        self.new_collection_type(ty, initializer, *span)
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
        let platform_constructor = match ty {
            TypeName::Http => Some(PlatformConstructor::Http),
            TypeName::HttpRequest => Some(PlatformConstructor::HttpRequest),
            TypeName::HttpResponse => Some(PlatformConstructor::HttpResponse),
            _ => None,
        };
        if let Some(constructor) = platform_constructor {
            for argument in arguments {
                self.expression_type(argument)?;
            }
            if !arguments.is_empty() {
                return Err(Diagnostic::new(
                    format!("{} constructor expects no arguments", ty.apex_name()),
                    arguments[0].span(),
                ));
            }
            self.calls
                .insert(span, CallTarget::PlatformConstructor(constructor));
            return Ok(ExpressionType::Value(ty.clone()));
        }
        let TypeName::Custom(name) = ty else {
            return Err(Diagnostic::new(
                "object construction requires a class type",
                span,
            ));
        };
        if let Some(object_id) = self.schema.object_index(&name.spelling) {
            for argument in arguments {
                self.expression_type(argument)?;
            }
            if !arguments.is_empty() {
                return Err(Diagnostic::new(
                    format!(
                        "SObject constructor for `{}` expects no arguments",
                        name.spelling
                    ),
                    arguments[0].span(),
                ));
            }
            self.calls.insert(
                span,
                CallTarget::SObjectConstructor {
                    object_id: Some(object_id),
                },
            );
            return Ok(ExpressionType::Value(ty.clone()));
        }
        if name.canonical == "sobject" {
            if arguments.len() != 1 {
                return Err(Diagnostic::new(
                    "dynamic SObject constructor expects one object API name",
                    span,
                ));
            }
            self.require_operand(&arguments[0], &TypeName::String, arguments[0].span())?;
            self.calls
                .insert(span, CallTarget::SObjectConstructor { object_id: None });
            return Ok(ExpressionType::Value(ty.clone()));
        }
        let class_id = self.class_ids[&name.canonical];
        let class = &self.classes[class_id];
        if self.class_is_custom_exception(class_id) {
            return self.new_custom_exception_type(class_id, ty, arguments, span);
        }
        self.validate_constructable_class(class_id, name, span)?;
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

    fn new_custom_exception_type(
        &mut self,
        class_id: usize,
        ty: &TypeName,
        arguments: &[Expression],
        span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        if self.classes[class_id]
            .members
            .iter()
            .any(|member| matches!(member, ClassMember::Constructor(_)))
        {
            return Err(Diagnostic::new(
                "custom exception constructors are not supported; use inherited zero- or one-String-argument construction",
                span,
            ));
        }
        match arguments {
            [] => {}
            [message] => self.require_operand(message, &TypeName::String, message.span())?,
            _ => {
                for argument in arguments {
                    self.expression_type(argument)?;
                }
                return Err(Diagnostic::new(
                    "custom exception constructor expects zero arguments or one String",
                    span,
                ));
            }
        }
        self.calls.insert(
            span,
            CallTarget::CustomExceptionConstructor {
                class_id: ClassId::from_index(class_id),
            },
        );
        Ok(ExpressionType::Value(ty.clone()))
    }

    fn validate_constructable_class(
        &self,
        class_id: usize,
        name: &crate::ast::NamedType,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let class = &self.classes[class_id];
        if class.kind == ClassKind::Enum {
            return Err(Diagnostic::new(
                format!("cannot construct enum `{}`", name.spelling),
                span,
            ));
        }
        if class.kind == ClassKind::Interface || class.modifiers.contains(&Modifier::Abstract) {
            return Err(Diagnostic::new(
                format!("cannot construct abstract type `{}`", name.spelling),
                span,
            ));
        }
        Ok(())
    }

    fn select_constructor<'a>(
        &self,
        applicable: &[&'a (ClassMemberId, ConstructorDeclaration)],
    ) -> Option<&'a (ClassMemberId, ConstructorDeclaration)> {
        let selected = overload::unique_most_specific(
            applicable,
            |left, right| left.0 == right.0,
            |left, right| {
                self.parameter_types_more_specific(
                    &left
                        .1
                        .parameters
                        .iter()
                        .map(|parameter| parameter.ty.clone())
                        .collect::<Vec<_>>(),
                    &right
                        .1
                        .parameters
                        .iter()
                        .map(|parameter| parameter.ty.clone())
                        .collect::<Vec<_>>(),
                )
            },
        )?;
        Some(applicable[selected])
    }

    fn member_access_type(
        &mut self,
        receiver: &Expression,
        name: &Identifier,
        span: Span,
        for_write: bool,
    ) -> Result<ExpressionType, Diagnostic> {
        if let Some(result) = self.qualified_type_reference(receiver, name, span, for_write) {
            return result;
        }
        if let Expression::Variable(identifier) = receiver
            && identifier.canonical == "trigger"
        {
            let Some(object_id) = self.current_trigger_object else {
                return Err(Diagnostic::new(
                    "Trigger context is only available inside a trigger",
                    identifier.span,
                ));
            };
            if for_write {
                return Err(Diagnostic::new(
                    format!("Trigger.{} is read-only", name.spelling),
                    name.span,
                ));
            }
            let object = self
                .schema
                .object_at(object_id)
                .expect("current trigger object is valid");
            let object_type = TypeName::Custom(crate::ast::NamedType::new(
                object.api_name().to_owned(),
                name.span,
            ));
            let (variable, ty) = match name.canonical.as_str() {
                "new" => (
                    TriggerContextVariable::New,
                    TypeName::List(Box::new(object_type.clone())),
                ),
                "old" => (
                    TriggerContextVariable::Old,
                    TypeName::List(Box::new(object_type.clone())),
                ),
                "newmap" => (
                    TriggerContextVariable::NewMap,
                    TypeName::Map(Box::new(TypeName::String), Box::new(object_type.clone())),
                ),
                "oldmap" => (
                    TriggerContextVariable::OldMap,
                    TypeName::Map(Box::new(TypeName::String), Box::new(object_type)),
                ),
                "isexecuting" => (TriggerContextVariable::IsExecuting, TypeName::Boolean),
                "isbefore" => (TriggerContextVariable::IsBefore, TypeName::Boolean),
                "isafter" => (TriggerContextVariable::IsAfter, TypeName::Boolean),
                "isinsert" => (TriggerContextVariable::IsInsert, TypeName::Boolean),
                "isupdate" => (TriggerContextVariable::IsUpdate, TypeName::Boolean),
                "isdelete" => (TriggerContextVariable::IsDelete, TypeName::Boolean),
                "isundelete" => (TriggerContextVariable::IsUndelete, TypeName::Boolean),
                "size" => (TriggerContextVariable::Size, TypeName::Integer),
                _ => {
                    return Err(Diagnostic::new(
                        format!("unknown Trigger context variable `{}`", name.spelling),
                        name.span,
                    ));
                }
            };
            self.members
                .insert(span, MemberTarget::TriggerContext(variable));
            return Ok(ExpressionType::Value(ty));
        }
        if let Some(result) = self.sobject_member_access_type(receiver, name, span, for_write) {
            return result;
        }
        let (class_id, static_access) = self.member_receiver_class(receiver, name)?;
        if let Some(result) =
            self.enum_constant_type(class_id, static_access, name, span, for_write)
        {
            return result;
        }
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

    fn sobject_member_access_type(
        &mut self,
        receiver: &Expression,
        name: &Identifier,
        span: Span,
        for_write: bool,
    ) -> Option<Result<ExpressionType, Diagnostic>> {
        let receiver_type = match receiver {
            Expression::Variable(identifier)
                if self.lookup(&identifier.canonical).is_none()
                    && self.current_class.is_none_or(|class_id| {
                        self.class_value_member(class_id, &identifier.canonical)
                            .is_none()
                    }) =>
            {
                return None;
            }
            _ => match self.expression_type(receiver) {
                Ok(ExpressionType::Value(ty)) => ty,
                Ok(ExpressionType::Null | ExpressionType::Void) => return None,
                Err(error) => return Some(Err(error)),
            },
        };
        if !self.is_sobject_type(&receiver_type) && !self.is_dynamic_sobject_type(&receiver_type) {
            return None;
        }
        Some(self.typed_sobject_member_access(&receiver_type, name, span, for_write))
    }

    fn typed_sobject_member_access(
        &mut self,
        receiver_type: &TypeName,
        name: &Identifier,
        span: Span,
        for_write: bool,
    ) -> Result<ExpressionType, Diagnostic> {
        let Some(object_id) = self.sobject_object_id(receiver_type) else {
            return Err(Diagnostic::new(
                "dynamic SObject fields require get/put access",
                name.span,
            ));
        };
        let object = self
            .schema
            .object_at(object_id)
            .expect("schema object index is valid");
        if let Some(field_id) = object.field_index(&name.spelling) {
            let field = object
                .field_at(field_id)
                .expect("schema field index is valid");
            let field_type = apex_field_type(field.data_type());
            self.members.insert(
                span,
                MemberTarget::SObjectField {
                    object_id,
                    field_id,
                },
            );
            return Ok(ExpressionType::Value(field_type));
        }
        if let Some((reference_field_id, target_object_id)) =
            self.sobject_relationship_target(object_id, &name.spelling)
        {
            return self.parent_relationship_member_type(
                object_id,
                reference_field_id,
                target_object_id,
                name,
                span,
                for_write,
            );
        }
        if let Some((child_object_id, _)) =
            self.schema.child_relationship(object_id, &name.spelling)
        {
            return self.child_relationship_member_type(
                object_id,
                child_object_id,
                name,
                span,
                for_write,
            );
        }
        Err(Diagnostic::new(
            format!(
                "unknown field `{}` on SObject `{}`",
                name.spelling,
                object.api_name()
            ),
            name.span,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn parent_relationship_member_type(
        &mut self,
        object_id: usize,
        reference_field_id: usize,
        target_object_id: usize,
        name: &Identifier,
        span: Span,
        for_write: bool,
    ) -> Result<ExpressionType, Diagnostic> {
        if for_write {
            return Err(Diagnostic::new(
                "parent relationship fields are read-only",
                name.span,
            ));
        }
        self.members.insert(
            span,
            MemberTarget::SObjectRelationship {
                object_id,
                reference_field_id,
                target_object_id,
            },
        );
        let target = self
            .schema
            .object_at(target_object_id)
            .expect("relationship target object index is valid");
        Ok(ExpressionType::Value(TypeName::Custom(
            crate::ast::NamedType::new(target.api_name().to_owned(), name.span),
        )))
    }

    fn child_relationship_member_type(
        &mut self,
        object_id: usize,
        child_object_id: usize,
        name: &Identifier,
        span: Span,
        for_write: bool,
    ) -> Result<ExpressionType, Diagnostic> {
        if for_write {
            return Err(Diagnostic::new(
                "child relationship collections are read-only",
                name.span,
            ));
        }
        self.members.insert(
            span,
            MemberTarget::SObjectChildRelationship {
                object_id,
                child_object_id,
                relationship: name.canonical.clone(),
            },
        );
        let child = self
            .schema
            .object_at(child_object_id)
            .expect("child relationship object index is valid");
        Ok(ExpressionType::Value(TypeName::List(Box::new(
            TypeName::Custom(crate::ast::NamedType::new(
                child.api_name().to_owned(),
                name.span,
            )),
        ))))
    }

    fn qualified_type_reference(
        &mut self,
        receiver: &Expression,
        name: &Identifier,
        span: Span,
        for_write: bool,
    ) -> Option<Result<ExpressionType, Diagnostic>> {
        let owner = qualified_expression_name(receiver)?;
        let qualified = format!("{owner}.{}", name.canonical);
        let class_id = self.class_ids.get(&qualified).copied()?;
        if for_write {
            return Some(Err(Diagnostic::new(
                "type references are not assignable",
                name.span,
            )));
        }
        self.members.insert(
            span,
            MemberTarget::TypeReference {
                class_id: ClassId::from_index(class_id),
            },
        );
        Some(Ok(ExpressionType::Value(TypeName::Type)))
    }

    fn member_receiver_class(
        &mut self,
        receiver: &Expression,
        name: &Identifier,
    ) -> Result<(usize, bool), Diagnostic> {
        if let Some(MemberTarget::TypeReference { class_id }) = self.members.get(&receiver.span()) {
            return Ok((class_id.index(), true));
        }
        if let Expression::Variable(identifier) = receiver
            && let Some(class_id) = self.class_ids.get(&identifier.canonical).copied()
            && (self
                .class_value_member(class_id, &name.canonical)
                .is_some_and(|member| member.modifiers.contains(&Modifier::Static))
                || (self.lookup(&identifier.canonical).is_none()
                    && self
                        .current_class
                        .and_then(|id| self.class_value_member(id, &identifier.canonical))
                        .is_none()))
        {
            return Ok((class_id, true));
        }
        let receiver_type = self.expression_type(receiver)?;
        Ok((
            self.class_id_from_expression(&receiver_type, receiver.span())?,
            false,
        ))
    }

    fn enum_constant_type(
        &mut self,
        class_id: usize,
        static_access: bool,
        name: &Identifier,
        span: Span,
        for_write: bool,
    ) -> Option<Result<ExpressionType, Diagnostic>> {
        if self.classes[class_id].kind != ClassKind::Enum {
            return None;
        }
        let ordinal = self.classes[class_id]
            .enum_constants
            .iter()
            .position(|constant| constant.canonical == name.canonical)?;
        if !static_access {
            return Some(Err(Diagnostic::new(
                "enum constants must be accessed through their enum type",
                name.span,
            )));
        }
        if for_write {
            return Some(Err(Diagnostic::new(
                "enum constants are read-only",
                name.span,
            )));
        }
        self.members.insert(
            span,
            MemberTarget::EnumConstant {
                class_id: ClassId::from_index(class_id),
                ordinal,
            },
        );
        Some(Ok(ExpressionType::Value(self.class_type(class_id))))
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

    fn prepare_null_aware_receiver(&mut self, receiver: &Expression) -> Result<(), Diagnostic> {
        let Expression::Soql(query) = receiver else {
            return Ok(());
        };
        let expected = TypeName::Custom(crate::ast::NamedType::new(
            query.from.spelling.clone(),
            query.from.span,
        ));
        let ty = self.soql_type(query, Some(&expected))?;
        self.expression_types.insert(receiver.span(), ty.clone());
        self.null_aware_queries.insert(receiver.span());
        Ok(())
    }

    fn class_type(&self, class_id: usize) -> TypeName {
        let class = &self.classes[class_id];
        TypeName::Custom(crate::ast::NamedType::new(
            class.qualified_name.spelling.clone(),
            class.name.span,
        ))
    }

    fn canonical_type_name(&self, ty: &TypeName) -> TypeName {
        match ty {
            TypeName::Custom(name) => self
                .class_ids
                .get(&name.canonical)
                .map_or_else(|| ty.clone(), |class_id| self.class_type(*class_id)),
            TypeName::List(element) => TypeName::List(Box::new(self.canonical_type_name(element))),
            TypeName::Set(element) => TypeName::Set(Box::new(self.canonical_type_name(element))),
            TypeName::Iterable(element) => {
                TypeName::Iterable(Box::new(self.canonical_type_name(element)))
            }
            TypeName::Map(key, value) => TypeName::Map(
                Box::new(self.canonical_type_name(key)),
                Box::new(self.canonical_type_name(value)),
            ),
            _ => ty.clone(),
        }
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

        let Some(best) = overload::unique_most_specific(
            &applicable,
            |left, right| left.id == right.id,
            |left, right| {
                self.parameter_types_more_specific(&left.parameter_types, &right.parameter_types)
            },
        )
        .map(|index| applicable[index]) else {
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
                    || (matches!(source, TypeName::Integer | TypeName::Long)
                        && matches!(target, TypeName::Integer | TypeName::Long))
                    || *source == TypeName::Object
                    || *target == TypeName::Object
                    || (*source == TypeName::Exception && self.is_exception_type(target))
                    || (*target == TypeName::Exception && self.is_exception_type(source))
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
        span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        self.validate_type(ty, span)?;
        match initializer {
            CollectionInitializer::Arguments(arguments) => {
                self.check_collection_constructor(ty, arguments)?;
            }
            CollectionInitializer::Elements(elements) => {
                self.validate_collection_elements(ty, elements)?;
            }
            CollectionInitializer::MapEntries(entries) => {
                self.validate_map_initializer(ty, entries)?;
            }
            CollectionInitializer::SizedArray(size) => self.validate_array_size(ty, size)?,
        }
        Ok(ExpressionType::Value(ty.clone()))
    }

    fn validate_collection_elements(
        &mut self,
        ty: &TypeName,
        elements: &[Expression],
    ) -> Result<(), Diagnostic> {
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
        Ok(())
    }

    fn validate_map_initializer(
        &mut self,
        ty: &TypeName,
        entries: &[crate::ast::MapEntry],
    ) -> Result<(), Diagnostic> {
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
        Ok(())
    }

    fn validate_array_size(&mut self, ty: &TypeName, size: &Expression) -> Result<(), Diagnostic> {
        if !matches!(ty, TypeName::List(_)) {
            return Err(Diagnostic::new(
                format!("{} cannot be allocated with an array size", ty.apex_name()),
                size.span(),
            ));
        }
        let size_type = self.expression_type(size)?;
        if matches!(
            size_type,
            ExpressionType::Value(TypeName::Integer | TypeName::Long)
        ) {
            Ok(())
        } else {
            Err(Diagnostic::new(
                format!("array size requires Integer, found {}", size_type.name()),
                size.span(),
            ))
        }
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
        require_read: bool,
    ) -> Result<TypeName, Diagnostic> {
        match target {
            AssignmentTarget::Variable(identifier) => {
                self.variable_assignment_target_type(identifier, require_read)
            }
            AssignmentTarget::Index {
                collection,
                index,
                span,
            } => {
                let ty = self.index_type(collection, index)?;
                self.places.insert(*span, PlaceTarget::ListIndex);
                Ok(ty)
            }
            AssignmentTarget::Member {
                receiver,
                member,
                span,
            } => self.member_assignment_target_type(receiver, member, *span, require_read),
        }
    }

    fn variable_assignment_target_type(
        &mut self,
        identifier: &Identifier,
        require_read: bool,
    ) -> Result<TypeName, Diagnostic> {
        if let Some(ty) = self.lookup(&identifier.canonical).cloned() {
            self.references
                .insert(identifier.span, ReferenceTarget::Local);
            self.places.insert(identifier.span, PlaceTarget::Local);
            return Ok(ty);
        }
        let class_id = self
            .current_class
            .ok_or_else(|| unknown_variable(identifier))?;
        let member = self
            .class_value_member(class_id, &identifier.canonical)
            .ok_or_else(|| unknown_variable(identifier))?;
        self.ensure_member_access(member.target, &member.write_access, identifier.span)?;
        self.ensure_member_is_mutable(identifier, &member, require_read)?;
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
        let reference = if is_static {
            ReferenceTarget::StaticMember(member.target)
        } else {
            ReferenceTarget::InstanceMember(member.target)
        };
        let place = if is_static {
            PlaceTarget::StaticMember(member.target)
        } else {
            PlaceTarget::InstanceMember(member.target)
        };
        self.references.insert(identifier.span, reference);
        self.places.insert(identifier.span, place);
        Ok(member.ty)
    }

    fn ensure_member_is_mutable(
        &self,
        identifier: &Identifier,
        member: &ClassValueMember,
        require_read: bool,
    ) -> Result<(), Diagnostic> {
        if !member.writable {
            return Err(Diagnostic::new(
                format!("member `{}` is read-only", identifier.spelling),
                identifier.span,
            ));
        }
        if require_read && !member.readable {
            return Err(Diagnostic::new(
                format!("member `{}` is write-only", identifier.spelling),
                identifier.span,
            ));
        }
        if require_read {
            self.ensure_member_access(member.target, &member.read_access, identifier.span)?;
        }
        Ok(())
    }

    fn member_assignment_target_type(
        &mut self,
        receiver: &Expression,
        member: &Identifier,
        span: Span,
        require_read: bool,
    ) -> Result<TypeName, Diagnostic> {
        if require_read {
            self.member_access_type(receiver, member, span, false)?;
        }
        let ty = match self.member_access_type(receiver, member, span, true)? {
            ExpressionType::Value(ty) => ty,
            _ => unreachable!("member access always has a value type"),
        };
        let place = match self.members.get(&span).cloned() {
            Some(MemberTarget::Static(target)) => PlaceTarget::StaticMember(target),
            Some(MemberTarget::Instance(target)) => PlaceTarget::InstanceMember(target),
            Some(MemberTarget::SObjectField {
                object_id,
                field_id,
            }) => PlaceTarget::SObjectField {
                object_id: ObjectTypeId::from_index(object_id),
                field_id: FieldId::from_index(field_id),
            },
            Some(
                MemberTarget::SObjectRelationship { .. }
                | MemberTarget::SObjectChildRelationship { .. }
                | MemberTarget::TriggerContext(_)
                | MemberTarget::EnumConstant { .. }
                | MemberTarget::TypeReference { .. },
            )
            | None => return Err(Diagnostic::new("member is not assignable", span)),
        };
        self.places.insert(span, place);
        Ok(ty)
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
        if let Some(result) = self.dynamic_database_method_call(receiver, method, arguments, span) {
            return result;
        }
        if let Some(result) = self.qualified_type_method_call(receiver, method, arguments, span) {
            return result;
        }
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
            } else if let Some(result) =
                self.unbound_class_method_call(identifier, method, arguments, span)
            {
                result
            } else {
                match identifier.canonical.as_str() {
                    "database" if method.canonical == "executebatch" => self
                        .static_platform_method_type("Database", method, arguments)
                        .map(|(intrinsic, result)| {
                            self.calls.insert(span, CallTarget::Intrinsic(intrinsic));
                            result
                        }),
                    "database" => self.database_method_type(method, arguments, span, None),
                    "string" | "math" | "system" => {
                        let checked = match identifier.canonical.as_str() {
                            "string" => self.static_string_method_type(method, arguments),
                            "math" => self.static_math_method_type(method, arguments),
                            "system" => self.static_system_method_type(method, arguments),
                            _ => unreachable!(),
                        };
                        checked.map(|(intrinsic, result)| {
                            self.calls.insert(span, CallTarget::Intrinsic(intrinsic));
                            result
                        })
                    }
                    owner if is_platform_static_owner(owner) => self
                        .static_platform_method_type(&identifier.spelling, method, arguments)
                        .map(|(intrinsic, result)| {
                            self.calls.insert(span, CallTarget::Intrinsic(intrinsic));
                            result
                        }),
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

    fn dynamic_database_method_call(
        &mut self,
        receiver: &Expression,
        method: &Identifier,
        arguments: &[Expression],
        span: Span,
    ) -> Option<Result<ExpressionType, Diagnostic>> {
        if !is_database_receiver(receiver)
            || !matches!(
                method.canonical.as_str(),
                "query" | "countquery" | "getquerylocator"
            )
        {
            return None;
        }
        Some(self.database_method_type(method, arguments, span, None))
    }

    fn qualified_type_method_call(
        &mut self,
        receiver: &Expression,
        method: &Identifier,
        arguments: &[Expression],
        span: Span,
    ) -> Option<Result<ExpressionType, Diagnostic>> {
        if matches!(receiver, Expression::Variable(_)) {
            return None;
        }
        if let Err(error) = self.expression_type(receiver) {
            return Some(Err(error));
        }
        let MemberTarget::TypeReference { class_id } =
            self.members.get(&receiver.span()).cloned()?
        else {
            return None;
        };
        let class_id = class_id.index();
        if self.classes[class_id].kind == ClassKind::Enum {
            return Some(self.enum_method_type(class_id, method, arguments, true, span));
        }
        let argument_types = arguments
            .iter()
            .map(|argument| self.expression_type(argument))
            .collect::<Result<Vec<_>, _>>();
        Some(argument_types.and_then(|argument_types| {
            let candidates = self.class_methods_named(class_id, &method.canonical);
            self.select_class_method_call(
                class_id,
                method,
                &argument_types,
                candidates,
                ClassCallKind::Static,
                span,
            )
        }))
    }

    fn unbound_class_method_call(
        &mut self,
        identifier: &Identifier,
        method: &Identifier,
        arguments: &[Expression],
        span: Span,
    ) -> Option<Result<ExpressionType, Diagnostic>> {
        let class_id = self.class_ids.get(&identifier.canonical).copied()?;
        if self
            .current_class
            .and_then(|id| self.class_value_member(id, &identifier.canonical))
            .is_some()
        {
            return None;
        }
        if self.classes[class_id].kind == ClassKind::Enum {
            return Some(self.enum_method_type(class_id, method, arguments, true, span));
        }
        let argument_types = arguments
            .iter()
            .map(|argument| self.expression_type(argument))
            .collect::<Result<Vec<_>, _>>();
        Some(argument_types.and_then(|argument_types| {
            let candidates = self.class_methods_named(class_id, &method.canonical);
            self.select_class_method_call(
                class_id,
                method,
                &argument_types,
                candidates,
                ClassCallKind::Static,
                span,
            )
        }))
    }

    fn instance_method_type(
        &mut self,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
        span: Span,
        super_call: bool,
    ) -> Result<ExpressionType, Diagnostic> {
        if self.is_sobject_type(receiver_type) || self.is_dynamic_sobject_type(receiver_type) {
            return match method.canonical.as_str() {
                "get" => {
                    require_arity(
                        receiver_type,
                        &method.spelling,
                        arguments.len(),
                        &[1],
                        arguments,
                    )?;
                    self.require_operand(&arguments[0], &TypeName::String, arguments[0].span())?;
                    self.calls.insert(span, CallTarget::SObjectGet);
                    Ok(ExpressionType::Value(TypeName::Object))
                }
                "put" => {
                    require_arity(
                        receiver_type,
                        &method.spelling,
                        arguments.len(),
                        &[2],
                        arguments,
                    )?;
                    self.require_operand(&arguments[0], &TypeName::String, arguments[0].span())?;
                    let value_type = self.expression_type(&arguments[1])?;
                    if value_type == ExpressionType::Void {
                        return Err(Diagnostic::new(
                            "SObject.put value cannot be void",
                            arguments[1].span(),
                        ));
                    }
                    self.calls.insert(span, CallTarget::SObjectPut);
                    Ok(ExpressionType::Void)
                }
                _ => Err(unknown_method(receiver_type, method)),
            };
        }
        if receiver_type == &TypeName::AggregateResult {
            if method.canonical != "get" || arguments.len() != 1 {
                return Err(unknown_method(receiver_type, method));
            }
            self.require_operand(&arguments[0], &TypeName::String, arguments[0].span())?;
            self.calls.insert(span, CallTarget::AggregateResultGet);
            return Ok(ExpressionType::Value(TypeName::Object));
        }
        let (intrinsic, result) = match receiver_type {
            TypeName::List(element) => {
                self.list_method_type(receiver_type, element, method, arguments)?
            }
            TypeName::Set(element) => {
                self.set_method_type(receiver_type, element, method, arguments)?
            }
            TypeName::Map(key, value) => {
                self.map_method_type(receiver_type, key, value, method, arguments)?
            }
            TypeName::String => self.string_instance_method_type(method, arguments)?,
            TypeName::Date
            | TypeName::Datetime
            | TypeName::Time
            | TypeName::Decimal
            | TypeName::Id
            | TypeName::Blob
            | TypeName::Object
            | TypeName::Pattern
            | TypeName::Matcher
            | TypeName::Http
            | TypeName::HttpRequest
            | TypeName::HttpResponse
            | TypeName::QueueableContext
            | TypeName::BatchableContext
            | TypeName::QueryLocator
            | TypeName::SchedulableContext
            | TypeName::SObjectType
            | TypeName::DescribeSObjectResult => {
                self.platform_instance_method_type(receiver_type, method, arguments)?
            }
            ty if self.is_exception_type(ty) => {
                self.exception_instance_method_type(receiver_type, method, arguments)?
            }
            TypeName::Custom(name) => {
                return self.custom_instance_method_type(name, method, arguments, span, super_call);
            }
            _ => return Err(unknown_method(receiver_type, method)),
        };
        self.calls.insert(span, CallTarget::Intrinsic(intrinsic));
        Ok(result)
    }

    fn custom_instance_method_type(
        &mut self,
        name: &crate::ast::NamedType,
        method: &Identifier,
        arguments: &[Expression],
        span: Span,
        super_call: bool,
    ) -> Result<ExpressionType, Diagnostic> {
        let class_id = self.class_ids[&name.canonical];
        if self.classes[class_id].kind == ClassKind::Enum {
            return self.enum_method_type(class_id, method, arguments, false, span);
        }
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

    fn enum_method_type(
        &mut self,
        class_id: usize,
        method: &Identifier,
        arguments: &[Expression],
        static_call: bool,
        span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let enum_type = self.class_type(class_id);
        let (target, result) = match (static_call, method.canonical.as_str()) {
            (false, "name") if arguments.is_empty() => (hir::EnumMethod::Name, TypeName::String),
            (false, "ordinal") if arguments.is_empty() => {
                (hir::EnumMethod::Ordinal, TypeName::Integer)
            }
            (true, "values") if arguments.is_empty() => (
                hir::EnumMethod::Values,
                TypeName::List(Box::new(enum_type.clone())),
            ),
            (true, "valueof") if arguments.len() == 1 => {
                self.require_operand(&arguments[0], &TypeName::String, arguments[0].span())?;
                (hir::EnumMethod::ValueOf, enum_type)
            }
            _ => {
                for argument in arguments {
                    self.expression_type(argument)?;
                }
                return Err(Diagnostic::new(
                    format!(
                        "unknown {}enum method `{}`",
                        if static_call { "static " } else { "" },
                        method.spelling
                    ),
                    method.span,
                ));
            }
        };
        self.calls.insert(
            span,
            CallTarget::EnumMethod {
                class_id: ClassId::from_index(class_id),
                method: target,
            },
        );
        Ok(ExpressionType::Value(result))
    }

    fn is_sobject_type(&self, ty: &TypeName) -> bool {
        matches!(ty, TypeName::Custom(name) if self.schema.object(&name.spelling).is_ok())
    }

    fn is_dynamic_sobject_type(&self, ty: &TypeName) -> bool {
        matches!(ty, TypeName::Custom(name) if name.canonical == "sobject")
    }

    fn sobject_object_id(&self, ty: &TypeName) -> Option<usize> {
        let TypeName::Custom(name) = ty else {
            return None;
        };
        self.schema.object_index(&name.spelling)
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
        let Some(best) = overload::unique_most_specific(
            &applicable,
            |left, right| left.target == right.target,
            |left, right| {
                self.parameter_types_more_specific(&left.parameter_types, &right.parameter_types)
            },
        )
        .map(|index| &applicable[index]) else {
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

    fn unary_type(
        &mut self,
        operator: UnaryOperator,
        operand: &Expression,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        match operator {
            UnaryOperator::Positive | UnaryOperator::Negate => {
                if operator == UnaryOperator::Negate
                    && matches!(
                        operand,
                        Expression::IntegerLiteral(value, _)
                            if *value == i64::from(i32::MAX) + 1
                    )
                {
                    self.unary_operations.insert(
                        operator_span,
                        CheckedUnaryOperation::Negate(NumericKind::Integer),
                    );
                    return Ok(ExpressionType::Value(TypeName::Integer));
                }
                if operator == UnaryOperator::Negate
                    && matches!(
                        operand,
                        Expression::LongLiteral(value, _)
                            if *value == i128::from(i64::MAX) + 1
                    )
                {
                    self.unary_operations.insert(
                        operator_span,
                        CheckedUnaryOperation::Negate(NumericKind::Long),
                    );
                    return Ok(ExpressionType::Value(TypeName::Long));
                }
                let ty = self.expression_type(operand)?;
                if let Some(kind) = numeric_kind(&ty) {
                    self.unary_operations.insert(
                        operator_span,
                        if operator == UnaryOperator::Positive {
                            CheckedUnaryOperation::Positive(kind)
                        } else {
                            CheckedUnaryOperation::Negate(kind)
                        },
                    );
                    Ok(ty)
                } else {
                    Err(Diagnostic::new(
                        format!("expected numeric operand, found {}", ty.name()),
                        operator_span,
                    ))
                }
            }
            UnaryOperator::Not => {
                self.require_operand(operand, &TypeName::Boolean, operator_span)?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            UnaryOperator::BitwiseNot => {
                let ty = self.expression_type(operand)?;
                let Some(kind) = integral_kind(&ty) else {
                    return Err(Diagnostic::new(
                        format!("expected Integer or Long operand, found {}", ty.name()),
                        operator_span,
                    ));
                };
                self.unary_operations
                    .insert(operator_span, CheckedUnaryOperation::BitwiseNot(kind));
                Ok(ty)
            }
            UnaryOperator::PrefixIncrement | UnaryOperator::PrefixDecrement => {
                self.require_mutable_integral(operand, operator_span)
            }
        }
    }

    fn postfix_type(
        &mut self,
        operand: &Expression,
        _operator: PostfixOperator,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        self.require_mutable_integral(operand, operator_span)
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
        self.checked_binary_type(left_type, operator, right_type, operator_span)
    }

    fn checked_binary_type(
        &mut self,
        left_type: ExpressionType,
        operator: BinaryOperator,
        right_type: ExpressionType,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        match operator {
            BinaryOperator::Add => self.checked_add_type(left_type, right_type, operator_span),
            BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => self.checked_numeric_or_temporal_type(
                left_type,
                operator,
                right_type,
                operator_span,
            ),
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                checked_equality_type(left_type, operator, right_type, operator_span)
            }
            BinaryOperator::BitwiseAnd | BinaryOperator::BitwiseOr | BinaryOperator::BitwiseXor => {
                self.checked_bitwise_type(left_type, operator, right_type, operator_span)
            }
            BinaryOperator::ShiftLeft
            | BinaryOperator::ShiftRight
            | BinaryOperator::UnsignedShiftRight => {
                self.checked_shift_type(left_type, operator, right_type, operator_span)
            }
            BinaryOperator::And | BinaryOperator::Or => {
                checked_boolean_type(left_type, operator, right_type, operator_span)
            }
        }
    }

    fn checked_add_type(
        &mut self,
        left_type: ExpressionType,
        right_type: ExpressionType,
        span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        if let Some(kind) = promoted_numeric_kind(&left_type, &right_type) {
            self.binary_operations.insert(
                span,
                CheckedBinaryOperation::Numeric {
                    operator: BinaryOperator::Add,
                    kind,
                },
            );
            return Ok(ExpressionType::Value(numeric_type(kind)));
        }
        let includes_string = left_type == ExpressionType::Value(TypeName::String)
            || right_type == ExpressionType::Value(TypeName::String);
        if includes_string
            && left_type != ExpressionType::Void
            && right_type != ExpressionType::Void
        {
            self.binary_operations
                .insert(span, CheckedBinaryOperation::StringConcat);
            return Ok(ExpressionType::Value(TypeName::String));
        }
        Err(invalid_binary_operands(
            BinaryOperator::Add,
            &left_type,
            &right_type,
            span,
        ))
    }

    fn checked_numeric_or_temporal_type(
        &mut self,
        left_type: ExpressionType,
        operator: BinaryOperator,
        right_type: ExpressionType,
        span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        if is_numeric_type(&left_type) && is_numeric_type(&right_type) {
            if is_ordering_operator(operator) {
                return Ok(ExpressionType::Value(TypeName::Boolean));
            }
            let kind = promoted_numeric_kind(&left_type, &right_type)
                .expect("numeric operands have a promoted kind");
            self.binary_operations
                .insert(span, CheckedBinaryOperation::Numeric { operator, kind });
            return Ok(ExpressionType::Value(numeric_type(kind)));
        }
        if is_ordering_operator(operator) && same_temporal_type(&left_type, &right_type) {
            return Ok(ExpressionType::Value(TypeName::Boolean));
        }
        Err(invalid_binary_operands(
            operator,
            &left_type,
            &right_type,
            span,
        ))
    }

    fn checked_bitwise_type(
        &mut self,
        left_type: ExpressionType,
        operator: BinaryOperator,
        right_type: ExpressionType,
        span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let boolean = ExpressionType::Value(TypeName::Boolean);
        if left_type == boolean && right_type == boolean {
            self.binary_operations
                .insert(span, CheckedBinaryOperation::BooleanBitwise(operator));
            return Ok(ExpressionType::Value(TypeName::Boolean));
        }
        if let Some(kind) = promoted_integral_kind(&left_type, &right_type) {
            self.binary_operations
                .insert(span, CheckedBinaryOperation::Integral { operator, kind });
            return Ok(ExpressionType::Value(numeric_type(kind)));
        }
        Err(invalid_binary_operands(
            operator,
            &left_type,
            &right_type,
            span,
        ))
    }

    fn checked_shift_type(
        &mut self,
        left_type: ExpressionType,
        operator: BinaryOperator,
        right_type: ExpressionType,
        span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let Some(kind) = integral_kind(&left_type) else {
            return Err(invalid_binary_operands(
                operator,
                &left_type,
                &right_type,
                span,
            ));
        };
        if right_type != ExpressionType::Value(TypeName::Integer) {
            return Err(invalid_binary_operands(
                operator,
                &left_type,
                &right_type,
                span,
            ));
        }
        self.binary_operations
            .insert(span, CheckedBinaryOperation::Shift { operator, kind });
        Ok(ExpressionType::Value(numeric_type(kind)))
    }

    fn conditional_type(
        &mut self,
        condition: &Expression,
        when_true: &Expression,
        when_false: &Expression,
        question_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        self.require_operand(condition, &TypeName::Boolean, condition.span())?;
        let true_type = self.expression_type(when_true)?;
        let false_type = self.expression_type(when_false)?;
        self.join_expression_types(
            &true_type,
            &false_type,
            question_span,
            "conditional branches",
        )
    }

    fn null_coalescing_type(
        &mut self,
        left: &Expression,
        right: &Expression,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let left_type = self.expression_type(left)?;
        let right_type = self.expression_type(right)?;
        self.join_expression_types(
            &left_type,
            &right_type,
            operator_span,
            "null-coalescing operands",
        )
    }

    fn join_expression_types(
        &self,
        left_type: &ExpressionType,
        right_type: &ExpressionType,
        operator_span: Span,
        subject: &str,
    ) -> Result<ExpressionType, Diagnostic> {
        match (left_type, right_type) {
            (ExpressionType::Void, _) | (_, ExpressionType::Void) => Err(Diagnostic::new(
                format!(
                    "{subject} must produce values, found {} and {}",
                    left_type.name(),
                    right_type.name()
                ),
                operator_span,
            )),
            (ExpressionType::Null, ExpressionType::Null) => Ok(ExpressionType::Null),
            (ExpressionType::Null, ExpressionType::Value(ty))
            | (ExpressionType::Value(ty), ExpressionType::Null) => {
                Ok(ExpressionType::Value(ty.clone()))
            }
            (ExpressionType::Value(left), ExpressionType::Value(right)) if left == right => {
                Ok(ExpressionType::Value(left.clone()))
            }
            (ExpressionType::Value(left), ExpressionType::Value(right))
                if self.is_subtype(left, right) =>
            {
                Ok(ExpressionType::Value(right.clone()))
            }
            (ExpressionType::Value(left), ExpressionType::Value(right))
                if self.is_subtype(right, left) =>
            {
                Ok(ExpressionType::Value(left.clone()))
            }
            (ExpressionType::Value(_), ExpressionType::Value(_)) => {
                Ok(ExpressionType::Value(TypeName::Object))
            }
        }
    }

    fn validate_safe_method_target(
        &self,
        call_span: Span,
        navigation_span: Span,
    ) -> Result<(), Diagnostic> {
        let target = self
            .calls
            .get(&call_span)
            .copied()
            .expect("checked method call has a target");
        let valid = match target {
            CallTarget::InstanceMethod(_)
            | CallTarget::SObjectGet
            | CallTarget::SObjectPut
            | CallTarget::AggregateResultGet => true,
            CallTarget::Intrinsic(intrinsic) => !intrinsic.is_static(),
            CallTarget::TopLevelMethod(_)
            | CallTarget::StaticMethod(_)
            | CallTarget::SuperMethod(_)
            | CallTarget::Constructor { .. }
            | CallTarget::CustomExceptionConstructor { .. }
            | CallTarget::SObjectConstructor { .. }
            | CallTarget::PlatformConstructor(_)
            | CallTarget::DatabaseDml(_)
            | CallTarget::DatabaseQuery { .. } => false,
            CallTarget::EnumMethod { method, .. } => {
                matches!(method, hir::EnumMethod::Name | hir::EnumMethod::Ordinal)
            }
        };
        if valid {
            Ok(())
        } else {
            Err(Diagnostic::new(
                "safe navigation requires an instance receiver",
                navigation_span,
            ))
        }
    }

    fn validate_safe_member_target(
        &self,
        member_span: Span,
        navigation_span: Span,
    ) -> Result<(), Diagnostic> {
        let valid = matches!(
            self.members.get(&member_span),
            Some(
                MemberTarget::Instance(_)
                    | MemberTarget::SObjectField { .. }
                    | MemberTarget::SObjectRelationship { .. }
            )
        );
        if valid {
            Ok(())
        } else {
            Err(Diagnostic::new(
                "safe navigation requires an instance receiver",
                navigation_span,
            ))
        }
    }

    fn instanceof_type(
        &mut self,
        value: &Expression,
        target: &TypeName,
        target_span: Span,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        self.validate_type(target, target_span)?;
        let actual = self.expression_type(value)?;
        match actual {
            ExpressionType::Null => Ok(ExpressionType::Value(TypeName::Boolean)),
            ExpressionType::Void => Err(Diagnostic::new(
                "`instanceof` left operand cannot be void",
                value.span(),
            )),
            ExpressionType::Value(actual) => {
                if self.is_runtime_subtype(&actual, target) {
                    return Err(Diagnostic::new(
                        format!(
                            "`instanceof` test is always true because {} is a {}",
                            actual.apex_name(),
                            target.apex_name()
                        ),
                        operator_span,
                    ));
                }
                if !self.instanceof_types_can_overlap(&actual, target) {
                    return Err(Diagnostic::new(
                        format!(
                            "{} is not a viable runtime type for {}",
                            target.apex_name(),
                            actual.apex_name()
                        ),
                        target_span,
                    ));
                }
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
        }
    }

    fn is_runtime_subtype(&self, actual: &TypeName, expected: &TypeName) -> bool {
        if actual == expected || *expected == TypeName::Object {
            return true;
        }
        if *expected == TypeName::Exception && self.is_exception_type(actual) {
            return true;
        }
        if self.is_sobject_type(actual) && self.is_dynamic_sobject_type(expected) {
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

    fn instanceof_types_can_overlap(&self, declared: &TypeName, target: &TypeName) -> bool {
        if self.is_runtime_subtype(target, declared) {
            return true;
        }
        let (TypeName::Custom(declared), TypeName::Custom(target)) = (declared, target) else {
            return false;
        };
        let (Some(declared_id), Some(target_id)) = (
            self.class_ids.get(&declared.canonical).copied(),
            self.class_ids.get(&target.canonical).copied(),
        ) else {
            return false;
        };
        let declared_class = &self.classes[declared_id];
        let target_class = &self.classes[target_id];
        match (declared_class.kind, target_class.kind) {
            (ClassKind::Interface, ClassKind::Interface) => true,
            (ClassKind::Class, ClassKind::Interface) => {
                declared_class.modifiers.contains(&Modifier::Virtual)
                    || declared_class.modifiers.contains(&Modifier::Abstract)
            }
            (ClassKind::Interface, ClassKind::Class) => {
                target_class.modifiers.contains(&Modifier::Virtual)
                    || target_class.modifiers.contains(&Modifier::Abstract)
            }
            (ClassKind::Class, ClassKind::Class) | (ClassKind::Enum, _) | (_, ClassKind::Enum) => {
                false
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

    fn require_mutable_integral(
        &mut self,
        operand: &Expression,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let actual = match operand {
            Expression::Variable(identifier) => {
                self.assignment_target_type(&AssignmentTarget::Variable(identifier.clone()), true)?
            }
            Expression::Index {
                collection,
                index,
                span,
            } => self.assignment_target_type(
                &AssignmentTarget::Index {
                    collection: collection.clone(),
                    index: index.clone(),
                    span: *span,
                },
                true,
            )?,
            Expression::MemberAccess {
                receiver,
                member,
                safe_navigation: false,
                span,
                ..
            } => self.assignment_target_type(
                &AssignmentTarget::Member {
                    receiver: receiver.clone(),
                    member: member.clone(),
                    span: *span,
                },
                true,
            )?,
            Expression::MemberAccess {
                navigation_span, ..
            } => {
                return Err(Diagnostic::new(
                    "safe-navigation access cannot be mutated",
                    *navigation_span,
                ));
            }
            _ => {
                return Err(Diagnostic::new(
                    "increment/decrement operand must be a variable",
                    operator_span,
                ));
            }
        };
        if !matches!(actual, TypeName::Integer | TypeName::Long) {
            return Err(Diagnostic::new(
                format!(
                    "increment/decrement requires Integer or Long, found {}",
                    actual.apex_name()
                ),
                operator_span,
            ));
        }
        Ok(ExpressionType::Value(actual))
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
    if modifiers.contains(&Modifier::Transient) {
        return Err(Diagnostic::new(
            format!(
                "modifier `transient` on {subject} is parsed but unsupported by the active compatibility profile"
            ),
            span,
        ));
    }
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

fn class_is_test(class: &ClassDeclaration) -> bool {
    class
        .annotations
        .iter()
        .any(|annotation| annotation.kind.is_test())
}

fn is_platform_async_interface(name: &str) -> bool {
    is_queueable_interface(name) || is_batchable_interface(name) || is_schedulable_interface(name)
}

fn is_queueable_interface(name: &str) -> bool {
    matches!(name, "queueable" | "system.queueable")
}

fn is_batchable_interface(name: &str) -> bool {
    matches!(name, "batchable" | "database.batchable")
}

fn is_schedulable_interface(name: &str) -> bool {
    matches!(name, "schedulable" | "system.schedulable")
}

fn is_future_parameter_type(ty: &TypeName) -> bool {
    matches!(
        ty,
        TypeName::String
            | TypeName::Boolean
            | TypeName::Integer
            | TypeName::Long
            | TypeName::Decimal
            | TypeName::Date
            | TypeName::Datetime
            | TypeName::Time
            | TypeName::Id
            | TypeName::Blob
    ) || matches!(
        ty,
        TypeName::List(element) | TypeName::Set(element)
            if is_future_parameter_type(element)
    )
}

fn async_contract_error(class: &ClassDeclaration, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(message, class.name.span)
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
        Modifier::Transient => "transient",
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

fn qualified_expression_name(expression: &Expression) -> Option<String> {
    match expression {
        Expression::Variable(identifier) => Some(identifier.canonical.clone()),
        Expression::MemberAccess {
            receiver,
            member,
            safe_navigation: false,
            ..
        } => Some(format!(
            "{}.{}",
            qualified_expression_name(receiver)?,
            member.canonical
        )),
        _ => None,
    }
}

fn is_database_receiver(expression: &Expression) -> bool {
    matches!(
        qualified_expression_name(expression).as_deref(),
        Some("database" | "system.database")
    )
}

fn outermost_type(qualified: &str) -> &str {
    qualified.split('.').next().unwrap_or(qualified)
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

fn literal_expression_type(expression: &Expression) -> Option<Result<ExpressionType, Diagnostic>> {
    Some(match expression {
        Expression::StringLiteral(..) => Ok(ExpressionType::Value(TypeName::String)),
        Expression::BooleanLiteral(..) => Ok(ExpressionType::Value(TypeName::Boolean)),
        Expression::IntegerLiteral(value, _) if *value > i64::from(i32::MAX) => {
            Ok(ExpressionType::Value(TypeName::Long))
        }
        Expression::IntegerLiteral(..) => Ok(ExpressionType::Value(TypeName::Integer)),
        Expression::LongLiteral(value, span) if *value > i128::from(i64::MAX) => {
            Err(Diagnostic::new("Long literal is out of range", *span))
        }
        Expression::LongLiteral(..) => Ok(ExpressionType::Value(TypeName::Long)),
        Expression::DecimalLiteral(..) => Ok(ExpressionType::Value(TypeName::Decimal)),
        Expression::NullLiteral(..) => Ok(ExpressionType::Null),
        _ => return None,
    })
}

fn checked_equality_type(
    left: ExpressionType,
    operator: BinaryOperator,
    right: ExpressionType,
    span: Span,
) -> Result<ExpressionType, Diagnostic> {
    let comparable = match (&left, &right) {
        (ExpressionType::Value(left_value), ExpressionType::Value(right_value)) => {
            left_value == right_value || (is_numeric_type(&left) && is_numeric_type(&right))
        }
        (ExpressionType::Null, ExpressionType::Value(_))
        | (ExpressionType::Value(_), ExpressionType::Null)
        | (ExpressionType::Null, ExpressionType::Null) => true,
        (ExpressionType::Void, _) | (_, ExpressionType::Void) => false,
    };
    if comparable {
        Ok(ExpressionType::Value(TypeName::Boolean))
    } else {
        Err(invalid_binary_operands(operator, &left, &right, span))
    }
}

fn checked_boolean_type(
    left: ExpressionType,
    operator: BinaryOperator,
    right: ExpressionType,
    span: Span,
) -> Result<ExpressionType, Diagnostic> {
    let boolean = ExpressionType::Value(TypeName::Boolean);
    if left == boolean && right == boolean {
        Ok(ExpressionType::Value(TypeName::Boolean))
    } else {
        Err(invalid_binary_operands(operator, &left, &right, span))
    }
}

fn is_ordering_operator(operator: BinaryOperator) -> bool {
    matches!(
        operator,
        BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
    )
}

fn same_temporal_type(left: &ExpressionType, right: &ExpressionType) -> bool {
    matches!(
        (left, right),
        (
            ExpressionType::Value(TypeName::Date),
            ExpressionType::Value(TypeName::Date)
        ) | (
            ExpressionType::Value(TypeName::Datetime),
            ExpressionType::Value(TypeName::Datetime)
        ) | (
            ExpressionType::Value(TypeName::Time),
            ExpressionType::Value(TypeName::Time)
        )
    )
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

fn is_platform_static_owner(name: &str) -> bool {
    matches!(
        name,
        "date"
            | "datetime"
            | "time"
            | "decimal"
            | "id"
            | "blob"
            | "json"
            | "pattern"
            | "schema"
            | "test"
            | "limits"
            | "userinfo"
            | "encodingutil"
            | "eventbus"
    )
}

fn is_numeric_type(ty: &ExpressionType) -> bool {
    matches!(
        ty,
        ExpressionType::Value(TypeName::Integer | TypeName::Long | TypeName::Decimal)
    )
}

fn numeric_kind(ty: &ExpressionType) -> Option<NumericKind> {
    match ty {
        ExpressionType::Value(TypeName::Integer) => Some(NumericKind::Integer),
        ExpressionType::Value(TypeName::Long) => Some(NumericKind::Long),
        ExpressionType::Value(TypeName::Decimal) => Some(NumericKind::Decimal),
        _ => None,
    }
}

fn integral_kind(ty: &ExpressionType) -> Option<NumericKind> {
    match ty {
        ExpressionType::Value(TypeName::Integer) => Some(NumericKind::Integer),
        ExpressionType::Value(TypeName::Long) => Some(NumericKind::Long),
        _ => None,
    }
}

fn promoted_numeric_kind(left: &ExpressionType, right: &ExpressionType) -> Option<NumericKind> {
    let left = numeric_kind(left)?;
    let right = numeric_kind(right)?;
    Some(
        if left == NumericKind::Decimal || right == NumericKind::Decimal {
            NumericKind::Decimal
        } else if left == NumericKind::Long || right == NumericKind::Long {
            NumericKind::Long
        } else {
            NumericKind::Integer
        },
    )
}

fn promoted_integral_kind(left: &ExpressionType, right: &ExpressionType) -> Option<NumericKind> {
    let left = integral_kind(left)?;
    let right = integral_kind(right)?;
    Some(if left == NumericKind::Long || right == NumericKind::Long {
        NumericKind::Long
    } else {
        NumericKind::Integer
    })
}

fn numeric_type(kind: NumericKind) -> TypeName {
    match kind {
        NumericKind::Integer => TypeName::Integer,
        NumericKind::Long => TypeName::Long,
        NumericKind::Decimal => TypeName::Decimal,
    }
}

fn compound_binary_operator(operator: AssignmentOperator) -> BinaryOperator {
    match operator {
        AssignmentOperator::Assign => {
            unreachable!("simple assignment does not have a binary operation")
        }
        AssignmentOperator::Add => BinaryOperator::Add,
        AssignmentOperator::Subtract => BinaryOperator::Subtract,
        AssignmentOperator::Multiply => BinaryOperator::Multiply,
        AssignmentOperator::Divide => BinaryOperator::Divide,
        AssignmentOperator::Remainder => BinaryOperator::Remainder,
        AssignmentOperator::BitwiseAnd => BinaryOperator::BitwiseAnd,
        AssignmentOperator::BitwiseOr => BinaryOperator::BitwiseOr,
        AssignmentOperator::BitwiseXor => BinaryOperator::BitwiseXor,
        AssignmentOperator::ShiftLeft => BinaryOperator::ShiftLeft,
        AssignmentOperator::ShiftRight => BinaryOperator::ShiftRight,
        AssignmentOperator::UnsignedShiftRight => BinaryOperator::UnsignedShiftRight,
    }
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
        BinaryOperator::BitwiseAnd => "&",
        BinaryOperator::BitwiseOr => "|",
        BinaryOperator::BitwiseXor => "^",
        BinaryOperator::ShiftLeft => "<<",
        BinaryOperator::ShiftRight => ">>",
        BinaryOperator::UnsignedShiftRight => ">>>",
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

fn reject_unsupported_annotations(annotations: &[Annotation]) -> Result<(), Diagnostic> {
    if let Some(annotation) = annotations
        .iter()
        .find(|annotation| annotation.kind == AnnotationKind::Other)
    {
        Err(unsupported_annotation(annotation))
    } else {
        Ok(())
    }
}

fn unsupported_annotation(annotation: &Annotation) -> Diagnostic {
    Diagnostic::new(
        format!(
            "annotation `@{}` is parsed but unsupported by the active compatibility profile",
            annotation.name.spelling
        ),
        annotation.span,
    )
}

fn apex_field_type(field_type: &FieldType) -> TypeName {
    match field_type {
        FieldType::Boolean => TypeName::Boolean,
        FieldType::Integer => TypeName::Integer,
        FieldType::String | FieldType::Id | FieldType::Reference { .. } => TypeName::String,
        FieldType::Date => TypeName::Date,
        FieldType::Datetime => TypeName::Datetime,
    }
}

#[cfg(test)]
mod tests;
