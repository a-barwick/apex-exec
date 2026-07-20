use super::{CacheVisibility, DisplayType, LoggingLevel, SoapType};
use crate::ast::TypeName;

/// Checked values for the supported platform enums.
///
/// Keeping these values typed avoids reducing platform enums to rendered strings
/// at the compiler/runtime boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformEnum {
    ParentJobResult(ParentJobResult),
    Quiddity(Quiddity),
    LoggingLevel(LoggingLevel),
    CacheVisibility(CacheVisibility),
    SoapType(SoapType),
    DisplayType(DisplayType),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformEnumDescriptor {
    ParentJobResult,
    Quiddity,
    LoggingLevel,
    CacheVisibility,
    SoapType,
    DisplayType,
}

impl PlatformEnumDescriptor {
    pub fn from_owner(owner: &str) -> Option<Self> {
        match owner {
            "parentjobresult" | "system.parentjobresult" => Some(Self::ParentJobResult),
            "quiddity" | "system.quiddity" => Some(Self::Quiddity),
            "logginglevel" | "system.logginglevel" => Some(Self::LoggingLevel),
            "cache.visibility" => Some(Self::CacheVisibility),
            "soaptype" | "schema.soaptype" => Some(Self::SoapType),
            "displaytype" | "schema.displaytype" => Some(Self::DisplayType),
            _ => None,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::ParentJobResult => "ParentJobResult",
            Self::Quiddity => "Quiddity",
            Self::LoggingLevel => "LoggingLevel",
            Self::CacheVisibility => "Cache.Visibility",
            Self::SoapType => "Schema.SoapType",
            Self::DisplayType => "Schema.DisplayType",
        }
    }

    pub fn ty(self) -> TypeName {
        match self {
            Self::ParentJobResult => TypeName::ParentJobResult,
            Self::Quiddity => TypeName::Quiddity,
            Self::LoggingLevel => TypeName::LoggingLevel,
            Self::CacheVisibility => TypeName::CacheVisibility,
            Self::SoapType => TypeName::SoapType,
            Self::DisplayType => TypeName::DisplayType,
        }
    }

    pub fn parse(self, name: &str) -> Option<PlatformEnum> {
        match self {
            Self::ParentJobResult => {
                ParentJobResult::from_apex_name(name).map(PlatformEnum::ParentJobResult)
            }
            Self::Quiddity => Quiddity::from_apex_name(name).map(PlatformEnum::Quiddity),
            Self::LoggingLevel => {
                LoggingLevel::from_apex_name(name).map(PlatformEnum::LoggingLevel)
            }
            Self::CacheVisibility => {
                CacheVisibility::from_apex_name(name).map(PlatformEnum::CacheVisibility)
            }
            Self::SoapType => SoapType::from_apex_name(name).map(PlatformEnum::SoapType),
            Self::DisplayType => DisplayType::from_apex_name(name).map(PlatformEnum::DisplayType),
        }
    }
}

impl PlatformEnum {
    pub fn ty(self) -> TypeName {
        match self {
            Self::ParentJobResult(_) => TypeName::ParentJobResult,
            Self::Quiddity(_) => TypeName::Quiddity,
            Self::LoggingLevel(_) => TypeName::LoggingLevel,
            Self::CacheVisibility(_) => TypeName::CacheVisibility,
            Self::SoapType(_) => TypeName::SoapType,
            Self::DisplayType(_) => TypeName::DisplayType,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::ParentJobResult(value) => value.apex_name(),
            Self::Quiddity(value) => value.apex_name(),
            Self::LoggingLevel(value) => value.apex_name(),
            Self::CacheVisibility(value) => value.apex_name(),
            Self::SoapType(value) => value.apex_name(),
            Self::DisplayType(value) => value.apex_name(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParentJobResult {
    Success,
    UnhandledException,
}

impl ParentJobResult {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "SUCCESS" => Some(Self::Success),
            "UNHANDLED_EXCEPTION" => Some(Self::UnhandledException),
            _ => None,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::UnhandledException => "UNHANDLED_EXCEPTION",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quiddity {
    RemoteAction,
    RunTestAsync,
    RunTestDeploy,
    RunTestSync,
    Undefined,
}

impl Quiddity {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "REMOTE_ACTION" => Some(Self::RemoteAction),
            "RUNTEST_ASYNC" => Some(Self::RunTestAsync),
            "RUNTEST_DEPLOY" => Some(Self::RunTestDeploy),
            "RUNTEST_SYNC" => Some(Self::RunTestSync),
            "UNDEFINED" => Some(Self::Undefined),
            _ => None,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::RemoteAction => "REMOTE_ACTION",
            Self::RunTestAsync => "RUNTEST_ASYNC",
            Self::RunTestDeploy => "RUNTEST_DEPLOY",
            Self::RunTestSync => "RUNTEST_SYNC",
            Self::Undefined => "UNDEFINED",
        }
    }
}
