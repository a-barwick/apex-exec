//! Shared immutable traversal for parsed Apex syntax.
//!
//! Compiler passes can override only the nodes they care about and delegate
//! recursion to the corresponding `walk_*` function. Keeping the tree shape in
//! one place prevents dependency analysis, indexing, and lowering passes from
//! growing independent, incomplete recursive walkers.

use super::{
    Annotation, AnnotationArgument, AssignmentTarget, CatchClause, ClassDeclaration, ClassMember,
    CollectionInitializer, ConstructorDeclaration, Expression, FieldDeclaration, Identifier,
    MapEntry, MethodDeclaration, NamedType, Parameter, Program, PropertyAccessor,
    PropertyDeclaration, ReturnType, SoqlCondition, SoqlInValues, SoqlQuery, SoqlSelectItem,
    SoqlValue, SoslQuery, Statement, SwitchArm, SwitchLabels, TriggerDeclaration, TypeName,
    VariableDeclarator,
};

pub trait Visitor<'ast> {
    fn visit_program(&mut self, program: &'ast Program) {
        walk_program(self, program);
    }

    fn visit_class_declaration(&mut self, class: &'ast ClassDeclaration) {
        walk_class_declaration(self, class);
    }

    fn visit_trigger_declaration(&mut self, trigger: &'ast TriggerDeclaration) {
        walk_trigger_declaration(self, trigger);
    }

    fn visit_class_member(&mut self, member: &'ast ClassMember) {
        walk_class_member(self, member);
    }

    fn visit_field_declaration(&mut self, field: &'ast FieldDeclaration) {
        walk_field_declaration(self, field);
    }

    fn visit_property_declaration(&mut self, property: &'ast PropertyDeclaration) {
        walk_property_declaration(self, property);
    }

    fn visit_property_accessor(&mut self, accessor: &'ast PropertyAccessor) {
        walk_property_accessor(self, accessor);
    }

    fn visit_constructor_declaration(&mut self, constructor: &'ast ConstructorDeclaration) {
        walk_constructor_declaration(self, constructor);
    }

    fn visit_method_declaration(&mut self, method: &'ast MethodDeclaration) {
        walk_method_declaration(self, method);
    }

    fn visit_annotation(&mut self, annotation: &'ast Annotation) {
        walk_annotation(self, annotation);
    }

    fn visit_annotation_argument(&mut self, argument: &'ast AnnotationArgument) {
        walk_annotation_argument(self, argument);
    }

    fn visit_parameter(&mut self, parameter: &'ast Parameter) {
        walk_parameter(self, parameter);
    }

    fn visit_return_type(&mut self, return_type: &'ast ReturnType) {
        walk_return_type(self, return_type);
    }

    fn visit_catch_clause(&mut self, catch: &'ast CatchClause) {
        walk_catch_clause(self, catch);
    }

    fn visit_variable_declarator(&mut self, declarator: &'ast VariableDeclarator) {
        walk_variable_declarator(self, declarator);
    }

    fn visit_switch_arm(&mut self, arm: &'ast SwitchArm) {
        walk_switch_arm(self, arm);
    }

    fn visit_statement(&mut self, statement: &'ast Statement) {
        walk_statement(self, statement);
    }

    fn visit_expression(&mut self, expression: &'ast Expression) {
        walk_expression(self, expression);
    }

    fn visit_assignment_target(&mut self, target: &'ast AssignmentTarget) {
        walk_assignment_target(self, target);
    }

    fn visit_collection_initializer(&mut self, initializer: &'ast CollectionInitializer) {
        walk_collection_initializer(self, initializer);
    }

    fn visit_map_entry(&mut self, entry: &'ast MapEntry) {
        walk_map_entry(self, entry);
    }

    fn visit_type_name(&mut self, ty: &'ast TypeName) {
        walk_type_name(self, ty);
    }

    fn visit_named_type(&mut self, named_type: &'ast NamedType) {
        walk_named_type(self, named_type);
    }

    fn visit_identifier(&mut self, identifier: &'ast Identifier) {
        walk_identifier(self, identifier);
    }
}

pub fn walk_program<'ast, V: Visitor<'ast> + ?Sized>(visitor: &mut V, program: &'ast Program) {
    for class in &program.classes {
        visitor.visit_class_declaration(class);
    }
    for trigger in &program.triggers {
        visitor.visit_trigger_declaration(trigger);
    }
    for method in &program.methods {
        visitor.visit_method_declaration(method);
    }
    for statement in &program.statements {
        visitor.visit_statement(statement);
    }
}

pub fn walk_trigger_declaration<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    trigger: &'ast TriggerDeclaration,
) {
    visitor.visit_identifier(&trigger.name);
    visitor.visit_named_type(&trigger.object);
    visitor.visit_statement(&trigger.body);
}

pub fn walk_class_declaration<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    class: &'ast ClassDeclaration,
) {
    for annotation in &class.annotations {
        visitor.visit_annotation(annotation);
    }
    visitor.visit_identifier(&class.name);
    if let Some(superclass) = &class.superclass {
        visitor.visit_named_type(superclass);
    }
    for interface in &class.interfaces {
        visitor.visit_named_type(interface);
    }
    for member in &class.members {
        visitor.visit_class_member(member);
    }
}

pub fn walk_class_member<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    member: &'ast ClassMember,
) {
    match member {
        ClassMember::Field(field) => visitor.visit_field_declaration(field),
        ClassMember::FieldGroup(group) => {
            for annotation in &group.annotations {
                visitor.visit_annotation(annotation);
            }
            visitor.visit_type_name(&group.ty);
            for declarator in &group.declarators {
                visitor.visit_variable_declarator(declarator);
            }
        }
        ClassMember::Property(property) => visitor.visit_property_declaration(property),
        ClassMember::Constructor(constructor) => {
            visitor.visit_constructor_declaration(constructor);
        }
        ClassMember::Method(method) => visitor.visit_method_declaration(method),
        ClassMember::Initializer(initializer) => visitor.visit_statement(&initializer.body),
    }
}

pub fn walk_field_declaration<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    field: &'ast FieldDeclaration,
) {
    for annotation in &field.annotations {
        visitor.visit_annotation(annotation);
    }
    visitor.visit_type_name(&field.ty);
    visitor.visit_identifier(&field.name);
    if let Some(initializer) = &field.initializer {
        visitor.visit_expression(initializer);
    }
}

pub fn walk_property_declaration<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    property: &'ast PropertyDeclaration,
) {
    for annotation in &property.annotations {
        visitor.visit_annotation(annotation);
    }
    visitor.visit_type_name(&property.ty);
    visitor.visit_identifier(&property.name);
    for accessor in &property.accessors {
        visitor.visit_property_accessor(accessor);
    }
}

pub fn walk_property_accessor<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    accessor: &'ast PropertyAccessor,
) {
    if let Some(body) = &accessor.body {
        visitor.visit_statement(body);
    }
}

pub fn walk_constructor_declaration<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    constructor: &'ast ConstructorDeclaration,
) {
    for annotation in &constructor.annotations {
        visitor.visit_annotation(annotation);
    }
    visitor.visit_identifier(&constructor.name);
    for parameter in &constructor.parameters {
        visitor.visit_parameter(parameter);
    }
    if let Some(delegation) = &constructor.delegation {
        for argument in &delegation.arguments {
            visitor.visit_expression(argument);
        }
    }
    visitor.visit_statement(&constructor.body);
}

pub fn walk_method_declaration<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    method: &'ast MethodDeclaration,
) {
    for annotation in &method.annotations {
        visitor.visit_annotation(annotation);
    }
    visitor.visit_return_type(&method.return_type);
    visitor.visit_identifier(&method.name);
    for parameter in &method.parameters {
        visitor.visit_parameter(parameter);
    }
    if let Some(body) = &method.body {
        visitor.visit_statement(body);
    }
}

pub fn walk_annotation<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    annotation: &'ast Annotation,
) {
    visitor.visit_identifier(&annotation.name);
    for argument in &annotation.arguments {
        visitor.visit_annotation_argument(argument);
    }
}

pub fn walk_annotation_argument<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    argument: &'ast AnnotationArgument,
) {
    if let Some(name) = &argument.name {
        visitor.visit_identifier(name);
    }
    visitor.visit_expression(&argument.value);
}

pub fn walk_parameter<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    parameter: &'ast Parameter,
) {
    visitor.visit_type_name(&parameter.ty);
    visitor.visit_identifier(&parameter.name);
}

pub fn walk_return_type<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    return_type: &'ast ReturnType,
) {
    if let ReturnType::Value(ty) = return_type {
        visitor.visit_type_name(ty);
    }
}

pub fn walk_catch_clause<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    catch: &'ast CatchClause,
) {
    visitor.visit_type_name(&catch.exception_type);
    visitor.visit_identifier(&catch.name);
    visitor.visit_statement(&catch.body);
}

pub fn walk_variable_declarator<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    declarator: &'ast VariableDeclarator,
) {
    visitor.visit_identifier(&declarator.name);
    if let Some(initializer) = &declarator.initializer {
        visitor.visit_expression(initializer);
    }
}

pub fn walk_switch_arm<'ast, V: Visitor<'ast> + ?Sized>(visitor: &mut V, arm: &'ast SwitchArm) {
    match &arm.labels {
        SwitchLabels::Expressions(labels) => {
            for label in labels {
                visitor.visit_expression(label);
            }
        }
        SwitchLabels::TypePattern { ty, binding, .. } => {
            visitor.visit_type_name(ty);
            visitor.visit_identifier(binding);
        }
        SwitchLabels::Else(_) => {}
    }
    visitor.visit_statement(&arm.body);
}

pub fn walk_statement<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    statement: &'ast Statement,
) {
    match statement {
        declaration @ Statement::VariableDeclaration { .. }
        | declaration @ Statement::LocalDeclaration { .. } => {
            walk_declaration_statement(visitor, declaration);
        }
        Statement::Sequence { statements, .. } | Statement::Block { statements, .. } => {
            statements
                .iter()
                .for_each(|statement| visitor.visit_statement(statement));
        }
        Statement::Expression { expression, .. } => visitor.visit_expression(expression),
        control @ Statement::If { .. }
        | control @ Statement::While { .. }
        | control @ Statement::DoWhile { .. }
        | control @ Statement::Switch { .. }
        | control @ Statement::For { .. }
        | control @ Statement::ForEach { .. } => walk_control_statement(visitor, control),
        Statement::Try {
            try_block,
            catches,
            finally_block,
            ..
        } => {
            visitor.visit_statement(try_block);
            for catch in catches {
                visitor.visit_catch_clause(catch);
            }
            if let Some(finally_block) = finally_block {
                visitor.visit_statement(finally_block);
            }
        }
        Statement::Throw { value, .. } | Statement::Dml { value, .. } => {
            visitor.visit_expression(value);
        }
        Statement::RunAs { user, body, .. } => {
            visitor.visit_expression(user);
            visitor.visit_statement(body);
        }
        Statement::Return { value, .. } => {
            if let Some(value) = value {
                visitor.visit_expression(value);
            }
        }
        Statement::Break { .. } | Statement::Continue { .. } => {}
    }
}

fn walk_declaration_statement<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    statement: &'ast Statement,
) {
    match statement {
        Statement::VariableDeclaration {
            ty,
            name,
            initializer,
            ..
        } => {
            visitor.visit_type_name(ty);
            visitor.visit_identifier(name);
            visitor.visit_expression(initializer);
        }
        Statement::LocalDeclaration {
            ty, declarators, ..
        } => {
            visitor.visit_type_name(ty);
            declarators
                .iter()
                .for_each(|declarator| visitor.visit_variable_declarator(declarator));
        }
        _ => unreachable!("caller selects declaration statements"),
    }
}

fn walk_control_statement<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    statement: &'ast Statement,
) {
    match statement {
        Statement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            visitor.visit_expression(condition);
            visitor.visit_statement(then_branch);
            if let Some(else_branch) = else_branch {
                visitor.visit_statement(else_branch);
            }
        }
        Statement::While {
            condition, body, ..
        } => {
            visitor.visit_expression(condition);
            visitor.visit_statement(body);
        }
        Statement::DoWhile {
            body, condition, ..
        } => {
            visitor.visit_statement(body);
            visitor.visit_expression(condition);
        }
        Statement::Switch { value, arms, .. } => {
            visitor.visit_expression(value);
            arms.iter().for_each(|arm| visitor.visit_switch_arm(arm));
        }
        Statement::For {
            initializer,
            condition,
            update,
            body,
            ..
        } => {
            initializer
                .iter()
                .for_each(|initializer| visitor.visit_statement(initializer));
            condition
                .iter()
                .for_each(|condition| visitor.visit_expression(condition));
            update
                .iter()
                .for_each(|update| visitor.visit_statement(update));
            visitor.visit_statement(body);
        }
        Statement::ForEach {
            element_type,
            name,
            iterable,
            body,
            ..
        } => {
            visitor.visit_type_name(element_type);
            visitor.visit_identifier(name);
            visitor.visit_expression(iterable);
            visitor.visit_statement(body);
        }
        _ => unreachable!("caller selects control statements"),
    }
}

pub fn walk_expression<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    expression: &'ast Expression,
) {
    match expression {
        Expression::StringLiteral(..) | Expression::BooleanLiteral(..) => {}
        Expression::IntegerLiteral(..) | Expression::LongLiteral(..) => {}
        Expression::DecimalLiteral(..) | Expression::NullLiteral(..) => {}
        Expression::Soql(query) => walk_soql_query(visitor, query),
        Expression::Sosl(query) => walk_sosl_query(visitor, query),
        Expression::Variable(identifier) => visitor.visit_identifier(identifier),
        Expression::TypeLiteral { ty, .. } => visitor.visit_type_name(ty),
        Expression::Assignment { target, value, .. } => {
            visitor.visit_assignment_target(target);
            visitor.visit_expression(value);
        }
        Expression::NewCollection {
            ty, initializer, ..
        } => {
            visitor.visit_type_name(ty);
            visitor.visit_collection_initializer(initializer);
        }
        Expression::NewException {
            exception_type,
            arguments,
            ..
        } => {
            visitor.visit_type_name(exception_type);
            for argument in arguments {
                visitor.visit_expression(argument);
            }
        }
        Expression::NewObject { ty, arguments, .. } => {
            visitor.visit_type_name(ty);
            for argument in arguments {
                visitor.visit_expression(argument);
            }
        }
        Expression::Index {
            collection, index, ..
        } => {
            visitor.visit_expression(collection);
            visitor.visit_expression(index);
        }
        Expression::FunctionCall {
            name, arguments, ..
        } => {
            visitor.visit_identifier(name);
            for argument in arguments {
                visitor.visit_expression(argument);
            }
        }
        Expression::MethodCall {
            receiver,
            method,
            arguments,
            ..
        } => {
            visitor.visit_expression(receiver);
            visitor.visit_identifier(method);
            for argument in arguments {
                visitor.visit_expression(argument);
            }
        }
        Expression::MemberAccess {
            receiver, member, ..
        } => {
            visitor.visit_expression(receiver);
            visitor.visit_identifier(member);
        }
        Expression::Cast { ty, expression, .. } => {
            visitor.visit_type_name(ty);
            visitor.visit_expression(expression);
        }
        Expression::Conditional {
            condition,
            when_true,
            when_false,
            ..
        } => {
            visitor.visit_expression(condition);
            visitor.visit_expression(when_true);
            visitor.visit_expression(when_false);
        }
        Expression::Instanceof { value, target, .. } => {
            visitor.visit_expression(value);
            visitor.visit_type_name(target);
        }
        Expression::Unary { operand, .. } | Expression::Postfix { operand, .. } => {
            visitor.visit_expression(operand);
        }
        Expression::Binary { left, right, .. } | Expression::NullCoalesce { left, right, .. } => {
            visitor.visit_expression(left);
            visitor.visit_expression(right);
        }
    }
}

fn walk_soql_query<'ast, V: Visitor<'ast> + ?Sized>(visitor: &mut V, query: &'ast SoqlQuery) {
    for item in &query.select {
        if let SoqlSelectItem::Subquery { query, .. } = item {
            walk_soql_query(visitor, query);
        }
    }
    if let Some(condition) = &query.where_clause {
        walk_soql_condition(visitor, condition);
    }
    if let Some(condition) = &query.having {
        walk_soql_condition(visitor, condition);
    }
    if let Some(limit) = &query.limit {
        walk_soql_value(visitor, limit);
    }
    if let Some(offset) = &query.offset {
        walk_soql_value(visitor, offset);
    }
}

fn walk_sosl_query<'ast, V: Visitor<'ast> + ?Sized>(visitor: &mut V, query: &'ast SoslQuery) {
    walk_soql_value(visitor, &query.search);
    for returning in &query.returning {
        if let Some(condition) = &returning.where_clause {
            walk_soql_condition(visitor, condition);
        }
        if let Some(limit) = &returning.limit {
            walk_soql_value(visitor, limit);
        }
    }
}

fn walk_soql_condition<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    condition: &'ast SoqlCondition,
) {
    match condition {
        SoqlCondition::AggregateComparison { right, .. }
        | SoqlCondition::Comparison { right, .. } => walk_soql_value(visitor, right),
        SoqlCondition::In { values, .. } => match values {
            SoqlInValues::Values(values) => {
                for value in values {
                    walk_soql_value(visitor, value);
                }
            }
            SoqlInValues::Bind(expression) => visitor.visit_expression(expression),
        },
        SoqlCondition::Not { condition, .. } => walk_soql_condition(visitor, condition),
        SoqlCondition::Logical { left, right, .. } => {
            walk_soql_condition(visitor, left);
            walk_soql_condition(visitor, right);
        }
    }
}

fn walk_soql_value<'ast, V: Visitor<'ast> + ?Sized>(visitor: &mut V, value: &'ast SoqlValue) {
    if let SoqlValue::Bind(expression, _) = value {
        visitor.visit_expression(expression);
    }
}

pub fn walk_assignment_target<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    target: &'ast AssignmentTarget,
) {
    match target {
        AssignmentTarget::Variable(identifier) => visitor.visit_identifier(identifier),
        AssignmentTarget::Index {
            collection, index, ..
        } => {
            visitor.visit_expression(collection);
            visitor.visit_expression(index);
        }
        AssignmentTarget::Member {
            receiver, member, ..
        } => {
            visitor.visit_expression(receiver);
            visitor.visit_identifier(member);
        }
    }
}

pub fn walk_collection_initializer<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    initializer: &'ast CollectionInitializer,
) {
    match initializer {
        CollectionInitializer::Arguments(expressions)
        | CollectionInitializer::Elements(expressions) => {
            for expression in expressions {
                visitor.visit_expression(expression);
            }
        }
        CollectionInitializer::MapEntries(entries) => {
            for entry in entries {
                visitor.visit_map_entry(entry);
            }
        }
        CollectionInitializer::SizedArray(size) => visitor.visit_expression(size),
    }
}

pub fn walk_map_entry<'ast, V: Visitor<'ast> + ?Sized>(visitor: &mut V, entry: &'ast MapEntry) {
    visitor.visit_expression(&entry.key);
    visitor.visit_expression(&entry.value);
}

pub fn walk_type_name<'ast, V: Visitor<'ast> + ?Sized>(visitor: &mut V, ty: &'ast TypeName) {
    match ty {
        TypeName::Custom(named_type) => visitor.visit_named_type(named_type),
        TypeName::List(element) | TypeName::Set(element) | TypeName::Iterable(element) => {
            visitor.visit_type_name(element)
        }
        TypeName::Map(key, value) => {
            visitor.visit_type_name(key);
            visitor.visit_type_name(value);
        }
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
        | TypeName::Object
        | TypeName::Pattern
        | TypeName::Matcher
        | TypeName::Http
        | TypeName::HttpRequest
        | TypeName::HttpResponse
        | TypeName::QueueableContext
        | TypeName::BatchableContext
        | TypeName::QueryLocator
        | TypeName::SaveResult
        | TypeName::UpsertResult
        | TypeName::DeleteResult
        | TypeName::UndeleteResult
        | TypeName::DatabaseError
        | TypeName::StatusCode
        | TypeName::AccessLevel
        | TypeName::AccessType
        | TypeName::SObjectAccessDecision
        | TypeName::SchedulableContext
        | TypeName::SObjectType
        | TypeName::DescribeSObjectResult
        | TypeName::Exception
        | TypeName::NullPointerException
        | TypeName::ListException
        | TypeName::MathException
        | TypeName::TypeException
        | TypeName::StringException
        | TypeName::IllegalArgumentException
        | TypeName::FinalException
        | TypeName::AssertException
        | TypeName::QueryException
        | TypeName::DmlException
        | TypeName::NoAccessException
        | TypeName::AsyncException
        | TypeName::AggregateResult
        | TypeName::Type => {}
    }
}

pub fn walk_named_type<'ast, V: Visitor<'ast> + ?Sized>(
    visitor: &mut V,
    named_type: &'ast NamedType,
) {
    for argument in &named_type.type_arguments {
        visitor.visit_type_name(&argument.ty);
    }
}

pub fn walk_identifier<'ast, V: Visitor<'ast> + ?Sized>(
    _visitor: &mut V,
    _identifier: &'ast Identifier,
) {
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingVisitor {
        identifiers: Vec<String>,
        custom_types: Vec<String>,
        expressions: usize,
    }

    impl<'ast> Visitor<'ast> for RecordingVisitor {
        fn visit_identifier(&mut self, identifier: &'ast Identifier) {
            self.identifiers.push(identifier.canonical.clone());
        }

        fn visit_named_type(&mut self, named_type: &'ast NamedType) {
            self.custom_types.push(named_type.canonical.clone());
        }

        fn visit_expression(&mut self, expression: &'ast Expression) {
            self.expressions += 1;
            walk_expression(self, expression);
        }
    }

    #[test]
    fn traverses_declarations_types_and_nested_expressions() {
        let program = crate::parse(
            "class Service extends Base implements Contract {
                Result build(Input value) {
                    Map<String, Input> values = new Map<String, Input>{'key' => value};
                    return Factory.create(values.get('key'));
                }
            }",
        )
        .unwrap();
        let mut visitor = RecordingVisitor::default();

        visitor.visit_program(&program);

        assert!(visitor.identifiers.contains(&"service".to_owned()));
        assert!(visitor.identifiers.contains(&"build".to_owned()));
        assert!(visitor.identifiers.contains(&"factory".to_owned()));
        assert!(visitor.identifiers.contains(&"create".to_owned()));
        assert_eq!(
            visitor.custom_types,
            ["base", "contract", "result", "input", "input", "input"]
        );
        assert!(visitor.expressions >= 8);
    }

    #[test]
    fn do_while_traversal_preserves_source_order() {
        #[derive(Default)]
        struct IdentifierOrder(Vec<String>);

        impl<'ast> Visitor<'ast> for IdentifierOrder {
            fn visit_identifier(&mut self, identifier: &'ast Identifier) {
                self.0.push(identifier.canonical.clone());
            }
        }

        let program = crate::parse("do { run(); } while (condition);").unwrap();
        let mut order = IdentifierOrder::default();

        order.visit_program(&program);

        assert_eq!(order.0, ["run", "condition"]);
    }
}
