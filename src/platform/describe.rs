/// Checked values for the Apex `Schema.SoapType` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoapType {
    Base64Binary,
    Boolean,
    Byte,
    Date,
    Datetime,
    Double,
    Id,
    Int,
    Integer,
    Long,
    String,
    Time,
}

impl SoapType {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "BASE64BINARY" => Some(Self::Base64Binary),
            "BOOLEAN" => Some(Self::Boolean),
            "BYTE" => Some(Self::Byte),
            "DATE" => Some(Self::Date),
            "DATETIME" => Some(Self::Datetime),
            "DOUBLE" => Some(Self::Double),
            "ID" => Some(Self::Id),
            "INT" => Some(Self::Int),
            "INTEGER" => Some(Self::Integer),
            "LONG" => Some(Self::Long),
            "STRING" => Some(Self::String),
            "TIME" => Some(Self::Time),
            _ => None,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::Base64Binary => "BASE64BINARY",
            Self::Boolean => "BOOLEAN",
            Self::Byte => "BYTE",
            Self::Date => "DATE",
            Self::Datetime => "DATETIME",
            Self::Double => "DOUBLE",
            Self::Id => "ID",
            Self::Int => "INT",
            Self::Integer => "INTEGER",
            Self::Long => "LONG",
            Self::String => "STRING",
            Self::Time => "TIME",
        }
    }
}

/// Checked values for the Apex `Schema.DisplayType` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayType {
    Address,
    AnyType,
    Base64,
    Boolean,
    Combobox,
    ComplexValue,
    Currency,
    DataCategoryGroupReference,
    Date,
    Datetime,
    Double,
    Email,
    EncryptedString,
    Id,
    Integer,
    Location,
    Long,
    MultiPicklist,
    Percent,
    Phone,
    Picklist,
    Reference,
    String,
    TextArea,
    Time,
    Url,
}

impl DisplayType {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "ADDRESS" => Some(Self::Address),
            "ANYTYPE" => Some(Self::AnyType),
            "BASE64" => Some(Self::Base64),
            "BOOLEAN" => Some(Self::Boolean),
            "COMBOBOX" => Some(Self::Combobox),
            "COMPLEXVALUE" => Some(Self::ComplexValue),
            "CURRENCY" => Some(Self::Currency),
            "DATACATEGORYGROUPREFERENCE" => Some(Self::DataCategoryGroupReference),
            "DATE" => Some(Self::Date),
            "DATETIME" => Some(Self::Datetime),
            "DOUBLE" => Some(Self::Double),
            "EMAIL" => Some(Self::Email),
            "ENCRYPTEDSTRING" => Some(Self::EncryptedString),
            "ID" => Some(Self::Id),
            "INTEGER" => Some(Self::Integer),
            "LOCATION" => Some(Self::Location),
            "LONG" => Some(Self::Long),
            "MULTIPICKLIST" => Some(Self::MultiPicklist),
            "PERCENT" => Some(Self::Percent),
            "PHONE" => Some(Self::Phone),
            "PICKLIST" => Some(Self::Picklist),
            "REFERENCE" => Some(Self::Reference),
            "STRING" => Some(Self::String),
            "TEXTAREA" => Some(Self::TextArea),
            "TIME" => Some(Self::Time),
            "URL" => Some(Self::Url),
            _ => None,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::Address => "ADDRESS",
            Self::AnyType => "ANYTYPE",
            Self::Base64 => "BASE64",
            Self::Boolean => "BOOLEAN",
            Self::Combobox => "COMBOBOX",
            Self::ComplexValue => "COMPLEXVALUE",
            Self::Currency => "CURRENCY",
            Self::DataCategoryGroupReference => "DATACATEGORYGROUPREFERENCE",
            Self::Date => "DATE",
            Self::Datetime => "DATETIME",
            Self::Double => "DOUBLE",
            Self::Email => "EMAIL",
            Self::EncryptedString => "ENCRYPTEDSTRING",
            Self::Id => "ID",
            Self::Integer => "INTEGER",
            Self::Location => "LOCATION",
            Self::Long => "LONG",
            Self::MultiPicklist => "MULTIPICKLIST",
            Self::Percent => "PERCENT",
            Self::Phone => "PHONE",
            Self::Picklist => "PICKLIST",
            Self::Reference => "REFERENCE",
            Self::String => "STRING",
            Self::TextArea => "TEXTAREA",
            Self::Time => "TIME",
            Self::Url => "URL",
        }
    }
}
