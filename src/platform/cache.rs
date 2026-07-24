/// Checked values for the Apex `Cache.Visibility` platform enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CacheVisibility {
    All,
    Namespace,
}

impl CacheVisibility {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "ALL" => Some(Self::All),
            "NAMESPACE" => Some(Self::Namespace),
            _ => None,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Namespace => "NAMESPACE",
        }
    }
}

impl From<CacheVisibility> for super::PlatformEnum {
    fn from(value: CacheVisibility) -> Self {
        Self::CacheVisibility(value)
    }
}
