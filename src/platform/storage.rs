use std::{collections::BTreeMap, error::Error, fmt};

/// Opaque, storage-independent identity of one SObject record.
///
/// Salesforce-shaped generation and validation belong to the platform host;
/// storage adapters only need a stable value they can compare and persist.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecordId(String);

impl RecordId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RecordId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<String> for RecordId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for RecordId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Values that the first storage boundary can persist without Apex runtime
/// representation details.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataValue {
    Null,
    Boolean(bool),
    Integer(i64),
    String(String),
    Id(RecordId),
}

impl From<bool> for DataValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<i64> for DataValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<String> for DataValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for DataValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<RecordId> for DataValue {
    fn from(value: RecordId) -> Self {
        Self::Id(value)
    }
}

/// Storage-neutral SObject record.
///
/// Field keys are canonicalized for Apex-compatible case-insensitive access.
/// Object spelling and the opaque record ID are retained for diagnostics and
/// persistence adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    object_api_name: String,
    id: RecordId,
    fields: BTreeMap<String, DataValue>,
}

impl Record {
    pub fn new(object_api_name: impl Into<String>, id: impl Into<RecordId>) -> Self {
        Self {
            object_api_name: object_api_name.into(),
            id: id.into(),
            fields: BTreeMap::new(),
        }
    }

    pub fn object_api_name(&self) -> &str {
        &self.object_api_name
    }

    pub fn id(&self) -> &RecordId {
        &self.id
    }

    pub fn set_field(
        &mut self,
        field_api_name: impl AsRef<str>,
        value: impl Into<DataValue>,
    ) -> Option<DataValue> {
        self.fields
            .insert(canonical_name(field_api_name.as_ref()), value.into())
    }

    pub fn field(&self, field_api_name: &str) -> Option<&DataValue> {
        self.fields.get(&canonical_name(field_api_name))
    }

    pub fn remove_field(&mut self, field_api_name: &str) -> Option<DataValue> {
        self.fields.remove(&canonical_name(field_api_name))
    }

    pub fn fields(&self) -> impl ExactSizeIterator<Item = (&str, &DataValue)> {
        self.fields
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }
}

/// Factory for isolated storage transactions.
///
/// The associated transaction may borrow the adapter, which allows both
/// in-memory and connection-backed implementations without allocation or a
/// SQLite dependency in this boundary. This generic contract intentionally
/// uses static dispatch; a dynamically erased host adapter can be layered over
/// it when runtime configuration needs trait objects.
pub trait Storage {
    type Error: Error + Send + Sync + 'static;
    type Transaction<'storage>: StorageTransaction<Error = Self::Error>
    where
        Self: 'storage;

    fn begin_transaction(&mut self) -> Result<Self::Transaction<'_>, Self::Error>;
}

/// Transactional record operations below Apex DML semantics.
///
/// `write` is deliberately an unconditional persistence operation. Insert
/// versus update validation, triggers, and DML result behavior belong to later
/// platform layers.
pub trait StorageTransaction {
    type Error: Error + Send + Sync + 'static;

    fn read(&mut self, object_api_name: &str, id: &RecordId)
    -> Result<Option<Record>, Self::Error>;

    fn write(&mut self, record: Record) -> Result<(), Self::Error>;

    fn delete(&mut self, object_api_name: &str, id: &RecordId) -> Result<bool, Self::Error>;

    fn commit(self) -> Result<(), Self::Error>;

    fn rollback(self) -> Result<(), Self::Error>;
}

fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    #[test]
    fn records_retain_identity_and_access_fields_case_insensitively() {
        let mut record = Record::new("Account", "001000000000001AAA");
        assert_eq!(record.object_api_name(), "Account");
        assert_eq!(record.id().as_str(), "001000000000001AAA");

        assert_eq!(record.set_field("Name", "Acme"), None);
        assert_eq!(
            record.field("nAmE"),
            Some(&DataValue::String("Acme".into()))
        );
        assert_eq!(
            record.set_field("NAME", "Updated"),
            Some(DataValue::String("Acme".into()))
        );
        assert_eq!(
            record.remove_field("name"),
            Some(DataValue::String("Updated".into()))
        );
    }

    #[derive(Default)]
    struct MemoryStorage {
        records: BTreeMap<(String, RecordId), Record>,
    }

    struct MemoryTransaction<'storage> {
        target: &'storage mut BTreeMap<(String, RecordId), Record>,
        working: BTreeMap<(String, RecordId), Record>,
    }

    impl Storage for MemoryStorage {
        type Error = Infallible;
        type Transaction<'storage> = MemoryTransaction<'storage>;

        fn begin_transaction(&mut self) -> Result<Self::Transaction<'_>, Self::Error> {
            let working = self.records.clone();
            Ok(MemoryTransaction {
                target: &mut self.records,
                working,
            })
        }
    }

    impl StorageTransaction for MemoryTransaction<'_> {
        type Error = Infallible;

        fn read(
            &mut self,
            object_api_name: &str,
            id: &RecordId,
        ) -> Result<Option<Record>, Self::Error> {
            Ok(self
                .working
                .get(&(canonical_name(object_api_name), id.clone()))
                .cloned())
        }

        fn write(&mut self, record: Record) -> Result<(), Self::Error> {
            self.working.insert(
                (
                    canonical_name(record.object_api_name()),
                    record.id().clone(),
                ),
                record,
            );
            Ok(())
        }

        fn delete(&mut self, object_api_name: &str, id: &RecordId) -> Result<bool, Self::Error> {
            Ok(self
                .working
                .remove(&(canonical_name(object_api_name), id.clone()))
                .is_some())
        }

        fn commit(self) -> Result<(), Self::Error> {
            *self.target = self.working;
            Ok(())
        }

        fn rollback(self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn storage_contract_supports_commit_and_rollback() {
        let id = RecordId::new("001000000000001AAA");
        let mut storage = MemoryStorage::default();

        let mut insert = storage.begin_transaction().unwrap();
        let mut account = Record::new("Account", id.clone());
        account.set_field("Name", "Acme");
        insert.write(account).unwrap();
        insert.commit().unwrap();

        let mut rollback = storage.begin_transaction().unwrap();
        assert!(rollback.delete("account", &id).unwrap());
        rollback.rollback().unwrap();

        let mut verify = storage.begin_transaction().unwrap();
        assert_eq!(
            verify.read("ACCOUNT", &id).unwrap().unwrap().field("name"),
            Some(&DataValue::String("Acme".into()))
        );
        verify.commit().unwrap();
    }
}
