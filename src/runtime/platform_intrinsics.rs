use super::{
    ApexDouble, Collection, CollectionId, EvaluatedArgument, Interpreter, PlatformHost,
    PlatformValue, SObjectId, SObjectInstance, Value, apex_field_type, invalid_runtime_operands,
    runtime_exception,
    value_graph::{CycleBehavior, GraphIdentity, TraversalError, ValueGraphTraversal},
};
use crate::{
    ast::TypeName,
    diagnostic::Diagnostic,
    hir::{LimitIntrinsic, PlatformIntrinsic},
    platform::{LoggingLevel, ObjectSchema, RecordId},
    span::Span,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{
    DateTime, Datelike, Duration, Months, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike,
    Utc,
};
use regex::Regex;
use rust_decimal::Decimal;
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::{collections::BTreeMap, str::FromStr};

const STRIP_INACCESSIBLE_DEPTH_LIMIT: usize = 32;
const STRIP_INACCESSIBLE_NODE_LIMIT: usize = 10_000;
const TYPED_JSON_DEPTH_LIMIT: usize = 64;
const TYPED_JSON_NODE_LIMIT: usize = 4_096;

#[derive(Default)]
struct TypedJsonState {
    nodes: usize,
}

impl TypedJsonState {
    fn visit(&mut self, depth: usize, span: Span) -> Result<(), Diagnostic> {
        self.nodes = self.nodes.saturating_add(1);
        if depth > TYPED_JSON_DEPTH_LIMIT || self.nodes > TYPED_JSON_NODE_LIMIT {
            return Err(platform_error(
                "typed JSON exceeds the bounded conversion limits",
                span,
            ));
        }
        Ok(())
    }
}

struct StripInputs {
    access_type: crate::platform::AccessType,
    elements: Vec<Value>,
    enforce_root_object_crud: bool,
    access_span: Span,
    records_span: Span,
}

struct StripState {
    access_type: crate::platform::AccessType,
    user_id: String,
    removed_fields: BTreeMap<String, Vec<String>>,
    memo: BTreeMap<SObjectId, SObjectId>,
    nodes: usize,
    span: Span,
}

use super::intrinsics::{
    expect_integer, expect_no_arguments, expect_string, invalid_call_arguments,
};

impl<'program, H: PlatformHost> Interpreter<'program, H> {
    pub(super) fn call_platform(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        if is_schema_intrinsic(intrinsic) {
            return self.call_schema_intrinsic(intrinsic, receiver, arguments, span);
        }
        if matches!(
            intrinsic,
            P::JsonSerialize
                | P::JsonSerializePretty
                | P::JsonDeserialize
                | P::JsonDeserializeUntyped
        ) {
            return self.call_json_intrinsic(intrinsic, arguments, span);
        }
        if matches!(
            intrinsic,
            P::LoggingLevelValues | P::LoggingLevelValueOf | P::PlatformEnumOrdinal
        ) {
            return self.call_logging_level(intrinsic, receiver, arguments, span);
        }
        if let P::Limits(limit) = intrinsic {
            return self.call_limit(limit, arguments, span);
        }
        if matches!(
            intrinsic,
            P::NetworkGetNetworkId
                | P::NetworkGetLoginUrl
                | P::NetworkGetLogoutUrl
                | P::NetworkGetSelfRegUrl
        ) {
            return self.call_network(intrinsic, arguments, span);
        }
        match intrinsic {
            P::DateNewInstance
            | P::DateValueOf
            | P::DateToday
            | P::DateAddDays
            | P::DateAddMonths
            | P::DateAddYears
            | P::DateDaysBetween
            | P::DateFormat
            | P::DateYear
            | P::DateMonth
            | P::DateDay => self.call_date_intrinsic(intrinsic, receiver, arguments, span),
            P::DatetimeNewInstance
            | P::DatetimeNow
            | P::DatetimeValueOf
            | P::DatetimeValueOfGmt
            | P::DatetimeGetTime
            | P::DatetimeDate
            | P::DatetimeDateGmt
            | P::DatetimeTime
            | P::DatetimeTimeGmt
            | P::DatetimeAddDays
            | P::DatetimeAddHours
            | P::DatetimeAddMinutes
            | P::DatetimeAddSeconds
            | P::DatetimeFormat => {
                self.call_datetime_intrinsic(intrinsic, receiver, arguments, span)
            }
            P::TimeNewInstance
            | P::TimeValueOf
            | P::TimeAddHours
            | P::TimeAddMinutes
            | P::TimeAddSeconds
            | P::TimeAddMilliseconds
            | P::TimeHour
            | P::TimeMinute
            | P::TimeSecond
            | P::TimeMillisecond
            | P::TimeFormat => self.call_time_intrinsic(intrinsic, receiver, arguments, span),
            P::DecimalValueOf
            | P::DoubleValueOf
            | P::LongValueOf
            | P::DecimalSetScale
            | P::DecimalAbs
            | P::DecimalScale => self.call_numeric_intrinsic(intrinsic, receiver, arguments, span),
            P::IdValueOf => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value = expect_string(&value.value, value.span)?;
                validate_id(value, span).map(Value::Id)
            }
            P::IdTo15 | P::IdTo18 => {
                expect_no_arguments(arguments, span)?;
                let id = expect_id(receiver, span)?;
                Ok(Value::String(if intrinsic == P::IdTo15 {
                    id[..15].to_owned()
                } else {
                    id_to_18(&id)
                }))
            }
            P::IdGetSObjectType => self.call_id_get_sobject_type(receiver, arguments, span),
            P::BlobValueOf => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value = expect_string(&value.value, value.span)?;
                Ok(self
                    .store
                    .allocate_platform(PlatformValue::Blob(value.as_bytes().to_vec())))
            }
            P::BlobToString => {
                expect_no_arguments(arguments, span)?;
                let bytes = self.expect_blob(receiver, span)?;
                String::from_utf8(bytes)
                    .map(Value::String)
                    .map_err(|_| platform_error("Blob does not contain valid UTF-8", span))
            }
            P::BlobSize => {
                expect_no_arguments(arguments, span)?;
                let bytes = self.expect_blob(receiver, span)?;
                Ok(Value::Integer(
                    i64::try_from(bytes.len())
                        .map_err(|_| platform_error("Blob is too large", span))?,
                ))
            }
            P::ObjectToString => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::String(self.stringify_value(
                    &receiver.ok_or_else(|| invalid_runtime_operands(span))?,
                )))
            }
            P::JsonSerialize
            | P::JsonSerializePretty
            | P::JsonDeserialize
            | P::JsonDeserializeUntyped => {
                unreachable!("JSON intrinsics are dispatched before the platform match")
            }
            P::PatternCompile => {
                let [pattern] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let pattern = expect_string(&pattern.value, pattern.span)?;
                Regex::new(pattern)
                    .map_err(|error| platform_error(format!("invalid regex: {error}"), span))?;
                Ok(self
                    .store
                    .allocate_platform(PlatformValue::Pattern(pattern.to_owned())))
            }
            P::PatternQuote => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                Ok(Value::String(regex::escape(expect_string(
                    &value.value,
                    value.span,
                )?)))
            }
            P::PatternMatcher => {
                let pattern = self.expect_pattern(receiver, span)?;
                let [input] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let input = expect_string(&input.value, input.span)?;
                Ok(self.store.allocate_platform(PlatformValue::Matcher {
                    pattern,
                    input: input.to_owned(),
                    next_start: 0,
                    captures: Vec::new(),
                }))
            }
            P::MatcherMatches => {
                expect_no_arguments(arguments, span)?;
                self.matcher_matches(receiver, span)
            }
            P::MatcherFind => {
                expect_no_arguments(arguments, span)?;
                self.matcher_find(receiver, span)
            }
            P::MatcherGroup | P::MatcherStart | P::MatcherEnd => {
                self.matcher_capture(receiver, intrinsic, arguments, span)
            }
            P::SObjectGetSObjectType => {
                expect_no_arguments(arguments, span)?;
                let Some(Value::SObject(id)) = receiver else {
                    return Err(invalid_runtime_operands(span));
                };
                let object_id = self.store.sobject(id).object_id;
                Ok(self
                    .store
                    .allocate_platform(PlatformValue::SObjectType(object_id)))
            }
            P::TestStartTest | P::TestStopTest | P::TestIsRunningTest => {
                self.call_test_context(intrinsic, arguments, span)
            }
            P::TestSetMock => self.set_test_mock(arguments, span),
            P::SystemEnqueueJob => {
                let [job] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.enqueue_queueable(job.value.clone(), span)
                    .map(Value::Id)
            }
            P::SystemSchedule => {
                let [name, cron, job] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let name_span = name.span;
                let cron_span = cron.span;
                let name = expect_string(&name.value, name.span)?;
                let cron = expect_string(&cron.value, cron.span)?;
                if name.trim().is_empty() {
                    return Err(platform_error(
                        "scheduled job name cannot be blank",
                        name_span,
                    ));
                }
                if cron.split_whitespace().count() != 7 {
                    return Err(platform_error(
                        "scheduled job cron expression must contain 7 fields",
                        cron_span,
                    ));
                }
                self.enqueue_scheduled(job.value.clone(), span)
                    .map(Value::Id)
            }
            P::SystemIsFuture | P::SystemIsQueueable | P::SystemIsBatch | P::SystemIsScheduled => {
                expect_no_arguments(arguments, span)?;
                let expected = match intrinsic {
                    P::SystemIsFuture => super::AsyncJobKind::Future,
                    P::SystemIsQueueable => super::AsyncJobKind::Queueable,
                    P::SystemIsBatch => super::AsyncJobKind::Batch,
                    P::SystemIsScheduled => super::AsyncJobKind::Scheduled,
                    _ => unreachable!(),
                };
                Ok(Value::Boolean(self.current_async_kind() == Some(expected)))
            }
            P::DatabaseExecuteBatch => {
                let ([job] | [job, _]) = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let scope_size = arguments.get(1).map_or_else(
                    || Ok(super::asynchronous::default_batch_scope_size()),
                    |scope| expect_integer(&scope.value, scope.span),
                )?;
                self.enqueue_batch(job.value.clone(), scope_size, span)
                    .map(Value::Id)
            }
            P::EventBusPublish => {
                let [events] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.enqueue_platform_events(events.value.clone(), span)?;
                Ok(Value::Void)
            }
            P::RequestGetCurrent => {
                expect_no_arguments(arguments, span)?;
                let request_id = self.current_request_id();
                let quiddity = self.current_request_quiddity();
                Ok(self.store.allocate_platform(PlatformValue::Request {
                    request_id,
                    quiddity,
                }))
            }
            P::CacheGetPartition => {
                let [name] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                expect_string(&name.value, name.span)?;
                Ok(self.store.allocate_platform(PlatformValue::CachePartition))
            }
            P::TypeForName => {
                let [name] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let name = expect_string(&name.value, name.span)?;
                Ok(self
                    .resolve_type_for_name(name, span)
                    .map_or_else(|| Value::Null(Some(TypeName::Type)), Value::TypeLiteral))
            }
            P::LoggingLevelValues | P::LoggingLevelValueOf | P::PlatformEnumOrdinal => {
                unreachable!("LoggingLevel intrinsics are dispatched before the platform match")
            }
            P::AsyncContextGetJobId
            | P::BatchableContextGetChildJobId
            | P::FinalizerContextGetAsyncApexJobId
            | P::FinalizerContextGetException
            | P::FinalizerContextGetResult
            | P::FinalizerContextGetRequestId
            | P::SchedulableContextGetTriggerId => {
                expect_no_arguments(arguments, span)?;
                self.call_async_context_method(receiver, intrinsic, span)
            }
            P::RequestGetRequestId | P::RequestGetQuiddity => {
                expect_no_arguments(arguments, span)?;
                let Some(Value::Platform(id)) = receiver else {
                    return Err(invalid_runtime_operands(span));
                };
                let PlatformValue::Request {
                    request_id,
                    quiddity,
                } = self.store.platform(id)
                else {
                    return Err(invalid_runtime_operands(span));
                };
                Ok(match intrinsic {
                    P::RequestGetRequestId => Value::String(request_id.clone()),
                    P::RequestGetQuiddity => {
                        self.store.allocate_platform(PlatformValue::PlatformEnum(
                            crate::platform::PlatformEnum::Quiddity(*quiddity),
                        ))
                    }
                    _ => unreachable!("only Request accessors use this branch"),
                })
            }
            P::PlatformEnumName => {
                expect_no_arguments(arguments, span)?;
                let Some(Value::Platform(id)) = receiver else {
                    return Err(invalid_runtime_operands(span));
                };
                let PlatformValue::PlatformEnum(value) = self.store.platform(id) else {
                    return Err(invalid_runtime_operands(span));
                };
                Ok(Value::String(value.apex_name().to_owned()))
            }
            P::CachePartitionContains => {
                self.require_cache_partition_receiver(receiver, span)?;
                let [key] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                expect_string(&key.value, key.span)?;
                Ok(Value::Boolean(false))
            }
            P::CachePartitionGet => {
                self.require_cache_partition_receiver(receiver, span)?;
                let [key] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                expect_string(&key.value, key.span)?;
                Ok(Value::Null(Some(TypeName::Object)))
            }
            P::CachePartitionIsAvailable => {
                self.require_cache_partition_receiver(receiver, span)?;
                expect_no_arguments(arguments, span)?;
                Ok(Value::Boolean(false))
            }
            P::CachePartitionPut => {
                self.require_cache_partition_receiver(receiver, span)?;
                if !(2..=5).contains(&arguments.len()) {
                    return Err(invalid_call_arguments(span));
                }
                Ok(Value::Void)
            }
            P::CachePartitionRemove => {
                self.require_cache_partition_receiver(receiver, span)?;
                let [key] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                expect_string(&key.value, key.span)?;
                Ok(Value::Void)
            }
            P::CallableCall => self.call_callable(receiver, arguments, span),
            P::VisualEditorDataRowGetLabel | P::VisualEditorDataRowGetValue => {
                expect_no_arguments(arguments, span)?;
                let id = platform_id(receiver, span)?;
                let PlatformValue::VisualEditorDataRow { label, value } = self.store.platform(id)
                else {
                    return Err(invalid_runtime_operands(span));
                };
                Ok(Value::String(
                    if intrinsic == P::VisualEditorDataRowGetLabel {
                        label.clone()
                    } else {
                        value.clone()
                    },
                ))
            }
            P::VisualEditorRowsAddRow => {
                let rows = platform_id(receiver, span)?;
                let [row] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let Value::Platform(row) = row.value else {
                    return Err(invalid_runtime_operands(row.span));
                };
                if !matches!(
                    self.store.platform(row),
                    PlatformValue::VisualEditorDataRow { .. }
                ) {
                    return Err(invalid_runtime_operands(span));
                }
                let PlatformValue::VisualEditorDynamicPickListRows(values) =
                    self.store.platform_mut(rows)
                else {
                    return Err(invalid_runtime_operands(span));
                };
                values.push(row);
                Ok(Value::Void)
            }
            P::VisualEditorRowsGetDataRows => {
                expect_no_arguments(arguments, span)?;
                let rows = platform_id(receiver, span)?;
                let PlatformValue::VisualEditorDynamicPickListRows(values) =
                    self.store.platform(rows)
                else {
                    return Err(invalid_runtime_operands(span));
                };
                let elements = values.iter().copied().map(Value::Platform).collect();
                Ok(self.allocate(Collection::List {
                    element_type: TypeName::VisualEditorDataRow,
                    elements,
                    iteration_depth: 0,
                }))
            }
            P::TypeGetName => {
                expect_no_arguments(arguments, span)?;
                let Some(Value::TypeLiteral(ty)) = receiver else {
                    return Err(invalid_runtime_operands(span));
                };
                Ok(Value::String(ty.apex_name()))
            }
            P::TypeNewInstance => {
                expect_no_arguments(arguments, span)?;
                self.instantiate_type(receiver, span)
            }
            P::SchemaGetGlobalDescribe
            | P::SObjectTypeGetDescribe
            | P::SObjectTypeGetName
            | P::SObjectTypeNewSObject
            | P::DescribeGetName
            | P::DescribeGetLocalName
            | P::DescribeGetLabel
            | P::DescribeGetLabelPlural
            | P::DescribeGetKeyPrefix
            | P::DescribeIsCustom
            | P::DescribeIsCustomSetting
            | P::DescribeIsAccessible
            | P::DescribeIsDeletable
            | P::DescribeIsUpdateable
            | P::SObjectFieldGetDescribe
            | P::SObjectFieldMapGetMap
            | P::FieldSetMapGetMap
            | P::DescribeFieldGetName
            | P::DescribeFieldGetLocalName
            | P::DescribeFieldGetLabel
            | P::DescribeFieldGetLength
            | P::DescribeFieldGetInlineHelpText
            | P::DescribeFieldGetRelationshipName
            | P::DescribeFieldGetSoapType
            | P::DescribeFieldGetType
            | P::DescribeFieldGetReferenceTo
            | P::DescribeFieldGetPicklistValues
            | P::DescribeFieldIsNameField
            | P::DescribeFieldIsSortable
            | P::DescribeFieldIsAccessible
            | P::FieldSetGetName
            | P::FieldSetGetLabel
            | P::FieldSetGetNamespace
            | P::FieldSetGetFields
            | P::FieldSetMemberGetFieldPath
            | P::FieldSetMemberGetLabel
            | P::FieldSetMemberGetSObjectField
            | P::PicklistEntryGetValue => {
                unreachable!("schema intrinsics are dispatched before the platform match")
            }
            P::Limits(_)
            | P::NetworkGetNetworkId
            | P::NetworkGetLoginUrl
            | P::NetworkGetLogoutUrl
            | P::NetworkGetSelfRegUrl => {
                unreachable!(
                    "limits and network intrinsics are dispatched before the platform match"
                )
            }
            P::UserInfoGetUserId | P::UserInfoGetUserName | P::UserInfoGetProfileId => {
                expect_no_arguments(arguments, span)?;
                let user = self.current_user_context();
                Ok(match intrinsic {
                    P::UserInfoGetUserId => Value::Id(validate_id(&user.user_id, span)?),
                    P::UserInfoGetUserName => Value::String(user.username),
                    P::UserInfoGetProfileId => Value::Id("00e000000000001AAA".to_owned()),
                    _ => unreachable!(),
                })
            }
            P::EncodingBase64Encode => {
                let [blob] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let bytes = self.expect_blob(Some(blob.value.clone()), blob.span)?;
                Ok(Value::String(BASE64.encode(bytes)))
            }
            P::EncodingBase64Decode => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value = expect_string(&value.value, value.span)?;
                let bytes = BASE64
                    .decode(value)
                    .map_err(|error| platform_error(format!("invalid base64: {error}"), span))?;
                Ok(self.store.allocate_platform(PlatformValue::Blob(bytes)))
            }
            P::SecurityStripInaccessible => self.strip_inaccessible(arguments, span),
            P::HttpRequestSetEndpoint
            | P::HttpRequestSetMethod
            | P::HttpRequestSetBody
            | P::HttpRequestSetTimeout
            | P::HttpRequestSetHeader => {
                self.mutate_http_request(receiver, intrinsic, arguments, span)
            }
            P::HttpRequestGetEndpoint
            | P::HttpRequestGetMethod
            | P::HttpRequestGetBody
            | P::HttpRequestGetTimeout
            | P::HttpRequestGetHeader => {
                self.read_http_request(receiver, intrinsic, arguments, span)
            }
            P::HttpResponseSetStatusCode
            | P::HttpResponseSetBody
            | P::HttpResponseSetHeader
            | P::HttpResponseSetStatus => {
                self.mutate_http_response(receiver, intrinsic, arguments, span)
            }
            P::HttpResponseGetStatusCode
            | P::HttpResponseGetBody
            | P::HttpResponseGetHeader
            | P::HttpResponseGetStatus => {
                self.read_http_response(receiver, intrinsic, arguments, span)
            }
            P::HttpCalloutMockRespond => {
                let Some(Value::Object(receiver)) = receiver else {
                    return Err(invalid_runtime_operands(span));
                };
                let [request] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.call_http_callout_mock(receiver, request.clone(), span)
            }
            P::HttpSend => {
                let Some(Value::Platform(client_id)) = receiver else {
                    return Err(invalid_runtime_operands(span));
                };
                if !matches!(self.store.platform(client_id), PlatformValue::Http) {
                    return Err(invalid_runtime_operands(span));
                }
                if !self.current_async_allows_callouts() {
                    return Err(runtime_exception(
                        "CalloutException",
                        "asynchronous callouts require Database.AllowsCallouts",
                        request_span(arguments, span),
                    ));
                }
                let [request] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                if let Some(mock) = self.http_callout_mock {
                    return self.call_http_callout_mock(mock, request.clone(), span);
                }
                let Value::Platform(request_id) = request.value else {
                    return Err(invalid_runtime_operands(request.span));
                };
                let PlatformValue::HttpRequest(request) = self.store.platform(request_id) else {
                    return Err(invalid_runtime_operands(request.span));
                };
                let profile = self.execution_context.compatibility_profile();
                let response = self.host.send_http(request, profile).map_err(|message| {
                    runtime_exception("CalloutException", message, request_span(arguments, span))
                })?;
                Ok(self
                    .store
                    .allocate_platform(PlatformValue::HttpResponse(response)))
            }
        }
    }

    fn call_id_get_sobject_type(
        &mut self,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        expect_no_arguments(arguments, span)?;
        let id = match receiver {
            Some(Value::Id(value)) => value,
            Some(Value::Null(_)) => {
                return Err(runtime_exception(
                    "NullPointerException",
                    "attempt to de-reference a null value while calling `getSObjectType`",
                    span,
                ));
            }
            _ => return Err(invalid_runtime_operands(span)),
        };
        let id = RecordId::parse(id.clone()).map_err(|error| {
            runtime_exception(
                "SObjectException",
                format!("Id.getSObjectType requires a valid Id: {error}"),
                span,
            )
        })?;
        let key_prefix = &id.as_str()[..3];
        let Some(object_id) = self
            .program()
            .schema()
            .object_index_by_key_prefix(key_prefix)
        else {
            return Err(runtime_exception(
                "SObjectException",
                format!("Id.getSObjectType cannot resolve key prefix `{key_prefix}`"),
                span,
            ));
        };
        Ok(self
            .store
            .allocate_platform(PlatformValue::SObjectType(object_id)))
    }

    fn call_date_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DateNewInstance | P::DateValueOf | P::DateToday => {
                self.call_date_static_intrinsic(intrinsic, arguments, span)
            }
            P::DateAddDays
            | P::DateAddMonths
            | P::DateAddYears
            | P::DateDaysBetween
            | P::DateFormat
            | P::DateYear
            | P::DateMonth
            | P::DateDay => self.call_date_instance_intrinsic(intrinsic, receiver, arguments, span),
            _ => unreachable!("Date intrinsic dispatch is closed"),
        }
    }

    fn call_date_static_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match intrinsic {
            PlatformIntrinsic::DateNewInstance => {
                let [year, month, day] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let date = date_from_parts(
                    expect_integer(&year.value, year.span)?,
                    expect_integer(&month.value, month.span)?,
                    expect_integer(&day.value, day.span)?,
                    span,
                )?;
                Ok(Value::Date(date))
            }
            PlatformIntrinsic::DateValueOf => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value = expect_string(&value.value, value.span)?;
                parse_date(value, value_span(arguments, span)).map(Value::Date)
            }
            PlatformIntrinsic::DateToday => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Date(
                    datetime_from_millis(self.host.now_millis(), span)?.date_naive(),
                ))
            }
            _ => unreachable!("static Date intrinsic dispatch is closed"),
        }
    }

    fn call_date_instance_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DateAddDays | P::DateAddMonths | P::DateAddYears => {
                let date = expect_date(receiver, span)?;
                let [amount] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let amount = expect_integer(&amount.value, amount.span)?;
                let result = match intrinsic {
                    P::DateAddDays => date.checked_add_signed(Duration::days(amount)),
                    P::DateAddMonths => add_months(date, amount),
                    P::DateAddYears => add_months(
                        date,
                        amount.checked_mul(12).ok_or_else(|| {
                            platform_error("Date year adjustment is out of range", span)
                        })?,
                    ),
                    _ => unreachable!(),
                }
                .ok_or_else(|| platform_error("Date adjustment is out of range", span))?;
                Ok(Value::Date(result))
            }
            P::DateDaysBetween => {
                let date = expect_date(receiver, span)?;
                let [other] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let Value::Date(other) = other.value else {
                    return Err(invalid_runtime_operands(other.span));
                };
                Ok(Value::Integer((other - date).num_days()))
            }
            P::DateFormat => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::String(
                    expect_date(receiver, span)?.format("%Y-%m-%d").to_string(),
                ))
            }
            P::DateYear | P::DateMonth | P::DateDay => {
                expect_no_arguments(arguments, span)?;
                let date = expect_date(receiver, span)?;
                Ok(Value::Integer(match intrinsic {
                    P::DateYear => i64::from(date.year()),
                    P::DateMonth => i64::from(date.month()),
                    P::DateDay => i64::from(date.day()),
                    _ => unreachable!(),
                }))
            }
            _ => unreachable!("instance Date intrinsic dispatch is closed"),
        }
    }

    fn call_datetime_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DatetimeNewInstance
            | P::DatetimeNow
            | P::DatetimeValueOf
            | P::DatetimeValueOfGmt => {
                self.call_datetime_static_intrinsic(intrinsic, arguments, span)
            }
            P::DatetimeGetTime
            | P::DatetimeDate
            | P::DatetimeDateGmt
            | P::DatetimeTime
            | P::DatetimeTimeGmt
            | P::DatetimeAddDays
            | P::DatetimeAddHours
            | P::DatetimeAddMinutes
            | P::DatetimeAddSeconds
            | P::DatetimeFormat => {
                self.call_datetime_instance_intrinsic(intrinsic, receiver, arguments, span)
            }
            _ => unreachable!("Datetime intrinsic dispatch is closed"),
        }
    }

    fn call_datetime_static_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DatetimeNewInstance => {
                let [year, month, day, hour, minute, second] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let date = date_from_parts(
                    expect_integer(&year.value, year.span)?,
                    expect_integer(&month.value, month.span)?,
                    expect_integer(&day.value, day.span)?,
                    span,
                )?;
                let time = time_from_parts(
                    expect_integer(&hour.value, hour.span)?,
                    expect_integer(&minute.value, minute.span)?,
                    expect_integer(&second.value, second.span)?,
                    0,
                    span,
                )?;
                Ok(Value::Datetime(
                    Utc.from_utc_datetime(&NaiveDateTime::new(date, time)),
                ))
            }
            P::DatetimeNow => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Datetime(datetime_from_millis(
                    self.host.now_millis(),
                    span,
                )?))
            }
            P::DatetimeValueOf | P::DatetimeValueOfGmt => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let argument_span = value.span;
                match &value.value {
                    Value::String(value) => {
                        parse_datetime(value, argument_span).map(Value::Datetime)
                    }
                    Value::Long(value) if intrinsic == P::DatetimeValueOf => {
                        datetime_from_millis(*value, argument_span).map(Value::Datetime)
                    }
                    _ => Err(invalid_runtime_operands(argument_span)),
                }
            }
            _ => unreachable!("static Datetime intrinsic dispatch is closed"),
        }
    }

    fn call_datetime_instance_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DatetimeGetTime => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Long(
                    expect_datetime(receiver, span)?.timestamp_millis(),
                ))
            }
            P::DatetimeDate | P::DatetimeDateGmt => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Date(expect_datetime(receiver, span)?.date_naive()))
            }
            P::DatetimeTime | P::DatetimeTimeGmt => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Time(expect_datetime(receiver, span)?.time()))
            }
            P::DatetimeAddDays
            | P::DatetimeAddHours
            | P::DatetimeAddMinutes
            | P::DatetimeAddSeconds => {
                let datetime = expect_datetime(receiver, span)?;
                let [amount] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let amount = expect_integer(&amount.value, amount.span)?;
                let duration = match intrinsic {
                    P::DatetimeAddDays => Duration::days(amount),
                    P::DatetimeAddHours => Duration::hours(amount),
                    P::DatetimeAddMinutes => Duration::minutes(amount),
                    P::DatetimeAddSeconds => Duration::seconds(amount),
                    _ => unreachable!(),
                };
                Ok(Value::Datetime(
                    datetime.checked_add_signed(duration).ok_or_else(|| {
                        platform_error("Datetime adjustment is out of range", span)
                    })?,
                ))
            }
            P::DatetimeFormat => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::String(
                    expect_datetime(receiver, span)?
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string(),
                ))
            }
            _ => unreachable!("instance Datetime intrinsic dispatch is closed"),
        }
    }

    fn call_time_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::TimeNewInstance | P::TimeValueOf => {
                self.call_time_static_intrinsic(intrinsic, arguments, span)
            }
            P::TimeAddHours
            | P::TimeAddMinutes
            | P::TimeAddSeconds
            | P::TimeAddMilliseconds
            | P::TimeHour
            | P::TimeMinute
            | P::TimeSecond
            | P::TimeMillisecond
            | P::TimeFormat => {
                self.call_time_instance_intrinsic(intrinsic, receiver, arguments, span)
            }
            _ => unreachable!("Time intrinsic dispatch is closed"),
        }
    }

    fn call_time_static_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match intrinsic {
            PlatformIntrinsic::TimeNewInstance => {
                let [hour, minute, second, millisecond] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                Ok(Value::Time(time_from_parts(
                    expect_integer(&hour.value, hour.span)?,
                    expect_integer(&minute.value, minute.span)?,
                    expect_integer(&second.value, second.span)?,
                    expect_integer(&millisecond.value, millisecond.span)?,
                    span,
                )?))
            }
            PlatformIntrinsic::TimeValueOf => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value = expect_string(&value.value, value.span)?;
                NaiveTime::parse_from_str(value, "%H:%M:%S%.f")
                    .map(Value::Time)
                    .map_err(|_| platform_error(format!("invalid Time `{value}`"), span))
            }
            _ => unreachable!("static Time intrinsic dispatch is closed"),
        }
    }

    fn call_time_instance_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::TimeAddHours | P::TimeAddMinutes | P::TimeAddSeconds | P::TimeAddMilliseconds => {
                let time = expect_time(receiver, span)?;
                let [amount] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let amount = expect_integer(&amount.value, amount.span)?;
                let duration = match intrinsic {
                    P::TimeAddHours => Duration::hours(amount),
                    P::TimeAddMinutes => Duration::minutes(amount),
                    P::TimeAddSeconds => Duration::seconds(amount),
                    P::TimeAddMilliseconds => Duration::milliseconds(amount),
                    _ => unreachable!(),
                };
                Ok(Value::Time(time.overflowing_add_signed(duration).0))
            }
            P::TimeHour | P::TimeMinute | P::TimeSecond | P::TimeMillisecond => {
                expect_no_arguments(arguments, span)?;
                let time = expect_time(receiver, span)?;
                Ok(Value::Integer(match intrinsic {
                    P::TimeHour => i64::from(time.hour()),
                    P::TimeMinute => i64::from(time.minute()),
                    P::TimeSecond => i64::from(time.second()),
                    P::TimeMillisecond => i64::from(time.nanosecond() / 1_000_000),
                    _ => unreachable!(),
                }))
            }
            P::TimeFormat => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::String(
                    expect_time(receiver, span)?
                        .format("%H:%M:%S%.3f")
                        .to_string(),
                ))
            }
            _ => unreachable!("instance Time intrinsic dispatch is closed"),
        }
    }

    fn call_numeric_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DecimalValueOf | P::DoubleValueOf | P::LongValueOf => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value = expect_string(&value.value, value.span)?;
                match intrinsic {
                    P::DecimalValueOf => Decimal::from_str(value)
                        .map(Value::Decimal)
                        .map_err(|_| platform_error(format!("invalid Decimal `{value}`"), span)),
                    P::DoubleValueOf => value
                        .parse::<f64>()
                        .ok()
                        .and_then(ApexDouble::new)
                        .map(Value::Double)
                        .ok_or_else(|| platform_error(format!("invalid Double `{value}`"), span)),
                    P::LongValueOf => value
                        .parse::<i64>()
                        .map(Value::Long)
                        .map_err(|_| platform_error(format!("invalid Long `{value}`"), span)),
                    _ => unreachable!(),
                }
            }
            P::DecimalSetScale => {
                let mut decimal = expect_decimal(receiver, span)?;
                let [scale] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let scale = expect_integer(&scale.value, scale.span)?;
                let scale = u32::try_from(scale)
                    .ok()
                    .filter(|scale| *scale <= 28)
                    .ok_or_else(|| {
                        platform_error("Decimal scale must be between 0 and 28", span)
                    })?;
                decimal.rescale(scale);
                Ok(Value::Decimal(decimal))
            }
            P::DecimalAbs => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Decimal(expect_decimal(receiver, span)?.abs()))
            }
            P::DecimalScale => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Integer(i64::from(
                    expect_decimal(receiver, span)?.scale(),
                )))
            }
            _ => unreachable!("numeric intrinsic dispatch is closed"),
        }
    }

    fn call_json_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::JsonSerialize | P::JsonSerializePretty => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let json = self.value_to_json(&value.value, value.span)?;
                let serialized = if intrinsic == P::JsonSerializePretty {
                    serde_json::to_string_pretty(&json)
                } else {
                    serde_json::to_string(&json)
                }
                .map_err(|error| {
                    platform_error(format!("JSON serialization failed: {error}"), span)
                })?;
                Ok(Value::String(serialized))
            }
            P::JsonDeserialize => {
                let [source, target] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source_span = source.span;
                let source = expect_string(&source.value, source_span)?;
                let Value::TypeLiteral(target) = &target.value else {
                    return Err(invalid_call_arguments(target.span));
                };
                let json: JsonValue = serde_json::from_str(source).map_err(|error| {
                    platform_error(format!("invalid JSON: {error}"), source_span)
                })?;
                self.typed_json_to_value(json, target, span, 0, &mut TypedJsonState::default())
            }
            P::JsonDeserializeUntyped => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source = expect_string(&value.value, value.span)?;
                let json: JsonValue = serde_json::from_str(source).map_err(|error| {
                    platform_error(format!("invalid JSON: {error}"), value.span)
                })?;
                self.json_to_value(json, span)
            }
            _ => unreachable!("JSON intrinsic dispatch is closed"),
        }
    }

    fn call_limit(
        &mut self,
        intrinsic: LimitIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use LimitIntrinsic as L;
        expect_no_arguments(arguments, span)?;
        let usage = self.host.limit_usage();
        let value = match intrinsic {
            L::AggregateQueries => usage.aggregate_queries,
            L::ApexCursorFetchCalls => usage.apex_cursor_fetch_calls,
            L::ApexCursorRows => usage.apex_cursor_rows,
            L::AsyncCalls => usage.async_calls,
            L::Callouts => usage.callouts,
            L::CpuTime => usage.cpu_time_millis,
            L::DmlRows => usage.dml_rows,
            L::DmlStatements => usage.dml_statements,
            L::EmailInvocations => usage.email_invocations,
            L::FutureCalls => usage.future_calls,
            L::HeapSize => usage.heap_size_bytes,
            L::MobilePushApexCalls => usage.mobile_push_apex_calls,
            L::PublishImmediateDml => usage.publish_immediate_dml,
            L::Queries => usage.queries,
            L::QueryLocatorRows => usage.query_locator_rows,
            L::QueryRows => usage.query_rows,
            L::QueueableJobs => usage.queueable_jobs,
            L::SoslQueries => usage.sosl_queries,
            L::LimitAggregateQueries => 300,
            L::LimitApexCursorFetchCalls => 100,
            L::LimitApexCursorRows => 50_000_000,
            L::LimitAsyncCalls => 200,
            L::LimitCallouts => 100,
            L::LimitCpuTime => 10_000,
            L::LimitDmlRows => 10_000,
            L::LimitDmlStatements => 150,
            L::LimitEmailInvocations => 10,
            L::LimitFutureCalls => 50,
            L::LimitHeapSize => 6_000_000,
            L::LimitMobilePushApexCalls => 10,
            L::LimitPublishImmediateDml => 150,
            L::LimitQueries => 100,
            L::LimitQueryLocatorRows => 10_000,
            L::LimitQueryRows => 50_000,
            L::LimitQueueableJobs => 50,
            L::LimitSoslQueries => 20,
        };
        Ok(Value::Integer(value))
    }

    fn call_network(
        &mut self,
        intrinsic: PlatformIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        if intrinsic == P::NetworkGetNetworkId {
            expect_no_arguments(arguments, span)?;
            return self.host.network_context().map_or_else(
                || Ok(Value::Null(Some(TypeName::Id))),
                |network| validate_id(&network.network_id, span).map(Value::Id),
            );
        }

        let [network_id] = arguments else {
            return Err(invalid_call_arguments(span));
        };
        let network_span = network_id.span;
        let network_id = match &network_id.value {
            Value::Id(value) | Value::String(value) => validate_id(value, network_span)?,
            Value::Null(_) => return Ok(Value::Null(Some(TypeName::String))),
            _ => return Err(invalid_runtime_operands(network_span)),
        };
        let context = self.host.network_context().ok_or_else(|| {
            runtime_exception(
                "TypeException",
                "System.Network URL lookup requires a configured network context",
                network_span,
            )
        })?;
        let configured_id = validate_id(&context.network_id, network_span)?;
        if !same_record_id(&network_id, &configured_id) {
            return Err(runtime_exception(
                "TypeException",
                format!("network `{network_id}` is not present in the configured network context"),
                network_span,
            ));
        }
        let value = match intrinsic {
            P::NetworkGetLoginUrl => context.login_url,
            P::NetworkGetLogoutUrl => context.logout_url,
            P::NetworkGetSelfRegUrl => context.self_registration_url,
            P::NetworkGetNetworkId => unreachable!(),
            _ => return Err(invalid_runtime_operands(span)),
        };
        Ok(value.map_or_else(|| Value::Null(Some(TypeName::String)), Value::String))
    }

    fn call_logging_level(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::LoggingLevelValues => {
                expect_no_arguments(arguments, span)?;
                let elements = LoggingLevel::VALUES
                    .into_iter()
                    .map(|value| {
                        self.store.allocate_platform(PlatformValue::PlatformEnum(
                            crate::platform::PlatformEnum::LoggingLevel(value),
                        ))
                    })
                    .collect();
                Ok(self.store.allocate_collection(Collection::List {
                    element_type: TypeName::LoggingLevel,
                    elements,
                    iteration_depth: 0,
                }))
            }
            P::LoggingLevelValueOf => {
                let [name] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let name_span = name.span;
                let name = expect_string(&name.value, name_span)?;
                let value = LoggingLevel::from_apex_name(name).ok_or_else(|| {
                    runtime_exception(
                        "TypeException",
                        format!("Invalid enum value: {name}"),
                        name_span,
                    )
                })?;
                Ok(self.store.allocate_platform(PlatformValue::PlatformEnum(
                    crate::platform::PlatformEnum::LoggingLevel(value),
                )))
            }
            P::PlatformEnumOrdinal => {
                expect_no_arguments(arguments, span)?;
                let Some(Value::Platform(id)) = receiver else {
                    return Err(invalid_runtime_operands(span));
                };
                let PlatformValue::PlatformEnum(value) = self.store.platform(id) else {
                    return Err(invalid_runtime_operands(span));
                };
                value
                    .ordinal()
                    .map(Value::Integer)
                    .ok_or_else(|| invalid_runtime_operands(span))
            }
            _ => unreachable!("only LoggingLevel intrinsics use this helper"),
        }
    }

    fn set_test_mock(
        &mut self,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if !self.execution_context.is_test() {
            return Err(runtime_exception(
                "TypeException",
                "Test.setMock is only valid while running an Apex test",
                span,
            ));
        }
        let [mock_type, mock] = arguments else {
            return Err(invalid_call_arguments(span));
        };
        if !matches!(
            mock_type.value,
            Value::TypeLiteral(TypeName::HttpCalloutMock)
        ) {
            return Err(runtime_exception(
                "TypeException",
                "Test.setMock currently requires System.HttpCalloutMock.class",
                mock_type.span,
            ));
        }
        let Value::Object(receiver) = mock.value else {
            return Err(invalid_runtime_operands(mock.span));
        };
        let class_id = self.store.object(receiver).class_id;
        if self
            .program()
            .http_callout_mock_contract(class_id)
            .is_none()
        {
            return Err(runtime_exception(
                "TypeException",
                "mock object does not implement System.HttpCalloutMock",
                mock.span,
            ));
        }
        self.http_callout_mock = Some(receiver);
        Ok(Value::Void)
    }

    fn call_http_callout_mock(
        &mut self,
        receiver: super::ObjectId,
        request: EvaluatedArgument,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let class_id = self.store.object(receiver).class_id;
        let target = self
            .program()
            .http_callout_mock_contract(class_id)
            .ok_or_else(|| {
                runtime_exception(
                    "TypeException",
                    "runtime object does not implement System.HttpCalloutMock",
                    span,
                )
            })?;
        let response = self.evaluate_class_method_arguments(
            target,
            Some(receiver),
            vec![request],
            span,
            true,
            false,
        )?;
        let Value::Platform(response_id) = response else {
            return Err(invalid_runtime_operands(span));
        };
        if !matches!(
            self.store.platform(response_id),
            PlatformValue::HttpResponse(_)
        ) {
            return Err(invalid_runtime_operands(span));
        }
        Ok(Value::Platform(response_id))
    }

    fn call_async_context_method(
        &mut self,
        receiver: Option<Value>,
        intrinsic: PlatformIntrinsic,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match receiver {
            Some(Value::Platform(id)) => self.call_native_async_context(id, intrinsic, span),
            Some(Value::Object(receiver)) => {
                self.call_mock_async_context(receiver, intrinsic, span)
            }
            _ => Err(invalid_runtime_operands(span)),
        }
    }

    fn call_native_async_context(
        &mut self,
        id: super::PlatformValueId,
        intrinsic: PlatformIntrinsic,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let PlatformValue::AsyncContext { job_id, .. } = self.store.platform(id) else {
            return Err(invalid_runtime_operands(span));
        };
        let job_id = job_id.clone();
        Ok(match intrinsic {
            PlatformIntrinsic::AsyncContextGetJobId
            | PlatformIntrinsic::FinalizerContextGetAsyncApexJobId
            | PlatformIntrinsic::SchedulableContextGetTriggerId => Value::Id(job_id),
            PlatformIntrinsic::BatchableContextGetChildJobId => Value::Null(Some(TypeName::Id)),
            PlatformIntrinsic::FinalizerContextGetException => {
                Value::Null(Some(TypeName::Exception))
            }
            PlatformIntrinsic::FinalizerContextGetResult => self.store.allocate_platform(
                PlatformValue::PlatformEnum(crate::platform::PlatformEnum::ParentJobResult(
                    crate::platform::ParentJobResult::Success,
                )),
            ),
            PlatformIntrinsic::FinalizerContextGetRequestId => {
                Value::String(self.current_request_id())
            }
            _ => unreachable!("non-context intrinsic reached context dispatch"),
        })
    }

    fn call_mock_async_context(
        &mut self,
        receiver: super::ObjectId,
        intrinsic: PlatformIntrinsic,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let class_id = self.store.object(receiver).class_id;
        let target = self
            .mock_async_context_target(class_id, intrinsic)
            .ok_or_else(|| {
                runtime_exception(
                    "TypeException",
                    "runtime object does not implement the checked platform context contract",
                    span,
                )
            })?;
        self.evaluate_class_method_arguments(target, Some(receiver), Vec::new(), span, true, false)
    }

    fn mock_async_context_target(
        &self,
        class_id: usize,
        intrinsic: PlatformIntrinsic,
    ) -> Option<crate::hir::ClassMemberId> {
        match intrinsic {
            PlatformIntrinsic::AsyncContextGetJobId => self
                .program()
                .batchable_context_contract(class_id)
                .map(|contract| contract.get_job_id)
                .or_else(|| self.program().queueable_context_contract(class_id)),
            PlatformIntrinsic::BatchableContextGetChildJobId => self
                .program()
                .batchable_context_contract(class_id)
                .map(|contract| contract.get_child_job_id),
            PlatformIntrinsic::FinalizerContextGetAsyncApexJobId => self
                .program()
                .finalizer_context_contract(class_id)
                .map(|contract| contract.get_async_apex_job_id),
            PlatformIntrinsic::FinalizerContextGetException => self
                .program()
                .finalizer_context_contract(class_id)
                .map(|contract| contract.get_exception),
            PlatformIntrinsic::FinalizerContextGetResult => self
                .program()
                .finalizer_context_contract(class_id)
                .map(|contract| contract.get_result),
            PlatformIntrinsic::FinalizerContextGetRequestId => self
                .program()
                .finalizer_context_contract(class_id)
                .map(|contract| contract.get_request_id),
            PlatformIntrinsic::SchedulableContextGetTriggerId => {
                self.program().schedulable_context_contract(class_id)
            }
            _ => unreachable!("non-context intrinsic reached context dispatch"),
        }
    }

    fn current_request_id(&self) -> String {
        self.current_async.as_ref().map_or_else(
            || "REQ000000000001".to_owned(),
            |context| context.id.clone(),
        )
    }

    fn current_request_quiddity(&self) -> crate::platform::Quiddity {
        if self.execution_context.is_test() {
            if self.current_async.is_some() {
                crate::platform::Quiddity::RunTestAsync
            } else {
                crate::platform::Quiddity::RunTestSync
            }
        } else {
            crate::platform::Quiddity::Undefined
        }
    }

    fn require_cache_partition_receiver(
        &self,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some(Value::Platform(id)) = receiver else {
            return Err(invalid_runtime_operands(span));
        };
        if matches!(self.store.platform(id), PlatformValue::CachePartition) {
            Ok(())
        } else {
            Err(invalid_runtime_operands(span))
        }
    }

    fn resolve_type_for_name(&self, name: &str, span: Span) -> Option<TypeName> {
        if let Some(ty) = TypeName::from_apex_name(name) {
            return Some(ty);
        }
        let named = crate::ast::NamedType::new(name.to_owned(), span);
        let schema_name = crate::hir::schema_api_name(&named);
        if name.to_ascii_lowercase().starts_with("schema.")
            && self.program().schema().object_index(schema_name).is_some()
        {
            return Some(TypeName::Custom(named));
        }
        if let Some(class_id) = self.runtime_class_id(&named) {
            return Some(TypeName::Custom(crate::ast::NamedType::new(
                self.classes()[class_id].qualified_name.spelling.clone(),
                span,
            )));
        }
        self.program()
            .schema()
            .object_index(schema_name)
            .map(|_| TypeName::Custom(named))
    }

    fn instantiate_type(
        &mut self,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Some(Value::TypeLiteral(TypeName::Custom(name))) = receiver else {
            return Err(runtime_exception(
                "TypeException",
                "Type.newInstance requires a constructible Apex or SObject type",
                span,
            ));
        };
        if let Some(object_id) = self
            .program()
            .schema()
            .object_index(crate::hir::schema_api_name(&name))
        {
            return Ok(self.store.allocate_sobject(object_id));
        }
        let class_id = self.runtime_class_id(&name).ok_or_else(|| {
            runtime_exception(
                "TypeException",
                format!("unknown Apex type `{}`", name.spelling),
                span,
            )
        })?;
        let class = &self.classes()[class_id];
        if class.kind != crate::ast::ClassKind::Class
            || class.modifiers.contains(&crate::ast::Modifier::Abstract)
        {
            return Err(runtime_exception(
                "TypeException",
                format!(
                    "type `{}` is not constructible",
                    class.qualified_name.spelling
                ),
                span,
            ));
        }
        let constructor = self.zero_argument_constructor(class_id);
        if constructor.is_none()
            && class
                .members
                .iter()
                .any(|member| matches!(member, crate::ast::ClassMember::Constructor(_)))
        {
            return Err(runtime_exception(
                "TypeException",
                format!(
                    "type `{}` has no zero-argument constructor",
                    class.qualified_name.spelling
                ),
                span,
            ));
        }
        self.ensure_class_initialized(class_id, span)?;
        self.construct_user_object(class_id, constructor, Vec::new(), span)
    }

    fn call_callable(
        &mut self,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Some(Value::Object(receiver)) = receiver else {
            return Err(invalid_runtime_operands(span));
        };
        if arguments.len() != 2 {
            return Err(invalid_call_arguments(span));
        }
        let class_id = self.store.object(receiver).class_id;
        let target = self.program().callable_contract(class_id).ok_or_else(|| {
            runtime_exception(
                "TypeException",
                "runtime object does not implement System.Callable",
                span,
            )
        })?;
        self.evaluate_class_method_arguments(
            target,
            Some(receiver),
            arguments.to_vec(),
            span,
            true,
            false,
        )
    }

    fn strip_inaccessible(
        &mut self,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let inputs = self.strip_inaccessible_inputs(arguments, span)?;
        let mut state = StripState {
            access_type: inputs.access_type,
            user_id: self.current_user_context().user_id,
            removed_fields: BTreeMap::new(),
            memo: BTreeMap::new(),
            nodes: 0,
            span: inputs.access_span,
        };
        let mut stripped = Vec::with_capacity(inputs.elements.len());
        for value in inputs.elements {
            let Value::SObject(source_id) = value else {
                return Err(invalid_runtime_operands(inputs.records_span));
            };
            self.require_root_object_access(source_id, inputs.enforce_root_object_crud, &state)?;
            stripped.push(Value::SObject(
                self.strip_sobject(source_id, 0, &mut state)?,
            ));
        }
        normalize_removed_fields(&mut state.removed_fields);
        let records = self.allocate_stripped_records(stripped, span);
        Ok(self
            .store
            .allocate_platform(PlatformValue::SecurityDecision {
                records,
                removed_fields: state.removed_fields,
            }))
    }

    fn strip_inaccessible_inputs(
        &self,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<StripInputs, Diagnostic> {
        let ([access, records] | [access, records, _]) = arguments else {
            return Err(invalid_call_arguments(span));
        };
        let Value::Platform(access_id) = access.value else {
            return Err(invalid_runtime_operands(access.span));
        };
        let PlatformValue::AccessType(access_type) = self.store.platform(access_id) else {
            return Err(invalid_runtime_operands(access.span));
        };
        let access_type = *access_type;
        let Value::Collection(records_id) = records.value else {
            return Err(invalid_runtime_operands(records.span));
        };
        let Collection::List { elements, .. } = self.store.collection(records_id) else {
            return Err(invalid_runtime_operands(records.span));
        };
        let elements = elements.clone();
        let enforce_root_object_crud = match arguments.get(2) {
            None => true,
            Some(argument) => match argument.value {
                Value::Boolean(value) => value,
                _ => return Err(invalid_runtime_operands(argument.span)),
            },
        };
        Ok(StripInputs {
            access_type,
            elements,
            enforce_root_object_crud,
            access_span: access.span,
            records_span: records.span,
        })
    }

    fn require_root_object_access(
        &self,
        source_id: SObjectId,
        enforce: bool,
        state: &StripState,
    ) -> Result<(), Diagnostic> {
        let source = self.store.sobject(source_id);
        let object = self
            .program()
            .schema()
            .object_at(source.object_id)
            .expect("checked SObject type is present");
        let allowed = self
            .host
            .security_object_access(&state.user_id, object.api_name(), state.access_type)
            .map_err(|error| {
                runtime_exception("NoAccessException", error.to_string(), state.span)
            })?;
        if enforce && !allowed {
            return Err(runtime_exception(
                "NoAccessException",
                format!(
                    "No {} access to entity: {}",
                    state.access_type.apex_name(),
                    object.api_name()
                ),
                state.span,
            ));
        }
        Ok(())
    }

    fn allocate_stripped_records(&mut self, elements: Vec<Value>, span: Span) -> CollectionId {
        let Value::Collection(records) = self.store.allocate_collection(Collection::List {
            element_type: TypeName::Custom(crate::ast::NamedType::new("SObject".to_owned(), span)),
            elements,
            iteration_depth: 0,
        }) else {
            unreachable!("list allocation always returns a collection")
        };
        records
    }

    fn strip_sobject(
        &mut self,
        source_id: SObjectId,
        depth: usize,
        state: &mut StripState,
    ) -> Result<SObjectId, Diagnostic> {
        if let Some(target) = state.memo.get(&source_id) {
            return Ok(*target);
        }
        if depth > STRIP_INACCESSIBLE_DEPTH_LIMIT {
            return Err(runtime_exception(
                "NoAccessException",
                format!(
                    "Security.stripInaccessible exceeded relationship depth limit of {STRIP_INACCESSIBLE_DEPTH_LIMIT}"
                ),
                state.span,
            ));
        }
        state.nodes = state.nodes.saturating_add(1);
        if state.nodes > STRIP_INACCESSIBLE_NODE_LIMIT {
            return Err(runtime_exception(
                "NoAccessException",
                format!(
                    "Security.stripInaccessible exceeded SObject node limit of {STRIP_INACCESSIBLE_NODE_LIMIT}"
                ),
                state.span,
            ));
        }

        let source = self.store.sobject(source_id).clone();
        let object = self
            .program()
            .schema()
            .object_at(source.object_id)
            .expect("runtime SObject type belongs to its schema")
            .clone();
        let Value::SObject(target_id) = self.store.allocate_sobject(source.object_id) else {
            unreachable!("SObject allocation always returns an SObject value")
        };
        state.memo.insert(source_id, target_id);
        let mut target_fields = self.strip_sobject_fields(&source, &object, state)?;
        let target_relationships =
            self.strip_sobject_relationships(&source, &object, depth, state, &mut target_fields)?;
        let target_children = self.strip_sobject_children(&source, depth, state)?;
        let target = self.store.sobject_mut(target_id);
        target.fields = target_fields;
        target.relationships = target_relationships;
        target.children = target_children;
        Ok(target_id)
    }

    fn strip_sobject_fields(
        &self,
        source: &SObjectInstance,
        object: &ObjectSchema,
        state: &mut StripState,
    ) -> Result<BTreeMap<usize, Value>, Diagnostic> {
        let mut target = BTreeMap::new();
        for (field_id, value) in &source.fields {
            let field = object
                .field_at(*field_id)
                .expect("runtime SObject field belongs to its schema");
            if self.security_field_allowed(
                &state.user_id,
                object.api_name(),
                field.api_name(),
                state.access_type,
                state.span,
            )? {
                target.insert(*field_id, value.clone());
            } else {
                record_removed_field(
                    &mut state.removed_fields,
                    object.api_name(),
                    field.api_name(),
                );
            }
        }
        Ok(target)
    }

    fn strip_sobject_relationships(
        &mut self,
        source: &SObjectInstance,
        object: &ObjectSchema,
        depth: usize,
        state: &mut StripState,
        target_fields: &mut BTreeMap<usize, Value>,
    ) -> Result<BTreeMap<usize, SObjectId>, Diagnostic> {
        let mut target_relationships = BTreeMap::new();
        for (reference_field_id, related_id) in &source.relationships {
            let field = object
                .field_at(*reference_field_id)
                .expect("runtime relationship field belongs to its schema");
            if self.security_field_allowed(
                &state.user_id,
                object.api_name(),
                field.api_name(),
                state.access_type,
                state.span,
            )? {
                let related = self.strip_sobject(*related_id, depth + 1, state)?;
                target_relationships.insert(*reference_field_id, related);
            } else {
                target_fields.remove(reference_field_id);
                record_removed_field(
                    &mut state.removed_fields,
                    object.api_name(),
                    field.api_name(),
                );
            }
        }
        Ok(target_relationships)
    }

    fn strip_sobject_children(
        &mut self,
        source: &SObjectInstance,
        depth: usize,
        state: &mut StripState,
    ) -> Result<BTreeMap<String, CollectionId>, Diagnostic> {
        let mut target_children = BTreeMap::new();
        for (relationship, collection_id) in &source.children {
            let Collection::List {
                element_type,
                elements,
                ..
            } = self.store.collection(*collection_id).clone()
            else {
                return Err(runtime_exception(
                    "NoAccessException",
                    "Security.stripInaccessible encountered a non-list child relationship",
                    state.span,
                ));
            };
            let mut sanitized = Vec::with_capacity(elements.len());
            for value in elements {
                let Value::SObject(child_id) = value else {
                    return Err(runtime_exception(
                        "NoAccessException",
                        "Security.stripInaccessible encountered a non-SObject child relationship",
                        state.span,
                    ));
                };
                sanitized.push(Value::SObject(self.strip_sobject(
                    child_id,
                    depth + 1,
                    state,
                )?));
            }
            let Value::Collection(collection) = self.store.allocate_collection(Collection::List {
                element_type,
                elements: sanitized,
                iteration_depth: 0,
            }) else {
                unreachable!("list allocation always returns a collection")
            };
            target_children.insert(relationship.clone(), collection);
        }
        Ok(target_children)
    }

    fn security_field_allowed(
        &self,
        user_id: &str,
        object: &str,
        field: &str,
        access_type: crate::platform::AccessType,
        span: Span,
    ) -> Result<bool, Diagnostic> {
        self.host
            .security_field_access(user_id, object, field, access_type)
            .map_err(|error| runtime_exception("NoAccessException", error.to_string(), span))
    }

    fn call_test_context(
        &mut self,
        intrinsic: PlatformIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        expect_no_arguments(arguments, span)?;
        match intrinsic {
            P::TestStartTest => {
                self.host.begin_test_window();
                Ok(Value::Void)
            }
            P::TestStopTest => {
                self.host.end_test_window();
                self.drain_async_jobs(span)?;
                Ok(Value::Void)
            }
            P::TestIsRunningTest => Ok(Value::Boolean(self.execution_context.is_test())),
            _ => unreachable!("only Test lifecycle intrinsics use this helper"),
        }
    }

    fn expect_blob(&self, receiver: Option<Value>, span: Span) -> Result<Vec<u8>, Diagnostic> {
        let Some(Value::Platform(id)) = receiver else {
            return Err(invalid_runtime_operands(span));
        };
        match self.store.platform(id) {
            PlatformValue::Blob(bytes) => Ok(bytes.clone()),
            _ => Err(invalid_runtime_operands(span)),
        }
    }

    fn expect_pattern(&self, receiver: Option<Value>, span: Span) -> Result<String, Diagnostic> {
        let Some(Value::Platform(id)) = receiver else {
            return Err(invalid_runtime_operands(span));
        };
        match self.store.platform(id) {
            PlatformValue::Pattern(pattern) => Ok(pattern.clone()),
            _ => Err(invalid_runtime_operands(span)),
        }
    }

    fn matcher_matches(
        &mut self,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let id = matcher_id(receiver, span)?;
        let (pattern, input) = match self.store.platform(id) {
            PlatformValue::Matcher { pattern, input, .. } => (pattern.clone(), input.clone()),
            _ => return Err(invalid_runtime_operands(span)),
        };
        let regex = Regex::new(&format!("^(?:{pattern})$"))
            .map_err(|error| platform_error(format!("invalid regex: {error}"), span))?;
        let captures = regex.captures(&input);
        let matched = captures.is_some();
        let stored = captures
            .map(|captures| {
                captures
                    .iter()
                    .map(|capture| capture.map(|capture| (capture.start(), capture.end())))
                    .collect()
            })
            .unwrap_or_default();
        let PlatformValue::Matcher { captures, .. } = self.store.platform_mut(id) else {
            unreachable!()
        };
        *captures = stored;
        Ok(Value::Boolean(matched))
    }

    fn matcher_find(&mut self, receiver: Option<Value>, span: Span) -> Result<Value, Diagnostic> {
        let id = matcher_id(receiver, span)?;
        let (pattern, input, start) = match self.store.platform(id) {
            PlatformValue::Matcher {
                pattern,
                input,
                next_start,
                ..
            } => (pattern.clone(), input.clone(), *next_start),
            _ => return Err(invalid_runtime_operands(span)),
        };
        let regex = Regex::new(&pattern)
            .map_err(|error| platform_error(format!("invalid regex: {error}"), span))?;
        let captures = regex.captures_at(&input, start);
        let (stored, next_start) = if let Some(captures) = captures {
            let spans = captures
                .iter()
                .map(|capture| capture.map(|capture| (capture.start(), capture.end())))
                .collect::<Vec<_>>();
            let whole = spans[0].expect("successful captures have a whole match");
            (spans, whole.1.max(whole.0.saturating_add(1)))
        } else {
            (Vec::new(), input.len())
        };
        let matched = !stored.is_empty();
        let PlatformValue::Matcher {
            captures,
            next_start: stored_start,
            ..
        } = self.store.platform_mut(id)
        else {
            unreachable!()
        };
        *captures = stored;
        *stored_start = next_start;
        Ok(Value::Boolean(matched))
    }

    fn matcher_capture(
        &self,
        receiver: Option<Value>,
        intrinsic: PlatformIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let id = matcher_id(receiver, span)?;
        let group = match arguments {
            [] => 0,
            [group] => usize::try_from(expect_integer(&group.value, group.span)?)
                .map_err(|_| platform_error("regex group cannot be negative", group.span))?,
            _ => return Err(invalid_call_arguments(span)),
        };
        let PlatformValue::Matcher {
            input, captures, ..
        } = self.store.platform(id)
        else {
            return Err(invalid_runtime_operands(span));
        };
        let capture =
            captures.get(group).copied().flatten().ok_or_else(|| {
                platform_error(format!("regex group {group} is unavailable"), span)
            })?;
        Ok(match intrinsic {
            PlatformIntrinsic::MatcherGroup => {
                Value::String(input[capture.0..capture.1].to_owned())
            }
            PlatformIntrinsic::MatcherStart => {
                Value::Integer(i64::try_from(capture.0).unwrap_or(i64::MAX))
            }
            PlatformIntrinsic::MatcherEnd => {
                Value::Integer(i64::try_from(capture.1).unwrap_or(i64::MAX))
            }
            _ => unreachable!(),
        })
    }

    fn expect_schema_object(
        &self,
        receiver: Option<Value>,
        describe: bool,
        span: Span,
    ) -> Result<usize, Diagnostic> {
        let Some(Value::Platform(id)) = receiver else {
            return Err(invalid_runtime_operands(span));
        };
        match self.store.platform(id) {
            PlatformValue::SObjectType(object_id) if !describe => Ok(*object_id),
            PlatformValue::DescribeSObject(object_id) if describe => Ok(*object_id),
            _ => Err(invalid_runtime_operands(span)),
        }
    }

    pub(super) fn expect_any_schema_object(
        &self,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<usize, Diagnostic> {
        let Some(Value::Platform(id)) = receiver else {
            return Err(invalid_runtime_operands(span));
        };
        match self.store.platform(id) {
            PlatformValue::SObjectType(object_id)
            | PlatformValue::DescribeSObject(object_id)
            | PlatformValue::SObjectFieldMap(object_id)
            | PlatformValue::FieldSetMap(object_id) => Ok(*object_id),
            _ => Err(invalid_runtime_operands(span)),
        }
    }

    fn expect_schema_field(
        &self,
        receiver: Option<Value>,
        describe: bool,
        span: Span,
    ) -> Result<(usize, usize), Diagnostic> {
        let Some(Value::Platform(id)) = receiver else {
            return Err(invalid_runtime_operands(span));
        };
        match self.store.platform(id) {
            PlatformValue::SObjectField {
                object_id,
                field_id,
            } if !describe => Ok((*object_id, *field_id)),
            PlatformValue::DescribeField {
                object_id,
                field_id,
            } if describe => Ok((*object_id, *field_id)),
            _ => Err(invalid_runtime_operands(span)),
        }
    }

    fn call_schema_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        if matches!(
            intrinsic,
            P::SObjectTypeGetDescribe
                | P::SObjectTypeGetName
                | P::SObjectTypeNewSObject
                | P::SObjectFieldGetDescribe
                | P::SObjectFieldMapGetMap
                | P::FieldSetMapGetMap
        ) {
            return self.call_schema_token_intrinsic(intrinsic, receiver, arguments, span);
        }
        expect_no_arguments(arguments, span)?;
        match intrinsic {
            P::SchemaGetGlobalDescribe => self.schema_global_describe(),
            P::DescribeGetName
            | P::DescribeGetLocalName
            | P::DescribeGetLabel
            | P::DescribeGetLabelPlural
            | P::DescribeGetKeyPrefix
            | P::DescribeIsCustom
            | P::DescribeIsCustomSetting
            | P::DescribeIsAccessible
            | P::DescribeIsDeletable
            | P::DescribeIsUpdateable => self.describe_sobject_value(intrinsic, receiver, span),
            P::DescribeFieldGetName
            | P::DescribeFieldGetLocalName
            | P::DescribeFieldGetLabel
            | P::DescribeFieldGetLength
            | P::DescribeFieldGetInlineHelpText
            | P::DescribeFieldGetRelationshipName
            | P::DescribeFieldGetSoapType
            | P::DescribeFieldGetType
            | P::DescribeFieldGetReferenceTo
            | P::DescribeFieldGetPicklistValues
            | P::DescribeFieldIsNameField
            | P::DescribeFieldIsSortable
            | P::DescribeFieldIsAccessible => self.describe_field_value(intrinsic, receiver, span),
            P::FieldSetGetName
            | P::FieldSetGetLabel
            | P::FieldSetGetNamespace
            | P::FieldSetGetFields => self.field_set_value(intrinsic, receiver, span),
            P::FieldSetMemberGetFieldPath
            | P::FieldSetMemberGetLabel
            | P::FieldSetMemberGetSObjectField => {
                self.field_set_member_value(intrinsic, receiver, span)
            }
            P::PicklistEntryGetValue => {
                let Some(Value::Platform(id)) = receiver else {
                    return Err(invalid_runtime_operands(span));
                };
                let PlatformValue::PicklistEntry(value) = self.store.platform(id) else {
                    return Err(invalid_runtime_operands(span));
                };
                Ok(Value::String(value.clone()))
            }
            _ => unreachable!("schema intrinsic dispatch is closed"),
        }
    }

    fn call_schema_token_intrinsic(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::SObjectTypeGetDescribe => {
                expect_no_arguments(arguments, span)?;
                let object_id = self.expect_schema_object(receiver, false, span)?;
                Ok(self
                    .store
                    .allocate_platform(PlatformValue::DescribeSObject(object_id)))
            }
            P::SObjectTypeGetName => {
                expect_no_arguments(arguments, span)?;
                let object_id = self.expect_schema_object(receiver, false, span)?;
                Ok(Value::String(
                    self.schema_object(object_id).api_name().to_owned(),
                ))
            }
            P::SObjectTypeNewSObject => {
                let object_id = self.expect_schema_object(receiver, false, span)?;
                self.schema_new_sobject(object_id, arguments, span)
            }
            P::SObjectFieldGetDescribe => {
                expect_no_arguments(arguments, span)?;
                let (object_id, field_id) = self.expect_schema_field(receiver, false, span)?;
                Ok(self.store.allocate_platform(PlatformValue::DescribeField {
                    object_id,
                    field_id,
                }))
            }
            P::SObjectFieldMapGetMap => {
                expect_no_arguments(arguments, span)?;
                self.sobject_field_map(receiver, span)
            }
            P::FieldSetMapGetMap => {
                expect_no_arguments(arguments, span)?;
                self.field_set_map(receiver, span)
            }
            _ => unreachable!("schema token dispatch is closed"),
        }
    }

    fn schema_global_describe(&mut self) -> Result<Value, Diagnostic> {
        let objects = self
            .program()
            .schema()
            .objects()
            .enumerate()
            .map(|(index, object)| (index, object.api_name().to_owned()))
            .collect::<Vec<_>>();
        let entries = objects
            .into_iter()
            .map(|(index, name)| {
                (
                    Value::String(name),
                    self.store
                        .allocate_platform(PlatformValue::SObjectType(index)),
                )
            })
            .collect();
        Ok(self.allocate(Collection::Map {
            key_type: TypeName::String,
            value_type: TypeName::SObjectType,
            entries,
        }))
    }

    fn schema_object(&self, object_id: usize) -> &ObjectSchema {
        self.program()
            .schema()
            .object_at(object_id)
            .expect("schema handle references a checked object")
    }

    fn schema_new_sobject(
        &mut self,
        object_id: usize,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if arguments.len() > 2 {
            return Err(invalid_call_arguments(span));
        }
        let id = match arguments.first() {
            None => None,
            Some(argument) => match &argument.value {
                Value::Id(id) | Value::String(id) => Some(id.clone()),
                Value::Null(_) => None,
                _ => return Err(invalid_runtime_operands(argument.span)),
            },
        };
        if let Some(load_defaults) = arguments.get(1)
            && !matches!(load_defaults.value, Value::Boolean(_))
        {
            return Err(invalid_runtime_operands(load_defaults.span));
        }
        let value = self.store.allocate_sobject(object_id);
        if let (Value::SObject(sobject), Some(id)) = (&value, id)
            && let Some(field_id) = self.schema_object(object_id).field_index("Id")
        {
            self.store
                .sobject_mut(*sobject)
                .fields
                .insert(field_id, Value::Id(id));
        }
        Ok(value)
    }

    fn describe_sobject_value(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        let object_id = self.expect_schema_object(receiver, true, span)?;
        let object = self.schema_object(object_id);
        Ok(match intrinsic {
            P::DescribeGetName => Value::String(object.api_name().to_owned()),
            P::DescribeGetLocalName => Value::String(local_schema_name(object.api_name())),
            P::DescribeGetLabel => Value::String(schema_label(object.api_name())),
            P::DescribeGetLabelPlural => {
                Value::String(format!("{}s", schema_label(object.api_name())))
            }
            P::DescribeGetKeyPrefix => Value::String(object.key_prefix().to_owned()),
            P::DescribeIsCustom => Value::Boolean(
                object.api_name().ends_with("__c")
                    || object.api_name().ends_with("__e")
                    || object.api_name().ends_with("__mdt"),
            ),
            P::DescribeIsCustomSetting => Value::Boolean(false),
            P::DescribeIsAccessible | P::DescribeIsDeletable | P::DescribeIsUpdateable => {
                Value::Boolean(true)
            }
            _ => unreachable!("describe SObject accessor is closed"),
        })
    }

    fn sobject_field_map(
        &mut self,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let object_id = self.expect_any_schema_object(receiver, span)?;
        let fields = self
            .schema_object(object_id)
            .fields()
            .enumerate()
            .map(|(field_id, field)| (field_id, field.api_name().to_owned()))
            .collect::<Vec<_>>();
        let entries = fields
            .into_iter()
            .map(|(field_id, name)| {
                (
                    Value::String(name),
                    self.store.allocate_platform(PlatformValue::SObjectField {
                        object_id,
                        field_id,
                    }),
                )
            })
            .collect();
        Ok(self.allocate(Collection::Map {
            key_type: TypeName::String,
            value_type: TypeName::SObjectField,
            entries,
        }))
    }

    fn describe_field_value(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DescribeFieldGetSoapType | P::DescribeFieldGetType => {
                return self.describe_field_enum_value(intrinsic, receiver, span);
            }
            P::DescribeFieldGetReferenceTo => {
                return self.describe_field_reference_to(receiver, span);
            }
            P::DescribeFieldGetPicklistValues => {
                return self.describe_field_picklist_values(receiver, span);
            }
            _ => {}
        }
        let (object_id, field_id) = self.expect_schema_field(receiver, true, span)?;
        let field = self
            .schema_object(object_id)
            .field_at(field_id)
            .expect("describe field handle references a checked field");
        Ok(match intrinsic {
            P::DescribeFieldGetName => Value::String(field.api_name().to_owned()),
            P::DescribeFieldGetLocalName => Value::String(local_schema_name(field.api_name())),
            P::DescribeFieldGetLabel => Value::String(field.label().to_owned()),
            P::DescribeFieldGetLength => {
                Value::Integer(i64::try_from(field.length()).unwrap_or(i64::MAX))
            }
            P::DescribeFieldGetInlineHelpText => field
                .inline_help_text()
                .map(|value| Value::String(value.to_owned()))
                .unwrap_or_else(|| Value::Null(Some(TypeName::String))),
            P::DescribeFieldGetRelationshipName => field
                .relationship_name()
                .map(|value| Value::String(value.to_owned()))
                .unwrap_or_else(|| Value::Null(Some(TypeName::String))),
            P::DescribeFieldIsNameField => {
                Value::Boolean(field.api_name().eq_ignore_ascii_case("Name"))
            }
            P::DescribeFieldIsSortable | P::DescribeFieldIsAccessible => Value::Boolean(true),
            _ => unreachable!("describe field accessor is closed"),
        })
    }

    fn describe_field_enum_value(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let (object_id, field_id) = self.expect_schema_field(receiver, true, span)?;
        let value = {
            let field = self
                .schema_object(object_id)
                .field_at(field_id)
                .expect("describe field handle references a checked field");
            match intrinsic {
                PlatformIntrinsic::DescribeFieldGetSoapType => schema_soap_type(field),
                PlatformIntrinsic::DescribeFieldGetType => {
                    crate::platform::PlatformEnum::DisplayType(field.display_type())
                }
                _ => unreachable!("describe field enum accessor is closed"),
            }
        };
        Ok(self
            .store
            .allocate_platform(PlatformValue::PlatformEnum(value)))
    }

    fn describe_field_reference_to(
        &mut self,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let (object_id, field_id) = self.expect_schema_field(receiver, true, span)?;
        let target = {
            let field = self
                .schema_object(object_id)
                .field_at(field_id)
                .expect("describe field handle references a checked field");
            match field.data_type() {
                crate::platform::FieldType::Reference { target_object } => {
                    Some(target_object.to_owned())
                }
                crate::platform::FieldType::MetadataRelationship {
                    target_metadata, ..
                } => Some(target_metadata.to_owned()),
                _ => None,
            }
        };
        let elements = target
            .as_deref()
            .and_then(|target| self.program().schema().object_index(target))
            .map(|target| {
                vec![
                    self.store
                        .allocate_platform(PlatformValue::SObjectType(target)),
                ]
            })
            .unwrap_or_default();
        Ok(self.allocate(Collection::List {
            element_type: TypeName::SObjectType,
            elements,
            iteration_depth: 0,
        }))
    }

    fn describe_field_picklist_values(
        &mut self,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let (object_id, field_id) = self.expect_schema_field(receiver, true, span)?;
        let values = self
            .schema_object(object_id)
            .field_at(field_id)
            .expect("describe field handle references a checked field")
            .picklist_values()
            .to_vec();
        let elements = values
            .into_iter()
            .map(|value| {
                self.store
                    .allocate_platform(PlatformValue::PicklistEntry(value))
            })
            .collect();
        Ok(self.allocate(Collection::List {
            element_type: TypeName::PicklistEntry,
            elements,
            iteration_depth: 0,
        }))
    }

    fn field_set_map(&mut self, receiver: Option<Value>, span: Span) -> Result<Value, Diagnostic> {
        let object_id = self.expect_any_schema_object(receiver, span)?;
        let field_sets = self
            .schema_object(object_id)
            .field_sets()
            .map(|field_set| field_set.api_name().to_owned())
            .collect::<Vec<_>>();
        let entries = field_sets
            .into_iter()
            .map(|name| {
                (
                    Value::String(name.clone()),
                    self.store
                        .allocate_platform(PlatformValue::FieldSet { object_id, name }),
                )
            })
            .collect();
        Ok(self.allocate(Collection::Map {
            key_type: TypeName::String,
            value_type: TypeName::FieldSet,
            entries,
        }))
    }

    fn field_set_value(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        let Some(Value::Platform(id)) = receiver else {
            return Err(invalid_runtime_operands(span));
        };
        let PlatformValue::FieldSet { object_id, name } = self.store.platform(id) else {
            return Err(invalid_runtime_operands(span));
        };
        let object_id = *object_id;
        let name = name.clone();
        let field_set = self
            .schema_object(object_id)
            .field_set(&name)
            .expect("field set handle references imported metadata");
        Ok(match intrinsic {
            P::FieldSetGetName => Value::String(name),
            P::FieldSetGetLabel => Value::String(field_set.label().to_owned()),
            P::FieldSetGetNamespace => Value::Null(Some(TypeName::String)),
            P::FieldSetGetFields => {
                let field_ids = field_set
                    .fields()
                    .iter()
                    .filter_map(|field| self.schema_object(object_id).field_index(field))
                    .collect::<Vec<_>>();
                let elements = field_ids
                    .into_iter()
                    .map(|field_id| {
                        self.store.allocate_platform(PlatformValue::FieldSetMember {
                            object_id,
                            field_id,
                        })
                    })
                    .collect();
                self.allocate(Collection::List {
                    element_type: TypeName::FieldSetMember,
                    elements,
                    iteration_depth: 0,
                })
            }
            _ => unreachable!("field set accessor is closed"),
        })
    }

    fn field_set_member_value(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        use PlatformIntrinsic as P;
        let Some(Value::Platform(id)) = receiver else {
            return Err(invalid_runtime_operands(span));
        };
        let PlatformValue::FieldSetMember {
            object_id,
            field_id,
        } = self.store.platform(id)
        else {
            return Err(invalid_runtime_operands(span));
        };
        let object_id = *object_id;
        let field_id = *field_id;
        let field = self
            .schema_object(object_id)
            .field_at(field_id)
            .expect("field set member references a checked field");
        Ok(match intrinsic {
            P::FieldSetMemberGetFieldPath => Value::String(field.api_name().to_owned()),
            P::FieldSetMemberGetLabel => Value::String(field.label().to_owned()),
            P::FieldSetMemberGetSObjectField => {
                self.store.allocate_platform(PlatformValue::SObjectField {
                    object_id,
                    field_id,
                })
            }
            _ => unreachable!("field set member accessor is closed"),
        })
    }

    fn mutate_http_request(
        &mut self,
        receiver: Option<Value>,
        intrinsic: PlatformIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let id = platform_id(receiver, span)?;
        let PlatformValue::HttpRequest(request) = self.store.platform_mut(id) else {
            return Err(invalid_runtime_operands(span));
        };
        match (intrinsic, arguments) {
            (PlatformIntrinsic::HttpRequestSetEndpoint, [value]) => {
                request.endpoint = expect_string(&value.value, value.span)?.to_owned()
            }
            (PlatformIntrinsic::HttpRequestSetMethod, [value]) => {
                request.method = expect_string(&value.value, value.span)?.to_uppercase()
            }
            (PlatformIntrinsic::HttpRequestSetBody, [value]) => {
                request.body = expect_string(&value.value, value.span)?.to_owned()
            }
            (PlatformIntrinsic::HttpRequestSetTimeout, [value]) => {
                request.timeout_ms = expect_integer(&value.value, value.span)?
            }
            (PlatformIntrinsic::HttpRequestSetHeader, [name, value]) => {
                request.headers.insert(
                    expect_string(&name.value, name.span)?.to_ascii_lowercase(),
                    expect_string(&value.value, value.span)?.to_owned(),
                );
            }
            _ => return Err(invalid_call_arguments(span)),
        }
        Ok(Value::Void)
    }

    fn read_http_request(
        &self,
        receiver: Option<Value>,
        intrinsic: PlatformIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let id = platform_id(receiver, span)?;
        let PlatformValue::HttpRequest(request) = self.store.platform(id) else {
            return Err(invalid_runtime_operands(span));
        };
        Ok(match intrinsic {
            PlatformIntrinsic::HttpRequestGetEndpoint => {
                expect_no_arguments(arguments, span)?;
                Value::String(request.endpoint.clone())
            }
            PlatformIntrinsic::HttpRequestGetMethod => {
                expect_no_arguments(arguments, span)?;
                Value::String(request.method.clone())
            }
            PlatformIntrinsic::HttpRequestGetBody => {
                expect_no_arguments(arguments, span)?;
                Value::String(request.body.clone())
            }
            PlatformIntrinsic::HttpRequestGetTimeout => {
                expect_no_arguments(arguments, span)?;
                Value::Integer(request.timeout_ms)
            }
            PlatformIntrinsic::HttpRequestGetHeader => {
                let [name] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let name = expect_string(&name.value, name.span)?.to_ascii_lowercase();
                request
                    .headers
                    .get(&name)
                    .cloned()
                    .map(Value::String)
                    .unwrap_or(Value::Null(Some(TypeName::String)))
            }
            _ => return Err(invalid_call_arguments(span)),
        })
    }

    fn mutate_http_response(
        &mut self,
        receiver: Option<Value>,
        intrinsic: PlatformIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let id = platform_id(receiver, span)?;
        let PlatformValue::HttpResponse(response) = self.store.platform_mut(id) else {
            return Err(invalid_runtime_operands(span));
        };
        match (intrinsic, arguments) {
            (PlatformIntrinsic::HttpResponseSetStatusCode, [value]) => {
                response.status_code = expect_integer(&value.value, value.span)?
            }
            (PlatformIntrinsic::HttpResponseSetBody, [value]) => {
                response.body = expect_string(&value.value, value.span)?.to_owned()
            }
            (PlatformIntrinsic::HttpResponseSetStatus, [value]) => {
                response.status = expect_string(&value.value, value.span)?.to_owned()
            }
            (PlatformIntrinsic::HttpResponseSetHeader, [name, value]) => {
                response.headers.insert(
                    expect_string(&name.value, name.span)?.to_ascii_lowercase(),
                    expect_string(&value.value, value.span)?.to_owned(),
                );
            }
            _ => return Err(invalid_call_arguments(span)),
        }
        Ok(Value::Void)
    }

    fn read_http_response(
        &self,
        receiver: Option<Value>,
        intrinsic: PlatformIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let id = platform_id(receiver, span)?;
        let PlatformValue::HttpResponse(response) = self.store.platform(id) else {
            return Err(invalid_runtime_operands(span));
        };
        Ok(match intrinsic {
            PlatformIntrinsic::HttpResponseGetStatusCode => {
                expect_no_arguments(arguments, span)?;
                Value::Integer(response.status_code)
            }
            PlatformIntrinsic::HttpResponseGetBody => {
                expect_no_arguments(arguments, span)?;
                Value::String(response.body.clone())
            }
            PlatformIntrinsic::HttpResponseGetStatus => {
                expect_no_arguments(arguments, span)?;
                Value::String(response.status.clone())
            }
            PlatformIntrinsic::HttpResponseGetHeader => {
                let [name] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let name = expect_string(&name.value, name.span)?.to_ascii_lowercase();
                response
                    .headers
                    .get(&name)
                    .cloned()
                    .map(Value::String)
                    .unwrap_or(Value::Null(Some(TypeName::String)))
            }
            _ => return Err(invalid_call_arguments(span)),
        })
    }

    pub(super) fn value_to_json(&self, value: &Value, span: Span) -> Result<JsonValue, Diagnostic> {
        let mut traversal = ValueGraphTraversal::for_json();
        self.value_to_json_inner(value, span, 0, &mut traversal)
    }

    fn value_to_json_inner(
        &self,
        value: &Value,
        span: Span,
        depth: usize,
        traversal: &mut ValueGraphTraversal,
    ) -> Result<JsonValue, Diagnostic> {
        if let Value::Object(id) = value {
            return self.object_to_json(*id, span, depth, traversal);
        }
        if matches!(value, Value::SObject(_) | Value::AggregateResult(_)) {
            let mut rendered = String::new();
            self.render_value_with_traversal(
                value,
                depth,
                traversal,
                &mut rendered,
                CycleBehavior::Error,
            )
            .map_err(|error| json_traversal_error(error, span))?;
            return Ok(JsonValue::String(rendered));
        }
        traversal
            .visit_node(depth)
            .map_err(|error| json_traversal_error(error, span))?;
        Ok(match value {
            Value::Null(_) => JsonValue::Null,
            Value::Boolean(value) => JsonValue::Bool(*value),
            Value::Integer(value) => JsonValue::Number((*value).into()),
            Value::Long(value) => JsonValue::Number((*value).into()),
            Value::Decimal(value) => JsonValue::Number(
                JsonNumber::from_str(&value.normalize().to_string())
                    .map_err(|_| platform_error("Decimal cannot be serialized to JSON", span))?,
            ),
            Value::Double(value) => JsonNumber::from_f64(value.get())
                .map(JsonValue::Number)
                .ok_or_else(|| platform_error("Double cannot be serialized to JSON", span))?,
            Value::String(value) | Value::Id(value) => JsonValue::String(value.clone()),
            Value::Date(value) => JsonValue::String(value.format("%Y-%m-%d").to_string()),
            Value::Datetime(value) => {
                JsonValue::String(value.format("%Y-%m-%dT%H:%M:%S.000Z").to_string())
            }
            Value::Time(value) => JsonValue::String(value.format("%H:%M:%S%.3f").to_string()),
            Value::Enum { class_id, ordinal } => JsonValue::String(
                self.classes()[class_id.index()].enum_constants[*ordinal]
                    .spelling
                    .clone(),
            ),
            Value::TypeLiteral(ty) => JsonValue::String(ty.apex_name()),
            Value::Collection(id) => {
                return self.collection_to_json(*id, span, depth, traversal);
            }
            Value::Platform(id) => match self.store.platform(*id) {
                PlatformValue::Blob(bytes) => JsonValue::String(BASE64.encode(bytes)),
                _ => {
                    return Err(platform_error(
                        format!(
                            "{} is not supported by JSON.serialize in compatibility profile `{}`",
                            self.store.platform(*id).ty().apex_name(),
                            self.execution_context.compatibility_profile().identity()
                        ),
                        span,
                    ));
                }
            },
            Value::Exception(exception) => JsonValue::String(exception.message.clone()),
            Value::Void => return Err(platform_error("cannot serialize void", span)),
            Value::SObject(_) | Value::AggregateResult(_) | Value::Object(_) => {
                unreachable!("handled before entering the structural JSON match")
            }
        })
    }

    fn object_to_json(
        &self,
        id: super::ObjectId,
        span: Span,
        depth: usize,
        traversal: &mut ValueGraphTraversal,
    ) -> Result<JsonValue, Diagnostic> {
        traversal
            .visit_node(depth)
            .map_err(|error| json_traversal_error(error, span))?;
        let identity = GraphIdentity::Object(id);
        traversal
            .enter_identity(identity)
            .map_err(|error| json_traversal_error(error, span))?;

        let fields = {
            let instance = self.store.object(id);
            let metadata = self
                .program()
                .class_metadata(crate::hir::ClassId::from_index(instance.class_id));
            let mut fields = Vec::new();
            for class_id in &metadata.lineage_base_first {
                for target in &self.program().class_metadata(*class_id).instance_slots {
                    let member = &self.classes()[target.class_id].members[target.member_id];
                    let (name, transient) = match member {
                        crate::ast::ClassMember::Field(field) => (
                            field.name.spelling.clone(),
                            field.modifiers.contains(&crate::ast::Modifier::Transient),
                        ),
                        crate::ast::ClassMember::Property(property) => (
                            property.name.spelling.clone(),
                            property
                                .modifiers
                                .contains(&crate::ast::Modifier::Transient),
                        ),
                        _ => unreachable!("instance slot metadata refers to a value member"),
                    };
                    if !transient {
                        let value = instance
                            .fields
                            .get(target)
                            .expect("allocated instance field has a runtime slot")
                            .value
                            .clone();
                        fields.push((name, value));
                    }
                }
            }
            fields
        };

        let result = (|| {
            let mut object = JsonMap::new();
            for (name, value) in fields {
                traversal
                    .visit_element()
                    .map_err(|error| json_traversal_error(error, span))?;
                object.insert(
                    name,
                    self.value_to_json_inner(&value, span, depth + 1, traversal)?,
                );
            }
            Ok(JsonValue::Object(object))
        })();
        traversal.leave_identity(identity);
        result
    }

    fn collection_to_json(
        &self,
        id: super::CollectionId,
        span: Span,
        depth: usize,
        traversal: &mut ValueGraphTraversal,
    ) -> Result<JsonValue, Diagnostic> {
        let identity = GraphIdentity::Collection(id);
        traversal
            .enter_identity(identity)
            .map_err(|error| json_traversal_error(error, span))?;
        let result = (|| match self.collection(id) {
            Collection::List { elements, .. } | Collection::Set { elements, .. } => {
                let mut values = Vec::new();
                for value in elements {
                    traversal
                        .visit_element()
                        .map_err(|error| json_traversal_error(error, span))?;
                    values.push(self.value_to_json_inner(value, span, depth + 1, traversal)?);
                }
                Ok(JsonValue::Array(values))
            }
            Collection::Map { entries, .. } => {
                let mut object = JsonMap::new();
                for (key, value) in entries {
                    traversal
                        .visit_element()
                        .map_err(|error| json_traversal_error(error, span))?;
                    traversal
                        .visit_node(depth + 1)
                        .map_err(|error| json_traversal_error(error, span))?;
                    let Value::String(key) = key else {
                        return Err(platform_error("JSON object maps require String keys", span));
                    };
                    object.insert(
                        key.clone(),
                        self.value_to_json_inner(value, span, depth + 1, traversal)?,
                    );
                }
                Ok(JsonValue::Object(object))
            }
        })();
        traversal.leave_identity(identity);
        result
    }

    fn typed_json_to_value(
        &mut self,
        value: JsonValue,
        target: &TypeName,
        span: Span,
        depth: usize,
        state: &mut TypedJsonState,
    ) -> Result<Value, Diagnostic> {
        state.visit(depth, span)?;
        if value.is_null() {
            return Ok(Value::Null(Some(target.clone())));
        }
        if is_typed_json_scalar(target) {
            return typed_json_scalar_value(value, target, span);
        }
        match target {
            TypeName::Object => self.bounded_untyped_json(value, span, depth, state),
            TypeName::List(element) | TypeName::Set(element) => {
                let JsonValue::Array(values) = value else {
                    return Err(typed_json_mismatch(target, span));
                };
                let elements = values
                    .into_iter()
                    .map(|value| self.typed_json_to_value(value, element, span, depth + 1, state))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self
                    .store
                    .allocate_collection(if matches!(target, TypeName::List(_)) {
                        Collection::List {
                            element_type: (**element).clone(),
                            elements,
                            iteration_depth: 0,
                        }
                    } else {
                        Collection::Set {
                            element_type: (**element).clone(),
                            elements,
                            iteration_depth: 0,
                        }
                    }))
            }
            TypeName::Map(key, value_type) if **key == TypeName::String => {
                let JsonValue::Object(values) = value else {
                    return Err(typed_json_mismatch(target, span));
                };
                let entries = values
                    .into_iter()
                    .map(|(key, value)| {
                        self.typed_json_to_value(value, value_type, span, depth + 1, state)
                            .map(|value| (Value::String(key), value))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self.store.allocate_collection(Collection::Map {
                    key_type: TypeName::String,
                    value_type: (**value_type).clone(),
                    entries,
                }))
            }
            TypeName::Custom(name) => {
                let JsonValue::Object(values) = value else {
                    return Err(typed_json_mismatch(target, span));
                };
                self.typed_json_object(name, values, span, depth, state)
            }
            _ => Err(platform_error(
                format!(
                    "JSON.deserialize does not yet support target {}",
                    target.apex_name()
                ),
                span,
            )),
        }
    }

    fn bounded_untyped_json(
        &mut self,
        value: JsonValue,
        span: Span,
        depth: usize,
        state: &mut TypedJsonState,
    ) -> Result<Value, Diagnostic> {
        Ok(match value {
            JsonValue::Null => Value::Null(Some(TypeName::Object)),
            JsonValue::Bool(value) => Value::Boolean(value),
            JsonValue::String(value) => Value::String(value),
            JsonValue::Number(value) => {
                if let Some(integer) = value.as_i64() {
                    if i32::try_from(integer).is_ok() {
                        Value::Integer(integer)
                    } else {
                        Value::Long(integer)
                    }
                } else {
                    Value::Decimal(
                        Decimal::from_str(&value.to_string())
                            .map_err(|_| platform_error("JSON number is out of range", span))?,
                    )
                }
            }
            JsonValue::Array(values) => {
                let elements = values
                    .into_iter()
                    .map(|value| {
                        state.visit(depth + 1, span)?;
                        self.bounded_untyped_json(value, span, depth + 1, state)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.allocate(Collection::List {
                    element_type: TypeName::Object,
                    elements,
                    iteration_depth: 0,
                })
            }
            JsonValue::Object(values) => {
                let entries = values
                    .into_iter()
                    .map(|(key, value)| {
                        state.visit(depth + 1, span)?;
                        self.bounded_untyped_json(value, span, depth + 1, state)
                            .map(|value| (Value::String(key), value))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.allocate(Collection::Map {
                    key_type: TypeName::String,
                    value_type: TypeName::Object,
                    entries,
                })
            }
        })
    }

    fn typed_json_object(
        &mut self,
        name: &crate::ast::NamedType,
        values: JsonMap<String, JsonValue>,
        span: Span,
        depth: usize,
        state: &mut TypedJsonState,
    ) -> Result<Value, Diagnostic> {
        if let Some(object_id) = self
            .program()
            .schema()
            .object_index(crate::hir::schema_api_name(name))
        {
            return self.typed_json_sobject(object_id, values, span, depth, state);
        }
        let class_id = self.runtime_class_id(name).ok_or_else(|| {
            platform_error(
                format!("unknown JSON target class `{}`", name.spelling),
                span,
            )
        })?;
        let object = self.store.allocate_object(class_id);
        self.allocate_instance_fields(object, class_id);
        let metadata = self
            .program()
            .class_metadata(crate::hir::ClassId::from_index(class_id))
            .clone();
        let mut slots = Vec::new();
        for owner in metadata.lineage_base_first {
            for target in &self.program().class_metadata(owner).instance_slots {
                let member = &self.classes()[target.class_id].members[target.member_id];
                let (name, ty) = match member {
                    crate::ast::ClassMember::Field(field) => {
                        (field.name.canonical.clone(), field.ty.clone())
                    }
                    crate::ast::ClassMember::Property(property) => {
                        (property.name.canonical.clone(), property.ty.clone())
                    }
                    _ => unreachable!("instance slot metadata refers to a value member"),
                };
                slots.push((name, *target, ty));
            }
        }
        for (name, value) in values {
            let canonical = name.to_ascii_lowercase();
            let Some((_, target, ty)) = slots.iter().find(|(name, _, _)| name == &canonical) else {
                continue;
            };
            let value = self.typed_json_to_value(value, ty, span, depth + 1, state)?;
            self.store
                .object_mut(object)
                .fields
                .get_mut(target)
                .expect("allocated JSON target has a runtime slot")
                .value = value;
        }
        Ok(Value::Object(object))
    }

    fn typed_json_sobject(
        &mut self,
        object_id: usize,
        values: JsonMap<String, JsonValue>,
        span: Span,
        depth: usize,
        state: &mut TypedJsonState,
    ) -> Result<Value, Diagnostic> {
        let Value::SObject(record) = self.store.allocate_sobject(object_id) else {
            unreachable!("SObject allocation returns an SObject value")
        };
        for (name, value) in values {
            if name.eq_ignore_ascii_case("attributes") {
                continue;
            }
            let (field_id, ty) = {
                let object = self
                    .program()
                    .schema()
                    .object_at(object_id)
                    .expect("typed JSON object ID is valid");
                let Some(field_id) = object.field_index(&name) else {
                    continue;
                };
                let field = object
                    .field_at(field_id)
                    .expect("typed JSON field index is valid");
                (field_id, apex_field_type(field.data_type()))
            };
            let value = self.typed_json_to_value(value, &ty, span, depth + 1, state)?;
            self.store
                .sobject_mut(record)
                .fields
                .insert(field_id, value);
        }
        Ok(Value::SObject(record))
    }

    fn json_to_value(&mut self, value: JsonValue, span: Span) -> Result<Value, Diagnostic> {
        Ok(match value {
            JsonValue::Null => Value::Null(Some(TypeName::Object)),
            JsonValue::Bool(value) => Value::Boolean(value),
            JsonValue::String(value) => Value::String(value),
            JsonValue::Number(value) => {
                if let Some(integer) = value.as_i64() {
                    if i32::try_from(integer).is_ok() {
                        Value::Integer(integer)
                    } else {
                        Value::Long(integer)
                    }
                } else {
                    Value::Decimal(
                        Decimal::from_str(&value.to_string())
                            .map_err(|_| platform_error("JSON number is out of range", span))?,
                    )
                }
            }
            JsonValue::Array(values) => {
                let elements = values
                    .into_iter()
                    .map(|value| self.json_to_value(value, span))
                    .collect::<Result<Vec<_>, _>>()?;
                self.allocate(Collection::List {
                    element_type: TypeName::Object,
                    elements,
                    iteration_depth: 0,
                })
            }
            JsonValue::Object(values) => {
                let mut entries = Vec::with_capacity(values.len());
                for (key, value) in values {
                    entries.push((Value::String(key), self.json_to_value(value, span)?));
                }
                self.allocate(Collection::Map {
                    key_type: TypeName::String,
                    value_type: TypeName::Object,
                    entries,
                })
            }
        })
    }
}

pub(super) fn datetime_from_millis(millis: i64, span: Span) -> Result<DateTime<Utc>, Diagnostic> {
    Utc.timestamp_millis_opt(millis)
        .single()
        .ok_or_else(|| platform_error("Datetime milliseconds are out of range", span))
}

fn date_from_parts(year: i64, month: i64, day: i64, span: Span) -> Result<NaiveDate, Diagnostic> {
    let year =
        i32::try_from(year).map_err(|_| platform_error("Date year is out of range", span))?;
    let month =
        u32::try_from(month).map_err(|_| platform_error("Date month is out of range", span))?;
    let day = u32::try_from(day).map_err(|_| platform_error("Date day is out of range", span))?;
    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| platform_error("invalid Date components", span))
}

fn time_from_parts(
    hour: i64,
    minute: i64,
    second: i64,
    millisecond: i64,
    span: Span,
) -> Result<NaiveTime, Diagnostic> {
    let hour = u32::try_from(hour).map_err(|_| platform_error("invalid Time hour", span))?;
    let minute = u32::try_from(minute).map_err(|_| platform_error("invalid Time minute", span))?;
    let second = u32::try_from(second).map_err(|_| platform_error("invalid Time second", span))?;
    let millisecond =
        u32::try_from(millisecond).map_err(|_| platform_error("invalid Time millisecond", span))?;
    NaiveTime::from_hms_milli_opt(hour, minute, second, millisecond)
        .ok_or_else(|| platform_error("invalid Time components", span))
}

fn parse_date(value: &str, span: Span) -> Result<NaiveDate, Diagnostic> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| platform_error(format!("invalid Date `{value}`"), span))
}

fn parse_datetime(value: &str, span: Span) -> Result<DateTime<Utc>, Diagnostic> {
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map_err(|_| platform_error(format!("invalid Datetime `{value}`"), span))?;
    Ok(Utc.from_utc_datetime(&naive))
}

fn add_months(date: NaiveDate, months: i64) -> Option<NaiveDate> {
    if months >= 0 {
        date.checked_add_months(Months::new(u32::try_from(months).ok()?))
    } else {
        date.checked_sub_months(Months::new(u32::try_from(months.unsigned_abs()).ok()?))
    }
}

fn parse_json_datetime(value: &str, span: Span) -> Result<DateTime<Utc>, Diagnostic> {
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Ok(value.with_timezone(&Utc));
    }
    parse_datetime(value, span).map_err(|_| typed_json_mismatch(&TypeName::Datetime, span))
}

fn is_typed_json_scalar(target: &TypeName) -> bool {
    matches!(
        target,
        TypeName::String
            | TypeName::Boolean
            | TypeName::Integer
            | TypeName::Long
            | TypeName::Decimal
            | TypeName::Double
            | TypeName::Date
            | TypeName::Datetime
            | TypeName::Time
            | TypeName::Id
    )
}

fn typed_json_scalar_value(
    value: JsonValue,
    target: &TypeName,
    span: Span,
) -> Result<Value, Diagnostic> {
    match target {
        TypeName::String => match value {
            JsonValue::String(value) => Ok(Value::String(value)),
            _ => Err(typed_json_mismatch(target, span)),
        },
        TypeName::Boolean => match value {
            JsonValue::Bool(value) => Ok(Value::Boolean(value)),
            _ => Err(typed_json_mismatch(target, span)),
        },
        TypeName::Integer => value
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .map(|value| Value::Integer(i64::from(value)))
            .ok_or_else(|| typed_json_mismatch(target, span)),
        TypeName::Long => value
            .as_i64()
            .map(Value::Long)
            .ok_or_else(|| typed_json_mismatch(target, span)),
        TypeName::Decimal => match value {
            JsonValue::Number(value) => Decimal::from_str(&value.to_string())
                .map(Value::Decimal)
                .map_err(|_| typed_json_mismatch(target, span)),
            _ => Err(typed_json_mismatch(target, span)),
        },
        TypeName::Double => match value {
            JsonValue::Number(value) => value
                .as_f64()
                .and_then(ApexDouble::new)
                .map(Value::Double)
                .ok_or_else(|| typed_json_mismatch(target, span)),
            _ => Err(typed_json_mismatch(target, span)),
        },
        TypeName::Date => match value {
            JsonValue::String(value) => NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                .map(Value::Date)
                .map_err(|_| typed_json_mismatch(target, span)),
            _ => Err(typed_json_mismatch(target, span)),
        },
        TypeName::Datetime => match value {
            JsonValue::String(value) => parse_json_datetime(&value, span).map(Value::Datetime),
            _ => Err(typed_json_mismatch(target, span)),
        },
        TypeName::Time => match value {
            JsonValue::String(value) => NaiveTime::parse_from_str(&value, "%H:%M:%S%.f")
                .map(Value::Time)
                .map_err(|_| typed_json_mismatch(target, span)),
            _ => Err(typed_json_mismatch(target, span)),
        },
        TypeName::Id => match value {
            JsonValue::String(value) => validate_id(&value, span).map(Value::Id),
            _ => Err(typed_json_mismatch(target, span)),
        },
        _ => unreachable!("typed JSON scalar target is closed"),
    }
}

fn typed_json_mismatch(target: &TypeName, span: Span) -> Diagnostic {
    platform_error(
        format!(
            "JSON value cannot be converted to target {}",
            target.apex_name()
        ),
        span,
    )
}

fn expect_date(receiver: Option<Value>, span: Span) -> Result<NaiveDate, Diagnostic> {
    match receiver {
        Some(Value::Date(value)) => Ok(value),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn expect_datetime(receiver: Option<Value>, span: Span) -> Result<DateTime<Utc>, Diagnostic> {
    match receiver {
        Some(Value::Datetime(value)) => Ok(value),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn expect_time(receiver: Option<Value>, span: Span) -> Result<NaiveTime, Diagnostic> {
    match receiver {
        Some(Value::Time(value)) => Ok(value),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn expect_decimal(receiver: Option<Value>, span: Span) -> Result<Decimal, Diagnostic> {
    match receiver {
        Some(Value::Decimal(value)) => Ok(value),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn expect_id(receiver: Option<Value>, span: Span) -> Result<String, Diagnostic> {
    match receiver {
        Some(Value::Id(value)) => Ok(value),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn is_schema_intrinsic(intrinsic: PlatformIntrinsic) -> bool {
    use PlatformIntrinsic as P;
    matches!(
        intrinsic,
        P::SchemaGetGlobalDescribe
            | P::SObjectTypeGetDescribe
            | P::SObjectTypeGetName
            | P::SObjectTypeNewSObject
            | P::DescribeGetName
            | P::DescribeGetLocalName
            | P::DescribeGetLabel
            | P::DescribeGetLabelPlural
            | P::DescribeGetKeyPrefix
            | P::DescribeIsCustom
            | P::DescribeIsCustomSetting
            | P::DescribeIsAccessible
            | P::DescribeIsDeletable
            | P::DescribeIsUpdateable
            | P::SObjectFieldGetDescribe
            | P::SObjectFieldMapGetMap
            | P::FieldSetMapGetMap
            | P::DescribeFieldGetName
            | P::DescribeFieldGetLocalName
            | P::DescribeFieldGetLabel
            | P::DescribeFieldGetLength
            | P::DescribeFieldGetInlineHelpText
            | P::DescribeFieldGetRelationshipName
            | P::DescribeFieldGetSoapType
            | P::DescribeFieldGetType
            | P::DescribeFieldGetReferenceTo
            | P::DescribeFieldGetPicklistValues
            | P::DescribeFieldIsNameField
            | P::DescribeFieldIsSortable
            | P::DescribeFieldIsAccessible
            | P::FieldSetGetName
            | P::FieldSetGetLabel
            | P::FieldSetGetNamespace
            | P::FieldSetGetFields
            | P::FieldSetMemberGetFieldPath
            | P::FieldSetMemberGetLabel
            | P::FieldSetMemberGetSObjectField
            | P::PicklistEntryGetValue
    )
}

fn local_schema_name(api_name: &str) -> String {
    let segments = api_name.split("__").collect::<Vec<_>>();
    if segments.len() >= 3 {
        segments[1..].join("__")
    } else {
        api_name.to_owned()
    }
}

fn schema_label(api_name: &str) -> String {
    local_schema_name(api_name)
        .trim_end_matches("__c")
        .trim_end_matches("__e")
        .trim_end_matches("__mdt")
        .replace('_', " ")
}

fn schema_soap_type(field: &crate::platform::FieldSchema) -> crate::platform::PlatformEnum {
    use crate::platform::{FieldType, PlatformEnum, SoapType};
    let value = match field.data_type() {
        FieldType::Boolean => SoapType::Boolean,
        FieldType::Integer if field.display_type() == crate::platform::DisplayType::Double => {
            SoapType::Double
        }
        FieldType::Integer | FieldType::Summary { .. } => SoapType::Integer,
        FieldType::String => SoapType::String,
        FieldType::Date => SoapType::Date,
        FieldType::Datetime => SoapType::Datetime,
        FieldType::Id | FieldType::Reference { .. } | FieldType::MetadataRelationship { .. } => {
            SoapType::Id
        }
    };
    PlatformEnum::SoapType(value)
}

fn validate_id(value: &str, span: Span) -> Result<String, Diagnostic> {
    RecordId::parse(value.to_owned())
        .map(|id| id.as_str().to_owned())
        .map_err(|error| platform_error(error.to_string(), span))
}

fn same_record_id(left: &str, right: &str) -> bool {
    left.as_bytes().get(..15) == right.as_bytes().get(..15)
}

fn id_to_18(value: &str) -> String {
    if value.len() == 18 {
        return value.to_owned();
    }
    const CHECKSUM: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ012345";
    let mut suffix = String::with_capacity(3);
    for chunk in value.as_bytes().chunks(5) {
        let mut bits = 0usize;
        for (index, byte) in chunk.iter().enumerate() {
            if byte.is_ascii_uppercase() {
                bits |= 1 << index;
            }
        }
        suffix.push(char::from(CHECKSUM[bits]));
    }
    format!("{value}{suffix}")
}

fn matcher_id(receiver: Option<Value>, span: Span) -> Result<super::PlatformValueId, Diagnostic> {
    platform_id(receiver, span)
}

fn platform_id(receiver: Option<Value>, span: Span) -> Result<super::PlatformValueId, Diagnostic> {
    match receiver {
        Some(Value::Platform(id)) => Ok(id),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn value_span(arguments: &[EvaluatedArgument], fallback: Span) -> Span {
    arguments.first().map_or(fallback, |argument| argument.span)
}

fn request_span(arguments: &[EvaluatedArgument], fallback: Span) -> Span {
    value_span(arguments, fallback)
}

fn record_removed_field(
    removed_fields: &mut BTreeMap<String, Vec<String>>,
    object: &str,
    field: &str,
) {
    removed_fields
        .entry(object.to_owned())
        .or_default()
        .push(field.to_owned());
}

fn normalize_removed_fields(removed_fields: &mut BTreeMap<String, Vec<String>>) {
    for fields in removed_fields.values_mut() {
        fields.sort_by_key(|field| field.to_ascii_lowercase());
        fields.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    }
}

fn platform_error(message: impl Into<String>, span: Span) -> Diagnostic {
    runtime_exception("IllegalArgumentException", message, span)
}

fn json_traversal_error(error: TraversalError, span: Span) -> Diagnostic {
    let message = match error {
        TraversalError::Cycle => "JSON serialization does not support cyclic runtime values",
        TraversalError::DepthLimit => "JSON serialization exceeded the runtime value depth limit",
        TraversalError::NodeLimit => "JSON serialization exceeded the runtime value node limit",
        TraversalError::ElementLimit => {
            "JSON serialization exceeded the runtime value element limit"
        }
        TraversalError::OutputLimit => "JSON serialization exceeded the runtime value output limit",
    };
    platform_error(message, span)
}
