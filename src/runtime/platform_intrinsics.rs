use super::{
    Collection, EvaluatedArgument, Interpreter, PlatformHost, PlatformValue, Value,
    invalid_runtime_operands, runtime_exception,
};
use crate::{
    ast::TypeName, diagnostic::Diagnostic, hir::PlatformIntrinsic, platform::RecordId, span::Span,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{
    DateTime, Datelike, Duration, Months, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike,
    Utc,
};
use regex::Regex;
use rust_decimal::Decimal;
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::str::FromStr;

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
        match intrinsic {
            P::DateNewInstance => {
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
            P::DateValueOf => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value = expect_string(&value.value, value.span)?;
                parse_date(value, value_span(arguments, span)).map(Value::Date)
            }
            P::DateToday => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Date(
                    datetime_from_millis(self.host.now_millis(), span)?.date_naive(),
                ))
            }
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
                let value = expect_string(&value.value, value.span)?;
                parse_datetime(value, value_span(arguments, span)).map(Value::Datetime)
            }
            P::DatetimeGetTime => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Integer(
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
            P::TimeNewInstance => {
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
            P::TimeValueOf => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value = expect_string(&value.value, value.span)?;
                NaiveTime::parse_from_str(value, "%H:%M:%S%.f")
                    .map(Value::Time)
                    .map_err(|_| platform_error(format!("invalid Time `{value}`"), span))
            }
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
            P::DecimalValueOf => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value = expect_string(&value.value, value.span)?;
                Decimal::from_str(value)
                    .map(Value::Decimal)
                    .map_err(|_| platform_error(format!("invalid Decimal `{value}`"), span))
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
                Ok(Value::String(self.display_value(
                    &receiver.ok_or_else(|| invalid_runtime_operands(span))?,
                )))
            }
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
            P::SchemaGetGlobalDescribe => {
                expect_no_arguments(arguments, span)?;
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
            P::SObjectTypeGetDescribe => {
                expect_no_arguments(arguments, span)?;
                let object_id = self.expect_schema_object(receiver, false, span)?;
                Ok(self
                    .store
                    .allocate_platform(PlatformValue::DescribeSObject(object_id)))
            }
            P::DescribeGetName | P::DescribeGetKeyPrefix | P::DescribeIsCustom => {
                expect_no_arguments(arguments, span)?;
                let object_id = self.expect_schema_object(receiver, true, span)?;
                let object = self
                    .program()
                    .schema()
                    .object_at(object_id)
                    .expect("describe handle references schema object");
                Ok(match intrinsic {
                    P::DescribeGetName => Value::String(object.api_name().to_owned()),
                    P::DescribeGetKeyPrefix => Value::String(object.key_prefix().to_owned()),
                    P::DescribeIsCustom => Value::Boolean(object.api_name().ends_with("__c")),
                    _ => unreachable!(),
                })
            }
            P::TestStartTest => {
                expect_no_arguments(arguments, span)?;
                self.host.begin_test_window();
                Ok(Value::Void)
            }
            P::TestStopTest => {
                expect_no_arguments(arguments, span)?;
                self.host.end_test_window();
                self.drain_async_jobs(span)?;
                Ok(Value::Void)
            }
            P::TestIsRunningTest => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Boolean(true))
            }
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
            P::AsyncContextGetJobId | P::SchedulableContextGetTriggerId => {
                expect_no_arguments(arguments, span)?;
                let Some(Value::Platform(id)) = receiver else {
                    return Err(invalid_runtime_operands(span));
                };
                let PlatformValue::AsyncContext { job_id, .. } = self.store.platform(id) else {
                    return Err(invalid_runtime_operands(span));
                };
                Ok(Value::Id(job_id.clone()))
            }
            P::LimitsGetQueries
            | P::LimitsGetLimitQueries
            | P::LimitsGetDmlStatements
            | P::LimitsGetLimitDmlStatements
            | P::LimitsGetCallouts
            | P::LimitsGetLimitCallouts => {
                expect_no_arguments(arguments, span)?;
                let usage = self.host.limit_usage();
                Ok(Value::Integer(match intrinsic {
                    P::LimitsGetQueries => usage.queries,
                    P::LimitsGetLimitQueries => 100,
                    P::LimitsGetDmlStatements => usage.dml_statements,
                    P::LimitsGetLimitDmlStatements => 150,
                    P::LimitsGetCallouts => usage.callouts,
                    P::LimitsGetLimitCallouts => 100,
                    _ => unreachable!(),
                }))
            }
            P::UserInfoGetUserId | P::UserInfoGetUserName => {
                expect_no_arguments(arguments, span)?;
                let user = self.host.user_context();
                Ok(if intrinsic == P::UserInfoGetUserId {
                    Value::Id(validate_id(&user.user_id, span)?)
                } else {
                    Value::String(user.username)
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
            P::HttpSend => {
                let Some(Value::Platform(client_id)) = receiver else {
                    return Err(invalid_runtime_operands(span));
                };
                if !matches!(self.store.platform(client_id), PlatformValue::Http) {
                    return Err(invalid_runtime_operands(span));
                }
                let [request] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let Value::Platform(request_id) = request.value else {
                    return Err(invalid_runtime_operands(request.span));
                };
                let PlatformValue::HttpRequest(request) = self.store.platform(request_id) else {
                    return Err(invalid_runtime_operands(request.span));
                };
                let response = self.host.send_http(request).map_err(|message| {
                    runtime_exception("CalloutException", message, request_span(arguments, span))
                })?;
                Ok(self
                    .store
                    .allocate_platform(PlatformValue::HttpResponse(response)))
            }
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

    fn value_to_json(&self, value: &Value, span: Span) -> Result<JsonValue, Diagnostic> {
        Ok(match value {
            Value::Null(_) => JsonValue::Null,
            Value::Boolean(value) => JsonValue::Bool(*value),
            Value::Integer(value) => JsonValue::Number((*value).into()),
            Value::Decimal(value) => JsonValue::Number(
                JsonNumber::from_str(&value.normalize().to_string())
                    .map_err(|_| platform_error("Decimal cannot be serialized to JSON", span))?,
            ),
            Value::String(value) | Value::Id(value) => JsonValue::String(value.clone()),
            Value::Date(value) => JsonValue::String(value.format("%Y-%m-%d").to_string()),
            Value::Datetime(value) => {
                JsonValue::String(value.format("%Y-%m-%dT%H:%M:%S.000Z").to_string())
            }
            Value::Time(value) => JsonValue::String(value.format("%H:%M:%S%.3f").to_string()),
            Value::Collection(id) => match self.collection(*id) {
                Collection::List { elements, .. } | Collection::Set { elements, .. } => {
                    JsonValue::Array(
                        elements
                            .iter()
                            .map(|value| self.value_to_json(value, span))
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                }
                Collection::Map { entries, .. } => {
                    let mut object = JsonMap::new();
                    for (key, value) in entries {
                        let Value::String(key) = key else {
                            return Err(platform_error(
                                "JSON object maps require String keys",
                                span,
                            ));
                        };
                        object.insert(key.clone(), self.value_to_json(value, span)?);
                    }
                    JsonValue::Object(object)
                }
            },
            Value::Platform(id) => match self.store.platform(*id) {
                PlatformValue::Blob(bytes) => JsonValue::String(BASE64.encode(bytes)),
                _ => {
                    return Err(platform_error(
                        format!(
                            "{} is not supported by JSON.serialize in compatibility profile `m10-common`",
                            self.store.platform(*id).ty().apex_name()
                        ),
                        span,
                    ));
                }
            },
            Value::SObject(_) | Value::AggregateResult(_) | Value::Object(_) => {
                JsonValue::String(self.display_value(value))
            }
            Value::Exception(exception) => JsonValue::String(exception.message.clone()),
            Value::Void => return Err(platform_error("cannot serialize void", span)),
        })
    }

    fn json_to_value(&mut self, value: JsonValue, span: Span) -> Result<Value, Diagnostic> {
        Ok(match value {
            JsonValue::Null => Value::Null(Some(TypeName::Object)),
            JsonValue::Bool(value) => Value::Boolean(value),
            JsonValue::String(value) => Value::String(value),
            JsonValue::Number(value) => {
                if let Some(integer) = value.as_i64() {
                    Value::Integer(integer)
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

fn validate_id(value: &str, span: Span) -> Result<String, Diagnostic> {
    RecordId::parse(value.to_owned())
        .map(|id| id.as_str().to_owned())
        .map_err(|error| platform_error(error.to_string(), span))
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

fn platform_error(message: impl Into<String>, span: Span) -> Diagnostic {
    runtime_exception("IllegalArgumentException", message, span)
}
