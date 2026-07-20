use super::CacheVisibility;
use crate::ast::TypeName;

/// Checked values for the supported platform enums.
///
/// Keeping these values typed avoids reducing platform enums to rendered strings
/// at the compiler/runtime boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformEnum {
    ParentJobResult(ParentJobResult),
    Quiddity(Quiddity),
    CacheVisibility(CacheVisibility),
}

impl PlatformEnum {
    pub fn ty(self) -> TypeName {
        match self {
            Self::ParentJobResult(_) => TypeName::ParentJobResult,
            Self::Quiddity(_) => TypeName::Quiddity,
            Self::CacheVisibility(_) => TypeName::CacheVisibility,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::ParentJobResult(value) => value.apex_name(),
            Self::Quiddity(value) => value.apex_name(),
            Self::CacheVisibility(value) => value.apex_name(),
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
