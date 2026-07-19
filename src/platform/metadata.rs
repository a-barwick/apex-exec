use super::{FieldSchema, FieldType, ObjectSchema, SchemaCatalog, SchemaError};
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
    for root in source_roots {
        collect_metadata_files(root.as_ref(), &mut object_files, &mut field_files)?;
    }
    object_files.sort();
    object_files.dedup();
    field_files.sort();
    field_files.dedup();

    let mut objects = BTreeMap::<String, ObjectBuilder>::new();
    for path in object_files {
        import_object_file(&path, &mut objects)?;
    }
    for path in field_files {
        import_field_file(&path, &mut objects)?;
    }

    let mut catalog = SchemaCatalog::new();
    for builder in objects.into_values() {
        catalog.insert_object(builder.finish()?)?;
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
    fields: Vec<FieldSchema>,
}

impl ObjectBuilder {
    fn new(api_name: String) -> Self {
        Self {
            api_name,
            fields: vec![
                FieldSchema::new("Id", FieldType::Id, false),
                FieldSchema::new("CreatedDate", FieldType::Datetime, true),
                FieldSchema::new("LastModifiedDate", FieldType::Datetime, true),
            ],
        }
    }

    fn finish(self) -> Result<ObjectSchema, SchemaError> {
        let mut object = ObjectSchema::new(self.api_name);
        for field in self.fields {
            object.insert_field(field)?;
        }
        Ok(object)
    }
}

fn collect_metadata_files(
    directory: &Path,
    object_files: &mut Vec<PathBuf>,
    field_files: &mut Vec<PathBuf>,
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
            collect_metadata_files(&path, object_files, field_files)?;
        } else if file_type.is_file() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if name.ends_with(".object-meta.xml") || name.ends_with(".object") {
                object_files.push(path);
            } else if name.ends_with(".field-meta.xml") {
                field_files.push(path);
            }
        }
    }
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
        builder.fields.push(parse_field(path, field_xml, None)?);
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
    builder
        .fields
        .push(parse_field(path, &xml, Some(fallback_name))?);
    Ok(())
}

fn parse_field(
    path: &Path,
    xml: &str,
    fallback_name: Option<&str>,
) -> Result<FieldSchema, MetadataError> {
    let api_name = tag_text(xml, "fullName")
        .or_else(|| fallback_name.map(ToOwned::to_owned))
        .ok_or_else(|| MetadataError::invalid(path, "field is missing `<fullName>`"))?;
    let metadata_type = required_text(path, xml, "type")?;
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
    let mut field = FieldSchema::new(api_name, data_type, !required);
    if external_id {
        field = field.with_external_id(unique);
    }
    Ok(match relationship_name {
        Some(name) => field.with_relationship_name(name),
        None => field,
    })
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

        let schema = import_metadata([&root]).unwrap();
        let invoice = schema.object("invoice__C").unwrap();
        assert_eq!(invoice.fields().len(), 6);
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
        assert_eq!(schema.object("Widget__c").unwrap().fields().len(), 5);

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
