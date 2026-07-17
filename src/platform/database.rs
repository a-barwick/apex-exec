use super::{
    DataValue, FieldType, Record, RecordId, SObject, SchemaCatalog, SqliteStorage, Storage,
    StorageTransaction,
};
use std::{cmp::Ordering, collections::BTreeMap, error::Error, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmlOperation {
    Insert,
    Update,
    Upsert,
    Delete,
    Undelete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryRelationship {
    pub reference_field: String,
    pub target_object: String,
    pub spelling: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryField {
    pub relationship: Option<QueryRelationship>,
    pub field: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateFunction {
    Count,
    Sum,
    Min,
    Max,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuerySelect {
    Field(QueryField),
    Aggregate {
        function: AggregateFunction,
        field: Option<QueryField>,
        alias: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryComparison {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Like,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryLogical {
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryInValues {
    Values(Vec<DataValue>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryCondition {
    Comparison {
        left: QueryField,
        operator: QueryComparison,
        right: DataValue,
    },
    In {
        field: QueryField,
        negated: bool,
        values: QueryInValues,
    },
    Not(Box<QueryCondition>),
    Logical {
        left: Box<QueryCondition>,
        operator: QueryLogical,
        right: Box<QueryCondition>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullOrder {
    First,
    Last,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryOrder {
    pub field: QueryField,
    pub direction: SortOrder,
    pub nulls: Option<NullOrder>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoqlRequest {
    pub object: String,
    pub select: Vec<QuerySelect>,
    pub condition: Option<QueryCondition>,
    pub group_by: Vec<QueryField>,
    pub order_by: Vec<QueryOrder>,
    pub limit: Option<usize>,
    pub offset: usize,
    pub count_scalar: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoslReturningRequest {
    pub object: String,
    pub fields: Vec<QueryField>,
    pub condition: Option<QueryCondition>,
    pub order_by: Vec<QueryOrder>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoslRequest {
    pub search: String,
    pub name_fields_only: bool,
    pub returning: Vec<SoslReturningRequest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryRecord {
    pub record: Record,
    pub relationships: BTreeMap<String, Record>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryOutcome {
    Records(Vec<QueryRecord>),
    Count(i64),
    Aggregates(Vec<BTreeMap<String, DataValue>>),
    Search(Vec<Vec<QueryRecord>>),
}

pub struct LocalDatabase {
    storage: SqliteStorage,
    sequences: BTreeMap<String, u64>,
}

impl LocalDatabase {
    pub fn new(schema: SchemaCatalog) -> Result<Self, DatabaseError> {
        Ok(Self {
            storage: SqliteStorage::in_memory(schema).map_err(DatabaseError::storage)?,
            sequences: BTreeMap::new(),
        })
    }

    pub fn schema(&self) -> &SchemaCatalog {
        self.storage.schema()
    }

    pub fn migrate(&mut self, schema: SchemaCatalog) -> Result<(), DatabaseError> {
        self.storage.migrate(schema).map_err(DatabaseError::storage)
    }

    pub fn execute_soql(&mut self, request: &SoqlRequest) -> Result<QueryOutcome, DatabaseError> {
        let schema = self.storage.schema().clone();
        let mut transaction = self
            .storage
            .begin_transaction()
            .map_err(DatabaseError::storage)?;
        let records = transaction
            .scan(&request.object)
            .map_err(DatabaseError::storage)?;
        let mut rows = hydrate_rows(&schema, &mut transaction, records)?;
        if let Some(condition) = &request.condition {
            rows.retain(|row| evaluate_condition(row, condition));
        }
        sort_rows(&mut rows, &request.order_by);
        let start = request.offset.min(rows.len());
        rows = rows.split_off(start);
        if let Some(limit) = request.limit {
            rows.truncate(limit);
        }
        let outcome = if request.count_scalar {
            QueryOutcome::Count(i64::try_from(rows.len()).unwrap_or(i64::MAX))
        } else if request
            .select
            .iter()
            .any(|item| matches!(item, QuerySelect::Aggregate { .. }))
        {
            QueryOutcome::Aggregates(aggregate_rows(&rows, request))
        } else {
            QueryOutcome::Records(
                rows.into_iter()
                    .map(|row| QueryRecord {
                        record: row.record,
                        relationships: row.relationships,
                    })
                    .collect(),
            )
        };
        transaction.commit().map_err(DatabaseError::storage)?;
        Ok(outcome)
    }

    pub fn execute_sosl(&mut self, request: &SoslRequest) -> Result<QueryOutcome, DatabaseError> {
        let schema = self.storage.schema().clone();
        let mut transaction = self
            .storage
            .begin_transaction()
            .map_err(DatabaseError::storage)?;
        let needle = request.search.to_ascii_lowercase();
        let mut groups = Vec::new();
        for returning in &request.returning {
            let records = transaction
                .scan(&returning.object)
                .map_err(DatabaseError::storage)?;
            let object = schema
                .object(&returning.object)
                .map_err(DatabaseError::schema)?;
            let searchable = object
                .fields()
                .filter(|field| {
                    field.data_type() == &FieldType::String
                        && (!request.name_fields_only
                            || field.api_name().eq_ignore_ascii_case("Name"))
                })
                .map(|field| field.api_name().to_owned())
                .collect::<Vec<_>>();
            let mut rows = hydrate_rows(&schema, &mut transaction, records)?;
            rows.retain(|row| {
                searchable.iter().any(|field| {
                    matches!(
                        row.record.field(field),
                        Some(DataValue::String(value))
                            if value.to_ascii_lowercase().contains(&needle)
                    )
                }) && returning
                    .condition
                    .as_ref()
                    .is_none_or(|condition| evaluate_condition(row, condition))
            });
            sort_rows(&mut rows, &returning.order_by);
            if let Some(limit) = returning.limit {
                rows.truncate(limit);
            }
            groups.push(
                rows.into_iter()
                    .map(|row| QueryRecord {
                        record: row.record,
                        relationships: row.relationships,
                    })
                    .collect(),
            );
        }
        transaction.commit().map_err(DatabaseError::storage)?;
        Ok(QueryOutcome::Search(groups))
    }

    pub fn execute_dml(
        &mut self,
        operation: DmlOperation,
        records: Vec<SObject>,
    ) -> Result<Vec<SObject>, DatabaseError> {
        let schema = self.storage.schema().clone();
        let mut transaction = self
            .storage
            .begin_transaction()
            .map_err(DatabaseError::storage)?;
        let mut persisted = Vec::with_capacity(records.len());
        for mut value in records {
            let object = schema
                .object(value.object_api_name())
                .map_err(DatabaseError::schema)?;
            let existing = value
                .id()
                .map(|id| {
                    transaction
                        .read(object.api_name(), id)
                        .map_err(DatabaseError::storage)
                })
                .transpose()?
                .flatten();
            match operation {
                DmlOperation::Insert if value.id().is_some() => {
                    return Err(DatabaseError::new(format!(
                        "insert requires a new {} record without an Id",
                        object.api_name()
                    )));
                }
                DmlOperation::Insert | DmlOperation::Upsert if value.id().is_none() => {
                    let canonical = object.api_name().to_ascii_lowercase();
                    let sequence = self.sequences.entry(canonical).or_default();
                    loop {
                        *sequence += 1;
                        let id = RecordId::generate(object.key_prefix(), *sequence)
                            .map_err(DatabaseError::id)?;
                        if transaction
                            .read(object.api_name(), &id)
                            .map_err(DatabaseError::storage)?
                            .is_none()
                        {
                            value.set_id(id);
                            break;
                        }
                    }
                    transaction
                        .write(
                            value
                                .clone()
                                .into_record()
                                .map_err(DatabaseError::sobject)?,
                        )
                        .map_err(DatabaseError::storage)?;
                }
                DmlOperation::Insert => unreachable!("insert with Id returned above"),
                DmlOperation::Update if existing.is_none() => {
                    return Err(DatabaseError::new(format!(
                        "update could not find {} record `{}`",
                        object.api_name(),
                        value.id().expect("update has an Id")
                    )));
                }
                DmlOperation::Update | DmlOperation::Upsert => {
                    let incoming = value
                        .clone()
                        .into_record()
                        .map_err(DatabaseError::sobject)?;
                    let record = if let Some(mut stored) = existing {
                        for (name, field_value) in incoming.fields() {
                            stored.set_field(name, field_value.clone());
                        }
                        stored
                    } else {
                        incoming
                    };
                    transaction.write(record).map_err(DatabaseError::storage)?;
                }
                DmlOperation::Undelete => {
                    return Err(DatabaseError::new(
                        "undelete requires recycle-bin semantics planned for milestone 9",
                    ));
                }
                DmlOperation::Delete => {
                    let Some(id) = value.id() else {
                        return Err(DatabaseError::new("delete requires a record Id"));
                    };
                    if !transaction
                        .delete(object.api_name(), id)
                        .map_err(DatabaseError::storage)?
                    {
                        return Err(DatabaseError::new(format!(
                            "delete could not find {} record `{id}`",
                            object.api_name()
                        )));
                    }
                }
            }
            persisted.push(value);
        }
        transaction.commit().map_err(DatabaseError::storage)?;
        Ok(persisted)
    }
}

struct EvalRow {
    record: Record,
    relationships: BTreeMap<String, Record>,
}

fn hydrate_rows<T: StorageTransaction>(
    schema: &SchemaCatalog,
    transaction: &mut T,
    records: Vec<Record>,
) -> Result<Vec<EvalRow>, DatabaseError> {
    let mut rows = Vec::with_capacity(records.len());
    for mut record in records {
        record.set_field("Id", DataValue::Id(record.id().clone()));
        let object = schema
            .object(record.object_api_name())
            .map_err(DatabaseError::schema)?;
        let mut relationships = BTreeMap::new();
        for field in object.fields() {
            let FieldType::Reference { target_object } = field.data_type() else {
                continue;
            };
            let Some(value) = record.field(field.api_name()) else {
                continue;
            };
            let id = match value {
                DataValue::Id(id) => Some(id.clone()),
                DataValue::String(id) => RecordId::parse(id.clone()).ok(),
                _ => None,
            };
            if let Some(id) = id
                && let Some(mut related) = transaction
                    .read(target_object, &id)
                    .map_err(DatabaseError::storage)?
            {
                related.set_field("Id", DataValue::Id(related.id().clone()));
                relationships.insert(field.api_name().to_ascii_lowercase(), related);
            }
        }
        rows.push(EvalRow {
            record,
            relationships,
        });
    }
    Ok(rows)
}

fn field_value<'row>(row: &'row EvalRow, field: &QueryField) -> &'row DataValue {
    static NULL: DataValue = DataValue::Null;
    if let Some(relationship) = &field.relationship {
        row.relationships
            .get(&relationship.reference_field.to_ascii_lowercase())
            .and_then(|record| record.field(&field.field))
            .unwrap_or(&NULL)
    } else {
        row.record.field(&field.field).unwrap_or(&NULL)
    }
}

fn evaluate_condition(row: &EvalRow, condition: &QueryCondition) -> bool {
    match condition {
        QueryCondition::Comparison {
            left,
            operator,
            right,
        } => compare(field_value(row, left), right, *operator),
        QueryCondition::In {
            field,
            negated,
            values: QueryInValues::Values(values),
        } => {
            let found = values
                .iter()
                .any(|value| values_equal(field_value(row, field), value));
            found != *negated
        }
        QueryCondition::Not(condition) => !evaluate_condition(row, condition),
        QueryCondition::Logical {
            left,
            operator,
            right,
        } => match operator {
            QueryLogical::And => evaluate_condition(row, left) && evaluate_condition(row, right),
            QueryLogical::Or => evaluate_condition(row, left) || evaluate_condition(row, right),
        },
    }
}

fn compare(left: &DataValue, right: &DataValue, operator: QueryComparison) -> bool {
    match operator {
        QueryComparison::Equal => values_equal(left, right),
        QueryComparison::NotEqual => !values_equal(left, right),
        QueryComparison::Like => match (left, right) {
            (DataValue::String(value), DataValue::String(pattern)) => like(value, pattern),
            _ => false,
        },
        QueryComparison::Less
        | QueryComparison::LessEqual
        | QueryComparison::Greater
        | QueryComparison::GreaterEqual => {
            let ordering = compare_values(left, right);
            matches!(
                (operator, ordering),
                (QueryComparison::Less, Ordering::Less)
                    | (QueryComparison::LessEqual, Ordering::Less | Ordering::Equal)
                    | (QueryComparison::Greater, Ordering::Greater)
                    | (
                        QueryComparison::GreaterEqual,
                        Ordering::Greater | Ordering::Equal
                    )
            )
        }
    }
}

fn values_equal(left: &DataValue, right: &DataValue) -> bool {
    match (left, right) {
        (DataValue::String(left), DataValue::String(right)) => left.eq_ignore_ascii_case(right),
        (DataValue::Id(left), DataValue::String(right))
        | (DataValue::String(right), DataValue::Id(left)) => left.as_str() == right,
        _ => left == right,
    }
}

fn compare_values(left: &DataValue, right: &DataValue) -> Ordering {
    match (left, right) {
        (DataValue::Null, DataValue::Null) => Ordering::Equal,
        (DataValue::Null, _) => Ordering::Less,
        (_, DataValue::Null) => Ordering::Greater,
        (DataValue::Boolean(left), DataValue::Boolean(right)) => left.cmp(right),
        (DataValue::Integer(left), DataValue::Integer(right)) => left.cmp(right),
        (DataValue::String(left), DataValue::String(right)) => {
            left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
        }
        (DataValue::Id(left), DataValue::Id(right)) => left.as_str().cmp(right.as_str()),
        (DataValue::Id(left), DataValue::String(right)) => left.as_str().cmp(right),
        (DataValue::String(left), DataValue::Id(right)) => left.as_str().cmp(right.as_str()),
        _ => data_rank(left).cmp(&data_rank(right)),
    }
}

fn data_rank(value: &DataValue) -> u8 {
    match value {
        DataValue::Null => 0,
        DataValue::Boolean(_) => 1,
        DataValue::Integer(_) => 2,
        DataValue::String(_) => 3,
        DataValue::Id(_) => 4,
    }
}

fn like(value: &str, pattern: &str) -> bool {
    let value = value.to_ascii_lowercase().into_bytes();
    let pattern = pattern.to_ascii_lowercase().into_bytes();
    let mut matches = vec![false; pattern.len() + 1];
    matches[0] = true;
    for index in 1..=pattern.len() {
        matches[index] = matches[index - 1] && pattern[index - 1] == b'%';
    }
    for byte in value {
        let mut next = vec![false; pattern.len() + 1];
        for index in 1..=pattern.len() {
            next[index] = match pattern[index - 1] {
                b'%' => next[index - 1] || matches[index],
                b'_' => matches[index - 1],
                expected => matches[index - 1] && expected == byte,
            };
        }
        matches = next;
    }
    matches[pattern.len()]
}

fn sort_rows(rows: &mut [EvalRow], ordering: &[QueryOrder]) {
    rows.sort_by(|left, right| {
        for order in ordering {
            let left_value = field_value(left, &order.field);
            let right_value = field_value(right, &order.field);
            let left_null = matches!(left_value, DataValue::Null);
            let right_null = matches!(right_value, DataValue::Null);
            let compared = if left_null || right_null {
                let null_order =
                    order
                        .nulls
                        .unwrap_or(if order.direction == SortOrder::Descending {
                            NullOrder::Last
                        } else {
                            NullOrder::First
                        });
                match (left_null, right_null, null_order) {
                    (true, false, NullOrder::First) | (false, true, NullOrder::Last) => {
                        Ordering::Less
                    }
                    (true, false, NullOrder::Last) | (false, true, NullOrder::First) => {
                        Ordering::Greater
                    }
                    _ => Ordering::Equal,
                }
            } else {
                let compared = compare_values(left_value, right_value);
                if order.direction == SortOrder::Descending {
                    compared.reverse()
                } else {
                    compared
                }
            };
            if compared != Ordering::Equal {
                return compared;
            }
        }
        left.record.id().as_str().cmp(right.record.id().as_str())
    });
}

fn aggregate_rows(rows: &[EvalRow], request: &SoqlRequest) -> Vec<BTreeMap<String, DataValue>> {
    let mut groups = BTreeMap::<Vec<String>, Vec<&EvalRow>>::new();
    if request.group_by.is_empty() {
        groups.insert(Vec::new(), rows.iter().collect());
    } else {
        for row in rows {
            let key = request
                .group_by
                .iter()
                .map(|field| format!("{:?}", field_value(row, field)))
                .collect();
            groups.entry(key).or_default().push(row);
        }
    }
    groups
        .into_values()
        .map(|group| {
            let mut result = BTreeMap::new();
            for item in &request.select {
                match item {
                    QuerySelect::Field(field) => {
                        result.insert(
                            field.field.to_ascii_lowercase(),
                            group
                                .first()
                                .map_or(DataValue::Null, |row| field_value(row, field).clone()),
                        );
                    }
                    QuerySelect::Aggregate {
                        function,
                        field,
                        alias,
                    } => {
                        result.insert(
                            alias.to_ascii_lowercase(),
                            aggregate_value(&group, *function, field.as_ref()),
                        );
                    }
                }
            }
            result
        })
        .collect()
}

fn aggregate_value(
    rows: &[&EvalRow],
    function: AggregateFunction,
    field: Option<&QueryField>,
) -> DataValue {
    match function {
        AggregateFunction::Count => {
            let count = field.map_or(rows.len(), |field| {
                rows.iter()
                    .filter(|row| !matches!(field_value(row, field), DataValue::Null))
                    .count()
            });
            DataValue::Integer(i64::try_from(count).unwrap_or(i64::MAX))
        }
        AggregateFunction::Sum => DataValue::Integer(
            rows.iter()
                .filter_map(
                    |row| match field_value(row, field.expect("SUM has a field")) {
                        DataValue::Integer(value) => Some(*value),
                        _ => None,
                    },
                )
                .sum(),
        ),
        AggregateFunction::Min | AggregateFunction::Max => rows
            .iter()
            .map(|row| field_value(row, field.expect("MIN/MAX has a field")).clone())
            .filter(|value| !matches!(value, DataValue::Null))
            .reduce(|left, right| {
                let choose_right = match function {
                    AggregateFunction::Min => compare_values(&right, &left) == Ordering::Less,
                    AggregateFunction::Max => compare_values(&right, &left) == Ordering::Greater,
                    _ => false,
                };
                if choose_right { right } else { left }
            })
            .unwrap_or(DataValue::Null),
    }
}

#[derive(Debug)]
pub struct DatabaseError {
    message: String,
}

impl DatabaseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn storage(error: impl fmt::Display) -> Self {
        Self::new(error.to_string())
    }

    fn schema(error: impl fmt::Display) -> Self {
        Self::new(error.to_string())
    }

    fn sobject(error: impl fmt::Display) -> Self {
        Self::new(error.to_string())
    }

    fn id(error: impl fmt::Display) -> Self {
        Self::new(error.to_string())
    }

    pub(crate) fn unavailable() -> Self {
        Self::new("platform host does not provide a local database")
    }
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for DatabaseError {}
