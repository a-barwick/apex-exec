use super::{
    DataValue, FieldType, Record, RecordId, SObject, SchemaCatalog, SqliteStorage, Storage,
    StorageTransaction, SummaryDefinition, SummaryFilterOperator, SummaryOperation,
};
use chrono::{Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmlOperation {
    Insert,
    Update,
    Upsert,
    Delete,
    Undelete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmlExternalId {
    pub object: String,
    pub field: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmlRow {
    pub input_index: usize,
    pub record: SObject,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmlRequest {
    pub operation: DmlOperation,
    pub all_or_none: bool,
    pub access: super::AccessLevel,
    pub sharing: super::SharingMode,
    pub user_id: String,
    pub external_id: Option<DmlExternalId>,
    pub rows: Vec<DmlRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmlStatus {
    CannotInsertUpdateActivateEntity,
    DuplicateExternalId,
    DuplicateValue,
    InvalidCrossReferenceKey,
    InvalidFieldForInsertUpdate,
    InsufficientAccessOrReadonly,
    MissingArgument,
    RequiredFieldMissing,
    UnknownException,
}

impl DmlStatus {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "CANNOT_INSERT_UPDATE_ACTIVATE_ENTITY" => Some(Self::CannotInsertUpdateActivateEntity),
            "DUPLICATE_EXTERNAL_ID" => Some(Self::DuplicateExternalId),
            "DUPLICATE_VALUE" => Some(Self::DuplicateValue),
            "INVALID_CROSS_REFERENCE_KEY" => Some(Self::InvalidCrossReferenceKey),
            "INVALID_FIELD_FOR_INSERT_UPDATE" => Some(Self::InvalidFieldForInsertUpdate),
            "INSUFFICIENT_ACCESS_OR_READONLY" => Some(Self::InsufficientAccessOrReadonly),
            "MISSING_ARGUMENT" => Some(Self::MissingArgument),
            "REQUIRED_FIELD_MISSING" => Some(Self::RequiredFieldMissing),
            "UNKNOWN_EXCEPTION" => Some(Self::UnknownException),
            _ => None,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::CannotInsertUpdateActivateEntity => "CANNOT_INSERT_UPDATE_ACTIVATE_ENTITY",
            Self::DuplicateExternalId => "DUPLICATE_EXTERNAL_ID",
            Self::DuplicateValue => "DUPLICATE_VALUE",
            Self::InvalidCrossReferenceKey => "INVALID_CROSS_REFERENCE_KEY",
            Self::InvalidFieldForInsertUpdate => "INVALID_FIELD_FOR_INSERT_UPDATE",
            Self::InsufficientAccessOrReadonly => "INSUFFICIENT_ACCESS_OR_READONLY",
            Self::MissingArgument => "MISSING_ARGUMENT",
            Self::RequiredFieldMissing => "REQUIRED_FIELD_MISSING",
            Self::UnknownException => "UNKNOWN_EXCEPTION",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmlError {
    pub status: DmlStatus,
    pub message: String,
    pub fields: Vec<String>,
}

impl DmlError {
    pub fn new(
        status: DmlStatus,
        message: impl Into<String>,
        fields: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            status,
            message: message.into(),
            fields: fields.into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedDmlRecord {
    pub input_index: usize,
    pub operation: DmlOperation,
    pub created: bool,
    pub old: Option<SObject>,
    pub new: Option<SObject>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreparedDmlOutcome {
    Ready(PreparedDmlRecord),
    Failed {
        input_index: usize,
        errors: Vec<DmlError>,
    },
}

impl PreparedDmlOutcome {
    pub fn input_index(&self) -> usize {
        match self {
            Self::Ready(record) => record.input_index,
            Self::Failed { input_index, .. } => *input_index,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmlRowOutcome {
    pub input_index: usize,
    pub id: Option<RecordId>,
    pub created: bool,
    pub record: Option<SObject>,
    pub errors: Vec<DmlError>,
}

impl DmlRowOutcome {
    pub fn success(
        input_index: usize,
        record: SObject,
        created: bool,
    ) -> Result<Self, DatabaseError> {
        let id = record
            .id()
            .cloned()
            .ok_or_else(|| DatabaseError::new("successful DML row is missing its record Id"))?;
        Ok(Self {
            input_index,
            id: Some(id),
            created,
            record: Some(record),
            errors: Vec::new(),
        })
    }

    pub fn failure(input_index: usize, errors: Vec<DmlError>) -> Self {
        Self {
            input_index,
            id: None,
            created: false,
            record: None,
            errors,
        }
    }

    pub fn is_success(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct DatabaseSnapshot {
    records: Vec<Record>,
    recycle_bin: BTreeMap<(String, RecordId), Record>,
    sequences: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryRelationship {
    pub reference_field: String,
    pub target_object: String,
    pub spelling: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryField {
    pub relationships: Vec<QueryRelationship>,
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
    Subquery {
        relationship: String,
        reference_field: String,
        query: Box<SoqlRequest>,
    },
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
    Values(Vec<QueryValue>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryCondition {
    Comparison {
        left: QueryField,
        operator: QueryComparison,
        right: QueryValue,
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
pub enum QueryDateLiteralKind {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryDateLiteral {
    pub kind: QueryDateLiteralKind,
    pub amount: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryValue {
    Data(DataValue),
    DateLiteral(QueryDateLiteral),
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
    pub access: super::QueryAccessMode,
    pub sharing: super::SharingMode,
    pub user_id: String,
    pub visible_record_ids: Option<BTreeSet<RecordId>>,
    pub group_by: Vec<QueryField>,
    pub having: Option<QueryCondition>,
    pub order_by: Vec<QueryOrder>,
    pub limit: Option<usize>,
    pub offset: usize,
    pub all_rows: bool,
    pub count_scalar: bool,
    pub now_millis: i64,
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
    pub relationships: BTreeMap<String, QueryRecord>,
    pub children: BTreeMap<String, Vec<QueryRecord>>,
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
    recycle_bin: BTreeMap<(String, RecordId), Record>,
    last_query_object_scans: usize,
}

impl LocalDatabase {
    pub fn new(schema: SchemaCatalog) -> Result<Self, DatabaseError> {
        Ok(Self {
            storage: SqliteStorage::in_memory(schema).map_err(DatabaseError::storage)?,
            sequences: BTreeMap::new(),
            recycle_bin: BTreeMap::new(),
            last_query_object_scans: 0,
        })
    }

    pub fn schema(&self) -> &SchemaCatalog {
        self.storage.schema()
    }

    pub fn load_fixture(
        &mut self,
        records: impl IntoIterator<Item = Record>,
    ) -> Result<(), DatabaseError> {
        self.storage
            .load_fixture(records)
            .map_err(DatabaseError::storage)?;
        self.sequences.clear();
        self.recycle_bin.clear();
        Ok(())
    }

    pub fn last_query_object_scans(&self) -> usize {
        self.last_query_object_scans
    }

    pub(crate) fn records_for_security(
        &mut self,
        object_api_name: &str,
    ) -> Result<Vec<Record>, DatabaseError> {
        let mut transaction = self
            .storage
            .begin_transaction()
            .map_err(DatabaseError::storage)?;
        let records = transaction
            .scan(object_api_name)
            .map_err(DatabaseError::storage)?;
        transaction.commit().map_err(DatabaseError::storage)?;
        Ok(records)
    }

    pub fn migrate(&mut self, schema: SchemaCatalog) -> Result<(), DatabaseError> {
        self.storage.migrate(schema).map_err(DatabaseError::storage)
    }

    pub fn snapshot(&mut self) -> Result<DatabaseSnapshot, DatabaseError> {
        let schema = self.storage.schema().clone();
        let mut transaction = self
            .storage
            .begin_transaction()
            .map_err(DatabaseError::storage)?;
        let mut records = Vec::new();
        for object in schema.objects() {
            records.extend(
                transaction
                    .scan(object.api_name())
                    .map_err(DatabaseError::storage)?,
            );
        }
        transaction.commit().map_err(DatabaseError::storage)?;
        Ok(DatabaseSnapshot {
            records,
            recycle_bin: self.recycle_bin.clone(),
            sequences: self.sequences.clone(),
        })
    }

    pub fn restore(&mut self, snapshot: DatabaseSnapshot) -> Result<(), DatabaseError> {
        self.storage
            .load_fixture(snapshot.records)
            .map_err(DatabaseError::storage)?;
        self.recycle_bin = snapshot.recycle_bin;
        self.sequences = snapshot.sequences;
        Ok(())
    }

    pub fn prepare_dml(
        &mut self,
        request: &DmlRequest,
    ) -> Result<Vec<PreparedDmlOutcome>, DatabaseError> {
        let schema = self.storage.schema().clone();
        let mut transaction = self
            .storage
            .begin_transaction()
            .map_err(DatabaseError::storage)?;
        let mut outcomes = Vec::with_capacity(request.rows.len());
        for row in &request.rows {
            let outcome =
                prepare_dml_row(&schema, &mut transaction, &self.recycle_bin, request, row)?;
            outcomes.push(match outcome {
                Ok(record) => PreparedDmlOutcome::Ready(record),
                Err(error) => PreparedDmlOutcome::Failed {
                    input_index: row.input_index,
                    errors: vec![error],
                },
            });
        }
        transaction.commit().map_err(DatabaseError::storage)?;
        outcomes.sort_by_key(PreparedDmlOutcome::input_index);
        Ok(outcomes)
    }

    pub fn execute_soql(&mut self, request: &SoqlRequest) -> Result<QueryOutcome, DatabaseError> {
        self.last_query_object_scans = 0;
        let schema = self.storage.schema().clone();
        let mut transaction = self
            .storage
            .begin_transaction()
            .map_err(DatabaseError::storage)?;
        let mut records = transaction
            .scan(&request.object)
            .map_err(DatabaseError::storage)?;
        for record in &mut records {
            record.set_field("IsDeleted", false);
        }
        if request.all_rows {
            records.extend(
                self.recycle_bin
                    .iter()
                    .filter(|((object, _), _)| object.eq_ignore_ascii_case(&request.object))
                    .map(|(_, record)| {
                        let mut record = record.clone();
                        record.set_field("IsDeleted", true);
                        record
                    }),
            );
        }
        let records = match &request.visible_record_ids {
            Some(visible) => records
                .into_iter()
                .filter(|record| visible.contains(record.id()))
                .collect(),
            None => records,
        };
        self.last_query_object_scans = 1;
        let mut rows = hydrate_rows(
            &schema,
            &mut transaction,
            records,
            requested_parent_depth(request),
        )?;
        self.last_query_object_scans +=
            hydrate_summary_fields(&schema, &mut transaction, &request.object, &mut rows)?;
        if let Some(condition) = &request.condition {
            rows.retain(|row| evaluate_condition(row, condition, request.now_millis));
        }
        let outcome = if request.count_scalar {
            apply_row_window(&mut rows, request);
            QueryOutcome::Count(i64::try_from(rows.len()).unwrap_or(i64::MAX))
        } else if request
            .select
            .iter()
            .any(|item| matches!(item, QuerySelect::Aggregate { .. }))
        {
            let mut aggregates = aggregate_rows(&rows, request);
            if let Some(condition) = &request.having {
                aggregates
                    .retain(|row| evaluate_aggregate_condition(row, condition, request.now_millis));
            }
            sort_aggregate_rows(&mut aggregates, &request.order_by);
            let start = request.offset.min(aggregates.len());
            aggregates = aggregates.split_off(start);
            if let Some(limit) = request.limit {
                aggregates.truncate(limit);
            }
            QueryOutcome::Aggregates(aggregates)
        } else {
            apply_row_window(&mut rows, request);
            self.last_query_object_scans +=
                hydrate_child_subqueries(&schema, &mut transaction, &mut rows, request)?;
            QueryOutcome::Records(rows.into_iter().map(eval_row_into_query_record).collect())
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
            let mut rows = hydrate_rows(
                &schema,
                &mut transaction,
                records,
                returning
                    .fields
                    .iter()
                    .map(|field| field.relationships.len())
                    .max()
                    .unwrap_or(0),
            )?;
            hydrate_summary_fields(&schema, &mut transaction, &returning.object, &mut rows)?;
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
                    .is_none_or(|condition| evaluate_condition(row, condition, 0))
            });
            sort_rows(&mut rows, &returning.order_by);
            if let Some(limit) = returning.limit {
                rows.truncate(limit);
            }
            groups.push(rows.into_iter().map(eval_row_into_query_record).collect());
        }
        transaction.commit().map_err(DatabaseError::storage)?;
        Ok(QueryOutcome::Search(groups))
    }

    pub fn execute_dml(
        &mut self,
        operation: DmlOperation,
        rows: Vec<DmlRow>,
    ) -> Result<Vec<DmlRowOutcome>, DatabaseError> {
        let schema = self.storage.schema().clone();
        let mut transaction = self
            .storage
            .begin_transaction()
            .map_err(DatabaseError::storage)?;
        let mut outcomes = Vec::with_capacity(rows.len());
        let mut recycle_bin = self.recycle_bin.clone();
        let mut sequences = self.sequences.clone();
        for row in rows {
            let input_index = row.input_index;
            match execute_dml_row(
                &schema,
                &mut transaction,
                &mut recycle_bin,
                &mut sequences,
                operation,
                row.record,
            )? {
                Ok(record) => outcomes.push(DmlRowOutcome::success(
                    input_index,
                    record,
                    operation == DmlOperation::Insert,
                )?),
                Err(error) => {
                    outcomes.push(DmlRowOutcome::failure(input_index, vec![error]));
                }
            }
        }
        transaction.commit().map_err(DatabaseError::storage)?;
        self.recycle_bin = recycle_bin;
        self.sequences = sequences;
        outcomes.sort_by_key(|outcome| outcome.input_index);
        Ok(outcomes)
    }
}

fn execute_dml_row<T: StorageTransaction>(
    schema: &SchemaCatalog,
    transaction: &mut T,
    recycle_bin: &mut BTreeMap<(String, RecordId), Record>,
    sequences: &mut BTreeMap<String, u64>,
    operation: DmlOperation,
    mut value: SObject,
) -> Result<Result<SObject, DmlError>, DatabaseError> {
    let object = schema
        .object(value.object_api_name())
        .map_err(DatabaseError::schema)?;
    match operation {
        DmlOperation::Insert => {
            if let Err(error) =
                execute_insert_row(transaction, object, sequences, &mut value, true)?
            {
                return Ok(Err(error));
            }
        }
        DmlOperation::Upsert if value.id().is_none() => {
            if let Err(error) =
                execute_insert_row(transaction, object, sequences, &mut value, false)?
            {
                return Ok(Err(error));
            }
        }
        DmlOperation::Update | DmlOperation::Upsert => {
            value = match execute_update_row(schema, transaction, object, value, operation)? {
                Ok(value) => value,
                Err(error) => return Ok(Err(error)),
            }
        }
        DmlOperation::Undelete => {
            value = match execute_undelete_row(schema, transaction, recycle_bin, object, value)? {
                Ok(value) => value,
                Err(error) => return Ok(Err(error)),
            }
        }
        DmlOperation::Delete => {
            if let Err(error) = execute_delete_row(transaction, recycle_bin, object, &value)? {
                return Ok(Err(error));
            }
        }
    }
    Ok(Ok(value))
}

fn execute_insert_row<T: StorageTransaction>(
    transaction: &mut T,
    object: &super::ObjectSchema,
    sequences: &mut BTreeMap<String, u64>,
    value: &mut SObject,
    reject_supplied_id: bool,
) -> Result<Result<(), DmlError>, DatabaseError> {
    if reject_supplied_id && value.id().is_some() {
        return Ok(Err(DmlError::new(
            DmlStatus::InvalidFieldForInsertUpdate,
            format!(
                "cannot specify Id in an insert call for {}",
                object.api_name()
            ),
            ["Id".to_owned()],
        )));
    }
    assign_generated_id(transaction, object, sequences, value)?;
    validate_and_write(transaction, object, value)
}

fn execute_update_row<T: StorageTransaction>(
    schema: &SchemaCatalog,
    transaction: &mut T,
    object: &super::ObjectSchema,
    value: SObject,
    operation: DmlOperation,
) -> Result<Result<SObject, DmlError>, DatabaseError> {
    let existing = value
        .id()
        .map(|id| transaction.read(object.api_name(), id))
        .transpose()
        .map_err(DatabaseError::storage)?
        .flatten();
    let Some(existing) = existing else {
        return Ok(Err(missing_record_error(
            if operation == DmlOperation::Update {
                "update"
            } else {
                "upsert"
            },
            object.api_name(),
            &value,
        )));
    };
    let incoming = value.into_record().map_err(DatabaseError::sobject)?;
    let mut record = existing;
    for (name, field_value) in incoming.fields() {
        record.set_field(name, field_value.clone());
    }
    let merged = SObject::from_record(schema, record).map_err(DatabaseError::sobject)?;
    if let Err(error) = validate_and_write(transaction, object, &merged)? {
        return Ok(Err(error));
    }
    Ok(Ok(merged))
}

fn execute_undelete_row<T: StorageTransaction>(
    schema: &SchemaCatalog,
    transaction: &mut T,
    recycle_bin: &mut BTreeMap<(String, RecordId), Record>,
    object: &super::ObjectSchema,
    value: SObject,
) -> Result<Result<SObject, DmlError>, DatabaseError> {
    let Some(key) = value
        .id()
        .cloned()
        .map(|id| (object.api_name().to_ascii_lowercase(), id))
    else {
        return Ok(Err(missing_record_error(
            "undelete",
            object.api_name(),
            &value,
        )));
    };
    let Some(record) = recycle_bin.remove(&key) else {
        return Ok(Err(missing_record_error(
            "undelete",
            object.api_name(),
            &value,
        )));
    };
    let merged = merge_sobject(schema, record.clone(), &value)?;
    if let Err(error) = validate_and_write(transaction, object, &merged)? {
        recycle_bin.insert(key, record);
        return Ok(Err(error));
    }
    Ok(Ok(merged))
}

fn execute_delete_row<T: StorageTransaction>(
    transaction: &mut T,
    recycle_bin: &mut BTreeMap<(String, RecordId), Record>,
    object: &super::ObjectSchema,
    value: &SObject,
) -> Result<Result<(), DmlError>, DatabaseError> {
    let Some(id) = value.id() else {
        return Ok(Err(missing_record_error(
            "delete",
            object.api_name(),
            value,
        )));
    };
    let stored = transaction
        .read(object.api_name(), id)
        .map_err(DatabaseError::storage)?;
    if !transaction
        .delete(object.api_name(), id)
        .map_err(DatabaseError::storage)?
    {
        return Ok(Err(missing_record_error(
            "delete",
            object.api_name(),
            value,
        )));
    }
    recycle_bin.insert(
        (object.api_name().to_ascii_lowercase(), id.clone()),
        stored.expect("successful delete had a stored record"),
    );
    Ok(Ok(()))
}

fn validate_and_write<T: StorageTransaction>(
    transaction: &mut T,
    object: &super::ObjectSchema,
    value: &SObject,
) -> Result<Result<(), DmlError>, DatabaseError> {
    if let Err(error) = validate_required_fields(object, value) {
        return Ok(Err(error));
    }
    if let Err(error) = validate_unique_fields(transaction, object, value)? {
        return Ok(Err(error));
    }
    transaction
        .write(
            value
                .clone()
                .into_record()
                .map_err(DatabaseError::sobject)?,
        )
        .map_err(DatabaseError::storage)?;
    Ok(Ok(()))
}

fn assign_generated_id<T: StorageTransaction>(
    transaction: &mut T,
    object: &super::ObjectSchema,
    sequences: &mut BTreeMap<String, u64>,
    value: &mut SObject,
) -> Result<(), DatabaseError> {
    let sequence = sequences
        .entry(object.api_name().to_ascii_lowercase())
        .or_default();
    loop {
        *sequence += 1;
        let id = RecordId::generate(object.key_prefix(), *sequence).map_err(DatabaseError::id)?;
        if transaction
            .read(object.api_name(), &id)
            .map_err(DatabaseError::storage)?
            .is_none()
        {
            value.set_id(id);
            return Ok(());
        }
    }
}

fn validate_required_fields(object: &super::ObjectSchema, value: &SObject) -> Result<(), DmlError> {
    let missing = object
        .fields()
        .filter(|field| {
            !field.api_name().eq_ignore_ascii_case("Id")
                && !field.is_nullable()
                && value
                    .fields()
                    .find(|(name, _)| name.eq_ignore_ascii_case(field.api_name()))
                    .is_none_or(|(_, value)| matches!(value, DataValue::Null))
        })
        .map(|field| field.api_name().to_owned())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(DmlError::new(
            DmlStatus::RequiredFieldMissing,
            format!("required fields are missing: {}", missing.join(", ")),
            missing,
        ))
    }
}

fn validate_unique_fields<T: StorageTransaction>(
    transaction: &mut T,
    object: &super::ObjectSchema,
    value: &SObject,
) -> Result<Result<(), DmlError>, DatabaseError> {
    let records = transaction
        .scan(object.api_name())
        .map_err(DatabaseError::storage)?;
    for field in object.fields().filter(|field| field.is_unique()) {
        let Some(candidate) = value
            .fields()
            .find(|(name, _)| name.eq_ignore_ascii_case(field.api_name()))
            .map(|(_, value)| value)
            .filter(|value| !matches!(value, DataValue::Null))
        else {
            continue;
        };
        let duplicate = records.iter().any(|record| {
            value.id().is_none_or(|id| record.id() != id)
                && record
                    .field(field.api_name())
                    .is_some_and(|stored| values_equal(stored, candidate))
        });
        if duplicate {
            return Ok(Err(DmlError::new(
                DmlStatus::DuplicateValue,
                format!(
                    "duplicate value on unique field {}.{}",
                    object.api_name(),
                    field.api_name()
                ),
                [field.api_name().to_owned()],
            )));
        }
    }
    Ok(Ok(()))
}

fn prepare_dml_row<T: StorageTransaction>(
    schema: &SchemaCatalog,
    transaction: &mut T,
    recycle_bin: &BTreeMap<(String, RecordId), Record>,
    request: &DmlRequest,
    row: &DmlRow,
) -> Result<Result<PreparedDmlRecord, DmlError>, DatabaseError> {
    let value = &row.record;
    let object = schema
        .object(value.object_api_name())
        .map_err(DatabaseError::schema)?;
    let existing = match resolve_dml_existing(schema, transaction, object, request, value)? {
        Ok(existing) => existing,
        Err(error) => return Ok(Err(error)),
    };
    match request.operation {
        DmlOperation::Insert => prepare_insert(row, object),
        DmlOperation::Update => prepare_update(schema, row, object, existing),
        DmlOperation::Upsert if existing.is_some() => {
            prepare_upsert_update(schema, row, existing.expect("checked existing upsert"))
        }
        DmlOperation::Upsert => Ok(Ok(prepared_insert(row))),
        DmlOperation::Delete => prepare_delete(schema, row, object, existing),
        DmlOperation::Undelete => prepare_undelete(schema, row, object, recycle_bin),
    }
}

fn resolve_dml_existing<T: StorageTransaction>(
    schema: &SchemaCatalog,
    transaction: &mut T,
    object: &super::ObjectSchema,
    request: &DmlRequest,
    value: &SObject,
) -> Result<Result<Option<Record>, DmlError>, DatabaseError> {
    if request.operation == DmlOperation::Upsert
        && let Some(external_id) = &request.external_id
    {
        return external_id_match(schema, transaction, object.api_name(), external_id, value);
    }
    let existing = value
        .id()
        .map(|id| transaction.read(object.api_name(), id))
        .transpose()
        .map_err(DatabaseError::storage)?
        .flatten();
    Ok(Ok(existing))
}

fn prepare_insert(
    row: &DmlRow,
    object: &super::ObjectSchema,
) -> Result<Result<PreparedDmlRecord, DmlError>, DatabaseError> {
    if row.record.id().is_some() {
        return Ok(Err(DmlError::new(
            DmlStatus::InvalidFieldForInsertUpdate,
            format!(
                "cannot specify Id in an insert call for {}",
                object.api_name()
            ),
            ["Id".to_owned()],
        )));
    }
    Ok(Ok(prepared_insert(row)))
}

fn prepared_insert(row: &DmlRow) -> PreparedDmlRecord {
    PreparedDmlRecord {
        input_index: row.input_index,
        operation: DmlOperation::Insert,
        created: true,
        old: None,
        new: Some(row.record.clone()),
    }
}

fn prepare_update(
    schema: &SchemaCatalog,
    row: &DmlRow,
    object: &super::ObjectSchema,
    existing: Option<Record>,
) -> Result<Result<PreparedDmlRecord, DmlError>, DatabaseError> {
    let Some(stored) = existing else {
        return Ok(Err(missing_record_error(
            "update",
            object.api_name(),
            &row.record,
        )));
    };
    prepare_upsert_update(schema, row, stored)
}

fn prepare_upsert_update(
    schema: &SchemaCatalog,
    row: &DmlRow,
    stored: Record,
) -> Result<Result<PreparedDmlRecord, DmlError>, DatabaseError> {
    let old = SObject::from_record(schema, stored.clone()).map_err(DatabaseError::sobject)?;
    let new = merge_sobject(schema, stored, &row.record)?;
    Ok(Ok(PreparedDmlRecord {
        input_index: row.input_index,
        operation: DmlOperation::Update,
        created: false,
        old: Some(old),
        new: Some(new),
    }))
}

fn prepare_delete(
    schema: &SchemaCatalog,
    row: &DmlRow,
    object: &super::ObjectSchema,
    existing: Option<Record>,
) -> Result<Result<PreparedDmlRecord, DmlError>, DatabaseError> {
    let Some(stored) = existing else {
        return Ok(Err(missing_record_error(
            "delete",
            object.api_name(),
            &row.record,
        )));
    };
    Ok(Ok(PreparedDmlRecord {
        input_index: row.input_index,
        operation: DmlOperation::Delete,
        created: false,
        old: Some(SObject::from_record(schema, stored).map_err(DatabaseError::sobject)?),
        new: None,
    }))
}

fn prepare_undelete(
    schema: &SchemaCatalog,
    row: &DmlRow,
    object: &super::ObjectSchema,
    recycle_bin: &BTreeMap<(String, RecordId), Record>,
) -> Result<Result<PreparedDmlRecord, DmlError>, DatabaseError> {
    let recycled = row
        .record
        .id()
        .and_then(|id| recycle_bin.get(&(object.api_name().to_ascii_lowercase(), id.clone())));
    let Some(stored) = recycled else {
        return Ok(Err(missing_record_error(
            "undelete",
            object.api_name(),
            &row.record,
        )));
    };
    Ok(Ok(PreparedDmlRecord {
        input_index: row.input_index,
        operation: DmlOperation::Undelete,
        created: false,
        old: None,
        new: Some(SObject::from_record(schema, stored.clone()).map_err(DatabaseError::sobject)?),
    }))
}

fn external_id_match<T: StorageTransaction>(
    schema: &SchemaCatalog,
    transaction: &mut T,
    object_api_name: &str,
    external_id: &DmlExternalId,
    value: &SObject,
) -> Result<Result<Option<Record>, DmlError>, DatabaseError> {
    if !external_id.object.eq_ignore_ascii_case(object_api_name) {
        return Ok(Err(DmlError::new(
            DmlStatus::InvalidFieldForInsertUpdate,
            format!(
                "external ID field {}.{} does not belong to {}",
                external_id.object, external_id.field, object_api_name
            ),
            [external_id.field.clone()],
        )));
    }
    let field = schema
        .field(object_api_name, &external_id.field)
        .map_err(DatabaseError::schema)?;
    if !field.is_external_id() {
        return Ok(Err(DmlError::new(
            DmlStatus::InvalidFieldForInsertUpdate,
            format!(
                "{}.{} is not configured as an external ID",
                object_api_name,
                field.api_name()
            ),
            [field.api_name().to_owned()],
        )));
    }
    let external_value = value
        .get(schema, field.api_name())
        .map_err(DatabaseError::sobject)?;
    let Some(external_value) = external_value.filter(|value| !matches!(value, DataValue::Null))
    else {
        return Ok(Err(DmlError::new(
            DmlStatus::MissingArgument,
            format!(
                "external ID field {}.{} must have a value",
                object_api_name,
                field.api_name()
            ),
            [field.api_name().to_owned()],
        )));
    };
    let matches = transaction
        .scan(object_api_name)
        .map_err(DatabaseError::storage)?
        .into_iter()
        .filter(|record| {
            record
                .field(field.api_name())
                .is_some_and(|stored| values_equal(stored, external_value))
        })
        .collect::<Vec<_>>();
    match matches.len() {
        0 => Ok(Ok(None)),
        1 => Ok(Ok(matches.into_iter().next())),
        _ => Ok(Err(DmlError::new(
            DmlStatus::DuplicateExternalId,
            format!(
                "external ID field {}.{} matches multiple records",
                object_api_name,
                field.api_name()
            ),
            [field.api_name().to_owned()],
        ))),
    }
}

fn missing_record_error(operation: &str, object: &str, value: &SObject) -> DmlError {
    DmlError::new(
        DmlStatus::InvalidCrossReferenceKey,
        format!(
            "{operation} could not find {object} record `{}`",
            value
                .id()
                .map(ToString::to_string)
                .unwrap_or_else(|| "null".to_owned())
        ),
        ["Id".to_owned()],
    )
}

fn merge_sobject(
    schema: &SchemaCatalog,
    stored: Record,
    incoming: &SObject,
) -> Result<SObject, DatabaseError> {
    let mut merged = SObject::from_record(schema, stored).map_err(DatabaseError::sobject)?;
    for (name, value) in incoming.fields() {
        merged
            .set(schema, name, value.clone())
            .map_err(DatabaseError::sobject)?;
    }
    Ok(merged)
}

#[derive(Clone)]
struct EvalRow {
    record: Record,
    relationships: BTreeMap<String, Box<EvalRow>>,
    children: BTreeMap<String, Vec<EvalRow>>,
}

fn hydrate_rows<T: StorageTransaction>(
    schema: &SchemaCatalog,
    transaction: &mut T,
    records: Vec<Record>,
    parent_depth: usize,
) -> Result<Vec<EvalRow>, DatabaseError> {
    let mut rows = Vec::with_capacity(records.len());
    for record in records {
        rows.push(hydrate_row(schema, transaction, record, parent_depth)?);
    }
    Ok(rows)
}

fn hydrate_row<T: StorageTransaction>(
    schema: &SchemaCatalog,
    transaction: &mut T,
    mut record: Record,
    parent_depth: usize,
) -> Result<EvalRow, DatabaseError> {
    record.set_field("Id", DataValue::Id(record.id().clone()));
    let object = schema
        .object(record.object_api_name())
        .map_err(DatabaseError::schema)?;
    let mut relationships = BTreeMap::new();
    if parent_depth > 0 {
        for field in object.fields() {
            let FieldType::Reference { target_object } = field.data_type() else {
                continue;
            };
            let Some(id) = record.field(field.api_name()).and_then(data_record_id) else {
                continue;
            };
            if let Some(related) = transaction
                .read(target_object, &id)
                .map_err(DatabaseError::storage)?
            {
                relationships.insert(
                    field.api_name().to_ascii_lowercase(),
                    Box::new(hydrate_row(schema, transaction, related, parent_depth - 1)?),
                );
            }
        }
    }
    Ok(EvalRow {
        record,
        relationships,
        children: BTreeMap::new(),
    })
}

fn data_record_id(value: &DataValue) -> Option<RecordId> {
    match value {
        DataValue::Id(id) => Some(id.clone()),
        DataValue::String(id) => RecordId::parse(id.clone()).ok(),
        _ => None,
    }
}

fn hydrate_summary_fields<T: StorageTransaction>(
    schema: &SchemaCatalog,
    transaction: &mut T,
    parent_object: &str,
    parents: &mut [EvalRow],
) -> Result<usize, DatabaseError> {
    let object = schema
        .object(parent_object)
        .map_err(DatabaseError::schema)?;
    let summaries = object
        .fields()
        .filter_map(|field| {
            let FieldType::Summary {
                result_type,
                definition,
            } = field.data_type()
            else {
                return None;
            };
            Some((
                field.api_name().to_owned(),
                result_type.as_ref().clone(),
                definition.clone(),
            ))
        })
        .collect::<Vec<_>>();
    if summaries.is_empty() || parents.is_empty() {
        return Ok(0);
    }

    let mut children_by_object = BTreeMap::<String, Vec<Record>>::new();
    for (_, _, definition) in &summaries {
        let canonical = definition.child_object.to_ascii_lowercase();
        if !children_by_object.contains_key(&canonical) {
            children_by_object.insert(
                canonical,
                transaction
                    .scan(&definition.child_object)
                    .map_err(DatabaseError::storage)?,
            );
        }
    }
    let scans = children_by_object.len();
    let parent_ids = parents
        .iter()
        .map(|parent| parent.record.id().to_string())
        .collect::<BTreeSet<_>>();

    for (field, result_type, definition) in summaries {
        let initial = match definition.operation {
            SummaryOperation::Count | SummaryOperation::Sum => DataValue::Integer(0),
            SummaryOperation::Min | SummaryOperation::Max => DataValue::Null,
        };
        let mut values = parent_ids
            .iter()
            .map(|id| (id.clone(), initial.clone()))
            .collect::<BTreeMap<_, _>>();
        let children = children_by_object
            .get(&definition.child_object.to_ascii_lowercase())
            .expect("summary child records were scanned");
        for child in children {
            let Some(parent_id) = child
                .field(&definition.foreign_key_field)
                .and_then(data_record_id)
                .map(|id| id.to_string())
            else {
                continue;
            };
            let Some(current) = values.get_mut(&parent_id) else {
                continue;
            };
            if !summary_filters_match(schema, &definition, child)? {
                continue;
            }
            match definition.operation {
                SummaryOperation::Count => {
                    let DataValue::Integer(count) = current else {
                        unreachable!("count roll-up accumulator is Integer")
                    };
                    *count = count.checked_add(1).ok_or_else(|| {
                        DatabaseError::new(format!(
                            "roll-up summary `{parent_object}.{field}` overflowed Integer"
                        ))
                    })?;
                }
                SummaryOperation::Sum => {
                    let Some(DataValue::Integer(value)) = definition
                        .summarized_field
                        .as_deref()
                        .and_then(|field| child.field(field))
                    else {
                        continue;
                    };
                    let DataValue::Integer(sum) = current else {
                        unreachable!("sum roll-up accumulator is Integer")
                    };
                    *sum = sum.checked_add(*value).ok_or_else(|| {
                        DatabaseError::new(format!(
                            "roll-up summary `{parent_object}.{field}` overflowed Integer"
                        ))
                    })?;
                }
                SummaryOperation::Min | SummaryOperation::Max => {
                    let Some(candidate) = definition
                        .summarized_field
                        .as_deref()
                        .and_then(|field| child.field(field))
                        .filter(|value| !matches!(value, DataValue::Null))
                    else {
                        continue;
                    };
                    let replace = matches!(current, DataValue::Null)
                        || matches!(
                            (definition.operation, compare_values(candidate, current)),
                            (SummaryOperation::Min, Ordering::Less)
                                | (SummaryOperation::Max, Ordering::Greater)
                        );
                    if replace {
                        *current = candidate.clone();
                    }
                }
            }
        }
        for parent in parents.iter_mut() {
            let value = values
                .remove(parent.record.id().as_str())
                .unwrap_or(initial.clone());
            debug_assert!(summary_value_matches(&result_type, &value));
            parent.record.set_field(&field, value);
        }
    }
    Ok(scans)
}

fn summary_filters_match(
    schema: &SchemaCatalog,
    definition: &SummaryDefinition,
    child: &Record,
) -> Result<bool, DatabaseError> {
    let object = schema
        .object(&definition.child_object)
        .map_err(DatabaseError::schema)?;
    for filter in &definition.filters {
        let field = object.field(&filter.field).map_err(DatabaseError::schema)?;
        let Some(expected) = summary_filter_value(field.data_type(), &filter.value) else {
            return Err(DatabaseError::new(format!(
                "invalid roll-up summary filter value `{}` for {}.{}",
                filter.value,
                object.api_name(),
                field.api_name()
            )));
        };
        let actual = child.field(field.api_name()).unwrap_or(&DataValue::Null);
        let matches = match filter.operator {
            SummaryFilterOperator::Equal => values_equal(actual, &expected),
        };
        if !matches {
            return Ok(false);
        }
    }
    Ok(true)
}

fn summary_filter_value(field_type: &FieldType, value: &str) -> Option<DataValue> {
    match field_type {
        FieldType::Boolean if value.eq_ignore_ascii_case("true") => Some(DataValue::Boolean(true)),
        FieldType::Boolean if value.eq_ignore_ascii_case("false") => {
            Some(DataValue::Boolean(false))
        }
        FieldType::Boolean => None,
        FieldType::Integer => value.parse::<i64>().ok().map(DataValue::Integer),
        FieldType::String | FieldType::MetadataRelationship { .. } => {
            Some(DataValue::String(value.to_owned()))
        }
        FieldType::Date => {
            let epoch = NaiveDate::from_ymd_opt(1970, 1, 1)?;
            let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
            i32::try_from(date.signed_duration_since(epoch).num_days())
                .ok()
                .map(DataValue::Date)
        }
        FieldType::Datetime => chrono::DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|value| DataValue::Datetime(value.timestamp_millis())),
        FieldType::Id | FieldType::Reference { .. } => Some(DataValue::String(value.to_owned())),
        FieldType::Summary { .. } => None,
    }
}

fn summary_value_matches(field_type: &FieldType, value: &DataValue) -> bool {
    matches!(
        (field_type, value),
        (_, DataValue::Null)
            | (FieldType::Boolean, DataValue::Boolean(_))
            | (FieldType::Integer, DataValue::Integer(_))
            | (FieldType::String, DataValue::String(_))
            | (FieldType::Date, DataValue::Date(_))
            | (FieldType::Datetime, DataValue::Datetime(_))
            | (FieldType::Id, DataValue::Id(_) | DataValue::String(_))
            | (
                FieldType::Reference { .. },
                DataValue::Id(_) | DataValue::String(_)
            )
            | (FieldType::MetadataRelationship { .. }, DataValue::String(_))
    )
}

fn field_value<'row>(row: &'row EvalRow, field: &QueryField) -> &'row DataValue {
    static NULL: DataValue = DataValue::Null;
    let mut current = row;
    for relationship in &field.relationships {
        let Some(related) = current
            .relationships
            .get(&relationship.reference_field.to_ascii_lowercase())
        else {
            return &NULL;
        };
        current = related;
    }
    current.record.field(&field.field).unwrap_or(&NULL)
}

fn evaluate_condition(row: &EvalRow, condition: &QueryCondition, now_millis: i64) -> bool {
    match condition {
        QueryCondition::Comparison {
            left,
            operator,
            right,
        } => compare(field_value(row, left), right, *operator, now_millis),
        QueryCondition::In {
            field,
            negated,
            values: QueryInValues::Values(values),
        } => {
            let found = values.iter().any(|value| {
                compare(
                    field_value(row, field),
                    value,
                    QueryComparison::Equal,
                    now_millis,
                )
            });
            found != *negated
        }
        QueryCondition::Not(condition) => !evaluate_condition(row, condition, now_millis),
        QueryCondition::Logical {
            left,
            operator,
            right,
        } => match operator {
            QueryLogical::And => {
                evaluate_condition(row, left, now_millis)
                    && evaluate_condition(row, right, now_millis)
            }
            QueryLogical::Or => {
                evaluate_condition(row, left, now_millis)
                    || evaluate_condition(row, right, now_millis)
            }
        },
    }
}

fn compare(
    left: &DataValue,
    right: &QueryValue,
    operator: QueryComparison,
    now_millis: i64,
) -> bool {
    if let QueryValue::DateLiteral(literal) = right {
        return compare_date_literal(left, *literal, operator, now_millis);
    }
    let QueryValue::Data(right) = right else {
        unreachable!("date literal handled above")
    };
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
        (DataValue::Date(left), DataValue::Date(right)) => left.cmp(right),
        (DataValue::Datetime(left), DataValue::Datetime(right)) => left.cmp(right),
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
        DataValue::Date(_) => 3,
        DataValue::Datetime(_) => 4,
        DataValue::String(_) => 5,
        DataValue::Id(_) => 6,
    }
}

fn compare_date_literal(
    left: &DataValue,
    literal: QueryDateLiteral,
    operator: QueryComparison,
    now_millis: i64,
) -> bool {
    let Some(value) = data_datetime_millis(left) else {
        return false;
    };
    let Some((start, end)) = date_literal_range(literal, now_millis) else {
        return false;
    };
    match operator {
        QueryComparison::Equal => value >= start && value < end,
        QueryComparison::NotEqual => value < start || value >= end,
        QueryComparison::Less => value < start,
        QueryComparison::LessEqual => value < end,
        QueryComparison::Greater => value >= end,
        QueryComparison::GreaterEqual => value >= start,
        QueryComparison::Like => false,
    }
}

fn data_datetime_millis(value: &DataValue) -> Option<i64> {
    match value {
        DataValue::Datetime(value) => Some(*value),
        DataValue::Date(days) => Some(i64::from(*days) * 86_400_000),
        _ => None,
    }
}

fn date_literal_range(literal: QueryDateLiteral, now_millis: i64) -> Option<(i64, i64)> {
    let today = Utc.timestamp_millis_opt(now_millis).single()?.date_naive();
    let one_day = Duration::days(1);
    let (start, end) = match literal.kind {
        QueryDateLiteralKind::Yesterday => (today - one_day, today),
        QueryDateLiteralKind::Today => (today, today + one_day),
        QueryDateLiteralKind::Tomorrow => (today + one_day, today + one_day * 2),
        QueryDateLiteralKind::LastNDays => {
            let amount = literal.amount?;
            (today - Duration::days(amount), today + one_day)
        }
        QueryDateLiteralKind::NextNDays => {
            let amount = literal.amount?;
            (today + one_day, today + Duration::days(amount + 1))
        }
        QueryDateLiteralKind::ThisWeek
        | QueryDateLiteralKind::LastWeek
        | QueryDateLiteralKind::NextWeek => week_literal_range(literal.kind, today),
        QueryDateLiteralKind::ThisMonth
        | QueryDateLiteralKind::LastMonth
        | QueryDateLiteralKind::NextMonth => month_literal_range(literal.kind, today)?,
        QueryDateLiteralKind::ThisYear
        | QueryDateLiteralKind::LastYear
        | QueryDateLiteralKind::NextYear => year_literal_range(literal.kind, today)?,
    };
    Some((date_millis(start)?, date_millis(end)?))
}

fn week_literal_range(kind: QueryDateLiteralKind, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let since_sunday = match today.weekday() {
        Weekday::Sun => 0,
        weekday => i64::from(weekday.num_days_from_sunday()),
    };
    let this_week = today - Duration::days(since_sunday);
    match kind {
        QueryDateLiteralKind::ThisWeek => (this_week, this_week + Duration::days(7)),
        QueryDateLiteralKind::LastWeek => (this_week - Duration::days(7), this_week),
        QueryDateLiteralKind::NextWeek => (
            this_week + Duration::days(7),
            this_week + Duration::days(14),
        ),
        _ => unreachable!("week helper receives a week literal"),
    }
}

fn month_literal_range(
    kind: QueryDateLiteralKind,
    today: NaiveDate,
) -> Option<(NaiveDate, NaiveDate)> {
    let this_month = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)?;
    match kind {
        QueryDateLiteralKind::ThisMonth => Some((this_month, shift_month(this_month, 1)?)),
        QueryDateLiteralKind::LastMonth => Some((shift_month(this_month, -1)?, this_month)),
        QueryDateLiteralKind::NextMonth => {
            Some((shift_month(this_month, 1)?, shift_month(this_month, 2)?))
        }
        _ => unreachable!("month helper receives a month literal"),
    }
}

fn year_literal_range(
    kind: QueryDateLiteralKind,
    today: NaiveDate,
) -> Option<(NaiveDate, NaiveDate)> {
    let this_year = NaiveDate::from_ymd_opt(today.year(), 1, 1)?;
    let year_start = |delta: i32| NaiveDate::from_ymd_opt(today.year().checked_add(delta)?, 1, 1);
    match kind {
        QueryDateLiteralKind::ThisYear => Some((this_year, year_start(1)?)),
        QueryDateLiteralKind::LastYear => Some((year_start(-1)?, this_year)),
        QueryDateLiteralKind::NextYear => Some((year_start(1)?, year_start(2)?)),
        _ => unreachable!("year helper receives a year literal"),
    }
}

fn shift_month(date: NaiveDate, delta: i32) -> Option<NaiveDate> {
    let month_index = date.year().checked_mul(12)? + i32::try_from(date.month0()).ok()? + delta;
    let year = month_index.div_euclid(12);
    let month = u32::try_from(month_index.rem_euclid(12)).ok()? + 1;
    NaiveDate::from_ymd_opt(year, month, 1)
}

fn date_millis(date: NaiveDate) -> Option<i64> {
    Some(
        Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0)?)
            .timestamp_millis(),
    )
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

fn apply_row_window(rows: &mut Vec<EvalRow>, request: &SoqlRequest) {
    sort_rows(rows, &request.order_by);
    let start = request.offset.min(rows.len());
    *rows = rows.split_off(start);
    if let Some(limit) = request.limit {
        rows.truncate(limit);
    }
}

fn requested_parent_depth(request: &SoqlRequest) -> usize {
    let mut depth = request
        .select
        .iter()
        .filter_map(|item| match item {
            QuerySelect::Field(field) => Some(field.relationships.len()),
            QuerySelect::Aggregate {
                field: Some(field), ..
            } => Some(field.relationships.len()),
            QuerySelect::Subquery { .. } | QuerySelect::Aggregate { field: None, .. } => None,
        })
        .chain(
            request
                .group_by
                .iter()
                .chain(request.order_by.iter().map(|order| &order.field))
                .map(|field| field.relationships.len()),
        )
        .max()
        .unwrap_or(0);
    if let Some(condition) = &request.condition {
        depth = depth.max(condition_parent_depth(condition));
    }
    if let Some(condition) = &request.having {
        depth = depth.max(condition_parent_depth(condition));
    }
    depth
}

fn condition_parent_depth(condition: &QueryCondition) -> usize {
    match condition {
        QueryCondition::Comparison { left, .. } => left.relationships.len(),
        QueryCondition::In { field, .. } => field.relationships.len(),
        QueryCondition::Not(condition) => condition_parent_depth(condition),
        QueryCondition::Logical { left, right, .. } => {
            condition_parent_depth(left).max(condition_parent_depth(right))
        }
    }
}

fn hydrate_child_subqueries<T: StorageTransaction>(
    schema: &SchemaCatalog,
    transaction: &mut T,
    parents: &mut [EvalRow],
    request: &SoqlRequest,
) -> Result<usize, DatabaseError> {
    let mut scans = 0;
    for item in &request.select {
        let QuerySelect::Subquery {
            relationship,
            reference_field,
            query,
        } = item
        else {
            continue;
        };
        let records = transaction
            .scan(&query.object)
            .map_err(DatabaseError::storage)?;
        scans += 1;
        let mut children =
            hydrate_rows(schema, transaction, records, requested_parent_depth(query))?;
        scans += hydrate_summary_fields(schema, transaction, &query.object, &mut children)?;
        if let Some(condition) = &query.condition {
            children.retain(|row| evaluate_condition(row, condition, request.now_millis));
        }
        let mut grouped = BTreeMap::<String, Vec<EvalRow>>::new();
        for child in children {
            let Some(parent_id) = child.record.field(reference_field).and_then(data_record_id)
            else {
                continue;
            };
            grouped
                .entry(parent_id.to_string())
                .or_default()
                .push(child);
        }
        for group in grouped.values_mut() {
            apply_row_window(group, query);
        }
        for parent in parents.iter_mut() {
            parent.children.insert(
                relationship.to_ascii_lowercase(),
                grouped
                    .remove(parent.record.id().as_str())
                    .unwrap_or_default(),
            );
        }
    }
    Ok(scans)
}

fn eval_row_into_query_record(row: EvalRow) -> QueryRecord {
    QueryRecord {
        record: row.record,
        relationships: row
            .relationships
            .into_iter()
            .map(|(name, related)| (name, eval_row_into_query_record(*related)))
            .collect(),
        children: row
            .children
            .into_iter()
            .map(|(name, children)| {
                (
                    name,
                    children
                        .into_iter()
                        .map(eval_row_into_query_record)
                        .collect(),
                )
            })
            .collect(),
    }
}

fn evaluate_aggregate_condition(
    row: &BTreeMap<String, DataValue>,
    condition: &QueryCondition,
    now_millis: i64,
) -> bool {
    match condition {
        QueryCondition::Comparison {
            left,
            operator,
            right,
        } => compare(
            row.get(&left.field.to_ascii_lowercase())
                .unwrap_or(&DataValue::Null),
            right,
            *operator,
            now_millis,
        ),
        QueryCondition::In {
            field,
            negated,
            values: QueryInValues::Values(values),
        } => {
            let actual = row
                .get(&field.field.to_ascii_lowercase())
                .unwrap_or(&DataValue::Null);
            values
                .iter()
                .any(|value| compare(actual, value, QueryComparison::Equal, now_millis))
                != *negated
        }
        QueryCondition::Not(condition) => !evaluate_aggregate_condition(row, condition, now_millis),
        QueryCondition::Logical {
            left,
            operator,
            right,
        } => match operator {
            QueryLogical::And => {
                evaluate_aggregate_condition(row, left, now_millis)
                    && evaluate_aggregate_condition(row, right, now_millis)
            }
            QueryLogical::Or => {
                evaluate_aggregate_condition(row, left, now_millis)
                    || evaluate_aggregate_condition(row, right, now_millis)
            }
        },
    }
}

fn sort_aggregate_rows(rows: &mut [BTreeMap<String, DataValue>], ordering: &[QueryOrder]) {
    rows.sort_by(|left, right| {
        for order in ordering {
            let left = left
                .get(&order.field.field.to_ascii_lowercase())
                .unwrap_or(&DataValue::Null);
            let right = right
                .get(&order.field.field.to_ascii_lowercase())
                .unwrap_or(&DataValue::Null);
            let compared = compare_values(left, right);
            let compared = if order.direction == SortOrder::Descending {
                compared.reverse()
            } else {
                compared
            };
            if compared != Ordering::Equal {
                return compared;
            }
        }
        Ordering::Equal
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
                    QuerySelect::Subquery { .. } => {
                        unreachable!("checker rejects child subqueries in aggregate queries")
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
    pub(crate) fn new(message: impl Into<String>) -> Self {
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
