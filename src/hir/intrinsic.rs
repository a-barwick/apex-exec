/// Checker-selected target for one supported built-in call.
///
/// Semantic analysis records this ID in HIR after validating the receiver,
/// overload, and argument types. Runtime execution matches the ID directly and
/// never repeats case-insensitive method lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IntrinsicId {
    StaticString(StaticStringIntrinsic),
    Math(MathIntrinsic),
    System(SystemIntrinsic),
    String(StringIntrinsic),
    Exception(ExceptionIntrinsic),
    List(ListIntrinsic),
    Set(SetIntrinsic),
    Map(MapIntrinsic),
}

impl IntrinsicId {
    /// Whether the call has a static type receiver rather than a runtime value.
    pub fn is_static(self) -> bool {
        matches!(
            self,
            Self::StaticString(_) | Self::Math(_) | Self::System(_)
        )
    }
}

/// Supported static methods on the Apex `String` type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StaticStringIntrinsic {
    ValueOf,
    Join,
    IsBlank,
    IsNotBlank,
    IsEmpty,
    IsNotEmpty,
}

/// Supported static methods on the Apex `Math` type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MathIntrinsic {
    Abs,
    Max,
    Min,
    Mod,
}

/// Supported static methods on the Apex `System` type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SystemIntrinsic {
    Debug,
    Assert,
    AssertEquals,
    AssertNotEquals,
}

/// Supported methods on an Apex `String` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StringIntrinsic {
    Length,
    Contains,
    StartsWith,
    EndsWith,
    Equals,
    EqualsIgnoreCase,
    IndexOf,
    Substring,
    Trim,
    ToLowerCase,
    ToUpperCase,
    Replace,
}

/// Supported methods on a core Apex exception value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExceptionIntrinsic {
    GetMessage,
    GetTypeName,
    GetStackTraceString,
}

/// Supported methods on an Apex `List` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ListIntrinsic {
    Add,
    AddAll,
    Clear,
    Clone,
    Contains,
    Get,
    IndexOf,
    IsEmpty,
    Remove,
    Set,
    Size,
    Sort,
}

/// Supported methods on an Apex `Set` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SetIntrinsic {
    Add,
    AddAll,
    Clear,
    Clone,
    Contains,
    ContainsAll,
    IsEmpty,
    Remove,
    RemoveAll,
    RetainAll,
    Size,
}

/// Supported methods on an Apex `Map` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MapIntrinsic {
    Clear,
    Clone,
    ContainsKey,
    Get,
    IsEmpty,
    KeySet,
    Put,
    PutAll,
    Remove,
    Size,
    Values,
}
