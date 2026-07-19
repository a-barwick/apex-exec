use crate::{
    ast,
    platform::{DataValue, FieldType, SchemaCatalog},
    span::Span,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Deref,
};

mod intrinsic;

pub use intrinsic::{
    ExceptionIntrinsic, IntrinsicId, ListIntrinsic, MapIntrinsic, MathIntrinsic,
    PlatformConstructor, PlatformIntrinsic, SetIntrinsic, StaticStringIntrinsic, StringIntrinsic,
    SystemIntrinsic,
};

/// The checked program consumed by execution.
///
/// Parsed syntax stays immutable and free of semantic annotations. Resolution
/// results live here so the runtime never repeats overload selection.
#[derive(Clone, Debug)]
pub struct Program {
    ast: ast::Program,
    expression_types: HashMap<Span, ExpressionType>,
    calls: HashMap<Span, CallTarget>,
    references: HashMap<Span, ReferenceTarget>,
    members: HashMap<Span, MemberTarget>,
    places: HashMap<Span, PlaceTarget>,
    binary_operations: HashMap<Span, CheckedBinaryOperation>,
    unary_operations: HashMap<Span, CheckedUnaryOperation>,
    type_literals: HashMap<Span, ast::TypeName>,
    queries: HashMap<Span, CheckedQuery>,
    null_aware_queries: HashSet<Span>,
    async_contracts: HashMap<usize, AsyncClassContract>,
    class_metadata: Vec<ClassRuntimeMetadata>,
    schema: SchemaCatalog,
}

impl Program {
    pub(crate) fn new(ast: ast::Program, facts: ProgramFacts, schema: SchemaCatalog) -> Self {
        let ProgramFacts {
            expression_types,
            calls,
            references,
            members,
            places,
            binary_operations,
            unary_operations,
            type_literals,
            queries,
            null_aware_queries,
            async_contracts,
        } = facts;
        let class_metadata = build_class_metadata(&ast);
        Self {
            ast,
            expression_types,
            calls,
            references,
            members,
            places,
            binary_operations,
            unary_operations,
            type_literals,
            queries,
            null_aware_queries,
            async_contracts,
            class_metadata,
            schema,
        }
    }

    pub fn ast(&self) -> &ast::Program {
        &self.ast
    }

    pub fn expression_type(&self, span: Span) -> Option<&ExpressionType> {
        self.expression_types.get(&span)
    }

    pub fn call_target(&self, span: Span) -> Option<CallTarget> {
        self.calls.get(&span).copied()
    }

    pub fn reference_target(&self, span: Span) -> Option<ReferenceTarget> {
        self.references.get(&span).copied()
    }

    pub fn member_target(&self, span: Span) -> Option<MemberTarget> {
        self.members.get(&span).copied()
    }

    pub(crate) fn place_target(&self, span: Span) -> Option<PlaceTarget> {
        self.places.get(&span).copied()
    }

    pub(crate) fn binary_operation(&self, span: Span) -> Option<CheckedBinaryOperation> {
        self.binary_operations.get(&span).copied()
    }

    pub(crate) fn unary_operation(&self, span: Span) -> Option<CheckedUnaryOperation> {
        self.unary_operations.get(&span).copied()
    }

    pub(crate) fn type_literal(&self, span: Span) -> Option<&ast::TypeName> {
        self.type_literals.get(&span)
    }

    pub fn checked_query(&self, span: Span) -> Option<&CheckedQuery> {
        self.queries.get(&span)
    }

    pub(crate) fn query_allows_empty_single_result(&self, span: Span) -> bool {
        self.null_aware_queries.contains(&span)
    }

    pub fn async_contract(&self, class_id: usize) -> Option<&AsyncClassContract> {
        self.async_contracts.get(&class_id)
    }

    pub(crate) fn class_metadata(&self, class_id: ClassId) -> &ClassRuntimeMetadata {
        &self.class_metadata[class_id.index()]
    }

    pub fn schema(&self) -> &SchemaCatalog {
        &self.schema
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClassRuntimeMetadata {
    pub parent: Option<ClassId>,
    pub lineage_base_first: Vec<ClassId>,
    pub static_slots: Vec<ClassMemberId>,
    pub static_steps: Vec<ClassInitializationStep>,
    pub instance_slots: Vec<ClassMemberId>,
    pub instance_steps: Vec<ClassInitializationStep>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClassInitializationStep {
    Field(ClassMemberId),
    Block(ClassMemberId),
}

fn build_class_metadata(program: &ast::Program) -> Vec<ClassRuntimeMetadata> {
    let mut qualified = HashMap::new();
    let mut short = HashMap::<String, Vec<usize>>::new();
    for (class_id, class) in program.classes.iter().enumerate() {
        qualified.insert(class.qualified_name.canonical.clone(), class_id);
        short
            .entry(class.name.canonical.clone())
            .or_default()
            .push(class_id);
    }
    let parents = program
        .classes
        .iter()
        .map(|class| {
            class
                .superclass
                .as_ref()
                .and_then(|name| resolve_class_id(name, &qualified, &short))
                .map(ClassId::from_index)
        })
        .collect::<Vec<_>>();

    program
        .classes
        .iter()
        .enumerate()
        .map(|(class_id, class)| class_runtime_metadata(class_id, class, &parents))
        .collect()
}

fn resolve_class_id(
    name: &ast::NamedType,
    qualified: &HashMap<String, usize>,
    short: &HashMap<String, Vec<usize>>,
) -> Option<usize> {
    qualified.get(&name.canonical).copied().or_else(|| {
        short
            .get(&name.canonical)
            .and_then(|ids| <&[usize; 1]>::try_from(ids.as_slice()).ok())
            .map(|ids| ids[0])
    })
}

fn class_runtime_metadata(
    class_id: usize,
    class: &ast::ClassDeclaration,
    parents: &[Option<ClassId>],
) -> ClassRuntimeMetadata {
    let mut lineage_base_first = Vec::new();
    let mut cursor = Some(ClassId::from_index(class_id));
    let mut remaining = parents.len();
    while let Some(current) = cursor
        && remaining > 0
    {
        lineage_base_first.push(current);
        cursor = parents[current.index()];
        remaining -= 1;
    }
    lineage_base_first.reverse();
    let mut metadata = ClassRuntimeMetadata {
        parent: parents[class_id],
        lineage_base_first,
        ..ClassRuntimeMetadata::default()
    };
    for (member_id, member) in class.members.iter().enumerate() {
        record_runtime_member(&mut metadata, class_id, member_id, member);
    }
    metadata
}

fn record_runtime_member(
    metadata: &mut ClassRuntimeMetadata,
    class_id: usize,
    member_id: usize,
    member: &ast::ClassMember,
) {
    let target = ClassMemberId {
        class_id,
        member_id,
    };
    match member {
        ast::ClassMember::Field(field) => {
            let (slots, steps) = if field.modifiers.contains(&ast::Modifier::Static) {
                (&mut metadata.static_slots, &mut metadata.static_steps)
            } else {
                (&mut metadata.instance_slots, &mut metadata.instance_steps)
            };
            slots.push(target);
            if field.initializer.is_some() {
                steps.push(ClassInitializationStep::Field(target));
            }
        }
        ast::ClassMember::FieldGroup(_) => {}
        ast::ClassMember::Property(property) => {
            if property.modifiers.contains(&ast::Modifier::Static) {
                metadata.static_slots.push(target);
            } else {
                metadata.instance_slots.push(target);
            }
        }
        ast::ClassMember::Initializer(initializer) => {
            let steps = if initializer.is_static {
                &mut metadata.static_steps
            } else {
                &mut metadata.instance_steps
            };
            steps.push(ClassInitializationStep::Block(target));
        }
        ast::ClassMember::Constructor(_) | ast::ClassMember::Method(_) => {}
    }
}

pub(crate) struct ProgramFacts {
    pub expression_types: HashMap<Span, ExpressionType>,
    pub calls: HashMap<Span, CallTarget>,
    pub references: HashMap<Span, ReferenceTarget>,
    pub members: HashMap<Span, MemberTarget>,
    pub places: HashMap<Span, PlaceTarget>,
    pub binary_operations: HashMap<Span, CheckedBinaryOperation>,
    pub unary_operations: HashMap<Span, CheckedUnaryOperation>,
    pub type_literals: HashMap<Span, ast::TypeName>,
    pub queries: HashMap<Span, CheckedQuery>,
    pub null_aware_queries: HashSet<Span>,
    pub async_contracts: HashMap<usize, AsyncClassContract>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AsyncClassContract {
    pub queueable: Option<ClassMemberId>,
    pub batch: Option<BatchContract>,
    pub schedulable: Option<ClassMemberId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchContract {
    pub start: ClassMemberId,
    pub execute: ClassMemberId,
    pub finish: ClassMemberId,
    pub scope_type: ast::TypeName,
}

impl Deref for Program {
    type Target = ast::Program;

    fn deref(&self) -> &Self::Target {
        &self.ast
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpressionType {
    Value(ast::TypeName),
    Null,
    Void,
}

impl ExpressionType {
    pub fn apex_name(&self) -> String {
        match self {
            Self::Value(ty) => ty.apex_name(),
            Self::Null => "null".to_owned(),
            Self::Void => "void".to_owned(),
        }
    }

    pub(crate) fn name(&self) -> String {
        self.apex_name()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallTarget {
    TopLevelMethod(usize),
    StaticMethod(ClassMemberId),
    InstanceMethod(ClassMemberId),
    SuperMethod(ClassMemberId),
    Intrinsic(IntrinsicId),
    Constructor {
        class_id: usize,
        member_id: Option<usize>,
    },
    CustomExceptionConstructor {
        class_id: ClassId,
    },
    SObjectConstructor {
        object_id: Option<usize>,
    },
    SObjectGet,
    SObjectPut,
    DatabaseDml(ast::DmlOperation),
    AggregateResultGet,
    EnumMethod {
        class_id: ClassId,
        method: EnumMethod,
    },
    PlatformConstructor(PlatformConstructor),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnumMethod {
    Name,
    Ordinal,
    Values,
    ValueOf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClassId(usize);

impl ClassId {
    pub(crate) fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub(crate) fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClassMemberId {
    pub class_id: usize,
    pub member_id: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectTypeId(usize);

impl ObjectTypeId {
    pub(crate) fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub(crate) fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FieldId(usize);

impl FieldId {
    pub(crate) fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub(crate) fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceTarget {
    Local,
    InstanceMember(ClassMemberId),
    StaticMember(ClassMemberId),
    ListIndex,
    SObjectField {
        object_id: ObjectTypeId,
        field_id: FieldId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericKind {
    Integer,
    Long,
    Decimal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckedBinaryOperation {
    StringConcat,
    Numeric {
        operator: ast::BinaryOperator,
        kind: NumericKind,
    },
    BooleanBitwise(ast::BinaryOperator),
    Integral {
        operator: ast::BinaryOperator,
        kind: NumericKind,
    },
    Shift {
        operator: ast::BinaryOperator,
        kind: NumericKind,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckedUnaryOperation {
    Positive(NumericKind),
    Negate(NumericKind),
    BitwiseNot(NumericKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceTarget {
    Local,
    This,
    Super(usize),
    InstanceMember(ClassMemberId),
    StaticMember(ClassMemberId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberTarget {
    Instance(ClassMemberId),
    Static(ClassMemberId),
    SObjectField {
        object_id: usize,
        field_id: usize,
    },
    SObjectRelationship {
        object_id: usize,
        reference_field_id: usize,
        target_object_id: usize,
    },
    TriggerContext(TriggerContextVariable),
    EnumConstant {
        class_id: ClassId,
        ordinal: usize,
    },
    TypeReference {
        class_id: ClassId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerContextVariable {
    New,
    Old,
    NewMap,
    OldMap,
    IsExecuting,
    IsBefore,
    IsAfter,
    IsInsert,
    IsUpdate,
    IsDelete,
    IsUndelete,
    Size,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckedQuery {
    Soql(Box<CheckedSoqlQuery>),
    Sosl(Box<CheckedSoslQuery>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedSoqlQuery {
    pub object_id: usize,
    pub select: Vec<CheckedSelectItem>,
    pub condition: Option<CheckedCondition>,
    pub group_by: Vec<CheckedFieldPath>,
    pub order_by: Vec<CheckedOrderBy>,
    pub limit: Option<CheckedValue>,
    pub offset: Option<CheckedValue>,
    pub result: QueryResultKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryResultKind {
    Records,
    RecordSingle,
    Count,
    Aggregates,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckedSelectItem {
    Field(CheckedFieldPath),
    Aggregate {
        function: ast::SoqlAggregateFunction,
        field: Option<CheckedFieldPath>,
        alias: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedFieldPath {
    pub root_object_id: usize,
    pub relationship: Option<CheckedRelationship>,
    pub field_id: usize,
    pub field_type: FieldType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedRelationship {
    pub reference_field_id: usize,
    pub target_object_id: usize,
    pub spelling: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckedCondition {
    Comparison {
        left: CheckedFieldPath,
        operator: ast::SoqlComparisonOperator,
        right: CheckedValue,
    },
    In {
        field: CheckedFieldPath,
        negated: bool,
        values: CheckedInValues,
    },
    Not(Box<CheckedCondition>),
    Logical {
        left: Box<CheckedCondition>,
        operator: ast::SoqlLogicalOperator,
        right: Box<CheckedCondition>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckedInValues {
    Values(Vec<CheckedValue>),
    Bind(Box<ast::Expression>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckedValue {
    Literal(DataValue),
    Bind(Box<ast::Expression>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedOrderBy {
    pub field: CheckedFieldPath,
    pub direction: ast::SortDirection,
    pub nulls: Option<ast::NullsOrder>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedSoslQuery {
    pub search: CheckedValue,
    pub scope: ast::SoslScope,
    pub returning: Vec<CheckedSoslReturning>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedSoslReturning {
    pub object_id: usize,
    pub fields: Vec<CheckedFieldPath>,
    pub condition: Option<CheckedCondition>,
    pub order_by: Vec<CheckedOrderBy>,
    pub limit: Option<CheckedValue>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expression, Statement};

    #[test]
    fn records_types_and_call_targets_outside_the_parsed_ast() {
        let parsed = crate::parse(
            "Integer doubleIt(Integer value) { return value * 2; } Integer result = doubleIt(3);",
        )
        .unwrap();
        let Statement::VariableDeclaration { initializer, .. } = &parsed.statements[0] else {
            panic!("expected variable declaration");
        };
        let Expression::FunctionCall { span, .. } = initializer else {
            panic!("expected function call");
        };

        let checked = crate::semantic::check(&parsed).unwrap();
        assert_eq!(
            checked.expression_type(*span),
            Some(&ExpressionType::Value(ast::TypeName::Integer))
        );
        assert_eq!(
            checked.call_target(*span),
            Some(CallTarget::TopLevelMethod(0))
        );
    }

    #[test]
    fn intrinsic_targets_are_stable_across_case_insensitive_spelling() {
        let parsed =
            crate::parse("Integer first = MaTh.AbS(1); Integer second = math.abs(2);").unwrap();
        let spans = parsed
            .statements
            .iter()
            .map(|statement| {
                let Statement::VariableDeclaration { initializer, .. } = statement else {
                    panic!("expected variable declaration");
                };
                let Expression::MethodCall { span, .. } = initializer else {
                    panic!("expected method call");
                };
                *span
            })
            .collect::<Vec<_>>();

        let checked = crate::semantic::check(&parsed).unwrap();
        let expected = Some(CallTarget::Intrinsic(IntrinsicId::Math(MathIntrinsic::Abs)));

        assert_eq!(checked.call_target(spans[0]), expected);
        assert_eq!(checked.call_target(spans[1]), expected);
    }
}
