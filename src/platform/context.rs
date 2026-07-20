use super::{CacheVisibility, DisplayType, LoggingLevel, SoapType};
use crate::ast::TypeName;

/// Checked values for the supported platform enums.
///
/// Keeping these values typed avoids reducing platform enums to rendered strings
/// at the compiler/runtime boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlatformEnum {
    ParentJobResult(ParentJobResult),
    Quiddity(Quiddity),
    TriggerOperation(TriggerOperation),
    LoggingLevel(LoggingLevel),
    CacheVisibility(CacheVisibility),
    SoapType(SoapType),
    DisplayType(DisplayType),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlatformEnumDescriptor {
    ParentJobResult,
    Quiddity,
    TriggerOperation,
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
            "triggeroperation" | "system.triggeroperation" => Some(Self::TriggerOperation),
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
            Self::TriggerOperation => "TriggerOperation",
            Self::LoggingLevel => "LoggingLevel",
            Self::CacheVisibility => "Cache.Visibility",
            Self::SoapType => "Schema.SoapType",
            Self::DisplayType => "Schema.DisplayType",
        }
    }

    pub fn from_type(ty: &TypeName) -> Option<Self> {
        match ty {
            TypeName::ParentJobResult => Some(Self::ParentJobResult),
            TypeName::Quiddity => Some(Self::Quiddity),
            TypeName::TriggerOperation => Some(Self::TriggerOperation),
            TypeName::LoggingLevel => Some(Self::LoggingLevel),
            TypeName::CacheVisibility => Some(Self::CacheVisibility),
            TypeName::SoapType => Some(Self::SoapType),
            TypeName::DisplayType => Some(Self::DisplayType),
            _ => None,
        }
    }

    pub fn ty(self) -> TypeName {
        match self {
            Self::ParentJobResult => TypeName::ParentJobResult,
            Self::Quiddity => TypeName::Quiddity,
            Self::TriggerOperation => TypeName::TriggerOperation,
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
            Self::TriggerOperation => {
                TriggerOperation::from_apex_name(name).map(PlatformEnum::TriggerOperation)
            }
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
            Self::TriggerOperation(_) => TypeName::TriggerOperation,
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
            Self::TriggerOperation(value) => value.apex_name(),
            Self::LoggingLevel(value) => value.apex_name(),
            Self::CacheVisibility(value) => value.apex_name(),
            Self::SoapType(value) => value.apex_name(),
            Self::DisplayType(value) => value.apex_name(),
        }
    }

    pub fn ordinal(self) -> Option<i64> {
        match self {
            Self::TriggerOperation(value) => Some(value.ordinal()),
            Self::LoggingLevel(value) => Some(value.ordinal()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// Checked values for the Apex `System.TriggerOperation` platform enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TriggerOperation {
    BeforeInsert,
    AfterInsert,
    BeforeUpdate,
    AfterUpdate,
    BeforeDelete,
    AfterDelete,
    AfterUndelete,
}

impl TriggerOperation {
    pub const VALUES: [Self; 7] = [
        Self::BeforeInsert,
        Self::AfterInsert,
        Self::BeforeUpdate,
        Self::AfterUpdate,
        Self::BeforeDelete,
        Self::AfterDelete,
        Self::AfterUndelete,
    ];

    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "BEFORE_INSERT" => Some(Self::BeforeInsert),
            "AFTER_INSERT" => Some(Self::AfterInsert),
            "BEFORE_UPDATE" => Some(Self::BeforeUpdate),
            "AFTER_UPDATE" => Some(Self::AfterUpdate),
            "BEFORE_DELETE" => Some(Self::BeforeDelete),
            "AFTER_DELETE" => Some(Self::AfterDelete),
            "AFTER_UNDELETE" => Some(Self::AfterUndelete),
            _ => None,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::BeforeInsert => "BEFORE_INSERT",
            Self::AfterInsert => "AFTER_INSERT",
            Self::BeforeUpdate => "BEFORE_UPDATE",
            Self::AfterUpdate => "AFTER_UPDATE",
            Self::BeforeDelete => "BEFORE_DELETE",
            Self::AfterDelete => "AFTER_DELETE",
            Self::AfterUndelete => "AFTER_UNDELETE",
        }
    }

    pub fn ordinal(self) -> i64 {
        Self::VALUES
            .iter()
            .position(|value| *value == self)
            .expect("trigger operation belongs to its closed value set") as i64
    }
}
