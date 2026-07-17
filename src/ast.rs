use crate::span::Span;
use std::hash::{Hash, Hasher};

pub mod visit;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub classes: Vec<ClassDeclaration>,
    pub methods: Vec<MethodDeclaration>,
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDeclaration {
    pub annotations: Vec<Annotation>,
    pub kind: ClassKind,
    pub modifiers: Vec<Modifier>,
    pub name: Identifier,
    pub superclass: Option<NamedType>,
    pub interfaces: Vec<NamedType>,
    pub members: Vec<ClassMember>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Interface,
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
    pub body: Statement,
    pub span: Span,
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
}

impl AnnotationKind {
    pub fn is_test(self) -> bool {
        matches!(self, Self::IsTest { .. })
    }

    pub fn is_test_setup(self) -> bool {
        matches!(self, Self::TestSetup)
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
    NullLiteral(Span),
    Variable(Identifier),
    Assignment {
        target: AssignmentTarget,
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
        span: Span,
    },
    MemberAccess {
        receiver: Box<Expression>,
        member: Identifier,
        span: Span,
    },
    Cast {
        ty: TypeName,
        expression: Box<Expression>,
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
            | Self::NullLiteral(span)
            | Self::Assignment { span, .. }
            | Self::NewCollection { span, .. }
            | Self::NewException { span, .. }
            | Self::NewObject { span, .. }
            | Self::Index { span, .. }
            | Self::FunctionCall { span, .. }
            | Self::MethodCall { span, .. }
            | Self::MemberAccess { span, .. }
            | Self::Cast { span, .. }
            | Self::Unary { span, .. }
            | Self::Postfix { span, .. }
            | Self::Binary { span, .. } => *span,
            Self::Variable(identifier) => identifier.span,
        }
    }
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
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
    Object,
    Exception,
    NullPointerException,
    ListException,
    MathException,
    TypeException,
    StringException,
    IllegalArgumentException,
    FinalException,
    AssertException,
    Custom(NamedType),
    List(Box<TypeName>),
    Set(Box<TypeName>),
    Map(Box<TypeName>, Box<TypeName>),
}

impl TypeName {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "string" => Some(Self::String),
            "boolean" => Some(Self::Boolean),
            "integer" => Some(Self::Integer),
            "object" => Some(Self::Object),
            "exception" => Some(Self::Exception),
            "nullpointerexception" => Some(Self::NullPointerException),
            "listexception" => Some(Self::ListException),
            "mathexception" => Some(Self::MathException),
            "typeexception" => Some(Self::TypeException),
            "stringexception" => Some(Self::StringException),
            "illegalargumentexception" => Some(Self::IllegalArgumentException),
            "finalexception" => Some(Self::FinalException),
            "assertexception" => Some(Self::AssertException),
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
        )
    }

    pub fn apex_name(&self) -> String {
        match self {
            Self::String => "String".to_owned(),
            Self::Boolean => "Boolean".to_owned(),
            Self::Integer => "Integer".to_owned(),
            Self::Object => "Object".to_owned(),
            Self::Exception => "Exception".to_owned(),
            Self::NullPointerException => "NullPointerException".to_owned(),
            Self::ListException => "ListException".to_owned(),
            Self::MathException => "MathException".to_owned(),
            Self::TypeException => "TypeException".to_owned(),
            Self::StringException => "StringException".to_owned(),
            Self::IllegalArgumentException => "IllegalArgumentException".to_owned(),
            Self::FinalException => "FinalException".to_owned(),
            Self::AssertException => "AssertException".to_owned(),
            Self::Custom(name) => name.spelling.clone(),
            Self::List(element) => format!("List<{}>", element.apex_name()),
            Self::Set(element) => format!("Set<{}>", element.apex_name()),
            Self::Map(key, value) => {
                format!("Map<{},{}>", key.apex_name(), value.apex_name())
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct NamedType {
    pub spelling: String,
    pub canonical: String,
    pub span: Span,
}

impl NamedType {
    pub fn new(spelling: String, span: Span) -> Self {
        let canonical = spelling.to_ascii_lowercase();
        Self {
            spelling,
            canonical,
            span,
        }
    }
}

impl PartialEq for NamedType {
    fn eq(&self, other: &Self) -> bool {
        self.canonical == other.canonical
    }
}

impl Eq for NamedType {}

impl Hash for NamedType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.canonical.hash(state);
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
