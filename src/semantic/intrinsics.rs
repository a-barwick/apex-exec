use super::Checker;
use crate::{
    ast::{Expression, Identifier, TypeName},
    diagnostic::Diagnostic,
    hir::{
        ExceptionIntrinsic, ExpressionType, IntrinsicId, LimitIntrinsic, ListIntrinsic,
        MapIntrinsic, MathIntrinsic, PlatformIntrinsic, SetIntrinsic, StaticStringIntrinsic,
        StringIntrinsic, SystemIntrinsic,
    },
    span::Span,
};

impl Checker {
    pub(super) fn exception_instance_method_type(
        &mut self,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        let intrinsic = match method.canonical.as_str() {
            "getmessage" => ExceptionIntrinsic::GetMessage,
            "gettypename" => ExceptionIntrinsic::GetTypeName,
            "getstacktracestring" => ExceptionIntrinsic::GetStackTraceString,
            _ => return Err(unknown_method(receiver_type, method)),
        };
        require_arity(
            receiver_type,
            &method.spelling,
            arguments.len(),
            &[0],
            arguments,
        )?;
        Ok((
            IntrinsicId::Exception(intrinsic),
            ExpressionType::Value(TypeName::String),
        ))
    }

    pub(super) fn list_method_type(
        &mut self,
        receiver_type: &TypeName,
        element: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        let intrinsic = match method.canonical.as_str() {
            "add" => ListIntrinsic::Add,
            "addall" => ListIntrinsic::AddAll,
            "clear" => ListIntrinsic::Clear,
            "clone" => ListIntrinsic::Clone,
            "deepclone" => ListIntrinsic::DeepClone,
            "contains" => ListIntrinsic::Contains,
            "get" => ListIntrinsic::Get,
            "indexof" => ListIntrinsic::IndexOf,
            "isempty" => ListIntrinsic::IsEmpty,
            "remove" => ListIntrinsic::Remove,
            "set" => ListIntrinsic::Set,
            "size" => ListIntrinsic::Size,
            "sort" => ListIntrinsic::Sort,
            _ => return Err(unknown_method(receiver_type, method)),
        };
        let result = match intrinsic {
            ListIntrinsic::Add => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1, 2],
                    arguments,
                )?;
                if arguments.len() == 2 {
                    self.require_argument(
                        receiver_type,
                        &method.spelling,
                        0,
                        &arguments[0],
                        &TypeName::Integer,
                    )?;
                    self.require_argument(
                        receiver_type,
                        &method.spelling,
                        1,
                        &arguments[1],
                        element,
                    )?;
                } else {
                    self.require_argument(
                        receiver_type,
                        &method.spelling,
                        0,
                        &arguments[0],
                        element,
                    )?;
                }
                Ok(ExpressionType::Void)
            }
            ListIntrinsic::AddAll => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_list_or_set_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    element,
                )?;
                Ok(ExpressionType::Void)
            }
            ListIntrinsic::Clear => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Void)
            }
            ListIntrinsic::Clone | ListIntrinsic::DeepClone => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(receiver_type.clone()))
            }
            ListIntrinsic::Contains => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], element)?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            ListIntrinsic::Get => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::Integer,
                )?;
                Ok(ExpressionType::Value(element.clone()))
            }
            ListIntrinsic::IndexOf => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], element)?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            ListIntrinsic::IsEmpty => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            ListIntrinsic::Remove => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::Integer,
                )?;
                Ok(ExpressionType::Value(element.clone()))
            }
            ListIntrinsic::Set => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[2],
                    arguments,
                )?;
                self.require_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::Integer,
                )?;
                self.require_argument(receiver_type, &method.spelling, 1, &arguments[1], element)?;
                Ok(ExpressionType::Void)
            }
            ListIntrinsic::Size => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            ListIntrinsic::Sort => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                if !self.sortable_list_element(element) {
                    return Err(Diagnostic::new(
                        format!(
                            "method `sort` requires primitive or Comparable list elements, found {}",
                            receiver_type.apex_name()
                        ),
                        method.span,
                    ));
                }
                Ok(ExpressionType::Void)
            }
        }?;
        Ok((IntrinsicId::List(intrinsic), result))
    }

    pub(super) fn set_method_type(
        &mut self,
        receiver_type: &TypeName,
        element: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        let intrinsic = match method.canonical.as_str() {
            "add" => SetIntrinsic::Add,
            "addall" => SetIntrinsic::AddAll,
            "clear" => SetIntrinsic::Clear,
            "clone" => SetIntrinsic::Clone,
            "contains" => SetIntrinsic::Contains,
            "containsall" => SetIntrinsic::ContainsAll,
            "isempty" => SetIntrinsic::IsEmpty,
            "remove" => SetIntrinsic::Remove,
            "removeall" => SetIntrinsic::RemoveAll,
            "retainall" => SetIntrinsic::RetainAll,
            "size" => SetIntrinsic::Size,
            _ => return Err(unknown_method(receiver_type, method)),
        };
        let result = match intrinsic {
            SetIntrinsic::Add | SetIntrinsic::Contains | SetIntrinsic::Remove => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], element)?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            SetIntrinsic::AddAll
            | SetIntrinsic::ContainsAll
            | SetIntrinsic::RemoveAll
            | SetIntrinsic::RetainAll => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_list_or_set_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    element,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            SetIntrinsic::Clear => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Void)
            }
            SetIntrinsic::Clone => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(receiver_type.clone()))
            }
            SetIntrinsic::IsEmpty => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            SetIntrinsic::Size => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
        }?;
        Ok((IntrinsicId::Set(intrinsic), result))
    }

    pub(super) fn map_method_type(
        &mut self,
        receiver_type: &TypeName,
        key: &TypeName,
        value: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        let intrinsic = match method.canonical.as_str() {
            "clear" => MapIntrinsic::Clear,
            "clone" => MapIntrinsic::Clone,
            "deepclone" => MapIntrinsic::DeepClone,
            "containskey" => MapIntrinsic::ContainsKey,
            "get" => MapIntrinsic::Get,
            "isempty" => MapIntrinsic::IsEmpty,
            "keyset" => MapIntrinsic::KeySet,
            "put" => MapIntrinsic::Put,
            "putall" => MapIntrinsic::PutAll,
            "remove" => MapIntrinsic::Remove,
            "size" => MapIntrinsic::Size,
            "values" => MapIntrinsic::Values,
            _ => return Err(unknown_method(receiver_type, method)),
        };
        let result = match intrinsic {
            MapIntrinsic::Clear => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Void)
            }
            MapIntrinsic::Clone | MapIntrinsic::DeepClone => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(receiver_type.clone()))
            }
            MapIntrinsic::ContainsKey => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], key)?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            MapIntrinsic::Get => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], key)?;
                Ok(ExpressionType::Value(value.clone()))
            }
            MapIntrinsic::IsEmpty => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            MapIntrinsic::KeySet => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Set(Box::new(key.clone()))))
            }
            MapIntrinsic::Put => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[2],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], key)?;
                self.require_argument(receiver_type, &method.spelling, 1, &arguments[1], value)?;
                Ok(ExpressionType::Value(value.clone()))
            }
            MapIntrinsic::PutAll => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    receiver_type,
                )?;
                Ok(ExpressionType::Void)
            }
            MapIntrinsic::Remove => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(receiver_type, &method.spelling, 0, &arguments[0], key)?;
                Ok(ExpressionType::Value(value.clone()))
            }
            MapIntrinsic::Size => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            MapIntrinsic::Values => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::List(Box::new(
                    value.clone(),
                ))))
            }
        }?;
        Ok((IntrinsicId::Map(intrinsic), result))
    }

    pub(super) fn static_string_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        let intrinsic = match method.canonical.as_str() {
            "valueof" => StaticStringIntrinsic::ValueOf,
            "join" => StaticStringIntrinsic::Join,
            "format" => StaticStringIntrinsic::Format,
            "escapesinglequotes" => StaticStringIntrinsic::EscapeSingleQuotes,
            "isblank" => StaticStringIntrinsic::IsBlank,
            "isnotblank" => StaticStringIntrinsic::IsNotBlank,
            "isempty" => StaticStringIntrinsic::IsEmpty,
            "isnotempty" => StaticStringIntrinsic::IsNotEmpty,
            _ => return Err(unknown_static_method("String", method)),
        };
        let result = match intrinsic {
            StaticStringIntrinsic::ValueOf => {
                require_static_arity("String", method, arguments.len(), &[1], arguments)?;
                self.require_non_void_argument("String", &method.spelling, 0, &arguments[0])?;
                Ok(ExpressionType::Value(TypeName::String))
            }
            StaticStringIntrinsic::Join => {
                require_static_arity("String", method, arguments.len(), &[2], arguments)?;
                self.require_list_or_set_argument(
                    &TypeName::String,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                self.require_argument(
                    &TypeName::String,
                    &method.spelling,
                    1,
                    &arguments[1],
                    &TypeName::String,
                )?;
                Ok(ExpressionType::Value(TypeName::String))
            }
            StaticStringIntrinsic::Format => {
                require_static_arity("String", method, arguments.len(), &[2], arguments)?;
                self.require_argument(
                    &TypeName::String,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                if !matches!(
                    self.expression_type(&arguments[1])?,
                    ExpressionType::Value(TypeName::List(_))
                ) {
                    return Err(Diagnostic::new(
                        "String.format argument 2 must be a List",
                        arguments[1].span(),
                    ));
                }
                Ok(ExpressionType::Value(TypeName::String))
            }
            StaticStringIntrinsic::EscapeSingleQuotes => {
                require_static_arity("String", method, arguments.len(), &[1], arguments)?;
                self.require_argument(
                    &TypeName::String,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                Ok(ExpressionType::Value(TypeName::String))
            }
            StaticStringIntrinsic::IsBlank
            | StaticStringIntrinsic::IsNotBlank
            | StaticStringIntrinsic::IsEmpty
            | StaticStringIntrinsic::IsNotEmpty => {
                require_static_arity("String", method, arguments.len(), &[1], arguments)?;
                self.require_argument(
                    &TypeName::String,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
        }?;
        Ok((IntrinsicId::StaticString(intrinsic), result))
    }

    pub(super) fn string_instance_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        let receiver_type = TypeName::String;
        let intrinsic = match method.canonical.as_str() {
            "length" => StringIntrinsic::Length,
            "contains" => StringIntrinsic::Contains,
            "containsignorecase" => StringIntrinsic::ContainsIgnoreCase,
            "startswith" => StringIntrinsic::StartsWith,
            "endswith" => StringIntrinsic::EndsWith,
            "equals" => StringIntrinsic::Equals,
            "equalsignorecase" => StringIntrinsic::EqualsIgnoreCase,
            "indexof" => StringIntrinsic::IndexOf,
            "substring" => StringIntrinsic::Substring,
            "substringbefore" => StringIntrinsic::SubstringBefore,
            "substringafter" => StringIntrinsic::SubstringAfter,
            "substringafterlast" => StringIntrinsic::SubstringAfterLast,
            "substringbetween" => StringIntrinsic::SubstringBetween,
            "left" => StringIntrinsic::Left,
            "split" => StringIntrinsic::Split,
            "trim" => StringIntrinsic::Trim,
            "tolowercase" => StringIntrinsic::ToLowerCase,
            "touppercase" => StringIntrinsic::ToUpperCase,
            "replace" => StringIntrinsic::Replace,
            "replaceall" => StringIntrinsic::ReplaceAll,
            _ => return Err(unknown_method(&receiver_type, method)),
        };
        let result = match intrinsic {
            StringIntrinsic::Length => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            StringIntrinsic::Contains
            | StringIntrinsic::ContainsIgnoreCase
            | StringIntrinsic::StartsWith
            | StringIntrinsic::EndsWith
            | StringIntrinsic::Equals
            | StringIntrinsic::EqualsIgnoreCase => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    &receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            StringIntrinsic::IndexOf => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    &receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                Ok(ExpressionType::Value(TypeName::Integer))
            }
            StringIntrinsic::Substring => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1, 2],
                    arguments,
                )?;
                for (index, argument) in arguments.iter().enumerate() {
                    self.require_argument(
                        &receiver_type,
                        &method.spelling,
                        index,
                        argument,
                        &TypeName::Integer,
                    )?;
                }
                Ok(ExpressionType::Value(TypeName::String))
            }
            StringIntrinsic::SubstringBefore
            | StringIntrinsic::SubstringAfter
            | StringIntrinsic::SubstringAfterLast => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    &receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                Ok(ExpressionType::Value(TypeName::String))
            }
            StringIntrinsic::SubstringBetween => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[2],
                    arguments,
                )?;
                for (index, argument) in arguments.iter().enumerate() {
                    self.require_argument(
                        &receiver_type,
                        &method.spelling,
                        index,
                        argument,
                        &TypeName::String,
                    )?;
                }
                Ok(ExpressionType::Value(TypeName::String))
            }
            StringIntrinsic::Left => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1],
                    arguments,
                )?;
                self.require_argument(
                    &receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::Integer,
                )?;
                Ok(ExpressionType::Value(TypeName::String))
            }
            StringIntrinsic::Split => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[1, 2],
                    arguments,
                )?;
                self.require_argument(
                    &receiver_type,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                if let Some(limit) = arguments.get(1) {
                    self.require_argument(
                        &receiver_type,
                        &method.spelling,
                        1,
                        limit,
                        &TypeName::Integer,
                    )?;
                }
                Ok(ExpressionType::Value(TypeName::List(Box::new(
                    TypeName::String,
                ))))
            }
            StringIntrinsic::Trim | StringIntrinsic::ToLowerCase | StringIntrinsic::ToUpperCase => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                Ok(ExpressionType::Value(TypeName::String))
            }
            StringIntrinsic::Replace | StringIntrinsic::ReplaceAll => {
                require_arity(
                    &receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[2],
                    arguments,
                )?;
                for (index, argument) in arguments.iter().enumerate() {
                    self.require_argument(
                        &receiver_type,
                        &method.spelling,
                        index,
                        argument,
                        &TypeName::String,
                    )?;
                }
                Ok(ExpressionType::Value(TypeName::String))
            }
        }?;
        Ok((IntrinsicId::String(intrinsic), result))
    }

    pub(super) fn static_math_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        let intrinsic = match method.canonical.as_str() {
            "abs" => MathIntrinsic::Abs,
            "max" => MathIntrinsic::Max,
            "min" => MathIntrinsic::Min,
            "mod" => MathIntrinsic::Mod,
            "random" => MathIntrinsic::Random,
            _ => return Err(unknown_static_method("Math", method)),
        };
        let arity = match intrinsic {
            MathIntrinsic::Abs => 1,
            MathIntrinsic::Max | MathIntrinsic::Min | MathIntrinsic::Mod => 2,
            MathIntrinsic::Random => 0,
        };
        require_static_arity("Math", method, arguments.len(), &[arity], arguments)?;
        for (index, argument) in arguments.iter().enumerate() {
            self.require_named_argument(
                "Math",
                &method.spelling,
                index,
                argument,
                &TypeName::Integer,
            )?;
        }
        Ok((
            IntrinsicId::Math(intrinsic),
            ExpressionType::Value(if intrinsic == MathIntrinsic::Random {
                TypeName::Decimal
            } else {
                TypeName::Integer
            }),
        ))
    }

    pub(super) fn static_system_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        use PlatformIntrinsic as P;
        let async_intrinsic = match method.canonical.as_str() {
            "enqueuejob" => Some(P::SystemEnqueueJob),
            "schedule" => Some(P::SystemSchedule),
            "isfuture" => Some(P::SystemIsFuture),
            "isqueueable" => Some(P::SystemIsQueueable),
            "isbatch" => Some(P::SystemIsBatch),
            "isscheduled" => Some(P::SystemIsScheduled),
            _ => None,
        };
        if let Some(intrinsic) = async_intrinsic {
            let result = match intrinsic {
                P::SystemEnqueueJob => {
                    require_static_arity("System", method, arguments.len(), &[1], arguments)?;
                    self.require_async_implementation(&arguments[0], "Queueable")?;
                    ExpressionType::Value(TypeName::Id)
                }
                P::SystemSchedule => {
                    require_static_arity("System", method, arguments.len(), &[3], arguments)?;
                    self.require_named_argument(
                        "System",
                        &method.spelling,
                        0,
                        &arguments[0],
                        &TypeName::String,
                    )?;
                    self.require_named_argument(
                        "System",
                        &method.spelling,
                        1,
                        &arguments[1],
                        &TypeName::String,
                    )?;
                    self.require_async_implementation(&arguments[2], "Schedulable")?;
                    ExpressionType::Value(TypeName::Id)
                }
                P::SystemIsFuture
                | P::SystemIsQueueable
                | P::SystemIsBatch
                | P::SystemIsScheduled => {
                    require_static_arity("System", method, arguments.len(), &[0], arguments)?;
                    ExpressionType::Value(TypeName::Boolean)
                }
                _ => unreachable!(),
            };
            return Ok((IntrinsicId::Platform(intrinsic), result));
        }
        let intrinsic = match method.canonical.as_str() {
            "debug" => SystemIntrinsic::Debug,
            "assert" => SystemIntrinsic::Assert,
            "assertequals" => SystemIntrinsic::AssertEquals,
            "assertnotequals" => SystemIntrinsic::AssertNotEquals,
            "now" => SystemIntrinsic::Now,
            "today" => SystemIntrinsic::Today,
            "currenttimemillis" => SystemIntrinsic::CurrentTimeMillis,
            _ => return Err(unknown_static_method("System", method)),
        };
        let result = match intrinsic {
            SystemIntrinsic::Debug => {
                require_static_arity("System", method, arguments.len(), &[1, 2], arguments)?;
                if arguments.len() == 2 {
                    self.require_named_argument(
                        "System",
                        &method.spelling,
                        0,
                        &arguments[0],
                        &TypeName::LoggingLevel,
                    )?;
                }
                let value_index = arguments.len() - 1;
                self.require_non_void_argument(
                    "System",
                    &method.spelling,
                    value_index,
                    &arguments[value_index],
                )?;
                Ok(ExpressionType::Void)
            }
            SystemIntrinsic::Assert => {
                require_static_arity("System", method, arguments.len(), &[1, 2], arguments)?;
                self.require_named_argument(
                    "System",
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::Boolean,
                )?;
                if let Some(message) = arguments.get(1) {
                    self.require_non_void_argument("System", &method.spelling, 1, message)?;
                }
                Ok(ExpressionType::Void)
            }
            SystemIntrinsic::AssertEquals | SystemIntrinsic::AssertNotEquals => {
                require_static_arity("System", method, arguments.len(), &[2, 3], arguments)?;
                self.require_non_void_argument("System", &method.spelling, 0, &arguments[0])?;
                self.require_non_void_argument("System", &method.spelling, 1, &arguments[1])?;
                if let Some(message) = arguments.get(2) {
                    self.require_non_void_argument("System", &method.spelling, 2, message)?;
                }
                Ok(ExpressionType::Void)
            }
            SystemIntrinsic::Now => {
                require_static_arity("System", method, arguments.len(), &[0], arguments)?;
                Ok(ExpressionType::Value(TypeName::Datetime))
            }
            SystemIntrinsic::Today => {
                require_static_arity("System", method, arguments.len(), &[0], arguments)?;
                Ok(ExpressionType::Value(TypeName::Date))
            }
            SystemIntrinsic::CurrentTimeMillis => {
                require_static_arity("System", method, arguments.len(), &[0], arguments)?;
                Ok(ExpressionType::Value(TypeName::Long))
            }
        }?;
        Ok((IntrinsicId::System(intrinsic), result))
    }

    pub(super) fn static_platform_method_type(
        &mut self,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        use PlatformIntrinsic as P;
        let normalized_owner = owner
            .strip_prefix("System.")
            .or_else(|| owner.strip_prefix("system."))
            .unwrap_or(owner);
        if let Some(result) =
            self.user_and_security_static_method_type(normalized_owner, method, arguments)
        {
            return result;
        }
        if let Some(result) =
            self.logging_level_static_method_type(normalized_owner, method, arguments)
        {
            return result;
        }
        let canonical_owner = normalized_owner.to_ascii_lowercase();
        if canonical_owner == "limits" {
            return self.limits_static_method_type(owner, method, arguments);
        }
        if canonical_owner == "network" {
            return self.network_static_method_type(owner, method, arguments);
        }
        let intrinsic = match (canonical_owner.as_str(), method.canonical.as_str()) {
            ("date", "newinstance") => P::DateNewInstance,
            ("date", "valueof") => P::DateValueOf,
            ("date", "today") => P::DateToday,
            ("datetime", "newinstance") => P::DatetimeNewInstance,
            ("datetime", "now") => P::DatetimeNow,
            ("datetime", "valueof") => P::DatetimeValueOf,
            ("datetime", "valueofgmt") => P::DatetimeValueOfGmt,
            ("time", "newinstance") => P::TimeNewInstance,
            ("time", "valueof") => P::TimeValueOf,
            ("decimal", "valueof") => P::DecimalValueOf,
            ("double", "valueof") => P::DoubleValueOf,
            ("long", "valueof") => P::LongValueOf,
            ("id", "valueof") => P::IdValueOf,
            ("blob", "valueof") => P::BlobValueOf,
            ("json", "serialize") => P::JsonSerialize,
            ("json", "serializepretty") => P::JsonSerializePretty,
            ("json", "deserialize") => P::JsonDeserialize,
            ("json", "deserializeuntyped") => P::JsonDeserializeUntyped,
            ("pattern", "compile") => P::PatternCompile,
            ("pattern", "quote") => P::PatternQuote,
            ("schema", "getglobaldescribe") => P::SchemaGetGlobalDescribe,
            ("test", "starttest") => P::TestStartTest,
            ("test", "stoptest") => P::TestStopTest,
            ("test", "isrunningtest") => P::TestIsRunningTest,
            ("test" | "system.test", "setmock") => P::TestSetMock,
            ("encodingutil", "base64encode") => P::EncodingBase64Encode,
            ("encodingutil", "base64decode") => P::EncodingBase64Decode,
            ("database", "executebatch") => P::DatabaseExecuteBatch,
            ("eventbus", "publish") => P::EventBusPublish,
            ("request" | "system.request", "getcurrent") => P::RequestGetCurrent,
            ("cache.org" | "cache.session", "getpartition") => P::CacheGetPartition,
            ("type" | "system.type", "forname") => P::TypeForName,
            _ => return Err(self.unsupported_platform_api(owner, method)),
        };
        let result = match intrinsic {
            P::DateNewInstance => {
                require_static_arity(owner, method, arguments.len(), &[3], arguments)?;
                self.require_all(owner, method, arguments, &TypeName::Integer)?;
                TypeName::Date
            }
            P::DateValueOf => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::Date
            }
            P::DateToday => {
                require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                TypeName::Date
            }
            P::DatetimeNewInstance => {
                require_static_arity(owner, method, arguments.len(), &[6], arguments)?;
                self.require_all(owner, method, arguments, &TypeName::Integer)?;
                TypeName::Datetime
            }
            P::DatetimeNow => {
                require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                TypeName::Datetime
            }
            P::DatetimeValueOf => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                let argument = self.expression_type(&arguments[0])?;
                if !matches!(
                    argument,
                    ExpressionType::Value(TypeName::String | TypeName::Long)
                ) {
                    return Err(Diagnostic::new(
                        format!(
                            "{}.{} argument 1 expects String or Long, found {}",
                            owner,
                            method.spelling,
                            argument.apex_name()
                        ),
                        arguments[0].span(),
                    ));
                }
                TypeName::Datetime
            }
            P::DatetimeValueOfGmt => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::Datetime
            }
            P::TimeNewInstance => {
                require_static_arity(owner, method, arguments.len(), &[4], arguments)?;
                self.require_all(owner, method, arguments, &TypeName::Integer)?;
                TypeName::Time
            }
            P::TimeValueOf => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::Time
            }
            P::DecimalValueOf => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::Decimal
            }
            P::DoubleValueOf => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::Double
            }
            P::LongValueOf => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::Long
            }
            P::IdValueOf => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::Id
            }
            P::BlobValueOf => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::Blob
            }
            P::JsonSerialize | P::JsonSerializePretty => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_non_void_argument(owner, &method.spelling, 0, &arguments[0])?;
                TypeName::String
            }
            P::JsonDeserialize => {
                require_static_arity(owner, method, arguments.len(), &[2], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    1,
                    &arguments[1],
                    &TypeName::Type,
                )?;
                TypeName::Object
            }
            P::JsonDeserializeUntyped => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::Object
            }
            P::PatternCompile | P::PatternQuote => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                if intrinsic == P::PatternCompile {
                    TypeName::Pattern
                } else {
                    TypeName::String
                }
            }
            P::SchemaGetGlobalDescribe => {
                require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                TypeName::Map(Box::new(TypeName::String), Box::new(TypeName::SObjectType))
            }
            P::TestStartTest | P::TestStopTest => {
                require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                return Ok((IntrinsicId::Platform(intrinsic), ExpressionType::Void));
            }
            P::TestSetMock => {
                require_static_arity(owner, method, arguments.len(), &[2], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::Type,
                )?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    1,
                    &arguments[1],
                    &TypeName::HttpCalloutMock,
                )?;
                return Ok((IntrinsicId::Platform(intrinsic), ExpressionType::Void));
            }
            P::TestIsRunningTest => {
                require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                TypeName::Boolean
            }
            P::EncodingBase64Encode => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::Blob,
                )?;
                TypeName::String
            }
            P::EncodingBase64Decode => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::Blob
            }
            P::UserInfoGetUserId
            | P::UserInfoGetUserName
            | P::UserInfoGetProfileId
            | P::SecurityStripInaccessible => {
                unreachable!("UserInfo and Security intrinsics were handled above")
            }
            P::DatabaseExecuteBatch => {
                require_static_arity(owner, method, arguments.len(), &[1, 2], arguments)?;
                self.require_async_implementation(&arguments[0], "Batchable")?;
                if let Some(scope_size) = arguments.get(1) {
                    self.require_named_argument(
                        owner,
                        &method.spelling,
                        1,
                        scope_size,
                        &TypeName::Integer,
                    )?;
                }
                TypeName::Id
            }
            P::EventBusPublish => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_platform_event_argument(&arguments[0])?;
                return Ok((IntrinsicId::Platform(intrinsic), ExpressionType::Void));
            }
            P::RequestGetCurrent => {
                require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                TypeName::Request
            }
            P::CacheGetPartition => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::CachePartition
            }
            P::TypeForName => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                TypeName::Type
            }
            _ => unreachable!("instance intrinsic selected as static"),
        };
        Ok((
            IntrinsicId::Platform(intrinsic),
            ExpressionType::Value(result),
        ))
    }

    fn limits_static_method_type(
        &mut self,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        let Some(intrinsic) = limit_intrinsic(&method.canonical) else {
            return Err(self.unsupported_platform_api(owner, method));
        };
        require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
        Ok((
            IntrinsicId::Platform(PlatformIntrinsic::Limits(intrinsic)),
            ExpressionType::Value(TypeName::Integer),
        ))
    }

    fn network_static_method_type(
        &mut self,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        use PlatformIntrinsic as P;
        let (intrinsic, result) = match method.canonical.as_str() {
            "getnetworkid" => {
                require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                (P::NetworkGetNetworkId, TypeName::Id)
            }
            "getloginurl" | "getlogouturl" | "getselfregurl" => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_named_argument(
                    owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::Id,
                )?;
                let intrinsic = match method.canonical.as_str() {
                    "getloginurl" => P::NetworkGetLoginUrl,
                    "getlogouturl" => P::NetworkGetLogoutUrl,
                    "getselfregurl" => P::NetworkGetSelfRegUrl,
                    _ => unreachable!(),
                };
                (intrinsic, TypeName::String)
            }
            _ => return Err(self.unsupported_platform_api(owner, method)),
        };
        Ok((
            IntrinsicId::Platform(intrinsic),
            ExpressionType::Value(result),
        ))
    }

    fn user_and_security_static_method_type(
        &mut self,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Option<Result<(IntrinsicId, ExpressionType), Diagnostic>> {
        use PlatformIntrinsic as P;
        let intrinsic = match (
            owner.to_ascii_lowercase().as_str(),
            method.canonical.as_str(),
        ) {
            ("userinfo", "getuserid") => P::UserInfoGetUserId,
            ("userinfo", "getusername") => P::UserInfoGetUserName,
            ("userinfo", "getprofileid") => P::UserInfoGetProfileId,
            ("security", "stripinaccessible") => P::SecurityStripInaccessible,
            _ => return None,
        };
        Some((|| {
            let result = match intrinsic {
                P::UserInfoGetUserId | P::UserInfoGetProfileId => {
                    require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                    TypeName::Id
                }
                P::UserInfoGetUserName => {
                    require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                    TypeName::String
                }
                P::SecurityStripInaccessible => {
                    self.security_strip_inaccessible_type(owner, method, arguments)?
                }
                _ => unreachable!("only UserInfo and Security intrinsics use this helper"),
            };
            Ok((
                IntrinsicId::Platform(intrinsic),
                ExpressionType::Value(result),
            ))
        })())
    }

    fn logging_level_static_method_type(
        &mut self,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Option<Result<(IntrinsicId, ExpressionType), Diagnostic>> {
        use PlatformIntrinsic as P;
        let descriptor =
            crate::platform::PlatformEnumDescriptor::from_owner(&owner.to_ascii_lowercase())?;
        if descriptor != crate::platform::PlatformEnumDescriptor::LoggingLevel {
            return None;
        }
        let intrinsic = match method.canonical.as_str() {
            "values" => P::LoggingLevelValues,
            "valueof" => P::LoggingLevelValueOf,
            _ => return None,
        };
        Some((|| {
            let ty = match intrinsic {
                P::LoggingLevelValues => {
                    require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                    TypeName::List(Box::new(TypeName::LoggingLevel))
                }
                P::LoggingLevelValueOf => {
                    require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                    self.require_named_argument(
                        owner,
                        &method.spelling,
                        0,
                        &arguments[0],
                        &TypeName::String,
                    )?;
                    TypeName::LoggingLevel
                }
                _ => unreachable!("only LoggingLevel static intrinsics use this helper"),
            };
            Ok((IntrinsicId::Platform(intrinsic), ExpressionType::Value(ty)))
        })())
    }

    fn security_strip_inaccessible_type(
        &mut self,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<TypeName, Diagnostic> {
        require_static_arity(owner, method, arguments.len(), &[2, 3], arguments)?;
        self.require_named_argument(
            owner,
            &method.spelling,
            0,
            &arguments[0],
            &TypeName::AccessType,
        )?;
        match self.expression_type(&arguments[1])? {
            ExpressionType::Value(TypeName::List(element))
                if self.is_sobject_type(&element) || self.is_dynamic_sobject_type(&element) => {}
            _ => {
                return Err(Diagnostic::new(
                    "Security.stripInaccessible argument 2 must be a List of SObjects",
                    arguments[1].span(),
                ));
            }
        }
        if let Some(enforce_root_object_crud) = arguments.get(2) {
            self.require_named_argument(
                owner,
                &method.spelling,
                2,
                enforce_root_object_crud,
                &TypeName::Boolean,
            )?;
        }
        Ok(TypeName::SObjectAccessDecision)
    }

    fn unsupported_instance_platform_api(
        &self,
        receiver_type: &TypeName,
        method: &Identifier,
    ) -> Diagnostic {
        self.unsupported_platform_api(&receiver_type.apex_name(), method)
    }

    pub(super) fn platform_instance_method_type(
        &mut self,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        use PlatformIntrinsic as P;
        if let Some(result) =
            self.platform_enum_instance_method_type(receiver_type, method, arguments)
        {
            return result;
        }
        if let Some(result) = self.schema_instance_method_type(receiver_type, method, arguments) {
            return result;
        }
        let intrinsic = match (receiver_type, method.canonical.as_str()) {
            (TypeName::Date, "adddays") => P::DateAddDays,
            (TypeName::Date, "addmonths") => P::DateAddMonths,
            (TypeName::Date, "addyears") => P::DateAddYears,
            (TypeName::Date, "daysbetween") => P::DateDaysBetween,
            (TypeName::Date, "format") => P::DateFormat,
            (TypeName::Date, "year") => P::DateYear,
            (TypeName::Date, "month") => P::DateMonth,
            (TypeName::Date, "day") => P::DateDay,
            (TypeName::Datetime, "gettime") => P::DatetimeGetTime,
            (TypeName::Datetime, "date") => P::DatetimeDate,
            (TypeName::Datetime, "dategmt") => P::DatetimeDateGmt,
            (TypeName::Datetime, "time") => P::DatetimeTime,
            (TypeName::Datetime, "timegmt") => P::DatetimeTimeGmt,
            (TypeName::Datetime, "adddays") => P::DatetimeAddDays,
            (TypeName::Datetime, "addhours") => P::DatetimeAddHours,
            (TypeName::Datetime, "addminutes") => P::DatetimeAddMinutes,
            (TypeName::Datetime, "addseconds") => P::DatetimeAddSeconds,
            (TypeName::Datetime, "format") => P::DatetimeFormat,
            (TypeName::Time, "addhours") => P::TimeAddHours,
            (TypeName::Time, "addminutes") => P::TimeAddMinutes,
            (TypeName::Time, "addseconds") => P::TimeAddSeconds,
            (TypeName::Time, "addmilliseconds") => P::TimeAddMilliseconds,
            (TypeName::Time, "hour") => P::TimeHour,
            (TypeName::Time, "minute") => P::TimeMinute,
            (TypeName::Time, "second") => P::TimeSecond,
            (TypeName::Time, "millisecond") => P::TimeMillisecond,
            (TypeName::Time, "format") => P::TimeFormat,
            (TypeName::Decimal, "setscale") => P::DecimalSetScale,
            (TypeName::Decimal, "abs") => P::DecimalAbs,
            (TypeName::Decimal, "scale") => P::DecimalScale,
            (TypeName::Decimal, "tostring") => P::ObjectToString,
            (TypeName::Double, "tostring") => P::ObjectToString,
            (TypeName::Id, "to15") => P::IdTo15,
            (TypeName::Id, "to18") => P::IdTo18,
            (TypeName::Blob, "tostring") => P::BlobToString,
            (TypeName::Blob, "size") => P::BlobSize,
            (TypeName::Object, "tostring") => P::ObjectToString,
            (TypeName::Pattern, "matcher") => P::PatternMatcher,
            (TypeName::Matcher, "matches") => P::MatcherMatches,
            (TypeName::Matcher, "find") => P::MatcherFind,
            (TypeName::Matcher, "group") => P::MatcherGroup,
            (TypeName::Matcher, "start") => P::MatcherStart,
            (TypeName::Matcher, "end") => P::MatcherEnd,
            (TypeName::SObjectType, "getdescribe") => P::SObjectTypeGetDescribe,
            (TypeName::SObjectType, "tostring") => P::ObjectToString,
            (TypeName::DescribeSObjectResult, "getname") => P::DescribeGetName,
            (TypeName::DescribeSObjectResult, "getkeyprefix") => P::DescribeGetKeyPrefix,
            (TypeName::DescribeSObjectResult, "iscustom") => P::DescribeIsCustom,
            (TypeName::HttpRequest, "setendpoint") => P::HttpRequestSetEndpoint,
            (TypeName::HttpRequest, "getendpoint") => P::HttpRequestGetEndpoint,
            (TypeName::HttpRequest, "setmethod") => P::HttpRequestSetMethod,
            (TypeName::HttpRequest, "getmethod") => P::HttpRequestGetMethod,
            (TypeName::HttpRequest, "setbody") => P::HttpRequestSetBody,
            (TypeName::HttpRequest, "getbody") => P::HttpRequestGetBody,
            (TypeName::HttpRequest, "setheader") => P::HttpRequestSetHeader,
            (TypeName::HttpRequest, "getheader") => P::HttpRequestGetHeader,
            (TypeName::HttpRequest, "settimeout") => P::HttpRequestSetTimeout,
            (TypeName::HttpRequest, "gettimeout") => P::HttpRequestGetTimeout,
            (TypeName::HttpResponse, "setstatuscode") => P::HttpResponseSetStatusCode,
            (TypeName::HttpResponse, "getstatuscode") => P::HttpResponseGetStatusCode,
            (TypeName::HttpResponse, "setbody") => P::HttpResponseSetBody,
            (TypeName::HttpResponse, "getbody") => P::HttpResponseGetBody,
            (TypeName::HttpResponse, "setheader") => P::HttpResponseSetHeader,
            (TypeName::HttpResponse, "getheader") => P::HttpResponseGetHeader,
            (TypeName::HttpResponse, "setstatus") => P::HttpResponseSetStatus,
            (TypeName::HttpResponse, "getstatus") => P::HttpResponseGetStatus,
            (TypeName::Http, "send") => P::HttpSend,
            (TypeName::HttpCalloutMock, "respond") => P::HttpCalloutMockRespond,
            (TypeName::VisualEditorDataRow, "getlabel") => P::VisualEditorDataRowGetLabel,
            (TypeName::VisualEditorDataRow, "getvalue") => P::VisualEditorDataRowGetValue,
            (TypeName::VisualEditorDynamicPickListRows, "addrow") => P::VisualEditorRowsAddRow,
            (TypeName::VisualEditorDynamicPickListRows, "getdatarows") => {
                P::VisualEditorRowsGetDataRows
            }
            (TypeName::QueueableContext | TypeName::BatchableContext, "getjobid") => {
                P::AsyncContextGetJobId
            }
            (TypeName::BatchableContext, "getchildjobid") => P::BatchableContextGetChildJobId,
            (TypeName::FinalizerContext, "getasyncapexjobid") => {
                P::FinalizerContextGetAsyncApexJobId
            }
            (TypeName::FinalizerContext, "getexception") => P::FinalizerContextGetException,
            (TypeName::FinalizerContext, "getresult") => P::FinalizerContextGetResult,
            (TypeName::FinalizerContext, "getrequestid") => P::FinalizerContextGetRequestId,
            (TypeName::SchedulableContext, "gettriggerid") => P::SchedulableContextGetTriggerId,
            (TypeName::Request, "getrequestid") => P::RequestGetRequestId,
            (TypeName::Request, "getquiddity") => P::RequestGetQuiddity,
            (TypeName::CachePartition, "contains") => P::CachePartitionContains,
            (TypeName::CachePartition, "get") => P::CachePartitionGet,
            (TypeName::CachePartition, "isavailable") => P::CachePartitionIsAvailable,
            (TypeName::CachePartition, "put") => P::CachePartitionPut,
            (TypeName::CachePartition, "remove") => P::CachePartitionRemove,
            (TypeName::Callable, "call") => P::CallableCall,
            (TypeName::Type, "getname") => P::TypeGetName,
            (TypeName::Type, "newinstance") => P::TypeNewInstance,
            _ => return Err(self.unsupported_instance_platform_api(receiver_type, method)),
        };
        let owner = receiver_type.apex_name();
        let result = match intrinsic {
            P::DateAddDays | P::DateAddMonths | P::DateAddYears => {
                self.one_argument(&owner, method, arguments, &TypeName::Integer)?;
                TypeName::Date
            }
            P::DateDaysBetween => {
                self.one_argument(&owner, method, arguments, &TypeName::Date)?;
                TypeName::Integer
            }
            P::DateFormat => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::String
            }
            P::DateYear | P::DateMonth | P::DateDay => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Integer
            }
            P::DatetimeGetTime => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Long
            }
            P::DatetimeDate | P::DatetimeDateGmt => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Date
            }
            P::DatetimeTime | P::DatetimeTimeGmt => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Time
            }
            P::DatetimeAddDays
            | P::DatetimeAddHours
            | P::DatetimeAddMinutes
            | P::DatetimeAddSeconds => {
                self.one_argument(&owner, method, arguments, &TypeName::Integer)?;
                TypeName::Datetime
            }
            P::DatetimeFormat => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::String
            }
            P::TimeAddHours | P::TimeAddMinutes | P::TimeAddSeconds | P::TimeAddMilliseconds => {
                self.one_argument(&owner, method, arguments, &TypeName::Integer)?;
                TypeName::Time
            }
            P::TimeHour | P::TimeMinute | P::TimeSecond | P::TimeMillisecond => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Integer
            }
            P::TimeFormat => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::String
            }
            P::DecimalSetScale => {
                self.one_argument(&owner, method, arguments, &TypeName::Integer)?;
                TypeName::Decimal
            }
            P::DecimalAbs => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Decimal
            }
            P::DecimalScale => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Integer
            }
            P::IdTo15 | P::IdTo18 | P::BlobToString | P::ObjectToString => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::String
            }
            P::VisualEditorDataRowGetLabel | P::VisualEditorDataRowGetValue => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::String
            }
            P::VisualEditorRowsAddRow => {
                self.one_argument(&owner, method, arguments, &TypeName::VisualEditorDataRow)?;
                return Ok((IntrinsicId::Platform(intrinsic), ExpressionType::Void));
            }
            P::VisualEditorRowsGetDataRows => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::List(Box::new(TypeName::VisualEditorDataRow))
            }
            P::BlobSize => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Integer
            }
            P::PatternMatcher => {
                self.one_argument(&owner, method, arguments, &TypeName::String)?;
                TypeName::Matcher
            }
            P::MatcherMatches | P::MatcherFind => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Boolean
            }
            P::MatcherGroup => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0, 1],
                    arguments,
                )?;
                if let Some(arg) = arguments.first() {
                    self.require_named_argument(
                        &owner,
                        &method.spelling,
                        0,
                        arg,
                        &TypeName::Integer,
                    )?;
                }
                TypeName::String
            }
            P::MatcherStart | P::MatcherEnd => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0, 1],
                    arguments,
                )?;
                if let Some(arg) = arguments.first() {
                    self.require_named_argument(
                        &owner,
                        &method.spelling,
                        0,
                        arg,
                        &TypeName::Integer,
                    )?;
                }
                TypeName::Integer
            }
            P::SObjectTypeGetDescribe => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::DescribeSObjectResult
            }
            P::DescribeGetName | P::DescribeGetKeyPrefix => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::String
            }
            P::DescribeIsCustom => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Boolean
            }
            P::HttpRequestSetEndpoint
            | P::HttpRequestSetMethod
            | P::HttpRequestSetBody
            | P::HttpResponseSetBody
            | P::HttpResponseSetStatus => {
                self.one_argument(&owner, method, arguments, &TypeName::String)?;
                return Ok((IntrinsicId::Platform(intrinsic), ExpressionType::Void));
            }
            P::HttpRequestSetHeader | P::HttpResponseSetHeader => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[2],
                    arguments,
                )?;
                self.require_all(&owner, method, arguments, &TypeName::String)?;
                return Ok((IntrinsicId::Platform(intrinsic), ExpressionType::Void));
            }
            P::HttpRequestSetTimeout | P::HttpResponseSetStatusCode => {
                self.one_argument(&owner, method, arguments, &TypeName::Integer)?;
                return Ok((IntrinsicId::Platform(intrinsic), ExpressionType::Void));
            }
            P::HttpRequestGetEndpoint
            | P::HttpRequestGetMethod
            | P::HttpRequestGetBody
            | P::HttpResponseGetBody
            | P::HttpResponseGetStatus => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::String
            }
            P::HttpRequestGetHeader | P::HttpResponseGetHeader => {
                self.one_argument(&owner, method, arguments, &TypeName::String)?;
                TypeName::String
            }
            P::HttpRequestGetTimeout | P::HttpResponseGetStatusCode => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Integer
            }
            P::HttpSend => {
                self.one_argument(&owner, method, arguments, &TypeName::HttpRequest)?;
                TypeName::HttpResponse
            }
            P::HttpCalloutMockRespond => {
                self.one_argument(&owner, method, arguments, &TypeName::HttpRequest)?;
                TypeName::HttpResponse
            }
            P::AsyncContextGetJobId
            | P::BatchableContextGetChildJobId
            | P::FinalizerContextGetAsyncApexJobId
            | P::SchedulableContextGetTriggerId => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Id
            }
            P::FinalizerContextGetException => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Exception
            }
            P::FinalizerContextGetResult => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::ParentJobResult
            }
            P::FinalizerContextGetRequestId | P::RequestGetRequestId | P::TypeGetName => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::String
            }
            P::RequestGetQuiddity => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Quiddity
            }
            P::CachePartitionContains => {
                self.one_argument(&owner, method, arguments, &TypeName::String)?;
                TypeName::Boolean
            }
            P::CachePartitionGet => {
                self.one_argument(&owner, method, arguments, &TypeName::String)?;
                TypeName::Object
            }
            P::CachePartitionIsAvailable => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Boolean
            }
            P::CachePartitionPut => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[2, 3, 4, 5],
                    arguments,
                )?;
                self.require_named_argument(
                    &owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                self.require_non_void_argument(&owner, &method.spelling, 1, &arguments[1])?;
                if let Some(ttl) = arguments.get(2) {
                    self.require_named_argument(
                        &owner,
                        &method.spelling,
                        2,
                        ttl,
                        &TypeName::Integer,
                    )?;
                }
                if let Some(visibility) = arguments.get(3) {
                    self.require_named_argument(
                        &owner,
                        &method.spelling,
                        3,
                        visibility,
                        &TypeName::CacheVisibility,
                    )?;
                }
                if let Some(immutable) = arguments.get(4) {
                    self.require_named_argument(
                        &owner,
                        &method.spelling,
                        4,
                        immutable,
                        &TypeName::Boolean,
                    )?;
                }
                return Ok((IntrinsicId::Platform(intrinsic), ExpressionType::Void));
            }
            P::CachePartitionRemove => {
                self.one_argument(&owner, method, arguments, &TypeName::String)?;
                return Ok((IntrinsicId::Platform(intrinsic), ExpressionType::Void));
            }
            P::CallableCall => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[2],
                    arguments,
                )?;
                self.require_named_argument(
                    &owner,
                    &method.spelling,
                    0,
                    &arguments[0],
                    &TypeName::String,
                )?;
                self.require_named_argument(
                    &owner,
                    &method.spelling,
                    1,
                    &arguments[1],
                    &TypeName::Map(Box::new(TypeName::String), Box::new(TypeName::Object)),
                )?;
                TypeName::Object
            }
            P::TypeNewInstance => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::Object
            }
            _ => unreachable!("static intrinsic selected as instance"),
        };
        Ok((
            IntrinsicId::Platform(intrinsic),
            ExpressionType::Value(result),
        ))
    }

    fn platform_enum_instance_method_type(
        &mut self,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Option<Result<(IntrinsicId, ExpressionType), Diagnostic>> {
        use PlatformIntrinsic as P;
        if !matches!(
            receiver_type,
            TypeName::ParentJobResult
                | TypeName::Quiddity
                | TypeName::TriggerOperation
                | TypeName::LoggingLevel
                | TypeName::CacheVisibility
                | TypeName::SoapType
                | TypeName::DisplayType
        ) {
            return None;
        }
        Some((|| {
            let (intrinsic, result) = match (receiver_type, method.canonical.as_str()) {
                (_, "name") => (P::PlatformEnumName, TypeName::String),
                (TypeName::LoggingLevel | TypeName::TriggerOperation, "ordinal") => {
                    (P::PlatformEnumOrdinal, TypeName::Integer)
                }
                _ => return Err(self.unsupported_instance_platform_api(receiver_type, method)),
            };
            require_arity(
                receiver_type,
                &method.spelling,
                arguments.len(),
                &[0],
                arguments,
            )?;
            Ok((
                IntrinsicId::Platform(intrinsic),
                ExpressionType::Value(result),
            ))
        })())
    }

    fn schema_instance_method_type(
        &mut self,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Option<Result<(IntrinsicId, ExpressionType), Diagnostic>> {
        use PlatformIntrinsic as P;
        let (intrinsic, result) = match (receiver_type, method.canonical.as_str()) {
            (TypeName::SObjectType, "getdescribe") => {
                (P::SObjectTypeGetDescribe, TypeName::DescribeSObjectResult)
            }
            (TypeName::SObjectType, "getname") => (P::SObjectTypeGetName, TypeName::String),
            (TypeName::SObjectType, "newsobject") => (
                P::SObjectTypeNewSObject,
                TypeName::Custom(crate::ast::NamedType::new(
                    "SObject".to_owned(),
                    method.span,
                )),
            ),
            (TypeName::SObjectType | TypeName::SObjectField, "tostring") => {
                (P::ObjectToString, TypeName::String)
            }
            (TypeName::SObjectField, "getdescribe") => {
                (P::SObjectFieldGetDescribe, TypeName::DescribeFieldResult)
            }
            (TypeName::SObjectFieldMap, "getmap") => (
                P::SObjectFieldMapGetMap,
                TypeName::Map(Box::new(TypeName::String), Box::new(TypeName::SObjectField)),
            ),
            (TypeName::FieldSetMap, "getmap") => (
                P::FieldSetMapGetMap,
                TypeName::Map(Box::new(TypeName::String), Box::new(TypeName::FieldSet)),
            ),
            (TypeName::DescribeSObjectResult, "getname") => (P::DescribeGetName, TypeName::String),
            (TypeName::DescribeSObjectResult, "getlocalname") => {
                (P::DescribeGetLocalName, TypeName::String)
            }
            (TypeName::DescribeSObjectResult, "getlabel") => {
                (P::DescribeGetLabel, TypeName::String)
            }
            (TypeName::DescribeSObjectResult, "getlabelplural") => {
                (P::DescribeGetLabelPlural, TypeName::String)
            }
            (TypeName::DescribeSObjectResult, "getkeyprefix") => {
                (P::DescribeGetKeyPrefix, TypeName::String)
            }
            (TypeName::DescribeSObjectResult, "iscustom") => {
                (P::DescribeIsCustom, TypeName::Boolean)
            }
            (TypeName::DescribeSObjectResult, "iscustomsetting") => {
                (P::DescribeIsCustomSetting, TypeName::Boolean)
            }
            (TypeName::DescribeSObjectResult, "isaccessible") => {
                (P::DescribeIsAccessible, TypeName::Boolean)
            }
            (TypeName::DescribeSObjectResult, "isdeletable") => {
                (P::DescribeIsDeletable, TypeName::Boolean)
            }
            (TypeName::DescribeSObjectResult, "isupdateable") => {
                (P::DescribeIsUpdateable, TypeName::Boolean)
            }
            (TypeName::DescribeFieldResult, "getname") => {
                (P::DescribeFieldGetName, TypeName::String)
            }
            (TypeName::DescribeFieldResult, "getlocalname") => {
                (P::DescribeFieldGetLocalName, TypeName::String)
            }
            (TypeName::DescribeFieldResult, "getlabel") => {
                (P::DescribeFieldGetLabel, TypeName::String)
            }
            (TypeName::DescribeFieldResult, "getlength") => {
                (P::DescribeFieldGetLength, TypeName::Integer)
            }
            (TypeName::DescribeFieldResult, "getinlinehelptext") => {
                (P::DescribeFieldGetInlineHelpText, TypeName::String)
            }
            (TypeName::DescribeFieldResult, "getrelationshipname") => {
                (P::DescribeFieldGetRelationshipName, TypeName::String)
            }
            (TypeName::DescribeFieldResult, "getsoaptype") => {
                (P::DescribeFieldGetSoapType, TypeName::SoapType)
            }
            (TypeName::DescribeFieldResult, "gettype") => {
                (P::DescribeFieldGetType, TypeName::DisplayType)
            }
            (TypeName::DescribeFieldResult, "getreferenceto") => (
                P::DescribeFieldGetReferenceTo,
                TypeName::List(Box::new(TypeName::SObjectType)),
            ),
            (TypeName::DescribeFieldResult, "getpicklistvalues") => (
                P::DescribeFieldGetPicklistValues,
                TypeName::List(Box::new(TypeName::PicklistEntry)),
            ),
            (TypeName::DescribeFieldResult, "isnamefield") => {
                (P::DescribeFieldIsNameField, TypeName::Boolean)
            }
            (TypeName::DescribeFieldResult, "issortable") => {
                (P::DescribeFieldIsSortable, TypeName::Boolean)
            }
            (TypeName::DescribeFieldResult, "isaccessible") => {
                (P::DescribeFieldIsAccessible, TypeName::Boolean)
            }
            (TypeName::FieldSet, "getname") => (P::FieldSetGetName, TypeName::String),
            (TypeName::FieldSet, "getlabel") => (P::FieldSetGetLabel, TypeName::String),
            (TypeName::FieldSet, "getnamespace") => (P::FieldSetGetNamespace, TypeName::String),
            (TypeName::FieldSet, "getfields") => (
                P::FieldSetGetFields,
                TypeName::List(Box::new(TypeName::FieldSetMember)),
            ),
            (TypeName::FieldSetMember, "getfieldpath") => {
                (P::FieldSetMemberGetFieldPath, TypeName::String)
            }
            (TypeName::FieldSetMember, "getlabel") => (P::FieldSetMemberGetLabel, TypeName::String),
            (TypeName::FieldSetMember, "getsobjectfield") => {
                (P::FieldSetMemberGetSObjectField, TypeName::SObjectField)
            }
            (TypeName::PicklistEntry, "getvalue") => (P::PicklistEntryGetValue, TypeName::String),
            _ => return None,
        };
        Some((|| {
            let arities = if intrinsic == P::SObjectTypeNewSObject {
                &[0, 1, 2][..]
            } else {
                &[0][..]
            };
            require_arity(
                receiver_type,
                &method.spelling,
                arguments.len(),
                arities,
                arguments,
            )?;
            if intrinsic == P::SObjectTypeNewSObject {
                if let Some(id) = arguments.first() {
                    self.require_named_argument(
                        &receiver_type.apex_name(),
                        &method.spelling,
                        0,
                        id,
                        &TypeName::Id,
                    )?;
                }
                if let Some(load_defaults) = arguments.get(1) {
                    self.require_named_argument(
                        &receiver_type.apex_name(),
                        &method.spelling,
                        1,
                        load_defaults,
                        &TypeName::Boolean,
                    )?;
                }
            }
            Ok((
                IntrinsicId::Platform(intrinsic),
                ExpressionType::Value(result),
            ))
        })())
    }

    fn require_async_implementation(
        &mut self,
        argument: &Expression,
        interface: &str,
    ) -> Result<(), Diagnostic> {
        let actual = self.expression_type(argument)?;
        let ExpressionType::Value(TypeName::Custom(name)) = actual else {
            return Err(Diagnostic::new(
                format!("{interface} submission requires a class instance"),
                argument.span(),
            ));
        };
        let Some(class_id) = self.class_ids.get(&name.canonical).copied() else {
            return Err(Diagnostic::new(
                format!("{interface} submission requires a user class instance"),
                argument.span(),
            ));
        };
        let implements =
            self.classes[class_id]
                .interfaces
                .iter()
                .any(|candidate| match interface {
                    "Queueable" => super::is_queueable_interface(&candidate.canonical),
                    "Batchable" => super::is_batchable_interface(&candidate.canonical),
                    "Schedulable" => super::is_schedulable_interface(&candidate.canonical),
                    _ => false,
                });
        if implements {
            Ok(())
        } else {
            Err(Diagnostic::new(
                format!(
                    "{} does not implement {interface}",
                    TypeName::Custom(name).apex_name()
                ),
                argument.span(),
            ))
        }
    }

    fn require_platform_event_argument(&mut self, argument: &Expression) -> Result<(), Diagnostic> {
        let actual = self.expression_type(argument)?;
        let event_type = match &actual {
            ExpressionType::Value(TypeName::Custom(name)) if name.canonical.ends_with("__e") => {
                true
            }
            ExpressionType::Value(TypeName::List(element)) => {
                matches!(&**element, TypeName::Custom(name) if name.canonical.ends_with("__e"))
            }
            _ => false,
        };
        if event_type {
            Ok(())
        } else {
            Err(Diagnostic::new(
                format!(
                    "EventBus.publish requires a platform event or List of platform events, found {}",
                    actual.apex_name()
                ),
                argument.span(),
            ))
        }
    }

    fn require_all(
        &mut self,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
        expected: &TypeName,
    ) -> Result<(), Diagnostic> {
        for (index, argument) in arguments.iter().enumerate() {
            self.require_named_argument(owner, &method.spelling, index, argument, expected)?;
        }
        Ok(())
    }

    fn one_argument(
        &mut self,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
        expected: &TypeName,
    ) -> Result<(), Diagnostic> {
        require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
        self.require_named_argument(owner, &method.spelling, 0, &arguments[0], expected)
    }

    pub(super) fn require_argument(
        &mut self,
        receiver_type: &TypeName,
        method: &str,
        position: usize,
        argument: &Expression,
        expected: &TypeName,
    ) -> Result<(), Diagnostic> {
        self.require_named_argument(
            &receiver_type.apex_name(),
            method,
            position,
            argument,
            expected,
        )
    }

    fn require_named_argument(
        &mut self,
        owner: &str,
        method: &str,
        position: usize,
        argument: &Expression,
        expected: &TypeName,
    ) -> Result<(), Diagnostic> {
        let actual = self.expression_type(argument)?;
        if self.is_assignable(expected, &actual) {
            Ok(())
        } else {
            Err(argument_type_error(
                owner,
                method,
                position,
                &expected.apex_name(),
                &actual,
                argument.span(),
            ))
        }
    }

    pub(super) fn require_list_or_set_argument(
        &mut self,
        receiver_type: &TypeName,
        method: &str,
        position: usize,
        argument: &Expression,
        expected_element: &TypeName,
    ) -> Result<(), Diagnostic> {
        let actual = self.expression_type(argument)?;
        let is_compatible = match &actual {
            ExpressionType::Value(TypeName::List(element))
            | ExpressionType::Value(TypeName::Set(element)) => element.as_ref() == expected_element,
            ExpressionType::Null => true,
            ExpressionType::Value(_) | ExpressionType::Void => false,
        };
        if is_compatible {
            Ok(())
        } else {
            Err(argument_type_error(
                &receiver_type.apex_name(),
                method,
                position,
                &format!(
                    "List<{}> or Set<{}>",
                    expected_element.apex_name(),
                    expected_element.apex_name()
                ),
                &actual,
                argument.span(),
            ))
        }
    }

    fn require_non_void_argument(
        &mut self,
        owner: &str,
        method: &str,
        position: usize,
        argument: &Expression,
    ) -> Result<(), Diagnostic> {
        let actual = self.expression_type(argument)?;
        if actual != ExpressionType::Void {
            Ok(())
        } else {
            Err(argument_type_error(
                owner,
                method,
                position,
                "a value",
                &actual,
                argument.span(),
            ))
        }
    }

    fn sortable_list_element(&self, element: &TypeName) -> bool {
        matches!(
            element,
            TypeName::String | TypeName::Integer | TypeName::Long
        ) || matches!(
            element,
            TypeName::Custom(name)
                if self
                    .class_ids
                    .get(&name.canonical)
                    .is_some_and(|class_id| self.comparable_contracts.contains_key(class_id))
        )
    }
}

fn limit_intrinsic(method: &str) -> Option<LimitIntrinsic> {
    use LimitIntrinsic as L;
    Some(match method {
        "getaggregatequeries" => L::AggregateQueries,
        "getfetchcallsonapexcursor" => L::ApexCursorFetchCalls,
        "getapexcursorrows" => L::ApexCursorRows,
        "getasynccalls" => L::AsyncCalls,
        "getcallouts" => L::Callouts,
        "getcputime" => L::CpuTime,
        "getdmlrows" => L::DmlRows,
        "getdmlstatements" => L::DmlStatements,
        "getemailinvocations" => L::EmailInvocations,
        "getfuturecalls" => L::FutureCalls,
        "getheapsize" => L::HeapSize,
        "getmobilepushapexcalls" => L::MobilePushApexCalls,
        "getpublishimmediatedml" => L::PublishImmediateDml,
        "getqueries" => L::Queries,
        "getquerylocatorrows" => L::QueryLocatorRows,
        "getqueryrows" => L::QueryRows,
        "getqueueablejobs" => L::QueueableJobs,
        "getsoslqueries" => L::SoslQueries,
        "getlimitaggregatequeries" => L::LimitAggregateQueries,
        "getlimitfetchcallsonapexcursor" => L::LimitApexCursorFetchCalls,
        "getlimitapexcursorrows" => L::LimitApexCursorRows,
        "getlimitasynccalls" => L::LimitAsyncCalls,
        "getlimitcallouts" => L::LimitCallouts,
        "getlimitcputime" => L::LimitCpuTime,
        "getlimitdmlrows" => L::LimitDmlRows,
        "getlimitdmlstatements" => L::LimitDmlStatements,
        "getlimitemailinvocations" => L::LimitEmailInvocations,
        "getlimitfuturecalls" => L::LimitFutureCalls,
        "getlimitheapsize" => L::LimitHeapSize,
        "getlimitmobilepushapexcalls" => L::LimitMobilePushApexCalls,
        "getlimitpublishimmediatedml" => L::LimitPublishImmediateDml,
        "getlimitqueries" => L::LimitQueries,
        "getlimitquerylocatorrows" => L::LimitQueryLocatorRows,
        "getlimitqueryrows" => L::LimitQueryRows,
        "getlimitqueueablejobs" => L::LimitQueueableJobs,
        "getlimitsoslqueries" => L::LimitSoslQueries,
        _ => return None,
    })
}

pub(super) fn unknown_method(receiver_type: &TypeName, method: &Identifier) -> Diagnostic {
    Diagnostic::new(
        format!(
            "unknown method `{}` on {}",
            method.spelling,
            receiver_type.apex_name()
        ),
        method.span,
    )
}

fn unknown_static_method(owner: &str, method: &Identifier) -> Diagnostic {
    Diagnostic::new(
        format!("unknown static method `{}` on {}", method.spelling, owner),
        method.span,
    )
}

pub(super) fn require_arity(
    receiver_type: &TypeName,
    method: &str,
    actual: usize,
    expected: &[usize],
    arguments: &[Expression],
) -> Result<(), Diagnostic> {
    require_arity_named(
        &receiver_type.apex_name(),
        method,
        actual,
        expected,
        arguments,
    )
}

fn require_static_arity(
    owner: &str,
    method: &Identifier,
    actual: usize,
    expected: &[usize],
    arguments: &[Expression],
) -> Result<(), Diagnostic> {
    require_arity_named(owner, &method.spelling, actual, expected, arguments).map_err(
        |mut error| {
            if arguments.is_empty() {
                error.span = method.span;
            }
            error
        },
    )
}

fn require_arity_named(
    owner: &str,
    method: &str,
    actual: usize,
    expected: &[usize],
    arguments: &[Expression],
) -> Result<(), Diagnostic> {
    if expected.contains(&actual) {
        return Ok(());
    }
    let expected = expected
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(" or ");
    Err(Diagnostic::new(
        format!("method `{method}` on {owner} expects {expected} arguments, found {actual}"),
        arguments.first().map_or(Span::new(0, 0), Expression::span),
    ))
}

fn argument_type_error(
    owner: &str,
    method: &str,
    position: usize,
    expected: &str,
    actual: &ExpressionType,
    span: Span,
) -> Diagnostic {
    Diagnostic::new(
        format!(
            "argument {} to `{}` on {} expects {}, found {}",
            position + 1,
            method,
            owner,
            expected,
            actual.name()
        ),
        span,
    )
}
