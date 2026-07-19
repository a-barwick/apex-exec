//! Typed Salesforce API-version compatibility profiles.
//!
//! Project discovery selects one exact API version for every Apex source unit.
//! Semantic analysis and execution consume the resulting typed profile instead
//! of inspecting metadata or comparing ad-hoc version strings.

use crate::span::{SourceId, Span};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, str::FromStr};

const MIN_CURRENT_API_MAJOR: u16 = 60;
const MAX_CURRENT_API_MAJOR: u16 = 66;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApiVersion {
    major: u16,
    minor: u16,
}

impl ApiVersion {
    pub const API_31: Self = Self::new(31, 0);
    pub const API_66: Self = Self::new(66, 0);

    const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    pub const fn major(self) -> u16 {
        self.major
    }

    pub const fn minor(self) -> u16 {
        self.minor
    }
}

impl FromStr for ApiVersion {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (major, minor) = value.split_once('.').ok_or_else(|| {
            format!("Salesforce API version `{value}` must use `major.minor` form")
        })?;
        if major.is_empty()
            || minor.is_empty()
            || !major.bytes().all(|byte| byte.is_ascii_digit())
            || !minor.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(format!(
                "Salesforce API version `{value}` must use `major.minor` form"
            ));
        }
        let major = major
            .parse::<u16>()
            .map_err(|_| format!("Salesforce API version `{value}` is out of range"))?;
        let minor = minor
            .parse::<u16>()
            .map_err(|_| format!("Salesforce API version `{value}` is out of range"))?;
        if minor != 0 {
            return Err(format!(
                "Salesforce API version `{value}` is not modeled; supported versions use minor version 0"
            ));
        }
        Ok(Self { major, minor })
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

impl Serialize for ApiVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ApiVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "camelCase")]
pub enum CompatibilityBehavior {
    LegacyApi31,
    CurrentApi60To66,
}

impl CompatibilityBehavior {
    pub const fn label(self) -> &'static str {
        match self {
            Self::LegacyApi31 => "legacy-api-31",
            Self::CurrentApi60To66 => "current-api-60-to-66",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CompatibilityProfile {
    api_version: ApiVersion,
    behavior: CompatibilityBehavior,
}

impl CompatibilityProfile {
    pub fn for_api_version(api_version: ApiVersion) -> Result<Self, String> {
        if api_version.minor != 0 {
            return Err(format!(
                "Salesforce API version `{api_version}` is not modeled; supported versions use minor version 0"
            ));
        }
        let behavior = match api_version.major {
            31 => CompatibilityBehavior::LegacyApi31,
            MIN_CURRENT_API_MAJOR..=MAX_CURRENT_API_MAJOR => {
                CompatibilityBehavior::CurrentApi60To66
            }
            _ => {
                return Err(format!(
                    "Salesforce API version `{api_version}` has no modeled compatibility profile; supported versions are 31.0 and 60.0 through 66.0"
                ));
            }
        };
        Ok(Self {
            api_version,
            behavior,
        })
    }

    pub const fn api_version(self) -> ApiVersion {
        self.api_version
    }

    pub const fn behavior(self) -> CompatibilityBehavior {
        self.behavior
    }

    pub fn identity(self) -> String {
        format!("salesforce-api-{}", self.api_version)
    }

    pub const fn null_instanceof_result(self) -> bool {
        matches!(self.behavior, CompatibilityBehavior::LegacyApi31)
    }

    pub const fn supports_current_syntax(self) -> bool {
        matches!(self.behavior, CompatibilityBehavior::CurrentApi60To66)
    }

    pub const fn supports_curated_platform(self) -> bool {
        matches!(self.behavior, CompatibilityBehavior::CurrentApi60To66)
    }
}

impl Default for CompatibilityProfile {
    fn default() -> Self {
        Self {
            api_version: ApiVersion::API_66,
            behavior: CompatibilityBehavior::CurrentApi60To66,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceProfiles {
    default: CompatibilityProfile,
    by_source: BTreeMap<SourceId, CompatibilityProfile>,
}

impl SourceProfiles {
    pub fn new(default: CompatibilityProfile) -> Self {
        Self {
            default,
            by_source: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, source_id: SourceId, profile: CompatibilityProfile) {
        self.by_source.insert(source_id, profile);
    }

    pub fn for_source(&self, source_id: SourceId) -> CompatibilityProfile {
        self.by_source
            .get(&source_id)
            .copied()
            .unwrap_or(self.default)
    }

    pub fn for_span(&self, span: Span) -> CompatibilityProfile {
        self.for_source(span.source_id)
    }
}

impl Default for SourceProfiles {
    fn default() -> Self {
        let profile = CompatibilityProfile::default();
        let mut profiles = Self::new(profile);
        profiles.insert(SourceId::ANONYMOUS, profile);
        profiles
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum ProfileOrigin {
    ProjectDefault,
    Sidecar,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectiveProfile {
    pub source: String,
    pub api_version: ApiVersion,
    pub behavior: CompatibilityBehavior,
    pub origin: ProfileOrigin,
}

impl EffectiveProfile {
    pub fn new(source: String, profile: CompatibilityProfile, origin: ProfileOrigin) -> Self {
        Self {
            source,
            api_version: profile.api_version(),
            behavior: profile.behavior(),
            origin,
        }
    }

    pub fn identity(&self) -> String {
        format!(
            "{}:{}:{}",
            self.source,
            self.api_version,
            self.behavior.label()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_versions_are_strict_and_unmodeled_ranges_fail_explicitly() {
        assert_eq!("31.0".parse::<ApiVersion>().unwrap(), ApiVersion::API_31);
        assert!("31".parse::<ApiVersion>().is_err());
        assert!("31.1".parse::<ApiVersion>().is_err());
        assert!(CompatibilityProfile::for_api_version(ApiVersion::new(59, 0)).is_err());
        assert!(CompatibilityProfile::for_api_version(ApiVersion::new(67, 0)).is_err());
    }

    #[test]
    fn modeled_profiles_expose_the_reviewed_runtime_difference() {
        let legacy = CompatibilityProfile::for_api_version(ApiVersion::API_31).unwrap();
        let current = CompatibilityProfile::for_api_version(ApiVersion::API_66).unwrap();
        assert!(legacy.null_instanceof_result());
        assert!(!current.null_instanceof_result());
        assert!(!legacy.supports_current_syntax());
        assert!(current.supports_current_syntax());
    }
}
