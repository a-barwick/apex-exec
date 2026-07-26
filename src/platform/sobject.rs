use super::{DataValue, FieldType, Record, RecordId, SchemaCatalog, SchemaError, SchemaProvider};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use std::{collections::BTreeMap, error::Error, fmt};

/// Schema-validated SObject value independent from the Apex interpreter and
/// database adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SObject {
    object_api_name: String,
    id: Option<RecordId>,
    fields: BTreeMap<String, DataValue>,
}

impl SObject {
    pub fn new(schema: &impl SchemaProvider, object_api_name: &str) -> Result<Self, SObjectError> {
        let object = schema.object(object_api_name)?;
        Ok(Self {
            object_api_name: object.api_name().to_owned(),
            id: None,
            fields: BTreeMap::new(),
        })
    }

    /// Construct a dynamic SObject using a runtime API name.
    pub fn dynamic(
        schema: &impl SchemaProvider,
        object_api_name: impl AsRef<str>,
    ) -> Result<Self, SObjectError> {
        Self::new(schema, object_api_name.as_ref())
    }

    pub fn object_api_name(&self) -> &str {
        &self.object_api_name
    }

    pub fn id(&self) -> Option<&RecordId> {
        self.id.as_ref()
    }

    pub fn fields(&self) -> impl ExactSizeIterator<Item = (&str, &DataValue)> {
        self.fields
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }

    pub fn set_id(&mut self, id: RecordId) {
        self.fields
            .insert("id".to_owned(), DataValue::Id(id.clone()));
        self.id = Some(id);
    }

    pub fn field(
        &self,
        schema: &impl SchemaProvider,
        field_api_name: &str,
    ) -> Result<&DataValue, SObjectError> {
        let field = schema.field(&self.object_api_name, field_api_name)?;
        self.fields
            .get(&canonical_name(field.api_name()))
            .ok_or_else(|| SObjectError::UnsetField {
                object: self.object_api_name.clone(),
                field: field.api_name().to_owned(),
            })
    }

    pub fn get(
        &self,
        schema: &impl SchemaProvider,
        field_api_name: &str,
    ) -> Result<Option<&DataValue>, SObjectError> {
        let field = schema.field(&self.object_api_name, field_api_name)?;
        Ok(self.fields.get(&canonical_name(field.api_name())))
    }

    pub fn set(
        &mut self,
        schema: &impl SchemaProvider,
        field_api_name: &str,
        value: DataValue,
    ) -> Result<Option<DataValue>, SObjectError> {
        let field = schema.field(&self.object_api_name, field_api_name)?;
        if matches!(field.data_type(), FieldType::Summary { .. }) {
            return Err(SObjectError::ReadOnlyField {
                object: self.object_api_name.clone(),
                field: field.api_name().to_owned(),
            });
        }
        validate_value(
            &self.object_api_name,
            field.api_name(),
            field.data_type(),
            &value,
        )?;
        if field.api_name().eq_ignore_ascii_case("Id") {
            match &value {
                DataValue::Id(id) => self.id = Some(id.clone()),
                DataValue::String(id) => self.id = Some(RecordId::parse(id.clone())?),
                DataValue::Null => self.id = None,
                _ => unreachable!("ID validation accepted an incompatible value"),
            }
        }
        Ok(self.fields.insert(canonical_name(field.api_name()), value))
    }

    /// Dynamic Apex-style field mutation.
    pub fn put(
        &mut self,
        schema: &impl SchemaProvider,
        field_api_name: impl AsRef<str>,
        value: DataValue,
    ) -> Result<Option<DataValue>, SObjectError> {
        self.set(schema, field_api_name.as_ref(), value)
    }

    pub fn into_record(self) -> Result<Record, SObjectError> {
        let id = self.id.ok_or_else(|| SObjectError::MissingId {
            object: self.object_api_name.clone(),
        })?;
        let mut record = Record::new(self.object_api_name, id);
        for (name, value) in self.fields {
            if name != "id" {
                record.set_field(name, value);
            }
        }
        Ok(record)
    }

    pub fn from_record(schema: &SchemaCatalog, record: Record) -> Result<Self, SObjectError> {
        let mut value = Self::new(schema, record.object_api_name())?;
        value.set_id(record.id().clone());
        for (name, field_value) in record.fields() {
            let field = schema.field(record.object_api_name(), name)?;
            if let FieldType::Summary { result_type, .. } = field.data_type() {
                validate_value(
                    record.object_api_name(),
                    field.api_name(),
                    result_type,
                    field_value,
                )?;
                value
                    .fields
                    .insert(canonical_name(field.api_name()), field_value.clone());
            } else {
                value.set(schema, name, field_value.clone())?;
            }
        }
        Ok(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SObjectError {
    Schema(SchemaError),
    InvalidFieldValue {
        object: String,
        field: String,
        expected: Box<FieldType>,
        actual: &'static str,
    },
    ReadOnlyField {
        object: String,
        field: String,
    },
    UnsetField {
        object: String,
        field: String,
    },
    MissingId {
        object: String,
    },
    InvalidId(super::RecordIdError),
}

impl fmt::Display for SObjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(error) => error.fmt(formatter),
            Self::InvalidFieldValue {
                object,
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "field `{object}.{field}` expects {expected:?}, found {actual}"
            ),
            Self::UnsetField { object, field } => {
                write!(formatter, "field `{object}.{field}` has not been assigned")
            }
            Self::ReadOnlyField { object, field } => {
                write!(formatter, "field `{object}.{field}` is read-only")
            }
            Self::MissingId { object } => {
                write!(
                    formatter,
                    "SObject `{object}` cannot become a stored record without an Id"
                )
            }
            Self::InvalidId(error) => error.fmt(formatter),
        }
    }
}

impl Error for SObjectError {}

impl From<SchemaError> for SObjectError {
    fn from(value: SchemaError) -> Self {
        Self::Schema(value)
    }
}

impl From<super::RecordIdError> for SObjectError {
    fn from(value: super::RecordIdError) -> Self {
        Self::InvalidId(value)
    }
}

fn validate_value(
    object: &str,
    field: &str,
    expected: &FieldType,
    value: &DataValue,
) -> Result<(), SObjectError> {
    let compatible = match (expected, value) {
        (_, DataValue::Null)
        | (FieldType::Boolean, DataValue::Boolean(_))
        | (FieldType::Integer, DataValue::Integer(_))
        | (FieldType::String, DataValue::String(_))
        | (FieldType::Id, DataValue::Id(_) | DataValue::String(_))
        | (FieldType::Reference { .. }, DataValue::Id(_) | DataValue::String(_))
        | (FieldType::MetadataRelationship { .. }, DataValue::String(_)) => true,
        (FieldType::Summary { result_type, .. }, value) => {
            return validate_value(object, field, result_type, value);
        }
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
        Err(SObjectError::InvalidFieldValue {
            object: object.to_owned(),
            field: field.to_owned(),
            expected: Box::new(expected.clone()),
            actual: data_value_name(value),
        })
    }
}

fn data_value_name(value: &DataValue) -> &'static str {
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

fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{FieldSchema, ObjectSchema};

    fn schema() -> SchemaCatalog {
        let mut account = ObjectSchema::with_key_prefix("Account", "001").unwrap();
        account
            .insert_field(FieldSchema::new("Id", FieldType::Id, false))
            .unwrap();
        account
            .insert_field(FieldSchema::new("Name", FieldType::String, false))
            .unwrap();
        account
            .insert_field(FieldSchema::new("Employees", FieldType::Integer, true))
            .unwrap();
        SchemaCatalog::from_objects([account]).unwrap()
    }

    #[test]
    fn typed_and_dynamic_access_share_case_insensitive_schema_validation() {
        let schema = schema();
        let mut account = SObject::new(&schema, "account").unwrap();
        let id = RecordId::generate("001", 1).unwrap();
        account
            .set(&schema, "ID", DataValue::Id(id.clone()))
            .unwrap();
        account
            .put(&schema, "name", DataValue::String("Acme".to_owned()))
            .unwrap();
        account
            .set(&schema, "Employees", DataValue::Integer(42))
            .unwrap();

        assert_eq!(account.object_api_name(), "Account");
        assert_eq!(account.id(), Some(&id));
        assert_eq!(
            account.get(&schema, "NAME").unwrap(),
            Some(&DataValue::String("Acme".to_owned()))
        );
        let invalid_value = account
            .set(&schema, "Employees", DataValue::Boolean(true))
            .unwrap_err();
        assert_eq!(
            invalid_value,
            SObjectError::InvalidFieldValue {
                object: "Account".to_owned(),
                field: "Employees".to_owned(),
                expected: Box::new(FieldType::Integer),
                actual: "Boolean",
            }
        );
        assert_eq!(
            invalid_value.to_string(),
            "field `Account.Employees` expects Integer, found Boolean"
        );
    }

    #[test]
    fn records_round_trip_without_runtime_value_coupling() {
        let schema = schema();
        let id = RecordId::generate("001", 7).unwrap();
        let mut account = SObject::dynamic(&schema, "Account").unwrap();
        account.set_id(id.clone());
        account
            .set(&schema, "Name", DataValue::String("Acme".to_owned()))
            .unwrap();
        let record = account.into_record().unwrap();
        let restored = SObject::from_record(&schema, record).unwrap();

        assert_eq!(restored.id(), Some(&id));
        assert_eq!(
            restored.get(&schema, "Name").unwrap(),
            Some(&DataValue::String("Acme".to_owned()))
        );
    }

    #[test]
    fn unset_fields_missing_ids_and_invalid_id_text_are_explicit() {
        let schema = schema();
        let mut account = SObject::new(&schema, "Account").unwrap();
        assert_eq!(
            account.field(&schema, "Name").unwrap_err(),
            SObjectError::UnsetField {
                object: "Account".to_owned(),
                field: "Name".to_owned(),
            }
        );
        assert_eq!(
            account.clone().into_record().unwrap_err(),
            SObjectError::MissingId {
                object: "Account".to_owned(),
            }
        );
        assert!(matches!(
            account
                .set(&schema, "Id", DataValue::String("invalid".to_owned()))
                .unwrap_err(),
            SObjectError::InvalidId(super::super::RecordIdError::InvalidLength(7))
        ));
        assert!(matches!(
            account.get(&schema, "Missing__c").unwrap_err(),
            SObjectError::Schema(SchemaError::UnknownField { .. })
        ));
    }
}
