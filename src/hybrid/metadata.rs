use crate::compatibility::CompatibilityProfile;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

const CATALOG_JSON: &str = include_str!("metadata_catalog.json");
const LOCAL_SEMANTICS_TYPES: &[&str] = &["ApexClass", "ApexTrigger", "CustomField", "CustomObject"];

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogDocument {
    schema_version: u32,
    source: CatalogSource,
    types: Vec<CatalogType>,
    org_profiles: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogSource {
    pub name: String,
    pub version: String,
    pub registry_types: usize,
    pub catalog_types: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogType {
    name: String,
    directory: String,
    suffix: Option<String>,
    folder_type: Option<String>,
    in_folder: bool,
    meta_file: bool,
    bundle: bool,
    mixed_content: bool,
    strict_directory: bool,
    children: Vec<CatalogChild>,
    source_supported: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogChild {
    name: String,
    directory: String,
    suffix: Option<String>,
    ignore_parent_name: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityState {
    Supported,
    Unsupported,
    OrgUnavailable,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TypeCapability {
    pub metadata_type: String,
    pub directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_type: Option<String>,
    pub in_folder: bool,
    pub meta_file: bool,
    pub bundle: bool,
    pub mixed_content: bool,
    pub child_types: Vec<String>,
    pub source_supported: bool,
    pub inventory: CapabilityState,
    pub retrieve: CapabilityState,
    pub deploy: CapabilityState,
    pub drift: CapabilityState,
    pub local_semantics: CapabilityState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FileDispositionKind {
    RecognizedMetadata,
    IntentionalNonMetadata,
    UnsupportedMetadata,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileDisposition {
    pub path: PathBuf,
    pub kind: FileDispositionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountingMetric {
    pub supported: usize,
    pub total: usize,
    pub percentage: f64,
}

impl Eq for AccountingMetric {}

impl AccountingMetric {
    fn new(supported: usize, total: usize) -> Self {
        Self {
            supported,
            total,
            percentage: if total == 0 {
                100.0
            } else {
                supported as f64 * 100.0 / total as f64
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetadataAccounting {
    pub catalog_source: CatalogSource,
    pub profile: String,
    pub catalog_types: AccountingMetric,
    pub package_files: AccountingMetric,
    pub components: AccountingMetric,
    pub retrieve: AccountingMetric,
    pub deploy: AccountingMetric,
    pub drift: AccountingMetric,
    pub local_semantics: AccountingMetric,
    pub recognized_files: usize,
    pub intentional_non_metadata_files: usize,
    pub unsupported_metadata_files: usize,
    pub unclassified_files: usize,
    pub dispositions: Vec<FileDisposition>,
    pub capabilities: Vec<TypeCapability>,
}

impl Eq for MetadataAccounting {}

#[derive(Clone, Debug)]
pub(crate) struct ClassifiedMetadata {
    pub metadata_type: String,
    pub full_name: String,
    pub role: String,
    pub category: super::ComponentCategory,
}

pub struct MetadataCatalog {
    document: &'static CatalogDocument,
}

impl MetadataCatalog {
    pub fn bundled() -> Result<Self, String> {
        static DOCUMENT: OnceLock<Result<CatalogDocument, String>> = OnceLock::new();
        let document = DOCUMENT
            .get_or_init(|| {
                let value = serde_json::from_str::<CatalogDocument>(CATALOG_JSON)
                    .map_err(|error| format!("invalid bundled metadata catalog: {error}"))?;
                validate_catalog(value)
            })
            .as_ref()
            .map_err(Clone::clone)?;
        Ok(Self { document })
    }

    pub(crate) fn classify(&self, path: &Path) -> Option<ClassifiedMetadata> {
        let parts = normal_parts(path);
        let file = parts.last()?;
        for (index, directory) in parts.iter().enumerate().rev() {
            for parent in self
                .document
                .types
                .iter()
                .filter(|entry| entry.directory == *directory)
            {
                if let Some(classified) = classify_at(parent, &parts, index, file) {
                    return Some(classified);
                }
            }
        }
        self.classify_by_suffix(&parts, file)
    }

    pub(crate) fn accounting(
        &self,
        profile: CompatibilityProfile,
        dispositions: Vec<FileDisposition>,
        component_count: usize,
    ) -> MetadataAccounting {
        let profile_name = profile.api_version().to_string();
        let enabled = self
            .document
            .org_profiles
            .get(&profile_name)
            .expect("catalog validation covers every compatibility profile")
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let capabilities = catalog_capabilities(&self.document.types, &enabled, &profile_name);
        let recognized = count_disposition(&dispositions, FileDispositionKind::RecognizedMetadata);
        let intentional =
            count_disposition(&dispositions, FileDispositionKind::IntentionalNonMetadata);
        let unsupported =
            count_disposition(&dispositions, FileDispositionKind::UnsupportedMetadata);
        let total_files = dispositions.len();
        let catalog_total = capabilities.len();
        let transport_supported = capabilities
            .iter()
            .filter(|capability| capability.retrieve == CapabilityState::Supported)
            .count();
        MetadataAccounting {
            catalog_source: self.document.source.clone(),
            profile: profile.identity(),
            catalog_types: AccountingMetric::new(catalog_total, catalog_total),
            package_files: AccountingMetric::new(total_files, total_files),
            components: AccountingMetric::new(component_count, component_count),
            retrieve: AccountingMetric::new(transport_supported, catalog_total),
            deploy: AccountingMetric::new(transport_supported, catalog_total),
            drift: AccountingMetric::new(catalog_total, catalog_total),
            local_semantics: AccountingMetric::new(LOCAL_SEMANTICS_TYPES.len(), catalog_total),
            recognized_files: recognized,
            intentional_non_metadata_files: intentional,
            unsupported_metadata_files: unsupported,
            unclassified_files: 0,
            dispositions,
            capabilities,
        }
    }
}

fn catalog_capabilities(
    types: &[CatalogType],
    enabled: &BTreeSet<String>,
    profile: &str,
) -> Vec<TypeCapability> {
    types
        .iter()
        .map(|entry| type_capability(entry, enabled, profile))
        .collect()
}

fn type_capability(
    entry: &CatalogType,
    enabled: &BTreeSet<String>,
    profile: &str,
) -> TypeCapability {
    let available = enabled.contains(&entry.name.to_ascii_lowercase());
    let transport = if available {
        CapabilityState::Supported
    } else {
        CapabilityState::OrgUnavailable
    };
    TypeCapability {
        metadata_type: entry.name.clone(),
        directory: entry.directory.clone(),
        suffix: entry.suffix.clone(),
        folder_type: entry.folder_type.clone(),
        in_folder: entry.in_folder,
        meta_file: entry.meta_file,
        bundle: entry.bundle,
        mixed_content: entry.mixed_content,
        child_types: entry
            .children
            .iter()
            .map(|child| child.name.clone())
            .collect(),
        source_supported: entry.source_supported,
        inventory: CapabilityState::Supported,
        retrieve: transport,
        deploy: transport,
        drift: CapabilityState::Supported,
        local_semantics: local_semantics_state(&entry.name),
        reason: (!available).then(|| {
            format!(
                "not returned by describeMetadata for API {profile} in the guarded validation org"
            )
        }),
    }
}

fn local_semantics_state(metadata_type: &str) -> CapabilityState {
    if LOCAL_SEMANTICS_TYPES
        .iter()
        .any(|name| metadata_type.eq_ignore_ascii_case(name))
    {
        CapabilityState::Supported
    } else {
        CapabilityState::Unsupported
    }
}

pub(crate) fn disposition(
    path: PathBuf,
    classified: Option<&ClassifiedMetadata>,
) -> FileDisposition {
    if let Some(classified) = classified {
        return FileDisposition {
            path,
            kind: FileDispositionKind::RecognizedMetadata,
            metadata_type: Some(classified.metadata_type.clone()),
            full_name: Some(classified.full_name.clone()),
            reason: "matched the API-profiled metadata catalog".to_owned(),
        };
    }
    let intentional = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, ".DS_Store" | "package.xml"));
    FileDisposition {
        path,
        kind: if intentional {
            FileDispositionKind::IntentionalNonMetadata
        } else {
            FileDispositionKind::UnsupportedMetadata
        },
        metadata_type: None,
        full_name: None,
        reason: if intentional {
            "package support file, not a deployable metadata component".to_owned()
        } else {
            "file is inside a package root but has no convention in the pinned catalog".to_owned()
        },
    }
}

fn validate_catalog(value: CatalogDocument) -> Result<CatalogDocument, String> {
    if value.schema_version != 1 || value.types.len() != value.source.catalog_types {
        return Err("bundled metadata catalog schema/count does not match its source".to_owned());
    }
    for version in [
        "31.0", "60.0", "61.0", "62.0", "63.0", "64.0", "65.0", "66.0",
    ] {
        if !value.org_profiles.contains_key(version) {
            return Err(format!("bundled metadata catalog is missing API {version}"));
        }
    }
    Ok(value)
}

fn normal_parts(path: &Path) -> Vec<&str> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect()
}

fn classify_at(
    parent: &CatalogType,
    parts: &[&str],
    index: usize,
    file: &str,
) -> Option<ClassifiedMetadata> {
    if parent.bundle || parent.mixed_content {
        let bundle = *parts.get(index + 1)?;
        let nested = index + 2 < parts.len();
        if parent.bundle || nested {
            return nested.then(|| ClassifiedMetadata {
                metadata_type: parent.name.clone(),
                full_name: bundle.to_owned(),
                role: parts[index + 2..].join("/"),
                category: category_for(&parent.name),
            });
        }
    }
    classify_child(parent, parts, index, file)
        .or_else(|| classify_parent_file(parent, parts, index, file))
}

fn classify_child(
    parent: &CatalogType,
    parts: &[&str],
    index: usize,
    file: &str,
) -> Option<ClassifiedMetadata> {
    let child_directory = parts.get(index + 2)?;
    let child = parent
        .children
        .iter()
        .find(|child| child.directory == *child_directory)?;
    let member = strip_metadata_suffix(file, child.suffix.as_deref()?)?;
    let owner = *parts.get(index + 1)?;
    (index + 3 == parts.len() - 1).then(|| ClassifiedMetadata {
        metadata_type: child.name.clone(),
        full_name: if child.ignore_parent_name {
            member.to_owned()
        } else {
            format!("{owner}.{member}")
        },
        role: file.to_owned(),
        category: category_for(&child.name),
    })
}

fn classify_parent_file(
    parent: &CatalogType,
    parts: &[&str],
    index: usize,
    file: &str,
) -> Option<ClassifiedMetadata> {
    if parent.in_folder && index + 2 == parts.len() - 1 {
        let member = strip_metadata_suffix(file, parent.suffix.as_deref()?)?;
        return Some(ClassifiedMetadata {
            metadata_type: parent.name.clone(),
            full_name: format!("{}/{member}", parts[index + 1]),
            role: file.to_owned(),
            category: category_for(&parent.name),
        });
    }
    if index + 1 != parts.len() - 1 {
        return None;
    }
    let suffix = parent.suffix.as_deref()?;
    let member = strip_metadata_suffix(file, suffix)?;
    Some(ClassifiedMetadata {
        metadata_type: parent.name.clone(),
        full_name: member.to_owned(),
        role: file.to_owned(),
        category: category_for(&parent.name),
    })
}

impl MetadataCatalog {
    fn classify_by_suffix(&self, parts: &[&str], file: &str) -> Option<ClassifiedMetadata> {
        let matches = self
            .document
            .types
            .iter()
            .filter_map(|entry| {
                let suffix = entry.suffix.as_deref()?;
                if suffix.eq_ignore_ascii_case("xml") {
                    return None;
                }
                let member = strip_metadata_suffix(file, suffix)?;
                Some((entry, member))
            })
            .collect::<Vec<_>>();
        let (entry, member) = matches
            .iter()
            .find(|(entry, _)| !entry.strict_directory)
            .or_else(|| (matches.len() == 1).then(|| &matches[0]))?;
        Some(ClassifiedMetadata {
            metadata_type: entry.name.clone(),
            full_name: if entry.in_folder && parts.len() > 1 {
                format!("{}/{member}", parts[parts.len() - 2])
            } else {
                (*member).to_owned()
            },
            role: file.to_owned(),
            category: category_for(&entry.name),
        })
    }
}

fn category_for(metadata_type: &str) -> super::ComponentCategory {
    match metadata_type {
        "ApexClass" | "ApexTrigger" | "ApexComponent" | "ApexPage" => {
            super::ComponentCategory::Code
        }
        "CustomObject" | "CustomField" | "Index" => super::ComponentCategory::Schema,
        _ => super::ComponentCategory::Configuration,
    }
}

fn strip_metadata_suffix<'a>(file: &'a str, suffix: &str) -> Option<&'a str> {
    file.strip_suffix(&format!(".{suffix}-meta.xml"))
        .or_else(|| file.strip_suffix(&format!(".{suffix}")))
}

fn count_disposition(dispositions: &[FileDisposition], kind: FileDispositionKind) -> usize {
    dispositions
        .iter()
        .filter(|disposition| disposition.kind == kind)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compatibility::ApiVersion;

    #[test]
    fn catalog_covers_every_modeled_profile_and_multipart_names() {
        let catalog = MetadataCatalog::bundled().unwrap();
        let custom_metadata = catalog
            .classify(Path::new(
                "force-app/main/default/customMetadata/Feature.Flag.md-meta.xml",
            ))
            .unwrap();
        assert_eq!(custom_metadata.metadata_type, "CustomMetadata");
        assert_eq!(custom_metadata.full_name, "Feature.Flag");

        for version in [
            "31.0", "60.0", "61.0", "62.0", "63.0", "64.0", "65.0", "66.0",
        ] {
            let version = version.parse::<ApiVersion>().unwrap();
            let profile = CompatibilityProfile::for_api_version(version).unwrap();
            let report = catalog.accounting(profile, Vec::new(), 0);
            assert_eq!(report.catalog_types.total, 548);
            assert_eq!(report.unclassified_files, 0);
        }
    }

    #[test]
    fn catalog_classifies_children_bundles_folders_sidecars_and_namespaces() {
        let catalog = MetadataCatalog::bundled().unwrap();
        let cases = [
            (
                "objects/ns__Invoice__c/fields/ns__Amount__c.field-meta.xml",
                "CustomField",
                "ns__Invoice__c.ns__Amount__c",
            ),
            (
                "lwc/invoiceCard/invoiceCard.js",
                "LightningComponentBundle",
                "invoiceCard",
            ),
            (
                "reports/Operations/Revenue.report-meta.xml",
                "Report",
                "Operations/Revenue",
            ),
            (
                "classes/Release.Service.cls-meta.xml",
                "ApexClass",
                "Release.Service",
            ),
        ];
        for (path, metadata_type, full_name) in cases {
            let classified = catalog.classify(Path::new(path)).unwrap();
            assert_eq!(classified.metadata_type, metadata_type);
            assert_eq!(classified.full_name, full_name);
        }
    }
}
