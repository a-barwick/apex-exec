use crate::span::Span;
use std::hash::{Hash, Hasher};

pub mod visit;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub classes: Vec<ClassDeclaration>,
    pub triggers: Vec<TriggerDeclaration>,
    pub methods: Vec<MethodDeclaration>,
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerDeclaration {
    pub name: Identifier,
    pub object: NamedType,
    pub events: Vec<TriggerEvent>,
    pub body: Statement,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TriggerEvent {
    BeforeInsert,
    BeforeUpdate,
    BeforeDelete,
    BeforeUndelete,
    AfterInsert,
    AfterUpdate,
    AfterDelete,
    AfterUndelete,
}

impl TriggerEvent {
    pub fn is_before(self) -> bool {
        matches!(
            self,
            Self::BeforeInsert | Self::BeforeUpdate | Self::BeforeDelete | Self::BeforeUndelete
        )
    }

    pub fn operation(self) -> DmlOperation {
        match self {
            Self::BeforeInsert | Self::AfterInsert => DmlOperation::Insert,
            Self::BeforeUpdate | Self::AfterUpdate => DmlOperation::Update,
            Self::BeforeDelete | Self::AfterDelete => DmlOperation::Delete,
            Self::BeforeUndelete | Self::AfterUndelete => DmlOperation::Undelete,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDeclaration {
    pub annotations: Vec<Annotation>,
    pub kind: ClassKind,
    pub modifiers: Vec<Modifier>,
    pub name: Identifier,
    /// Canonical source-qualified identity, including every enclosing type.
    pub qualified_name: NamedType,
    /// Qualified identity of the lexical owner for a nested declaration.
    pub enclosing_type: Option<NamedType>,
    pub superclass: Option<NamedType>,
    pub interfaces: Vec<NamedType>,
    pub enum_constants: Vec<Identifier>,
    pub members: Vec<ClassMember>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Interface,
    Enum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Modifier {
    Public,
    Private,
    Protected,
    Global,
    Static,
    Virtual,
    Abstract,
    Override,
    Final,
    WithSharing,
    WithoutSharing,
    InheritedSharing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassMember {
    Field(FieldDeclaration),
    Property(PropertyDeclaration),
    Constructor(ConstructorDeclaration),
    Method(MethodDeclaration),
    Initializer(InitializerBlock),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitializerBlock {
    pub is_static: bool,
    pub body: Statement,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDeclaration {
    pub modifiers: Vec<Modifier>,
    pub ty: TypeName,
    pub name: Identifier,
    pub initializer: Option<Expression>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyDeclaration {
    pub modifiers: Vec<Modifier>,
    pub ty: TypeName,
    pub name: Identifier,
    pub accessors: Vec<PropertyAccessor>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyAccessor {
    pub kind: AccessorKind,
    pub modifier: Option<Modifier>,
    pub body: Option<Statement>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessorKind {
    Get,
    Set,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructorDeclaration {
    pub modifiers: Vec<Modifier>,
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub delegation: Option<ConstructorDelegation>,
    pub body: Statement,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructorDelegation {
    pub kind: ConstructorDelegationKind,
    pub arguments: Vec<Expression>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstructorDelegationKind {
    This,
    Super,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodDeclaration {
    pub annotations: Vec<Annotation>,
    pub modifiers: Vec<Modifier>,
    pub return_type: ReturnType,
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub body: Option<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Annotation {
    pub kind: AnnotationKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnotationKind {
    IsTest { see_all_data: Option<bool> },
    TestSetup,
    Future,
}

impl AnnotationKind {
    pub fn is_test(self) -> bool {
        matches!(self, Self::IsTest { .. })
    }

    pub fn is_test_setup(self) -> bool {
        matches!(self, Self::TestSetup)
    }

    pub fn is_future(self) -> bool {
        matches!(self, Self::Future)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub ty: TypeName,
    pub name: Identifier,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReturnType {
    Void,
    Value(TypeName),
}

impl ReturnType {
    pub fn apex_name(&self) -> String {
        match self {
            Self::Void => "void".to_owned(),
            Self::Value(ty) => ty.apex_name(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatchClause {
    pub exception_type: TypeName,
    pub name: Identifier,
    pub body: Statement,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    VariableDeclaration {
        ty: TypeName,
        name: Identifier,
        initializer: Expression,
        span: Span,
    },
    Expression {
        expression: Expression,
        span: Span,
    },
    Block {
        statements: Vec<Statement>,
        span: Span,
    },
    If {
        condition: Expression,
        then_branch: Box<Statement>,
        else_branch: Option<Box<Statement>>,
        span: Span,
    },
    While {
        condition: Expression,
        body: Box<Statement>,
        span: Span,
    },
    DoWhile {
        body: Box<Statement>,
        condition: Expression,
        span: Span,
    },
    For {
        initializer: Option<Box<Statement>>,
        condition: Option<Expression>,
        update: Option<Box<Statement>>,
        body: Box<Statement>,
        span: Span,
    },
    ForEach {
        element_type: TypeName,
        name: Identifier,
        iterable: Expression,
        body: Box<Statement>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Try {
        try_block: Box<Statement>,
        catches: Vec<CatchClause>,
        finally_block: Option<Box<Statement>>,
        span: Span,
    },
    Throw {
        value: Expression,
        span: Span,
    },
    Dml {
        operation: DmlOperation,
        value: Expression,
        span: Span,
    },
    Return {
        value: Option<Expression>,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expression {
    StringLiteral(String, Span),
    BooleanLiteral(bool, Span),
    IntegerLiteral(i64, Span),
    LongLiteral(i128, Span),
    DecimalLiteral(String, Span),
    NullLiteral(Span),
    Soql(Box<SoqlQuery>),
    Sosl(Box<SoslQuery>),
    Variable(Identifier),
    TypeLiteral {
        ty: TypeName,
        span: Span,
    },
    Assignment {
        target: AssignmentTarget,
        operator: AssignmentOperator,
        operator_span: Span,
        value: Box<Expression>,
        span: Span,
    },
    NewCollection {
        ty: TypeName,
        initializer: CollectionInitializer,
        span: Span,
    },
    NewException {
        exception_type: TypeName,
        arguments: Vec<Expression>,
        span: Span,
    },
    NewObject {
        ty: TypeName,
        arguments: Vec<Expression>,
        span: Span,
    },
    Index {
        collection: Box<Expression>,
        index: Box<Expression>,
        span: Span,
    },
    FunctionCall {
        name: Identifier,
        arguments: Vec<Expression>,
        span: Span,
    },
    MethodCall {
        receiver: Box<Expression>,
        method: Identifier,
        arguments: Vec<Expression>,
        safe_navigation: bool,
        navigation_span: Span,
        span: Span,
    },
    MemberAccess {
        receiver: Box<Expression>,
        member: Identifier,
        safe_navigation: bool,
        navigation_span: Span,
        span: Span,
    },
    Cast {
        ty: TypeName,
        expression: Box<Expression>,
        span: Span,
    },
    Conditional {
        condition: Box<Expression>,
        when_true: Box<Expression>,
        when_false: Box<Expression>,
        question_span: Span,
        span: Span,
    },
    NullCoalesce {
        left: Box<Expression>,
        right: Box<Expression>,
        operator_span: Span,
        span: Span,
    },
    Instanceof {
        value: Box<Expression>,
        target: TypeName,
        target_span: Span,
        operator_span: Span,
        span: Span,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
        operator_span: Span,
        span: Span,
    },
    Postfix {
        operand: Box<Expression>,
        operator: PostfixOperator,
        operator_span: Span,
        span: Span,
    },
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
        operator_span: Span,
        span: Span,
    },
}

impl Expression {
    pub fn span(&self) -> Span {
        match self {
            Self::StringLiteral(_, span)
            | Self::BooleanLiteral(_, span)
            | Self::IntegerLiteral(_, span)
            | Self::LongLiteral(_, span)
            | Self::DecimalLiteral(_, span)
            | Self::NullLiteral(span)
            | Self::TypeLiteral { span, .. }
            | Self::Assignment { span, .. }
            | Self::NewCollection { span, .. }
            | Self::NewException { span, .. }
            | Self::NewObject { span, .. }
            | Self::Index { span, .. }
            | Self::FunctionCall { span, .. }
            | Self::MethodCall { span, .. }
            | Self::MemberAccess { span, .. }
            | Self::Cast { span, .. }
            | Self::Conditional { span, .. }
            | Self::NullCoalesce { span, .. }
            | Self::Instanceof { span, .. }
            | Self::Unary { span, .. }
            | Self::Postfix { span, .. }
            | Self::Binary { span, .. } => *span,
            Self::Soql(query) => query.span,
            Self::Sosl(query) => query.span,
            Self::Variable(identifier) => identifier.span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmlOperation {
    Insert,
    Update,
    Upsert,
    Delete,
    Undelete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoqlQuery {
    pub select: Vec<SoqlSelectItem>,
    pub from: Identifier,
    pub where_clause: Option<SoqlCondition>,
    pub group_by: Vec<FieldPath>,
    pub order_by: Vec<SoqlOrderBy>,
    pub limit: Option<SoqlValue>,
    pub offset: Option<SoqlValue>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoqlSelectItem {
    Field(FieldPath),
    Aggregate {
        function: SoqlAggregateFunction,
        field: Option<FieldPath>,
        alias: Option<Identifier>,
        span: Span,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoqlAggregateFunction {
    Count,
    Sum,
    Min,
    Max,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldPath {
    pub segments: Vec<Identifier>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoqlCondition {
    Comparison {
        left: FieldPath,
        operator: SoqlComparisonOperator,
        right: SoqlValue,
        span: Span,
    },
    In {
        field: FieldPath,
        negated: bool,
        values: SoqlInValues,
        span: Span,
    },
    Not {
        condition: Box<SoqlCondition>,
        span: Span,
    },
    Logical {
        left: Box<SoqlCondition>,
        operator: SoqlLogicalOperator,
        right: Box<SoqlCondition>,
        span: Span,
    },
}

impl SoqlCondition {
    pub fn span(&self) -> Span {
        match self {
            Self::Comparison { span, .. }
            | Self::In { span, .. }
            | Self::Not { span, .. }
            | Self::Logical { span, .. } => *span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoqlComparisonOperator {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Like,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoqlLogicalOperator {
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoqlInValues {
    Values(Vec<SoqlValue>),
    Bind(Box<Expression>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoqlValue {
    String(String, Span),
    Boolean(bool, Span),
    Integer(i64, Span),
    Null(Span),
    Bind(Box<Expression>, Span),
}

impl SoqlValue {
    pub fn span(&self) -> Span {
        match self {
            Self::String(_, span)
            | Self::Boolean(_, span)
            | Self::Integer(_, span)
            | Self::Null(span)
            | Self::Bind(_, span) => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoqlOrderBy {
    pub field: FieldPath,
    pub direction: SortDirection,
    pub nulls: Option<NullsOrder>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullsOrder {
    First,
    Last,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoslQuery {
    pub search: SoqlValue,
    pub scope: SoslScope,
    pub returning: Vec<SoslReturning>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoslScope {
    AllFields,
    NameFields,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoslReturning {
    pub object: Identifier,
    pub fields: Vec<FieldPath>,
    pub where_clause: Option<SoqlCondition>,
    pub order_by: Vec<SoqlOrderBy>,
    pub limit: Option<SoqlValue>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignmentTarget {
    Variable(Identifier),
    Index {
        collection: Box<Expression>,
        index: Box<Expression>,
        span: Span,
    },
    Member {
        receiver: Box<Expression>,
        member: Identifier,
        span: Span,
    },
}

impl AssignmentTarget {
    pub fn span(&self) -> Span {
        match self {
            Self::Variable(identifier) => identifier.span,
            Self::Index { span, .. } | Self::Member { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollectionInitializer {
    Arguments(Vec<Expression>),
    Elements(Vec<Expression>),
    MapEntries(Vec<MapEntry>),
    SizedArray(Box<Expression>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapEntry {
    pub key: Expression,
    pub value: Expression,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOperator {
    Positive,
    Negate,
    Not,
    BitwiseNot,
    PrefixIncrement,
    PrefixDecrement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostfixOperator {
    Increment,
    Decrement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    ShiftLeft,
    ShiftRight,
    UnsignedShiftRight,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignmentOperator {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    ShiftLeft,
    ShiftRight,
    UnsignedShiftRight,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Identifier {
    pub spelling: String,
    pub canonical: String,
    pub span: Span,
}

impl Identifier {
    pub fn new(spelling: String, span: Span) -> Self {
        let canonical = spelling.to_ascii_lowercase();
        Self {
            spelling,
            canonical,
            span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeName {
    String,
    Boolean,
    Integer,
    Long,
    Decimal,
    Date,
    Datetime,
    Time,
    Id,
    Blob,
    Object,
    Pattern,
    Matcher,
    Http,
    HttpRequest,
    HttpResponse,
    QueueableContext,
    BatchableContext,
    SchedulableContext,
    SObjectType,
    DescribeSObjectResult,
    Exception,
    NullPointerException,
    ListException,
    MathException,
    TypeException,
    StringException,
    IllegalArgumentException,
    FinalException,
    AssertException,
    QueryException,
    DmlException,
    AsyncException,
    AggregateResult,
    Type,
    Custom(NamedType),
    List(Box<TypeName>),
    Set(Box<TypeName>),
    Map(Box<TypeName>, Box<TypeName>),
    Iterable(Box<TypeName>),
}

impl TypeName {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "string" => Some(Self::String),
            "boolean" => Some(Self::Boolean),
            "integer" => Some(Self::Integer),
            "long" => Some(Self::Long),
            "decimal" => Some(Self::Decimal),
            "date" => Some(Self::Date),
            "datetime" => Some(Self::Datetime),
            "time" => Some(Self::Time),
            "id" => Some(Self::Id),
            "blob" => Some(Self::Blob),
            "object" => Some(Self::Object),
            "pattern" => Some(Self::Pattern),
            "matcher" => Some(Self::Matcher),
            "http" => Some(Self::Http),
            "httprequest" => Some(Self::HttpRequest),
            "httpresponse" => Some(Self::HttpResponse),
            "queueablecontext" | "system.queueablecontext" => Some(Self::QueueableContext),
            "batchablecontext" | "database.batchablecontext" => Some(Self::BatchableContext),
            "schedulablecontext" | "system.schedulablecontext" => Some(Self::SchedulableContext),
            "sobjecttype" | "schema.sobjecttype" => Some(Self::SObjectType),
            "describesobjectresult" | "schema.describesobjectresult" => {
                Some(Self::DescribeSObjectResult)
            }
            "exception" => Some(Self::Exception),
            "nullpointerexception" => Some(Self::NullPointerException),
            "listexception" => Some(Self::ListException),
            "mathexception" => Some(Self::MathException),
            "typeexception" => Some(Self::TypeException),
            "stringexception" => Some(Self::StringException),
            "illegalargumentexception" => Some(Self::IllegalArgumentException),
            "finalexception" => Some(Self::FinalException),
            "assertexception" => Some(Self::AssertException),
            "queryexception" => Some(Self::QueryException),
            "dmlexception" => Some(Self::DmlException),
            "asyncexception" => Some(Self::AsyncException),
            "aggregateresult" => Some(Self::AggregateResult),
            "type" | "system.type" => Some(Self::Type),
            _ => None,
        }
    }

    pub fn is_exception(&self) -> bool {
        matches!(
            self,
            Self::Exception
                | Self::NullPointerException
                | Self::ListException
                | Self::MathException
                | Self::TypeException
                | Self::StringException
                | Self::IllegalArgumentException
                | Self::FinalException
                | Self::AssertException
                | Self::QueryException
                | Self::DmlException
                | Self::AsyncException
        )
    }

    pub fn apex_name(&self) -> String {
        match self {
            Self::String => "String".to_owned(),
            Self::Boolean => "Boolean".to_owned(),
            Self::Integer => "Integer".to_owned(),
            Self::Long => "Long".to_owned(),
            Self::Decimal => "Decimal".to_owned(),
            Self::Date => "Date".to_owned(),
            Self::Datetime => "Datetime".to_owned(),
            Self::Time => "Time".to_owned(),
            Self::Id => "Id".to_owned(),
            Self::Blob => "Blob".to_owned(),
            Self::Object => "Object".to_owned(),
            Self::Pattern => "Pattern".to_owned(),
            Self::Matcher => "Matcher".to_owned(),
            Self::Http => "Http".to_owned(),
            Self::HttpRequest => "HttpRequest".to_owned(),
            Self::HttpResponse => "HttpResponse".to_owned(),
            Self::QueueableContext => "System.QueueableContext".to_owned(),
            Self::BatchableContext => "Database.BatchableContext".to_owned(),
            Self::SchedulableContext => "System.SchedulableContext".to_owned(),
            Self::SObjectType => "Schema.SObjectType".to_owned(),
            Self::DescribeSObjectResult => "Schema.DescribeSObjectResult".to_owned(),
            Self::Exception => "Exception".to_owned(),
            Self::NullPointerException => "NullPointerException".to_owned(),
            Self::ListException => "ListException".to_owned(),
            Self::MathException => "MathException".to_owned(),
            Self::TypeException => "TypeException".to_owned(),
            Self::StringException => "StringException".to_owned(),
            Self::IllegalArgumentException => "IllegalArgumentException".to_owned(),
            Self::FinalException => "FinalException".to_owned(),
            Self::AssertException => "AssertException".to_owned(),
            Self::QueryException => "QueryException".to_owned(),
            Self::DmlException => "DmlException".to_owned(),
            Self::AsyncException => "AsyncException".to_owned(),
            Self::AggregateResult => "AggregateResult".to_owned(),
            Self::Type => "System.Type".to_owned(),
            Self::Custom(name) => name.spelling.clone(),
            Self::List(element) => format!("List<{}>", element.apex_name()),
            Self::Set(element) => format!("Set<{}>", element.apex_name()),
            Self::Map(key, value) => {
                format!("Map<{},{}>", key.apex_name(), value.apex_name())
            }
            Self::Iterable(element) => format!("Iterable<{}>", element.apex_name()),
        }
    }
}

/// Lossless source syntax for one Apex type reference.
///
/// Semantic analysis continues to expose [`TypeName`] as a transitional view,
/// but parser lookahead and every parsed generic/hierarchy reference share this
/// single grammar and retain spelling, segment spans, argument structure, and
/// array suffix spans.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeRef {
    pub segments: Vec<Identifier>,
    pub type_arguments: Vec<TypeRef>,
    pub array_suffixes: Vec<Span>,
    pub span: Span,
}

impl TypeRef {
    pub fn spelling(&self) -> String {
        self.segments
            .iter()
            .map(|segment| segment.spelling.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }

    pub fn canonical(&self) -> String {
        self.segments
            .iter()
            .map(|segment| segment.canonical.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// A preserved generic argument on a named hierarchy or trigger type.
#[derive(Clone, Debug)]
pub struct TypeArgument {
    /// Lossless argument syntax.
    pub syntax: TypeRef,
    /// Parsed argument type.
    pub ty: TypeName,
    /// Exact source span of the argument syntax.
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct NamedType {
    pub spelling: String,
    pub canonical: String,
    /// Lossless parsed syntax when this name came from source.
    pub syntax: Option<TypeRef>,
    /// Generic arguments preserved from source for semantic validation.
    pub type_arguments: Vec<TypeArgument>,
    pub span: Span,
}

impl NamedType {
    pub fn new(spelling: String, span: Span) -> Self {
        Self::with_type_arguments(spelling, Vec::new(), span)
    }

    pub fn with_type_arguments(
        spelling: String,
        type_arguments: Vec<TypeArgument>,
        span: Span,
    ) -> Self {
        let canonical = spelling.to_ascii_lowercase();
        Self {
            spelling,
            canonical,
            syntax: None,
            type_arguments,
            span,
        }
    }

    pub fn from_type_ref(syntax: TypeRef, type_arguments: Vec<TypeArgument>) -> Self {
        let spelling = syntax.spelling();
        let canonical = syntax.canonical();
        let span = syntax.span;
        Self {
            spelling,
            canonical,
            syntax: Some(syntax),
            type_arguments,
            span,
        }
    }
}

impl PartialEq for NamedType {
    fn eq(&self, other: &Self) -> bool {
        self.canonical == other.canonical
            && self
                .type_arguments
                .iter()
                .map(|argument| &argument.ty)
                .eq(other.type_arguments.iter().map(|argument| &argument.ty))
    }
}

impl Eq for NamedType {}

impl Hash for NamedType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.canonical.hash(state);
        self.type_arguments.len().hash(state);
        for argument in &self.type_arguments {
            argument.ty.hash(state);
        }
    }
}

impl Statement {
    pub fn span(&self) -> Span {
        match self {
            Self::VariableDeclaration { span, .. }
            | Self::Expression { span, .. }
            | Self::Block { span, .. }
            | Self::If { span, .. }
            | Self::While { span, .. }
            | Self::DoWhile { span, .. }
            | Self::For { span, .. }
            | Self::ForEach { span, .. }
            | Self::Break { span }
            | Self::Continue { span }
            | Self::Try { span, .. }
            | Self::Throw { span, .. }
            | Self::Dml { span, .. }
            | Self::Return { span, .. } => *span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_core_exception_types_case_insensitively() {
        for name in [
            "Exception",
            "NullPointerException",
            "ListException",
            "MathException",
            "TypeException",
            "StringException",
            "IllegalArgumentException",
            "FinalException",
            "AssertException",
            "QueryException",
            "DmlException",
        ] {
            let ty = TypeName::from_apex_name(&name.to_ascii_uppercase())
                .expect("core exception should be a known type");
            assert!(ty.is_exception());
            assert_eq!(ty.apex_name(), name);
        }

        assert_eq!(TypeName::from_apex_name("OBJECT"), Some(TypeName::Object));
        assert!(!TypeName::Object.is_exception());
    }
}
