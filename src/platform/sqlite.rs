use super::{
    DataValue, FieldSchema, FieldType, Record, RecordId, SchemaCatalog, SchemaError, Storage,
    StorageTransaction,
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
            .filter(|field| !is_id(field))
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
            validate_data_value(object.api_name(), field, value)?;
        }

        let fields = object
            .fields()
            .filter(|field| !is_id(field))
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
            .filter(|field| !is_id(field))
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
        expected: FieldType,
        actual: &'static str,
    },
    MissingRequiredField {
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
                PRIMARY KEY (object_name, api_name)
             );",
        )
        .map_err(SqliteError::database)?;
    let has_relationship_name = {
        let mut statement = transaction
            .prepare("PRAGMA table_info('_apex_fields')")
            .map_err(SqliteError::database)?;
        statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(SqliteError::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(SqliteError::database)?
            .iter()
            .any(|name| name.eq_ignore_ascii_case("relationship_name"))
    };
    if !has_relationship_name {
        transaction
            .execute(
                "ALTER TABLE _apex_fields ADD COLUMN relationship_name TEXT",
                [],
            )
            .map_err(SqliteError::database)?;
    }

    for object in schema.objects() {
        let existing_prefix = transaction
            .query_row(
                "SELECT key_prefix FROM _apex_objects WHERE api_name = ?1",
                [object.api_name()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(SqliteError::database)?;
        if let Some(existing) = existing_prefix {
            if existing != object.key_prefix() {
                return Err(SqliteError::IncompatibleMigration {
                    object: object.api_name().to_owned(),
                    field: "Id".to_owned(),
                    existing,
                    requested: object.key_prefix().to_owned(),
                });
            }
        } else {
            transaction
                .execute(
                    "INSERT INTO _apex_objects (api_name, key_prefix) VALUES (?1, ?2)",
                    params![object.api_name(), object.key_prefix()],
                )
                .map_err(SqliteError::database)?;
        }
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
            let spec = field_spec(field);
            let existing = transaction
                .query_row(
                    "SELECT field_type, nullable, COALESCE(target_object, ''),
                            COALESCE(relationship_name, '')
                     FROM _apex_fields WHERE object_name = ?1 AND api_name = ?2",
                    params![object.api_name(), field.api_name()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, bool>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
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
                continue;
            }
            if !is_id(field) {
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
                     (object_name, api_name, field_type, nullable, target_object, relationship_name)
                     VALUES (?1, ?2, ?3, ?4, NULLIF(?5, ''), NULLIF(?6, ''))",
                    params![
                        object.api_name(),
                        field.api_name(),
                        spec.0,
                        spec.1,
                        spec.2,
                        spec.3
                    ],
                )
                .map_err(SqliteError::database)?;
        }
    }
    transaction.commit().map_err(SqliteError::database)
}

fn validate_data_value(
    object: &str,
    field: &FieldSchema,
    value: &DataValue,
) -> Result<(), SqliteError> {
    let compatible = match (field.data_type(), value) {
        (_, DataValue::Null)
        | (FieldType::Boolean, DataValue::Boolean(_))
        | (FieldType::Integer, DataValue::Integer(_))
        | (FieldType::String, DataValue::String(_))
        | (FieldType::Id, DataValue::Id(_) | DataValue::String(_))
        | (FieldType::Reference { .. }, DataValue::Id(_) | DataValue::String(_)) => true,
        (FieldType::Date, DataValue::Date(value)) => NaiveDate::from_ymd_opt(1970, 1, 1)
            .and_then(|epoch| epoch.checked_add_signed(Duration::days(i64::from(*value))))
            .is_some(),
        (FieldType::Datetime, DataValue::Datetime(value)) => {
            Utc.timestamp_millis_opt(*value).single().is_some()
        }
        _ => false,
    };
    if compatible {
        Ok(())
    } else {
        Err(SqliteError::InvalidFieldValue {
            object: object.to_owned(),
            field: field.api_name().to_owned(),
            expected: field.data_type().clone(),
            actual: value_name(value),
        })
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
    match (field.data_type(), value) {
        (_, ValueRef::Null) => Ok(DataValue::Null),
        (FieldType::Boolean, ValueRef::Integer(value)) => Ok(DataValue::Boolean(value != 0)),
        (FieldType::Integer, ValueRef::Integer(value)) => Ok(DataValue::Integer(value)),
        (FieldType::String, ValueRef::Text(value)) => Ok(DataValue::String(
            String::from_utf8_lossy(value).into_owned(),
        )),
        (FieldType::Date, ValueRef::Integer(value)) => i32::try_from(value)
            .map(DataValue::Date)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, value)),
        (FieldType::Datetime, ValueRef::Integer(value)) => Ok(DataValue::Datetime(value)),
        (FieldType::Id | FieldType::Reference { .. }, ValueRef::Text(value)) => Ok(DataValue::Id(
            RecordId::new(String::from_utf8_lossy(value).into_owned()),
        )),
        _ => Err(rusqlite::Error::InvalidColumnType(
            0,
            field.api_name().to_owned(),
            value.data_type(),
        )),
    }
}

fn field_spec(field: &FieldSchema) -> (String, bool, String, String) {
    let relationship_name = field.relationship_name().unwrap_or_default().to_owned();
    let (field_type, nullable, target_object) = match field.data_type() {
        FieldType::Boolean => ("Boolean".to_owned(), field.is_nullable(), String::new()),
        FieldType::Integer => ("Integer".to_owned(), field.is_nullable(), String::new()),
        FieldType::String => ("String".to_owned(), field.is_nullable(), String::new()),
        FieldType::Date => ("Date".to_owned(), field.is_nullable(), String::new()),
        FieldType::Datetime => ("Datetime".to_owned(), field.is_nullable(), String::new()),
        FieldType::Id => ("Id".to_owned(), field.is_nullable(), String::new()),
        FieldType::Reference { target_object } => (
            "Reference".to_owned(),
            field.is_nullable(),
            target_object.clone(),
        ),
    };
    (field_type, nullable, target_object, relationship_name)
}

fn format_field_spec(spec: &(String, bool, String, String)) -> String {
    if spec.2.is_empty() && spec.3.is_empty() {
        format!("{} nullable={}", spec.0, spec.1)
    } else {
        format!(
            "{}({}) relationship={} nullable={}",
            spec.0, spec.2, spec.3, spec.1
        )
    }
}

fn sqlite_type(field_type: &FieldType) -> &'static str {
    match field_type {
        FieldType::Boolean | FieldType::Integer | FieldType::Date | FieldType::Datetime => {
            "INTEGER"
        }
        FieldType::String | FieldType::Id | FieldType::Reference { .. } => "TEXT",
    }
}

fn is_id(field: &FieldSchema) -> bool {
    field.api_name().eq_ignore_ascii_case("Id")
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
    use crate::platform::{FieldSchema, ObjectSchema};
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
        assert!(matches!(
            transaction.write(wrong_type).unwrap_err(),
            SqliteError::InvalidFieldValue { .. }
        ));
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
