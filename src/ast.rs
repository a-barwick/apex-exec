use crate::span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub statements: Vec<Statement>,
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
    Index {
        collection: Box<Expression>,
        index: Box<Expression>,
        span: Span,
    },
    MethodCall {
        receiver: Box<Expression>,
        method: Identifier,
        arguments: Vec<Expression>,
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
            | Self::Index { span, .. }
            | Self::MethodCall { span, .. }
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
}

impl AssignmentTarget {
    pub fn span(&self) -> Span {
        match self {
            Self::Variable(identifier) => identifier.span,
            Self::Index { span, .. } => *span,
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
            _ => None,
        }
    }

    pub fn apex_name(&self) -> String {
        match self {
            Self::String => "String".to_owned(),
            Self::Boolean => "Boolean".to_owned(),
            Self::Integer => "Integer".to_owned(),
            Self::List(element) => format!("List<{}>", element.apex_name()),
            Self::Set(element) => format!("Set<{}>", element.apex_name()),
            Self::Map(key, value) => {
                format!("Map<{},{}>", key.apex_name(), value.apex_name())
            }
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
            | Self::Return { span, .. } => *span,
        }
    }
}
