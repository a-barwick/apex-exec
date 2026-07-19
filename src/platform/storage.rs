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

    pub fn parse(value: impl Into<String>) -> Result<Self, RecordIdError> {
        let value = value.into();
        validate_record_id(&value)?;
        Ok(Self(value))
    }

    pub fn generate(key_prefix: &str, sequence: u64) -> Result<Self, RecordIdError> {
        if key_prefix.len() != 3 || !key_prefix.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(RecordIdError::InvalidKeyPrefix(key_prefix.to_owned()));
        }
        const ALPHABET: &[u8; 62] =
            b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let mut value = sequence;
        let mut body = [b'0'; 12];
        for byte in body.iter_mut().rev() {
            *byte = ALPHABET[(value % 62) as usize];
            value /= 62;
        }
        let mut id15 = String::with_capacity(15);
        id15.push_str(key_prefix);
        id15.push_str(std::str::from_utf8(&body).expect("ID alphabet is ASCII"));
        let suffix = checksum_suffix(&id15);
        Ok(Self(format!("{id15}{suffix}")))
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

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecordIdError {
    InvalidLength(usize),
    InvalidCharacter { index: usize, character: char },
    InvalidChecksum { expected: String, actual: String },
    InvalidKeyPrefix(String),
}

impl fmt::Display for RecordIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(
                    formatter,
                    "Salesforce ID must contain 15 or 18 characters, found {length}"
                )
            }
            Self::InvalidCharacter { index, character } => write!(
                formatter,
                "Salesforce ID contains invalid character `{character}` at index {index}"
            ),
            Self::InvalidChecksum { expected, actual } => write!(
                formatter,
                "Salesforce ID checksum is invalid: expected `{expected}`, found `{actual}`"
            ),
            Self::InvalidKeyPrefix(prefix) => write!(
                formatter,
                "Salesforce key prefix `{prefix}` must contain three ASCII alphanumeric characters"
            ),
        }
    }
}

impl Error for RecordIdError {}

/// Values that the first storage boundary can persist without Apex runtime
/// representation details.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataValue {
    Null,
    Boolean(bool),
    Integer(i64),
    String(String),
    Date(i32),
    Datetime(i64),
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

    /// Remove every record while retaining the migrated schema.
    fn reset(&mut self) -> Result<(), Self::Error>;
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

    fn scan(&mut self, object_api_name: &str) -> Result<Vec<Record>, Self::Error>;

    fn write(&mut self, record: Record) -> Result<(), Self::Error>;

    fn delete(&mut self, object_api_name: &str, id: &RecordId) -> Result<bool, Self::Error>;

    fn savepoint(&mut self, name: &str) -> Result<(), Self::Error>;

    fn rollback_to(&mut self, name: &str) -> Result<(), Self::Error>;

    fn release_savepoint(&mut self, name: &str) -> Result<(), Self::Error>;

    fn commit(self) -> Result<(), Self::Error>;

    fn rollback(self) -> Result<(), Self::Error>;
}

fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn validate_record_id(value: &str) -> Result<(), RecordIdError> {
    let length = value.len();
    if length != 15 && length != 18 {
        return Err(RecordIdError::InvalidLength(length));
    }
    for (index, byte) in value.bytes().enumerate() {
        if !byte.is_ascii_alphanumeric() {
            return Err(RecordIdError::InvalidCharacter {
                index,
                character: char::from(byte),
            });
        }
    }
    if length == 18 {
        let expected = checksum_suffix(&value[..15]);
        let actual = &value[15..];
        if expected != actual {
            return Err(RecordIdError::InvalidChecksum {
                expected,
                actual: actual.to_owned(),
            });
        }
    }
    Ok(())
}

fn checksum_suffix(id15: &str) -> String {
    const CHECKSUM: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ012345";
    let bytes = id15.as_bytes();
    let mut suffix = String::with_capacity(3);
    for chunk in 0..3 {
        let mut bits = 0_u8;
        for offset in 0..5 {
            let byte = bytes[chunk * 5 + offset];
            if byte.is_ascii_uppercase() {
                bits |= 1 << offset;
            }
        }
        suffix.push(char::from(CHECKSUM[usize::from(bits)]));
    }
    suffix
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
        savepoints: BTreeMap<String, BTreeMap<(String, RecordId), Record>>,
    }

    impl Storage for MemoryStorage {
        type Error = Infallible;
        type Transaction<'storage> = MemoryTransaction<'storage>;

        fn begin_transaction(&mut self) -> Result<Self::Transaction<'_>, Self::Error> {
            let working = self.records.clone();
            Ok(MemoryTransaction {
                target: &mut self.records,
                working,
                savepoints: BTreeMap::new(),
            })
        }

        fn reset(&mut self) -> Result<(), Self::Error> {
            self.records.clear();
            Ok(())
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

        fn scan(&mut self, object_api_name: &str) -> Result<Vec<Record>, Self::Error> {
            let object = canonical_name(object_api_name);
            Ok(self
                .working
                .iter()
                .filter(|((record_object, _), _)| record_object == &object)
                .map(|(_, record)| record.clone())
                .collect())
        }

        fn delete(&mut self, object_api_name: &str, id: &RecordId) -> Result<bool, Self::Error> {
            Ok(self
                .working
                .remove(&(canonical_name(object_api_name), id.clone()))
                .is_some())
        }

        fn savepoint(&mut self, name: &str) -> Result<(), Self::Error> {
            self.savepoints
                .insert(name.to_owned(), self.working.clone());
            Ok(())
        }

        fn rollback_to(&mut self, name: &str) -> Result<(), Self::Error> {
            if let Some(snapshot) = self.savepoints.get(name) {
                self.working = snapshot.clone();
            }
            Ok(())
        }

        fn release_savepoint(&mut self, name: &str) -> Result<(), Self::Error> {
            self.savepoints.remove(name);
            Ok(())
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

    #[test]
    fn record_ids_validate_and_generate_salesforce_shapes() {
        let generated = RecordId::generate("a01", 42).unwrap();
        assert_eq!(generated.as_str().len(), 18);
        assert!(generated.as_str().starts_with("a01"));
        assert_eq!(RecordId::parse(generated.to_string()).unwrap(), generated);
        assert_eq!(
            RecordId::parse("001000000000001AAB").unwrap_err(),
            RecordIdError::InvalidChecksum {
                expected: "AAA".to_owned(),
                actual: "AAB".to_owned(),
            }
        );
        assert_eq!(
            RecordId::parse("bad").unwrap_err(),
            RecordIdError::InvalidLength(3)
        );
    }

    #[test]
    fn storage_contract_supports_savepoints_and_fast_reset() {
        let mut storage = MemoryStorage::default();
        let first = RecordId::generate("001", 1).unwrap();
        let second = RecordId::generate("001", 2).unwrap();

        let mut transaction = storage.begin_transaction().unwrap();
        transaction
            .write(Record::new("Account", first.clone()))
            .unwrap();
        transaction.savepoint("after_first").unwrap();
        transaction
            .write(Record::new("Account", second.clone()))
            .unwrap();
        transaction.rollback_to("after_first").unwrap();
        transaction.release_savepoint("after_first").unwrap();
        assert!(transaction.read("Account", &first).unwrap().is_some());
        assert!(transaction.read("Account", &second).unwrap().is_none());
        transaction.commit().unwrap();

        storage.reset().unwrap();
        let mut verify = storage.begin_transaction().unwrap();
        assert!(verify.read("Account", &first).unwrap().is_none());
        verify.commit().unwrap();
    }
}
