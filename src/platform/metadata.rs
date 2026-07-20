use super::{
    DisplayType, FieldSchema, FieldSetSchema, FieldType, ObjectSchema, SchemaCatalog, SchemaError,
    SharingModel, SummaryDefinition, SummaryFilter, SummaryFilterOperator, SummaryOperation,
};
use chrono::{DateTime, NaiveDate};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

/// Import custom-object metadata from one or more SFDX package directories.
pub fn import_metadata(
    source_roots: impl IntoIterator<Item = impl AsRef<Path>>,
) -> Result<SchemaCatalog, MetadataError> {
    let mut object_files = Vec::new();
    let mut field_files = Vec::new();
    let mut field_set_files = Vec::new();
    for root in source_roots {
        collect_metadata_files(
            root.as_ref(),
            &mut object_files,
            &mut field_files,
            &mut field_set_files,
        )?;
    }
    object_files.sort();
    object_files.dedup();
    field_files.sort();
    field_files.dedup();
    field_set_files.sort();
    field_set_files.dedup();

    let mut objects = BTreeMap::<String, ObjectBuilder>::new();
    for path in object_files {
        import_object_file(&path, &mut objects)?;
    }
    for path in field_files {
        import_field_file(&path, &mut objects)?;
    }
    for path in field_set_files {
        import_field_set_file(&path, &mut objects)?;
    }
    insert_metadata_relationship_targets(&mut objects);
    resolve_summaries(&mut objects)?;

    let mut catalog = SchemaCatalog::new();
    for object in super::standard_schema::standard_objects() {
        catalog.insert_object(object)?;
    }
    for builder in objects.into_values() {
        let object = builder.finish()?;
        if catalog.object(object.api_name()).is_err() {
            catalog.insert_object(object)?;
        }
    }
    Ok(catalog)
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MetadataError {
    Io {
        path: PathBuf,
        operation: &'static str,
        message: String,
    },
    Invalid {
        path: PathBuf,
        message: String,
    },
    Schema(SchemaError),
}

impl MetadataError {
    fn invalid(path: &Path, message: impl Into<String>) -> Self {
        Self::Invalid {
            path: path.to_path_buf(),
            message: message.into(),
        }
    }

    fn io(path: &Path, operation: &'static str, error: std::io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            operation,
            message: error.to_string(),
        }
    }
}

impl fmt::Display for MetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                path,
                operation,
                message,
            } => write!(
                formatter,
                "could not {operation} SFDX metadata `{}`: {message}",
                path.display()
            ),
            Self::Invalid { path, message } => {
                write!(
                    formatter,
                    "invalid SFDX metadata `{}`: {message}",
                    path.display()
                )
            }
            Self::Schema(error) => error.fmt(formatter),
        }
    }
}

impl Error for MetadataError {}

impl From<SchemaError> for MetadataError {
    fn from(value: SchemaError) -> Self {
        Self::Schema(value)
    }
}

struct ObjectBuilder {
    api_name: String,
    sharing_model: SharingModel,
    fields: Vec<FieldSchema>,
    field_sets: Vec<FieldSetSchema>,
    summaries: Vec<PendingSummary>,
}

impl ObjectBuilder {
    fn new(api_name: String) -> Self {
        let mut builder = Self {
            api_name,
            sharing_model: SharingModel::default(),
            fields: vec![
                FieldSchema::new("Id", FieldType::Id, false),
                FieldSchema::new("OwnerId", FieldType::Id, true),
                FieldSchema::new("CreatedDate", FieldType::Datetime, true),
                FieldSchema::new("LastModifiedDate", FieldType::Datetime, true),
                FieldSchema::new("IsDeleted", FieldType::Boolean, true),
            ],
            field_sets: Vec::new(),
            summaries: Vec::new(),
        };
        if builder.api_name.ends_with("__mdt") {
            builder.fields.extend([
                FieldSchema::new("DeveloperName", FieldType::String, false),
                FieldSchema::new("MasterLabel", FieldType::String, false),
                FieldSchema::new("NamespacePrefix", FieldType::String, true),
                FieldSchema::new("QualifiedApiName", FieldType::String, false),
            ]);
        }
        if builder.api_name.ends_with("__e") {
            builder
                .fields
                .push(FieldSchema::new("EventUuid", FieldType::String, true));
        }
        builder
    }

    fn push_field(&mut self, field: ParsedField) {
        match field {
            ParsedField::Ready(field) => self.fields.push(field),
            ParsedField::Summary(summary) => self.summaries.push(summary),
        }
    }

    fn finish(self) -> Result<ObjectSchema, SchemaError> {
        debug_assert!(self.summaries.is_empty());
        let mut object = ObjectSchema::new(self.api_name).with_sharing_model(self.sharing_model);
        for field in self.fields {
            object.insert_field(field)?;
        }
        for field_set in self.field_sets {
            object.insert_field_set(field_set)?;
        }
        Ok(object)
    }
}

struct PendingSummary {
    path: PathBuf,
    api_name: String,
    nullable: bool,
    definition: SummaryDefinition,
}

enum ParsedField {
    Ready(FieldSchema),
    Summary(PendingSummary),
}

fn insert_metadata_relationship_targets(objects: &mut BTreeMap<String, ObjectBuilder>) {
    let targets = objects
        .values()
        .flat_map(|object| object.fields.iter())
        .filter_map(|field| match field.data_type() {
            FieldType::MetadataRelationship {
                target_metadata, ..
            } => Some(target_metadata.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    for target in targets {
        let canonical = canonical_name(&target);
        if objects.contains_key(&canonical) {
            continue;
        }
        let fields = match canonical.as_str() {
            "entitydefinition" => &["DeveloperName", "QualifiedApiName"][..],
            "fielddefinition" => &["DeveloperName", "QualifiedApiName"][..],
            _ => continue,
        };
        let mut builder = ObjectBuilder::new(target);
        builder.fields.extend(
            fields
                .iter()
                .map(|name| FieldSchema::new(*name, FieldType::String, false)),
        );
        objects.insert(canonical, builder);
    }
}

fn collect_metadata_files(
    directory: &Path,
    object_files: &mut Vec<PathBuf>,
    field_files: &mut Vec<PathBuf>,
    field_set_files: &mut Vec<PathBuf>,
) -> Result<(), MetadataError> {
    if !directory.exists() {
        return Err(MetadataError::invalid(
            directory,
            "package directory does not exist",
        ));
    }
    let entries =
        fs::read_dir(directory).map_err(|error| MetadataError::io(directory, "scan", error))?;
    for entry in entries {
        let entry = entry.map_err(|error| MetadataError::io(directory, "scan", error))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| MetadataError::io(&path, "inspect", error))?;
        if file_type.is_dir() {
            collect_metadata_files(&path, object_files, field_files, field_set_files)?;
        } else if file_type.is_file() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if name.ends_with(".object-meta.xml") || name.ends_with(".object") {
                object_files.push(path);
            } else if name.ends_with(".field-meta.xml") {
                field_files.push(path);
            } else if name.ends_with(".fieldSet-meta.xml") {
                field_set_files.push(path);
            }
        }
    }
    Ok(())
}

fn import_field_set_file(
    path: &Path,
    objects: &mut BTreeMap<String, ObjectBuilder>,
) -> Result<(), MetadataError> {
    let xml = fs::read_to_string(path).map_err(|error| MetadataError::io(path, "read", error))?;
    let object_name = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            MetadataError::invalid(
                path,
                "field set file must be under objects/<Object>/fieldSets",
            )
        })?;
    let fallback_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".fieldSet-meta.xml"))
        .ok_or_else(|| MetadataError::invalid(path, "invalid field set metadata filename"))?;
    let builder = objects
        .get_mut(&canonical_name(object_name))
        .ok_or_else(|| {
            MetadataError::invalid(
                path,
                format!("field set belongs to unknown custom object `{object_name}`"),
            )
        })?;
    let api_name = tag_text(&xml, "fullName").unwrap_or_else(|| fallback_name.to_owned());
    let label = tag_text(&xml, "label").unwrap_or_else(|| api_name.clone());
    let fields = elements(&xml, "displayedFields")
        .map(|entry| required_text(path, entry, "field"))
        .collect::<Result<Vec<_>, _>>()?;
    builder
        .field_sets
        .push(FieldSetSchema::new(api_name, label, fields));
    Ok(())
}

fn import_object_file(
    path: &Path,
    objects: &mut BTreeMap<String, ObjectBuilder>,
) -> Result<(), MetadataError> {
    let xml = fs::read_to_string(path).map_err(|error| MetadataError::io(path, "read", error))?;
    let api_name = object_name_from_path(path)?;
    let canonical = canonical_name(&api_name);
    if objects.contains_key(&canonical) {
        return Err(MetadataError::invalid(
            path,
            format!("duplicate custom object `{api_name}`"),
        ));
    }
    let mut builder = ObjectBuilder::new(api_name);
    builder.sharing_model = match tag_text(&xml, "sharingModel").as_deref() {
        None | Some("ReadWrite") => SharingModel::PublicReadWrite,
        Some("Read") => SharingModel::PublicReadOnly,
        Some("Private") => SharingModel::Private,
        Some("ControlledByParent") => SharingModel::ControlledByParent,
        Some(value) => {
            return Err(MetadataError::invalid(
                path,
                format!("unsupported sharing model `{value}`"),
            ));
        }
    };

    if let Some(name_field) = first_element(&xml, "nameField") {
        let field_type = required_text(path, name_field, "type")?;
        if !matches!(field_type.as_str(), "Text" | "AutoNumber") {
            return Err(MetadataError::invalid(
                path,
                format!("unsupported name field type `{field_type}`"),
            ));
        }
        builder
            .fields
            .push(FieldSchema::new("Name", FieldType::String, false));
    }
    for field_xml in elements(&xml, "fields") {
        builder.push_field(parse_field(path, field_xml, None)?);
    }
    objects.insert(canonical, builder);
    Ok(())
}

fn import_field_file(
    path: &Path,
    objects: &mut BTreeMap<String, ObjectBuilder>,
) -> Result<(), MetadataError> {
    let xml = fs::read_to_string(path).map_err(|error| MetadataError::io(path, "read", error))?;
    let object_name = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            MetadataError::invalid(path, "field file must be under objects/<Object>/fields")
        })?;
    let fallback_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".field-meta.xml"))
        .ok_or_else(|| MetadataError::invalid(path, "invalid field metadata filename"))?;
    let builder = objects
        .get_mut(&canonical_name(object_name))
        .ok_or_else(|| {
            MetadataError::invalid(
                path,
                format!("field belongs to unknown custom object `{object_name}`"),
            )
        })?;
    builder.push_field(parse_field(path, &xml, Some(fallback_name))?);
    Ok(())
}

fn parse_field(
    path: &Path,
    xml: &str,
    fallback_name: Option<&str>,
) -> Result<ParsedField, MetadataError> {
    let api_name = tag_text(xml, "fullName")
        .or_else(|| fallback_name.map(ToOwned::to_owned))
        .ok_or_else(|| MetadataError::invalid(path, "field is missing `<fullName>`"))?;
    let metadata_type = required_text(path, xml, "type")?;
    if metadata_type == "Summary" {
        return parse_summary(path, xml, api_name);
    }
    let display_type = metadata_display_type(&metadata_type);
    let data_type = match metadata_type.as_str() {
        "Checkbox" => FieldType::Boolean,
        "Number" => {
            let scale = tag_text(xml, "scale").unwrap_or_else(|| "0".to_owned());
            if scale != "0" {
                return Err(MetadataError::invalid(
                    path,
                    format!(
                        "Number field `{api_name}` has scale {scale}; Decimal storage is not supported"
                    ),
                ));
            }
            FieldType::Integer
        }
        "Text"
        | "TextArea"
        | "LongTextArea"
        | "Email"
        | "Phone"
        | "Url"
        | "Picklist"
        | "MultiselectPicklist"
        | "AutoNumber"
        | "EncryptedText" => FieldType::String,
        "Date" => FieldType::Date,
        "DateTime" => FieldType::Datetime,
        "Lookup" | "MasterDetail" => {
            let target_object = required_text(path, xml, "referenceTo")?;
            FieldType::Reference { target_object }
        }
        "MetadataRelationship" => {
            let target_metadata = required_text(path, xml, "referenceTo")?;
            let controlling_field = tag_text(xml, "metadataRelationshipControllingField");
            FieldType::MetadataRelationship {
                target_metadata,
                controlling_field,
            }
        }
        "Id" => FieldType::Id,
        unsupported => {
            return Err(MetadataError::invalid(
                path,
                format!("unsupported field type `{unsupported}` on `{api_name}`"),
            ));
        }
    };
    let required =
        tag_text(xml, "required").is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let external_id =
        tag_text(xml, "externalId").is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let unique = tag_text(xml, "unique").is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let relationship_name = tag_text(xml, "relationshipName");
    let label = tag_text(xml, "label").unwrap_or_else(|| api_name.clone());
    let length = tag_text(xml, "length")
        .map(|value| {
            value.parse::<usize>().map_err(|_| {
                MetadataError::invalid(
                    path,
                    format!("field `{api_name}` has invalid length `{value}`"),
                )
            })
        })
        .transpose()?;
    let picklist_values = elements(xml, "value")
        .filter_map(|value| tag_text(value, "fullName"))
        .collect();
    let mut field = FieldSchema::new(api_name, data_type, !required).with_describe(
        label,
        length,
        tag_text(xml, "inlineHelpText"),
        display_type,
        picklist_values,
    );
    if external_id {
        field = field.with_external_id(unique);
    }
    Ok(ParsedField::Ready(match relationship_name {
        Some(name) => field.with_relationship_name(name),
        None => field,
    }))
}

fn metadata_display_type(metadata_type: &str) -> DisplayType {
    match metadata_type {
        "Checkbox" => DisplayType::Boolean,
        "Number" => DisplayType::Double,
        "Text" | "AutoNumber" => DisplayType::String,
        "TextArea" | "LongTextArea" => DisplayType::TextArea,
        "Email" => DisplayType::Email,
        "Phone" => DisplayType::Phone,
        "Url" => DisplayType::Url,
        "Picklist" => DisplayType::Picklist,
        "MultiselectPicklist" => DisplayType::MultiPicklist,
        "EncryptedText" => DisplayType::EncryptedString,
        "Date" => DisplayType::Date,
        "DateTime" => DisplayType::Datetime,
        "Lookup" | "MasterDetail" | "MetadataRelationship" => DisplayType::Reference,
        "Id" => DisplayType::Id,
        "Summary" => DisplayType::Double,
        _ => DisplayType::AnyType,
    }
}

fn parse_summary(path: &Path, xml: &str, api_name: String) -> Result<ParsedField, MetadataError> {
    let operation = match required_text(path, xml, "summaryOperation")?.as_str() {
        "count" => SummaryOperation::Count,
        "sum" => SummaryOperation::Sum,
        "min" => SummaryOperation::Min,
        "max" => SummaryOperation::Max,
        unsupported => {
            return Err(MetadataError::invalid(
                path,
                format!("unsupported roll-up summary operation `{unsupported}`"),
            ));
        }
    };
    let foreign_key = required_text(path, xml, "summaryForeignKey")?;
    let (child_object, foreign_key_field) =
        qualified_field(path, &foreign_key, "summaryForeignKey")?;
    let summarized_field = tag_text(xml, "summarizedField")
        .map(|field| {
            let (object, field) = qualified_field(path, &field, "summarizedField")?;
            if !object.eq_ignore_ascii_case(&child_object) {
                return Err(MetadataError::invalid(
                    path,
                    format!(
                        "summarized field object `{object}` does not match roll-up child object `{child_object}`"
                    ),
                ));
            }
            Ok(field)
        })
        .transpose()?;
    if operation != SummaryOperation::Count && summarized_field.is_none() {
        return Err(MetadataError::invalid(
            path,
            format!(
                "{} roll-up summary `{api_name}` is missing `<summarizedField>`",
                summary_operation_name(operation)
            ),
        ));
    }

    let mut filters = Vec::new();
    for filter_xml in elements(xml, "summaryFilterItems") {
        let field = required_text(path, filter_xml, "field")?;
        let (object, field) = qualified_field(path, &field, "summary filter field")?;
        if !object.eq_ignore_ascii_case(&child_object) {
            return Err(MetadataError::invalid(
                path,
                format!(
                    "summary filter object `{object}` does not match roll-up child object `{child_object}`"
                ),
            ));
        }
        let operator = match required_text(path, filter_xml, "operation")?.as_str() {
            "equals" => SummaryFilterOperator::Equal,
            unsupported => {
                return Err(MetadataError::invalid(
                    path,
                    format!("unsupported roll-up summary filter operation `{unsupported}`"),
                ));
            }
        };
        filters.push(SummaryFilter {
            field,
            operator,
            value: required_text(path, filter_xml, "value")?,
        });
    }

    Ok(ParsedField::Summary(PendingSummary {
        path: path.to_path_buf(),
        api_name,
        nullable: operation != SummaryOperation::Count,
        definition: SummaryDefinition {
            child_object,
            foreign_key_field,
            operation,
            summarized_field,
            filters,
        },
    }))
}

fn resolve_summaries(objects: &mut BTreeMap<String, ObjectBuilder>) -> Result<(), MetadataError> {
    let mut pending = Vec::new();
    for (parent, builder) in objects.iter_mut() {
        pending.extend(
            std::mem::take(&mut builder.summaries)
                .into_iter()
                .map(|summary| (parent.clone(), summary)),
        );
    }
    for (parent_key, summary) in pending {
        let parent = objects
            .get(&parent_key)
            .expect("pending roll-up parent remains in object map");
        let child = objects
            .get(&canonical_name(&summary.definition.child_object))
            .ok_or_else(|| {
                MetadataError::invalid(
                    &summary.path,
                    format!(
                        "roll-up summary child object `{}` is not present in imported metadata",
                        summary.definition.child_object
                    ),
                )
            })?;
        let foreign_key =
            builder_field(child, &summary.definition.foreign_key_field).ok_or_else(|| {
                MetadataError::invalid(
                    &summary.path,
                    format!(
                        "roll-up foreign key `{}.{}` is not present in imported metadata",
                        child.api_name, summary.definition.foreign_key_field
                    ),
                )
            })?;
        match foreign_key.data_type() {
            FieldType::Reference { target_object }
                if target_object.eq_ignore_ascii_case(&parent.api_name) => {}
            FieldType::Reference { target_object } => {
                return Err(MetadataError::invalid(
                    &summary.path,
                    format!(
                        "roll-up foreign key `{}.{}` targets `{target_object}`, not `{}`",
                        child.api_name, summary.definition.foreign_key_field, parent.api_name
                    ),
                ));
            }
            _ => {
                return Err(MetadataError::invalid(
                    &summary.path,
                    format!(
                        "roll-up foreign key `{}.{}` is not a Lookup or MasterDetail field",
                        child.api_name, summary.definition.foreign_key_field
                    ),
                ));
            }
        }
        for filter in &summary.definition.filters {
            let field = builder_field(child, &filter.field).ok_or_else(|| {
                MetadataError::invalid(
                    &summary.path,
                    format!(
                        "roll-up filter field `{}.{}` is not present in imported metadata",
                        child.api_name, filter.field
                    ),
                )
            })?;
            if !valid_summary_filter_value(field.data_type(), &filter.value) {
                return Err(MetadataError::invalid(
                    &summary.path,
                    format!(
                        "roll-up filter value `{}` is invalid for `{}.{}` ({:?})",
                        filter.value,
                        child.api_name,
                        field.api_name(),
                        field.data_type()
                    ),
                ));
            }
        }
        let result_type = match summary.definition.operation {
            SummaryOperation::Count => FieldType::Integer,
            SummaryOperation::Sum | SummaryOperation::Min | SummaryOperation::Max => {
                let field_name = summary
                    .definition
                    .summarized_field
                    .as_deref()
                    .expect("non-count roll-up has summarized field");
                let field = builder_field(child, field_name).ok_or_else(|| {
                    MetadataError::invalid(
                        &summary.path,
                        format!(
                            "summarized field `{}.{field_name}` is not present in imported metadata",
                            child.api_name
                        ),
                    )
                })?;
                match (summary.definition.operation, field.data_type()) {
                    (SummaryOperation::Sum, FieldType::Integer)
                    | (
                        SummaryOperation::Min | SummaryOperation::Max,
                        FieldType::Integer | FieldType::Date | FieldType::Datetime,
                    ) => field.data_type().clone(),
                    (operation, unsupported) => {
                        return Err(MetadataError::invalid(
                            &summary.path,
                            format!(
                                "{} roll-up summary `{}` cannot summarize field type {unsupported:?}",
                                summary_operation_name(operation),
                                summary.api_name
                            ),
                        ));
                    }
                }
            }
        };
        objects
            .get_mut(&parent_key)
            .expect("pending roll-up parent remains in object map")
            .fields
            .push(FieldSchema::new(
                summary.api_name,
                FieldType::Summary {
                    result_type: Box::new(result_type),
                    definition: summary.definition,
                },
                summary.nullable,
            ));
    }
    Ok(())
}

fn builder_field<'a>(builder: &'a ObjectBuilder, api_name: &str) -> Option<&'a FieldSchema> {
    builder
        .fields
        .iter()
        .find(|field| field.api_name().eq_ignore_ascii_case(api_name))
}

fn valid_summary_filter_value(field_type: &FieldType, value: &str) -> bool {
    match field_type {
        FieldType::Boolean => {
            value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false")
        }
        FieldType::Integer => value.parse::<i64>().is_ok(),
        FieldType::Date => NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok(),
        FieldType::Datetime => DateTime::parse_from_rfc3339(value).is_ok(),
        FieldType::String
        | FieldType::Id
        | FieldType::Reference { .. }
        | FieldType::MetadataRelationship { .. } => true,
        FieldType::Summary { .. } => false,
    }
}

fn qualified_field(
    path: &Path,
    value: &str,
    element: &str,
) -> Result<(String, String), MetadataError> {
    let Some((object, field)) = value.split_once('.') else {
        return Err(MetadataError::invalid(
            path,
            format!("`<{element}>` value `{value}` must be Object.Field"),
        ));
    };
    if object.is_empty() || field.is_empty() || field.contains('.') {
        return Err(MetadataError::invalid(
            path,
            format!("`<{element}>` value `{value}` must be Object.Field"),
        ));
    }
    Ok((object.to_owned(), field.to_owned()))
}

fn summary_operation_name(operation: SummaryOperation) -> &'static str {
    match operation {
        SummaryOperation::Count => "count",
        SummaryOperation::Sum => "sum",
        SummaryOperation::Min => "min",
        SummaryOperation::Max => "max",
    }
}

fn object_name_from_path(path: &Path) -> Result<String, MetadataError> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| MetadataError::invalid(path, "object metadata filename is not UTF-8"))?;
    name.strip_suffix(".object-meta.xml")
        .or_else(|| name.strip_suffix(".object"))
        .map(ToOwned::to_owned)
        .ok_or_else(|| MetadataError::invalid(path, "invalid object metadata filename"))
}

fn required_text(path: &Path, xml: &str, tag: &str) -> Result<String, MetadataError> {
    tag_text(xml, tag)
        .ok_or_else(|| MetadataError::invalid(path, format!("missing `<{tag}>` element")))
}

fn tag_text(xml: &str, tag: &str) -> Option<String> {
    first_element(xml, tag).map(|value| decode_xml(value.trim()))
}

fn first_element<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    elements(xml, tag).next()
}

fn elements<'a>(xml: &'a str, tag: &str) -> impl Iterator<Item = &'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut cursor = 0;
    std::iter::from_fn(move || {
        let start = xml[cursor..].find(&open)? + cursor + open.len();
        let end = xml[start..].find(&close)? + start;
        cursor = end + close.len();
        Some(&xml[start..end])
    })
}

fn decode_xml(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn imports_decomposed_custom_objects_fields_and_relationships() {
        let root = temp_directory();
        let object = root.join("main/default/objects/Invoice__c");
        write_decomposed_invoice(&object);
        write_decomposed_invoice_line(&root.join("main/default/objects/InvoiceLine__c"));
        let metadata = root.join("main/default/objects/Mapping__mdt");
        write_decomposed_mapping(&metadata);

        let schema = import_metadata([&root]).unwrap();
        let invoice = schema.object("invoice__C").unwrap();
        assert_eq!(invoice.fields().len(), 10);
        assert_eq!(
            invoice.field("OwnerId").unwrap().data_type(),
            &FieldType::Id
        );
        assert_eq!(
            invoice.field("Amount__c").unwrap().data_type(),
            &FieldType::Integer
        );
        assert!(!invoice.field("Amount__c").unwrap().is_nullable());
        assert!(invoice.field("Amount__c").unwrap().is_external_id());
        assert!(invoice.field("Amount__c").unwrap().is_unique());
        assert_eq!(
            invoice.field("Account__c").unwrap().data_type(),
            &FieldType::Reference {
                target_object: "Account".to_owned()
            }
        );
        assert_eq!(
            invoice.field("Total__c").unwrap().data_type(),
            &FieldType::Summary {
                result_type: Box::new(FieldType::Integer),
                definition: SummaryDefinition {
                    child_object: "InvoiceLine__c".to_owned(),
                    foreign_key_field: "Invoice__c".to_owned(),
                    operation: SummaryOperation::Sum,
                    summarized_field: Some("Amount__c".to_owned()),
                    filters: Vec::new(),
                },
            }
        );
        assert_eq!(
            invoice.field("PaidLines__c").unwrap().data_type(),
            &FieldType::Summary {
                result_type: Box::new(FieldType::Integer),
                definition: SummaryDefinition {
                    child_object: "InvoiceLine__c".to_owned(),
                    foreign_key_field: "Invoice__c".to_owned(),
                    operation: SummaryOperation::Count,
                    summarized_field: None,
                    filters: vec![SummaryFilter {
                        field: "Paid__c".to_owned(),
                        operator: SummaryFilterOperator::Equal,
                        value: "true".to_owned(),
                    }],
                },
            }
        );
        assert_eq!(
            schema
                .field("Mapping__mdt", "TargetField__c")
                .unwrap()
                .data_type(),
            &FieldType::MetadataRelationship {
                target_metadata: "FieldDefinition".to_owned(),
                controlling_field: Some("Mapping__mdt.TargetType__c".to_owned()),
            }
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_rollup_filter_values_that_do_not_match_the_child_field_type() {
        let root = temp_directory();
        let invoice = root.join("main/default/objects/Invoice__c");
        let line = root.join("main/default/objects/InvoiceLine__c");
        write_decomposed_invoice(&invoice);
        write_decomposed_invoice_line(&line);
        let path = invoice.join("fields/PaidLines__c.field-meta.xml");
        let xml = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            xml.replace("<value>true</value>", "<value>yes</value>"),
        )
        .unwrap();

        let error = import_metadata([&root]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("roll-up filter value `yes` is invalid for `InvoiceLine__c.Paid__c`"),
            "{error}"
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn write_decomposed_invoice(object: &Path) {
        fs::create_dir_all(object.join("fields")).unwrap();
        fs::write(
            object.join("Invoice__c.object-meta.xml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<CustomObject xmlns="http://soap.sforce.com/2006/04/metadata">
  <label>Invoice</label>
  <nameField><label>Invoice Number</label><type>AutoNumber</type></nameField>
</CustomObject>"#,
        )
        .unwrap();
        fs::write(
            object.join("fields/Amount__c.field-meta.xml"),
            r#"<CustomField><fullName>Amount__c</fullName><externalId>true</externalId><required>true</required><scale>0</scale><type>Number</type><unique>true</unique></CustomField>"#,
        )
        .unwrap();
        fs::write(
            object.join("fields/Account__c.field-meta.xml"),
            r#"<CustomField><fullName>Account__c</fullName><referenceTo>Account</referenceTo><type>Lookup</type></CustomField>"#,
        )
        .unwrap();
        fs::write(
            object.join("fields/Total__c.field-meta.xml"),
            r#"<CustomField>
<fullName>Total__c</fullName>
<summarizedField>InvoiceLine__c.Amount__c</summarizedField>
<summaryForeignKey>InvoiceLine__c.Invoice__c</summaryForeignKey>
<summaryOperation>sum</summaryOperation>
<type>Summary</type>
</CustomField>"#,
        )
        .unwrap();
        fs::write(
            object.join("fields/PaidLines__c.field-meta.xml"),
            r#"<CustomField>
<fullName>PaidLines__c</fullName>
<summaryFilterItems><field>InvoiceLine__c.Paid__c</field><operation>equals</operation><value>true</value></summaryFilterItems>
<summaryForeignKey>InvoiceLine__c.Invoice__c</summaryForeignKey>
<summaryOperation>count</summaryOperation>
<type>Summary</type>
</CustomField>"#,
        )
        .unwrap();
    }

    fn write_decomposed_invoice_line(object: &Path) {
        fs::create_dir_all(object.join("fields")).unwrap();
        fs::write(
            object.join("InvoiceLine__c.object-meta.xml"),
            r#"<CustomObject><label>Invoice Line</label></CustomObject>"#,
        )
        .unwrap();
        fs::write(
            object.join("fields/Amount__c.field-meta.xml"),
            r#"<CustomField><fullName>Amount__c</fullName><scale>0</scale><type>Number</type></CustomField>"#,
        )
        .unwrap();
        fs::write(
            object.join("fields/Paid__c.field-meta.xml"),
            r#"<CustomField><fullName>Paid__c</fullName><type>Checkbox</type></CustomField>"#,
        )
        .unwrap();
        fs::write(
            object.join("fields/Invoice__c.field-meta.xml"),
            r#"<CustomField><fullName>Invoice__c</fullName><referenceTo>Invoice__c</referenceTo><type>MasterDetail</type></CustomField>"#,
        )
        .unwrap();
    }

    fn write_decomposed_mapping(object: &Path) {
        fs::create_dir_all(object.join("fields")).unwrap();
        fs::write(
            object.join("Mapping__mdt.object-meta.xml"),
            r#"<CustomObject><label>Mapping</label></CustomObject>"#,
        )
        .unwrap();
        fs::write(
            object.join("fields/TargetField__c.field-meta.xml"),
            r#"<CustomField>
<fullName>TargetField__c</fullName>
<metadataRelationshipControllingField>Mapping__mdt.TargetType__c</metadataRelationshipControllingField>
<referenceTo>FieldDefinition</referenceTo>
<relationshipName>TargetField</relationshipName>
<required>true</required>
<type>MetadataRelationship</type>
</CustomField>"#,
        )
        .unwrap();
    }

    #[test]
    fn imports_monolithic_object_fields_and_rejects_unsupported_types() {
        let root = temp_directory();
        let objects = root.join("objects");
        fs::create_dir_all(&objects).unwrap();
        fs::write(
            objects.join("Widget__c.object"),
            r#"<CustomObject>
<fields><fullName>Enabled__c</fullName><type>Checkbox</type></fields>
<fields><fullName>Label__c</fullName><type>Text</type></fields>
</CustomObject>"#,
        )
        .unwrap();
        let schema = import_metadata([&root]).unwrap();
        let widget = schema.object("Widget__c").unwrap();
        assert_eq!(widget.fields().len(), 7);
        assert_eq!(widget.field("OwnerId").unwrap().data_type(), &FieldType::Id);

        fs::write(
            objects.join("Bad__c.object"),
            r#"<CustomObject><fields><fullName>Where__c</fullName><type>Geolocation</type></fields></CustomObject>"#,
        )
        .unwrap();
        let error = import_metadata([&root]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported field type `Geolocation`")
        );

        fs::remove_file(objects.join("Bad__c.object")).unwrap();
        fs::write(
            objects.join("BadRelationship__mdt.object"),
            r#"<CustomObject><fields><fullName>Target__c</fullName><type>MetadataRelationship</type></fields></CustomObject>"#,
        )
        .unwrap();
        let error = import_metadata([&root]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("missing `<referenceTo>` element")
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn temp_directory() -> PathBuf {
        let unique = format!(
            "apex-exec-metadata-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).unwrap();
        path
    }
}
