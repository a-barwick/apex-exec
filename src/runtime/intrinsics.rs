use super::platform_intrinsics::datetime_from_millis;
use super::{
    Collection, CollectionId, DebugEvent, EvaluatedArgument, Interpreter, PlatformHost, Value,
    checked_list_index, integer_overflow, invalid_runtime_operands, runtime_exception, typed_value,
};
use crate::{
    ast::{Expression, Identifier},
    diagnostic::Diagnostic,
    hir::{
        ExceptionIntrinsic, IntrinsicId, ListIntrinsic, MapIntrinsic, MathIntrinsic, SetIntrinsic,
        StaticStringIntrinsic, StringIntrinsic, SystemIntrinsic,
    },
    span::Span,
};
use std::cmp::Ordering;

impl<'program, H: PlatformHost> Interpreter<'program, H> {
    pub(super) fn evaluate_intrinsic_call(
        &mut self,
        intrinsic: IntrinsicId,
        receiver: &Expression,
        method: &Identifier,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let receiver_span = receiver.span();
        let receiver = if intrinsic.is_static() {
            None
        } else {
            Some(self.evaluate(receiver)?)
        };
        let arguments = self.evaluate_arguments(arguments)?;
        match intrinsic {
            IntrinsicId::StaticString(intrinsic) => {
                self.call_static_string(intrinsic, &arguments, span)
            }
            IntrinsicId::Math(intrinsic) => self.call_math(intrinsic, &arguments, span),
            IntrinsicId::System(intrinsic) => self.call_system(intrinsic, &arguments, span),
            IntrinsicId::String(intrinsic) => match receiver {
                Some(Value::String(receiver)) => {
                    self.call_string_instance(receiver, intrinsic, &arguments, span)
                }
                Some(Value::Null(_)) => Err(null_method_receiver(method, receiver_span)),
                _ => Err(invalid_runtime_operands(receiver_span)),
            },
            IntrinsicId::Exception(intrinsic) => match receiver {
                Some(Value::Exception(exception)) => {
                    self.call_exception_instance(&exception, intrinsic, &arguments, span)
                }
                Some(Value::Null(_)) => Err(null_method_receiver(method, receiver_span)),
                _ => Err(invalid_runtime_operands(receiver_span)),
            },
            IntrinsicId::List(intrinsic) => match receiver {
                Some(Value::Collection(id))
                    if matches!(self.collection(id), Collection::List { .. }) =>
                {
                    self.call_list(id, intrinsic, &arguments, span)
                }
                Some(Value::Null(_)) => Err(null_method_receiver(method, receiver_span)),
                _ => Err(invalid_runtime_operands(receiver_span)),
            },
            IntrinsicId::Set(intrinsic) => match receiver {
                Some(Value::Collection(id))
                    if matches!(self.collection(id), Collection::Set { .. }) =>
                {
                    self.call_set(id, intrinsic, &arguments, span)
                }
                Some(Value::Null(_)) => Err(null_method_receiver(method, receiver_span)),
                _ => Err(invalid_runtime_operands(receiver_span)),
            },
            IntrinsicId::Map(intrinsic) => match receiver {
                Some(Value::Collection(id))
                    if matches!(self.collection(id), Collection::Map { .. }) =>
                {
                    self.call_map(id, intrinsic, &arguments, span)
                }
                Some(Value::Null(_)) => Err(null_method_receiver(method, receiver_span)),
                _ => Err(invalid_runtime_operands(receiver_span)),
            },
            IntrinsicId::Platform(intrinsic) => {
                self.call_platform(intrinsic, receiver, &arguments, span)
            }
        }
    }

    fn call_static_string(
        &mut self,
        intrinsic: StaticStringIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match intrinsic {
            StaticStringIntrinsic::ValueOf => {
                let [argument] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                if matches!(argument.value, Value::Void) {
                    return Err(Diagnostic::new(
                        "cannot convert void to String",
                        argument.span,
                    ));
                }
                Ok(Value::String(self.display_value(&argument.value)))
            }
            StaticStringIntrinsic::Join => {
                let [iterable, separator] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let separator = expect_string(&separator.value, separator.span)?;
                let id = self.expect_collection_id(iterable.value.clone(), iterable.span)?;
                let elements = self.sequence_snapshot(id, iterable.span)?;
                let joined = elements
                    .iter()
                    .map(|value| self.display_value(value))
                    .collect::<Vec<_>>()
                    .join(separator);
                Ok(Value::String(joined))
            }
            StaticStringIntrinsic::IsBlank
            | StaticStringIntrinsic::IsNotBlank
            | StaticStringIntrinsic::IsEmpty
            | StaticStringIntrinsic::IsNotEmpty => {
                let [argument] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let (empty, blank) = match &argument.value {
                    Value::String(value) => (value.is_empty(), value.trim().is_empty()),
                    Value::Null(_) => (true, true),
                    _ => return Err(invalid_runtime_operands(argument.span)),
                };
                let value = match intrinsic {
                    StaticStringIntrinsic::IsBlank => blank,
                    StaticStringIntrinsic::IsNotBlank => !blank,
                    StaticStringIntrinsic::IsEmpty => empty,
                    StaticStringIntrinsic::IsNotEmpty => !empty,
                    StaticStringIntrinsic::ValueOf | StaticStringIntrinsic::Join => unreachable!(),
                };
                Ok(Value::Boolean(value))
            }
        }
    }

    fn call_math(
        &mut self,
        intrinsic: MathIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match intrinsic {
            MathIntrinsic::Abs => {
                let [argument] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                expect_integer(&argument.value, argument.span)?
                    .checked_abs()
                    .map(Value::Integer)
                    .ok_or_else(|| integer_overflow(span))
            }
            MathIntrinsic::Max | MathIntrinsic::Min => {
                let [left, right] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let left = expect_integer(&left.value, left.span)?;
                let right = expect_integer(&right.value, right.span)?;
                Ok(Value::Integer(if intrinsic == MathIntrinsic::Max {
                    left.max(right)
                } else {
                    left.min(right)
                }))
            }
            MathIntrinsic::Mod => {
                let [left, right] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let left = expect_integer(&left.value, left.span)?;
                let right_span = right.span;
                let right = expect_integer(&right.value, right_span)?;
                if right == 0 {
                    return Err(runtime_exception(
                        "MathException",
                        "Math.mod divisor cannot be zero",
                        right_span,
                    ));
                }
                left.checked_rem(right)
                    .map(Value::Integer)
                    .ok_or_else(|| integer_overflow(span))
            }
            MathIntrinsic::Random => {
                let [] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let numerator = rust_decimal::Decimal::from(self.host.random_u64() >> 1);
                let denominator = rust_decimal::Decimal::from(u64::MAX >> 1);
                Ok(Value::Decimal(numerator / denominator))
            }
        }
    }

    fn call_system(
        &mut self,
        intrinsic: SystemIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match intrinsic {
            SystemIntrinsic::Debug => {
                let [argument] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                if matches!(argument.value, Value::Void) {
                    return Err(Diagnostic::new("cannot debug void", argument.span));
                }
                let message = self.display_value(&argument.value);
                self.host.debug(DebugEvent { message });
                Ok(Value::Void)
            }
            SystemIntrinsic::Assert => {
                let ([condition] | [condition, _]) = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                if expect_boolean(&condition.value, condition.span)? {
                    return Ok(Value::Void);
                }
                let message = arguments
                    .get(1)
                    .map(|message| self.display_value(&message.value));
                Err(runtime_exception(
                    "AssertException",
                    assertion_failure_message(message.as_deref(), "condition is false"),
                    condition.span,
                ))
            }
            SystemIntrinsic::AssertEquals | SystemIntrinsic::AssertNotEquals => {
                let ([expected, actual] | [expected, actual, _]) = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let equal = self.values_equal(&expected.value, &actual.value);
                let passed = if intrinsic == SystemIntrinsic::AssertEquals {
                    equal
                } else {
                    !equal
                };
                if passed {
                    return Ok(Value::Void);
                }
                let detail = if intrinsic == SystemIntrinsic::AssertEquals {
                    format!(
                        "expected {}, actual {}",
                        self.display_value(&expected.value),
                        self.display_value(&actual.value)
                    )
                } else {
                    format!("did not expect {}", self.display_value(&actual.value))
                };
                let message = arguments
                    .get(2)
                    .map(|message| self.display_value(&message.value));
                Err(runtime_exception(
                    "AssertException",
                    assertion_failure_message(message.as_deref(), &detail),
                    actual.span,
                ))
            }
            SystemIntrinsic::Now => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Datetime(datetime_from_millis(
                    self.host.now_millis(),
                    span,
                )?))
            }
            SystemIntrinsic::Today => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Date(
                    datetime_from_millis(self.host.now_millis(), span)?.date_naive(),
                ))
            }
            SystemIntrinsic::CurrentTimeMillis => {
                expect_no_arguments(arguments, span)?;
                Ok(Value::Integer(self.host.now_millis()))
            }
        }
    }

    fn call_exception_instance(
        &self,
        exception: &Diagnostic,
        intrinsic: ExceptionIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        expect_no_arguments(arguments, span)?;
        match intrinsic {
            ExceptionIntrinsic::GetMessage => Ok(Value::String(exception.message.clone())),
            ExceptionIntrinsic::GetTypeName => Ok(Value::String(
                exception
                    .exception_type
                    .clone()
                    .unwrap_or_else(|| "Exception".to_owned()),
            )),
            ExceptionIntrinsic::GetStackTraceString => Ok(Value::String(
                exception
                    .stack_trace
                    .iter()
                    .map(|frame| {
                        format!(
                            "{} @ bytes {}..{}",
                            frame.method, frame.span.start, frame.span.end
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            )),
        }
    }

    fn call_string_instance(
        &mut self,
        receiver: String,
        intrinsic: StringIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match intrinsic {
            StringIntrinsic::Length => {
                let [] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let length = i64::try_from(receiver.encode_utf16().count()).map_err(|_| {
                    runtime_exception("StringException", "String length is too large", span)
                })?;
                Ok(Value::Integer(length))
            }
            StringIntrinsic::Contains
            | StringIntrinsic::StartsWith
            | StringIntrinsic::EndsWith
            | StringIntrinsic::Equals
            | StringIntrinsic::EqualsIgnoreCase
            | StringIntrinsic::IndexOf => {
                let [argument] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                if matches!(
                    intrinsic,
                    StringIntrinsic::Equals | StringIntrinsic::EqualsIgnoreCase
                ) && matches!(argument.value, Value::Null(_))
                {
                    return Ok(Value::Boolean(false));
                }
                let argument = expect_string(&argument.value, argument.span)?;
                match intrinsic {
                    StringIntrinsic::Contains => Ok(Value::Boolean(receiver.contains(argument))),
                    StringIntrinsic::StartsWith => {
                        Ok(Value::Boolean(receiver.starts_with(argument)))
                    }
                    StringIntrinsic::EndsWith => Ok(Value::Boolean(receiver.ends_with(argument))),
                    StringIntrinsic::Equals => Ok(Value::Boolean(receiver == argument)),
                    StringIntrinsic::EqualsIgnoreCase => Ok(Value::Boolean(
                        receiver.to_lowercase() == argument.to_lowercase(),
                    )),
                    StringIntrinsic::IndexOf => {
                        let index = receiver.find(argument).map_or(-1, |byte_index| {
                            i64::try_from(receiver[..byte_index].encode_utf16().count())
                                .expect("String index fits in i64 when String length does")
                        });
                        Ok(Value::Integer(index))
                    }
                    _ => unreachable!(),
                }
            }
            StringIntrinsic::Substring => self.string_substring(&receiver, arguments, span),
            StringIntrinsic::Trim | StringIntrinsic::ToLowerCase | StringIntrinsic::ToUpperCase => {
                let [] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value = match intrinsic {
                    StringIntrinsic::Trim => receiver.trim().to_owned(),
                    StringIntrinsic::ToLowerCase => receiver.to_lowercase(),
                    StringIntrinsic::ToUpperCase => receiver.to_uppercase(),
                    _ => unreachable!(),
                };
                Ok(Value::String(value))
            }
            StringIntrinsic::Replace => {
                let [target, replacement] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let target = expect_string(&target.value, target.span)?;
                let replacement = expect_string(&replacement.value, replacement.span)?;
                Ok(Value::String(receiver.replace(target, replacement)))
            }
        }
    }

    fn string_substring(
        &self,
        receiver: &str,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let utf16_length = receiver.encode_utf16().count();
        let (start, end, error_span) = match arguments {
            [start] => (
                nonnegative_usize(&start.value, start.span, "String index")?,
                utf16_length,
                start.span,
            ),
            [start, end] => (
                nonnegative_usize(&start.value, start.span, "String index")?,
                nonnegative_usize(&end.value, end.span, "String index")?,
                start.span.merge(end.span),
            ),
            _ => return Err(invalid_call_arguments(span)),
        };
        if start > end || end > utf16_length {
            return Err(runtime_exception(
                "StringException",
                format!(
                    "String substring range {start}..{end} is out of bounds for length {utf16_length}"
                ),
                error_span,
            ));
        }
        let start_byte = utf16_byte_index(receiver, start).ok_or_else(|| {
            runtime_exception(
                "StringException",
                "String index splits a UTF-16 surrogate pair",
                error_span,
            )
        })?;
        let end_byte = utf16_byte_index(receiver, end).ok_or_else(|| {
            runtime_exception(
                "StringException",
                "String index splits a UTF-16 surrogate pair",
                error_span,
            )
        })?;
        Ok(Value::String(receiver[start_byte..end_byte].to_owned()))
    }

    fn call_list(
        &mut self,
        id: CollectionId,
        intrinsic: ListIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match intrinsic {
            ListIntrinsic::Add => match arguments {
                [value] => {
                    self.ensure_collection_mutable(id, span)?;
                    let element_type = self.list_type(id).clone();
                    let value = typed_value(value.value.clone(), &element_type);
                    let Collection::List { elements, .. } = self.collection_mut(id) else {
                        unreachable!()
                    };
                    elements.push(value);
                    Ok(Value::Void)
                }
                [index, value] => {
                    self.ensure_collection_mutable(id, span)?;
                    let index_value = expect_integer(&index.value, index.span)?;
                    let (element_type, size) = match self.collection(id) {
                        Collection::List {
                            element_type,
                            elements,
                            ..
                        } => (element_type.clone(), elements.len()),
                        _ => unreachable!(),
                    };
                    let index = checked_list_index(index_value, size, true, index.span)?;
                    let value = typed_value(value.value.clone(), &element_type);
                    let Collection::List { elements, .. } = self.collection_mut(id) else {
                        unreachable!()
                    };
                    elements.insert(index, value);
                    Ok(Value::Void)
                }
                _ => Err(invalid_call_arguments(span)),
            },
            ListIntrinsic::AddAll => {
                let [source] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source_id = self.expect_collection_id(source.value.clone(), source.span)?;
                let source_elements = self.sequence_snapshot(source_id, source.span)?;
                self.ensure_collection_mutable(id, span)?;
                let element_type = self.list_type(id).clone();
                let values: Vec<Value> = source_elements
                    .into_iter()
                    .map(|value| typed_value(value, &element_type))
                    .collect();
                let Collection::List { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                elements.extend(values);
                Ok(Value::Void)
            }
            ListIntrinsic::Clear => {
                expect_no_arguments(arguments, span)?;
                self.ensure_collection_mutable(id, span)?;
                let Collection::List { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                elements.clear();
                Ok(Value::Void)
            }
            ListIntrinsic::Clone => {
                expect_no_arguments(arguments, span)?;
                let (element_type, elements) = match self.collection(id) {
                    Collection::List {
                        element_type,
                        elements,
                        ..
                    } => (element_type.clone(), elements.clone()),
                    _ => unreachable!(),
                };
                Ok(self.allocate(Collection::List {
                    element_type,
                    elements,
                    iteration_depth: 0,
                }))
            }
            ListIntrinsic::Contains => {
                let [needle] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let Collection::List { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(
                    elements
                        .iter()
                        .any(|value| self.values_equal(value, &needle.value)),
                ))
            }
            ListIntrinsic::Get => {
                let [index] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.list_get(id, &index.value, index.span)
            }
            ListIntrinsic::IndexOf => {
                let [needle] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let Collection::List { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                let index = elements
                    .iter()
                    .position(|value| self.values_equal(value, &needle.value))
                    .map_or(-1, |index| i64::try_from(index).unwrap_or(i64::MAX));
                Ok(Value::Integer(index))
            }
            ListIntrinsic::IsEmpty => {
                expect_no_arguments(arguments, span)?;
                let Collection::List { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(elements.is_empty()))
            }
            ListIntrinsic::Remove => {
                let [index] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.ensure_collection_mutable(id, span)?;
                let index_value = expect_integer(&index.value, index.span)?;
                let size = match self.collection(id) {
                    Collection::List { elements, .. } => elements.len(),
                    _ => unreachable!(),
                };
                let index = checked_list_index(index_value, size, false, index.span)?;
                let Collection::List { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                Ok(elements.remove(index))
            }
            ListIntrinsic::Set => {
                let [index, value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.ensure_collection_mutable(id, span)?;
                let index_value = expect_integer(&index.value, index.span)?;
                let (element_type, size) = match self.collection(id) {
                    Collection::List {
                        element_type,
                        elements,
                        ..
                    } => (element_type.clone(), elements.len()),
                    _ => unreachable!(),
                };
                let index = checked_list_index(index_value, size, false, index.span)?;
                let value = typed_value(value.value.clone(), &element_type);
                let Collection::List { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                elements[index] = value;
                Ok(Value::Void)
            }
            ListIntrinsic::Size => {
                expect_no_arguments(arguments, span)?;
                let Collection::List { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Integer(collection_size(elements.len(), span)?))
            }
            ListIntrinsic::Sort => {
                expect_no_arguments(arguments, span)?;
                self.ensure_collection_mutable(id, span)?;
                let mut elements = match self.collection(id) {
                    Collection::List { elements, .. } => elements.clone(),
                    _ => unreachable!(),
                };
                sort_primitive_values(&mut elements, span)?;
                let Collection::List {
                    elements: stored, ..
                } = self.collection_mut(id)
                else {
                    unreachable!()
                };
                *stored = elements;
                Ok(Value::Void)
            }
        }
    }

    fn call_set(
        &mut self,
        id: CollectionId,
        intrinsic: SetIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match intrinsic {
            SetIntrinsic::Add => {
                let [value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.ensure_collection_mutable(id, span)?;
                let element_type = self.set_type(id).clone();
                let value = typed_value(value.value.clone(), &element_type);
                let changed = {
                    let Collection::Set { elements, .. } = self.collection(id) else {
                        unreachable!()
                    };
                    !elements
                        .iter()
                        .any(|existing| self.values_equal(existing, &value))
                };
                if changed {
                    let Collection::Set { elements, .. } = self.collection_mut(id) else {
                        unreachable!()
                    };
                    elements.push(value);
                }
                Ok(Value::Boolean(changed))
            }
            SetIntrinsic::AddAll => {
                let [source] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source_id = self.expect_collection_id(source.value.clone(), source.span)?;
                let source_elements = self.sequence_snapshot(source_id, source.span)?;
                self.ensure_collection_mutable(id, span)?;
                let element_type = self.set_type(id).clone();
                let mut current = match self.collection(id) {
                    Collection::Set { elements, .. } => elements.clone(),
                    _ => unreachable!(),
                };
                let original_len = current.len();
                for value in source_elements {
                    let value = typed_value(value, &element_type);
                    if !current
                        .iter()
                        .any(|existing| self.values_equal(existing, &value))
                    {
                        current.push(value);
                    }
                }
                let changed = current.len() != original_len;
                let Collection::Set { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                *elements = current;
                Ok(Value::Boolean(changed))
            }
            SetIntrinsic::Clear => {
                expect_no_arguments(arguments, span)?;
                self.ensure_collection_mutable(id, span)?;
                let Collection::Set { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                elements.clear();
                Ok(Value::Void)
            }
            SetIntrinsic::Clone => {
                expect_no_arguments(arguments, span)?;
                let (element_type, elements) = match self.collection(id) {
                    Collection::Set {
                        element_type,
                        elements,
                        ..
                    } => (element_type.clone(), elements.clone()),
                    _ => unreachable!(),
                };
                Ok(self.allocate(Collection::Set {
                    element_type,
                    elements,
                    iteration_depth: 0,
                }))
            }
            SetIntrinsic::Contains => {
                let [needle] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let Collection::Set { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(
                    elements
                        .iter()
                        .any(|value| self.values_equal(value, &needle.value)),
                ))
            }
            SetIntrinsic::ContainsAll => {
                let [source] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source_id = self.expect_collection_id(source.value.clone(), source.span)?;
                let source = self.sequence_snapshot(source_id, source.span)?;
                let Collection::Set { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(source.iter().all(|needle| {
                    elements
                        .iter()
                        .any(|value| self.values_equal(value, needle))
                })))
            }
            SetIntrinsic::IsEmpty => {
                expect_no_arguments(arguments, span)?;
                let Collection::Set { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(elements.is_empty()))
            }
            SetIntrinsic::Remove => {
                let [needle] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                self.ensure_collection_mutable(id, span)?;
                let position = {
                    let Collection::Set { elements, .. } = self.collection(id) else {
                        unreachable!()
                    };
                    elements
                        .iter()
                        .position(|value| self.values_equal(value, &needle.value))
                };
                if let Some(position) = position {
                    let Collection::Set { elements, .. } = self.collection_mut(id) else {
                        unreachable!()
                    };
                    elements.remove(position);
                    Ok(Value::Boolean(true))
                } else {
                    Ok(Value::Boolean(false))
                }
            }
            SetIntrinsic::RemoveAll | SetIntrinsic::RetainAll => {
                let [source] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source_id = self.expect_collection_id(source.value.clone(), source.span)?;
                let source = self.sequence_snapshot(source_id, source.span)?;
                self.ensure_collection_mutable(id, span)?;
                let current = match self.collection(id) {
                    Collection::Set { elements, .. } => elements.clone(),
                    _ => unreachable!(),
                };
                let retain_matches = intrinsic == SetIntrinsic::RetainAll;
                let retained: Vec<Value> = current
                    .iter()
                    .filter(|value| {
                        let found = source.iter().any(|needle| self.values_equal(value, needle));
                        found == retain_matches
                    })
                    .cloned()
                    .collect();
                let changed = retained.len() != current.len();
                let Collection::Set { elements, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                *elements = retained;
                Ok(Value::Boolean(changed))
            }
            SetIntrinsic::Size => {
                expect_no_arguments(arguments, span)?;
                let Collection::Set { elements, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Integer(collection_size(elements.len(), span)?))
            }
        }
    }

    fn call_map(
        &mut self,
        id: CollectionId,
        intrinsic: MapIntrinsic,
        arguments: &[EvaluatedArgument],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match intrinsic {
            MapIntrinsic::Clear => {
                expect_no_arguments(arguments, span)?;
                self.ensure_collection_mutable(id, span)?;
                let Collection::Map { entries, .. } = self.collection_mut(id) else {
                    unreachable!()
                };
                entries.clear();
                Ok(Value::Void)
            }
            MapIntrinsic::Clone => {
                expect_no_arguments(arguments, span)?;
                let (key_type, value_type, entries) = match self.collection(id) {
                    Collection::Map {
                        key_type,
                        value_type,
                        entries,
                    } => (key_type.clone(), value_type.clone(), entries.clone()),
                    _ => unreachable!(),
                };
                Ok(self.allocate(Collection::Map {
                    key_type,
                    value_type,
                    entries,
                }))
            }
            MapIntrinsic::ContainsKey => {
                let [key] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                Ok(Value::Boolean(self.map_key_index(id, &key.value).is_some()))
            }
            MapIntrinsic::Get => {
                let [key] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let Collection::Map {
                    value_type,
                    entries,
                    ..
                } = self.collection(id)
                else {
                    unreachable!()
                };
                Ok(self
                    .map_key_index(id, &key.value)
                    .map(|index| entries[index].1.clone())
                    .unwrap_or_else(|| Value::Null(Some(value_type.clone()))))
            }
            MapIntrinsic::IsEmpty => {
                expect_no_arguments(arguments, span)?;
                let Collection::Map { entries, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Boolean(entries.is_empty()))
            }
            MapIntrinsic::KeySet => {
                expect_no_arguments(arguments, span)?;
                let (key_type, elements) = match self.collection(id) {
                    Collection::Map {
                        key_type, entries, ..
                    } => (
                        key_type.clone(),
                        entries.iter().map(|(key, _)| key.clone()).collect(),
                    ),
                    _ => unreachable!(),
                };
                Ok(self.allocate(Collection::Set {
                    element_type: key_type,
                    elements,
                    iteration_depth: 0,
                }))
            }
            MapIntrinsic::Put => {
                let [key, value] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let (key_type, value_type) = match self.collection(id) {
                    Collection::Map {
                        key_type,
                        value_type,
                        ..
                    } => (key_type.clone(), value_type.clone()),
                    _ => unreachable!(),
                };
                let key = typed_value(key.value.clone(), &key_type);
                let value = typed_value(value.value.clone(), &value_type);
                self.ensure_collection_mutable(id, span)?;
                Ok(self.map_put(id, key, value))
            }
            MapIntrinsic::PutAll => {
                let [source] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let source_id = self.expect_collection_id(source.value.clone(), source.span)?;
                let source_entries = match self.collection(source_id) {
                    Collection::Map { entries, .. } => entries.clone(),
                    _ => return Err(invalid_runtime_operands(source.span)),
                };
                let (key_type, value_type) = match self.collection(id) {
                    Collection::Map {
                        key_type,
                        value_type,
                        ..
                    } => (key_type.clone(), value_type.clone()),
                    _ => unreachable!(),
                };
                self.ensure_collection_mutable(id, span)?;
                for (key, value) in source_entries {
                    self.map_put(
                        id,
                        typed_value(key, &key_type),
                        typed_value(value, &value_type),
                    );
                }
                Ok(Value::Void)
            }
            MapIntrinsic::Remove => {
                let [key] = arguments else {
                    return Err(invalid_call_arguments(span));
                };
                let value_type = match self.collection(id) {
                    Collection::Map { value_type, .. } => value_type.clone(),
                    _ => unreachable!(),
                };
                self.ensure_collection_mutable(id, span)?;
                if let Some(index) = self.map_key_index(id, &key.value) {
                    let Collection::Map { entries, .. } = self.collection_mut(id) else {
                        unreachable!()
                    };
                    Ok(entries.remove(index).1)
                } else {
                    Ok(Value::Null(Some(value_type)))
                }
            }
            MapIntrinsic::Size => {
                expect_no_arguments(arguments, span)?;
                let Collection::Map { entries, .. } = self.collection(id) else {
                    unreachable!()
                };
                Ok(Value::Integer(collection_size(entries.len(), span)?))
            }
            MapIntrinsic::Values => {
                expect_no_arguments(arguments, span)?;
                let (value_type, elements) = match self.collection(id) {
                    Collection::Map {
                        value_type,
                        entries,
                        ..
                    } => (
                        value_type.clone(),
                        entries.iter().map(|(_, value)| value.clone()).collect(),
                    ),
                    _ => unreachable!(),
                };
                Ok(self.allocate(Collection::List {
                    element_type: value_type,
                    elements,
                    iteration_depth: 0,
                }))
            }
        }
    }

    fn map_put(&mut self, id: CollectionId, key: Value, value: Value) -> Value {
        if let Some(index) = self.map_key_index(id, &key) {
            let Collection::Map { entries, .. } = self.collection_mut(id) else {
                unreachable!()
            };
            let previous = entries[index].1.clone();
            entries[index] = (key, value);
            previous
        } else {
            let value_type = match self.collection(id) {
                Collection::Map { value_type, .. } => value_type.clone(),
                _ => unreachable!(),
            };
            let Collection::Map { entries, .. } = self.collection_mut(id) else {
                unreachable!()
            };
            entries.push((key, value));
            Value::Null(Some(value_type))
        }
    }

    fn map_key_index(&self, id: CollectionId, key: &Value) -> Option<usize> {
        let Collection::Map { entries, .. } = self.collection(id) else {
            return None;
        };
        entries
            .iter()
            .position(|(existing, _)| self.values_equal(existing, key))
    }

    fn list_get(&self, id: CollectionId, index: &Value, span: Span) -> Result<Value, Diagnostic> {
        let index = expect_integer(index, span)?;
        let Collection::List { elements, .. } = self.collection(id) else {
            return Err(invalid_runtime_operands(span));
        };
        let index = checked_list_index(index, elements.len(), false, span)?;
        Ok(elements[index].clone())
    }
}

fn null_method_receiver(method: &Identifier, span: Span) -> Diagnostic {
    runtime_exception(
        "NullPointerException",
        format!(
            "attempt to de-reference a null value while calling `{}`",
            method.spelling
        ),
        span,
    )
}

fn assertion_failure_message(message: Option<&str>, detail: &str) -> String {
    match message {
        Some(message) => format!("Assertion Failed: {message} ({detail})"),
        None => format!("Assertion Failed: {detail}"),
    }
}

pub(super) fn expect_integer(value: &Value, span: Span) -> Result<i64, Diagnostic> {
    match value {
        Value::Integer(value) => Ok(*value),
        Value::Null(_) => Err(runtime_exception(
            "NullPointerException",
            "expected non-null Integer at runtime",
            span,
        )),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn expect_boolean(value: &Value, span: Span) -> Result<bool, Diagnostic> {
    match value {
        Value::Boolean(value) => Ok(*value),
        Value::Null(_) => Err(runtime_exception(
            "NullPointerException",
            "expected non-null Boolean at runtime",
            span,
        )),
        _ => Err(invalid_runtime_operands(span)),
    }
}

pub(super) fn expect_string(value: &Value, span: Span) -> Result<&str, Diagnostic> {
    match value {
        Value::String(value) => Ok(value),
        Value::Null(_) => Err(runtime_exception(
            "NullPointerException",
            "expected non-null String at runtime",
            span,
        )),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn nonnegative_usize(value: &Value, span: Span, label: &str) -> Result<usize, Diagnostic> {
    let value = expect_integer(value, span)?;
    if value < 0 {
        return Err(runtime_exception(
            "StringException",
            format!("{label} cannot be negative"),
            span,
        ));
    }
    usize::try_from(value)
        .map_err(|_| runtime_exception("StringException", format!("{label} is too large"), span))
}

fn collection_size(size: usize, span: Span) -> Result<i64, Diagnostic> {
    i64::try_from(size)
        .map_err(|_| runtime_exception("ListException", "collection size is too large", span))
}

fn sort_primitive_values(values: &mut [Value], span: Span) -> Result<(), Diagnostic> {
    if values
        .iter()
        .any(|value| !matches!(value, Value::String(_) | Value::Integer(_) | Value::Null(_)))
    {
        return Err(runtime_exception(
            "TypeException",
            "List.sort currently requires String or Integer values",
            span,
        ));
    }
    values.sort_by(|left, right| match (left, right) {
        (Value::Null(_), Value::Null(_)) => Ordering::Equal,
        (Value::Null(_), _) => Ordering::Less,
        (_, Value::Null(_)) => Ordering::Greater,
        (Value::String(left), Value::String(right)) => left.cmp(right),
        (Value::Integer(left), Value::Integer(right)) => left.cmp(right),
        _ => Ordering::Equal,
    });
    Ok(())
}

fn utf16_byte_index(value: &str, target: usize) -> Option<usize> {
    if target == 0 {
        return Some(0);
    }
    let mut units = 0;
    for (byte_index, character) in value.char_indices() {
        if units == target {
            return Some(byte_index);
        }
        units += character.len_utf16();
        if units > target {
            return None;
        }
    }
    (units == target).then_some(value.len())
}

pub(super) fn expect_no_arguments(
    arguments: &[EvaluatedArgument],
    span: Span,
) -> Result<(), Diagnostic> {
    if arguments.is_empty() {
        Ok(())
    } else {
        Err(invalid_call_arguments(span))
    }
}

pub(super) fn invalid_call_arguments(span: Span) -> Diagnostic {
    runtime_exception(
        "TypeException",
        "invalid call arguments escaped semantic validation",
        span,
    )
}
