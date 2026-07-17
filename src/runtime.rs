use crate::{
    ast::{
        AccessorKind, AssignmentTarget, BinaryOperator, CatchClause, ClassMember,
        CollectionInitializer, Expression, Identifier, Modifier, PostfixOperator, ReturnType,
        Statement, TypeName, UnaryOperator,
    },
    diagnostic::Diagnostic,
    hir::{CallTarget, ClassMemberId, MemberTarget, Program, ReferenceTarget},
    platform::FieldType,
    span::Span,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

mod host;
mod image;
mod intrinsics;
mod store;

pub use host::{DebugEvent, PlatformHost, RecordingHost};
use image::RuntimeImage;
use store::ExecutionStore;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BranchHits {
    pub true_hits: usize,
    pub false_hits: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExecutionTrace {
    pub executed_statements: BTreeSet<Span>,
    pub branches: BTreeMap<Span, BranchHits>,
}

#[derive(Clone, Debug)]
pub(crate) struct TestExecution {
    pub output: Vec<String>,
    pub diagnostic: Option<Diagnostic>,
    pub trace: ExecutionTrace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CollectionId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObjectId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SObjectId(usize);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    String(String),
    Boolean(bool),
    Integer(i64),
    Collection(CollectionId),
    Object(ObjectId),
    SObject(SObjectId),
    Exception(Box<Diagnostic>),
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
struct ObjectInstance {
    class_id: usize,
    fields: HashMap<ClassMemberId, Slot>,
}

#[derive(Clone, Debug)]
struct SObjectInstance {
    object_id: usize,
    fields: BTreeMap<usize, Value>,
}

#[derive(Clone, Debug)]
struct EvaluatedArgument {
    value: Value,
    span: Span,
}

#[derive(Clone, Debug)]
struct ActiveCall {
    method: String,
    call_span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Flow {
    Normal,
    Break,
    Continue,
    Return(Option<Value>),
}

/// One isolated Apex execution with a configurable platform host.
///
/// Entry points consume the interpreter and borrow one checked program for the
/// duration of that execution. The program lifetime is normally inferred by
/// [`Interpreter::execute`] or [`Interpreter::invoke_static`].
pub struct Interpreter<'program, H = RecordingHost> {
    scopes: Vec<HashMap<String, Slot>>,
    store: ExecutionStore,
    host: H,
    call_stack: Vec<ActiveCall>,
    image: Option<RuntimeImage<'program>>,
    current_receiver: Option<ObjectId>,
    current_declaring_class: Option<usize>,
    trace: ExecutionTrace,
}

impl<'program> Interpreter<'program, RecordingHost> {
    /// Creates an isolated interpreter with the default buffering debug host.
    pub fn new() -> Self {
        Self::with_host(RecordingHost::default())
    }
}

impl<'program, H: PlatformHost> Interpreter<'program, H> {
    /// Creates an isolated interpreter using a caller-provided platform host.
    ///
    /// Passing `&mut host` is supported when the caller needs to inspect host
    /// state after execution. Such a borrowed host may intentionally share
    /// external state across interpreter instances.
    pub fn with_host(host: H) -> Self {
        Self {
            scopes: vec![HashMap::new()],
            store: ExecutionStore::default(),
            host,
            call_stack: Vec::new(),
            image: None,
            current_receiver: None,
            current_declaring_class: None,
            trace: ExecutionTrace::default(),
        }
    }

    /// Executes the anonymous statements in a checked program.
    ///
    /// The returned lines are whatever the configured host drains through
    /// [`PlatformHost::take_debug_output`]. A streaming host that keeps the
    /// default implementation returns an empty vector.
    pub fn execute(mut self, program: &'program Program) -> Result<Vec<String>, Diagnostic> {
        self.prepare(program)?;
        for statement in &program.statements {
            match self.execute_statement(statement)? {
                Flow::Normal => {}
                Flow::Return(None) => break,
                Flow::Return(Some(_)) => {
                    return Err(Diagnostic::new(
                        "value return escaped semantic validation",
                        statement.span(),
                    ));
                }
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
        Ok(self.host.take_debug_output())
    }

    /// Invokes one public or global static zero-argument method.
    ///
    /// Class and method names are matched case-insensitively. The result
    /// contains drained host output followed by a deterministic rendering of a
    /// non-void return value.
    pub fn invoke_static(
        mut self,
        program: &'program Program,
        class_name: &str,
        method_name: &str,
    ) -> Result<Vec<String>, Diagnostic> {
        self.prepare(program)?;
        let class_id = self
            .classes()
            .iter()
            .position(|class| class.name.spelling.eq_ignore_ascii_case(class_name))
            .ok_or_else(|| {
                Diagnostic::new(format!("unknown class `{class_name}`"), Span::new(0, 0))
            })?;
        let candidates = self.classes()[class_id]
            .members
            .iter()
            .enumerate()
            .filter_map(|(member_id, member)| {
                let ClassMember::Method(method) = member else {
                    return None;
                };
                (method.name.spelling.eq_ignore_ascii_case(method_name)
                    && method.parameters.is_empty()
                    && method.modifiers.contains(&Modifier::Static)
                    && (method.modifiers.contains(&Modifier::Public)
                        || method.modifiers.contains(&Modifier::Global)))
                .then_some((member_id, method.name.span))
            })
            .collect::<Vec<_>>();
        let [(member_id, span)] = candidates.as_slice() else {
            return Err(Diagnostic::new(
                format!(
                    "invocation requires one public static zero-argument method `{class_name}.{method_name}`"
                ),
                self.classes()[class_id].name.span,
            ));
        };
        let value = self.evaluate_class_method(
            ClassMemberId {
                class_id,
                member_id: *member_id,
            },
            None,
            &[],
            *span,
            false,
        )?;
        let mut output = self.host.take_debug_output();
        if !matches!(value, Value::Void) {
            output.push(self.display_value(&value));
        }
        Ok(output)
    }

    pub(crate) fn run_test(
        mut self,
        program: &'program Program,
        setup_methods: &[ClassMemberId],
        test_method: ClassMemberId,
    ) -> TestExecution {
        let result = self.prepare(program).and_then(|_| {
            for setup_method in setup_methods {
                let span = self.class_method_span(*setup_method)?;
                self.evaluate_class_method(*setup_method, None, &[], span, false)?;
            }
            let span = self.class_method_span(test_method)?;
            self.evaluate_class_method(test_method, None, &[], span, false)?;
            Ok(())
        });
        TestExecution {
            output: self.host.take_debug_output(),
            diagnostic: result.err(),
            trace: self.trace,
        }
    }

    fn class_method_span(&self, target: ClassMemberId) -> Result<Span, Diagnostic> {
        match self
            .classes()
            .get(target.class_id)
            .and_then(|class| class.members.get(target.member_id))
        {
            Some(ClassMember::Method(method)) => Ok(method.name.span),
            _ => Err(Diagnostic::new(
                "test method target is invalid",
                Span::new(0, 0),
            )),
        }
    }

    fn prepare(&mut self, program: &'program Program) -> Result<(), Diagnostic> {
        self.image = Some(RuntimeImage::new(program));
        self.initialize_static_fields()
    }

    fn image(&self) -> RuntimeImage<'program> {
        self.image
            .expect("execution always has an immutable runtime image")
    }

    fn program(&self) -> &'program Program {
        self.image().program()
    }

    fn methods(&self) -> &'program [crate::ast::MethodDeclaration] {
        self.image().methods()
    }

    fn classes(&self) -> &'program [crate::ast::ClassDeclaration] {
        self.image().classes()
    }

    fn initialize_static_fields(&mut self) -> Result<(), Diagnostic> {
        for (class_id, class) in self.classes().iter().enumerate() {
            for (member_id, member) in class.members.iter().enumerate() {
                let target = ClassMemberId {
                    class_id,
                    member_id,
                };
                match member {
                    ClassMember::Field(field) if field.modifiers.contains(&Modifier::Static) => {
                        self.store.insert_static_slot(
                            target,
                            Slot {
                                ty: field.ty.clone(),
                                value: Value::Null(Some(field.ty.clone())),
                            },
                        );
                    }
                    ClassMember::Property(property)
                        if property.modifiers.contains(&Modifier::Static) =>
                    {
                        self.store.insert_static_slot(
                            target,
                            Slot {
                                ty: property.ty.clone(),
                                value: Value::Null(Some(property.ty.clone())),
                            },
                        );
                    }
                    _ => {}
                }
            }
        }

        let classes = self.classes();
        for (class_id, class) in classes.iter().enumerate() {
            self.current_declaring_class = Some(class_id);
            for (member_id, member) in class.members.iter().enumerate() {
                let ClassMember::Field(field) = member else {
                    continue;
                };
                if !field.modifiers.contains(&Modifier::Static) {
                    continue;
                }
                if let Some(initializer) = &field.initializer {
                    let target = ClassMemberId {
                        class_id,
                        member_id,
                    };
                    let value = typed_value(self.evaluate(initializer)?, &field.ty);
                    self.store
                        .static_slot_mut(&target)
                        .expect("static field was allocated")
                        .value = value;
                }
            }
        }
        self.current_declaring_class = None;
        Ok(())
    }

    fn execute_statement(&mut self, statement: &Statement) -> Result<Flow, Diagnostic> {
        self.trace.executed_statements.insert(statement.span());
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
                let outcome = self.evaluate_boolean(condition)?;
                self.record_branch(condition.span(), outcome);
                if outcome {
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
                loop {
                    let outcome = self.evaluate_boolean(condition)?;
                    self.record_branch(condition.span(), outcome);
                    if !outcome {
                        break;
                    }
                    match self.execute_statement(body)? {
                        Flow::Normal | Flow::Continue => {}
                        Flow::Break => break,
                        flow @ Flow::Return(_) => return Ok(flow),
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
                        flow @ Flow::Return(_) => return Ok(flow),
                    }
                    let outcome = self.evaluate_boolean(condition)?;
                    self.record_branch(condition.span(), outcome);
                    if !outcome {
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
            Statement::Return { value, .. } => {
                let value = value
                    .as_ref()
                    .map(|value| self.evaluate(value))
                    .transpose()?;
                Ok(Flow::Return(value))
            }
            Statement::Try {
                try_block,
                catches,
                finally_block,
                ..
            } => self.execute_try(try_block, catches, finally_block.as_deref()),
            Statement::Throw { value, span } => {
                let value = self.evaluate(value)?;
                match value {
                    Value::Exception(mut exception) => {
                        if exception.span == Span::new(0, 0) {
                            exception.span = *span;
                        }
                        Err(*exception)
                    }
                    Value::Null(_) => Err(runtime_exception(
                        "NullPointerException",
                        "attempt to throw null",
                        *span,
                    )),
                    _ => Err(Diagnostic::new(
                        "non-exception throw escaped semantic validation",
                        *span,
                    )),
                }
            }
        }
    }

    fn execute_try(
        &mut self,
        try_block: &Statement,
        catches: &[CatchClause],
        finally_block: Option<&Statement>,
    ) -> Result<Flow, Diagnostic> {
        let mut outcome = self.execute_statement(try_block);

        if let Err(mut exception) = outcome {
            self.attach_stack_if_missing(&mut exception);
            if exception.exception_type.is_some()
                && let Some(catch) = catches
                    .iter()
                    .find(|catch| exception_matches(&exception, &catch.exception_type))
            {
                self.scopes.push(HashMap::new());
                self.current_scope_mut().insert(
                    catch.name.canonical.clone(),
                    Slot {
                        ty: catch.exception_type.clone(),
                        value: Value::Exception(Box::new(exception)),
                    },
                );
                outcome = self.execute_statement(&catch.body);
                self.scopes.pop();
            } else {
                outcome = Err(exception);
            }
        }

        if let Some(finally_block) = finally_block {
            match self.execute_statement(finally_block) {
                Ok(Flow::Normal) => {}
                overriding => return overriding,
            }
        }

        outcome
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
                if let Some(condition) = condition {
                    let outcome = self.evaluate_boolean(condition)?;
                    self.record_branch(condition.span(), outcome);
                    if !outcome {
                        break;
                    }
                }
                match self.execute_statement(body)? {
                    Flow::Normal | Flow::Continue => {}
                    Flow::Break => break,
                    flow @ Flow::Return(_) => return Ok(flow),
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

    fn record_branch(&mut self, span: Span, outcome: bool) {
        let hits = self.trace.branches.entry(span).or_default();
        if outcome {
            hits.true_hits += 1;
        } else {
            hits.false_hits += 1;
        }
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
                return Err(runtime_exception(
                    "NullPointerException",
                    "cannot iterate over null",
                    iterable.span(),
                ));
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
                    flow @ Flow::Return(_) => return Ok(flow),
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
            Expression::Variable(identifier) => self.evaluate_variable(identifier),
            Expression::Assignment { target, value, .. } => self.evaluate_assignment(target, value),
            Expression::NewCollection {
                ty,
                initializer,
                span,
            } => self.evaluate_new_collection(ty, initializer, *span),
            Expression::NewException {
                exception_type,
                arguments,
                span,
            } => self.evaluate_new_exception(exception_type, arguments, *span),
            Expression::NewObject {
                arguments, span, ..
            } => self.evaluate_new_object(arguments, *span),
            Expression::FunctionCall {
                name,
                arguments,
                span,
            } => {
                let target = self.program().call_target(*span).ok_or_else(|| {
                    Diagnostic::new(
                        "unresolved method call escaped semantic validation",
                        name.span,
                    )
                })?;
                match target {
                    CallTarget::TopLevelMethod(method_id) => {
                        self.evaluate_function_call(method_id, name, arguments, *span)
                    }
                    CallTarget::StaticMethod(target) => {
                        self.evaluate_class_method(target, None, arguments, *span, false)
                    }
                    CallTarget::InstanceMethod(target) => {
                        let receiver = self.current_receiver.ok_or_else(|| {
                            Diagnostic::new("instance call has no current receiver", *span)
                        })?;
                        self.evaluate_class_method(target, Some(receiver), arguments, *span, true)
                    }
                    CallTarget::SuperMethod(target) => {
                        let receiver = self.current_receiver.ok_or_else(|| {
                            Diagnostic::new("super call has no current receiver", *span)
                        })?;
                        self.evaluate_class_method(target, Some(receiver), arguments, *span, false)
                    }
                    CallTarget::Intrinsic(_) => Err(Diagnostic::new(
                        "intrinsic target attached to a function call",
                        *span,
                    )),
                    CallTarget::Constructor { .. }
                    | CallTarget::SObjectConstructor { .. }
                    | CallTarget::SObjectGet
                    | CallTarget::SObjectPut => Err(Diagnostic::new(
                        "constructor target attached to a method call",
                        *span,
                    )),
                }
            }
            Expression::Cast {
                ty,
                expression,
                span,
            } => self.evaluate_cast(ty, expression, *span),
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
            } => {
                let target = self.program().call_target(*span);
                match target {
                    Some(CallTarget::Intrinsic(intrinsic)) => {
                        self.evaluate_intrinsic_call(intrinsic, receiver, method, arguments, *span)
                    }
                    Some(CallTarget::StaticMethod(target)) => {
                        self.evaluate_class_method(target, None, arguments, *span, false)
                    }
                    Some(CallTarget::InstanceMethod(target)) => {
                        let receiver = match self.evaluate(receiver)? {
                            Value::Object(receiver) => receiver,
                            Value::Null(_) => {
                                return Err(runtime_exception(
                                    "NullPointerException",
                                    "class method receiver is null",
                                    receiver.span(),
                                ));
                            }
                            _ => return Err(invalid_runtime_operands(receiver.span())),
                        };
                        self.evaluate_class_method(target, Some(receiver), arguments, *span, true)
                    }
                    Some(CallTarget::SuperMethod(target)) => {
                        let receiver = self.current_receiver.ok_or_else(|| {
                            Diagnostic::new("super call has no current receiver", *span)
                        })?;
                        self.evaluate_class_method(target, Some(receiver), arguments, *span, false)
                    }
                    Some(CallTarget::SObjectGet) => {
                        self.evaluate_sobject_get(receiver, arguments, *span)
                    }
                    Some(CallTarget::SObjectPut) => {
                        self.evaluate_sobject_put(receiver, arguments, *span)
                    }
                    Some(
                        CallTarget::TopLevelMethod(_)
                        | CallTarget::Constructor { .. }
                        | CallTarget::SObjectConstructor { .. },
                    ) => Err(Diagnostic::new(
                        "invalid checked target for member call",
                        *span,
                    )),
                    None => Err(Diagnostic::new(
                        "unresolved method call escaped semantic validation",
                        method.span,
                    )),
                }
            }
            Expression::MemberAccess {
                receiver,
                member: _,
                span,
            } => self.evaluate_member_access(receiver, *span),
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

    fn evaluate_variable(&mut self, identifier: &Identifier) -> Result<Value, Diagnostic> {
        let target = self
            .program()
            .reference_target(identifier.span)
            .ok_or_else(|| {
                Diagnostic::new(
                    "unresolved variable escaped semantic validation",
                    identifier.span,
                )
            })?;
        match target {
            ReferenceTarget::Local => self.lookup(identifier).map(|slot| slot.value.clone()),
            ReferenceTarget::This | ReferenceTarget::Super(_) => self
                .current_receiver
                .map(Value::Object)
                .ok_or_else(|| Diagnostic::new("missing instance receiver", identifier.span)),
            ReferenceTarget::InstanceMember(target) => {
                let receiver = self
                    .current_receiver
                    .ok_or_else(|| Diagnostic::new("missing instance receiver", identifier.span))?;
                self.read_class_member(target, Some(receiver), identifier.span)
            }
            ReferenceTarget::StaticMember(target) => {
                self.read_class_member(target, None, identifier.span)
            }
        }
    }

    fn evaluate_member_access(
        &mut self,
        receiver: &Expression,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let target = self.program().member_target(span).ok_or_else(|| {
            Diagnostic::new("unresolved member escaped semantic validation", span)
        })?;
        match target {
            MemberTarget::Static(target) => self.read_class_member(target, None, span),
            MemberTarget::Instance(target) => {
                let receiver = match self.evaluate(receiver)? {
                    Value::Object(receiver) => receiver,
                    Value::Null(_) => {
                        return Err(runtime_exception(
                            "NullPointerException",
                            "member access receiver is null",
                            receiver.span(),
                        ));
                    }
                    _ => return Err(invalid_runtime_operands(receiver.span())),
                };
                self.read_class_member(target, Some(receiver), span)
            }
            MemberTarget::SObjectField {
                object_id,
                field_id,
            } => {
                let receiver = self.evaluate_sobject_receiver(receiver)?;
                self.read_sobject_field(receiver, object_id, field_id, span)
            }
        }
    }

    fn evaluate_sobject_get(
        &mut self,
        receiver: &Expression,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let [field_name] = arguments else {
            return Err(Diagnostic::new("invalid checked SObject.get call", span));
        };
        let receiver = self.evaluate_sobject_receiver(receiver)?;
        let field_name = match self.evaluate(field_name)? {
            Value::String(value) => value,
            Value::Null(_) => {
                return Err(runtime_exception(
                    "IllegalArgumentException",
                    "SObject field name cannot be null",
                    field_name.span(),
                ));
            }
            _ => return Err(invalid_runtime_operands(field_name.span())),
        };
        let object_id = self.store.sobject(receiver).object_id;
        let object = self
            .program()
            .schema()
            .object_at(object_id)
            .expect("runtime SObject schema index is valid");
        let field_id = object.field_index(&field_name).ok_or_else(|| {
            runtime_exception(
                "IllegalArgumentException",
                format!(
                    "unknown field `{}` on SObject `{}`",
                    field_name,
                    object.api_name()
                ),
                span,
            )
        })?;
        self.read_sobject_field(receiver, object_id, field_id, span)
    }

    fn evaluate_sobject_put(
        &mut self,
        receiver: &Expression,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let [field_name, value] = arguments else {
            return Err(Diagnostic::new("invalid checked SObject.put call", span));
        };
        let receiver = self.evaluate_sobject_receiver(receiver)?;
        let field_name_value = self.evaluate(field_name)?;
        let value = self.evaluate(value)?;
        let Value::String(field_name) = field_name_value else {
            return Err(runtime_exception(
                "IllegalArgumentException",
                "SObject field name must be a non-null String",
                field_name.span(),
            ));
        };
        let object_id = self.store.sobject(receiver).object_id;
        let object = self
            .program()
            .schema()
            .object_at(object_id)
            .expect("runtime SObject schema index is valid");
        let field_id = object.field_index(&field_name).ok_or_else(|| {
            runtime_exception(
                "IllegalArgumentException",
                format!(
                    "unknown field `{}` on SObject `{}`",
                    field_name,
                    object.api_name()
                ),
                span,
            )
        })?;
        self.write_sobject_field(receiver, object_id, field_id, value, span)?;
        Ok(Value::Void)
    }

    fn evaluate_sobject_receiver(
        &mut self,
        receiver: &Expression,
    ) -> Result<SObjectId, Diagnostic> {
        match self.evaluate(receiver)? {
            Value::SObject(receiver) => Ok(receiver),
            Value::Null(_) => Err(runtime_exception(
                "NullPointerException",
                "SObject receiver is null",
                receiver.span(),
            )),
            _ => Err(invalid_runtime_operands(receiver.span())),
        }
    }

    fn read_sobject_field(
        &self,
        receiver: SObjectId,
        object_id: usize,
        field_id: usize,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let instance = self.store.sobject(receiver);
        if instance.object_id != object_id {
            return Err(Diagnostic::new(
                "SObject field target does not match runtime object type",
                span,
            ));
        }
        let field = self
            .program()
            .schema()
            .object_at(object_id)
            .and_then(|object| object.field_at(field_id))
            .expect("checked SObject field index is valid");
        Ok(instance
            .fields
            .get(&field_id)
            .cloned()
            .unwrap_or_else(|| Value::Null(Some(apex_field_type(field.data_type())))))
    }

    fn write_sobject_field(
        &mut self,
        receiver: SObjectId,
        object_id: usize,
        field_id: usize,
        value: Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let instance_object = self.store.sobject(receiver).object_id;
        if instance_object != object_id {
            return Err(Diagnostic::new(
                "SObject field target does not match runtime object type",
                span,
            ));
        }
        let field = self
            .program()
            .schema()
            .object_at(object_id)
            .and_then(|object| object.field_at(field_id))
            .expect("checked SObject field index is valid");
        let field_type = apex_field_type(field.data_type());
        if !self.value_has_type(&value, &field_type) {
            return Err(runtime_exception(
                "TypeException",
                format!(
                    "field `{}` expects {}, found {}",
                    field.api_name(),
                    field_type.apex_name(),
                    self.value_type_name(&value)
                ),
                span,
            ));
        }
        let value = typed_value(value, &field_type);
        self.store
            .sobject_mut(receiver)
            .fields
            .insert(field_id, value.clone());
        Ok(value)
    }

    fn read_class_member(
        &mut self,
        target: ClassMemberId,
        receiver: Option<ObjectId>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let member = self.classes()[target.class_id].members[target.member_id].clone();
        match member {
            ClassMember::Field(field) => {
                if field.modifiers.contains(&Modifier::Static) {
                    self.store
                        .static_slot(&target)
                        .map(|slot| slot.value.clone())
                        .ok_or_else(|| Diagnostic::new("missing static field storage", span))
                } else {
                    let receiver =
                        receiver.ok_or_else(|| Diagnostic::new("missing field receiver", span))?;
                    self.store
                        .object(receiver)
                        .fields
                        .get(&target)
                        .map(|slot| slot.value.clone())
                        .ok_or_else(|| Diagnostic::new("missing instance field storage", span))
                }
            }
            ClassMember::Property(property) => {
                let accessor = property
                    .accessors
                    .iter()
                    .find(|accessor| accessor.kind == AccessorKind::Get)
                    .cloned()
                    .ok_or_else(|| Diagnostic::new("property has no getter", span))?;
                if let Some(body) = accessor.body {
                    self.execute_property_getter(
                        target,
                        &property.name.spelling,
                        &property.ty,
                        receiver,
                        &body,
                        span,
                    )
                } else if property.modifiers.contains(&Modifier::Static) {
                    Ok(self
                        .store
                        .static_slot(&target)
                        .expect("auto property storage exists")
                        .value
                        .clone())
                } else {
                    let receiver = receiver
                        .ok_or_else(|| Diagnostic::new("missing property receiver", span))?;
                    Ok(self.store.object(receiver).fields[&target].value.clone())
                }
            }
            _ => Err(Diagnostic::new("target is not a value member", span)),
        }
    }

    fn write_class_member(
        &mut self,
        target: ClassMemberId,
        receiver: Option<ObjectId>,
        value: Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let member = self.classes()[target.class_id].members[target.member_id].clone();
        match member {
            ClassMember::Field(field) => {
                let value = typed_value(value, &field.ty);
                if field.modifiers.contains(&Modifier::Static) {
                    self.store
                        .static_slot_mut(&target)
                        .ok_or_else(|| Diagnostic::new("missing static field storage", span))?
                        .value = value.clone();
                } else {
                    let receiver =
                        receiver.ok_or_else(|| Diagnostic::new("missing field receiver", span))?;
                    self.store
                        .object_mut(receiver)
                        .fields
                        .get_mut(&target)
                        .ok_or_else(|| Diagnostic::new("missing instance field storage", span))?
                        .value = value.clone();
                }
                Ok(value)
            }
            ClassMember::Property(property) => {
                let value = typed_value(value, &property.ty);
                let accessor = property
                    .accessors
                    .iter()
                    .find(|accessor| accessor.kind == AccessorKind::Set)
                    .cloned()
                    .ok_or_else(|| Diagnostic::new("property has no setter", span))?;
                if let Some(body) = accessor.body {
                    self.execute_property_setter(
                        target,
                        &property.name.spelling,
                        &property.ty,
                        receiver,
                        &body,
                        value.clone(),
                        span,
                    )?;
                } else if property.modifiers.contains(&Modifier::Static) {
                    self.store
                        .static_slot_mut(&target)
                        .expect("auto property storage exists")
                        .value = value.clone();
                } else {
                    let receiver = receiver
                        .ok_or_else(|| Diagnostic::new("missing property receiver", span))?;
                    self.store
                        .object_mut(receiver)
                        .fields
                        .get_mut(&target)
                        .expect("auto property storage exists")
                        .value = value.clone();
                }
                Ok(value)
            }
            _ => Err(Diagnostic::new("target is not a value member", span)),
        }
    }

    fn evaluate_new_object(
        &mut self,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let target = self
            .program()
            .call_target(span)
            .ok_or_else(|| Diagnostic::new("unresolved constructor", span))?;
        if let CallTarget::SObjectConstructor { object_id } = target {
            let object_id = if let Some(object_id) = object_id {
                if !arguments.is_empty() {
                    return Err(Diagnostic::new(
                        "typed SObject constructor received unexpected arguments",
                        span,
                    ));
                }
                object_id
            } else {
                let [object_name] = arguments else {
                    return Err(Diagnostic::new(
                        "dynamic SObject constructor requires one API name",
                        span,
                    ));
                };
                let Value::String(object_name) = self.evaluate(object_name)? else {
                    return Err(invalid_runtime_operands(object_name.span()));
                };
                self.program()
                    .schema()
                    .object_index(&object_name)
                    .ok_or_else(|| {
                        runtime_exception(
                            "IllegalArgumentException",
                            format!("unknown SObject `{object_name}`"),
                            span,
                        )
                    })?
            };
            return Ok(self.store.allocate_sobject(object_id));
        }
        let CallTarget::Constructor {
            class_id,
            member_id,
        } = target
        else {
            return Err(Diagnostic::new("invalid constructor target", span));
        };
        let arguments = self.evaluate_arguments(arguments)?;
        let object_id = self.store.allocate_object(class_id);
        self.allocate_instance_fields(object_id, class_id);
        let lineage = self.class_lineage_base_first(class_id);
        for current in lineage {
            self.initialize_instance_fields(object_id, current)?;
            let selected = if current == class_id {
                member_id
            } else {
                self.zero_argument_constructor(current)
            };
            if let Some(member_id) = selected {
                let constructor = match self.classes()[current].members[member_id].clone() {
                    ClassMember::Constructor(constructor) => constructor,
                    _ => return Err(Diagnostic::new("invalid constructor member", span)),
                };
                let constructor_arguments = if current == class_id {
                    arguments.clone()
                } else {
                    Vec::new()
                };
                self.execute_callable(
                    &constructor.parameters,
                    &constructor.body,
                    &ReturnType::Void,
                    &constructor.name.spelling,
                    Some(object_id),
                    current,
                    constructor_arguments,
                    span,
                )?;
            }
        }
        Ok(Value::Object(object_id))
    }

    fn allocate_instance_fields(&mut self, object: ObjectId, class_id: usize) {
        for current in self.class_lineage_base_first(class_id) {
            for (member_id, member) in self.classes()[current].members.iter().enumerate() {
                let (ty, is_static) = match member {
                    ClassMember::Field(field) => {
                        (&field.ty, field.modifiers.contains(&Modifier::Static))
                    }
                    ClassMember::Property(property) => {
                        (&property.ty, property.modifiers.contains(&Modifier::Static))
                    }
                    _ => continue,
                };
                if !is_static {
                    self.store.object_mut(object).fields.insert(
                        ClassMemberId {
                            class_id: current,
                            member_id,
                        },
                        Slot {
                            ty: ty.clone(),
                            value: Value::Null(Some(ty.clone())),
                        },
                    );
                }
            }
        }
    }

    fn initialize_instance_fields(
        &mut self,
        object: ObjectId,
        class_id: usize,
    ) -> Result<(), Diagnostic> {
        let saved_receiver = self.current_receiver.replace(object);
        let saved_declaring = self.current_declaring_class.replace(class_id);
        let members = self.classes()[class_id].members.clone();
        let result = (|| {
            for (member_id, member) in members.iter().enumerate() {
                let ClassMember::Field(field) = member else {
                    continue;
                };
                if field.modifiers.contains(&Modifier::Static) {
                    continue;
                }
                if let Some(initializer) = &field.initializer {
                    let value = typed_value(self.evaluate(initializer)?, &field.ty);
                    self.store
                        .object_mut(object)
                        .fields
                        .get_mut(&ClassMemberId {
                            class_id,
                            member_id,
                        })
                        .expect("instance field was allocated")
                        .value = value;
                }
            }
            Ok(())
        })();
        self.current_receiver = saved_receiver;
        self.current_declaring_class = saved_declaring;
        result
    }

    fn class_lineage_base_first(&self, class_id: usize) -> Vec<usize> {
        let mut lineage = Vec::new();
        let mut cursor = Some(class_id);
        while let Some(id) = cursor {
            lineage.push(id);
            cursor = self.classes()[id].superclass.as_ref().and_then(|parent| {
                self.classes()
                    .iter()
                    .position(|class| class.name.canonical == parent.canonical)
            });
        }
        lineage.reverse();
        lineage
    }

    fn zero_argument_constructor(&self, class_id: usize) -> Option<usize> {
        self.classes()[class_id]
            .members
            .iter()
            .position(|member| matches!(member, ClassMember::Constructor(constructor) if constructor.parameters.is_empty()))
    }

    fn evaluate_new_exception(
        &mut self,
        exception_type: &TypeName,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let message = match arguments {
            [] => String::new(),
            [message] => match self.evaluate(message)? {
                Value::String(message) => message,
                Value::Null(_) => String::new(),
                _ => {
                    return Err(Diagnostic::new(
                        "invalid exception message escaped semantic validation",
                        message.span(),
                    ));
                }
            },
            _ => {
                return Err(Diagnostic::new(
                    "invalid exception constructor arity escaped semantic validation",
                    span,
                ));
            }
        };
        if !exception_type.is_exception() {
            return Err(Diagnostic::new(
                "non-exception construction escaped semantic validation",
                span,
            ));
        }
        Ok(Value::Exception(Box::new(Diagnostic::runtime_exception(
            exception_type.apex_name(),
            message,
            Span::new(0, 0),
        ))))
    }

    fn evaluate_class_method(
        &mut self,
        target: ClassMemberId,
        receiver: Option<ObjectId>,
        arguments: &[Expression],
        span: Span,
        virtual_dispatch: bool,
    ) -> Result<Value, Diagnostic> {
        let arguments = self.evaluate_arguments(arguments)?;
        let target = if virtual_dispatch {
            receiver
                .map(|receiver| self.virtual_method_target(receiver, target))
                .unwrap_or(target)
        } else {
            target
        };
        let method = match self.classes()[target.class_id].members[target.member_id].clone() {
            ClassMember::Method(method) => method,
            _ => return Err(Diagnostic::new("method target is invalid", span)),
        };
        let body = method
            .body
            .as_ref()
            .ok_or_else(|| Diagnostic::new("abstract method cannot execute", span))?;
        self.execute_callable(
            &method.parameters,
            body,
            &method.return_type,
            &method.name.spelling,
            receiver,
            target.class_id,
            arguments,
            span,
        )
    }

    fn virtual_method_target(&self, receiver: ObjectId, declared: ClassMemberId) -> ClassMemberId {
        let declared_method = match &self.classes()[declared.class_id].members[declared.member_id] {
            ClassMember::Method(method) => method,
            _ => return declared,
        };
        let parameter_types = declared_method
            .parameters
            .iter()
            .map(|parameter| parameter.ty.clone())
            .collect::<Vec<_>>();
        let mut cursor = Some(self.store.object(receiver).class_id);
        while let Some(class_id) = cursor {
            for (member_id, member) in self.classes()[class_id].members.iter().enumerate() {
                let ClassMember::Method(method) = member else {
                    continue;
                };
                if method.name.canonical == declared_method.name.canonical
                    && method
                        .parameters
                        .iter()
                        .map(|parameter| parameter.ty.clone())
                        .collect::<Vec<_>>()
                        == parameter_types
                    && method.body.is_some()
                {
                    return ClassMemberId {
                        class_id,
                        member_id,
                    };
                }
            }
            if class_id == declared.class_id {
                break;
            }
            cursor = self.classes()[class_id]
                .superclass
                .as_ref()
                .and_then(|parent| {
                    self.classes()
                        .iter()
                        .position(|class| class.name.canonical == parent.canonical)
                });
        }
        declared
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_callable(
        &mut self,
        parameters: &[crate::ast::Parameter],
        body: &Statement,
        return_type: &ReturnType,
        name: &str,
        receiver: Option<ObjectId>,
        declaring_class: usize,
        arguments: Vec<EvaluatedArgument>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let mut method_scope = HashMap::new();
        for (parameter, argument) in parameters.iter().zip(arguments) {
            method_scope.insert(
                parameter.name.canonical.clone(),
                Slot {
                    ty: parameter.ty.clone(),
                    value: typed_value(argument.value, &parameter.ty),
                },
            );
        }
        let caller_scopes = std::mem::replace(&mut self.scopes, vec![method_scope]);
        let saved_receiver = std::mem::replace(&mut self.current_receiver, receiver);
        let saved_declaring = self.current_declaring_class.replace(declaring_class);
        self.call_stack.push(ActiveCall {
            method: name.to_owned(),
            call_span: span,
        });
        let mut outcome = self.execute_statement(body);
        if let Err(exception) = &mut outcome {
            self.attach_stack_if_missing(exception);
        }
        self.call_stack.pop();
        self.scopes = caller_scopes;
        self.current_receiver = saved_receiver;
        self.current_declaring_class = saved_declaring;
        match outcome {
            Ok(Flow::Return(value)) => match (return_type, value) {
                (ReturnType::Void, None) => Ok(Value::Void),
                (ReturnType::Value(ty), Some(value)) => Ok(typed_value(value, ty)),
                _ => Err(Diagnostic::new(
                    "invalid callable return escaped semantic validation",
                    span,
                )),
            },
            Ok(Flow::Normal) if matches!(return_type, ReturnType::Void) => Ok(Value::Void),
            Ok(Flow::Normal) => Err(Diagnostic::new(
                "value-returning method completed without a return",
                span,
            )),
            Ok(Flow::Break | Flow::Continue) => Err(Diagnostic::new(
                "loop control escaped method semantic validation",
                span,
            )),
            Err(exception) => Err(exception),
        }
    }

    fn execute_property_getter(
        &mut self,
        target: ClassMemberId,
        name: &str,
        ty: &TypeName,
        receiver: Option<ObjectId>,
        body: &Statement,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.execute_callable(
            &[],
            body,
            &ReturnType::Value(ty.clone()),
            name,
            receiver,
            target.class_id,
            Vec::new(),
            span,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_property_setter(
        &mut self,
        target: ClassMemberId,
        name: &str,
        ty: &TypeName,
        receiver: Option<ObjectId>,
        body: &Statement,
        value: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let parameter = crate::ast::Parameter {
            ty: ty.clone(),
            name: Identifier::new("value".to_owned(), span),
            span,
        };
        self.execute_callable(
            &[parameter],
            body,
            &ReturnType::Void,
            name,
            receiver,
            target.class_id,
            vec![EvaluatedArgument { value, span }],
            span,
        )?;
        Ok(())
    }

    fn evaluate_function_call(
        &mut self,
        method_id: usize,
        name: &Identifier,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let arguments = self.evaluate_arguments(arguments)?;
        let method = self.methods().get(method_id).cloned().ok_or_else(|| {
            Diagnostic::new("resolved method does not exist at runtime", name.span)
        })?;
        let body = method
            .body
            .as_ref()
            .ok_or_else(|| Diagnostic::new("abstract method cannot execute", method.name.span))?;

        let mut method_scope = HashMap::new();
        for (parameter, argument) in method.parameters.iter().zip(arguments) {
            method_scope.insert(
                parameter.name.canonical.clone(),
                Slot {
                    ty: parameter.ty.clone(),
                    value: typed_value(argument.value, &parameter.ty),
                },
            );
        }

        let caller_scopes = std::mem::replace(&mut self.scopes, vec![method_scope]);
        self.call_stack.push(ActiveCall {
            method: method.name.spelling.clone(),
            call_span: span,
        });
        let mut outcome = self.execute_statement(body);
        if let Err(exception) = &mut outcome {
            self.attach_stack_if_missing(exception);
        }
        self.call_stack.pop();
        self.scopes = caller_scopes;

        match outcome {
            Ok(Flow::Return(value)) => match (&method.return_type, value) {
                (ReturnType::Void, None) => Ok(Value::Void),
                (ReturnType::Value(ty), Some(value)) => Ok(typed_value(value, ty)),
                _ => Err(Diagnostic::new(
                    "invalid method return escaped semantic validation",
                    method.name.span,
                )),
            },
            Ok(Flow::Normal) if matches!(method.return_type, ReturnType::Void) => Ok(Value::Void),
            Ok(Flow::Normal) => Err(Diagnostic::new(
                "value-returning method completed without a return",
                method.name.span,
            )),
            Ok(Flow::Break | Flow::Continue) => Err(Diagnostic::new(
                "loop control escaped method semantic validation",
                method.name.span,
            )),
            Err(exception) => Err(exception),
        }
    }

    fn evaluate_cast(
        &mut self,
        target: &TypeName,
        expression: &Expression,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let value = self.evaluate(expression)?;
        if matches!(value, Value::Null(_)) {
            return Ok(Value::Null(Some(target.clone())));
        }
        if self.value_has_type(&value, target) || matches!(target, TypeName::Object) {
            return Ok(value);
        }

        Err(runtime_exception(
            "TypeException",
            format!(
                "invalid conversion from runtime type {} to {}",
                self.value_type_name(&value),
                target.apex_name()
            ),
            span,
        ))
    }

    fn evaluate_assignment(
        &mut self,
        target: &AssignmentTarget,
        value: &Expression,
    ) -> Result<Value, Diagnostic> {
        match target {
            AssignmentTarget::Variable(identifier) => {
                let value = self.evaluate(value)?;
                let target = self
                    .program()
                    .reference_target(identifier.span)
                    .ok_or_else(|| {
                        Diagnostic::new("unresolved assignment target", identifier.span)
                    })?;
                match target {
                    ReferenceTarget::Local => self.assign_variable(identifier, value),
                    ReferenceTarget::InstanceMember(target) => {
                        let receiver = self.current_receiver.ok_or_else(|| {
                            Diagnostic::new("missing assignment receiver", identifier.span)
                        })?;
                        self.write_class_member(target, Some(receiver), value, identifier.span)
                    }
                    ReferenceTarget::StaticMember(target) => {
                        self.write_class_member(target, None, value, identifier.span)
                    }
                    ReferenceTarget::This | ReferenceTarget::Super(_) => Err(Diagnostic::new(
                        "cannot assign to this or super",
                        identifier.span,
                    )),
                }
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
            AssignmentTarget::Member {
                receiver,
                member: _,
                span,
            } => {
                let target = self
                    .program()
                    .member_target(*span)
                    .ok_or_else(|| Diagnostic::new("unresolved member assignment", *span))?;
                let value = self.evaluate(value)?;
                match target {
                    MemberTarget::Static(target) => {
                        self.write_class_member(target, None, value, *span)
                    }
                    MemberTarget::Instance(target) => {
                        let receiver = match self.evaluate(receiver)? {
                            Value::Object(receiver) => receiver,
                            Value::Null(_) => {
                                return Err(runtime_exception(
                                    "NullPointerException",
                                    "member assignment receiver is null",
                                    receiver.span(),
                                ));
                            }
                            _ => return Err(invalid_runtime_operands(receiver.span())),
                        };
                        self.write_class_member(target, Some(receiver), value, *span)
                    }
                    MemberTarget::SObjectField {
                        object_id,
                        field_id,
                    } => {
                        let receiver = self.evaluate_sobject_receiver(receiver)?;
                        self.write_sobject_field(receiver, object_id, field_id, value, *span)
                    }
                }
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
                    return Err(runtime_exception(
                        "NullPointerException",
                        "array size must be a non-null Integer",
                        size_span,
                    ));
                };
                if size_value < 0 {
                    return Err(runtime_exception(
                        "ListException",
                        "array size cannot be negative",
                        size_span,
                    ));
                }
                let TypeName::List(element_type) = ty else {
                    return Err(invalid_runtime_operands(span));
                };
                let size = usize::try_from(size_value).map_err(|_| {
                    runtime_exception("ListException", "array size is too large", size_span)
                })?;
                let mut elements = Vec::new();
                elements.try_reserve_exact(size).map_err(|_| {
                    runtime_exception("ListException", "array size is too large", size_span)
                })?;
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
                return Err(runtime_exception(
                    "NullPointerException",
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
                        (Value::Null(_), _) | (_, Value::Null(_)) => Err(runtime_exception(
                            "NullPointerException",
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
                    return Err(runtime_exception(
                        "MathException",
                        "division by zero",
                        operator_span,
                    ));
                }
                left.checked_div(right)
                    .map(Value::Integer)
                    .ok_or_else(|| integer_overflow(operator_span))
            }
            BinaryOperator::Remainder => {
                let (left, right) = integer_pair(left, right, operator_span)?;
                if right == 0 {
                    return Err(runtime_exception(
                        "MathException",
                        "remainder by zero",
                        operator_span,
                    ));
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
            Value::Null(_) => Err(runtime_exception(
                "NullPointerException",
                "expected non-null Boolean value at runtime",
                expression.span(),
            )),
            _ => Err(invalid_runtime_operands(expression.span())),
        }
    }

    fn evaluate_integer(&mut self, expression: &Expression) -> Result<i64, Diagnostic> {
        match self.evaluate(expression)? {
            Value::Integer(value) => Ok(value),
            Value::Null(_) => Err(runtime_exception(
                "NullPointerException",
                "expected non-null Integer value at runtime",
                expression.span(),
            )),
            _ => Err(invalid_runtime_operands(expression.span())),
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
                let target = self
                    .program()
                    .reference_target(identifier.span)
                    .ok_or_else(|| Diagnostic::new("unresolved increment target", operator_span))?;
                match target {
                    ReferenceTarget::Local => {
                        let old = match self.lookup(identifier)?.value {
                            Value::Integer(value) => value,
                            _ => {
                                return Err(runtime_exception(
                                    "NullPointerException",
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
                    ReferenceTarget::InstanceMember(target) => {
                        let receiver = self.current_receiver.ok_or_else(|| {
                            Diagnostic::new("missing increment receiver", operator_span)
                        })?;
                        self.mutate_class_member(
                            target,
                            Some(receiver),
                            delta,
                            return_old,
                            operator_span,
                        )
                    }
                    ReferenceTarget::StaticMember(target) => {
                        self.mutate_class_member(target, None, delta, return_old, operator_span)
                    }
                    ReferenceTarget::This | ReferenceTarget::Super(_) => {
                        Err(invalid_runtime_operands(operator_span))
                    }
                }
            }
            Expression::MemberAccess { receiver, span, .. } => {
                let target = self
                    .program()
                    .member_target(*span)
                    .ok_or_else(|| Diagnostic::new("unresolved increment target", *span))?;
                match target {
                    MemberTarget::Static(target) => {
                        self.mutate_class_member(target, None, delta, return_old, operator_span)
                    }
                    MemberTarget::Instance(target) => {
                        let receiver = match self.evaluate(receiver)? {
                            Value::Object(receiver) => receiver,
                            Value::Null(_) => {
                                return Err(runtime_exception(
                                    "NullPointerException",
                                    "increment receiver is null",
                                    receiver.span(),
                                ));
                            }
                            _ => return Err(invalid_runtime_operands(receiver.span())),
                        };
                        self.mutate_class_member(
                            target,
                            Some(receiver),
                            delta,
                            return_old,
                            operator_span,
                        )
                    }
                    MemberTarget::SObjectField {
                        object_id,
                        field_id,
                    } => {
                        let receiver = self.evaluate_sobject_receiver(receiver)?;
                        let old = match self.read_sobject_field(
                            receiver,
                            object_id,
                            field_id,
                            operator_span,
                        )? {
                            Value::Integer(value) => value,
                            Value::Null(_) => {
                                return Err(runtime_exception(
                                    "NullPointerException",
                                    "increment/decrement requires a non-null Integer value",
                                    operator_span,
                                ));
                            }
                            _ => return Err(invalid_runtime_operands(operator_span)),
                        };
                        let new = old
                            .checked_add(delta)
                            .ok_or_else(|| integer_overflow(operator_span))?;
                        self.write_sobject_field(
                            receiver,
                            object_id,
                            field_id,
                            Value::Integer(new),
                            operator_span,
                        )?;
                        Ok(Value::Integer(if return_old { old } else { new }))
                    }
                }
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
                    return Err(runtime_exception(
                        "NullPointerException",
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

    fn mutate_class_member(
        &mut self,
        target: ClassMemberId,
        receiver: Option<ObjectId>,
        delta: i64,
        return_old: bool,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let old = match self.read_class_member(target, receiver, span)? {
            Value::Integer(value) => value,
            _ => {
                return Err(runtime_exception(
                    "NullPointerException",
                    "increment/decrement requires a non-null Integer value",
                    span,
                ));
            }
        };
        let new = old
            .checked_add(delta)
            .ok_or_else(|| integer_overflow(span))?;
        self.write_class_member(target, receiver, Value::Integer(new), span)?;
        Ok(Value::Integer(if return_old { old } else { new }))
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
            Value::Null(_) => Err(runtime_exception(
                "NullPointerException",
                "attempt to de-reference a null value",
                span,
            )),
            _ => Err(invalid_runtime_operands(span)),
        }
    }

    fn expect_index(&self, value: Value, span: Span) -> Result<i64, Diagnostic> {
        match value {
            Value::Integer(value) => Ok(value),
            Value::Null(_) => Err(runtime_exception(
                "NullPointerException",
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
            Err(runtime_exception(
                "FinalException",
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
            (Value::Object(left), Value::Object(right)) => left == right,
            (Value::SObject(left), Value::SObject(right)) => left == right,
            (Value::Exception(left), Value::Exception(right)) => left == right,
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
            Value::Object(id) => {
                let instance = self.store.object(*id);
                format!(
                    "{}@{}",
                    self.classes()[instance.class_id].name.spelling,
                    id.0
                )
            }
            Value::SObject(id) => {
                let instance = self.store.sobject(*id);
                let object = self
                    .program()
                    .schema()
                    .object_at(instance.object_id)
                    .expect("runtime SObject schema index is valid");
                let fields = instance
                    .fields
                    .iter()
                    .map(|(field_id, value)| {
                        let field = object
                            .field_at(*field_id)
                            .expect("runtime SObject field index is valid");
                        format!("{}={}", field.api_name(), self.display_value(value))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}:{{{fields}}}", object.api_name())
            }
            Value::Exception(exception) => {
                let exception_type = exception.exception_type.as_deref().unwrap_or("Exception");
                if exception.message.is_empty() {
                    exception_type.to_owned()
                } else {
                    format!("{exception_type}: {}", exception.message)
                }
            }
            Value::Null(_) => "null".to_owned(),
            Value::Void => "void".to_owned(),
        }
    }

    fn allocate(&mut self, collection: Collection) -> Value {
        self.store.allocate_collection(collection)
    }

    fn collection(&self, id: CollectionId) -> &Collection {
        self.store.collection(id)
    }

    fn collection_mut(&mut self, id: CollectionId) -> &mut Collection {
        self.store.collection_mut(id)
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

    fn value_has_type(&self, value: &Value, target: &TypeName) -> bool {
        if matches!(target, TypeName::Object) {
            return !matches!(value, Value::Void);
        }
        match value {
            Value::String(_) => matches!(target, TypeName::String),
            Value::Boolean(_) => matches!(target, TypeName::Boolean),
            Value::Integer(_) => matches!(target, TypeName::Integer),
            Value::Collection(id) => self.collection_type(*id) == *target,
            Value::Object(id) => {
                let TypeName::Custom(target) = target else {
                    return false;
                };
                let target_id = self
                    .classes()
                    .iter()
                    .position(|class| class.name.canonical == target.canonical);
                target_id.is_some_and(|target_id| {
                    self.runtime_class_is_or_inherits(self.store.object(*id).class_id, target_id)
                })
            }
            Value::SObject(id) => {
                let TypeName::Custom(target) = target else {
                    return false;
                };
                let actual = self
                    .program()
                    .schema()
                    .object_at(self.store.sobject(*id).object_id)
                    .expect("runtime SObject schema index is valid");
                target.canonical == "sobject"
                    || actual.api_name().eq_ignore_ascii_case(&target.spelling)
            }
            Value::Exception(exception) => {
                matches!(target, TypeName::Exception)
                    || exception.exception_type.as_deref() == Some(target.apex_name().as_str())
            }
            Value::Null(ty) => ty.as_ref().is_none_or(|ty| ty == target),
            Value::Void => false,
        }
    }

    fn value_type_name(&self, value: &Value) -> String {
        match value {
            Value::String(_) => TypeName::String.apex_name(),
            Value::Boolean(_) => TypeName::Boolean.apex_name(),
            Value::Integer(_) => TypeName::Integer.apex_name(),
            Value::Collection(id) => self.collection_type(*id).apex_name(),
            Value::Object(id) => self.classes()[self.store.object(*id).class_id]
                .name
                .spelling
                .clone(),
            Value::SObject(id) => self
                .program()
                .schema()
                .object_at(self.store.sobject(*id).object_id)
                .expect("runtime SObject schema index is valid")
                .api_name()
                .to_owned(),
            Value::Exception(exception) => exception
                .exception_type
                .clone()
                .unwrap_or_else(|| "Exception".to_owned()),
            Value::Null(ty) => ty
                .as_ref()
                .map_or_else(|| "null".to_owned(), TypeName::apex_name),
            Value::Void => "void".to_owned(),
        }
    }

    fn runtime_class_is_or_inherits(&self, actual: usize, expected: usize) -> bool {
        if actual == expected {
            return true;
        }
        if self.classes()[actual].interfaces.iter().any(|interface| {
            self.classes()
                .iter()
                .position(|class| class.name.canonical == interface.canonical)
                .is_some_and(|id| self.runtime_class_is_or_inherits(id, expected))
        }) {
            return true;
        }
        self.classes()[actual]
            .superclass
            .as_ref()
            .and_then(|parent| {
                self.classes()
                    .iter()
                    .position(|class| class.name.canonical == parent.canonical)
            })
            .is_some_and(|parent| self.runtime_class_is_or_inherits(parent, expected))
    }

    fn collection_type(&self, id: CollectionId) -> TypeName {
        match self.collection(id) {
            Collection::List { element_type, .. } => TypeName::List(Box::new(element_type.clone())),
            Collection::Set { element_type, .. } => TypeName::Set(Box::new(element_type.clone())),
            Collection::Map {
                key_type,
                value_type,
                ..
            } => TypeName::Map(Box::new(key_type.clone()), Box::new(value_type.clone())),
        }
    }

    fn attach_stack_if_missing(&self, exception: &mut Diagnostic) {
        if exception.exception_type.is_none()
            || !exception.stack_trace.is_empty()
            || self.call_stack.is_empty()
        {
            return;
        }

        for index in (0..self.call_stack.len()).rev() {
            let span = if index + 1 == self.call_stack.len() {
                exception.span
            } else {
                self.call_stack[index + 1].call_span
            };
            exception.push_frame(self.call_stack[index].method.clone(), span);
        }
    }
}

impl<'program> Default for Interpreter<'program, RecordingHost> {
    fn default() -> Self {
        Self::new()
    }
}

fn runtime_exception(exception_type: &str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::runtime_exception(exception_type, message, span)
}

fn exception_matches(exception: &Diagnostic, catch_type: &TypeName) -> bool {
    let Some(exception_type) = exception.exception_type.as_deref() else {
        return false;
    };
    matches!(catch_type, TypeName::Exception)
        || exception_type.eq_ignore_ascii_case(&catch_type.apex_name())
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
        (Value::Null(_), _) | (_, Value::Null(_)) => Err(runtime_exception(
            "NullPointerException",
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

fn apex_field_type(field_type: &FieldType) -> TypeName {
    match field_type {
        FieldType::Boolean => TypeName::Boolean,
        FieldType::Integer => TypeName::Integer,
        FieldType::String | FieldType::Id | FieldType::Reference { .. } => TypeName::String,
    }
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
        Err(runtime_exception(
            "ListException",
            format!("list index {index} is out of bounds for size {size}"),
            span,
        ))
    }
}

fn unknown_variable(identifier: &Identifier) -> Diagnostic {
    Diagnostic::new(
        format!("unknown variable `{}`", identifier.spelling),
        identifier.span,
    )
}

fn invalid_runtime_operands(span: Span) -> Diagnostic {
    runtime_exception(
        "TypeException",
        "invalid operands escaped semantic validation",
        span,
    )
}

fn integer_overflow(span: Span) -> Diagnostic {
    runtime_exception("MathException", "integer overflow", span)
}

#[cfg(test)]
mod tests;
