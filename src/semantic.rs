use crate::{
    ast::{
        AssignmentTarget, BinaryOperator, CatchClause, CollectionInitializer, Expression,
        Identifier, MethodDeclaration, PostfixOperator, Program, ReturnType, Statement, TypeName,
        UnaryOperator,
    },
    diagnostic::Diagnostic,
    span::Span,
};
use std::collections::HashMap;

pub fn check(program: &Program) -> Result<(), Diagnostic> {
    Checker::new().check_program(program)
}

#[derive(Clone)]
struct MethodSignature {
    id: usize,
    parameter_types: Vec<TypeName>,
    return_type: ReturnType,
}

struct Checker {
    scopes: Vec<HashMap<String, TypeName>>,
    loop_depth: usize,
    return_type: Option<ReturnType>,
    methods: HashMap<String, Vec<MethodSignature>>,
}

impl Checker {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            loop_depth: 0,
            return_type: None,
            methods: HashMap::new(),
        }
    }

    fn check_program(&mut self, program: &Program) -> Result<(), Diagnostic> {
        self.collect_method_signatures(program)?;
        for method in &program.methods {
            self.check_method(method)?;
        }
        for statement in &program.statements {
            self.check_statement(statement)?;
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
        let saved_scopes = std::mem::replace(&mut self.scopes, vec![HashMap::new()]);
        let saved_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let saved_return_type = self.return_type.replace(method.return_type.clone());

        let result = (|| {
            for parameter in &method.parameters {
                if self.current_scope().contains_key(&parameter.name.canonical) {
                    return Err(Diagnostic::new(
                        format!("duplicate parameter `{}`", parameter.name.spelling),
                        parameter.name.span,
                    ));
                }
                self.current_scope_mut()
                    .insert(parameter.name.canonical.clone(), parameter.ty.clone());
            }

            self.check_method_body(&method.body)?;
            if matches!(method.return_type, ReturnType::Value(_))
                && !statement_definitely_returns_or_throws(&method.body)
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
                if self.current_scope().contains_key(&name.canonical) {
                    return Err(Diagnostic::new(
                        format!("duplicate variable `{}`", name.spelling),
                        name.span,
                    ));
                }
                let initializer_type = self.expression_type(initializer)?;
                require_assignable(ty, &initializer_type, initializer.span())?;
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
                require_assignable(
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
                if is_assignable(&expected, &actual) {
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
        match expression {
            Expression::StringLiteral(..) => Ok(ExpressionType::Value(TypeName::String)),
            Expression::BooleanLiteral(..) => Ok(ExpressionType::Value(TypeName::Boolean)),
            Expression::IntegerLiteral(..) => Ok(ExpressionType::Value(TypeName::Integer)),
            Expression::NullLiteral(..) => Ok(ExpressionType::Null),
            Expression::Variable(identifier) => self
                .lookup(&identifier.canonical)
                .cloned()
                .map(ExpressionType::Value)
                .ok_or_else(|| unknown_variable(identifier)),
            Expression::Assignment { target, value, .. } => {
                let expected = self.assignment_target_type(target)?;
                let actual = self.expression_type(value)?;
                require_assignable(&expected, &actual, value.span())?;
                Ok(ExpressionType::Value(expected))
            }
            Expression::NewCollection {
                ty, initializer, ..
            } => self.new_collection_type(ty, initializer),
            Expression::NewException {
                exception_type,
                message,
                ..
            } => self.new_exception_type(exception_type, message.as_deref()),
            Expression::Index {
                collection, index, ..
            } => self
                .index_type(collection, index)
                .map(ExpressionType::Value),
            Expression::FunctionCall {
                name,
                arguments,
                resolved_method,
                ..
            } => self.function_call_type(name, arguments, resolved_method),
            Expression::MethodCall {
                receiver,
                method,
                arguments,
                ..
            } => self.method_call_type(receiver, method, arguments),
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

    fn new_exception_type(
        &mut self,
        exception_type: &TypeName,
        message: Option<&Expression>,
    ) -> Result<ExpressionType, Diagnostic> {
        if !exception_type.is_exception() {
            return Err(Diagnostic::new(
                format!("{} is not an Exception type", exception_type.apex_name()),
                message.map_or(Span::new(0, 0), Expression::span),
            ));
        }
        if let Some(message) = message {
            self.require_operand(message, &TypeName::String, message.span())?;
        }
        Ok(ExpressionType::Value(exception_type.clone()))
    }

    fn function_call_type(
        &mut self,
        name: &Identifier,
        arguments: &[Expression],
        resolved_method: &std::cell::Cell<Option<usize>>,
    ) -> Result<ExpressionType, Diagnostic> {
        let argument_types = arguments
            .iter()
            .map(|argument| self.expression_type(argument))
            .collect::<Result<Vec<_>, _>>()?;
        let Some(overloads) = self.methods.get(&name.canonical) else {
            return Err(Diagnostic::new(
                format!("unknown method `{}`", name.spelling),
                name.span,
            ));
        };

        let mut matches = overloads
            .iter()
            .filter(|overload| overload.parameter_types.len() == argument_types.len())
            .filter_map(|overload| {
                overload
                    .parameter_types
                    .iter()
                    .zip(&argument_types)
                    .map(|(expected, actual)| conversion_rank(expected, actual))
                    .try_fold(0_u32, |total, rank| rank.map(|rank| total + rank))
                    .map(|rank| (rank, overload))
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|(rank, _)| *rank);

        let Some((best_rank, best)) = matches.first().copied() else {
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
        };
        if matches.get(1).is_some_and(|(rank, _)| *rank == best_rank) {
            return Err(Diagnostic::new(
                format!("ambiguous overload for method `{}`", name.spelling),
                name.span,
            ));
        }

        resolved_method.set(Some(best.id));
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
                    || (source.is_exception() && target.is_exception())
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
                    require_assignable(element_type, &actual, element.span())?;
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
                    require_assignable(key_type, &actual_key, entry.key.span())?;
                    let actual_value = self.expression_type(&entry.value)?;
                    require_assignable(value_type, &actual_value, entry.value.span())?;
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
            AssignmentTarget::Variable(identifier) => self
                .lookup(&identifier.canonical)
                .cloned()
                .ok_or_else(|| unknown_variable(identifier)),
            AssignmentTarget::Index {
                collection, index, ..
            } => self.index_type(collection, index),
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
    ) -> Result<ExpressionType, Diagnostic> {
        let result = if let Expression::Variable(identifier) = receiver {
            if let Some(receiver_type) = self.lookup(&identifier.canonical).cloned() {
                self.instance_method_type(&receiver_type, method, arguments)
            } else {
                match identifier.canonical.as_str() {
                    "string" => self.static_string_method_type(method, arguments),
                    "math" => self.static_math_method_type(method, arguments),
                    "system" => self.static_system_method_type(method, arguments),
                    _ => Err(unknown_variable(identifier)),
                }
            }
        } else {
            match self.expression_type(receiver)? {
                ExpressionType::Value(receiver_type) => {
                    self.instance_method_type(&receiver_type, method, arguments)
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
            _ => Err(unknown_method(receiver_type, method)),
        }
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
        if is_assignable(expected, &actual) {
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
            Expression::Variable(identifier) => self
                .lookup(&identifier.canonical)
                .cloned()
                .ok_or_else(|| unknown_variable(identifier))?,
            Expression::Index {
                collection, index, ..
            } => self.index_type(collection, index)?,
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum ExpressionType {
    Value(TypeName),
    Null,
    Void,
}

impl ExpressionType {
    fn name(&self) -> String {
        match self {
            Self::Value(ty) => ty.apex_name(),
            Self::Null => "null".to_owned(),
            Self::Void => "void".to_owned(),
        }
    }
}

fn is_assignable(expected: &TypeName, actual: &ExpressionType) -> bool {
    match actual {
        ExpressionType::Value(actual) => {
            actual == expected
                || *expected == TypeName::Object
                || (*expected == TypeName::Exception && actual.is_exception())
        }
        ExpressionType::Null => true,
        ExpressionType::Void => false,
    }
}

fn conversion_rank(expected: &TypeName, actual: &ExpressionType) -> Option<u32> {
    match actual {
        ExpressionType::Value(actual) if actual == expected => Some(0),
        ExpressionType::Value(actual)
            if *expected == TypeName::Exception && actual.is_exception() =>
        {
            Some(1)
        }
        ExpressionType::Value(_) if *expected == TypeName::Object => Some(2),
        ExpressionType::Null if *expected == TypeName::Object => Some(2),
        ExpressionType::Null if *expected == TypeName::Exception => Some(1),
        ExpressionType::Null => Some(0),
        ExpressionType::Value(_) | ExpressionType::Void => None,
    }
}

fn require_assignable(
    expected: &TypeName,
    actual: &ExpressionType,
    span: Span,
) -> Result<(), Diagnostic> {
    if is_assignable(expected, actual) {
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
    match statement {
        Statement::Return { .. } | Statement::Throw { .. } => true,
        Statement::Block { statements, .. } => {
            for statement in statements {
                if matches!(
                    statement,
                    Statement::Break { .. } | Statement::Continue { .. }
                ) {
                    return false;
                }
                if statement_definitely_returns_or_throws(statement) {
                    return true;
                }
            }
            false
        }
        Statement::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => {
            statement_definitely_returns_or_throws(then_branch)
                && statement_definitely_returns_or_throws(else_branch)
        }
        Statement::Try {
            try_block,
            catches,
            finally_block,
            ..
        } => {
            finally_block
                .as_deref()
                .is_some_and(statement_definitely_returns_or_throws)
                || (statement_definitely_returns_or_throws(try_block)
                    && catches
                        .iter()
                        .all(|catch| statement_definitely_returns_or_throws(&catch.body)))
        }
        Statement::VariableDeclaration { .. }
        | Statement::Expression { .. }
        | Statement::If {
            else_branch: None, ..
        }
        | Statement::While { .. }
        | Statement::DoWhile { .. }
        | Statement::For { .. }
        | Statement::ForEach { .. }
        | Statement::Break { .. }
        | Statement::Continue { .. } => false,
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
        check(&program)
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
    }

    #[test]
    fn validates_method_return_types_and_definite_completion() {
        check_source(
            "Integer complete(Boolean branch) { \
                 if (branch) return 1; else throw new MathException('failed'); \
             } \
             void done() { return; }",
        )
        .unwrap();

        let error = check_source("Integer incomplete(Boolean branch) { if (branch) return 1; }")
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
