/// Checked values for the Apex `System.LoggingLevel` platform enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoggingLevel {
    None,
    Error,
    Warn,
    Info,
    Debug,
    Fine,
    Finer,
    Finest,
}

impl LoggingLevel {
    pub const VALUES: [Self; 8] = [
        Self::None,
        Self::Error,
        Self::Warn,
        Self::Info,
        Self::Debug,
        Self::Fine,
        Self::Finer,
        Self::Finest,
    ];

    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "NONE" => Some(Self::None),
            "ERROR" => Some(Self::Error),
            "WARN" => Some(Self::Warn),
            "INFO" => Some(Self::Info),
            "DEBUG" => Some(Self::Debug),
            "FINE" => Some(Self::Fine),
            "FINER" => Some(Self::Finer),
            "FINEST" => Some(Self::Finest),
            _ => None,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Fine => "FINE",
            Self::Finer => "FINER",
            Self::Finest => "FINEST",
        }
    }

    pub fn ordinal(self) -> i64 {
        Self::VALUES
            .iter()
            .position(|value| *value == self)
            .expect("logging level belongs to its closed value set") as i64
    }
}
