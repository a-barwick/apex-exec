use std::{collections::BTreeMap, error::Error, fmt};

use super::DisplayType;

/// Organization-wide default visibility for one SObject.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum SharingModel {
    Private,
    PublicReadOnly,
    #[default]
    PublicReadWrite,
    ControlledByParent,
}

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
    Date,
    Datetime,
    Id,
    Reference {
        target_object: String,
    },
    MetadataRelationship {
        target_metadata: String,
        controlling_field: Option<String>,
    },
    Summary {
        result_type: Box<FieldType>,
        definition: SummaryDefinition,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryOperation {
    Count,
    Sum,
    Min,
    Max,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryFilterOperator {
    Equal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryFilter {
    pub field: String,
    pub operator: SummaryFilterOperator,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryDefinition {
    pub child_object: String,
    pub foreign_key_field: String,
    pub operation: SummaryOperation,
    pub summarized_field: Option<String>,
    pub filters: Vec<SummaryFilter>,
}

/// A normalized SObject field definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSchema {
    api_name: String,
    data_type: FieldType,
    nullable: bool,
    external_id: bool,
    unique: bool,
    relationship_name: Option<String>,
    label: String,
    length: usize,
    inline_help_text: Option<String>,
    display_type: DisplayType,
    picklist_values: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSetSchema {
    api_name: String,
    label: String,
    fields: Vec<String>,
}

impl FieldSetSchema {
    pub fn new(api_name: impl Into<String>, label: impl Into<String>, fields: Vec<String>) -> Self {
        Self {
            api_name: api_name.into(),
            label: label.into(),
            fields,
        }
    }

    pub fn api_name(&self) -> &str {
        &self.api_name
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn fields(&self) -> &[String] {
        &self.fields
    }
}

impl FieldSchema {
    pub fn new(api_name: impl Into<String>, data_type: FieldType, nullable: bool) -> Self {
        let api_name = api_name.into();
        let display_type = display_type_for_field(&data_type);
        let length = default_field_length(&data_type);
        Self {
            label: api_name.clone(),
            api_name,
            data_type,
            nullable,
            external_id: false,
            unique: false,
            relationship_name: None,
            length,
            inline_help_text: None,
            display_type,
            picklist_values: Vec::new(),
        }
    }

    pub fn with_external_id(mut self, unique: bool) -> Self {
        self.external_id = true;
        self.unique = unique;
        self
    }

    pub fn with_relationship_name(mut self, relationship_name: impl Into<String>) -> Self {
        self.relationship_name = Some(relationship_name.into());
        self
    }

    pub fn with_describe(
        mut self,
        label: impl Into<String>,
        length: Option<usize>,
        inline_help_text: Option<String>,
        display_type: DisplayType,
        picklist_values: Vec<String>,
    ) -> Self {
        self.label = label.into();
        if let Some(length) = length {
            self.length = length;
        }
        self.inline_help_text = inline_help_text;
        self.display_type = display_type;
        self.picklist_values = picklist_values;
        self
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

    pub fn is_external_id(&self) -> bool {
        self.external_id
    }

    pub fn is_unique(&self) -> bool {
        self.unique
    }

    pub fn relationship_name(&self) -> Option<&str> {
        self.relationship_name.as_deref()
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn length(&self) -> usize {
        self.length
    }

    pub fn inline_help_text(&self) -> Option<&str> {
        self.inline_help_text.as_deref()
    }

    pub fn display_type(&self) -> DisplayType {
        self.display_type
    }

    pub fn picklist_values(&self) -> &[String] {
        &self.picklist_values
    }
}

fn display_type_for_field(field_type: &FieldType) -> DisplayType {
    match field_type {
        FieldType::Boolean => DisplayType::Boolean,
        FieldType::Integer => DisplayType::Integer,
        FieldType::String => DisplayType::String,
        FieldType::Date => DisplayType::Date,
        FieldType::Datetime => DisplayType::Datetime,
        FieldType::Id => DisplayType::Id,
        FieldType::Reference { .. } | FieldType::MetadataRelationship { .. } => {
            DisplayType::Reference
        }
        FieldType::Summary { result_type, .. } => display_type_for_field(result_type),
    }
}

fn default_field_length(field_type: &FieldType) -> usize {
    match field_type {
        FieldType::String => 255,
        FieldType::Id | FieldType::Reference { .. } | FieldType::MetadataRelationship { .. } => 18,
        FieldType::Summary { result_type, .. } => default_field_length(result_type),
        FieldType::Boolean | FieldType::Integer | FieldType::Date | FieldType::Datetime => 0,
    }
}

/// A normalized SObject definition with case-insensitive field lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectSchema {
    api_name: String,
    key_prefix: String,
    sharing_model: SharingModel,
    fields: BTreeMap<String, FieldSchema>,
    field_sets: BTreeMap<String, FieldSetSchema>,
}

impl ObjectSchema {
    pub fn new(api_name: impl Into<String>) -> Self {
        let api_name = api_name.into();
        let key_prefix = deterministic_key_prefix(&api_name);
        Self {
            api_name,
            key_prefix,
            sharing_model: SharingModel::default(),
            fields: BTreeMap::new(),
            field_sets: BTreeMap::new(),
        }
    }

    pub fn with_key_prefix(
        api_name: impl Into<String>,
        key_prefix: impl Into<String>,
    ) -> Result<Self, SchemaError> {
        let api_name = api_name.into();
        let key_prefix = key_prefix.into();
        validate_key_prefix(&api_name, &key_prefix)?;
        Ok(Self {
            api_name,
            key_prefix,
            sharing_model: SharingModel::default(),
            fields: BTreeMap::new(),
            field_sets: BTreeMap::new(),
        })
    }

    pub fn with_sharing_model(mut self, sharing_model: SharingModel) -> Self {
        self.sharing_model = sharing_model;
        self
    }

    pub fn api_name(&self) -> &str {
        &self.api_name
    }

    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }

    pub fn sharing_model(&self) -> &SharingModel {
        &self.sharing_model
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

    pub fn field_index(&self, api_name: &str) -> Option<usize> {
        let canonical = canonical_name(api_name);
        self.fields.keys().position(|name| name == &canonical)
    }

    pub fn field_at(&self, index: usize) -> Option<&FieldSchema> {
        self.fields.values().nth(index)
    }

    pub fn insert_field_set(&mut self, field_set: FieldSetSchema) -> Result<(), SchemaError> {
        for field in field_set.fields() {
            self.field(field)?;
        }
        let canonical = canonical_name(field_set.api_name());
        if self.field_sets.contains_key(&canonical) {
            return Err(SchemaError::DuplicateFieldSet {
                object: self.api_name.clone(),
                field_set: field_set.api_name().to_owned(),
            });
        }
        self.field_sets.insert(canonical, field_set);
        Ok(())
    }

    pub fn field_sets(&self) -> impl ExactSizeIterator<Item = &FieldSetSchema> {
        self.field_sets.values()
    }

    pub fn field_set(&self, api_name: &str) -> Option<&FieldSetSchema> {
        self.field_sets.get(&canonical_name(api_name))
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
        if let Some(existing) = self
            .objects
            .values()
            .find(|existing| existing.key_prefix() == object.key_prefix())
        {
            return Err(SchemaError::DuplicateKeyPrefix {
                prefix: object.key_prefix().to_owned(),
                first_object: existing.api_name().to_owned(),
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

    pub fn object_index(&self, api_name: &str) -> Option<usize> {
        let canonical = canonical_name(api_name);
        self.objects.keys().position(|name| name == &canonical)
    }

    pub fn object_at(&self, index: usize) -> Option<&ObjectSchema> {
        self.objects.values().nth(index)
    }

    pub fn child_relationship(
        &self,
        parent_object_id: usize,
        relationship_name: &str,
    ) -> Option<(usize, usize)> {
        let parent = self.object_at(parent_object_id)?;
        let mut matched = None;
        for (child_object_id, child) in self.objects.values().enumerate() {
            for (reference_field_id, field) in child.fields().enumerate() {
                let FieldType::Reference { target_object } = field.data_type() else {
                    continue;
                };
                if target_object.eq_ignore_ascii_case(parent.api_name())
                    && field.relationship_name().is_some_and(|name| {
                        name.eq_ignore_ascii_case(relationship_name)
                            || relationship_name
                                .strip_suffix("__r")
                                .or_else(|| relationship_name.strip_suffix("__R"))
                                .is_some_and(|base| name.eq_ignore_ascii_case(base))
                    })
                {
                    if matched.is_some() {
                        return None;
                    }
                    matched = Some((child_object_id, reference_field_id));
                }
            }
        }
        matched
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
    DuplicateObject {
        object: String,
    },
    UnknownObject {
        object: String,
    },
    DuplicateField {
        object: String,
        field: String,
    },
    UnknownField {
        object: String,
        field: String,
    },
    DuplicateFieldSet {
        object: String,
        field_set: String,
    },
    InvalidKeyPrefix {
        object: String,
        prefix: String,
    },
    DuplicateKeyPrefix {
        prefix: String,
        first_object: String,
        object: String,
    },
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
            Self::DuplicateFieldSet { object, field_set } => {
                write!(
                    formatter,
                    "duplicate field set `{field_set}` on SObject `{object}`"
                )
            }
            Self::InvalidKeyPrefix { object, prefix } => write!(
                formatter,
                "SObject `{object}` has invalid key prefix `{prefix}`; expected three ASCII alphanumeric characters"
            ),
            Self::DuplicateKeyPrefix {
                prefix,
                first_object,
                object,
            } => write!(
                formatter,
                "SObject key prefix `{prefix}` is shared by `{first_object}` and `{object}`"
            ),
        }
    }
}

impl Error for SchemaError {}

fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn validate_key_prefix(object: &str, prefix: &str) -> Result<(), SchemaError> {
    if prefix.len() == 3 && prefix.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        Ok(())
    } else {
        Err(SchemaError::InvalidKeyPrefix {
            object: object.to_owned(),
            prefix: prefix.to_owned(),
        })
    }
}

fn deterministic_key_prefix(api_name: &str) -> String {
    const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let hash = api_name.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte.to_ascii_lowercase())).wrapping_mul(0x0100_0193)
    });
    let mut value = usize::try_from(hash).expect("u32 fits usize on supported targets");
    let mut prefix = String::with_capacity(3);
    for _ in 0..3 {
        prefix.push(char::from(ALPHABET[value % ALPHABET.len()]));
        value /= ALPHABET.len();
    }
    prefix
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
        assert_eq!(account.key_prefix().len(), 3);
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

    #[test]
    fn explicit_key_prefixes_are_validated() {
        assert_eq!(
            ObjectSchema::with_key_prefix("Account", "01").unwrap_err(),
            SchemaError::InvalidKeyPrefix {
                object: "Account".to_owned(),
                prefix: "01".to_owned(),
            }
        );
        assert_eq!(
            ObjectSchema::with_key_prefix("Account", "001")
                .unwrap()
                .key_prefix(),
            "001"
        );
    }

    #[test]
    fn duplicate_key_prefixes_are_rejected() {
        let mut catalog = SchemaCatalog::new();
        catalog
            .insert_object(ObjectSchema::with_key_prefix("First__c", "a01").unwrap())
            .unwrap();
        assert_eq!(
            catalog
                .insert_object(ObjectSchema::with_key_prefix("Second__c", "a01").unwrap())
                .unwrap_err(),
            SchemaError::DuplicateKeyPrefix {
                prefix: "a01".to_owned(),
                first_object: "First__c".to_owned(),
                object: "Second__c".to_owned(),
            }
        );
    }
}
