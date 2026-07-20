use crate::{
    ast::{
        AccessorKind, AssignmentOperator, AssignmentTarget, BinaryOperator, CatchClause,
        ClassMember, CollectionInitializer, Expression, Identifier, Modifier, PostfixOperator,
        ReturnType, Statement, SwitchArm, SwitchLabels, TypeName, UnaryOperator,
    },
    diagnostic::Diagnostic,
    hir::{
        CallTarget, CheckedBinaryOperation, CheckedUnaryOperation, ClassId, ClassMemberId,
        CustomMetadataMethod, DatabaseDmlTarget, ExpressionType, FieldId, MemberTarget,
        NumericKind, ObjectTypeId, PlaceTarget, Program, ReferenceTarget, SchemaMemberTarget,
        SecurityDecisionMethod, TriggerContextVariable,
    },
    platform::{DmlError as PlatformDmlError, DmlRowOutcome, FieldType},
    span::Span,
};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use rust_decimal::Decimal;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
};

mod asynchronous;
mod class_initialization;
mod context;
mod database;
mod host;
mod image;
mod instrumentation;
mod intrinsics;
mod numeric;
mod platform_intrinsics;
mod security;
mod store;
mod value_graph;

use class_initialization::{ClassInitializationState, MAX_CLASS_INITIALIZATION_DEPTH};
use context::ExecutionContext;
pub use host::{
    AsyncEvent, AsyncJobKind, AsyncStage, DebugEvent, DmlEvent, HttpRequestData, HttpResponseData,
    LimitUsage, M11_ASYNC_PROFILE, PlatformHost, QueryEvent, QueryKind, RecordingHost,
    TransactionEvent, TriggerEvent as RuntimeTriggerEvent, TriggerPhase, TriggerStage, UserContext,
};
use image::RuntimeImage;
pub(crate) use instrumentation::{BranchHits, ExecutionTrace};
pub use instrumentation::{DebugFrame, DebugSnapshot, DebugTraceStatus, DebugVariable};
use instrumentation::{
    DebugSnapshotBuilder, InstrumentationPolicy, InstrumentationState, StatementInstrumentation,
};
use store::ExecutionStore;

#[derive(Clone, Copy, Debug)]
struct ApexDouble(f64);

impl ApexDouble {
    fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    fn get(self) -> f64 {
        self.0
    }
}

impl PartialEq for ApexDouble {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for ApexDouble {}

/// Complete deterministic trace from one debugger launch.
#[derive(Clone, Debug)]
pub struct DebugExecution {
    pub output: Vec<String>,
    pub diagnostic: Option<Diagnostic>,
    pub snapshots: Vec<DebugSnapshot>,
    pub timeline: Vec<TransactionEvent>,
    /// Retained-memory accounting and truncation state for `snapshots`.
    pub trace_status: DebugTraceStatus,
}

#[derive(Clone, Debug)]
pub(crate) struct TestExecution {
    pub output: Vec<String>,
    pub diagnostic: Option<Diagnostic>,
    pub trace: ExecutionTrace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct CollectionId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ObjectId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SObjectId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AggregateResultId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlatformValueId(usize);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    String(String),
    Boolean(bool),
    Integer(i64),
    Long(i64),
    Decimal(Decimal),
    Double(ApexDouble),
    Date(NaiveDate),
    Datetime(DateTime<Utc>),
    Time(NaiveTime),
    Id(String),
    Platform(PlatformValueId),
    Collection(CollectionId),
    Object(ObjectId),
    Enum { class_id: ClassId, ordinal: usize },
    TypeLiteral(TypeName),
    SObject(SObjectId),
    AggregateResult(AggregateResultId),
    Exception(Box<Diagnostic>),
    Null(Option<TypeName>),
    Void,
}

#[cfg(test)]
impl Value {
    fn has_string_type(&self) -> bool {
        matches!(self, Self::String(_) | Self::Null(Some(TypeName::String)))
    }
}

#[derive(Clone, Debug)]
enum PlatformValue {
    Blob(Vec<u8>),
    Pattern(String),
    Matcher {
        pattern: String,
        input: String,
        next_start: usize,
        captures: Vec<Option<(usize, usize)>>,
    },
    Http,
    HttpRequest(HttpRequestData),
    HttpResponse(HttpResponseData),
    VisualEditorDataRow {
        label: String,
        value: String,
    },
    VisualEditorDynamicPickListRows(Vec<PlatformValueId>),
    SObjectType(usize),
    DescribeSObject(usize),
    SObjectField {
        object_id: usize,
        field_id: usize,
    },
    DescribeField {
        object_id: usize,
        field_id: usize,
    },
    SObjectFieldMap(usize),
    FieldSetMap(usize),
    FieldSet {
        object_id: usize,
        name: String,
    },
    FieldSetMember {
        object_id: usize,
        field_id: usize,
    },
    PicklistEntry(String),
    AsyncContext {
        ty: TypeName,
        job_id: String,
    },
    QueryLocator(CollectionId),
    DmlResult {
        ty: TypeName,
        outcome: DmlRowOutcome,
    },
    DmlError(PlatformDmlError),
    DmlStatus(crate::platform::DmlStatus),
    AccessLevel(crate::platform::AccessLevel),
    AccessType(crate::platform::AccessType),
    PlatformEnum(crate::platform::PlatformEnum),
    CachePartition,
    Request {
        request_id: String,
        quiddity: crate::platform::Quiddity,
    },
    SecurityDecision {
        records: CollectionId,
        removed_fields: BTreeMap<String, Vec<String>>,
    },
}

impl PlatformValue {
    fn ty(&self) -> TypeName {
        match self {
            Self::Blob(_) => TypeName::Blob,
            Self::Pattern(_) => TypeName::Pattern,
            Self::Matcher { .. } => TypeName::Matcher,
            Self::Http => TypeName::Http,
            Self::HttpRequest(_) => TypeName::HttpRequest,
            Self::HttpResponse(_) => TypeName::HttpResponse,
            Self::VisualEditorDataRow { .. } => TypeName::VisualEditorDataRow,
            Self::VisualEditorDynamicPickListRows(_) => TypeName::VisualEditorDynamicPickListRows,
            Self::SObjectType(_) => TypeName::SObjectType,
            Self::DescribeSObject(_) => TypeName::DescribeSObjectResult,
            Self::SObjectField { .. } => TypeName::SObjectField,
            Self::DescribeField { .. } => TypeName::DescribeFieldResult,
            Self::SObjectFieldMap(_) => TypeName::SObjectFieldMap,
            Self::FieldSetMap(_) => TypeName::FieldSetMap,
            Self::FieldSet { .. } => TypeName::FieldSet,
            Self::FieldSetMember { .. } => TypeName::FieldSetMember,
            Self::PicklistEntry(_) => TypeName::PicklistEntry,
            Self::AsyncContext { ty, .. } => ty.clone(),
            Self::QueryLocator(_) => TypeName::QueryLocator,
            Self::DmlResult { ty, .. } => ty.clone(),
            Self::DmlError(_) => TypeName::DatabaseError,
            Self::DmlStatus(_) => TypeName::StatusCode,
            Self::AccessLevel(_) => TypeName::AccessLevel,
            Self::AccessType(_) => TypeName::AccessType,
            Self::PlatformEnum(value) => value.ty(),
            Self::CachePartition => TypeName::CachePartition,
            Self::Request { .. } => TypeName::Request,
            Self::SecurityDecision { .. } => TypeName::SObjectAccessDecision,
        }
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
    relationships: BTreeMap<usize, SObjectId>,
    children: BTreeMap<String, CollectionId>,
}

#[derive(Clone, Debug)]
struct EvaluatedArgument {
    value: Value,
    span: Span,
}

enum PlaceSyntax<'a> {
    Variable(&'a Identifier),
    Index {
        collection: &'a Expression,
        index: &'a Expression,
        span: Span,
    },
    Member {
        receiver: &'a Expression,
        span: Span,
    },
}

impl PlaceSyntax<'_> {
    fn span(&self) -> Span {
        match self {
            Self::Variable(identifier) => identifier.span,
            Self::Index { span, .. } | Self::Member { span, .. } => *span,
        }
    }
}

enum Place {
    Local(Identifier),
    ClassMember {
        target: ClassMemberId,
        receiver: Option<ObjectId>,
        property_storage: bool,
    },
    ListIndex {
        collection: CollectionId,
        index: usize,
    },
    SObjectField {
        receiver: SObjectId,
        object_id: usize,
        field_id: usize,
    },
}

#[derive(Clone, Debug)]
struct PendingAsyncJob {
    id: String,
    parent_id: Option<String>,
    kind: AsyncJobKind,
    work: AsyncWork,
    span: Span,
    execution_context: ExecutionContext,
    user_context: UserContext,
}

#[derive(Clone, Debug)]
enum AsyncWork {
    Queueable {
        receiver: ObjectId,
    },
    Future {
        target: ClassMemberId,
        arguments: Vec<EvaluatedArgument>,
    },
    Batch {
        receiver: ObjectId,
        scope_size: usize,
    },
    Scheduled {
        receiver: ObjectId,
    },
    PlatformEvent {
        records: Vec<SObjectId>,
    },
}

#[derive(Clone, Debug)]
struct CurrentAsync {
    id: String,
    kind: AsyncJobKind,
    allows_callouts: bool,
}

#[derive(Clone, Debug)]
struct ActiveCall {
    method: String,
    call_span: Span,
}

#[derive(Clone, Debug)]
struct TriggerContext {
    event: crate::ast::TriggerEvent,
    new_list: Value,
    old_list: Value,
    new_map: Value,
    old_map: Value,
    size: usize,
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
    trigger_context: Option<TriggerContext>,
    trigger_depth: usize,
    read_only_sobjects: BTreeSet<SObjectId>,
    read_only_collections: BTreeSet<CollectionId>,
    instrumentation: InstrumentationState,
    execution_context: ExecutionContext,
    initialization_stack: Vec<usize>,
    async_queue: VecDeque<PendingAsyncJob>,
    current_async: Option<CurrentAsync>,
    http_callout_mock: Option<ObjectId>,
    user_override: Option<UserContext>,
    next_async_sequence: u64,
}

impl<'program> Interpreter<'program, RecordingHost> {
    /// Creates an isolated interpreter with the default buffering debug host.
    pub fn new() -> Self {
        Self::with_host(RecordingHost::default())
    }

    /// Executes anonymous Apex while retaining statement-level debugger state.
    ///
    /// Execution remains deterministic and single-threaded. Protocol adapters
    /// can navigate the immutable snapshots without pausing the language
    /// runtime on an editor or transport thread.
    pub fn debug_execute(mut self, program: &'program Program) -> DebugExecution {
        self.execution_context = ExecutionContext::debugger();
        debug_assert!(self.execution_context.is_debug());
        self.instrumentation
            .configure(InstrumentationPolicy::Debugger);
        let result = self.execute_anonymous_entry(program);
        let (snapshots, trace_status) = self.instrumentation.take_debug_trace();
        DebugExecution {
            output: self.host.take_debug_output(),
            diagnostic: result.err(),
            snapshots,
            timeline: self.host.timeline_events().to_vec(),
            trace_status,
        }
    }

    /// Invokes a project entry point and records the same debugger snapshots as
    /// anonymous execution.
    pub fn debug_invoke(
        mut self,
        program: &'program Program,
        class_name: &str,
        method_name: &str,
    ) -> DebugExecution {
        self.execution_context = ExecutionContext::debugger();
        debug_assert!(self.execution_context.is_debug());
        self.instrumentation
            .configure(InstrumentationPolicy::Debugger);
        let result = self.invoke_static_entry(program, class_name, method_name);
        let mut output = self.host.take_debug_output();
        let diagnostic = match result {
            Ok(value) => {
                if !matches!(value, Value::Void) {
                    let rendered = self.render_value(&value);
                    self.instrumentation
                        .record_render_truncation(rendered.truncated);
                    output.push(rendered.text);
                }
                None
            }
            Err(diagnostic) => Some(diagnostic),
        };
        let (snapshots, trace_status) = self.instrumentation.take_debug_trace();
        DebugExecution {
            output,
            diagnostic,
            snapshots,
            timeline: self.host.timeline_events().to_vec(),
            trace_status,
        }
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
            trigger_context: None,
            trigger_depth: 0,
            read_only_sobjects: BTreeSet::new(),
            read_only_collections: BTreeSet::new(),
            instrumentation: InstrumentationState::new(InstrumentationPolicy::None),
            execution_context: ExecutionContext::ordinary(),
            initialization_stack: Vec::new(),
            async_queue: VecDeque::new(),
            current_async: None,
            http_callout_mock: None,
            user_override: None,
            next_async_sequence: 1,
        }
    }

    /// Executes the anonymous statements in a checked program.
    ///
    /// The returned lines are whatever the configured host drains through
    /// [`PlatformHost::take_debug_output`]. A streaming host that keeps the
    /// default implementation returns an empty vector.
    pub fn execute(mut self, program: &'program Program) -> Result<Vec<String>, Diagnostic> {
        self.execution_context = ExecutionContext::ordinary();
        self.instrumentation.configure(InstrumentationPolicy::None);
        self.execute_anonymous_entry(program)?;
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
        self.execution_context = ExecutionContext::ordinary();
        self.instrumentation.configure(InstrumentationPolicy::None);
        let value = self.invoke_static_entry(program, class_name, method_name)?;
        let mut output = self.host.take_debug_output();
        if !matches!(value, Value::Void) {
            output.push(self.stringify_value(&value));
        }
        Ok(output)
    }

    fn invoke_static_inner(
        &mut self,
        class_name: &str,
        method_name: &str,
    ) -> Result<Value, Diagnostic> {
        let class_id = self
            .classes()
            .iter()
            .position(|class| {
                class
                    .qualified_name
                    .spelling
                    .eq_ignore_ascii_case(class_name)
            })
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
        let saved_execution_context = self.execution_context;
        self.execution_context = self.execution_context.for_entry_class(
            self.program()
                .class_metadata(ClassId::from_index(class_id))
                .sharing,
        );
        let result = self.evaluate_class_method(
            ClassMemberId {
                class_id,
                member_id: *member_id,
            },
            None,
            &[],
            *span,
            false,
        );
        self.execution_context = saved_execution_context;
        result
    }

    pub(crate) fn run_test(
        mut self,
        program: &'program Program,
        setup_methods: &[ClassMemberId],
        test_method: ClassMemberId,
    ) -> TestExecution {
        self.execution_context = ExecutionContext::test();
        self.instrumentation
            .configure(InstrumentationPolicy::Coverage);
        let result = self.prepare(program).and_then(|_| {
            self.begin_transaction(Span::new(0, 0))?;
            let result = (|| {
                for setup_method in setup_methods {
                    let span = self.class_method_span(*setup_method)?;
                    self.evaluate_class_method(*setup_method, None, &[], span, false)?;
                }
                let span = self.class_method_span(test_method)?;
                self.evaluate_class_method(test_method, None, &[], span, false)?;
                Ok(())
            })();
            self.finish_transaction(result, Span::new(0, 0))
        });
        TestExecution {
            output: self.host.take_debug_output(),
            diagnostic: result.err(),
            trace: self.instrumentation.take_trace(),
        }
    }

    fn execute_anonymous_entry(&mut self, program: &'program Program) -> Result<(), Diagnostic> {
        self.prepare(program)?;
        self.begin_transaction(Span::new(0, 0))?;
        let result = (|| {
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
            Ok(())
        })();
        self.finish_transaction(result, Span::new(0, 0))
    }

    fn invoke_static_entry(
        &mut self,
        program: &'program Program,
        class_name: &str,
        method_name: &str,
    ) -> Result<Value, Diagnostic> {
        self.prepare(program)?;
        self.begin_transaction(Span::new(0, 0))?;
        let result = self.invoke_static_inner(class_name, method_name);
        self.finish_transaction(result, Span::new(0, 0))
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
        Ok(())
    }

    fn begin_transaction(&mut self, span: Span) -> Result<(), Diagnostic> {
        let schema = self.program().schema().clone();
        self.host
            .begin_unit(&schema)
            .map_err(|error| runtime_exception("DmlException", error.to_string(), span))
    }

    fn finish_transaction<T>(
        &mut self,
        result: Result<T, Diagnostic>,
        span: Span,
    ) -> Result<T, Diagnostic> {
        match result {
            Ok(value) => {
                self.host
                    .commit_unit()
                    .map_err(|error| runtime_exception("DmlException", error.to_string(), span))?;
                Ok(value)
            }
            Err(error) => {
                if let Err(rollback) = self.host.rollback_unit() {
                    return Err(runtime_exception(
                        "DmlException",
                        format!("{error}; transaction rollback failed: {rollback}"),
                        span,
                    ));
                }
                Err(error)
            }
        }
    }

    fn rollback_transaction(&mut self, span: Span) -> Result<(), Diagnostic> {
        self.host
            .rollback_unit()
            .map_err(|error| runtime_exception("DmlException", error.to_string(), span))
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

    fn ensure_class_initialized(
        &mut self,
        class_id: usize,
        use_span: Span,
    ) -> Result<(), Diagnostic> {
        for class_id in self.class_lineage_base_first(class_id) {
            self.initialize_one_class(class_id, use_span)?;
        }
        Ok(())
    }

    fn initialize_one_class(&mut self, class_id: usize, use_span: Span) -> Result<(), Diagnostic> {
        match self.store.class_initialization(class_id) {
            ClassInitializationState::Initialized => return Ok(()),
            ClassInitializationState::Failed(diagnostic) => return Err(diagnostic),
            ClassInitializationState::Initializing => {
                if self.initialization_stack.last() == Some(&class_id) {
                    return Ok(());
                }
                return Err(self.class_initialization_cycle(class_id, use_span));
            }
            ClassInitializationState::Uninitialized => {}
        }
        if self.initialization_stack.len() >= MAX_CLASS_INITIALIZATION_DEPTH {
            return Err(runtime_exception(
                "TypeException",
                format!(
                    "static initialization exceeds the depth limit of {MAX_CLASS_INITIALIZATION_DEPTH} classes"
                ),
                use_span,
            ));
        }

        self.store
            .set_class_initialization(class_id, ClassInitializationState::Initializing);
        self.initialization_stack.push(class_id);
        let saved_declaring = self.current_declaring_class.replace(class_id);
        let saved_receiver = self.current_receiver.take();
        let saved_execution_context = self.execution_context;
        self.execution_context = self.execution_context.for_class(
            self.program()
                .class_metadata(ClassId::from_index(class_id))
                .sharing,
        );
        let result = self.initialize_class_members(class_id);
        self.execution_context = saved_execution_context;
        self.current_receiver = saved_receiver;
        self.current_declaring_class = saved_declaring;
        let popped = self.initialization_stack.pop();
        debug_assert_eq!(popped, Some(class_id));

        self.store.set_class_initialization(
            class_id,
            match &result {
                Ok(()) => ClassInitializationState::Initialized,
                Err(diagnostic) => ClassInitializationState::Failed(diagnostic.clone()),
            },
        );
        result
    }

    fn initialize_class_members(&mut self, class_id: usize) -> Result<(), Diagnostic> {
        let metadata = self
            .program()
            .class_metadata(ClassId::from_index(class_id))
            .clone();
        for target in metadata.static_slots {
            let ty = match &self.classes()[class_id].members[target.member_id] {
                ClassMember::Field(field) => field.ty.clone(),
                ClassMember::Property(property) => property.ty.clone(),
                _ => unreachable!("static slot metadata refers to a value member"),
            };
            self.store.insert_static_slot(
                target,
                Slot {
                    value: Value::Null(Some(ty.clone())),
                    ty,
                },
            );
        }

        for step in metadata.static_steps {
            match step {
                crate::hir::ClassInitializationStep::Field(target) => {
                    let ClassMember::Field(field) =
                        self.classes()[class_id].members[target.member_id].clone()
                    else {
                        unreachable!("field initialization metadata refers to a field");
                    };
                    let initializer = field
                        .initializer
                        .as_ref()
                        .expect("field initialization metadata requires an initializer");
                    let value = typed_value(self.evaluate(initializer)?, &field.ty);
                    self.store
                        .static_slot_mut(&target)
                        .expect("static field was allocated")
                        .value = value;
                }
                crate::hir::ClassInitializationStep::Block(target) => {
                    let ClassMember::Initializer(initializer) =
                        self.classes()[class_id].members[target.member_id].clone()
                    else {
                        unreachable!("initializer metadata refers to a block");
                    };
                    if !matches!(self.execute_statement(&initializer.body)?, Flow::Normal) {
                        return Err(Diagnostic::new(
                            "static initializer completed abruptly",
                            initializer.span,
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn class_initialization_cycle(&self, class_id: usize, span: Span) -> Diagnostic {
        let cycle_start = self
            .initialization_stack
            .iter()
            .position(|active| *active == class_id)
            .unwrap_or(0);
        let mut names = self.initialization_stack[cycle_start..]
            .iter()
            .map(|active| self.classes()[*active].name.spelling.as_str())
            .collect::<Vec<_>>();
        names.push(self.classes()[class_id].name.spelling.as_str());
        runtime_exception(
            "TypeException",
            format!("circular static initialization: {}", names.join(" -> ")),
            span,
        )
    }

    fn execute_statement(&mut self, statement: &Statement) -> Result<Flow, Diagnostic> {
        self.instrument_statement(
            statement.span(),
            !matches!(
                statement,
                Statement::Block { .. } | Statement::Sequence { .. }
            ),
        );
        match statement {
            declaration @ Statement::VariableDeclaration { .. }
            | declaration @ Statement::LocalDeclaration { .. }
            | declaration @ Statement::Sequence { .. } => {
                self.execute_declaration_statement(declaration)
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
            Statement::Switch { value, arms, .. } => self.execute_switch(value, arms),
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
            Statement::RunAs { user, body, span } => self.execute_run_as(user, body, *span),
            Statement::Dml { .. } => self.execute_dml_statement(statement),
        }
    }

    fn execute_declaration_statement(&mut self, statement: &Statement) -> Result<Flow, Diagnostic> {
        match statement {
            Statement::VariableDeclaration {
                ty,
                name,
                initializer,
                ..
            } => {
                let value = typed_value(self.evaluate(initializer)?, ty);
                self.bind_local(name, ty, value);
            }
            Statement::LocalDeclaration {
                ty, declarators, ..
            } => {
                for declarator in declarators {
                    let value = declarator.initializer.as_ref().map_or_else(
                        || Ok(Value::Null(Some(ty.clone()))),
                        |initializer| {
                            self.evaluate(initializer)
                                .map(|value| typed_value(value, ty))
                        },
                    )?;
                    self.bind_local(&declarator.name, ty, value);
                }
            }
            Statement::Sequence { statements, .. } => {
                for statement in statements {
                    let flow = self.execute_statement(statement)?;
                    if flow != Flow::Normal {
                        return Ok(flow);
                    }
                }
            }
            _ => unreachable!("caller selects declarations and sequences"),
        }
        Ok(Flow::Normal)
    }

    fn bind_local(&mut self, name: &Identifier, ty: &TypeName, value: Value) {
        self.current_scope_mut().insert(
            name.canonical.clone(),
            Slot {
                ty: ty.clone(),
                value,
            },
        );
    }

    fn execute_dml_statement(&mut self, statement: &Statement) -> Result<Flow, Diagnostic> {
        let Statement::Dml { value, span, .. } = statement else {
            unreachable!("caller selects DML statements");
        };
        let Some(CallTarget::DatabaseDml(target)) = self.program().call_target(*span) else {
            return Err(Diagnostic::new(
                "DML statement is missing its checked target",
                *span,
            ));
        };
        self.execute_dml(target, value, *span)?;
        Ok(Flow::Normal)
    }

    fn execute_run_as(
        &mut self,
        user: &Expression,
        body: &Statement,
        span: Span,
    ) -> Result<Flow, Diagnostic> {
        if !self.execution_context.is_test() {
            return Err(runtime_exception(
                "IllegalArgumentException",
                "System.runAs is only valid in a test context",
                span,
            ));
        }
        let Value::SObject(user_handle) = self.evaluate(user)? else {
            return Err(invalid_runtime_operands(user.span()));
        };
        let instance = self.store.sobject(user_handle);
        let object = self
            .program()
            .schema()
            .object_at(instance.object_id)
            .expect("runtime SObject type belongs to its schema");
        if !object.api_name().eq_ignore_ascii_case("User") {
            return Err(Diagnostic::new(
                "System.runAs requires a User SObject",
                user.span(),
            ));
        }
        let id_field = object.field_index("Id").expect("User schema contains Id");
        let username_field = object
            .field_index("Username")
            .expect("User schema contains Username");
        let user_id = match instance.fields.get(&id_field) {
            Some(Value::Id(value) | Value::String(value)) => value.clone(),
            _ => {
                return Err(runtime_exception(
                    "IllegalArgumentException",
                    "System.runAs requires an inserted User",
                    span,
                ));
            }
        };
        let username = match instance.fields.get(&username_field) {
            Some(Value::String(value)) => value.clone(),
            _ => format!("{user_id}@local.invalid"),
        };
        let saved = self
            .user_override
            .replace(UserContext { user_id, username });
        let result = self.execute_statement(body);
        self.user_override = saved;
        result
    }

    pub(super) fn current_user_context(&self) -> UserContext {
        self.user_override
            .clone()
            .unwrap_or_else(|| self.host.user_context())
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
                    .find(|catch| self.exception_matches(&exception, &catch.exception_type))
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

    fn execute_switch(
        &mut self,
        expression: &Expression,
        arms: &[SwitchArm],
    ) -> Result<Flow, Diagnostic> {
        let value = self.evaluate(expression)?;
        for arm in arms {
            match &arm.labels {
                SwitchLabels::TypePattern {
                    ty,
                    binding,
                    span: pattern_span,
                } => {
                    let Some(expected_object) = self.program().switch_pattern(*pattern_span) else {
                        return Err(Diagnostic::new(
                            "SObject switch pattern is missing its checked target",
                            *pattern_span,
                        ));
                    };
                    let matched = matches!(
                        &value,
                        Value::SObject(id)
                            if self.store.sobject(*id).object_id == expected_object.index()
                    );
                    self.record_branch(*pattern_span, matched);
                    if matched {
                        self.scopes.push(HashMap::new());
                        self.bind_local(binding, ty, typed_value(value.clone(), ty));
                        let result = self.execute_statement(&arm.body);
                        self.scopes.pop();
                        return result;
                    }
                }
                SwitchLabels::Else(_) => return self.execute_statement(&arm.body),
                SwitchLabels::Expressions(labels) => {
                    for label in labels {
                        let label_value = self.evaluate(label)?;
                        let matched = self.values_equal(&value, &label_value);
                        self.record_branch(label.span(), matched);
                        if matched {
                            return self.execute_statement(&arm.body);
                        }
                    }
                }
            }
        }
        Ok(Flow::Normal)
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
        self.instrumentation.record_branch(span, outcome);
    }

    fn instrument_statement(&mut self, span: Span, capture_debugger: bool) {
        let StatementInstrumentation::CaptureDebugger {
            retained_byte_budget,
        } = self
            .instrumentation
            .before_statement(span, capture_debugger)
        else {
            return;
        };
        let (snapshot, retained_bytes, truncated) =
            self.build_debug_snapshot(span, retained_byte_budget);
        self.instrumentation
            .record_debug_snapshot(snapshot, retained_bytes, truncated);
    }

    fn build_debug_snapshot(
        &self,
        span: Span,
        retained_byte_budget: usize,
    ) -> (DebugSnapshot, usize, bool) {
        let mut builder = DebugSnapshotBuilder::new(
            span,
            self.host.transaction_event_count(),
            retained_byte_budget,
        );
        self.capture_debug_frames(span, &mut builder);
        self.capture_debug_variables(&mut builder);
        builder.finish()
    }

    fn capture_debug_frames(&self, span: Span, builder: &mut DebugSnapshotBuilder) {
        let leaf_name = self
            .call_stack
            .last()
            .map_or("<anonymous>", |call| call.method.as_str());
        if !builder.can_push_frame() {
            builder.mark_truncated();
        } else if builder.push_frame(leaf_name.to_owned(), span) {
            for index in (0..self.call_stack.len().saturating_sub(1)).rev() {
                if !builder.can_push_frame() {
                    builder.mark_truncated();
                    break;
                }
                if !builder.push_frame(
                    self.call_stack[index].method.clone(),
                    self.call_stack[index + 1].call_span,
                ) {
                    break;
                }
            }
            if let Some(call) = self.call_stack.first() {
                if builder.can_push_frame() {
                    builder.push_frame("<anonymous>".to_owned(), call.call_span);
                } else {
                    builder.mark_truncated();
                }
            }
        }
    }

    fn capture_debug_variables(&self, builder: &mut DebugSnapshotBuilder) {
        let limit = builder.remaining_variable_slots();
        let (visible, variables_truncated) = self.visible_debug_names(limit);
        if variables_truncated {
            builder.mark_truncated();
        }
        for canonical in visible {
            if !builder.can_push_variable() {
                builder.mark_truncated();
                break;
            }
            let slot = self
                .lookup_canonical(canonical)
                .expect("selected debugger variable remains visible");
            let rendered = self.render_value(&slot.value);
            if rendered.truncated {
                builder.mark_truncated();
            }
            if !builder.push_variable(canonical.to_owned(), slot.ty.apex_name(), rendered.text) {
                break;
            }
        }
    }

    fn visible_debug_names(&self, limit: usize) -> (BTreeSet<&str>, bool) {
        let mut visible = BTreeSet::<&str>::new();
        let mut truncated = false;
        for scope in &self.scopes {
            for canonical in scope.keys().map(String::as_str) {
                if visible.contains(canonical) {
                    continue;
                }
                if visible.len() < limit {
                    visible.insert(canonical);
                    continue;
                }
                truncated = true;
                let Some(last) = visible.last().copied() else {
                    continue;
                };
                if canonical < last {
                    visible.remove(last);
                    visible.insert(canonical);
                }
            }
        }
        (visible, truncated)
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
        let profile = self.program().compatibility_profile(expression.span());
        self.execution_context = self.execution_context.with_compatibility_profile(profile);
        match expression {
            Expression::StringLiteral(value, _) => Ok(Value::String(value.clone())),
            Expression::BooleanLiteral(value, _) => Ok(Value::Boolean(*value)),
            Expression::IntegerLiteral(value, _) => Ok(Value::Integer(*value)),
            Expression::LongLiteral(value, span) => i64::try_from(*value)
                .map(Value::Long)
                .map_err(|_| Diagnostic::new("Long literal is out of range", *span)),
            Expression::DecimalLiteral(value, span) => value
                .parse::<Decimal>()
                .map(Value::Decimal)
                .map_err(|_| Diagnostic::new("invalid Decimal literal", *span)),
            Expression::NullLiteral(_) => Ok(Value::Null(None)),
            Expression::Soql(query) => self.evaluate_soql(query.span),
            Expression::Sosl(query) => self.evaluate_sosl(query.span),
            Expression::Variable(identifier) => self.evaluate_variable(identifier),
            Expression::TypeLiteral { ty, span } => Ok(Value::TypeLiteral(
                self.program()
                    .type_literal(*span)
                    .cloned()
                    .unwrap_or_else(|| ty.clone()),
            )),
            Expression::Assignment {
                target,
                operator,
                operator_span,
                value,
                ..
            } => self.evaluate_assignment(target, *operator, *operator_span, value),
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
                    | CallTarget::CustomExceptionConstructor { .. }
                    | CallTarget::SObjectConstructor { .. }
                    | CallTarget::PlatformConstructor(_)
                    | CallTarget::EnumMethod { .. }
                    | CallTarget::SObjectGet
                    | CallTarget::SObjectPut
                    | CallTarget::DatabaseDml(_)
                    | CallTarget::DatabaseQuery { .. }
                    | CallTarget::AggregateResultGet
                    | CallTarget::DmlResultMethod(_)
                    | CallTarget::DmlErrorMethod(_)
                    | CallTarget::SecurityDecisionMethod(_)
                    | CallTarget::CustomMetadataMethod { .. } => Err(Diagnostic::new(
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
            Expression::Conditional {
                condition,
                when_true,
                when_false,
                ..
            } => {
                let outcome = self.evaluate_boolean(condition)?;
                self.record_branch(condition.span(), outcome);
                if outcome {
                    self.evaluate(when_true)
                } else {
                    self.evaluate(when_false)
                }
            }
            Expression::NullCoalesce { left, right, .. } => {
                let value = self.evaluate(left)?;
                let present = !matches!(value, Value::Null(_));
                self.record_branch(left.span(), present);
                if present {
                    Ok(value)
                } else {
                    self.evaluate(right)
                }
            }
            Expression::Instanceof {
                value,
                target,
                operator_span,
                ..
            } => {
                let profile = self.program().compatibility_profile(*operator_span);
                let value = self.evaluate(value)?;
                Ok(Value::Boolean(if matches!(value, Value::Null(_)) {
                    profile.null_instanceof_result()
                } else {
                    self.value_has_type(&value, target)
                }))
            }
            Expression::Index {
                collection,
                index,
                span,
            } => self.evaluate_index(collection, index, *span),
            Expression::MethodCall {
                receiver,
                method,
                arguments,
                safe_navigation,
                span,
                ..
            } => self.evaluate_method_call(receiver, method, arguments, *safe_navigation, *span),
            Expression::MemberAccess {
                receiver,
                member: _,
                safe_navigation,
                span,
                ..
            } => self.evaluate_member_access(receiver, *safe_navigation, *span),
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
            ReferenceTarget::InstancePropertyStorage(target) => {
                let receiver = self
                    .current_receiver
                    .ok_or_else(|| Diagnostic::new("missing instance receiver", identifier.span))?;
                self.read_property_storage(target, Some(receiver), identifier.span)
            }
            ReferenceTarget::StaticPropertyStorage(target) => {
                self.read_property_storage(target, None, identifier.span)
            }
            ReferenceTarget::EnumConstant { class_id, ordinal } => {
                Ok(enum_value(class_id, ordinal))
            }
        }
    }

    fn evaluate_method_call(
        &mut self,
        receiver: &Expression,
        method: &Identifier,
        arguments: &[Expression],
        safe_navigation: bool,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Some(evaluated_receiver) =
            self.evaluate_optional_safe_receiver(receiver, safe_navigation)?
        else {
            return Ok(self.null_short_circuit_value(span));
        };

        let target = self.program().call_target(span);
        match target {
            Some(CallTarget::Intrinsic(intrinsic)) => self.evaluate_intrinsic_call(
                intrinsic,
                receiver,
                evaluated_receiver,
                method,
                arguments,
                span,
            ),
            Some(CallTarget::StaticMethod(target)) => {
                self.evaluate_class_method(target, None, arguments, span, false)
            }
            Some(CallTarget::InstanceMethod(target)) => self.evaluate_object_method_call(
                target,
                receiver,
                evaluated_receiver,
                arguments,
                span,
            ),
            Some(CallTarget::SuperMethod(target)) => {
                let receiver = self
                    .current_receiver
                    .ok_or_else(|| Diagnostic::new("super call has no current receiver", span))?;
                self.evaluate_class_method(target, Some(receiver), arguments, span, false)
            }
            Some(CallTarget::SObjectGet) => {
                self.evaluate_sobject_get(receiver, evaluated_receiver, arguments, span)
            }
            Some(CallTarget::SObjectPut) => {
                self.evaluate_sobject_put(receiver, evaluated_receiver, arguments, span)
            }
            Some(CallTarget::CustomMetadataMethod { object_id, method }) => {
                self.evaluate_custom_metadata_method(object_id, method, arguments, span)
            }
            Some(
                target @ (CallTarget::DatabaseDml(_)
                | CallTarget::DatabaseQuery { .. }
                | CallTarget::AggregateResultGet
                | CallTarget::DmlResultMethod(_)
                | CallTarget::DmlErrorMethod(_)
                | CallTarget::SecurityDecisionMethod(_)),
            ) => self.evaluate_data_method_target(
                target,
                receiver,
                evaluated_receiver,
                arguments,
                span,
            ),
            Some(target @ CallTarget::EnumMethod { .. }) => self.evaluate_checked_enum_call(
                target,
                receiver,
                evaluated_receiver,
                arguments,
                span,
            ),
            Some(
                CallTarget::TopLevelMethod(_)
                | CallTarget::Constructor { .. }
                | CallTarget::CustomExceptionConstructor { .. }
                | CallTarget::SObjectConstructor { .. }
                | CallTarget::PlatformConstructor(_),
            ) => invalid_member_call("invalid checked target for member call", span),
            None => invalid_member_call(
                "unresolved method call escaped semantic validation",
                method.span,
            ),
        }
    }

    fn evaluate_data_method_target(
        &mut self,
        target: CallTarget,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match target {
            CallTarget::DatabaseDml(target) => {
                self.evaluate_database_dml_call(target, arguments, span)
            }
            CallTarget::DatabaseQuery {
                kind,
                expected_object_id,
                access_level_argument,
            } => self.evaluate_database_query_call(
                kind,
                expected_object_id,
                access_level_argument,
                arguments,
                span,
            ),
            CallTarget::AggregateResultGet => {
                self.evaluate_aggregate_result_get(receiver, evaluated_receiver, arguments, span)
            }
            target @ (CallTarget::DmlResultMethod(_) | CallTarget::DmlErrorMethod(_)) => {
                self.evaluate_dml_support_call(target, receiver, evaluated_receiver, span)
            }
            CallTarget::SecurityDecisionMethod(target) => self.evaluate_security_decision_method(
                target,
                receiver,
                evaluated_receiver,
                arguments,
                span,
            ),
            _ => unreachable!("only data method targets use this helper"),
        }
    }

    fn evaluate_custom_metadata_method(
        &mut self,
        object_id: ObjectTypeId,
        method: CustomMetadataMethod,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let object = self
            .program()
            .schema()
            .object_at(object_id.index())
            .expect("checked custom metadata object index is valid");
        let object_type = TypeName::Custom(crate::ast::NamedType::new(
            object.api_name().to_owned(),
            span,
        ));
        match method {
            CustomMetadataMethod::GetAll => {
                if !arguments.is_empty() {
                    return Err(invalid_runtime_operands(span));
                }
                Ok(self.store.allocate_collection(Collection::Map {
                    key_type: TypeName::String,
                    value_type: object_type,
                    entries: Vec::new(),
                }))
            }
            CustomMetadataMethod::GetInstance => {
                let [_] = self.evaluate_arguments(arguments)?.as_slice() else {
                    return Err(invalid_runtime_operands(span));
                };
                Ok(Value::Null(Some(object_type)))
            }
        }
    }

    fn evaluate_security_decision_method(
        &mut self,
        target: SecurityDecisionMethod,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if !arguments.is_empty() {
            return Err(invalid_runtime_operands(span));
        }
        let value = match evaluated_receiver {
            Some(value) => value,
            None => self.evaluate(receiver)?,
        };
        let Value::Platform(id) = value else {
            return Err(invalid_runtime_operands(receiver.span()));
        };
        let PlatformValue::SecurityDecision {
            records,
            removed_fields,
        } = self.store.platform(id).clone()
        else {
            return Err(invalid_runtime_operands(receiver.span()));
        };
        match target {
            SecurityDecisionMethod::GetRecords => Ok(Value::Collection(records)),
            SecurityDecisionMethod::GetRemovedFields => {
                let entries = removed_fields
                    .into_iter()
                    .map(|(object, fields)| {
                        let values = fields.into_iter().map(Value::String).collect();
                        let set = self.store.allocate_collection(Collection::Set {
                            element_type: TypeName::String,
                            elements: values,
                            iteration_depth: 0,
                        });
                        (Value::String(object), set)
                    })
                    .collect();
                Ok(self.store.allocate_collection(Collection::Map {
                    key_type: TypeName::String,
                    value_type: TypeName::Set(Box::new(TypeName::String)),
                    entries,
                }))
            }
        }
    }

    fn evaluate_dml_support_call(
        &mut self,
        target: CallTarget,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match target {
            CallTarget::DmlResultMethod(target) => {
                self.evaluate_dml_result_method(target, receiver, evaluated_receiver, span)
            }
            CallTarget::DmlErrorMethod(target) => {
                self.evaluate_dml_error_method(target, receiver, evaluated_receiver, span)
            }
            _ => unreachable!("caller selects Database DML support calls"),
        }
    }

    fn evaluate_checked_enum_call(
        &mut self,
        target: CallTarget,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let CallTarget::EnumMethod { class_id, method } = target else {
            unreachable!("enum call helper requires an enum target");
        };
        self.evaluate_enum_method(
            class_id,
            method,
            receiver,
            evaluated_receiver,
            arguments,
            span,
        )
    }

    fn evaluate_object_method_call(
        &mut self,
        target: ClassMemberId,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let receiver_value = match evaluated_receiver {
            Some(receiver) => receiver,
            None => self.evaluate(receiver)?,
        };
        let receiver = match receiver_value {
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
        self.evaluate_class_method(target, Some(receiver), arguments, span, true)
    }

    fn evaluate_enum_method(
        &mut self,
        class_id: ClassId,
        method: crate::hir::EnumMethod,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            crate::hir::EnumMethod::Name | crate::hir::EnumMethod::Ordinal => {
                let value = match evaluated_receiver {
                    Some(value) => value,
                    None => self.evaluate(receiver)?,
                };
                let Value::Enum {
                    class_id: actual_class,
                    ordinal,
                } = value
                else {
                    return Err(invalid_runtime_operands(receiver.span()));
                };
                if actual_class != class_id {
                    return Err(Diagnostic::new("enum receiver type mismatch", span));
                }
                if method == crate::hir::EnumMethod::Name {
                    Ok(Value::String(
                        self.classes()[class_id.index()].enum_constants[ordinal]
                            .spelling
                            .clone(),
                    ))
                } else {
                    Ok(Value::Integer(i64::try_from(ordinal).unwrap_or(i64::MAX)))
                }
            }
            crate::hir::EnumMethod::Values => {
                let element_type = TypeName::Custom(crate::ast::NamedType::new(
                    self.classes()[class_id.index()]
                        .qualified_name
                        .spelling
                        .clone(),
                    span,
                ));
                let elements = (0..self.classes()[class_id.index()].enum_constants.len())
                    .map(|ordinal| Value::Enum { class_id, ordinal })
                    .collect();
                Ok(self.store.allocate_collection(Collection::List {
                    element_type,
                    elements,
                    iteration_depth: 0,
                }))
            }
            crate::hir::EnumMethod::ValueOf => {
                let [argument] = arguments else {
                    return Err(Diagnostic::new("invalid enum valueOf arguments", span));
                };
                let Value::String(name) = self.evaluate(argument)? else {
                    return Err(invalid_runtime_operands(argument.span()));
                };
                let ordinal = self.classes()[class_id.index()]
                    .enum_constants
                    .iter()
                    .position(|constant| constant.spelling == name)
                    .ok_or_else(|| {
                        runtime_exception(
                            "IllegalArgumentException",
                            format!(
                                "No enum constant {}.{}",
                                self.classes()[class_id.index()].qualified_name.spelling,
                                name
                            ),
                            span,
                        )
                    })?;
                Ok(Value::Enum { class_id, ordinal })
            }
        }
    }

    fn evaluate_database_dml_call(
        &mut self,
        target: DatabaseDmlTarget,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Some(value) = arguments.first() else {
            return Err(Diagnostic::new("invalid checked Database DML call", span));
        };
        let value = self.evaluate(value)?;
        let all_or_none = match target.all_or_none_argument {
            Some(index) => match self.evaluate(&arguments[index])? {
                Value::Boolean(value) => value,
                Value::Null(_) => {
                    return Err(runtime_exception(
                        "NullPointerException",
                        "Database allOrNone argument is null",
                        arguments[index].span(),
                    ));
                }
                _ => return Err(invalid_runtime_operands(arguments[index].span())),
            },
            None => true,
        };
        let access = match target.access_level_argument {
            Some(index) => {
                let access = self.evaluate(&arguments[index])?;
                self.runtime_access_level(access, arguments[index].span())?
            }
            None => target
                .statement_access
                .unwrap_or(crate::platform::AccessLevel::SystemMode),
        };
        let outcomes = self.execute_dml_value(target, value, all_or_none, access, span)?;
        let result_type = match self.program().expression_type(span) {
            Some(ExpressionType::Value(ty)) => ty.clone(),
            _ => {
                return Err(Diagnostic::new(
                    "Database DML call is missing its checked result type",
                    span,
                ));
            }
        };
        self.dml_outcomes_value(outcomes, &result_type, span)
    }

    fn runtime_access_level(
        &self,
        value: Value,
        span: Span,
    ) -> Result<crate::platform::AccessLevel, Diagnostic> {
        let Value::Platform(id) = value else {
            return Err(invalid_runtime_operands(span));
        };
        match self.store.platform(id) {
            PlatformValue::AccessLevel(access) => Ok(*access),
            _ => Err(invalid_runtime_operands(span)),
        }
    }

    fn null_short_circuit_value(&self, span: Span) -> Value {
        match self.program().expression_type(span) {
            Some(ExpressionType::Value(ty)) => Value::Null(Some(ty.clone())),
            Some(ExpressionType::Void) => Value::Void,
            Some(ExpressionType::Null) | None => Value::Null(None),
        }
    }

    fn evaluate_member_access(
        &mut self,
        receiver: &Expression,
        safe_navigation: bool,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let target = self.program().member_target(span).ok_or_else(|| {
            Diagnostic::new("unresolved member escaped semantic validation", span)
        })?;
        let Some(evaluated_receiver) =
            self.evaluate_optional_safe_receiver(receiver, safe_navigation)?
        else {
            return Ok(self.null_short_circuit_value(span));
        };
        match target {
            MemberTarget::Static(target) => self.read_class_member(target, None, span),
            MemberTarget::Instance(target) => {
                let receiver_value = match evaluated_receiver {
                    Some(receiver) => receiver,
                    None => self.evaluate(receiver)?,
                };
                let receiver = match receiver_value {
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
            MemberTarget::StaticPropertyStorage(target) => {
                self.read_property_storage(target, None, span)
            }
            MemberTarget::InstancePropertyStorage(target) => {
                let receiver_value = match evaluated_receiver {
                    Some(receiver) => receiver,
                    None => self.evaluate(receiver)?,
                };
                let receiver = match receiver_value {
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
                self.read_property_storage(target, Some(receiver), span)
            }
            MemberTarget::SObjectField {
                object_id,
                field_id,
            } => {
                let receiver = self.evaluate_sobject_receiver(receiver, evaluated_receiver)?;
                self.read_sobject_field(receiver, object_id, field_id, span)
            }
            MemberTarget::SObjectRelationship {
                object_id,
                reference_field_id,
                target_object_id,
            } => self.evaluate_sobject_relationship(
                receiver,
                evaluated_receiver,
                object_id,
                reference_field_id,
                target_object_id,
                span,
            ),
            MemberTarget::SObjectChildRelationship {
                object_id,
                child_object_id,
                relationship,
            } => self.evaluate_sobject_child_relationship(
                receiver,
                evaluated_receiver,
                object_id,
                child_object_id,
                &relationship,
                span,
            ),
            MemberTarget::Schema(target) => {
                self.evaluate_schema_member(target, receiver, evaluated_receiver, span)
            }
            MemberTarget::TriggerContext(variable) => self.trigger_context_value(variable, span),
            target @ (MemberTarget::DmlStatus(_)
            | MemberTarget::AccessLevel(_)
            | MemberTarget::AccessType(_)
            | MemberTarget::PlatformEnum(_)
            | MemberTarget::EnumConstant { .. }
            | MemberTarget::TypeReference { .. }) => self.evaluate_constant_member(target, span),
        }
    }

    fn evaluate_constant_member(
        &mut self,
        target: MemberTarget,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        Ok(match target {
            MemberTarget::DmlStatus(status) => self.dml_status_value(status),
            MemberTarget::AccessLevel(access) => self
                .store
                .allocate_platform(PlatformValue::AccessLevel(access)),
            MemberTarget::AccessType(access) => self
                .store
                .allocate_platform(PlatformValue::AccessType(access)),
            MemberTarget::PlatformEnum(value) => self
                .store
                .allocate_platform(PlatformValue::PlatformEnum(value)),
            MemberTarget::EnumConstant { class_id, ordinal } => enum_value(class_id, ordinal),
            MemberTarget::TypeReference { class_id } => {
                Value::TypeLiteral(TypeName::Custom(crate::ast::NamedType::new(
                    self.classes()[class_id.index()]
                        .qualified_name
                        .spelling
                        .clone(),
                    span,
                )))
            }
            _ => unreachable!("only constant members use this helper"),
        })
    }

    fn evaluate_schema_member(
        &mut self,
        target: SchemaMemberTarget,
        expression: &Expression,
        evaluated_receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        Ok(match target {
            SchemaMemberTarget::SObjectType { object_id } => self
                .store
                .allocate_platform(PlatformValue::SObjectType(object_id)),
            SchemaMemberTarget::SObjectField {
                object_id,
                field_id,
            } => self.store.allocate_platform(PlatformValue::SObjectField {
                object_id,
                field_id,
            }),
            SchemaMemberTarget::DescribeFields => {
                let receiver = match evaluated_receiver {
                    Some(receiver) => receiver,
                    None => self.evaluate(expression)?,
                };
                let object_id = self.expect_any_schema_object(Some(receiver), span)?;
                self.store
                    .allocate_platform(PlatformValue::SObjectFieldMap(object_id))
            }
            SchemaMemberTarget::DescribeFieldSets => {
                let receiver = match evaluated_receiver {
                    Some(receiver) => receiver,
                    None => self.evaluate(expression)?,
                };
                let object_id = self.expect_any_schema_object(Some(receiver), span)?;
                self.store
                    .allocate_platform(PlatformValue::FieldSetMap(object_id))
            }
            SchemaMemberTarget::PicklistValue(value) => Value::String(value),
        })
    }

    fn evaluate_optional_safe_receiver(
        &mut self,
        receiver: &Expression,
        safe_navigation: bool,
    ) -> Result<Option<Option<Value>>, Diagnostic> {
        if !safe_navigation {
            return Ok(Some(None));
        }
        let value = self.evaluate(receiver)?;
        let present = !matches!(value, Value::Null(_));
        self.record_branch(receiver.span(), present);
        Ok(present.then_some(Some(value)))
    }

    fn dml_status_value(&mut self, status: crate::platform::DmlStatus) -> Value {
        self.store
            .allocate_platform(PlatformValue::DmlStatus(status))
    }

    fn evaluate_sobject_relationship(
        &mut self,
        expression: &Expression,
        evaluated_receiver: Option<Value>,
        object_id: usize,
        reference_field_id: usize,
        target_object_id: usize,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let receiver = self.evaluate_sobject_receiver(expression, evaluated_receiver)?;
        let instance = self.store.sobject(receiver);
        if instance.object_id != object_id {
            return Err(Diagnostic::new(
                "SObject relationship target does not match runtime object type",
                span,
            ));
        }
        Ok(instance
            .relationships
            .get(&reference_field_id)
            .copied()
            .map(Value::SObject)
            .unwrap_or_else(|| {
                let target = self
                    .program()
                    .schema()
                    .object_at(target_object_id)
                    .expect("checked relationship target is valid");
                Value::Null(Some(TypeName::Custom(crate::ast::NamedType::new(
                    target.api_name().to_owned(),
                    span,
                ))))
            }))
    }

    fn evaluate_sobject_child_relationship(
        &mut self,
        expression: &Expression,
        evaluated_receiver: Option<Value>,
        object_id: usize,
        child_object_id: usize,
        relationship: &str,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let receiver = self.evaluate_sobject_receiver(expression, evaluated_receiver)?;
        let instance = self.store.sobject(receiver);
        if instance.object_id != object_id {
            return Err(Diagnostic::new(
                "SObject child relationship target does not match runtime object type",
                span,
            ));
        }
        if let Some(children) = instance.children.get(relationship).copied() {
            return Ok(Value::Collection(children));
        }
        let child = self
            .program()
            .schema()
            .object_at(child_object_id)
            .expect("checked child relationship target is valid");
        Ok(self.store.allocate_collection(Collection::List {
            element_type: TypeName::Custom(crate::ast::NamedType::new(
                child.api_name().to_owned(),
                span,
            )),
            elements: Vec::new(),
            iteration_depth: 0,
        }))
    }

    fn trigger_context_value(
        &self,
        variable: TriggerContextVariable,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let context = self.trigger_context.as_ref().ok_or_else(|| {
            Diagnostic::new(
                "Trigger context is unavailable outside trigger execution",
                span,
            )
        })?;
        let event = context.event;
        Ok(match variable {
            TriggerContextVariable::New => context.new_list.clone(),
            TriggerContextVariable::Old => context.old_list.clone(),
            TriggerContextVariable::NewMap => context.new_map.clone(),
            TriggerContextVariable::OldMap => context.old_map.clone(),
            TriggerContextVariable::IsExecuting => Value::Boolean(true),
            TriggerContextVariable::IsBefore => Value::Boolean(event.is_before()),
            TriggerContextVariable::IsAfter => Value::Boolean(!event.is_before()),
            TriggerContextVariable::IsInsert => {
                Value::Boolean(event.operation() == crate::ast::DmlOperation::Insert)
            }
            TriggerContextVariable::IsUpdate => {
                Value::Boolean(event.operation() == crate::ast::DmlOperation::Update)
            }
            TriggerContextVariable::IsDelete => {
                Value::Boolean(event.operation() == crate::ast::DmlOperation::Delete)
            }
            TriggerContextVariable::IsUndelete => {
                Value::Boolean(event.operation() == crate::ast::DmlOperation::Undelete)
            }
            TriggerContextVariable::Size => {
                Value::Integer(i64::try_from(context.size).unwrap_or(i64::MAX))
            }
        })
    }

    fn evaluate_sobject_get(
        &mut self,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let [field_name] = arguments else {
            return Err(Diagnostic::new("invalid checked SObject.get call", span));
        };
        let receiver = self.evaluate_sobject_receiver(receiver, evaluated_receiver)?;
        let object_id = self.store.sobject(receiver).object_id;
        let field_value = self.evaluate(field_name)?;
        let field_id = match field_value {
            Value::String(value) => self.sobject_field_id(object_id, &value, span)?,
            Value::Platform(id) => match self.store.platform(id) {
                PlatformValue::SObjectField {
                    object_id: field_object,
                    field_id,
                } if *field_object == object_id => *field_id,
                PlatformValue::SObjectField { .. } => {
                    return Err(runtime_exception(
                        "SObjectException",
                        "SObject field token belongs to a different object type",
                        field_name.span(),
                    ));
                }
                _ => return Err(invalid_runtime_operands(field_name.span())),
            },
            Value::Null(_) => {
                return Err(runtime_exception(
                    "IllegalArgumentException",
                    "SObject field name cannot be null",
                    field_name.span(),
                ));
            }
            _ => return Err(invalid_runtime_operands(field_name.span())),
        };
        self.read_sobject_field(receiver, object_id, field_id, span)
    }

    fn evaluate_sobject_put(
        &mut self,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let [field_name, value] = arguments else {
            return Err(Diagnostic::new("invalid checked SObject.put call", span));
        };
        let receiver = self.evaluate_sobject_receiver(receiver, evaluated_receiver)?;
        let field_name_value = self.evaluate(field_name)?;
        let value = self.evaluate(value)?;
        let object_id = self.store.sobject(receiver).object_id;
        let field_id = match field_name_value {
            Value::String(field_name) => self.sobject_field_id(object_id, &field_name, span)?,
            Value::Platform(id) => match self.store.platform(id) {
                PlatformValue::SObjectField {
                    object_id: field_object,
                    field_id,
                } if *field_object == object_id => *field_id,
                PlatformValue::SObjectField { .. } => {
                    return Err(runtime_exception(
                        "SObjectException",
                        "SObject field token belongs to a different object type",
                        field_name.span(),
                    ));
                }
                _ => return Err(invalid_runtime_operands(field_name.span())),
            },
            _ => {
                return Err(runtime_exception(
                    "IllegalArgumentException",
                    "SObject field name must be a non-null String or Schema.SObjectField",
                    field_name.span(),
                ));
            }
        };
        self.write_sobject_field(receiver, object_id, field_id, value, span)?;
        Ok(Value::Void)
    }

    fn sobject_field_id(
        &self,
        object_id: usize,
        field_name: &str,
        span: Span,
    ) -> Result<usize, Diagnostic> {
        let object = self
            .program()
            .schema()
            .object_at(object_id)
            .expect("runtime SObject schema index is valid");
        object.field_index(field_name).ok_or_else(|| {
            runtime_exception(
                "IllegalArgumentException",
                format!(
                    "unknown field `{}` on SObject `{}`",
                    field_name,
                    object.api_name()
                ),
                span,
            )
        })
    }

    fn evaluate_sobject_receiver(
        &mut self,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
    ) -> Result<SObjectId, Diagnostic> {
        let receiver_value = match evaluated_receiver {
            Some(receiver) => receiver,
            None => self.evaluate(receiver)?,
        };
        match receiver_value {
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
        if self.read_only_sobjects.contains(&receiver) {
            return Err(runtime_exception(
                "FinalException",
                "record is read-only in this trigger context",
                span,
            ));
        }
        let field = self
            .program()
            .schema()
            .object_at(object_id)
            .and_then(|object| object.field_at(field_id))
            .expect("checked SObject field index is valid");
        if matches!(field.data_type(), FieldType::Summary { .. })
            || field.api_name().eq_ignore_ascii_case("IsDeleted")
        {
            return Err(runtime_exception(
                "SObjectException",
                format!("field `{}` is read-only", field.api_name()),
                span,
            ));
        }
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
        self.ensure_class_initialized(target.class_id, span)?;
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
        self.ensure_class_initialized(target.class_id, span)?;
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
                self.write_class_property(target, receiver, property, value, span)
            }
            _ => Err(Diagnostic::new("target is not a value member", span)),
        }
    }

    fn read_property_storage(
        &mut self,
        target: ClassMemberId,
        receiver: Option<ObjectId>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.ensure_class_initialized(target.class_id, span)?;
        let ClassMember::Property(property) =
            &self.classes()[target.class_id].members[target.member_id]
        else {
            return Err(Diagnostic::new(
                "property storage target is not a property",
                span,
            ));
        };
        if property.modifiers.contains(&Modifier::Static) {
            Ok(self
                .store
                .static_slot(&target)
                .expect("property storage exists")
                .value
                .clone())
        } else {
            let receiver =
                receiver.ok_or_else(|| Diagnostic::new("missing property receiver", span))?;
            Ok(self.store.object(receiver).fields[&target].value.clone())
        }
    }

    fn write_property_storage(
        &mut self,
        target: ClassMemberId,
        receiver: Option<ObjectId>,
        value: Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.ensure_class_initialized(target.class_id, span)?;
        let ClassMember::Property(property) =
            &self.classes()[target.class_id].members[target.member_id]
        else {
            return Err(Diagnostic::new(
                "property storage target is not a property",
                span,
            ));
        };
        let ty = property.ty.clone();
        let is_static = property.modifiers.contains(&Modifier::Static);
        let value = typed_value(value, &ty);
        if is_static {
            self.store
                .static_slot_mut(&target)
                .expect("property storage exists")
                .value = value.clone();
        } else {
            let receiver =
                receiver.ok_or_else(|| Diagnostic::new("missing property receiver", span))?;
            self.store
                .object_mut(receiver)
                .fields
                .get_mut(&target)
                .expect("property storage exists")
                .value = value.clone();
        }
        Ok(value)
    }

    fn write_class_property(
        &mut self,
        target: ClassMemberId,
        receiver: Option<ObjectId>,
        property: crate::ast::PropertyDeclaration,
        value: Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
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
            let receiver =
                receiver.ok_or_else(|| Diagnostic::new("missing property receiver", span))?;
            self.store
                .object_mut(receiver)
                .fields
                .get_mut(&target)
                .expect("auto property storage exists")
                .value = value.clone();
        }
        Ok(value)
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
        match target {
            CallTarget::PlatformConstructor(constructor) => {
                self.construct_platform_value(constructor, arguments, span)
            }
            CallTarget::SObjectConstructor { object_id } => {
                self.construct_sobject(object_id, arguments, span)
            }
            CallTarget::Constructor {
                class_id,
                member_id,
            } => {
                let arguments = self.evaluate_arguments(arguments)?;
                self.ensure_class_initialized(class_id, span)?;
                self.construct_user_object(class_id, member_id, arguments, span)
            }
            CallTarget::CustomExceptionConstructor { class_id } => {
                let class_id = class_id.index();
                let message = match arguments {
                    [] => String::new(),
                    [message] => match self.evaluate(message)? {
                        Value::String(message) => message,
                        Value::Null(_) => String::new(),
                        _ => return Err(invalid_runtime_operands(message.span())),
                    },
                    _ => {
                        return Err(Diagnostic::new(
                            "invalid custom exception constructor arguments",
                            span,
                        ));
                    }
                };
                Ok(Value::Exception(Box::new(Diagnostic::runtime_exception(
                    self.classes()[class_id].qualified_name.spelling.clone(),
                    message,
                    Span::new(0, 0),
                ))))
            }
            _ => Err(Diagnostic::new("invalid constructor target", span)),
        }
    }

    fn construct_platform_value(
        &mut self,
        constructor: crate::hir::PlatformConstructor,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let value = match constructor {
            crate::hir::PlatformConstructor::Http => {
                require_no_constructor_arguments(arguments, span)?;
                PlatformValue::Http
            }
            crate::hir::PlatformConstructor::HttpRequest => {
                require_no_constructor_arguments(arguments, span)?;
                PlatformValue::HttpRequest(HttpRequestData::default())
            }
            crate::hir::PlatformConstructor::HttpResponse => {
                require_no_constructor_arguments(arguments, span)?;
                PlatformValue::HttpResponse(HttpResponseData::default())
            }
            crate::hir::PlatformConstructor::VisualEditorDataRow => {
                let evaluated = self.evaluate_arguments(arguments)?;
                let [label, value] = evaluated.as_slice() else {
                    return Err(Diagnostic::new(
                        "VisualEditor.DataRow constructor requires two arguments",
                        span,
                    ));
                };
                let (Value::String(label), Value::String(value)) = (&label.value, &value.value)
                else {
                    return Err(invalid_runtime_operands(span));
                };
                PlatformValue::VisualEditorDataRow {
                    label: label.clone(),
                    value: value.clone(),
                }
            }
            crate::hir::PlatformConstructor::VisualEditorDynamicPickListRows => {
                require_no_constructor_arguments(arguments, span)?;
                PlatformValue::VisualEditorDynamicPickListRows(Vec::new())
            }
        };
        Ok(self.store.allocate_platform(value))
    }

    fn construct_sobject(
        &mut self,
        object_id: Option<usize>,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
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
        Ok(self.store.allocate_sobject(object_id))
    }

    fn construct_user_object(
        &mut self,
        class_id: usize,
        member_id: Option<usize>,
        arguments: Vec<EvaluatedArgument>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let object_id = self.store.allocate_object(class_id);
        self.allocate_instance_fields(object_id, class_id);
        let mut initialized = HashSet::new();
        self.execute_constructor_chain(
            object_id,
            class_id,
            member_id,
            arguments,
            span,
            &mut initialized,
        )?;
        Ok(Value::Object(object_id))
    }

    fn execute_constructor_chain(
        &mut self,
        object: ObjectId,
        class_id: usize,
        member_id: Option<usize>,
        arguments: Vec<EvaluatedArgument>,
        span: Span,
        initialized: &mut HashSet<usize>,
    ) -> Result<(), Diagnostic> {
        let constructor = member_id
            .map(
                |member_id| match self.classes()[class_id].members[member_id].clone() {
                    ClassMember::Constructor(constructor) => Ok(constructor),
                    _ => Err(Diagnostic::new("invalid constructor member", span)),
                },
            )
            .transpose()?;

        if let Some(delegation) = constructor
            .as_ref()
            .and_then(|constructor| constructor.delegation.as_ref())
        {
            let delegated_arguments = self.evaluate_constructor_delegation_arguments(
                object,
                class_id,
                constructor.as_ref().expect("delegation has a constructor"),
                &arguments,
                &delegation.arguments,
            )?;
            let Some(CallTarget::Constructor {
                class_id: target_class,
                member_id: target_member,
            }) = self.program().call_target(delegation.span)
            else {
                return Err(Diagnostic::new(
                    "unresolved constructor delegation",
                    delegation.span,
                ));
            };
            self.execute_constructor_chain(
                object,
                target_class,
                target_member,
                delegated_arguments,
                delegation.span,
                initialized,
            )?;
        } else if let Some(parent) = self.classes()[class_id]
            .superclass
            .as_ref()
            .and_then(|parent| self.runtime_class_id(parent))
        {
            self.execute_constructor_chain(
                object,
                parent,
                self.zero_argument_constructor(parent),
                Vec::new(),
                span,
                initialized,
            )?;
        }

        if initialized.insert(class_id) {
            self.initialize_instance_fields(object, class_id)?;
        }
        if let Some(constructor) = constructor {
            self.execute_callable(
                &constructor.parameters,
                &constructor.body,
                &ReturnType::Void,
                &constructor.name.spelling,
                Some(object),
                class_id,
                arguments,
                span,
            )?;
        }
        Ok(())
    }

    fn evaluate_constructor_delegation_arguments(
        &mut self,
        object: ObjectId,
        class_id: usize,
        constructor: &crate::ast::ConstructorDeclaration,
        current_arguments: &[EvaluatedArgument],
        expressions: &[Expression],
    ) -> Result<Vec<EvaluatedArgument>, Diagnostic> {
        let scope = constructor
            .parameters
            .iter()
            .zip(current_arguments)
            .map(|(parameter, argument)| {
                (
                    parameter.name.canonical.clone(),
                    Slot {
                        ty: parameter.ty.clone(),
                        value: typed_value(argument.value.clone(), &parameter.ty),
                    },
                )
            })
            .collect();
        let caller_scopes = std::mem::replace(&mut self.scopes, vec![scope]);
        let saved_receiver = self.current_receiver.replace(object);
        let saved_declaring = self.current_declaring_class.replace(class_id);
        let result = self.evaluate_arguments(expressions);
        self.scopes = caller_scopes;
        self.current_receiver = saved_receiver;
        self.current_declaring_class = saved_declaring;
        result
    }

    fn allocate_instance_fields(&mut self, object: ObjectId, class_id: usize) {
        for current in self.class_lineage_base_first(class_id) {
            let targets = self
                .program()
                .class_metadata(ClassId::from_index(current))
                .instance_slots
                .clone();
            for target in targets {
                let ty = match &self.classes()[current].members[target.member_id] {
                    ClassMember::Field(field) => field.ty.clone(),
                    ClassMember::Property(property) => property.ty.clone(),
                    _ => unreachable!("instance slot metadata refers to a value member"),
                };
                self.store.object_mut(object).fields.insert(
                    target,
                    Slot {
                        value: Value::Null(Some(ty.clone())),
                        ty,
                    },
                );
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
        let steps = self
            .program()
            .class_metadata(ClassId::from_index(class_id))
            .instance_steps
            .clone();
        let result = (|| {
            for step in steps {
                match step {
                    crate::hir::ClassInitializationStep::Field(target) => {
                        let ClassMember::Field(field) =
                            self.classes()[class_id].members[target.member_id].clone()
                        else {
                            unreachable!("field initialization metadata refers to a field");
                        };
                        let initializer = field
                            .initializer
                            .as_ref()
                            .expect("field initialization metadata requires an initializer");
                        let value = typed_value(self.evaluate(initializer)?, &field.ty);
                        self.store
                            .object_mut(object)
                            .fields
                            .get_mut(&target)
                            .expect("instance field was allocated")
                            .value = value;
                    }
                    crate::hir::ClassInitializationStep::Block(target) => {
                        let ClassMember::Initializer(initializer) =
                            self.classes()[class_id].members[target.member_id].clone()
                        else {
                            unreachable!("initializer metadata refers to a block");
                        };
                        if !matches!(self.execute_statement(&initializer.body)?, Flow::Normal) {
                            return Err(Diagnostic::new(
                                "instance initializer completed abruptly",
                                initializer.span,
                            ));
                        }
                    }
                }
            }
            Ok(())
        })();
        self.current_receiver = saved_receiver;
        self.current_declaring_class = saved_declaring;
        result
    }

    fn class_lineage_base_first(&self, class_id: usize) -> Vec<usize> {
        self.program()
            .class_metadata(ClassId::from_index(class_id))
            .lineage_base_first
            .iter()
            .map(|class_id| class_id.index())
            .collect()
    }

    fn zero_argument_constructor(&self, class_id: usize) -> Option<usize> {
        self.classes()[class_id]
            .members
            .iter()
            .position(|member| matches!(member, ClassMember::Constructor(constructor) if constructor.parameters.is_empty()))
    }

    fn runtime_class_id(&self, name: &crate::ast::NamedType) -> Option<usize> {
        self.classes()
            .iter()
            .position(|class| class.qualified_name.canonical == name.canonical)
            .or_else(|| {
                let mut matches = self
                    .classes()
                    .iter()
                    .enumerate()
                    .filter(|(_, class)| class.name.canonical == name.canonical)
                    .map(|(class_id, _)| class_id);
                let first = matches.next()?;
                matches.next().is_none().then_some(first)
            })
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
        self.evaluate_class_method_arguments(
            target,
            receiver,
            arguments,
            span,
            virtual_dispatch,
            true,
        )
    }

    fn evaluate_class_method_arguments(
        &mut self,
        target: ClassMemberId,
        receiver: Option<ObjectId>,
        arguments: Vec<EvaluatedArgument>,
        span: Span,
        virtual_dispatch: bool,
        enqueue_future: bool,
    ) -> Result<Value, Diagnostic> {
        let target = if virtual_dispatch {
            receiver
                .map(|receiver| self.virtual_method_target(receiver, target))
                .unwrap_or(target)
        } else {
            target
        };
        self.ensure_class_initialized(target.class_id, span)?;
        let method = match self.classes()[target.class_id].members[target.member_id].clone() {
            ClassMember::Method(method) => method,
            _ => return Err(Diagnostic::new("method target is invalid", span)),
        };
        if enqueue_future
            && method
                .annotations
                .iter()
                .any(|annotation| annotation.kind.is_future())
        {
            self.enqueue_future(target, arguments, span)?;
            return Ok(Value::Void);
        }
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
                .and_then(|parent| self.runtime_class_id(parent));
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
        let saved_execution_context = self.execution_context;
        self.execution_context = self.execution_context.for_class(
            self.program()
                .class_metadata(ClassId::from_index(declaring_class))
                .sharing,
        );
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
        self.execution_context = saved_execution_context;
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
        match (&value, target) {
            (Value::Integer(value), TypeName::Long) => return Ok(Value::Long(*value)),
            (Value::Integer(_) | Value::Long(_) | Value::Decimal(_), TypeName::Double) => {
                return Ok(typed_value(value, target));
            }
            (Value::Long(value), TypeName::Integer) => {
                return i32::try_from(*value)
                    .map(|value| Value::Integer(i64::from(value)))
                    .map_err(|_| {
                        runtime_exception(
                            "TypeException",
                            "Long value is out of range for Integer",
                            span,
                        )
                    });
            }
            _ => {}
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
        operator: AssignmentOperator,
        operator_span: Span,
        value: &Expression,
    ) -> Result<Value, Diagnostic> {
        let place = self.resolve_assignment_place(target)?;
        if operator == AssignmentOperator::Assign {
            let value = self.evaluate(value)?;
            return self.write_place(&place, value, target.span());
        }

        let left = self.read_place(&place, operator_span)?;
        let right = self.evaluate(value)?;
        let operation = self
            .program()
            .binary_operation(operator_span)
            .ok_or_else(|| Diagnostic::new("unresolved compound operation", operator_span))?;
        let result = self.apply_checked_binary(operation, left, right, operator_span)?;
        self.write_place(&place, result, target.span())
    }

    fn resolve_assignment_place(&mut self, target: &AssignmentTarget) -> Result<Place, Diagnostic> {
        let syntax = match target {
            AssignmentTarget::Variable(identifier) => PlaceSyntax::Variable(identifier),
            AssignmentTarget::Index {
                collection,
                index,
                span,
            } => PlaceSyntax::Index {
                collection,
                index,
                span: *span,
            },
            AssignmentTarget::Member { receiver, span, .. } => PlaceSyntax::Member {
                receiver,
                span: *span,
            },
        };
        self.resolve_place(syntax)
    }

    fn resolve_mutation_place(&mut self, operand: &Expression) -> Result<Place, Diagnostic> {
        let syntax = match operand {
            Expression::Variable(identifier) => PlaceSyntax::Variable(identifier),
            Expression::Index {
                collection,
                index,
                span,
            } => PlaceSyntax::Index {
                collection,
                index,
                span: *span,
            },
            Expression::MemberAccess {
                receiver,
                safe_navigation: false,
                span,
                ..
            } => PlaceSyntax::Member {
                receiver,
                span: *span,
            },
            _ => {
                return Err(Diagnostic::new(
                    "mutation operand must be an assignable value",
                    operand.span(),
                ));
            }
        };
        self.resolve_place(syntax)
    }

    fn resolve_place(&mut self, syntax: PlaceSyntax<'_>) -> Result<Place, Diagnostic> {
        let span = syntax.span();
        let target = self
            .program()
            .place_target(span)
            .ok_or_else(|| Diagnostic::new("unresolved assignable place", span))?;
        match target {
            PlaceTarget::Local => {
                let PlaceSyntax::Variable(identifier) = syntax else {
                    return Err(Diagnostic::new("invalid local place syntax", span));
                };
                Ok(Place::Local(identifier.clone()))
            }
            PlaceTarget::InstanceMember(target) => {
                self.resolve_instance_member_place(syntax, target, span)
            }
            PlaceTarget::StaticMember(target) => Ok(Place::ClassMember {
                target,
                receiver: None,
                property_storage: false,
            }),
            PlaceTarget::InstancePropertyStorage(target) => {
                let mut place = self.resolve_instance_member_place(syntax, target, span)?;
                let Place::ClassMember {
                    property_storage, ..
                } = &mut place
                else {
                    unreachable!("instance member resolution creates a class member place")
                };
                *property_storage = true;
                Ok(place)
            }
            PlaceTarget::StaticPropertyStorage(target) => Ok(Place::ClassMember {
                target,
                receiver: None,
                property_storage: true,
            }),
            PlaceTarget::ListIndex => self.resolve_list_index_place(syntax, span),
            PlaceTarget::SObjectField {
                object_id,
                field_id,
            } => self.resolve_sobject_field_place(syntax, object_id, field_id, span),
        }
    }

    fn resolve_instance_member_place(
        &mut self,
        syntax: PlaceSyntax<'_>,
        target: ClassMemberId,
        span: Span,
    ) -> Result<Place, Diagnostic> {
        let receiver = match syntax {
            PlaceSyntax::Variable(_) => self
                .current_receiver
                .ok_or_else(|| Diagnostic::new("missing instance receiver", span))?,
            PlaceSyntax::Member { receiver, .. } => match self.evaluate(receiver)? {
                Value::Object(receiver) => receiver,
                Value::Null(_) => {
                    return Err(runtime_exception(
                        "NullPointerException",
                        "member assignment receiver is null",
                        receiver.span(),
                    ));
                }
                _ => return Err(invalid_runtime_operands(receiver.span())),
            },
            PlaceSyntax::Index { .. } => {
                return Err(Diagnostic::new("invalid member place syntax", span));
            }
        };
        Ok(Place::ClassMember {
            target,
            receiver: Some(receiver),
            property_storage: false,
        })
    }

    fn resolve_list_index_place(
        &mut self,
        syntax: PlaceSyntax<'_>,
        place_span: Span,
    ) -> Result<Place, Diagnostic> {
        let PlaceSyntax::Index {
            collection, index, ..
        } = syntax
        else {
            return Err(Diagnostic::new("invalid indexed place syntax", place_span));
        };
        let collection_value = self.evaluate(collection)?;
        let index_span = index.span();
        let index_value = self.evaluate(index)?;
        let collection = self.expect_collection_id(collection_value, collection.span())?;
        let index = self.expect_index(index_value, index_span)?;
        self.ensure_collection_mutable(collection, place_span)?;
        let Collection::List { elements, .. } = self.collection(collection) else {
            return Err(invalid_runtime_operands(place_span));
        };
        let index = checked_list_index(index, elements.len(), false, index_span)?;
        Ok(Place::ListIndex { collection, index })
    }

    fn resolve_sobject_field_place(
        &mut self,
        syntax: PlaceSyntax<'_>,
        object_id: ObjectTypeId,
        field_id: FieldId,
        span: Span,
    ) -> Result<Place, Diagnostic> {
        let PlaceSyntax::Member { receiver, .. } = syntax else {
            return Err(Diagnostic::new("invalid SObject place syntax", span));
        };
        let receiver = self.evaluate_sobject_receiver(receiver, None)?;
        Ok(Place::SObjectField {
            receiver,
            object_id: object_id.index(),
            field_id: field_id.index(),
        })
    }

    fn read_place(&mut self, place: &Place, span: Span) -> Result<Value, Diagnostic> {
        match place {
            Place::Local(identifier) => self.lookup(identifier).map(|slot| slot.value.clone()),
            Place::ClassMember {
                target,
                receiver,
                property_storage,
            } => {
                if *property_storage {
                    self.read_property_storage(*target, *receiver, span)
                } else {
                    self.read_class_member(*target, *receiver, span)
                }
            }
            Place::ListIndex { collection, index } => {
                let Collection::List { elements, .. } = self.collection(*collection) else {
                    return Err(invalid_runtime_operands(span));
                };
                Ok(elements[*index].clone())
            }
            Place::SObjectField {
                receiver,
                object_id,
                field_id,
            } => self.read_sobject_field(*receiver, *object_id, *field_id, span),
        }
    }

    fn write_place(
        &mut self,
        place: &Place,
        value: Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match place {
            Place::Local(identifier) => self.assign_variable(identifier, value),
            Place::ClassMember {
                target,
                receiver,
                property_storage,
            } => {
                if *property_storage {
                    self.write_property_storage(*target, *receiver, value, span)
                } else {
                    self.write_class_member(*target, *receiver, value, span)
                }
            }
            Place::ListIndex { collection, index } => {
                let element_type = match self.collection(*collection) {
                    Collection::List { element_type, .. } => element_type.clone(),
                    _ => return Err(invalid_runtime_operands(span)),
                };
                let value = typed_value(value, &element_type);
                let Collection::List { elements, .. } = self.collection_mut(*collection) else {
                    unreachable!("List place was resolved above")
                };
                elements[*index] = value.clone();
                Ok(value)
            }
            Place::SObjectField {
                receiver,
                object_id,
                field_id,
            } => self.write_sobject_field(*receiver, *object_id, *field_id, value, span),
        }
    }

    fn apply_checked_binary(
        &self,
        operation: CheckedBinaryOperation,
        left: Value,
        right: Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if operation == CheckedBinaryOperation::StringConcat {
            return Ok(Value::String(
                self.stringify_value(&left) + &self.stringify_value(&right),
            ));
        }
        numeric::apply_binary(operation, left, right, span)
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
                let size_value = match value {
                    Value::Integer(value) | Value::Long(value) => value,
                    _ => {
                        return Err(runtime_exception(
                            "NullPointerException",
                            "array size must be a non-null Integer",
                            size_span,
                        ));
                    }
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
            UnaryOperator::Positive | UnaryOperator::Negate | UnaryOperator::BitwiseNot => {
                let operation = self
                    .program()
                    .unary_operation(operator_span)
                    .ok_or_else(|| Diagnostic::new("unresolved unary operation", operator_span))?;
                if operation == CheckedUnaryOperation::Negate(NumericKind::Integer)
                    && matches!(
                        operand,
                        Expression::IntegerLiteral(value, _)
                            if *value == i64::from(i32::MAX) + 1
                    )
                {
                    return Ok(Value::Integer(i64::from(i32::MIN)));
                }
                if operation == CheckedUnaryOperation::Negate(NumericKind::Long)
                    && matches!(
                        operand,
                        Expression::LongLiteral(value, _)
                            if *value == i128::from(i64::MAX) + 1
                    )
                {
                    return Ok(Value::Long(i64::MIN));
                }
                let value = self.evaluate(operand)?;
                numeric::apply_unary(operation, value, operator_span)
            }
            UnaryOperator::Not => {
                let value = self.evaluate_boolean(operand)?;
                Ok(Value::Boolean(!value))
            }
            UnaryOperator::PrefixIncrement => {
                self.mutate_integral(operand, 1, false, operator_span)
            }
            UnaryOperator::PrefixDecrement => {
                self.mutate_integral(operand, -1, false, operator_span)
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
        self.mutate_integral(operand, delta, true, operator_span)
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
        if let Some(operation) = self.program().binary_operation(operator_span) {
            return self.apply_checked_binary(operation, left, right, operator_span);
        }
        match operator {
            BinaryOperator::Less => {
                compare_values(left, right, operator_span, |ordering| ordering.is_lt())
            }
            BinaryOperator::LessEqual => {
                compare_values(left, right, operator_span, |ordering| ordering.is_le())
            }
            BinaryOperator::Greater => {
                compare_values(left, right, operator_span, |ordering| ordering.is_gt())
            }
            BinaryOperator::GreaterEqual => {
                compare_values(left, right, operator_span, |ordering| ordering.is_ge())
            }
            BinaryOperator::Equal => Ok(Value::Boolean(self.operator_values_equal(&left, &right))),
            BinaryOperator::NotEqual => {
                Ok(Value::Boolean(!self.operator_values_equal(&left, &right)))
            }
            BinaryOperator::ExactEqual => Ok(Value::Boolean(
                self.operator_values_identical(&left, &right),
            )),
            BinaryOperator::ExactNotEqual => Ok(Value::Boolean(
                !self.operator_values_identical(&left, &right),
            )),
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder
            | BinaryOperator::BitwiseAnd
            | BinaryOperator::BitwiseOr
            | BinaryOperator::BitwiseXor
            | BinaryOperator::ShiftLeft
            | BinaryOperator::ShiftRight
            | BinaryOperator::UnsignedShiftRight => {
                Err(Diagnostic::new("missing checked operation", operator_span))
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

    fn mutate_integral(
        &mut self,
        operand: &Expression,
        delta: i32,
        return_old: bool,
        operator_span: Span,
    ) -> Result<Value, Diagnostic> {
        let place = self.resolve_mutation_place(operand)?;
        let old = self.read_place(&place, operator_span)?;
        let kind = match old {
            Value::Integer(_) | Value::Null(Some(TypeName::Integer)) => NumericKind::Integer,
            Value::Long(_) | Value::Null(Some(TypeName::Long)) => NumericKind::Long,
            Value::Null(_) => {
                return Err(runtime_exception(
                    "NullPointerException",
                    "increment/decrement requires a non-null Integer or Long value",
                    operator_span,
                ));
            }
            _ => return Err(invalid_runtime_operands(operator_span)),
        };
        let new = numeric::increment(kind, old.clone(), delta, operator_span)?;
        self.write_place(&place, new.clone(), operator_span)?;
        Ok(if return_old { old } else { new })
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
        if self.read_only_collections.contains(&id) {
            return Err(runtime_exception(
                "FinalException",
                "Trigger context collections are read-only",
                span,
            ));
        }
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
            Value::Long(_) => matches!(target, TypeName::Long),
            Value::Decimal(_) => matches!(target, TypeName::Decimal),
            Value::Double(_) => matches!(target, TypeName::Double),
            Value::Date(_) => matches!(target, TypeName::Date),
            Value::Datetime(_) => matches!(target, TypeName::Datetime),
            Value::Time(_) => matches!(target, TypeName::Time),
            Value::Id(_) => matches!(target, TypeName::Id),
            Value::Platform(id) => self.store.platform(*id).ty() == *target,
            Value::Collection(id) => self.collection_type(*id) == *target,
            Value::Object(id) => {
                let class_id = self.store.object(*id).class_id;
                if matches!(target, TypeName::Callable) {
                    return self.program().callable_contract(class_id).is_some();
                }
                let TypeName::Custom(target) = target else {
                    return false;
                };
                let target_id = self.runtime_class_id(target);
                target_id
                    .is_some_and(|target_id| self.runtime_class_is_or_inherits(class_id, target_id))
            }
            Value::Enum { class_id, .. } => {
                matches!(
                    target,
                    TypeName::Custom(name)
                        if self.classes()[class_id.index()].qualified_name.canonical == name.canonical
                )
            }
            Value::TypeLiteral(_) => matches!(target, TypeName::Type),
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
                    || actual
                        .api_name()
                        .eq_ignore_ascii_case(crate::hir::schema_api_name(target))
            }
            Value::AggregateResult(_) => matches!(target, TypeName::AggregateResult),
            Value::Exception(exception) => self.exception_matches(exception, target),
            Value::Null(ty) => ty.as_ref().is_none_or(|ty| ty == target),
            Value::Void => false,
        }
    }

    fn value_type_name(&self, value: &Value) -> String {
        match value {
            Value::String(_) => TypeName::String.apex_name(),
            Value::Boolean(_) => TypeName::Boolean.apex_name(),
            Value::Integer(_) => TypeName::Integer.apex_name(),
            Value::Long(_) => TypeName::Long.apex_name(),
            Value::Decimal(_) => TypeName::Decimal.apex_name(),
            Value::Double(_) => TypeName::Double.apex_name(),
            Value::Date(_) => TypeName::Date.apex_name(),
            Value::Datetime(_) => TypeName::Datetime.apex_name(),
            Value::Time(_) => TypeName::Time.apex_name(),
            Value::Id(_) => TypeName::Id.apex_name(),
            Value::Platform(id) => self.store.platform(*id).ty().apex_name(),
            Value::Collection(id) => self.collection_type(*id).apex_name(),
            Value::Object(id) => self.classes()[self.store.object(*id).class_id]
                .qualified_name
                .spelling
                .clone(),
            Value::Enum { class_id, .. } => self.classes()[class_id.index()]
                .qualified_name
                .spelling
                .clone(),
            Value::TypeLiteral(_) => TypeName::Type.apex_name(),
            Value::SObject(id) => self
                .program()
                .schema()
                .object_at(self.store.sobject(*id).object_id)
                .expect("runtime SObject schema index is valid")
                .api_name()
                .to_owned(),
            Value::AggregateResult(_) => TypeName::AggregateResult.apex_name(),
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
            self.runtime_class_id(interface)
                .is_some_and(|id| self.runtime_class_is_or_inherits(id, expected))
        }) {
            return true;
        }
        self.classes()[actual]
            .superclass
            .as_ref()
            .and_then(|parent| self.runtime_class_id(parent))
            .is_some_and(|parent| self.runtime_class_is_or_inherits(parent, expected))
    }

    fn exception_matches(&self, exception: &Diagnostic, catch_type: &TypeName) -> bool {
        let Some(exception_type) = exception.exception_type.as_deref() else {
            return false;
        };
        if matches!(catch_type, TypeName::Exception)
            || exception_type.eq_ignore_ascii_case(&catch_type.apex_name())
        {
            return true;
        }
        let TypeName::Custom(catch_name) = catch_type else {
            return false;
        };
        let actual = self.classes().iter().position(|class| {
            class
                .qualified_name
                .spelling
                .eq_ignore_ascii_case(exception_type)
        });
        let expected = self.runtime_class_id(catch_name);
        actual
            .zip(expected)
            .is_some_and(|(actual, expected)| self.runtime_class_is_or_inherits(actual, expected))
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

fn require_no_constructor_arguments(
    arguments: &[Expression],
    span: Span,
) -> Result<(), Diagnostic> {
    if arguments.is_empty() {
        Ok(())
    } else {
        Err(Diagnostic::new(
            "platform constructor received unexpected arguments",
            span,
        ))
    }
}

fn typed_value(value: Value, ty: &TypeName) -> Value {
    match value {
        Value::Null(_) => Value::Null(Some(ty.clone())),
        Value::Integer(value) if *ty == TypeName::Long => Value::Long(value),
        Value::Integer(value) | Value::Long(value) if *ty == TypeName::Decimal => {
            Value::Decimal(Decimal::from(value))
        }
        Value::Integer(value) | Value::Long(value) if *ty == TypeName::Double => {
            Value::Double(ApexDouble(value as f64))
        }
        Value::Decimal(value) if *ty == TypeName::Double => value
            .normalize()
            .to_string()
            .parse::<f64>()
            .ok()
            .and_then(ApexDouble::new)
            .map_or(Value::Decimal(value), Value::Double),
        value => value,
    }
}

fn double_value(value: &Value, span: Span) -> Result<f64, Diagnostic> {
    match value {
        Value::Integer(value) | Value::Long(value) => Ok(*value as f64),
        Value::Decimal(value) => value
            .normalize()
            .to_string()
            .parse::<f64>()
            .map_err(|_| invalid_runtime_operands(span)),
        Value::Double(value) => Ok(value.get()),
        Value::Null(_) => Err(runtime_exception(
            "NullPointerException",
            "operator cannot be applied to null at runtime",
            span,
        )),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn decimal_value(value: &Value, span: Span) -> Result<Decimal, Diagnostic> {
    match value {
        Value::Integer(value) => Ok(Decimal::from(*value)),
        Value::Long(value) => Ok(Decimal::from(*value)),
        Value::Decimal(value) => Ok(*value),
        Value::Null(_) => Err(runtime_exception(
            "NullPointerException",
            "operator cannot be applied to null at runtime",
            span,
        )),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn compare_values(
    left: Value,
    right: Value,
    span: Span,
    comparison: impl FnOnce(Ordering) -> bool,
) -> Result<Value, Diagnostic> {
    let ordering = match (&left, &right) {
        (
            Value::Integer(_) | Value::Long(_) | Value::Decimal(_) | Value::Double(_),
            Value::Integer(_) | Value::Long(_) | Value::Decimal(_) | Value::Double(_),
        ) if matches!(left, Value::Double(_)) || matches!(right, Value::Double(_)) => {
            double_value(&left, span)?
                .partial_cmp(&double_value(&right, span)?)
                .ok_or_else(|| invalid_runtime_operands(span))?
        }
        (
            Value::Integer(_) | Value::Long(_) | Value::Decimal(_),
            Value::Integer(_) | Value::Long(_) | Value::Decimal(_),
        ) => decimal_value(&left, span)?.cmp(&decimal_value(&right, span)?),
        (Value::Date(left), Value::Date(right)) => left.cmp(right),
        (Value::Datetime(left), Value::Datetime(right)) => left.cmp(right),
        (Value::Time(left), Value::Time(right)) => left.cmp(right),
        (Value::Null(_), _) | (_, Value::Null(_)) => {
            return Err(runtime_exception(
                "NullPointerException",
                "operator cannot be applied to null at runtime",
                span,
            ));
        }
        _ => return Err(invalid_runtime_operands(span)),
    };
    Ok(Value::Boolean(comparison(ordering)))
}

fn apex_field_type(field_type: &FieldType) -> TypeName {
    match field_type {
        FieldType::Boolean => TypeName::Boolean,
        FieldType::Integer => TypeName::Integer,
        FieldType::String
        | FieldType::Id
        | FieldType::Reference { .. }
        | FieldType::MetadataRelationship { .. } => TypeName::String,
        FieldType::Summary { result_type, .. } => apex_field_type(result_type),
        FieldType::Date => TypeName::Date,
        FieldType::Datetime => TypeName::Datetime,
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

fn enum_value(class_id: ClassId, ordinal: usize) -> Value {
    Value::Enum { class_id, ordinal }
}

fn invalid_member_call(message: &str, span: Span) -> Result<Value, Diagnostic> {
    Err(Diagnostic::new(message, span))
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
