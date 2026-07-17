#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    pub fn is_static(self) -> bool {
        matches!(
            self,
            Self::StaticString(_) | Self::Math(_) | Self::System(_)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StaticStringIntrinsic {
    ValueOf,
    Join,
    IsBlank,
    IsNotBlank,
    IsEmpty,
    IsNotEmpty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MathIntrinsic {
    Abs,
    Max,
    Min,
    Mod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SystemIntrinsic {
    Debug,
    Assert,
    AssertEquals,
    AssertNotEquals,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExceptionIntrinsic {
    GetMessage,
    GetTypeName,
    GetStackTraceString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
