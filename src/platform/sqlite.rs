use super::{
    DataValue, FieldSchema, FieldType, ObjectSchema, Record, RecordId, SchemaCatalog, SchemaError,
    Storage, StorageTransaction, SummaryDefinition, SummaryFilterOperator, SummaryOperation,
};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use rusqlite::{
    Connection, OptionalExtension, Transaction, params, params_from_iter,
    types::{Value as SqlValue, ValueRef},
};
use std::{error::Error, fmt, path::Path};

/// SQLite-backed storage migrated from a normalized [`SchemaCatalog`].
pub struct SqliteStorage {
    connection: Connection,
    schema: SchemaCatalog,
}

impl SqliteStorage {
    pub fn open(path: impl AsRef<Path>, schema: SchemaCatalog) -> Result<Self, SqliteError> {
        let mut connection = Connection::open(path).map_err(SqliteError::database)?;
        configure(&connection)?;
        migrate(&mut connection, &schema)?;
        Ok(Self { connection, schema })
    }

    pub fn in_memory(schema: SchemaCatalog) -> Result<Self, SqliteError> {
        let mut connection = Connection::open_in_memory().map_err(SqliteError::database)?;
        configure(&connection)?;
        migrate(&mut connection, &schema)?;
        Ok(Self { connection, schema })
    }

    pub fn schema(&self) -> &SchemaCatalog {
        &self.schema
    }

    /// Apply an additive schema migration and make the new catalog active.
    pub fn migrate(&mut self, schema: SchemaCatalog) -> Result<(), SqliteError> {
        migrate(&mut self.connection, &schema)?;
        self.schema = schema;
        Ok(())
    }

    /// Replace all local data with a deterministic fixture in one transaction.
    pub fn load_fixture(
        &mut self,
        records: impl IntoIterator<Item = Record>,
    ) -> Result<(), SqliteError> {
        let mut transaction = self.begin_transaction()?;
        transaction.clear_records()?;
        for record in records {
            transaction.write(record)?;
        }
        transaction.commit()
    }

    #[cfg(test)]
    fn connection(&self) -> &Connection {
        &self.connection
    }
}

pub struct SqliteStorageTransaction<'storage> {
    transaction: Transaction<'storage>,
    schema: &'storage SchemaCatalog,
}

impl SqliteStorageTransaction<'_> {
    fn clear_records(&mut self) -> Result<(), SqliteError> {
        for object in self.schema.objects() {
            self.transaction
                .execute(&format!("DELETE FROM {}", quote(object.api_name())), [])
                .map_err(SqliteError::database)?;
        }
        Ok(())
    }
}

impl Storage for SqliteStorage {
    type Error = SqliteError;
    type Transaction<'storage> = SqliteStorageTransaction<'storage>;

    fn begin_transaction(&mut self) -> Result<Self::Transaction<'_>, Self::Error> {
        let schema = &self.schema;
        let transaction = self
            .connection
            .transaction()
            .map_err(SqliteError::database)?;
        Ok(SqliteStorageTransaction {
            transaction,
            schema,
        })
    }

    fn reset(&mut self) -> Result<(), Self::Error> {
        let mut transaction = self.begin_transaction()?;
        transaction.clear_records()?;
        transaction.commit()
    }
}

impl StorageTransaction for SqliteStorageTransaction<'_> {
    type Error = SqliteError;

    fn read(
        &mut self,
        object_api_name: &str,
        id: &RecordId,
    ) -> Result<Option<Record>, Self::Error> {
        let object = self.schema.object(object_api_name)?;
        let fields = object
            .fields()
            .filter(|field| is_stored_field(field))
            .collect::<Vec<_>>();
        let mut sql = format!(
            "SELECT {} FROM {} WHERE {} = ?1",
            quote("Id"),
            quote(object.api_name()),
            quote("Id")
        );
        for field in &fields {
            let insertion = format!(", {}", quote(field.api_name()));
            sql.insert_str(sql.find(" FROM ").expect("SELECT has FROM"), &insertion);
        }
        let mut statement = self
            .transaction
            .prepare(&sql)
            .map_err(SqliteError::database)?;
        statement
            .query_row([id.as_str()], |row| {
                let stored_id: String = row.get(0)?;
                let mut record = Record::new(object.api_name(), RecordId::new(stored_id));
                for (index, field) in fields.iter().enumerate() {
                    let value = decode_value(field, row.get_ref(index + 1)?)?;
                    record.set_field(field.api_name(), value);
                }
                Ok(record)
            })
            .optional()
            .map_err(SqliteError::database)
    }

    fn write(&mut self, record: Record) -> Result<(), Self::Error> {
        let object = self.schema.object(record.object_api_name())?;
        RecordId::parse(record.id().to_string()).map_err(SqliteError::invalid_id)?;
        for (name, value) in record.fields() {
            let field = object.field(name)?;
            if matches!(field.data_type(), FieldType::Summary { .. }) {
                return Err(SqliteError::ReadOnlyField {
                    object: object.api_name().to_owned(),
                    field: field.api_name().to_owned(),
                });
            }
            validate_data_value(object.api_name(), field, value)?;
        }

        let fields = object
            .fields()
            .filter(|field| is_stored_field(field))
            .collect::<Vec<_>>();
        let mut values = vec![SqlValue::Text(record.id().to_string())];
        for field in &fields {
            let value = record.field(field.api_name()).unwrap_or(&DataValue::Null);
            if !field.is_nullable() && matches!(value, DataValue::Null) {
                return Err(SqliteError::MissingRequiredField {
                    object: object.api_name().to_owned(),
                    field: field.api_name().to_owned(),
                });
            }
            validate_data_value(object.api_name(), field, value)?;
            values.push(encode_value(value));
        }

        let columns = std::iter::once(quote("Id"))
            .chain(fields.iter().map(|field| quote(field.api_name())))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = (1..=values.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let conflict = if fields.is_empty() {
            "DO NOTHING".to_owned()
        } else {
            format!(
                "DO UPDATE SET {}",
                fields
                    .iter()
                    .map(|field| {
                        let name = quote(field.api_name());
                        format!("{name} = excluded.{name}")
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let sql = format!(
            "INSERT INTO {} ({columns}) VALUES ({placeholders}) ON CONFLICT({}) {conflict}",
            quote(object.api_name()),
            quote("Id")
        );
        self.transaction
            .execute(&sql, params_from_iter(values))
            .map_err(SqliteError::database)?;
        Ok(())
    }

    fn scan(&mut self, object_api_name: &str) -> Result<Vec<Record>, Self::Error> {
        let object = self.schema.object(object_api_name)?;
        let fields = object
            .fields()
            .filter(|field| is_stored_field(field))
            .collect::<Vec<_>>();
        let columns = std::iter::once(quote("Id"))
            .chain(fields.iter().map(|field| quote(field.api_name())))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT {columns} FROM {} ORDER BY {}",
            quote(object.api_name()),
            quote("Id")
        );
        let mut statement = self
            .transaction
            .prepare(&sql)
            .map_err(SqliteError::database)?;
        let rows = statement
            .query_map([], |row| {
                let stored_id: String = row.get(0)?;
                let mut record = Record::new(object.api_name(), RecordId::new(stored_id));
                for (index, field) in fields.iter().enumerate() {
                    let value = decode_value(field, row.get_ref(index + 1)?)?;
                    record.set_field(field.api_name(), value);
                }
                Ok(record)
            })
            .map_err(SqliteError::database)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(SqliteError::database)
    }

    fn delete(&mut self, object_api_name: &str, id: &RecordId) -> Result<bool, Self::Error> {
        let object = self.schema.object(object_api_name)?;
        let changed = self
            .transaction
            .execute(
                &format!(
                    "DELETE FROM {} WHERE {} = ?1",
                    quote(object.api_name()),
                    quote("Id")
                ),
                [id.as_str()],
            )
            .map_err(SqliteError::database)?;
        Ok(changed != 0)
    }

    fn savepoint(&mut self, name: &str) -> Result<(), Self::Error> {
        validate_savepoint(name)?;
        self.transaction
            .execute_batch(&format!("SAVEPOINT {}", quote(name)))
            .map_err(SqliteError::database)
    }

    fn rollback_to(&mut self, name: &str) -> Result<(), Self::Error> {
        validate_savepoint(name)?;
        self.transaction
            .execute_batch(&format!("ROLLBACK TO SAVEPOINT {}", quote(name)))
            .map_err(SqliteError::database)
    }

    fn release_savepoint(&mut self, name: &str) -> Result<(), Self::Error> {
        validate_savepoint(name)?;
        self.transaction
            .execute_batch(&format!("RELEASE SAVEPOINT {}", quote(name)))
            .map_err(SqliteError::database)
    }

    fn commit(self) -> Result<(), Self::Error> {
        self.transaction.commit().map_err(SqliteError::database)
    }

    fn rollback(self) -> Result<(), Self::Error> {
        self.transaction.rollback().map_err(SqliteError::database)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum SqliteError {
    Database(rusqlite::Error),
    Schema(SchemaError),
    InvalidId(super::RecordIdError),
    IncompatibleMigration {
        object: String,
        field: String,
        existing: String,
        requested: String,
    },
    InvalidFieldValue {
        object: String,
        field: String,
        expected: Box<FieldType>,
        actual: &'static str,
    },
    MissingRequiredField {
        object: String,
        field: String,
    },
    ReadOnlyField {
        object: String,
        field: String,
    },
    InvalidSavepoint(String),
}

impl SqliteError {
    fn database(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }

    fn invalid_id(error: super::RecordIdError) -> Self {
        Self::InvalidId(error)
    }
}

impl fmt::Display for SqliteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "SQLite storage error: {error}"),
            Self::Schema(error) => error.fmt(formatter),
            Self::InvalidId(error) => error.fmt(formatter),
            Self::IncompatibleMigration {
                object,
                field,
                existing,
                requested,
            } => write!(
                formatter,
                "cannot migrate `{object}.{field}` from `{existing}` to `{requested}`"
            ),
            Self::InvalidFieldValue {
                object,
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "field `{object}.{field}` expects {expected:?}, found {actual}"
            ),
            Self::MissingRequiredField { object, field } => {
                write!(formatter, "required field `{object}.{field}` is missing")
            }
            Self::ReadOnlyField { object, field } => {
                write!(formatter, "field `{object}.{field}` is read-only")
            }
            Self::InvalidSavepoint(name) => {
                write!(formatter, "invalid storage savepoint name `{name}`")
            }
        }
    }
}

impl Error for SqliteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::Schema(error) => Some(error),
            Self::InvalidId(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SchemaError> for SqliteError {
    fn from(value: SchemaError) -> Self {
        Self::Schema(value)
    }
}

fn configure(connection: &Connection) -> Result<(), SqliteError> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )
        .map_err(SqliteError::database)
}

fn migrate(connection: &mut Connection, schema: &SchemaCatalog) -> Result<(), SqliteError> {
    let transaction = connection.transaction().map_err(SqliteError::database)?;
    create_schema_registry(&transaction)?;
    ensure_field_registry_columns(&transaction)?;
    for object in schema.objects() {
        migrate_object(&transaction, object)?;
    }
    transaction.commit().map_err(SqliteError::database)
}

fn create_schema_registry(transaction: &Transaction<'_>) -> Result<(), SqliteError> {
    transaction
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS _apex_objects (
                api_name TEXT PRIMARY KEY COLLATE NOCASE,
                key_prefix TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS _apex_fields (
                object_name TEXT NOT NULL COLLATE NOCASE,
                api_name TEXT NOT NULL COLLATE NOCASE,
                field_type TEXT NOT NULL,
                nullable INTEGER NOT NULL,
                target_object TEXT,
                relationship_name TEXT,
                controlling_field TEXT,
                computed_definition TEXT,
                PRIMARY KEY (object_name, api_name)
             );",
        )
        .map_err(SqliteError::database)
}

fn ensure_field_registry_columns(transaction: &Transaction<'_>) -> Result<(), SqliteError> {
    let mut statement = transaction
        .prepare("PRAGMA table_info('_apex_fields')")
        .map_err(SqliteError::database)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(SqliteError::database)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(SqliteError::database)?;
    for column in [
        "relationship_name",
        "controlling_field",
        "computed_definition",
    ] {
        if !columns.iter().any(|name| name.eq_ignore_ascii_case(column)) {
            transaction
                .execute(
                    &format!("ALTER TABLE _apex_fields ADD COLUMN {column} TEXT"),
                    [],
                )
                .map_err(SqliteError::database)?;
        }
    }
    Ok(())
}

fn migrate_object(transaction: &Transaction<'_>, object: &ObjectSchema) -> Result<(), SqliteError> {
    validate_or_insert_object_registry(transaction, object)?;
    transaction
        .execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {} ({} TEXT PRIMARY KEY NOT NULL)",
                quote(object.api_name()),
                quote("Id")
            ),
            [],
        )
        .map_err(SqliteError::database)?;
    for field in object.fields() {
        migrate_field(transaction, object, field)?;
    }
    Ok(())
}

fn validate_or_insert_object_registry(
    transaction: &Transaction<'_>,
    object: &ObjectSchema,
) -> Result<(), SqliteError> {
    let existing = transaction
        .query_row(
            "SELECT key_prefix FROM _apex_objects WHERE api_name = ?1",
            [object.api_name()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(SqliteError::database)?;
    if let Some(existing) = existing {
        if existing != object.key_prefix() {
            return Err(SqliteError::IncompatibleMigration {
                object: object.api_name().to_owned(),
                field: "Id".to_owned(),
                existing,
                requested: object.key_prefix().to_owned(),
            });
        }
        return Ok(());
    }
    transaction
        .execute(
            "INSERT INTO _apex_objects (api_name, key_prefix) VALUES (?1, ?2)",
            params![object.api_name(), object.key_prefix()],
        )
        .map_err(SqliteError::database)?;
    Ok(())
}

fn migrate_field(
    transaction: &Transaction<'_>,
    object: &ObjectSchema,
    field: &FieldSchema,
) -> Result<(), SqliteError> {
    let spec = field_spec(field);
    let existing = transaction
        .query_row(
            "SELECT field_type, nullable, COALESCE(target_object, ''),
                    COALESCE(relationship_name, ''),
                    COALESCE(controlling_field, ''),
                    COALESCE(computed_definition, '')
             FROM _apex_fields WHERE object_name = ?1 AND api_name = ?2",
            params![object.api_name(), field.api_name()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, bool>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .map_err(SqliteError::database)?;
    if let Some(existing) = existing {
        if existing != spec {
            return Err(SqliteError::IncompatibleMigration {
                object: object.api_name().to_owned(),
                field: field.api_name().to_owned(),
                existing: format_field_spec(&existing),
                requested: format_field_spec(&spec),
            });
        }
        return Ok(());
    }
    if is_stored_field(field) {
        transaction
            .execute(
                &format!(
                    "ALTER TABLE {} ADD COLUMN {} {}",
                    quote(object.api_name()),
                    quote(field.api_name()),
                    sqlite_type(field.data_type())
                ),
                [],
            )
            .map_err(SqliteError::database)?;
    }
    transaction
        .execute(
            "INSERT INTO _apex_fields
             (object_name, api_name, field_type, nullable, target_object,
              relationship_name, controlling_field, computed_definition)
             VALUES (?1, ?2, ?3, ?4, NULLIF(?5, ''), NULLIF(?6, ''),
                     NULLIF(?7, ''), NULLIF(?8, ''))",
            params![
                object.api_name(),
                field.api_name(),
                spec.0,
                spec.1,
                spec.2,
                spec.3,
                spec.4,
                spec.5
            ],
        )
        .map_err(SqliteError::database)?;
    Ok(())
}

fn validate_data_value(
    object: &str,
    field: &FieldSchema,
    value: &DataValue,
) -> Result<(), SqliteError> {
    if data_value_compatible(field.data_type(), value) {
        Ok(())
    } else {
        Err(SqliteError::InvalidFieldValue {
            object: object.to_owned(),
            field: field.api_name().to_owned(),
            expected: Box::new(field.data_type().clone()),
            actual: value_name(value),
        })
    }
}

fn data_value_compatible(expected: &FieldType, value: &DataValue) -> bool {
    match (expected, value) {
        (_, DataValue::Null)
        | (FieldType::Boolean, DataValue::Boolean(_))
        | (FieldType::Integer, DataValue::Integer(_))
        | (FieldType::String, DataValue::String(_))
        | (FieldType::Id, DataValue::Id(_) | DataValue::String(_))
        | (FieldType::Reference { .. }, DataValue::Id(_) | DataValue::String(_))
        | (FieldType::MetadataRelationship { .. }, DataValue::String(_)) => true,
        (FieldType::Summary { result_type, .. }, value) => {
            data_value_compatible(result_type, value)
        }
        (FieldType::Date, DataValue::Date(value)) => NaiveDate::from_ymd_opt(1970, 1, 1)
            .and_then(|epoch| epoch.checked_add_signed(Duration::days(i64::from(*value))))
            .is_some(),
        (FieldType::Datetime, DataValue::Datetime(value)) => {
            Utc.timestamp_millis_opt(*value).single().is_some()
        }
        _ => false,
    }
}

fn encode_value(value: &DataValue) -> SqlValue {
    match value {
        DataValue::Null => SqlValue::Null,
        DataValue::Boolean(value) => SqlValue::Integer(i64::from(*value)),
        DataValue::Integer(value) => SqlValue::Integer(*value),
        DataValue::String(value) => SqlValue::Text(value.clone()),
        DataValue::Date(value) => SqlValue::Integer(i64::from(*value)),
        DataValue::Datetime(value) => SqlValue::Integer(*value),
        DataValue::Id(value) => SqlValue::Text(value.to_string()),
    }
}

fn decode_value(field: &FieldSchema, value: ValueRef<'_>) -> rusqlite::Result<DataValue> {
    decode_field_type(field.api_name(), field.data_type(), value)
}

fn decode_field_type(
    field_name: &str,
    field_type: &FieldType,
    value: ValueRef<'_>,
) -> rusqlite::Result<DataValue> {
    match (field_type, value) {
        (_, ValueRef::Null) => Ok(DataValue::Null),
        (FieldType::Boolean, ValueRef::Integer(value)) => Ok(DataValue::Boolean(value != 0)),
        (FieldType::Integer, ValueRef::Integer(value)) => Ok(DataValue::Integer(value)),
        (FieldType::String, ValueRef::Text(value)) => Ok(DataValue::String(
            String::from_utf8_lossy(value).into_owned(),
        )),
        (FieldType::MetadataRelationship { .. }, ValueRef::Text(value)) => Ok(DataValue::String(
            String::from_utf8_lossy(value).into_owned(),
        )),
        (FieldType::Summary { result_type, .. }, value) => {
            decode_field_type(field_name, result_type, value)
        }
        (FieldType::Date, ValueRef::Integer(value)) => i32::try_from(value)
            .map(DataValue::Date)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, value)),
        (FieldType::Datetime, ValueRef::Integer(value)) => Ok(DataValue::Datetime(value)),
        (FieldType::Id | FieldType::Reference { .. }, ValueRef::Text(value)) => Ok(DataValue::Id(
            RecordId::new(String::from_utf8_lossy(value).into_owned()),
        )),
        _ => Err(rusqlite::Error::InvalidColumnType(
            0,
            field_name.to_owned(),
            value.data_type(),
        )),
    }
}

fn field_spec(field: &FieldSchema) -> (String, bool, String, String, String, String) {
    let relationship_name = field.relationship_name().unwrap_or_default().to_owned();
    let (field_type, nullable, target_object, controlling_field, computed_definition) =
        match field.data_type() {
            FieldType::Boolean => (
                "Boolean".to_owned(),
                field.is_nullable(),
                String::new(),
                String::new(),
                String::new(),
            ),
            FieldType::Integer => (
                "Integer".to_owned(),
                field.is_nullable(),
                String::new(),
                String::new(),
                String::new(),
            ),
            FieldType::String => (
                "String".to_owned(),
                field.is_nullable(),
                String::new(),
                String::new(),
                String::new(),
            ),
            FieldType::Date => (
                "Date".to_owned(),
                field.is_nullable(),
                String::new(),
                String::new(),
                String::new(),
            ),
            FieldType::Datetime => (
                "Datetime".to_owned(),
                field.is_nullable(),
                String::new(),
                String::new(),
                String::new(),
            ),
            FieldType::Id => (
                "Id".to_owned(),
                field.is_nullable(),
                String::new(),
                String::new(),
                String::new(),
            ),
            FieldType::Reference { target_object } => (
                "Reference".to_owned(),
                field.is_nullable(),
                target_object.clone(),
                String::new(),
                String::new(),
            ),
            FieldType::MetadataRelationship {
                target_metadata,
                controlling_field,
            } => (
                "MetadataRelationship".to_owned(),
                field.is_nullable(),
                target_metadata.clone(),
                controlling_field.clone().unwrap_or_default(),
                String::new(),
            ),
            FieldType::Summary {
                result_type,
                definition,
            } => (
                "Summary".to_owned(),
                field.is_nullable(),
                definition.child_object.clone(),
                String::new(),
                summary_definition_spec(result_type, definition),
            ),
        };
    (
        field_type,
        nullable,
        target_object,
        relationship_name,
        controlling_field,
        computed_definition,
    )
}

fn summary_definition_spec(result_type: &FieldType, definition: &SummaryDefinition) -> String {
    let filters = definition
        .filters
        .iter()
        .map(|filter| {
            serde_json::json!({
                "field": filter.field,
                "operator": match filter.operator {
                    SummaryFilterOperator::Equal => "Equal",
                },
                "value": filter.value,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "resultType": field_type_registry_name(result_type),
        "operation": match definition.operation {
            SummaryOperation::Count => "Count",
            SummaryOperation::Sum => "Sum",
            SummaryOperation::Min => "Min",
            SummaryOperation::Max => "Max",
        },
        "foreignKeyField": definition.foreign_key_field,
        "summarizedField": definition.summarized_field,
        "filters": filters,
    })
    .to_string()
}

fn field_type_registry_name(field_type: &FieldType) -> &'static str {
    match field_type {
        FieldType::Boolean => "Boolean",
        FieldType::Integer => "Integer",
        FieldType::String => "String",
        FieldType::Date => "Date",
        FieldType::Datetime => "Datetime",
        FieldType::Id => "Id",
        FieldType::Reference { .. } => "Reference",
        FieldType::MetadataRelationship { .. } => "MetadataRelationship",
        FieldType::Summary { .. } => "Summary",
    }
}

fn format_field_spec(spec: &(String, bool, String, String, String, String)) -> String {
    if spec.2.is_empty() && spec.3.is_empty() && spec.4.is_empty() && spec.5.is_empty() {
        format!("{} nullable={}", spec.0, spec.1)
    } else {
        format!(
            "{}({}) relationship={} controlling={} definition={} nullable={}",
            spec.0, spec.2, spec.3, spec.4, spec.5, spec.1
        )
    }
}

fn sqlite_type(field_type: &FieldType) -> &'static str {
    match field_type {
        FieldType::Boolean | FieldType::Integer | FieldType::Date | FieldType::Datetime => {
            "INTEGER"
        }
        FieldType::String
        | FieldType::Id
        | FieldType::Reference { .. }
        | FieldType::MetadataRelationship { .. } => "TEXT",
        FieldType::Summary { result_type, .. } => sqlite_type(result_type),
    }
}

fn is_id(field: &FieldSchema) -> bool {
    field.api_name().eq_ignore_ascii_case("Id")
}

fn is_stored_field(field: &FieldSchema) -> bool {
    !is_id(field) && !matches!(field.data_type(), FieldType::Summary { .. })
}

fn value_name(value: &DataValue) -> &'static str {
    match value {
        DataValue::Null => "null",
        DataValue::Boolean(_) => "Boolean",
        DataValue::Integer(_) => "Integer",
        DataValue::String(_) => "String",
        DataValue::Date(_) => "Date",
        DataValue::Datetime(_) => "Datetime",
        DataValue::Id(_) => "Id",
    }
}

fn validate_savepoint(name: &str) -> Result<(), SqliteError> {
    let mut bytes = name.bytes();
    if !bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(SqliteError::InvalidSavepoint(name.to_owned()));
    }
    Ok(())
}

fn quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{FieldSchema, ObjectSchema, SummaryFilter};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn schema(include_note: bool) -> SchemaCatalog {
        let mut account = ObjectSchema::with_key_prefix("Account", "001").unwrap();
        account
            .insert_field(FieldSchema::new("Id", FieldType::Id, false))
            .unwrap();
        account
            .insert_field(FieldSchema::new("Name", FieldType::String, false))
            .unwrap();
        account
            .insert_field(FieldSchema::new("Active__c", FieldType::Boolean, true))
            .unwrap();
        if include_note {
            account
                .insert_field(FieldSchema::new("Note__c", FieldType::String, true))
                .unwrap();
        }
        SchemaCatalog::from_objects([account]).unwrap()
    }

    fn account(sequence: u64, name: &str) -> Record {
        let mut record = Record::new("Account", RecordId::generate("001", sequence).unwrap());
        record.set_field("Name", name);
        record.set_field("Active__c", true);
        record
    }

    fn metadata_relationship_schema(controlling_field: &str) -> SchemaCatalog {
        let mut mapping = ObjectSchema::new("Mapping__mdt");
        mapping
            .insert_field(FieldSchema::new("Id", FieldType::Id, false))
            .unwrap();
        mapping
            .insert_field(
                FieldSchema::new(
                    "TargetField__c",
                    FieldType::MetadataRelationship {
                        target_metadata: "FieldDefinition".to_owned(),
                        controlling_field: Some(controlling_field.to_owned()),
                    },
                    false,
                )
                .with_relationship_name("TargetField"),
            )
            .unwrap();
        SchemaCatalog::from_objects([mapping]).unwrap()
    }

    fn summary_schema(filter_value: &str) -> SchemaCatalog {
        let mut invoice = ObjectSchema::with_key_prefix("Invoice__c", "a10").unwrap();
        invoice
            .insert_field(FieldSchema::new("Id", FieldType::Id, false))
            .unwrap();
        invoice
            .insert_field(FieldSchema::new(
                "PaidLines__c",
                FieldType::Summary {
                    result_type: Box::new(FieldType::Integer),
                    definition: SummaryDefinition {
                        child_object: "InvoiceLine__c".to_owned(),
                        foreign_key_field: "Invoice__c".to_owned(),
                        operation: SummaryOperation::Count,
                        summarized_field: None,
                        filters: vec![SummaryFilter {
                            field: "Paid__c".to_owned(),
                            operator: SummaryFilterOperator::Equal,
                            value: filter_value.to_owned(),
                        }],
                    },
                },
                false,
            ))
            .unwrap();
        let mut line = ObjectSchema::with_key_prefix("InvoiceLine__c", "a11").unwrap();
        line.insert_field(FieldSchema::new("Id", FieldType::Id, false))
            .unwrap();
        line.insert_field(FieldSchema::new(
            "Invoice__c",
            FieldType::Reference {
                target_object: "Invoice__c".to_owned(),
            },
            false,
        ))
        .unwrap();
        line.insert_field(FieldSchema::new("Paid__c", FieldType::Boolean, false))
            .unwrap();
        SchemaCatalog::from_objects([invoice, line]).unwrap()
    }

    #[test]
    fn migrates_normalized_schema_and_round_trips_crud() {
        let mut storage = SqliteStorage::in_memory(schema(false)).unwrap();
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM _apex_fields WHERE object_name = 'Account'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            3
        );
        let original = account(1, "Acme");
        let id = original.id().clone();
        let mut transaction = storage.begin_transaction().unwrap();
        transaction.write(original).unwrap();
        assert_eq!(
            transaction
                .read("account", &id)
                .unwrap()
                .unwrap()
                .field("NAME"),
            Some(&DataValue::String("Acme".to_owned()))
        );
        let mut updated = account(1, "Updated");
        updated.set_field("Active__c", false);
        transaction.write(updated).unwrap();
        assert_eq!(
            transaction
                .read("ACCOUNT", &id)
                .unwrap()
                .unwrap()
                .field("active__c"),
            Some(&DataValue::Boolean(false))
        );
        assert!(transaction.delete("Account", &id).unwrap());
        assert!(transaction.read("Account", &id).unwrap().is_none());
        transaction.commit().unwrap();
    }

    #[test]
    fn transactions_savepoints_fixtures_and_reset_are_isolated() {
        let mut storage = SqliteStorage::in_memory(schema(false)).unwrap();
        let first = account(1, "First");
        let first_id = first.id().clone();
        let second = account(2, "Second");
        let second_id = second.id().clone();
        storage.load_fixture([first]).unwrap();

        let invalid = Record::new("Account", RecordId::generate("001", 3).unwrap());
        assert!(matches!(
            storage.load_fixture([invalid]).unwrap_err(),
            SqliteError::MissingRequiredField { .. }
        ));
        let mut preserved = storage.begin_transaction().unwrap();
        assert!(preserved.read("Account", &first_id).unwrap().is_some());
        preserved.commit().unwrap();

        let mut transaction = storage.begin_transaction().unwrap();
        transaction.savepoint("test_setup").unwrap();
        transaction.write(second).unwrap();
        assert!(transaction.read("Account", &second_id).unwrap().is_some());
        transaction.rollback_to("test_setup").unwrap();
        transaction.release_savepoint("test_setup").unwrap();
        assert!(transaction.read("Account", &second_id).unwrap().is_none());
        transaction.rollback().unwrap();

        let mut verify = storage.begin_transaction().unwrap();
        assert!(verify.read("Account", &first_id).unwrap().is_some());
        verify.commit().unwrap();
        storage.reset().unwrap();
        let mut empty = storage.begin_transaction().unwrap();
        assert!(empty.read("Account", &first_id).unwrap().is_none());
        empty.commit().unwrap();
    }

    #[test]
    fn additive_migration_preserves_data_and_rejects_type_changes() {
        let mut storage = SqliteStorage::in_memory(schema(false)).unwrap();
        let original = account(1, "Acme");
        let id = original.id().clone();
        storage.load_fixture([original]).unwrap();
        storage.migrate(schema(true)).unwrap();

        let mut verify = storage.begin_transaction().unwrap();
        let record = verify.read("Account", &id).unwrap().unwrap();
        assert_eq!(
            record.field("Name"),
            Some(&DataValue::String("Acme".to_owned()))
        );
        assert_eq!(record.field("Note__c"), Some(&DataValue::Null));
        verify.commit().unwrap();

        let mut incompatible = ObjectSchema::with_key_prefix("Account", "001").unwrap();
        incompatible
            .insert_field(FieldSchema::new("Id", FieldType::Id, false))
            .unwrap();
        incompatible
            .insert_field(FieldSchema::new("Name", FieldType::Integer, false))
            .unwrap();
        incompatible
            .insert_field(FieldSchema::new("Active__c", FieldType::Boolean, true))
            .unwrap();
        let error = storage
            .migrate(SchemaCatalog::from_objects([incompatible]).unwrap())
            .unwrap_err();
        assert!(error.to_string().contains("cannot migrate `Account.Name`"));
    }

    #[test]
    fn metadata_relationship_registry_and_values_round_trip_without_coercion() {
        let mut storage =
            SqliteStorage::in_memory(metadata_relationship_schema("Mapping__mdt.TargetType__c"))
                .unwrap();
        let registry = storage
            .connection()
            .query_row(
                "SELECT field_type, target_object, relationship_name, controlling_field
                 FROM _apex_fields
                 WHERE object_name = 'Mapping__mdt' AND api_name = 'TargetField__c'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            registry,
            (
                "MetadataRelationship".to_owned(),
                "FieldDefinition".to_owned(),
                "TargetField".to_owned(),
                "Mapping__mdt.TargetType__c".to_owned(),
            )
        );

        let prefix = storage
            .schema()
            .object("Mapping__mdt")
            .unwrap()
            .key_prefix()
            .to_owned();
        let id = RecordId::generate(&prefix, 1).unwrap();
        let mut record = Record::new("Mapping__mdt", id.clone());
        record.set_field("TargetField__c", "Account.Name");
        let mut transaction = storage.begin_transaction().unwrap();
        transaction.write(record).unwrap();
        assert_eq!(
            transaction
                .read("Mapping__mdt", &id)
                .unwrap()
                .unwrap()
                .field("TargetField__c"),
            Some(&DataValue::String("Account.Name".to_owned()))
        );
        transaction.commit().unwrap();

        let error = storage
            .migrate(metadata_relationship_schema(
                "Mapping__mdt.DifferentType__c",
            ))
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cannot migrate `Mapping__mdt.TargetField__c`")
        );
    }

    #[test]
    fn summary_registry_is_lossless_non_physical_and_read_only() {
        let mut storage = SqliteStorage::in_memory(summary_schema("true")).unwrap();
        let (field_type, target_object, definition) = storage
            .connection()
            .query_row(
                "SELECT field_type, target_object, computed_definition
                 FROM _apex_fields
                 WHERE object_name = 'Invoice__c' AND api_name = 'PaidLines__c'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(field_type, "Summary");
        assert_eq!(target_object, "InvoiceLine__c");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&definition).unwrap(),
            serde_json::json!({
                "resultType": "Integer",
                "operation": "Count",
                "foreignKeyField": "Invoice__c",
                "summarizedField": null,
                "filters": [{
                    "field": "Paid__c",
                    "operator": "Equal",
                    "value": "true"
                }]
            })
        );
        let physical_columns = storage
            .connection()
            .prepare("PRAGMA table_info(\"Invoice__c\")")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(physical_columns, ["Id"]);

        let id = RecordId::generate("a10", 1).unwrap();
        let mut invalid = Record::new("Invoice__c", id);
        invalid.set_field("PaidLines__c", 2_i64);
        let mut transaction = storage.begin_transaction().unwrap();
        assert!(matches!(
            transaction.write(invalid).unwrap_err(),
            SqliteError::ReadOnlyField { object, field }
                if object == "Invoice__c" && field == "PaidLines__c"
        ));
        transaction.rollback().unwrap();

        let error = storage.migrate(summary_schema("false")).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cannot migrate `Invoice__c.PaidLines__c`")
        );
    }

    #[test]
    fn invalid_records_and_savepoints_fail_explicitly() {
        let mut storage = SqliteStorage::in_memory(schema(false)).unwrap();
        let mut transaction = storage.begin_transaction().unwrap();
        let missing_name = Record::new("Account", RecordId::generate("001", 1).unwrap());
        assert!(matches!(
            transaction.write(missing_name).unwrap_err(),
            SqliteError::MissingRequiredField { .. }
        ));
        let mut wrong_type = Record::new("Account", RecordId::generate("001", 2).unwrap());
        wrong_type.set_field("Name", 42_i64);
        let invalid_field_value = transaction.write(wrong_type).unwrap_err();
        match &invalid_field_value {
            SqliteError::InvalidFieldValue {
                object,
                field,
                expected,
                actual,
            } => {
                assert_eq!(object, "Account");
                assert_eq!(field, "Name");
                assert_eq!(**expected, FieldType::String);
                assert_eq!(actual, &"Integer");
            }
            error => panic!("expected invalid field value error, got {error:?}"),
        }
        assert_eq!(
            invalid_field_value.to_string(),
            "field `Account.Name` expects String, found Integer"
        );
        assert!(matches!(
            transaction.savepoint("bad-name").unwrap_err(),
            SqliteError::InvalidSavepoint(_)
        ));
        transaction.rollback().unwrap();
    }

    #[test]
    fn file_backed_storage_persists_records_across_reopen() {
        let path = std::env::temp_dir().join(format!(
            "apex-exec-sqlite-{}-{}.db",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let id = RecordId::generate("001", 9).unwrap();
        {
            let mut storage = SqliteStorage::open(&path, schema(false)).unwrap();
            let mut transaction = storage.begin_transaction().unwrap();
            transaction.write(account(9, "Persistent")).unwrap();
            transaction.commit().unwrap();
        }
        {
            let mut storage = SqliteStorage::open(&path, schema(false)).unwrap();
            let mut transaction = storage.begin_transaction().unwrap();
            assert_eq!(
                transaction
                    .read("Account", &id)
                    .unwrap()
                    .unwrap()
                    .field("Name"),
                Some(&DataValue::String("Persistent".to_owned()))
            );
            transaction.commit().unwrap();
        }
        std::fs::remove_file(path).unwrap();
    }
}
