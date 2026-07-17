use std::{collections::BTreeMap, error::Error, fmt};

/// Storage-independent type of a normalized SObject field.
///
/// Metadata-specific field kinds can map onto this smaller runtime-facing
/// representation. Additional value kinds can be introduced as the supported
/// Apex data surface grows.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldType {
    Boolean,
    Integer,
    String,
    Id,
    Reference { target_object: String },
}

/// A normalized SObject field definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSchema {
    api_name: String,
    data_type: FieldType,
    nullable: bool,
}

impl FieldSchema {
    pub fn new(api_name: impl Into<String>, data_type: FieldType, nullable: bool) -> Self {
        Self {
            api_name: api_name.into(),
            data_type,
            nullable,
        }
    }

    pub fn api_name(&self) -> &str {
        &self.api_name
    }

    pub fn data_type(&self) -> &FieldType {
        &self.data_type
    }

    pub fn is_nullable(&self) -> bool {
        self.nullable
    }
}

/// A normalized SObject definition with case-insensitive field lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectSchema {
    api_name: String,
    fields: BTreeMap<String, FieldSchema>,
}

impl ObjectSchema {
    pub fn new(api_name: impl Into<String>) -> Self {
        Self {
            api_name: api_name.into(),
            fields: BTreeMap::new(),
        }
    }

    pub fn api_name(&self) -> &str {
        &self.api_name
    }

    pub fn insert_field(&mut self, field: FieldSchema) -> Result<(), SchemaError> {
        let canonical = canonical_name(field.api_name());
        if self.fields.contains_key(&canonical) {
            return Err(SchemaError::DuplicateField {
                object: self.api_name.clone(),
                field: field.api_name().to_owned(),
            });
        }
        self.fields.insert(canonical, field);
        Ok(())
    }

    pub fn field(&self, api_name: &str) -> Result<&FieldSchema, SchemaError> {
        self.fields
            .get(&canonical_name(api_name))
            .ok_or_else(|| SchemaError::UnknownField {
                object: self.api_name.clone(),
                field: api_name.to_owned(),
            })
    }

    pub fn fields(&self) -> impl ExactSizeIterator<Item = &FieldSchema> {
        self.fields.values()
    }
}

/// Case-insensitive in-memory catalog of normalized SObject definitions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaCatalog {
    objects: BTreeMap<String, ObjectSchema>,
}

impl SchemaCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_objects(
        objects: impl IntoIterator<Item = ObjectSchema>,
    ) -> Result<Self, SchemaError> {
        let mut catalog = Self::new();
        for object in objects {
            catalog.insert_object(object)?;
        }
        Ok(catalog)
    }

    pub fn insert_object(&mut self, object: ObjectSchema) -> Result<(), SchemaError> {
        let canonical = canonical_name(object.api_name());
        if self.objects.contains_key(&canonical) {
            return Err(SchemaError::DuplicateObject {
                object: object.api_name().to_owned(),
            });
        }
        self.objects.insert(canonical, object);
        Ok(())
    }

    pub fn object(&self, api_name: &str) -> Result<&ObjectSchema, SchemaError> {
        self.objects
            .get(&canonical_name(api_name))
            .ok_or_else(|| SchemaError::UnknownObject {
                object: api_name.to_owned(),
            })
    }

    pub fn field(
        &self,
        object_api_name: &str,
        field_api_name: &str,
    ) -> Result<&FieldSchema, SchemaError> {
        self.object(object_api_name)?.field(field_api_name)
    }

    pub fn objects(&self) -> impl ExactSizeIterator<Item = &ObjectSchema> {
        self.objects.values()
    }
}

/// Read-only schema service used by compiler and runtime-facing platform code.
pub trait SchemaProvider {
    fn object(&self, api_name: &str) -> Result<&ObjectSchema, SchemaError>;

    fn field(
        &self,
        object_api_name: &str,
        field_api_name: &str,
    ) -> Result<&FieldSchema, SchemaError> {
        self.object(object_api_name)?.field(field_api_name)
    }
}

impl SchemaProvider for SchemaCatalog {
    fn object(&self, api_name: &str) -> Result<&ObjectSchema, SchemaError> {
        SchemaCatalog::object(self, api_name)
    }
}

/// Explicit schema construction and lookup failures.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SchemaError {
    DuplicateObject { object: String },
    UnknownObject { object: String },
    DuplicateField { object: String, field: String },
    UnknownField { object: String, field: String },
}

impl fmt::Display for SchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateObject { object } => {
                write!(formatter, "duplicate SObject schema `{object}`")
            }
            Self::UnknownObject { object } => write!(formatter, "unknown SObject `{object}`"),
            Self::DuplicateField { object, field } => {
                write!(formatter, "duplicate field `{field}` on SObject `{object}`")
            }
            Self::UnknownField { object, field } => {
                write!(formatter, "unknown field `{field}` on SObject `{object}`")
            }
        }
    }
}

impl Error for SchemaError {}

fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account_schema() -> ObjectSchema {
        let mut account = ObjectSchema::new("Account");
        account
            .insert_field(FieldSchema::new("Id", FieldType::Id, false))
            .unwrap();
        account
            .insert_field(FieldSchema::new("Name", FieldType::String, false))
            .unwrap();
        account
    }

    #[test]
    fn catalog_and_field_lookup_are_case_insensitive() {
        let catalog = SchemaCatalog::from_objects([account_schema()]).unwrap();

        let account = catalog.object("aCcOuNt").unwrap();
        assert_eq!(account.api_name(), "Account");
        let name = catalog.field("ACCOUNT", "name").unwrap();
        assert_eq!(name.api_name(), "Name");
        assert_eq!(name.data_type(), &FieldType::String);
        assert!(!name.is_nullable());
    }

    #[test]
    fn duplicate_names_are_rejected_case_insensitively() {
        let mut catalog = SchemaCatalog::new();
        catalog.insert_object(account_schema()).unwrap();
        assert_eq!(
            catalog
                .insert_object(ObjectSchema::new("account"))
                .unwrap_err(),
            SchemaError::DuplicateObject {
                object: "account".to_owned(),
            }
        );

        let mut contact = ObjectSchema::new("Contact");
        contact
            .insert_field(FieldSchema::new("Email", FieldType::String, true))
            .unwrap();
        assert_eq!(
            contact
                .insert_field(FieldSchema::new("EMAIL", FieldType::String, true))
                .unwrap_err(),
            SchemaError::DuplicateField {
                object: "Contact".to_owned(),
                field: "EMAIL".to_owned(),
            }
        );
    }

    #[test]
    fn unknown_objects_and_fields_return_typed_errors() {
        let catalog = SchemaCatalog::from_objects([account_schema()]).unwrap();

        assert_eq!(
            catalog.object("Missing__c").unwrap_err(),
            SchemaError::UnknownObject {
                object: "Missing__c".to_owned(),
            }
        );
        assert_eq!(
            catalog.field("account", "Missing__c").unwrap_err(),
            SchemaError::UnknownField {
                object: "Account".to_owned(),
                field: "Missing__c".to_owned(),
            }
        );
    }

    #[test]
    fn catalog_satisfies_the_read_only_provider_boundary() {
        fn resolve_name(provider: &dyn SchemaProvider) -> &FieldSchema {
            provider.field("account", "name").unwrap()
        }

        let catalog = SchemaCatalog::from_objects([account_schema()]).unwrap();
        assert_eq!(resolve_name(&catalog).api_name(), "Name");
    }
}
