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
    Platform(PlatformIntrinsic),
}

impl IntrinsicId {
    /// Whether the call has a static type receiver rather than a runtime value.
    pub fn is_static(self) -> bool {
        matches!(
            self,
            Self::StaticString(_) | Self::Math(_) | Self::System(_)
        ) || matches!(self, Self::Platform(intrinsic) if intrinsic.is_static())
    }

    /// Whether this intrinsic belongs to the curated platform surface whose
    /// behavior is currently modeled only by the current API profile family.
    pub const fn requires_curated_platform_profile(self) -> bool {
        match self {
            Self::Platform(_) => true,
            Self::Math(MathIntrinsic::Random) => true,
            Self::System(
                SystemIntrinsic::Now | SystemIntrinsic::Today | SystemIntrinsic::CurrentTimeMillis,
            ) => true,
            Self::StaticString(_)
            | Self::Math(_)
            | Self::System(_)
            | Self::String(_)
            | Self::Exception(_)
            | Self::List(_)
            | Self::Set(_)
            | Self::Map(_) => false,
        }
    }
}

/// Constructors for platform objects whose state is owned by the execution
/// store rather than by user-class fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlatformConstructor {
    Http,
    HttpRequest,
    HttpResponse,
    DmlOptions,
    VisualEditorDataRow,
    VisualEditorDynamicPickListRows,
}

/// Curated M10 platform calls. This remains a closed checker-selected set so
/// unsupported APIs cannot fall through to name-based runtime behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlatformIntrinsic {
    DateNewInstance,
    DateValueOf,
    DateToday,
    DateAddDays,
    DateAddMonths,
    DateAddYears,
    DateDaysBetween,
    DateFormat,
    DateYear,
    DateMonth,
    DateDay,
    DatetimeNewInstance,
    DatetimeNow,
    DatetimeValueOf,
    DatetimeValueOfGmt,
    DatetimeGetTime,
    DatetimeDate,
    DatetimeDateGmt,
    DatetimeTime,
    DatetimeTimeGmt,
    DatetimeAddDays,
    DatetimeAddHours,
    DatetimeAddMinutes,
    DatetimeAddSeconds,
    DatetimeFormat,
    TimeNewInstance,
    TimeValueOf,
    TimeAddHours,
    TimeAddMinutes,
    TimeAddSeconds,
    TimeAddMilliseconds,
    TimeHour,
    TimeMinute,
    TimeSecond,
    TimeMillisecond,
    TimeFormat,
    DecimalValueOf,
    DecimalSetScale,
    DecimalAbs,
    DecimalScale,
    DoubleValueOf,
    IdValueOf,
    IdTo15,
    IdTo18,
    BlobValueOf,
    BlobToString,
    BlobSize,
    ObjectToString,
    JsonSerialize,
    JsonSerializePretty,
    JsonDeserialize,
    JsonDeserializeUntyped,
    PatternCompile,
    PatternQuote,
    PatternMatcher,
    MatcherMatches,
    MatcherFind,
    MatcherGroup,
    MatcherStart,
    MatcherEnd,
    SchemaGetGlobalDescribe,
    SObjectTypeGetDescribe,
    SObjectTypeGetName,
    SObjectTypeNewSObject,
    SObjectGetSObjectType,
    DescribeGetName,
    DescribeGetLocalName,
    DescribeGetLabel,
    DescribeGetLabelPlural,
    DescribeGetKeyPrefix,
    DescribeIsCustom,
    DescribeIsCustomSetting,
    DescribeIsAccessible,
    DescribeIsDeletable,
    DescribeIsUpdateable,
    SObjectFieldGetDescribe,
    SObjectFieldMapGetMap,
    FieldSetMapGetMap,
    DescribeFieldGetName,
    DescribeFieldGetLocalName,
    DescribeFieldGetLabel,
    DescribeFieldGetLength,
    DescribeFieldGetInlineHelpText,
    DescribeFieldGetRelationshipName,
    DescribeFieldGetSoapType,
    DescribeFieldGetType,
    DescribeFieldGetReferenceTo,
    DescribeFieldGetPicklistValues,
    DescribeFieldIsNameField,
    DescribeFieldIsSortable,
    DescribeFieldIsAccessible,
    FieldSetGetName,
    FieldSetGetLabel,
    FieldSetGetNamespace,
    FieldSetGetFields,
    FieldSetMemberGetFieldPath,
    FieldSetMemberGetLabel,
    FieldSetMemberGetSObjectField,
    PicklistEntryGetValue,
    TestStartTest,
    TestStopTest,
    TestIsRunningTest,
    TestSetMock,
    SystemEnqueueJob,
    SystemSchedule,
    SystemIsFuture,
    SystemIsQueueable,
    SystemIsBatch,
    SystemIsScheduled,
    DatabaseExecuteBatch,
    EventBusPublish,
    AsyncContextGetJobId,
    BatchableContextGetChildJobId,
    FinalizerContextGetAsyncApexJobId,
    FinalizerContextGetException,
    FinalizerContextGetResult,
    FinalizerContextGetRequestId,
    SchedulableContextGetTriggerId,
    RequestGetCurrent,
    RequestGetRequestId,
    RequestGetQuiddity,
    PlatformEnumName,
    PlatformEnumOrdinal,
    LoggingLevelValues,
    LoggingLevelValueOf,
    CacheGetPartition,
    CachePartitionContains,
    CachePartitionGet,
    CachePartitionIsAvailable,
    CachePartitionPut,
    CachePartitionRemove,
    CallableCall,
    TypeForName,
    TypeGetName,
    TypeNewInstance,
    LimitsGetQueries,
    LimitsGetLimitQueries,
    LimitsGetDmlStatements,
    LimitsGetLimitDmlStatements,
    LimitsGetCallouts,
    LimitsGetLimitCallouts,
    UserInfoGetUserId,
    UserInfoGetUserName,
    UserInfoGetProfileId,
    EncodingBase64Encode,
    EncodingBase64Decode,
    SecurityStripInaccessible,
    HttpRequestSetEndpoint,
    HttpRequestGetEndpoint,
    HttpRequestSetMethod,
    HttpRequestGetMethod,
    HttpRequestSetBody,
    HttpRequestGetBody,
    HttpRequestSetHeader,
    HttpRequestGetHeader,
    HttpRequestSetTimeout,
    HttpRequestGetTimeout,
    HttpResponseSetStatusCode,
    HttpResponseGetStatusCode,
    HttpResponseSetBody,
    HttpResponseGetBody,
    HttpResponseSetHeader,
    HttpResponseGetHeader,
    HttpResponseSetStatus,
    HttpResponseGetStatus,
    HttpSend,
    HttpCalloutMockRespond,
    VisualEditorDataRowGetLabel,
    VisualEditorDataRowGetValue,
    VisualEditorRowsAddRow,
    VisualEditorRowsGetDataRows,
}

impl PlatformIntrinsic {
    pub fn is_static(self) -> bool {
        matches!(
            self,
            Self::DateNewInstance
                | Self::DateValueOf
                | Self::DateToday
                | Self::DatetimeNewInstance
                | Self::DatetimeNow
                | Self::DatetimeValueOf
                | Self::DatetimeValueOfGmt
                | Self::TimeNewInstance
                | Self::TimeValueOf
                | Self::DecimalValueOf
                | Self::DoubleValueOf
                | Self::IdValueOf
                | Self::BlobValueOf
                | Self::JsonSerialize
                | Self::JsonSerializePretty
                | Self::JsonDeserialize
                | Self::JsonDeserializeUntyped
                | Self::PatternCompile
                | Self::PatternQuote
                | Self::SchemaGetGlobalDescribe
                | Self::TestStartTest
                | Self::TestStopTest
                | Self::TestIsRunningTest
                | Self::TestSetMock
                | Self::SystemEnqueueJob
                | Self::SystemSchedule
                | Self::SystemIsFuture
                | Self::SystemIsQueueable
                | Self::SystemIsBatch
                | Self::SystemIsScheduled
                | Self::DatabaseExecuteBatch
                | Self::EventBusPublish
                | Self::RequestGetCurrent
                | Self::CacheGetPartition
                | Self::TypeForName
                | Self::LoggingLevelValues
                | Self::LoggingLevelValueOf
                | Self::LimitsGetQueries
                | Self::LimitsGetLimitQueries
                | Self::LimitsGetDmlStatements
                | Self::LimitsGetLimitDmlStatements
                | Self::LimitsGetCallouts
                | Self::LimitsGetLimitCallouts
                | Self::UserInfoGetUserId
                | Self::UserInfoGetUserName
                | Self::UserInfoGetProfileId
                | Self::EncodingBase64Encode
                | Self::EncodingBase64Decode
                | Self::SecurityStripInaccessible
        )
    }
}

/// Supported static methods on the Apex `String` type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StaticStringIntrinsic {
    ValueOf,
    Join,
    Format,
    EscapeSingleQuotes,
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
    Random,
}

/// Supported static methods on the Apex `System` type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SystemIntrinsic {
    Debug,
    Assert,
    AssertEquals,
    AssertNotEquals,
    Now,
    Today,
    CurrentTimeMillis,
}

/// Supported methods on an Apex `String` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StringIntrinsic {
    Length,
    Contains,
    ContainsIgnoreCase,
    StartsWith,
    EndsWith,
    Equals,
    EqualsIgnoreCase,
    IndexOf,
    Substring,
    SubstringBefore,
    SubstringAfter,
    SubstringAfterLast,
    SubstringBetween,
    Left,
    Split,
    Trim,
    ToLowerCase,
    ToUpperCase,
    Replace,
    ReplaceAll,
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
    DeepClone,
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
    DeepClone,
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
