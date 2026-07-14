use crate::{
    ast::{
        AssignmentTarget, BinaryOperator, CollectionInitializer, Expression, Identifier,
        PostfixOperator, Program, Statement, TypeName, UnaryOperator,
    },
    diagnostic::Diagnostic,
    span::Span,
};
use std::{cmp::Ordering, collections::HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CollectionId(usize);

#[derive(Clone, Debug)]
enum Value {
    String(String),
    Boolean(bool),
    Integer(i64),
    Collection(CollectionId),
    Null(Option<TypeName>),
    Void,
}

impl Value {
    fn has_string_type(&self) -> bool {
        matches!(self, Self::String(_) | Self::Null(Some(TypeName::String)))
    }
}

#[derive(Clone, Debug)]
enum Collection {
    List {
        element_type: TypeName,
        elements: Vec<Value>,
        iteration_depth: usize,
    },
    Set {
        element_type: TypeName,
        elements: Vec<Value>,
        iteration_depth: usize,
    },
    Map {
        key_type: TypeName,
        value_type: TypeName,
        entries: Vec<(Value, Value)>,
    },
}

#[derive(Clone, Debug)]
struct Slot {
    ty: TypeName,
    value: Value,
}

#[derive(Clone, Debug)]
struct EvaluatedArgument {
    value: Value,
    span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StaticReceiver {
    String,
    Math,
    System,
}

#[derive(Clone, Debug)]
enum CallReceiver {
    Static(StaticReceiver),
    Value(Value),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flow {
    Normal,
    Break,
    Continue,
    Return,
}

pub struct Interpreter {
    scopes: Vec<HashMap<String, Slot>>,
    collections: Vec<Collection>,
    output: Vec<String>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            collections: Vec::new(),
            output: Vec::new(),
        }
    }

    pub fn execute(mut self, program: &Program) -> Result<Vec<String>, Diagnostic> {
        for statement in &program.statements {
            match self.execute_statement(statement)? {
                Flow::Normal => {}
                Flow::Return => break,
                Flow::Break => {
                    return Err(Diagnostic::new(
                        "`break` escaped semantic validation",
                        statement.span(),
                    ));
                }
                Flow::Continue => {
                    return Err(Diagnostic::new(
                        "`continue` escaped semantic validation",
                        statement.span(),
                    ));
                }
            }
        }
        Ok(self.output)
    }

    fn execute_statement(&mut self, statement: &Statement) -> Result<Flow, Diagnostic> {
        match statement {
            Statement::VariableDeclaration {
                ty,
                name,
                initializer,
                ..
            } => {
                let value = typed_value(self.evaluate(initializer)?, ty);
                self.current_scope_mut().insert(
                    name.canonical.clone(),
                    Slot {
                        ty: ty.clone(),
                        value,
                    },
                );
                Ok(Flow::Normal)
            }
            Statement::Expression { expression, .. } => {
                self.evaluate(expression)?;
                Ok(Flow::Normal)
            }
            Statement::Block { statements, .. } => self.execute_block(statements),
            Statement::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                if self.evaluate_boolean(condition)? {
                    self.execute_statement(then_branch)
                } else if let Some(else_branch) = else_branch {
                    self.execute_statement(else_branch)
                } else {
                    Ok(Flow::Normal)
                }
            }
            Statement::While {
                condition, body, ..
            } => {
                while self.evaluate_boolean(condition)? {
                    match self.execute_statement(body)? {
                        Flow::Normal | Flow::Continue => {}
                        Flow::Break => break,
                        Flow::Return => return Ok(Flow::Return),
                    }
                }
                Ok(Flow::Normal)
            }
            Statement::DoWhile {
                body, condition, ..
            } => {
                loop {
                    match self.execute_statement(body)? {
                        Flow::Normal | Flow::Continue => {}
                        Flow::Break => break,
                        Flow::Return => return Ok(Flow::Return),
                    }
                    if !self.evaluate_boolean(condition)? {
                        break;
                    }
                }
                Ok(Flow::Normal)
            }
            Statement::For {
                initializer,
                condition,
                update,
                body,
                ..
            } => self.execute_for(
                initializer.as_deref(),
                condition.as_ref(),
                update.as_deref(),
                body,
            ),
            Statement::ForEach {
                element_type,
                name,
                iterable,
                body,
                ..
            } => self.execute_for_each(element_type, name, iterable, body),
            Statement::Break { .. } => Ok(Flow::Break),
            Statement::Continue { .. } => Ok(Flow::Continue),
            Statement::Return { .. } => Ok(Flow::Return),
        }
    }

    fn execute_block(&mut self, statements: &[Statement]) -> Result<Flow, Diagnostic> {
        self.scopes.push(HashMap::new());
        let result = (|| {
            for statement in statements {
                let flow = self.execute_statement(statement)?;
                if flow != Flow::Normal {
                    return Ok(flow);
                }
            }
            Ok(Flow::Normal)
        })();
        self.scopes.pop();
        result
    }

    fn execute_for(
        &mut self,
        initializer: Option<&Statement>,
        condition: Option<&Expression>,
        update: Option<&Statement>,
        body: &Statement,
    ) -> Result<Flow, Diagnostic> {
        self.scopes.push(HashMap::new());
        let result = (|| {
            if let Some(initializer) = initializer {
                let flow = self.execute_statement(initializer)?;
                if flow != Flow::Normal {
                    return Ok(flow);
                }
            }
            loop {
                if let Some(condition) = condition
                    && !self.evaluate_boolean(condition)?
                {
                    break;
                }
                match self.execute_statement(body)? {
                    Flow::Normal | Flow::Continue => {}
                    Flow::Break => break,
                    Flow::Return => return Ok(Flow::Return),
                }
                if let Some(update) = update {
                    let flow = self.execute_statement(update)?;
                    if flow != Flow::Normal {
                        return Ok(flow);
                    }
                }
            }
            Ok(Flow::Normal)
        })();
        self.scopes.pop();
        result
    }

    fn execute_for_each(
        &mut self,
        element_type: &TypeName,
        name: &Identifier,
        iterable: &Expression,
        body: &Statement,
    ) -> Result<Flow, Diagnostic> {
        let iterable_value = self.evaluate(iterable)?;
        let id = match iterable_value {
            Value::Collection(id) => id,
            Value::Null(_) => {
                return Err(Diagnostic::new("cannot iterate over null", iterable.span()));
            }
            _ => return Err(invalid_runtime_operands(iterable.span())),
        };

        let elements = match self.collection_mut(id) {
            Collection::List {
                elements,
                iteration_depth,
                ..
            }
            | Collection::Set {
                elements,
                iteration_depth,
                ..
            } => {
                *iteration_depth += 1;
                elements.clone()
            }
            Collection::Map { .. } => {
                return Err(Diagnostic::new(
                    "Map cannot be iterated directly at runtime",
                    iterable.span(),
                ));
            }
        };

        self.scopes.push(HashMap::new());
        let result = (|| {
            for element in elements {
                self.current_scope_mut().insert(
                    name.canonical.clone(),
                    Slot {
                        ty: element_type.clone(),
                        value: typed_value(element, element_type),
                    },
                );
                match self.execute_statement(body)? {
                    Flow::Normal | Flow::Continue => {}
                    Flow::Break => return Ok(Flow::Normal),
                    Flow::Return => return Ok(Flow::Return),
                }
            }
            Ok(Flow::Normal)
        })();
        self.scopes.pop();

        match self.collection_mut(id) {
            Collection::List {
                iteration_depth, ..
            }
            | Collection::Set {
                iteration_depth, ..
            } => *iteration_depth -= 1,
            Collection::Map { .. } => unreachable!("Map iteration rejected above"),
        }

        result
    }

    fn evaluate(&mut self, expression: &Expression) -> Result<Value, Diagnostic> {
        match expression {
            Expression::StringLiteral(value, _) => Ok(Value::String(value.clone())),
            Expression::BooleanLiteral(value, _) => Ok(Value::Boolean(*value)),
            Expression::IntegerLiteral(value, _) => Ok(Value::Integer(*value)),
            Expression::NullLiteral(_) => Ok(Value::Null(None)),
            Expression::Variable(identifier) => {
                self.lookup(identifier).map(|slot| slot.value.clone())
            }
            Expression::Assignment { target, value, .. } => self.evaluate_assignment(target, value),
            Expression::NewCollection {
                ty,
                initializer,
                span,
            } => self.evaluate_new_collection(ty, initializer, *span),
            Expression::Index {
                collection,
                index,
                span,
            } => self.evaluate_index(collection, index, *span),
            Expression::MethodCall {
                receiver,
                method,
                arguments,
                span,
            } => self.evaluate_method_call(receiver, method, arguments, *span),
            Expression::Unary {
                operator,
                operand,
                operator_span,
                ..
            } => self.evaluate_unary(*operator, operand, *operator_span),
            Expression::Postfix {
                operand,
                operator,
                operator_span,
                ..
            } => self.evaluate_postfix(operand, *operator, *operator_span),
            Expression::Binary {
                left,
                operator,
                right,
                operator_span,
                ..
            } => self.evaluate_binary(left, *operator, right, *operator_span),
        }
    }

    fn evaluate_assignment(
        &mut self,
        target: &AssignmentTarget,
        value: &Expression,
    ) -> Result<Value, Diagnostic> {
        match target {
            AssignmentTarget::Variable(identifier) => {
                let value = self.evaluate(value)?;
                self.assign_variable(identifier, value)
            }
            AssignmentTarget::Index {
                collection,
                index,
                span,
            } => {
                let collection_value = self.evaluate(collection)?;
                let index_value = self.evaluate(index)?;
                let value = self.evaluate(value)?;
                self.assign_index(collection_value, index_value, value, index.span(), *span)
            }
        }
    }

    fn evaluate_new_collection(
        &mut self,
        ty: &TypeName,
        initializer: &CollectionInitializer,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match initializer {
            CollectionInitializer::Arguments(arguments) => {
                let arguments = self.evaluate_arguments(arguments)?;
                self.construct_with_arguments(ty, &arguments, span)
            }
            CollectionInitializer::Elements(elements) => {
                let values = self.evaluate_arguments(elements)?;
                self.construct_with_elements(ty, values, span)
            }
            CollectionInitializer::MapEntries(entries) => {
                let mut values = Vec::with_capacity(entries.len());
                for entry in entries {
                    let key = self.evaluate(&entry.key)?;
                    let value = self.evaluate(&entry.value)?;
                    values.push((key, value));
                }
                self.construct_map_entries(ty, values, span)
            }
            CollectionInitializer::SizedArray(size) => {
                let size_span = size.span();
                let value = self.evaluate(size)?;
                let Value::Integer(size_value) = value else {
                    return Err(Diagnostic::new(
                        "array size must be a non-null Integer",
                        size_span,
                    ));
                };
                if size_value < 0 {
                    return Err(Diagnostic::new("array size cannot be negative", size_span));
                }
                let TypeName::List(element_type) = ty else {
                    return Err(invalid_runtime_operands(span));
                };
                let size = usize::try_from(size_value)
                    .map_err(|_| Diagnostic::new("array size is too large", size_span))?;
                let mut elements = Vec::new();
                elements
                    .try_reserve_exact(size)
                    .map_err(|_| Diagnostic::new("array size is too large", size_span))?;
                elements.resize(size, Value::Null(Some((**element_type).clone())));
                Ok(self.allocate(Collection::List {
                    element_type: (**element_type).clone(),
                    elements,
                    iteration_depth: 0,
                }))
            }
        }
    }

    fn construct_with_arguments(
        &mut self,
        ty: &TypeName,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if arguments.is_empty() {
            return self.allocate_empty_collection(ty, span);
        }
        let [source] = arguments else {
            return Err(Diagnostic::new(
                "invalid collection constructor arguments escaped semantic validation",
                span,
            ));
        };
        let Value::Collection(source_id) = source.value else {
            if matches!(source.value, Value::Null(_)) {
                return Err(Diagnostic::new(
                    "cannot copy a null collection",
                    source.span,
                ));
            }
            return Err(invalid_runtime_operands(source.span));
        };

        match ty {
            TypeName::List(element_type) => {
                let source_elements = self.sequence_snapshot(source_id, source.span)?;
                let elements = source_elements
                    .into_iter()
                    .map(|value| typed_value(value, element_type))
                    .collect();
                Ok(self.allocate(Collection::List {
                    element_type: (**element_type).clone(),
                    elements,
                    iteration_depth: 0,
                }))
            }
            TypeName::Set(element_type) => {
                let source_elements = self.sequence_snapshot(source_id, source.span)?;
                let mut elements = Vec::new();
                for value in source_elements {
                    let value = typed_value(value, element_type);
                    if !elements
                        .iter()
                        .any(|existing| self.values_equal(existing, &value))
                    {
                        elements.push(value);
                    }
                }
                Ok(self.allocate(Collection::Set {
                    element_type: (**element_type).clone(),
                    elements,
                    iteration_depth: 0,
                }))
            }
            TypeName::Map(key_type, value_type) => {
                let Collection::Map { entries, .. } = self.collection(source_id) else {
                    return Err(invalid_runtime_operands(source.span));
                };
                let entries = entries.clone();
                Ok(self.allocate(Collection::Map {
                    key_type: (**key_type).clone(),
                    value_type: (**value_type).clone(),
                    entries,
                }))
            }
            _ => Err(Diagnostic::new(
                "primitive construction escaped semantic validation",
                span,
            )),
        }
    }

    fn allocate_empty_collection(
        &mut self,
        ty: &TypeName,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let collection = match ty {
            TypeName::List(element_type) => Collection::List {
                element_type: (**element_type).clone(),
                elements: Vec::new(),
                iteration_depth: 0,
            },
            TypeName::Set(element_type) => Collection::Set {
                element_type: (**element_type).clone(),
                elements: Vec::new(),
                iteration_depth: 0,
            },
            TypeName::Map(key_type, value_type) => Collection::Map {
                key_type: (**key_type).clone(),
                value_type: (**value_type).clone(),
                entries: Vec::new(),
            },
            _ => {
                return Err(Diagnostic::new(
                    "primitive construction escaped semantic validation",
                    span,
                ));
            }
        };
        Ok(self.allocate(collection))
    }

    fn construct_with_elements(
        &mut self,
        ty: &TypeName,
        values: Vec<EvaluatedArgument>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match ty {
            TypeName::List(element_type) => {
                let elements = values
                    .into_iter()
                    .map(|argument| typed_value(argument.value, element_type))
                    .collect();
                Ok(self.allocate(Collection::List {
                    element_type: (**element_type).clone(),
                    elements,
                    iteration_depth: 0,
                }))
            }
            TypeName::Set(element_type) => {
                let mut elements = Vec::new();
                for argument in values {
                    let value = typed_value(argument.value, element_type);
                    if !elements
                        .iter()
                        .any(|existing| self.values_equal(existing, &value))
                    {
                        elements.push(value);
                    }
                }
                Ok(self.allocate(Collection::Set {
                    element_type: (**element_type).clone(),
                    elements,
                    iteration_depth: 0,
                }))
            }
            _ => Err(Diagnostic::new(
                "element initializer requires List or Set",
                span,
            )),
        }
    }

    fn construct_map_entries(
        &mut self,
        ty: &TypeName,
        values: Vec<(Value, Value)>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let TypeName::Map(key_type, value_type) = ty else {
            return Err(Diagnostic::new("map entry initializer requires Map", span));
        };
        let mut entries: Vec<(Value, Value)> = Vec::new();
        for (key, value) in values {
            let key = typed_value(key, key_type);
            let value = typed_value(value, value_type);
            if let Some(index) = entries
                .iter()
                .position(|(existing, _)| self.values_equal(existing, &key))
            {
                entries[index] = (key, value);
            } else {
                entries.push((key, value));
            }
        }
        Ok(self.allocate(Collection::Map {
            key_type: (**key_type).clone(),
            value_type: (**value_type).clone(),
            entries,
        }))
    }

    fn evaluate_index(
        &mut self,
        collection: &Expression,
        index: &Expression,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let collection_value = self.evaluate(collection)?;
        let index_value = self.evaluate(index)?;
        let id = self.expect_collection_id(collection_value, collection.span())?;
        let index_span = index.span();
        let index = self.expect_index(index_value, index_span)?;
        let Collection::List { elements, .. } = self.collection(id) else {
            return Err(Diagnostic::new(
                "only List values support indexing at runtime",
                span,
            ));
        };
        let index = checked_list_index(index, elements.len(), false, index_span)?;
        Ok(elements[index].clone())
    }

    fn assign_index(
        &mut self,
        collection_value: Value,
        index_value: Value,
        value: Value,
        index_span: Span,
        target_span: Span,
    ) -> Result<Value, Diagnostic> {
        let id = self.expect_collection_id(collection_value, target_span)?;
        let index = self.expect_index(index_value, index_span)?;
        self.ensure_collection_mutable(id, target_span)?;
        let (element_type, size) = match self.collection(id) {
            Collection::List {
                element_type,
                elements,
                ..
            } => (element_type.clone(), elements.len()),
            _ => {
                return Err(Diagnostic::new(
                    "only List values support indexed assignment at runtime",
                    target_span,
                ));
            }
        };
        let index = checked_list_index(index, size, false, index_span)?;
        let value = typed_value(value, &element_type);
        let Collection::List { elements, .. } = self.collection_mut(id) else {
            unreachable!("List checked above")
        };
        elements[index] = value.clone();
        Ok(value)
    }

    fn evaluate_method_call(
        &mut self,
        receiver: &Expression,
        method: &Identifier,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let receiver_span = receiver.span();
        let receiver_value = if let Expression::Variable(identifier) = receiver {
            if let Some(slot) = self.lookup_canonical(&identifier.canonical) {
                CallReceiver::Value(slot.value.clone())
            } else if let Some(static_receiver) = static_receiver(identifier) {
                CallReceiver::Static(static_receiver)
            } else {
                return Err(unknown_variable(identifier));
            }
        } else {
            CallReceiver::Value(self.evaluate(receiver)?)
        };
        let arguments = self.evaluate_arguments(arguments)?;
        match receiver_value {
            CallReceiver::Static(receiver) => self.call_static(receiver, method, &arguments, span),
            CallReceiver::Value(receiver) => {
                self.call_instance(receiver, receiver_span, method, &arguments, span)
            }
        }
    }

    fn evaluate_arguments(
        &mut self,
        arguments: &[Expression],
    ) -> Result<Vec<EvaluatedArgument>, Diagnostic> {
        arguments
            .iter()
            .map(|argument| {
                Ok(EvaluatedArgument {
                    value: self.evaluate(argument)?,
                    span: argument.span(),
                })
            })
            .collect()
    }

    fn call_static(
        &mut self,
        receiver: StaticReceiver,
        method: &Identifier,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match receiver {
            StaticReceiver::String => self.call_static_string(method, arguments, span),
            StaticReceiver::Math => self.call_math(method, arguments, span),
            StaticReceiver::System => self.call_system(method, arguments, span),
        }
    }

    fn call_static_string(
        &mut self,
        method: &Identifier,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method.canonical.as_str() {
            "valueof" => {
                let [argument] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                if matches!(argument.value, Value::Void) {
                    return Err(Diagnostic::new(
                        "cannot convert void to String",
                        argument.span,
                    ));
                }
                Ok(Value::String(self.display_value(&argument.value)))
            }
            "join" => {
                let [iterable, separator] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let separator = expect_string(&separator.value, separator.span)?;
                let id = self.expect_collection_id(iterable.value.clone(), iterable.span)?;
                let elements = self.sequence_snapshot(id, iterable.span)?;
                let joined = elements
                    .iter()
                    .map(|value| self.display_value(value))
                    .collect::<Vec<_>>()
                    .join(separator);
                Ok(Value::String(joined))
            }
            "isblank" | "isnotblank" | "isempty" | "isnotempty" => {
                let [argument] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let (empty, blank) = match &argument.value {
                    Value::String(value) => (value.is_empty(), value.trim().is_empty()),
                    Value::Null(_) => (true, true),
                    _ => return Err(invalid_runtime_operands(argument.span)),
                };
                let value = match method.canonical.as_str() {
                    "isblank" => blank,
                    "isnotblank" => !blank,
                    "isempty" => empty,
                    "isnotempty" => !empty,
                    _ => unreachable!(),
                };
                Ok(Value::Boolean(value))
            }
            _ => Err(unsupported_method("String", method)),
        }
    }

    fn call_math(
        &mut self,
        method: &Identifier,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method.canonical.as_str() {
            "abs" => {
                let [argument] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                expect_integer(&argument.value, argument.span)?
                    .checked_abs()
                    .map(Value::Integer)
                    .ok_or_else(|| integer_overflow(span))
            }
            "max" | "min" => {
                let [left, right] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let left = expect_integer(&left.value, left.span)?;
                let right = expect_integer(&right.value, right.span)?;
                Ok(Value::Integer(if method.canonical == "max" {
                    left.max(right)
                } else {
                    left.min(right)
                }))
            }
            "mod" => {
                let [left, right] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let left = expect_integer(&left.value, left.span)?;
                let right_span = right.span;
                let right = expect_integer(&right.value, right_span)?;
                if right == 0 {
                    return Err(Diagnostic::new(
                        "Math.mod divisor cannot be zero",
                        right_span,
                    ));
                }
                left.checked_rem(right)
                    .map(Value::Integer)
                    .ok_or_else(|| integer_overflow(span))
            }
            _ => Err(unsupported_method("Math", method)),
        }
    }

    fn call_system(
        &mut self,
        method: &Identifier,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method.canonical.as_str() {
            "debug" => {
                let [argument] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                if matches!(argument.value, Value::Void) {
                    return Err(Diagnostic::new("cannot debug void", argument.span));
                }
                self.output.push(self.display_value(&argument.value));
                Ok(Value::Void)
            }
            _ => Err(unsupported_method("System", method)),
        }
    }

    fn call_instance(
        &mut self,
        receiver: Value,
        receiver_span: Span,
        method: &Identifier,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match receiver {
            Value::String(value) => self.call_string_instance(value, method, arguments, span),
            Value::Collection(id) => self.call_collection(id, method, arguments, span),
            Value::Null(_) => Err(Diagnostic::new(
                format!(
                    "attempt to de-reference a null value while calling `{}`",
                    method.spelling
                ),
                receiver_span,
            )),
            Value::Boolean(_) => Err(unsupported_method("Boolean", method)),
            Value::Integer(_) => Err(unsupported_method("Integer", method)),
            Value::Void => Err(Diagnostic::new(
                "cannot call a method on void",
                receiver_span,
            )),
        }
    }

    fn call_string_instance(
        &mut self,
        receiver: String,
        method: &Identifier,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method.canonical.as_str() {
            "length" => {
                let [] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let length = i64::try_from(receiver.encode_utf16().count())
                    .map_err(|_| Diagnostic::new("String length is too large", span))?;
                Ok(Value::Integer(length))
            }
            "contains" | "startswith" | "endswith" | "equals" | "equalsignorecase" | "indexof" => {
                let [argument] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                if matches!(method.canonical.as_str(), "equals" | "equalsignorecase")
                    && matches!(argument.value, Value::Null(_))
                {
                    return Ok(Value::Boolean(false));
                }
                let argument = expect_string(&argument.value, argument.span)?;
                match method.canonical.as_str() {
                    "contains" => Ok(Value::Boolean(receiver.contains(argument))),
                    "startswith" => Ok(Value::Boolean(receiver.starts_with(argument))),
                    "endswith" => Ok(Value::Boolean(receiver.ends_with(argument))),
                    "equals" => Ok(Value::Boolean(receiver == argument)),
                    "equalsignorecase" => Ok(Value::Boolean(
                        receiver.to_lowercase() == argument.to_lowercase(),
                    )),
                    "indexof" => {
                        let index = receiver.find(argument).map_or(-1, |byte_index| {
                            i64::try_from(receiver[..byte_index].encode_utf16().count())
                                .expect("String index fits in i64 when String length does")
                        });
                        Ok(Value::Integer(index))
                    }
                    _ => unreachable!(),
                }
            }
            "substring" => self.string_substring(&receiver, arguments, span),
            "trim" | "tolowercase" | "touppercase" => {
                let [] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value = match method.canonical.as_str() {
                    "trim" => receiver.trim().to_owned(),
                    "tolowercase" => receiver.to_lowercase(),
                    "touppercase" => receiver.to_uppercase(),
                    _ => unreachable!(),
                };
                Ok(Value::String(value))
            }
            "replace" => {
                let [target, replacement] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let target = expect_string(&target.value, target.span)?;
                let replacement = expect_string(&replacement.value, replacement.span)?;
                Ok(Value::String(receiver.replace(target, replacement)))
            }
            _ => Err(unsupported_method("String", method)),
        }
    }

    fn string_substring(
        &self,
        receiver: &str,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let utf16_length = receiver.encode_utf16().count();
        let (start, end, error_span) = match arguments {
            [start] => (
                nonnegative_usize(&start.value, start.span, "String index")?,
                utf16_length,
                start.span,
            ),
            [start, end] => (
                nonnegative_usize(&start.value, start.span, "String index")?,
                nonnegative_usize(&end.value, end.span, "String index")?,
                start.span.merge(end.span),
            ),
            _ => return Err(invalid_call_arguments(span)),
        };
        if start > end || end > utf16_length {
            return Err(Diagnostic::new(
                format!(
                    "String substring range {start}..{end} is out of bounds for length {utf16_length}"
                ),
                error_span,
            ));
        }
        let start_byte = utf16_byte_index(receiver, start).ok_or_else(|| {
            Diagnostic::new("String index splits a UTF-16 surrogate pair", error_span)
        })?;
        let end_byte = utf16_byte_index(receiver, end).ok_or_else(|| {
            Diagnostic::new("String index splits a UTF-16 surrogate pair", error_span)
        })?;
        Ok(Value::String(receiver[start_byte..end_byte].to_owned()))
    }

    fn call_collection(
        &mut self,
        id: CollectionId,
        method: &Identifier,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match self.collection(id) {
            Collection::List { .. } => self.call_list(id, method, arguments, span),
            Collection::Set { .. } => self.call_set(id, method, arguments, span),
            Collection::Map { .. } => self.call_map(id, method, arguments, span),
        }
    }

    fn call_list(
        &mut self,
        id: CollectionId,
        method: &Identifier,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method.canonical.as_str() {
            "add" => match arguments {
                [value] => {
                    self.ensure_collection_mutable(id, span)?;
                    let element_type = self.list_type(id).clone();
                    let value = typed_value(value.value.clone(), &element_type);
                    let Collection::List { elements, .. } = self.collection_mut(id) else {
                        unreachable!()
                    };
                    elements.push(value);
                    Ok(Value::Void)
                }
                [index, value] => {
                    self.ensure_collection_mutable(id, span)?;
                    let index_value = expect_integer(&index.value, index.span)?;
                    let (element_type, size) = match self.collection(id) {
                        Collection::List {
                            element_type,
                            elements,
                            ..
                        } => (element_type.clone(), elements.len()),
                        _ => unreachable!(),
                    };
                    let index = checked_list_index(index_value, size, true, index.span)?;
                    let value = typed_value(value.value.clone(), &element_type);
                    let Collection::List { elements, .. } = self.collection_mut(id) else {
                        unreachable!()
                    };
                    elements.insert(index, value);
                    Ok(Value::Void)
                }
                _ => Err(invalid_call_arguments(span)),
            },
            "addall" => {
                let [source] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source_id = self.expect_collection_id(source.value.clone(), source.span)?;
                let source_elements = self.sequence_snapshot(source_id, source.span)?;
                self.ensure_collection_mutable(id, span)?;
                let element_type = self.list_type(id).clone();
                let values: Vec<Value> = source_elements
                    .into_iter()
                    .map(|value| typed_value(value, &element_type))
                    .collect();
                let Collection::List { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                elements.extend(values);
                Ok(Value::Void)
            }
            "clear" => {
                expect_no_arguments(arguments, span)?;
                self.ensure_collection_mutable(id, span)?;
                let Collection::List { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                elements.clear();
                Ok(Value::Void)
            }
            "clone" => {
                expect_no_arguments(arguments, span)?;
                let (element_type, elements) = match self.collection(id) {
                    Collection::List {
                        element_type,
                        elements,
                        ..
                    } => (element_type.clone(), elements.clone()),
                    _ => unreachable!(),
                };
                Ok(self.allocate(Collection::List {
                    element_type,
                    elements,
                    iteration_depth: 0,
                }))
            }
            "contains" => {
                let [needle] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let Collection::List { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(
                    elements
                        .iter()
                        .any(|value| self.values_equal(value, &needle.value)),
                ))
            }
            "get" => {
                let [index] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.list_get(id, &index.value, index.span)
            }
            "indexof" => {
                let [needle] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let Collection::List { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                let index = elements
                    .iter()
                    .position(|value| self.values_equal(value, &needle.value))
                    .map_or(-1, |index| i64::try_from(index).unwrap_or(i64::MAX));
                Ok(Value::Integer(index))
            }
            "isempty" => {
                expect_no_arguments(arguments, span)?;
                let Collection::List { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(elements.is_empty()))
            }
            "remove" => {
                let [index] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.ensure_collection_mutable(id, span)?;
                let index_value = expect_integer(&index.value, index.span)?;
                let size = match self.collection(id) {
                    Collection::List { elements, .. } => elements.len(),
                    _ => unreachable!(),
                };
                let index = checked_list_index(index_value, size, false, index.span)?;
                let Collection::List { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                Ok(elements.remove(index))
            }
            "set" => {
                let [index, value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.ensure_collection_mutable(id, span)?;
                let index_value = expect_integer(&index.value, index.span)?;
                let (element_type, size) = match self.collection(id) {
                    Collection::List {
                        element_type,
                        elements,
                        ..
                    } => (element_type.clone(), elements.len()),
                    _ => unreachable!(),
                };
                let index = checked_list_index(index_value, size, false, index.span)?;
                let value = typed_value(value.value.clone(), &element_type);
                let Collection::List { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                elements[index] = value;
                Ok(Value::Void)
            }
            "size" => {
                expect_no_arguments(arguments, span)?;
                let Collection::List { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Integer(collection_size(elements.len(), span)?))
            }
            "sort" => {
                expect_no_arguments(arguments, span)?;
                self.ensure_collection_mutable(id, span)?;
                let mut elements = match self.collection(id) {
                    Collection::List { elements, .. } => elements.clone(),
                    _ => unreachable!(),
                };
                sort_primitive_values(&mut elements, span)?;
                let Collection::List {
                    elements: stored, ..
                } = self.collection_mut(id)
                else {
                    unreachable!()
                };
                *stored = elements;
                Ok(Value::Void)
            }
            _ => Err(unsupported_method("List", method)),
        }
    }

    fn call_set(
        &mut self,
        id: CollectionId,
        method: &Identifier,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method.canonical.as_str() {
            "add" => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.ensure_collection_mutable(id, span)?;
                let element_type = self.set_type(id).clone();
                let value = typed_value(value.value.clone(), &element_type);
                let changed = {
                    let Collection::Set { elements, .. } = self.collection(id) else {
                        unreachable!()
                    };
                    !elements
                        .iter()
                        .any(|existing| self.values_equal(existing, &value))
                };
                if changed {
                    let Collection::Set { elements, .. } = self.collection_mut(id) else {
                        unreachable!()
                    };
                    elements.push(value);
                }
                Ok(Value::Boolean(changed))
            }
            "addall" => {
                let [source] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source_id = self.expect_collection_id(source.value.clone(), source.span)?;
                let source_elements = self.sequence_snapshot(source_id, source.span)?;
                self.ensure_collection_mutable(id, span)?;
                let element_type = self.set_type(id).clone();
                let mut current = match self.collection(id) {
                    Collection::Set { elements, .. } => elements.clone(),
                    _ => unreachable!(),
                };
                let original_len = current.len();
                for value in source_elements {
                    let value = typed_value(value, &element_type);
                    if !current
                        .iter()
                        .any(|existing| self.values_equal(existing, &value))
                    {
                        current.push(value);
                    }
                }
                let changed = current.len() != original_len;
                let Collection::Set { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                *elements = current;
                Ok(Value::Boolean(changed))
            }
            "clear" => {
                expect_no_arguments(arguments, span)?;
                self.ensure_collection_mutable(id, span)?;
                let Collection::Set { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                elements.clear();
                Ok(Value::Void)
            }
            "clone" => {
                expect_no_arguments(arguments, span)?;
                let (element_type, elements) = match self.collection(id) {
                    Collection::Set {
                        element_type,
                        elements,
                        ..
                    } => (element_type.clone(), elements.clone()),
                    _ => unreachable!(),
                };
                Ok(self.allocate(Collection::Set {
                    element_type,
                    elements,
                    iteration_depth: 0,
                }))
            }
            "contains" => {
                let [needle] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let Collection::Set { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(
                    elements
                        .iter()
                        .any(|value| self.values_equal(value, &needle.value)),
                ))
            }
            "containsall" => {
                let [source] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source_id = self.expect_collection_id(source.value.clone(), source.span)?;
                let source = self.sequence_snapshot(source_id, source.span)?;
                let Collection::Set { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(source.iter().all(|needle| {
                    elements
                        .iter()
                        .any(|value| self.values_equal(value, needle))
                })))
            }
            "isempty" => {
                expect_no_arguments(arguments, span)?;
                let Collection::Set { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(elements.is_empty()))
            }
            "remove" => {
                let [needle] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.ensure_collection_mutable(id, span)?;
                let position = {
                    let Collection::Set { elements, .. } = self.collection(id) else {
                        unreachable!()
                    };
                    elements
                        .iter()
                        .position(|value| self.values_equal(value, &needle.value))
                };
                if let Some(position) = position {
                    let Collection::Set { elements, .. } = self.collection_mut(id) else {
                        unreachable!()
                    };
                    elements.remove(position);
                    Ok(Value::Boolean(true))
                } else {
                    Ok(Value::Boolean(false))
                }
            }
            "removeall" | "retainall" => {
                let [source] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source_id = self.expect_collection_id(source.value.clone(), source.span)?;
                let source = self.sequence_snapshot(source_id, source.span)?;
                self.ensure_collection_mutable(id, span)?;
                let current = match self.collection(id) {
                    Collection::Set { elements, .. } => elements.clone(),
                    _ => unreachable!(),
                };
                let retain_matches = method.canonical == "retainall";
                let retained: Vec<Value> = current
                    .iter()
                    .filter(|value| {
                        let found = source.iter().any(|needle| self.values_equal(value, needle));
                        found == retain_matches
                    })
                    .cloned()
                    .collect();
                let changed = retained.len() != current.len();
                let Collection::Set { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                *elements = retained;
                Ok(Value::Boolean(changed))
            }
            "size" => {
                expect_no_arguments(arguments, span)?;
                let Collection::Set { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Integer(collection_size(elements.len(), span)?))
            }
            _ => Err(unsupported_method("Set", method)),
        }
    }

    fn call_map(
        &mut self,
        id: CollectionId,
        method: &Identifier,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method.canonical.as_str() {
            "clear" => {
                expect_no_arguments(arguments, span)?;
                let Collection::Map { entries, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                entries.clear();
                Ok(Value::Void)
            }
            "clone" => {
                expect_no_arguments(arguments, span)?;
                let (key_type, value_type, entries) = match self.collection(id) {
                    Collection::Map {
                        key_type,
                        value_type,
                        entries,
                    } => (key_type.clone(), value_type.clone(), entries.clone()),
                    _ => unreachable!(),
                };
                Ok(self.allocate(Collection::Map {
                    key_type,
                    value_type,
                    entries,
                }))
            }
            "containskey" => {
                let [key] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                Ok(Value::Boolean(self.map_key_index(id, &key.value).is_some()))
            }
            "get" => {
                let [key] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let Collection::Map {
                    value_type,
                    entries,
                    ..
                } = self.collection(id)
                else {
                    unreachable!()
                };
                Ok(self
                    .map_key_index(id, &key.value)
                    .map(|index| entries[index].1.clone())
                    .unwrap_or_else(|| Value::Null(Some(value_type.clone()))))
            }
            "isempty" => {
                expect_no_arguments(arguments, span)?;
                let Collection::Map { entries, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(entries.is_empty()))
            }
            "keyset" => {
                expect_no_arguments(arguments, span)?;
                let (key_type, elements) = match self.collection(id) {
                    Collection::Map {
                        key_type, entries, ..
                    } => (
                        key_type.clone(),
                        entries.iter().map(|(key, _)| key.clone()).collect(),
                    ),
                    _ => unreachable!(),
                };
                Ok(self.allocate(Collection::Set {
                    element_type: key_type,
                    elements,
                    iteration_depth: 0,
                }))
            }
            "put" => {
                let [key, value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let (key_type, value_type) = match self.collection(id) {
                    Collection::Map {
                        key_type,
                        value_type,
                        ..
                    } => (key_type.clone(), value_type.clone()),
                    _ => unreachable!(),
                };
                let key = typed_value(key.value.clone(), &key_type);
                let value = typed_value(value.value.clone(), &value_type);
                Ok(self.map_put(id, key, value))
            }
            "putall" => {
                let [source] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source_id = self.expect_collection_id(source.value.clone(), source.span)?;
                let source_entries = match self.collection(source_id) {
                    Collection::Map { entries, .. } => entries.clone(),
                    _ => return Err(invalid_runtime_operands(source.span)),
                };
                let (key_type, value_type) = match self.collection(id) {
                    Collection::Map {
                        key_type,
                        value_type,
                        ..
                    } => (key_type.clone(), value_type.clone()),
                    _ => unreachable!(),
                };
                for (key, value) in source_entries {
                    self.map_put(
                        id,
                        typed_value(key, &key_type),
                        typed_value(value, &value_type),
                    );
                }
                Ok(Value::Void)
            }
            "remove" => {
                let [key] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value_type = match self.collection(id) {
                    Collection::Map { value_type, .. } => value_type.clone(),
                    _ => unreachable!(),
                };
                if let Some(index) = self.map_key_index(id, &key.value) {
                    let Collection::Map { entries, .. } = self.collection_mut(id) else {
                        unreachable!()
                    };
                    Ok(entries.remove(index).1)
                } else {
                    Ok(Value::Null(Some(value_type)))
                }
            }
            "size" => {
                expect_no_arguments(arguments, span)?;
                let Collection::Map { entries, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Integer(collection_size(entries.len(), span)?))
            }
            "values" => {
                expect_no_arguments(arguments, span)?;
                let (value_type, elements) = match self.collection(id) {
                    Collection::Map {
                        value_type,
                        entries,
                        ..
                    } => (
                        value_type.clone(),
                        entries.iter().map(|(_, value)| value.clone()).collect(),
                    ),
                    _ => unreachable!(),
                };
                Ok(self.allocate(Collection::List {
                    element_type: value_type,
                    elements,
                    iteration_depth: 0,
                }))
            }
            _ => Err(unsupported_method("Map", method)),
        }
    }

    fn map_put(&mut self, id: CollectionId, key: Value, value: Value) -> Value {
        if let Some(index) = self.map_key_index(id, &key) {
            let Collection::Map { entries, .. } = self.collection_mut(id) else {
                unreachable!()
            };
            let previous = entries[index].1.clone();
            entries[index] = (key, value);
            previous
        } else {
            let value_type = match self.collection(id) {
                Collection::Map { value_type, .. } => value_type.clone(),
                _ => unreachable!(),
            };
            let Collection::Map { entries, .. } = self.collection_mut(id) else {
                unreachable!()
            };
            entries.push((key, value));
            Value::Null(Some(value_type))
        }
    }

    fn map_key_index(&self, id: CollectionId, key: &Value) -> Option<usize> {
        let Collection::Map { entries, .. } = self.collection(id) else {
            return None;
        };
        entries
            .iter()
            .position(|(existing, _)| self.values_equal(existing, key))
    }

    fn list_get(&self, id: CollectionId, index: &Value, span: Span) -> Result<Value, Diagnostic> {
        let index = expect_integer(index, span)?;
        let Collection::List { elements, .. } = self.collection(id) else {
            return Err(invalid_runtime_operands(span));
        };
        let index = checked_list_index(index, elements.len(), false, span)?;
        Ok(elements[index].clone())
    }

    fn evaluate_unary(
        &mut self,
        operator: UnaryOperator,
        operand: &Expression,
        operator_span: Span,
    ) -> Result<Value, Diagnostic> {
        match operator {
            UnaryOperator::Positive => {
                let value = self.evaluate_integer(operand)?;
                Ok(Value::Integer(value))
            }
            UnaryOperator::Negate => {
                let value = self.evaluate_integer(operand)?;
                value
                    .checked_neg()
                    .map(Value::Integer)
                    .ok_or_else(|| integer_overflow(operator_span))
            }
            UnaryOperator::Not => {
                let value = self.evaluate_boolean(operand)?;
                Ok(Value::Boolean(!value))
            }
            UnaryOperator::PrefixIncrement => self.mutate_integer(operand, 1, false, operator_span),
            UnaryOperator::PrefixDecrement => {
                self.mutate_integer(operand, -1, false, operator_span)
            }
        }
    }

    fn evaluate_postfix(
        &mut self,
        operand: &Expression,
        operator: PostfixOperator,
        operator_span: Span,
    ) -> Result<Value, Diagnostic> {
        let delta = match operator {
            PostfixOperator::Increment => 1,
            PostfixOperator::Decrement => -1,
        };
        self.mutate_integer(operand, delta, true, operator_span)
    }

    fn evaluate_binary(
        &mut self,
        left: &Expression,
        operator: BinaryOperator,
        right: &Expression,
        operator_span: Span,
    ) -> Result<Value, Diagnostic> {
        if operator == BinaryOperator::And {
            let left = self.evaluate_boolean(left)?;
            return if left {
                Ok(Value::Boolean(self.evaluate_boolean(right)?))
            } else {
                Ok(Value::Boolean(false))
            };
        }
        if operator == BinaryOperator::Or {
            let left = self.evaluate_boolean(left)?;
            return if left {
                Ok(Value::Boolean(true))
            } else {
                Ok(Value::Boolean(self.evaluate_boolean(right)?))
            };
        }

        let left = self.evaluate(left)?;
        let right = self.evaluate(right)?;
        match operator {
            BinaryOperator::Add => {
                if left.has_string_type() || right.has_string_type() {
                    Ok(Value::String(
                        self.display_value(&left) + &self.display_value(&right),
                    ))
                } else {
                    match (&left, &right) {
                        (Value::Integer(left), Value::Integer(right)) => left
                            .checked_add(*right)
                            .map(Value::Integer)
                            .ok_or_else(|| integer_overflow(operator_span)),
                        (Value::Null(_), _) | (_, Value::Null(_)) => Err(Diagnostic::new(
                            "operator cannot be applied to null at runtime",
                            operator_span,
                        )),
                        _ => Err(invalid_runtime_operands(operator_span)),
                    }
                }
            }
            BinaryOperator::Subtract => {
                checked_integer_binary(left, right, operator_span, i64::checked_sub)
            }
            BinaryOperator::Multiply => {
                checked_integer_binary(left, right, operator_span, i64::checked_mul)
            }
            BinaryOperator::Divide => {
                let (left, right) = integer_pair(left, right, operator_span)?;
                if right == 0 {
                    return Err(Diagnostic::new("division by zero", operator_span));
                }
                left.checked_div(right)
                    .map(Value::Integer)
                    .ok_or_else(|| integer_overflow(operator_span))
            }
            BinaryOperator::Remainder => {
                let (left, right) = integer_pair(left, right, operator_span)?;
                if right == 0 {
                    return Err(Diagnostic::new("remainder by zero", operator_span));
                }
                left.checked_rem(right)
                    .map(Value::Integer)
                    .ok_or_else(|| integer_overflow(operator_span))
            }
            BinaryOperator::Less => compare_integers(left, right, operator_span, |a, b| a < b),
            BinaryOperator::LessEqual => {
                compare_integers(left, right, operator_span, |a, b| a <= b)
            }
            BinaryOperator::Greater => compare_integers(left, right, operator_span, |a, b| a > b),
            BinaryOperator::GreaterEqual => {
                compare_integers(left, right, operator_span, |a, b| a >= b)
            }
            BinaryOperator::Equal => Ok(Value::Boolean(self.operator_values_equal(&left, &right))),
            BinaryOperator::NotEqual => {
                Ok(Value::Boolean(!self.operator_values_equal(&left, &right)))
            }
            BinaryOperator::And | BinaryOperator::Or => unreachable!("handled above"),
        }
    }

    fn evaluate_boolean(&mut self, expression: &Expression) -> Result<bool, Diagnostic> {
        match self.evaluate(expression)? {
            Value::Boolean(value) => Ok(value),
            _ => Err(Diagnostic::new(
                "expected Boolean value at runtime",
                expression.span(),
            )),
        }
    }

    fn evaluate_integer(&mut self, expression: &Expression) -> Result<i64, Diagnostic> {
        match self.evaluate(expression)? {
            Value::Integer(value) => Ok(value),
            _ => Err(Diagnostic::new(
                "expected Integer value at runtime",
                expression.span(),
            )),
        }
    }

    fn mutate_integer(
        &mut self,
        operand: &Expression,
        delta: i64,
        return_old: bool,
        operator_span: Span,
    ) -> Result<Value, Diagnostic> {
        match operand {
            Expression::Variable(identifier) => {
                let old = match self.lookup(identifier)?.value {
                    Value::Integer(value) => value,
                    _ => {
                        return Err(Diagnostic::new(
                            "increment/decrement requires a non-null Integer value",
                            operator_span,
                        ));
                    }
                };
                let new = old
                    .checked_add(delta)
                    .ok_or_else(|| integer_overflow(operator_span))?;
                self.lookup_mut(identifier)?.value = Value::Integer(new);
                Ok(Value::Integer(if return_old { old } else { new }))
            }
            Expression::Index {
                collection,
                index,
                span,
            } => {
                let collection_value = self.evaluate(collection)?;
                let index_value = self.evaluate(index)?;
                let id = self.expect_collection_id(collection_value, collection.span())?;
                let index_value = self.expect_index(index_value, index.span())?;
                self.ensure_collection_mutable(id, *span)?;
                let (size, old) = match self.collection(id) {
                    Collection::List { elements, .. } => {
                        let index =
                            checked_list_index(index_value, elements.len(), false, index.span())?;
                        (elements.len(), (index, elements[index].clone()))
                    }
                    _ => return Err(invalid_runtime_operands(*span)),
                };
                let (index, old) = old;
                let Value::Integer(old) = old else {
                    return Err(Diagnostic::new(
                        "increment/decrement requires a non-null Integer value",
                        operator_span,
                    ));
                };
                let new = old
                    .checked_add(delta)
                    .ok_or_else(|| integer_overflow(operator_span))?;
                debug_assert!(index < size);
                let Collection::List { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                elements[index] = Value::Integer(new);
                Ok(Value::Integer(if return_old { old } else { new }))
            }
            _ => Err(Diagnostic::new(
                "increment/decrement operand must be an assignable value",
                operator_span,
            )),
        }
    }

    fn assign_variable(
        &mut self,
        identifier: &Identifier,
        value: Value,
    ) -> Result<Value, Diagnostic> {
        let ty = self.lookup(identifier)?.ty.clone();
        let value = typed_value(value, &ty);
        self.lookup_mut(identifier)?.value = value.clone();
        Ok(value)
    }

    fn expect_collection_id(&self, value: Value, span: Span) -> Result<CollectionId, Diagnostic> {
        match value {
            Value::Collection(id) => Ok(id),
            Value::Null(_) => Err(Diagnostic::new(
                "attempt to de-reference a null value",
                span,
            )),
            _ => Err(invalid_runtime_operands(span)),
        }
    }

    fn expect_index(&self, value: Value, span: Span) -> Result<i64, Diagnostic> {
        match value {
            Value::Integer(value) => Ok(value),
            Value::Null(_) => Err(Diagnostic::new(
                "list index must be a non-null Integer",
                span,
            )),
            _ => Err(invalid_runtime_operands(span)),
        }
    }

    fn ensure_collection_mutable(&self, id: CollectionId, span: Span) -> Result<(), Diagnostic> {
        let iteration_depth = match self.collection(id) {
            Collection::List {
                iteration_depth, ..
            }
            | Collection::Set {
                iteration_depth, ..
            } => *iteration_depth,
            Collection::Map { .. } => 0,
        };
        if iteration_depth == 0 {
            Ok(())
        } else {
            Err(Diagnostic::new(
                "cannot modify a collection while it is being iterated",
                span,
            ))
        }
    }

    fn sequence_snapshot(&self, id: CollectionId, span: Span) -> Result<Vec<Value>, Diagnostic> {
        match self.collection(id) {
            Collection::List { elements, .. } | Collection::Set { elements, .. } => {
                Ok(elements.clone())
            }
            Collection::Map { .. } => Err(Diagnostic::new("expected List or Set at runtime", span)),
        }
    }

    fn list_type(&self, id: CollectionId) -> &TypeName {
        let Collection::List { element_type, .. } = self.collection(id) else {
            unreachable!("List method called with another collection kind")
        };
        element_type
    }

    fn set_type(&self, id: CollectionId) -> &TypeName {
        let Collection::Set { element_type, .. } = self.collection(id) else {
            unreachable!("Set method called with another collection kind")
        };
        element_type
    }

    fn values_equal(&self, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::String(left), Value::String(right)) => left == right,
            (Value::Boolean(left), Value::Boolean(right)) => left == right,
            (Value::Integer(left), Value::Integer(right)) => left == right,
            (Value::Collection(left), Value::Collection(right)) => {
                self.collections_equal(*left, *right)
            }
            (Value::Null(_), Value::Null(_)) => true,
            (Value::Void, Value::Void) => true,
            _ => false,
        }
    }

    fn operator_values_equal(&self, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::String(left), Value::String(right)) => {
                left.to_lowercase() == right.to_lowercase()
            }
            _ => self.values_equal(left, right),
        }
    }

    fn collections_equal(&self, left: CollectionId, right: CollectionId) -> bool {
        if left == right {
            return true;
        }
        match (self.collection(left), self.collection(right)) {
            (
                Collection::List { elements: left, .. },
                Collection::List {
                    elements: right, ..
                },
            ) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(left, right)| self.values_equal(left, right))
            }
            (
                Collection::Set { elements: left, .. },
                Collection::Set {
                    elements: right, ..
                },
            ) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .all(|left| right.iter().any(|right| self.values_equal(left, right)))
            }
            (Collection::Map { entries: left, .. }, Collection::Map { entries: right, .. }) => {
                left.len() == right.len()
                    && left.iter().all(|(left_key, left_value)| {
                        right.iter().any(|(right_key, right_value)| {
                            self.values_equal(left_key, right_key)
                                && self.values_equal(left_value, right_value)
                        })
                    })
            }
            _ => false,
        }
    }

    fn display_value(&self, value: &Value) -> String {
        match value {
            Value::String(value) => value.clone(),
            Value::Boolean(value) => value.to_string(),
            Value::Integer(value) => value.to_string(),
            Value::Collection(id) => match self.collection(*id) {
                Collection::List { elements, .. } => format!(
                    "({})",
                    elements
                        .iter()
                        .map(|value| self.display_value(value))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Collection::Set { elements, .. } => format!(
                    "{{{}}}",
                    elements
                        .iter()
                        .map(|value| self.display_value(value))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Collection::Map { entries, .. } => format!(
                    "{{{}}}",
                    entries
                        .iter()
                        .map(|(key, value)| format!(
                            "{}={}",
                            self.display_value(key),
                            self.display_value(value)
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            },
            Value::Null(_) => "null".to_owned(),
            Value::Void => "void".to_owned(),
        }
    }

    fn allocate(&mut self, collection: Collection) -> Value {
        let id = CollectionId(self.collections.len());
        self.collections.push(collection);
        Value::Collection(id)
    }

    fn collection(&self, id: CollectionId) -> &Collection {
        self.collections
            .get(id.0)
            .expect("runtime collection handles are always valid")
    }

    fn collection_mut(&mut self, id: CollectionId) -> &mut Collection {
        self.collections
            .get_mut(id.0)
            .expect("runtime collection handles are always valid")
    }

    fn lookup(&self, identifier: &Identifier) -> Result<&Slot, Diagnostic> {
        self.lookup_canonical(&identifier.canonical)
            .ok_or_else(|| unknown_variable(identifier))
    }

    fn lookup_canonical(&self, canonical: &str) -> Option<&Slot> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(canonical))
    }

    fn lookup_mut(&mut self, identifier: &Identifier) -> Result<&mut Slot, Diagnostic> {
        self.scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.get_mut(&identifier.canonical))
            .ok_or_else(|| unknown_variable(identifier))
    }

    fn current_scope_mut(&mut self) -> &mut HashMap<String, Slot> {
        self.scopes
            .last_mut()
            .expect("interpreter always has a scope")
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

fn static_receiver(identifier: &Identifier) -> Option<StaticReceiver> {
    match identifier.canonical.as_str() {
        "string" => Some(StaticReceiver::String),
        "math" => Some(StaticReceiver::Math),
        "system" => Some(StaticReceiver::System),
        _ => None,
    }
}

fn checked_integer_binary(
    left: Value,
    right: Value,
    span: Span,
    operation: fn(i64, i64) -> Option<i64>,
) -> Result<Value, Diagnostic> {
    let (left, right) = integer_pair(left, right, span)?;
    operation(left, right)
        .map(Value::Integer)
        .ok_or_else(|| integer_overflow(span))
}

fn compare_integers(
    left: Value,
    right: Value,
    span: Span,
    comparison: impl FnOnce(i64, i64) -> bool,
) -> Result<Value, Diagnostic> {
    let (left, right) = integer_pair(left, right, span)?;
    Ok(Value::Boolean(comparison(left, right)))
}

fn integer_pair(left: Value, right: Value, span: Span) -> Result<(i64, i64), Diagnostic> {
    match (left, right) {
        (Value::Integer(left), Value::Integer(right)) => Ok((left, right)),
        (Value::Null(_), _) | (_, Value::Null(_)) => Err(Diagnostic::new(
            "operator cannot be applied to null at runtime",
            span,
        )),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn typed_value(value: Value, ty: &TypeName) -> Value {
    match value {
        Value::Null(_) => Value::Null(Some(ty.clone())),
        value => value,
    }
}

fn expect_integer(value: &Value, span: Span) -> Result<i64, Diagnostic> {
    match value {
        Value::Integer(value) => Ok(*value),
        Value::Null(_) => Err(Diagnostic::new(
            "expected non-null Integer at runtime",
            span,
        )),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn expect_string(value: &Value, span: Span) -> Result<&str, Diagnostic> {
    match value {
        Value::String(value) => Ok(value),
        Value::Null(_) => Err(Diagnostic::new("expected non-null String at runtime", span)),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn nonnegative_usize(value: &Value, span: Span, label: &str) -> Result<usize, Diagnostic> {
    let value = expect_integer(value, span)?;
    if value < 0 {
        return Err(Diagnostic::new(format!("{label} cannot be negative"), span));
    }
    usize::try_from(value).map_err(|_| Diagnostic::new(format!("{label} is too large"), span))
}

fn checked_list_index(
    index: i64,
    size: usize,
    allow_end: bool,
    span: Span,
) -> Result<usize, Diagnostic> {
    let converted = usize::try_from(index).ok();
    let valid = converted.is_some_and(|index| index < size || (allow_end && index == size));
    if valid {
        Ok(converted.expect("validated above"))
    } else {
        Err(Diagnostic::new(
            format!("list index {index} is out of bounds for size {size}"),
            span,
        ))
    }
}

fn collection_size(size: usize, span: Span) -> Result<i64, Diagnostic> {
    i64::try_from(size).map_err(|_| Diagnostic::new("collection size is too large", span))
}

fn sort_primitive_values(values: &mut [Value], span: Span) -> Result<(), Diagnostic> {
    if values
        .iter()
        .any(|value| !matches!(value, Value::String(_) | Value::Integer(_) | Value::Null(_)))
    {
        return Err(Diagnostic::new(
            "List.sort currently requires String or Integer values",
            span,
        ));
    }
    values.sort_by(|left, right| match (left, right) {
        (Value::Null(_), Value::Null(_)) => Ordering::Equal,
        (Value::Null(_), _) => Ordering::Less,
        (_, Value::Null(_)) => Ordering::Greater,
        (Value::String(left), Value::String(right)) => left.cmp(right),
        (Value::Integer(left), Value::Integer(right)) => left.cmp(right),
        _ => Ordering::Equal,
    });
    Ok(())
}

fn utf16_byte_index(value: &str, target: usize) -> Option<usize> {
    if target == 0 {
        return Some(0);
    }
    let mut units = 0;
    for (byte_index, character) in value.char_indices() {
        if units == target {
            return Some(byte_index);
        }
        units += character.len_utf16();
        if units > target {
            return None;
        }
    }
    (units == target).then_some(value.len())
}

fn expect_no_arguments(arguments: &[EvaluatedArgument], span: Span) -> Result<(), Diagnostic> {
    if arguments.is_empty() {
        Ok(())
    } else {
        Err(invalid_call_arguments(span))
    }
}

fn unknown_variable(identifier: &Identifier) -> Diagnostic {
    Diagnostic::new(
        format!("unknown variable `{}`", identifier.spelling),
        identifier.span,
    )
}

fn unsupported_method(receiver: &str, method: &Identifier) -> Diagnostic {
    Diagnostic::new(
        format!(
            "unsupported {receiver} method `{}` escaped semantic validation",
            method.spelling
        ),
        method.span,
    )
}

fn invalid_call_arguments(span: Span) -> Diagnostic {
    Diagnostic::new("invalid call arguments escaped semantic validation", span)
}

fn invalid_runtime_operands(span: Span) -> Diagnostic {
    Diagnostic::new("invalid operands escaped semantic validation", span)
}

fn integer_overflow(span: Span) -> Diagnostic {
    Diagnostic::new("integer overflow", span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn execute_source(source: &str) -> Result<Vec<String>, Diagnostic> {
        let program = crate::check(source)?;
        Interpreter::new().execute(&program)
    }

    #[test]
    fn typed_nulls_retain_static_string_behavior_and_compare_as_null() {
        let interpreter = Interpreter::new();
        let string_null = typed_value(Value::Null(None), &TypeName::String);
        let integer_null = typed_value(Value::Null(None), &TypeName::Integer);

        assert!(string_null.has_string_type());
        assert!(!integer_null.has_string_type());
        assert!(interpreter.values_equal(&string_null, &Value::Null(None)));
    }

    #[test]
    fn continue_in_a_for_loop_still_executes_the_update_clause() {
        let output = execute_source(
            "Integer i = 0; Integer total = 0; \
             for (; i < 4; i++) { if (i < 2) continue; total = total + i; } \
             System.debug(i); System.debug(total);",
        )
        .unwrap();

        assert_eq!(output, ["4", "5"]);
    }

    #[test]
    fn return_unwinds_nested_blocks_and_loops() {
        let output = execute_source(
            "System.debug('before'); while (true) { { return; } } System.debug('after');",
        )
        .unwrap();

        assert_eq!(output, ["before"]);
    }

    #[test]
    fn collection_assignment_aliases_while_copy_construction_is_independent() {
        let output = execute_source(
            "List<Integer> original = new List<Integer>{1}; \
             List<Integer> alias = original; alias.add(2); \
             List<Integer> copied = new List<Integer>(original); copied.set(0, 9); \
             System.debug(original); System.debug(alias); System.debug(copied);",
        )
        .unwrap();

        assert_eq!(output, ["(1, 2)", "(1, 2)", "(9, 2)"]);
    }

    #[test]
    fn sized_arrays_support_index_mutation_and_remain_elastic() {
        let output = execute_source(
            "Integer[] values = new Integer[2]; \
             values[0] = 3; values[1] = 4; values[0]++; values.add(5); \
             System.debug(values); System.debug(values.size());",
        )
        .unwrap();

        assert_eq!(output, ["(4, 4, 5)", "3"]);
    }

    #[test]
    fn set_and_map_methods_are_deterministic_and_return_previous_values() {
        let output = execute_source(
            "Set<String> names = new Set<String>{'Ada', 'Ada', 'Grace'}; \
             Boolean changed = names.add('Linus'); Boolean duplicate = names.add('Ada'); \
             Map<String, Integer> counts = new Map<String, Integer>{'a' => 1, 'a' => 2}; \
             Integer previous = counts.put('a', 3); Integer missing = counts.get('none'); \
             System.debug(names); System.debug(changed); System.debug(duplicate); \
             System.debug(counts); System.debug(previous); System.debug(missing);",
        )
        .unwrap();

        assert_eq!(
            output,
            ["{Ada, Grace, Linus}", "true", "false", "{a=3}", "2", "null"]
        );
    }

    #[test]
    fn enhanced_for_iterates_snapshots_but_rejects_alias_mutation() {
        let output = execute_source(
            "List<Integer> values = new List<Integer>{1, 2, 3}; Integer total = 0; \
             for (Integer value : values) { if (value == 2) continue; total = total + value; } \
             System.debug(total);",
        )
        .unwrap();
        assert_eq!(output, ["4"]);

        let error = execute_source(
            "List<Integer> values = new List<Integer>{1}; List<Integer> alias = values; \
             for (Integer value : values) alias.add(2);",
        )
        .unwrap_err();
        assert_eq!(
            error.message,
            "cannot modify a collection while it is being iterated"
        );
    }

    #[test]
    fn self_bulk_operations_use_source_snapshots() {
        let output = execute_source(
            "List<Integer> values = new List<Integer>{1, 2}; values.addAll(values); \
             Map<String, Integer> counts = new Map<String, Integer>{'a' => 1}; \
             counts.putAll(counts); System.debug(values); System.debug(counts);",
        )
        .unwrap();

        assert_eq!(output, ["(1, 2, 1, 2)", "{a=1}"]);
    }

    #[test]
    fn map_key_and_value_accessors_return_independent_snapshots() {
        let output = execute_source(
            "Map<String, Integer> source = new Map<String, Integer>{'a' => 1}; \
             Set<String> keys = source.keySet(); List<Integer> values = source.values(); \
             keys.add('b'); values.add(2); \
             System.debug(source.size()); System.debug(keys); System.debug(values);",
        )
        .unwrap();

        assert_eq!(output, ["1", "{a, b}", "(1, 2)"]);
    }

    #[test]
    fn string_math_and_system_calls_cover_utf16_indices() {
        let output = execute_source(
            "String emoji = 'A😀B'; \
             System.debug(emoji.length()); System.debug(emoji.substring(1, 3)); \
             System.debug(emoji.indexOf('B')); System.debug('  Ada  '.trim().toUpperCase()); \
             System.debug('Apex'.equals('Apex')); System.debug('Apex'.equalsIgnoreCase('aPeX')); \
             System.debug(String.join(new List<String>{'1', '2', '3'}, '-')); \
             System.debug(Math.abs(-4)); System.debug(Math.max(2, 5)); \
             System.debug(Math.min(2, 5)); System.debug(Math.mod(7, 3));",
        )
        .unwrap();

        assert_eq!(
            output,
            [
                "4", "😀", "3", "ADA", "true", "true", "1-2-3", "4", "5", "2", "1"
            ]
        );

        let error = execute_source("String value = '😀'; System.debug(value.substring(0, 1));")
            .unwrap_err();
        assert_eq!(error.message, "String index splits a UTF-16 surrogate pair");
    }

    #[test]
    fn reports_collection_bounds_null_and_negative_size_failures() {
        let bounds =
            execute_source("List<Integer> values = new List<Integer>{1}; System.debug(values[1]);")
                .unwrap_err();
        assert_eq!(bounds.message, "list index 1 is out of bounds for size 1");

        let null_receiver =
            execute_source("List<Integer> values = null; System.debug(values.size());")
                .unwrap_err();
        assert_eq!(
            null_receiver.message,
            "attempt to de-reference a null value while calling `size`"
        );

        let negative_size =
            execute_source("Integer size = -1; Integer[] values = new Integer[size];").unwrap_err();
        assert_eq!(negative_size.message, "array size cannot be negative");
    }
}
