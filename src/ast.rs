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
    Transient,
    WithSharing,
    WithoutSharing,
    InheritedSharing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassMember {
    Field(FieldDeclaration),
    FieldGroup(FieldGroupDeclaration),
    Property(PropertyDeclaration),
    Constructor(ConstructorDeclaration),
    Method(MethodDeclaration),
    Initializer(InitializerBlock),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldGroupDeclaration {
    pub annotations: Vec<Annotation>,
    pub modifiers: Vec<Modifier>,
    pub ty: TypeName,
    pub declarators: Vec<VariableDeclarator>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitializerBlock {
    pub is_static: bool,
    pub body: Statement,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDeclaration {
    pub annotations: Vec<Annotation>,
    pub modifiers: Vec<Modifier>,
    pub ty: TypeName,
    pub name: Identifier,
    pub initializer: Option<Expression>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyDeclaration {
    pub annotations: Vec<Annotation>,
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
    pub annotations: Vec<Annotation>,
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
    pub name: Identifier,
    pub arguments: Vec<AnnotationArgument>,
    pub kind: AnnotationKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnnotationArgument {
    pub name: Option<Identifier>,
    pub value: Expression,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnotationKind {
    IsTest {
        see_all_data: Option<bool>,
        is_parallel: Option<bool>,
    },
    TestSetup,
    Future,
    AuraEnabled {
        cacheable: Option<bool>,
        continuation: Option<bool>,
    },
    SuppressWarnings,
    TestVisible,
    Other,
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

    pub fn test_parallelism(self) -> Option<bool> {
        match self {
            Self::IsTest { is_parallel, .. } => is_parallel,
            _ => None,
        }
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
pub struct VariableDeclarator {
    pub name: Identifier,
    pub initializer: Option<Expression>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchArm {
    pub labels: SwitchLabels,
    pub body: Statement,
    pub span: Span,
}

/// A typed `switch when` label that binds a concrete SObject pattern.
///
/// Kept behind `SwitchLabels::TypePattern`'s boxed boundary so scalar switch
/// labels do not inherit the storage cost of a full `TypeName`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchTypePattern {
    pub ty: TypeName,
    pub binding: Identifier,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchLabels {
    Expressions(Vec<Expression>),
    TypePattern(Box<SwitchTypePattern>),
    Else(Span),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    VariableDeclaration {
        ty: TypeName,
        name: Identifier,
        initializer: Expression,
        span: Span,
    },
    LocalDeclaration {
        modifiers: Vec<Modifier>,
        ty: TypeName,
        declarators: Vec<VariableDeclarator>,
        span: Span,
    },
    Sequence {
        statements: Vec<Statement>,
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
    Switch {
        value: Expression,
        arms: Vec<SwitchArm>,
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
    RunAs {
        user: Expression,
        body: Box<Statement>,
        span: Span,
    },
    Dml {
        operation: DmlOperation,
        access: DmlAccess,
        value: Expression,
        external_id: Option<Identifier>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DmlAccess {
    #[default]
    Default,
    UserMode,
    SystemMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoqlQuery {
    pub select: Vec<SoqlSelectItem>,
    pub from: Identifier,
    pub where_clause: Option<SoqlCondition>,
    pub access: SoqlAccess,
    pub group_by: Vec<FieldPath>,
    pub having: Option<SoqlCondition>,
    pub order_by: Vec<SoqlOrderBy>,
    pub limit: Option<SoqlValue>,
    pub offset: Option<SoqlValue>,
    pub all_rows: bool,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SoqlAccess {
    #[default]
    Default,
    SecurityEnforced,
    UserMode,
    SystemMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoqlSelectItem {
    Field(FieldPath),
    Subquery {
        query: Box<SoqlQuery>,
        span: Span,
    },
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
    AggregateComparison {
        function: SoqlAggregateFunction,
        field: Option<FieldPath>,
        operator: SoqlComparisonOperator,
        right: SoqlValue,
        span: Span,
    },
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
            Self::AggregateComparison { span, .. }
            | Self::Comparison { span, .. }
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
    DateLiteral(SoqlDateLiteral),
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
            Self::DateLiteral(literal) => literal.span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoqlDateLiteral {
    pub kind: SoqlDateLiteralKind,
    pub amount: Option<i64>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoqlDateLiteralKind {
    Yesterday,
    Today,
    Tomorrow,
    LastNDays,
    NextNDays,
    ThisWeek,
    LastWeek,
    NextWeek,
    ThisMonth,
    LastMonth,
    NextMonth,
    ThisYear,
    LastYear,
    NextYear,
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
    ExactEqual,
    ExactNotEqual,
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
    Double,
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
    HttpCalloutMock,
    Callable,
    Queueable,
    QueueableContext,
    BatchableContext,
    FinalizerContext,
    ParentJobResult,
    Quiddity,
    TriggerOperation,
    LoggingLevel,
    CacheVisibility,
    CachePartition,
    Request,
    QueryLocator,
    DmlOptions,
    SaveResult,
    UpsertResult,
    DeleteResult,
    UndeleteResult,
    DatabaseError,
    StatusCode,
    AccessLevel,
    AccessType,
    SObjectAccessDecision,
    SchedulableContext,
    SObjectType,
    DescribeSObjectResult,
    SObjectField,
    DescribeFieldResult,
    SObjectFieldMap,
    FieldSetMap,
    FieldSet,
    FieldSetMember,
    PicklistEntry,
    VisualEditorDataRow,
    VisualEditorDynamicPickListRows,
    SoapType,
    DisplayType,
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
    SObjectException,
    NoAccessException,
    AsyncException,
    OrgCacheException,
    SessionCacheException,
    AggregateResult,
    Type,
    Custom(NamedType),
    List(Box<TypeName>),
    Set(Box<TypeName>),
    Map(Box<TypeName>, Box<TypeName>),
    Iterable(Box<TypeName>),
}

struct BuiltInTypeSpec {
    apex_name: &'static str,
    aliases: &'static [&'static str],
    ty: TypeName,
}

const BUILTIN_TYPE_SPECS: &[BuiltInTypeSpec] = &[
    BuiltInTypeSpec {
        apex_name: "String",
        aliases: &["string", "system.string"],
        ty: TypeName::String,
    },
    BuiltInTypeSpec {
        apex_name: "Boolean",
        aliases: &["boolean", "system.boolean"],
        ty: TypeName::Boolean,
    },
    BuiltInTypeSpec {
        apex_name: "Integer",
        aliases: &["integer", "system.integer"],
        ty: TypeName::Integer,
    },
    BuiltInTypeSpec {
        apex_name: "Long",
        aliases: &["long", "system.long"],
        ty: TypeName::Long,
    },
    BuiltInTypeSpec {
        apex_name: "Decimal",
        aliases: &["decimal", "system.decimal"],
        ty: TypeName::Decimal,
    },
    BuiltInTypeSpec {
        apex_name: "Double",
        aliases: &["double", "system.double"],
        ty: TypeName::Double,
    },
    BuiltInTypeSpec {
        apex_name: "Date",
        aliases: &["date", "system.date"],
        ty: TypeName::Date,
    },
    BuiltInTypeSpec {
        apex_name: "Datetime",
        aliases: &["datetime", "system.datetime"],
        ty: TypeName::Datetime,
    },
    BuiltInTypeSpec {
        apex_name: "Time",
        aliases: &["time", "system.time"],
        ty: TypeName::Time,
    },
    BuiltInTypeSpec {
        apex_name: "Id",
        aliases: &["id", "system.id"],
        ty: TypeName::Id,
    },
    BuiltInTypeSpec {
        apex_name: "Blob",
        aliases: &["blob", "system.blob"],
        ty: TypeName::Blob,
    },
    BuiltInTypeSpec {
        apex_name: "Object",
        aliases: &["object", "system.object"],
        ty: TypeName::Object,
    },
    BuiltInTypeSpec {
        apex_name: "Pattern",
        aliases: &["pattern", "system.pattern"],
        ty: TypeName::Pattern,
    },
    BuiltInTypeSpec {
        apex_name: "Matcher",
        aliases: &["matcher", "system.matcher"],
        ty: TypeName::Matcher,
    },
    BuiltInTypeSpec {
        apex_name: "Http",
        aliases: &["http", "system.http"],
        ty: TypeName::Http,
    },
    BuiltInTypeSpec {
        apex_name: "HttpRequest",
        aliases: &["httprequest", "system.httprequest"],
        ty: TypeName::HttpRequest,
    },
    BuiltInTypeSpec {
        apex_name: "HttpResponse",
        aliases: &["httpresponse", "system.httpresponse"],
        ty: TypeName::HttpResponse,
    },
    BuiltInTypeSpec {
        apex_name: "System.HttpCalloutMock",
        aliases: &["httpcalloutmock", "system.httpcalloutmock"],
        ty: TypeName::HttpCalloutMock,
    },
    BuiltInTypeSpec {
        apex_name: "System.Callable",
        aliases: &["callable", "system.callable"],
        ty: TypeName::Callable,
    },
    BuiltInTypeSpec {
        apex_name: "System.Queueable",
        aliases: &["queueable", "system.queueable"],
        ty: TypeName::Queueable,
    },
    BuiltInTypeSpec {
        apex_name: "System.QueueableContext",
        aliases: &["queueablecontext", "system.queueablecontext"],
        ty: TypeName::QueueableContext,
    },
    BuiltInTypeSpec {
        apex_name: "Database.BatchableContext",
        aliases: &["batchablecontext", "database.batchablecontext"],
        ty: TypeName::BatchableContext,
    },
    BuiltInTypeSpec {
        apex_name: "System.FinalizerContext",
        aliases: &["finalizercontext", "system.finalizercontext"],
        ty: TypeName::FinalizerContext,
    },
    BuiltInTypeSpec {
        apex_name: "System.ParentJobResult",
        aliases: &["parentjobresult", "system.parentjobresult"],
        ty: TypeName::ParentJobResult,
    },
    BuiltInTypeSpec {
        apex_name: "System.Quiddity",
        aliases: &["quiddity", "system.quiddity"],
        ty: TypeName::Quiddity,
    },
    BuiltInTypeSpec {
        apex_name: "System.TriggerOperation",
        aliases: &["triggeroperation", "system.triggeroperation"],
        ty: TypeName::TriggerOperation,
    },
    BuiltInTypeSpec {
        apex_name: "System.LoggingLevel",
        aliases: &["logginglevel", "system.logginglevel"],
        ty: TypeName::LoggingLevel,
    },
    BuiltInTypeSpec {
        apex_name: "Cache.Visibility",
        aliases: &["cache.visibility"],
        ty: TypeName::CacheVisibility,
    },
    BuiltInTypeSpec {
        apex_name: "Cache.Partition",
        aliases: &["cache.partition"],
        ty: TypeName::CachePartition,
    },
    BuiltInTypeSpec {
        apex_name: "System.Request",
        aliases: &["request", "system.request"],
        ty: TypeName::Request,
    },
    BuiltInTypeSpec {
        apex_name: "Database.QueryLocator",
        aliases: &["querylocator", "database.querylocator"],
        ty: TypeName::QueryLocator,
    },
    BuiltInTypeSpec {
        apex_name: "Database.DmlOptions",
        aliases: &["dmloptions", "database.dmloptions"],
        ty: TypeName::DmlOptions,
    },
    BuiltInTypeSpec {
        apex_name: "Database.SaveResult",
        aliases: &["saveresult", "database.saveresult"],
        ty: TypeName::SaveResult,
    },
    BuiltInTypeSpec {
        apex_name: "Database.UpsertResult",
        aliases: &["upsertresult", "database.upsertresult"],
        ty: TypeName::UpsertResult,
    },
    BuiltInTypeSpec {
        apex_name: "Database.DeleteResult",
        aliases: &["deleteresult", "database.deleteresult"],
        ty: TypeName::DeleteResult,
    },
    BuiltInTypeSpec {
        apex_name: "Database.UndeleteResult",
        aliases: &["undeleteresult", "database.undeleteresult"],
        ty: TypeName::UndeleteResult,
    },
    BuiltInTypeSpec {
        apex_name: "Database.Error",
        aliases: &["error", "database.error"],
        ty: TypeName::DatabaseError,
    },
    BuiltInTypeSpec {
        apex_name: "StatusCode",
        aliases: &["statuscode", "system.statuscode"],
        ty: TypeName::StatusCode,
    },
    BuiltInTypeSpec {
        apex_name: "AccessLevel",
        aliases: &["accesslevel", "system.accesslevel", "database.accesslevel"],
        ty: TypeName::AccessLevel,
    },
    BuiltInTypeSpec {
        apex_name: "System.AccessType",
        aliases: &["accesstype", "system.accesstype"],
        ty: TypeName::AccessType,
    },
    BuiltInTypeSpec {
        apex_name: "SObjectAccessDecision",
        aliases: &["sobjectaccessdecision", "system.sobjectaccessdecision"],
        ty: TypeName::SObjectAccessDecision,
    },
    BuiltInTypeSpec {
        apex_name: "System.SchedulableContext",
        aliases: &["schedulablecontext", "system.schedulablecontext"],
        ty: TypeName::SchedulableContext,
    },
    BuiltInTypeSpec {
        apex_name: "Schema.SObjectType",
        aliases: &["sobjecttype", "schema.sobjecttype"],
        ty: TypeName::SObjectType,
    },
    BuiltInTypeSpec {
        apex_name: "Schema.DescribeSObjectResult",
        aliases: &["describesobjectresult", "schema.describesobjectresult"],
        ty: TypeName::DescribeSObjectResult,
    },
    BuiltInTypeSpec {
        apex_name: "Schema.SObjectField",
        aliases: &["sobjectfield", "schema.sobjectfield"],
        ty: TypeName::SObjectField,
    },
    BuiltInTypeSpec {
        apex_name: "Schema.DescribeFieldResult",
        aliases: &["describefieldresult", "schema.describefieldresult"],
        ty: TypeName::DescribeFieldResult,
    },
    BuiltInTypeSpec {
        apex_name: "Schema.SObjectFieldMap",
        aliases: &["sobjectfieldmap", "schema.sobjectfieldmap"],
        ty: TypeName::SObjectFieldMap,
    },
    BuiltInTypeSpec {
        apex_name: "Schema.FieldSetMap",
        aliases: &["fieldsetmap", "schema.fieldsetmap"],
        ty: TypeName::FieldSetMap,
    },
    BuiltInTypeSpec {
        apex_name: "Schema.FieldSet",
        aliases: &["fieldset", "schema.fieldset"],
        ty: TypeName::FieldSet,
    },
    BuiltInTypeSpec {
        apex_name: "Schema.FieldSetMember",
        aliases: &["fieldsetmember", "schema.fieldsetmember"],
        ty: TypeName::FieldSetMember,
    },
    BuiltInTypeSpec {
        apex_name: "Schema.PicklistEntry",
        aliases: &["picklistentry", "schema.picklistentry"],
        ty: TypeName::PicklistEntry,
    },
    BuiltInTypeSpec {
        apex_name: "VisualEditor.DataRow",
        aliases: &["visualeditor.datarow"],
        ty: TypeName::VisualEditorDataRow,
    },
    BuiltInTypeSpec {
        apex_name: "VisualEditor.DynamicPickListRows",
        aliases: &["visualeditor.dynamicpicklistrows"],
        ty: TypeName::VisualEditorDynamicPickListRows,
    },
    BuiltInTypeSpec {
        apex_name: "Schema.SoapType",
        aliases: &["soaptype", "schema.soaptype"],
        ty: TypeName::SoapType,
    },
    BuiltInTypeSpec {
        apex_name: "Schema.DisplayType",
        aliases: &["displaytype", "schema.displaytype"],
        ty: TypeName::DisplayType,
    },
    BuiltInTypeSpec {
        apex_name: "Exception",
        aliases: &["exception", "system.exception"],
        ty: TypeName::Exception,
    },
    BuiltInTypeSpec {
        apex_name: "NullPointerException",
        aliases: &["nullpointerexception", "system.nullpointerexception"],
        ty: TypeName::NullPointerException,
    },
    BuiltInTypeSpec {
        apex_name: "ListException",
        aliases: &["listexception", "system.listexception"],
        ty: TypeName::ListException,
    },
    BuiltInTypeSpec {
        apex_name: "MathException",
        aliases: &["mathexception", "system.mathexception"],
        ty: TypeName::MathException,
    },
    BuiltInTypeSpec {
        apex_name: "TypeException",
        aliases: &["typeexception", "system.typeexception"],
        ty: TypeName::TypeException,
    },
    BuiltInTypeSpec {
        apex_name: "StringException",
        aliases: &["stringexception", "system.stringexception"],
        ty: TypeName::StringException,
    },
    BuiltInTypeSpec {
        apex_name: "IllegalArgumentException",
        aliases: &[
            "illegalargumentexception",
            "system.illegalargumentexception",
        ],
        ty: TypeName::IllegalArgumentException,
    },
    BuiltInTypeSpec {
        apex_name: "FinalException",
        aliases: &["finalexception", "system.finalexception"],
        ty: TypeName::FinalException,
    },
    BuiltInTypeSpec {
        apex_name: "AssertException",
        aliases: &["assertexception", "system.assertexception"],
        ty: TypeName::AssertException,
    },
    BuiltInTypeSpec {
        apex_name: "QueryException",
        aliases: &["queryexception", "system.queryexception"],
        ty: TypeName::QueryException,
    },
    BuiltInTypeSpec {
        apex_name: "DmlException",
        aliases: &["dmlexception", "system.dmlexception"],
        ty: TypeName::DmlException,
    },
    BuiltInTypeSpec {
        apex_name: "SObjectException",
        aliases: &["sobjectexception", "system.sobjectexception"],
        ty: TypeName::SObjectException,
    },
    BuiltInTypeSpec {
        apex_name: "NoAccessException",
        aliases: &["noaccessexception", "system.noaccessexception"],
        ty: TypeName::NoAccessException,
    },
    BuiltInTypeSpec {
        apex_name: "AsyncException",
        aliases: &["asyncexception", "system.asyncexception"],
        ty: TypeName::AsyncException,
    },
    BuiltInTypeSpec {
        apex_name: "Cache.Org.OrgCacheException",
        aliases: &["cache.org.orgcacheexception"],
        ty: TypeName::OrgCacheException,
    },
    BuiltInTypeSpec {
        apex_name: "Cache.Session.SessionCacheException",
        aliases: &["cache.session.sessioncacheexception"],
        ty: TypeName::SessionCacheException,
    },
    BuiltInTypeSpec {
        apex_name: "AggregateResult",
        aliases: &["aggregateresult", "system.aggregateresult"],
        ty: TypeName::AggregateResult,
    },
    BuiltInTypeSpec {
        apex_name: "System.Type",
        aliases: &["type", "system.type"],
        ty: TypeName::Type,
    },
];

impl TypeName {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        Self::built_in_type_spec(name).map(|spec| spec.ty.clone())
    }

    fn built_in_type_spec(name: &str) -> Option<&'static BuiltInTypeSpec> {
        BUILTIN_TYPE_SPECS.iter().find(|spec| {
            spec.aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(name))
        })
    }

    fn built_in_apex_name(&self) -> Option<&'static str> {
        BUILTIN_TYPE_SPECS
            .iter()
            .find(|spec| spec.ty == *self)
            .map(|spec| spec.apex_name)
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
                | Self::SObjectException
                | Self::NoAccessException
                | Self::AsyncException
                | Self::OrgCacheException
                | Self::SessionCacheException
        )
    }

    pub fn apex_name(&self) -> String {
        if let Some(name) = self.built_in_apex_name() {
            return name.to_owned();
        }
        match self {
            Self::Custom(name) => name.spelling.clone(),
            Self::List(element) => format!("List<{}>", element.apex_name()),
            Self::Set(element) => format!("Set<{}>", element.apex_name()),
            Self::Map(key, value) => {
                format!("Map<{},{}>", key.apex_name(), value.apex_name())
            }
            Self::Iterable(element) => format!("Iterable<{}>", element.apex_name()),
            _ => unreachable!("every unit TypeName variant has a built-in type spec"),
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
            | Self::LocalDeclaration { span, .. }
            | Self::Sequence { span, .. }
            | Self::Expression { span, .. }
            | Self::Block { span, .. }
            | Self::If { span, .. }
            | Self::While { span, .. }
            | Self::DoWhile { span, .. }
            | Self::Switch { span, .. }
            | Self::For { span, .. }
            | Self::ForEach { span, .. }
            | Self::Break { span }
            | Self::Continue { span }
            | Self::Try { span, .. }
            | Self::Throw { span, .. }
            | Self::RunAs { span, .. }
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
            "SObjectException",
            "NoAccessException",
        ] {
            let ty = TypeName::from_apex_name(&name.to_ascii_uppercase())
                .expect("core exception should be a known type");
            assert!(ty.is_exception());
            assert_eq!(ty.apex_name(), name);
        }

        assert_eq!(TypeName::from_apex_name("OBJECT"), Some(TypeName::Object));
        assert!(!TypeName::Object.is_exception());
    }

    #[test]
    fn built_in_type_specs_preserve_aliases_and_non_builtin_rendering() {
        for (alias, expected) in [
            ("sYsTeM.sTrInG", TypeName::String),
            ("DATABASE.ACCESSLEVEL", TypeName::AccessLevel),
            ("Cache.Org.OrgCacheException", TypeName::OrgCacheException),
            ("Schema.DescribeFieldResult", TypeName::DescribeFieldResult),
        ] {
            assert_eq!(TypeName::from_apex_name(alias), Some(expected));
        }

        assert_eq!(
            TypeName::HttpCalloutMock.apex_name(),
            "System.HttpCalloutMock"
        );
        assert_eq!(TypeName::AccessLevel.apex_name(), "AccessLevel");
        let custom = TypeName::Custom(NamedType::new("Domain.Widget".to_owned(), Span::new(0, 13)));
        let generic = TypeName::Map(Box::new(TypeName::String), Box::new(custom));
        assert_eq!(generic.apex_name(), "Map<String,Domain.Widget>");
    }
}
