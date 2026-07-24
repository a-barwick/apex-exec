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
            ListIntrinsic::Add => self.list_add_type(receiver_type, element, method, arguments),
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
                self.map_put_all_type(receiver_type, key, value, method, arguments)
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

    fn list_add_type(
        &mut self,
        receiver_type: &TypeName,
        element: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        require_arity(
            receiver_type,
            &method.spelling,
            arguments.len(),
            &[1, 2],
            arguments,
        )?;
        let (index, expected) = if arguments.len() == 2 {
            self.require_argument(
                receiver_type,
                &method.spelling,
                0,
                &arguments[0],
                &TypeName::Integer,
            )?;
            (1, element)
        } else {
            (0, element)
        };
        self.require_argument(
            receiver_type,
            &method.spelling,
            index,
            &arguments[index],
            expected,
        )?;
        Ok(ExpressionType::Void)
    }

    fn map_put_all_type(
        &mut self,
        receiver_type: &TypeName,
        key: &TypeName,
        value: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        require_arity(
            receiver_type,
            &method.spelling,
            arguments.len(),
            &[1],
            arguments,
        )?;
        let actual = self.expression_type(&arguments[0])?;
        if !self.is_assignable(receiver_type, &actual)
            && !self.is_sobject_list_map_source(key, value, &actual)
        {
            self.require_argument(
                receiver_type,
                &method.spelling,
                0,
                &arguments[0],
                receiver_type,
            )?;
        }
        Ok(ExpressionType::Void)
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
            StaticStringIntrinsic::Format => self.static_string_format_type(method, arguments),
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

    fn static_string_format_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
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
            StringIntrinsic::Substring => self.string_substring_type(method, arguments),
            StringIntrinsic::SubstringBefore
            | StringIntrinsic::SubstringAfter
            | StringIntrinsic::SubstringAfterLast => self.string_delimiter_type(method, arguments),
            StringIntrinsic::SubstringBetween => self.string_between_type(method, arguments),
            StringIntrinsic::Left => self.string_left_type(method, arguments),
            StringIntrinsic::Split => self.string_split_type(method, arguments),
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
                self.string_replace_type(method, arguments)
            }
        }?;
        Ok((IntrinsicId::String(intrinsic), result))
    }

    fn string_substring_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        self.string_arguments(method, arguments, &[1, 2], &TypeName::Integer)?;
        Ok(ExpressionType::Value(TypeName::String))
    }

    fn string_delimiter_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        self.string_arguments(method, arguments, &[1], &TypeName::String)?;
        Ok(ExpressionType::Value(TypeName::String))
    }

    fn string_between_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        self.string_arguments(method, arguments, &[2], &TypeName::String)?;
        Ok(ExpressionType::Value(TypeName::String))
    }

    fn string_left_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        self.string_arguments(method, arguments, &[1], &TypeName::Integer)?;
        Ok(ExpressionType::Value(TypeName::String))
    }

    fn string_split_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        require_arity(
            &TypeName::String,
            &method.spelling,
            arguments.len(),
            &[1, 2],
            arguments,
        )?;
        self.require_argument(
            &TypeName::String,
            &method.spelling,
            0,
            &arguments[0],
            &TypeName::String,
        )?;
        if let Some(limit) = arguments.get(1) {
            self.require_argument(
                &TypeName::String,
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

    fn string_replace_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        self.string_arguments(method, arguments, &[2], &TypeName::String)?;
        Ok(ExpressionType::Value(TypeName::String))
    }

    fn string_arguments(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
        expected_arities: &[usize],
        expected_type: &TypeName,
    ) -> Result<(), Diagnostic> {
        require_arity(
            &TypeName::String,
            &method.spelling,
            arguments.len(),
            expected_arities,
            arguments,
        )?;
        for (index, argument) in arguments.iter().enumerate() {
            self.require_argument(
                &TypeName::String,
                &method.spelling,
                index,
                argument,
                expected_type,
            )?;
        }
        Ok(())
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
        if let Some(result) = self.async_system_method_type(method, arguments) {
            return result;
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

    fn async_system_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Option<Result<(IntrinsicId, ExpressionType), Diagnostic>> {
        use PlatformIntrinsic as P;
        let intrinsic = match method.canonical.as_str() {
            "enqueuejob" => P::SystemEnqueueJob,
            "schedule" => P::SystemSchedule,
            "isfuture" => P::SystemIsFuture,
            "isqueueable" => P::SystemIsQueueable,
            "isbatch" => P::SystemIsBatch,
            "isscheduled" => P::SystemIsScheduled,
            _ => return None,
        };
        Some((|| {
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
                _ => unreachable!("only System async intrinsics use this helper"),
            };
            Ok((IntrinsicId::Platform(intrinsic), result))
        })())
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
        let result = self.static_platform_signature_type(intrinsic, owner, method, arguments)?;
        Ok((IntrinsicId::Platform(intrinsic), result))
    }

    fn static_platform_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DateNewInstance | P::DateValueOf | P::DateToday => {
                self.static_date_signature_type(intrinsic, owner, method, arguments)
            }
            P::DatetimeNewInstance
            | P::DatetimeNow
            | P::DatetimeValueOf
            | P::DatetimeValueOfGmt => {
                self.static_datetime_signature_type(intrinsic, owner, method, arguments)
            }
            P::TimeNewInstance | P::TimeValueOf => {
                self.static_time_signature_type(intrinsic, owner, method, arguments)
            }
            P::DecimalValueOf
            | P::DoubleValueOf
            | P::LongValueOf
            | P::IdValueOf
            | P::BlobValueOf => {
                self.static_string_conversion_type(intrinsic, owner, method, arguments)
            }
            P::JsonSerialize
            | P::JsonSerializePretty
            | P::JsonDeserialize
            | P::JsonDeserializeUntyped => {
                self.static_json_signature_type(intrinsic, owner, method, arguments)
            }
            P::PatternCompile | P::PatternQuote | P::SchemaGetGlobalDescribe => {
                self.static_pattern_or_schema_signature_type(intrinsic, owner, method, arguments)
            }
            P::TestStartTest | P::TestStopTest | P::TestIsRunningTest | P::TestSetMock => {
                self.static_test_signature_type(intrinsic, owner, method, arguments)
            }
            P::EncodingBase64Encode
            | P::EncodingBase64Decode
            | P::DatabaseExecuteBatch
            | P::EventBusPublish
            | P::RequestGetCurrent
            | P::CacheGetPartition
            | P::TypeForName => {
                self.static_service_signature_type(intrinsic, owner, method, arguments)
            }
            P::UserInfoGetUserId
            | P::UserInfoGetUserName
            | P::UserInfoGetProfileId
            | P::SecurityStripInaccessible => {
                unreachable!("UserInfo and Security intrinsics were handled above")
            }
            _ => unreachable!("instance intrinsic selected as static"),
        }
    }

    fn static_date_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DateNewInstance => {
                require_static_arity(owner, method, arguments.len(), &[3], arguments)?;
                self.require_all(owner, method, arguments, &TypeName::Integer)?;
            }
            P::DateValueOf => {
                self.static_named_argument(owner, method, arguments, &TypeName::String)?
            }
            P::DateToday => require_static_arity(owner, method, arguments.len(), &[0], arguments)?,
            _ => unreachable!("only Date intrinsics use this helper"),
        }
        Ok(ExpressionType::Value(TypeName::Date))
    }

    fn static_datetime_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DatetimeNewInstance => {
                require_static_arity(owner, method, arguments.len(), &[6], arguments)?;
                self.require_all(owner, method, arguments, &TypeName::Integer)?;
            }
            P::DatetimeNow => {
                require_static_arity(owner, method, arguments.len(), &[0], arguments)?
            }
            P::DatetimeValueOf => {
                self.require_datetime_value_of_argument(owner, method, arguments)?
            }
            P::DatetimeValueOfGmt => {
                self.static_named_argument(owner, method, arguments, &TypeName::String)?;
            }
            _ => unreachable!("only Datetime intrinsics use this helper"),
        }
        Ok(ExpressionType::Value(TypeName::Datetime))
    }

    fn static_time_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::TimeNewInstance => {
                require_static_arity(owner, method, arguments.len(), &[4], arguments)?;
                self.require_all(owner, method, arguments, &TypeName::Integer)?;
            }
            P::TimeValueOf => {
                self.static_named_argument(owner, method, arguments, &TypeName::String)?
            }
            _ => unreachable!("only Time intrinsics use this helper"),
        }
        Ok(ExpressionType::Value(TypeName::Time))
    }

    fn static_string_conversion_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        use PlatformIntrinsic as P;
        self.static_named_argument(owner, method, arguments, &TypeName::String)?;
        let result = match intrinsic {
            P::DecimalValueOf => TypeName::Decimal,
            P::DoubleValueOf => TypeName::Double,
            P::LongValueOf => TypeName::Long,
            P::IdValueOf => TypeName::Id,
            P::BlobValueOf => TypeName::Blob,
            _ => unreachable!("only String conversion intrinsics use this helper"),
        };
        Ok(ExpressionType::Value(result))
    }

    fn static_json_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::JsonSerialize | P::JsonSerializePretty => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_non_void_argument(owner, &method.spelling, 0, &arguments[0])?;
                Ok(ExpressionType::Value(TypeName::String))
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
                Ok(ExpressionType::Value(TypeName::Object))
            }
            P::JsonDeserializeUntyped => {
                self.static_named_argument(owner, method, arguments, &TypeName::String)?;
                Ok(ExpressionType::Value(TypeName::Object))
            }
            _ => unreachable!("only JSON intrinsics use this helper"),
        }
    }

    fn static_pattern_or_schema_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        use PlatformIntrinsic as P;
        let result = match intrinsic {
            P::PatternCompile | P::PatternQuote => {
                self.static_named_argument(owner, method, arguments, &TypeName::String)?;
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
            _ => unreachable!("only Pattern and Schema intrinsics use this helper"),
        };
        Ok(ExpressionType::Value(result))
    }

    fn static_test_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::TestStartTest | P::TestStopTest => {
                require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                Ok(ExpressionType::Void)
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
                Ok(ExpressionType::Void)
            }
            P::TestIsRunningTest => {
                require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                Ok(ExpressionType::Value(TypeName::Boolean))
            }
            _ => unreachable!("only Test intrinsics use this helper"),
        }
    }

    fn static_service_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<ExpressionType, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::EncodingBase64Encode => {
                self.static_named_argument(owner, method, arguments, &TypeName::Blob)?;
                Ok(ExpressionType::Value(TypeName::String))
            }
            P::EncodingBase64Decode => {
                self.static_named_argument(owner, method, arguments, &TypeName::String)?;
                Ok(ExpressionType::Value(TypeName::Blob))
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
                Ok(ExpressionType::Value(TypeName::Id))
            }
            P::EventBusPublish => {
                require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
                self.require_platform_event_argument(&arguments[0])?;
                Ok(ExpressionType::Void)
            }
            P::RequestGetCurrent => {
                require_static_arity(owner, method, arguments.len(), &[0], arguments)?;
                Ok(ExpressionType::Value(TypeName::Request))
            }
            P::CacheGetPartition => {
                self.static_named_argument(owner, method, arguments, &TypeName::String)?;
                Ok(ExpressionType::Value(TypeName::CachePartition))
            }
            P::TypeForName => {
                self.static_named_argument(owner, method, arguments, &TypeName::String)?;
                Ok(ExpressionType::Value(TypeName::Type))
            }
            _ => unreachable!("only service intrinsics use this helper"),
        }
    }

    fn static_named_argument(
        &mut self,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
        expected: &TypeName,
    ) -> Result<(), Diagnostic> {
        require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
        self.require_named_argument(owner, &method.spelling, 0, &arguments[0], expected)
    }

    fn require_datetime_value_of_argument(
        &mut self,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(), Diagnostic> {
        require_static_arity(owner, method, arguments.len(), &[1], arguments)?;
        let argument = self.expression_type(&arguments[0])?;
        if matches!(
            argument,
            ExpressionType::Value(TypeName::String | TypeName::Long)
        ) {
            return Ok(());
        }
        Err(Diagnostic::new(
            format!(
                "{}.{} argument 1 expects String or Long, found {}",
                owner,
                method.spelling,
                argument.apex_name()
            ),
            arguments[0].span(),
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
        let Some(intrinsic) = platform_instance_intrinsic(receiver_type, &method.canonical) else {
            return Err(self.unsupported_instance_platform_api(receiver_type, method));
        };
        let owner = receiver_type.apex_name();
        let result = match intrinsic {
            P::DateAddDays
            | P::DateAddMonths
            | P::DateAddYears
            | P::DateDaysBetween
            | P::DateFormat
            | P::DateYear
            | P::DateMonth
            | P::DateDay => {
                self.date_instance_signature_type(intrinsic, receiver_type, method, arguments)?
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
                self.datetime_instance_signature_type(intrinsic, receiver_type, method, arguments)?
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
                self.time_instance_signature_type(intrinsic, receiver_type, method, arguments)?
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
            P::IdGetSObjectType => {
                require_arity(
                    receiver_type,
                    &method.spelling,
                    arguments.len(),
                    &[0],
                    arguments,
                )?;
                TypeName::SObjectType
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
            | P::HttpRequestGetEndpoint
            | P::HttpRequestGetMethod
            | P::HttpRequestGetBody
            | P::HttpRequestSetHeader
            | P::HttpRequestGetHeader
            | P::HttpRequestSetTimeout
            | P::HttpRequestGetTimeout
            | P::HttpResponseSetStatusCode
            | P::HttpResponseGetStatusCode
            | P::HttpResponseSetBody
            | P::HttpResponseGetBody
            | P::HttpResponseSetHeader
            | P::HttpResponseGetHeader
            | P::HttpResponseSetStatus
            | P::HttpResponseGetStatus
            | P::HttpSend
            | P::HttpCalloutMockRespond => {
                return self.http_instance_signature_type(
                    intrinsic,
                    receiver_type,
                    method,
                    arguments,
                );
            }
            P::AsyncContextGetJobId
            | P::BatchableContextGetChildJobId
            | P::FinalizerContextGetAsyncApexJobId
            | P::SchedulableContextGetTriggerId
            | P::FinalizerContextGetException
            | P::FinalizerContextGetResult
            | P::FinalizerContextGetRequestId
            | P::RequestGetRequestId
            | P::TypeGetName
            | P::RequestGetQuiddity => {
                return self.async_context_signature_type(
                    intrinsic,
                    receiver_type,
                    method,
                    arguments,
                );
            }
            P::CachePartitionContains
            | P::CachePartitionGet
            | P::CachePartitionIsAvailable
            | P::CachePartitionPut
            | P::CachePartitionRemove => {
                return self.cache_partition_signature_type(
                    intrinsic,
                    receiver_type,
                    method,
                    arguments,
                );
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

    fn date_instance_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<TypeName, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DateAddDays | P::DateAddMonths | P::DateAddYears => {
                self.one_argument(
                    &receiver_type.apex_name(),
                    method,
                    arguments,
                    &TypeName::Integer,
                )?;
                Ok(TypeName::Date)
            }
            P::DateDaysBetween => {
                self.one_argument(
                    &receiver_type.apex_name(),
                    method,
                    arguments,
                    &TypeName::Date,
                )?;
                Ok(TypeName::Integer)
            }
            P::DateFormat => {
                self.platform_zero_arity(receiver_type, method, arguments)?;
                Ok(TypeName::String)
            }
            P::DateYear | P::DateMonth | P::DateDay => {
                self.platform_zero_arity(receiver_type, method, arguments)?;
                Ok(TypeName::Integer)
            }
            _ => unreachable!("only Date intrinsics use this helper"),
        }
    }

    fn datetime_instance_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<TypeName, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::DatetimeGetTime => {
                self.platform_zero_arity(receiver_type, method, arguments)?;
                Ok(TypeName::Long)
            }
            P::DatetimeDate | P::DatetimeDateGmt => {
                self.platform_zero_arity(receiver_type, method, arguments)?;
                Ok(TypeName::Date)
            }
            P::DatetimeTime | P::DatetimeTimeGmt => {
                self.platform_zero_arity(receiver_type, method, arguments)?;
                Ok(TypeName::Time)
            }
            P::DatetimeAddDays
            | P::DatetimeAddHours
            | P::DatetimeAddMinutes
            | P::DatetimeAddSeconds => {
                self.one_argument(
                    &receiver_type.apex_name(),
                    method,
                    arguments,
                    &TypeName::Integer,
                )?;
                Ok(TypeName::Datetime)
            }
            P::DatetimeFormat => {
                self.platform_zero_arity(receiver_type, method, arguments)?;
                Ok(TypeName::String)
            }
            _ => unreachable!("only Datetime intrinsics use this helper"),
        }
    }

    fn time_instance_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<TypeName, Diagnostic> {
        use PlatformIntrinsic as P;
        match intrinsic {
            P::TimeAddHours | P::TimeAddMinutes | P::TimeAddSeconds | P::TimeAddMilliseconds => {
                self.one_argument(
                    &receiver_type.apex_name(),
                    method,
                    arguments,
                    &TypeName::Integer,
                )?;
                Ok(TypeName::Time)
            }
            P::TimeHour | P::TimeMinute | P::TimeSecond | P::TimeMillisecond => {
                self.platform_zero_arity(receiver_type, method, arguments)?;
                Ok(TypeName::Integer)
            }
            P::TimeFormat => {
                self.platform_zero_arity(receiver_type, method, arguments)?;
                Ok(TypeName::String)
            }
            _ => unreachable!("only Time intrinsics use this helper"),
        }
    }

    fn platform_zero_arity(
        &self,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(), Diagnostic> {
        require_arity(
            receiver_type,
            &method.spelling,
            arguments.len(),
            &[0],
            arguments,
        )
    }

    fn http_instance_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        use PlatformIntrinsic as P;
        let owner = receiver_type.apex_name();
        let result = match intrinsic {
            P::HttpRequestSetEndpoint
            | P::HttpRequestSetMethod
            | P::HttpRequestSetBody
            | P::HttpResponseSetBody
            | P::HttpResponseSetStatus => {
                self.one_argument(&owner, method, arguments, &TypeName::String)?;
                ExpressionType::Void
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
                ExpressionType::Void
            }
            P::HttpRequestSetTimeout | P::HttpResponseSetStatusCode => {
                self.one_argument(&owner, method, arguments, &TypeName::Integer)?;
                ExpressionType::Void
            }
            P::HttpRequestGetEndpoint
            | P::HttpRequestGetMethod
            | P::HttpRequestGetBody
            | P::HttpResponseGetBody
            | P::HttpResponseGetStatus => {
                self.platform_zero_arity(receiver_type, method, arguments)?;
                ExpressionType::Value(TypeName::String)
            }
            P::HttpRequestGetHeader | P::HttpResponseGetHeader => {
                self.one_argument(&owner, method, arguments, &TypeName::String)?;
                ExpressionType::Value(TypeName::String)
            }
            P::HttpRequestGetTimeout | P::HttpResponseGetStatusCode => {
                self.platform_zero_arity(receiver_type, method, arguments)?;
                ExpressionType::Value(TypeName::Integer)
            }
            P::HttpSend | P::HttpCalloutMockRespond => {
                self.one_argument(&owner, method, arguments, &TypeName::HttpRequest)?;
                ExpressionType::Value(TypeName::HttpResponse)
            }
            _ => unreachable!("only HTTP intrinsics use this helper"),
        };
        Ok((IntrinsicId::Platform(intrinsic), result))
    }

    fn async_context_signature_type(
        &self,
        intrinsic: PlatformIntrinsic,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        use PlatformIntrinsic as P;
        self.platform_zero_arity(receiver_type, method, arguments)?;
        let result = match intrinsic {
            P::AsyncContextGetJobId
            | P::BatchableContextGetChildJobId
            | P::FinalizerContextGetAsyncApexJobId
            | P::SchedulableContextGetTriggerId => TypeName::Id,
            P::FinalizerContextGetException => TypeName::Exception,
            P::FinalizerContextGetResult => TypeName::ParentJobResult,
            P::FinalizerContextGetRequestId | P::RequestGetRequestId | P::TypeGetName => {
                TypeName::String
            }
            P::RequestGetQuiddity => TypeName::Quiddity,
            _ => unreachable!("only async-context intrinsics use this helper"),
        };
        Ok((
            IntrinsicId::Platform(intrinsic),
            ExpressionType::Value(result),
        ))
    }

    fn cache_partition_signature_type(
        &mut self,
        intrinsic: PlatformIntrinsic,
        receiver_type: &TypeName,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        use PlatformIntrinsic as P;
        let owner = receiver_type.apex_name();
        let result = match intrinsic {
            P::CachePartitionContains => {
                self.one_argument(&owner, method, arguments, &TypeName::String)?;
                ExpressionType::Value(TypeName::Boolean)
            }
            P::CachePartitionGet => {
                self.one_argument(&owner, method, arguments, &TypeName::String)?;
                ExpressionType::Value(TypeName::Object)
            }
            P::CachePartitionIsAvailable => {
                self.platform_zero_arity(receiver_type, method, arguments)?;
                ExpressionType::Value(TypeName::Boolean)
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
                self.cache_partition_optional_arguments(&owner, method, arguments)?;
                ExpressionType::Void
            }
            P::CachePartitionRemove => {
                self.one_argument(&owner, method, arguments, &TypeName::String)?;
                ExpressionType::Void
            }
            _ => unreachable!("only CachePartition intrinsics use this helper"),
        };
        Ok((IntrinsicId::Platform(intrinsic), result))
    }

    fn cache_partition_optional_arguments(
        &mut self,
        owner: &str,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(), Diagnostic> {
        for (index, expected) in [
            (2, TypeName::Integer),
            (3, TypeName::CacheVisibility),
            (4, TypeName::Boolean),
        ] {
            if let Some(argument) = arguments.get(index) {
                self.require_named_argument(owner, &method.spelling, index, argument, &expected)?;
            }
        }
        Ok(())
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
        let descriptor =
            schema_owner_method_descriptor(receiver_type, &method.canonical, method.span)
                .or_else(|| describe_method_descriptor(receiver_type, &method.canonical))
                .or_else(|| field_set_method_descriptor(receiver_type, &method.canonical));
        let (intrinsic, result) = descriptor?;
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

fn platform_instance_intrinsic(
    receiver_type: &TypeName,
    method: &str,
) -> Option<PlatformIntrinsic> {
    date_instance_intrinsic(receiver_type, method)
        .or_else(|| datetime_instance_intrinsic(receiver_type, method))
        .or_else(|| time_instance_intrinsic(receiver_type, method))
        .or_else(|| scalar_instance_intrinsic(receiver_type, method))
        .or_else(|| regex_instance_intrinsic(receiver_type, method))
        .or_else(|| http_instance_intrinsic(receiver_type, method))
        .or_else(|| visual_editor_instance_intrinsic(receiver_type, method))
        .or_else(|| async_context_instance_intrinsic(receiver_type, method))
        .or_else(|| cache_and_type_instance_intrinsic(receiver_type, method))
}

fn date_instance_intrinsic(receiver_type: &TypeName, method: &str) -> Option<PlatformIntrinsic> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::Date, "adddays") => Some(P::DateAddDays),
        (TypeName::Date, "addmonths") => Some(P::DateAddMonths),
        (TypeName::Date, "addyears") => Some(P::DateAddYears),
        (TypeName::Date, "daysbetween") => Some(P::DateDaysBetween),
        (TypeName::Date, "format") => Some(P::DateFormat),
        (TypeName::Date, "year") => Some(P::DateYear),
        (TypeName::Date, "month") => Some(P::DateMonth),
        (TypeName::Date, "day") => Some(P::DateDay),
        _ => None,
    }
}

fn datetime_instance_intrinsic(
    receiver_type: &TypeName,
    method: &str,
) -> Option<PlatformIntrinsic> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::Datetime, "gettime") => Some(P::DatetimeGetTime),
        (TypeName::Datetime, "date") => Some(P::DatetimeDate),
        (TypeName::Datetime, "dategmt") => Some(P::DatetimeDateGmt),
        (TypeName::Datetime, "time") => Some(P::DatetimeTime),
        (TypeName::Datetime, "timegmt") => Some(P::DatetimeTimeGmt),
        (TypeName::Datetime, "adddays") => Some(P::DatetimeAddDays),
        (TypeName::Datetime, "addhours") => Some(P::DatetimeAddHours),
        (TypeName::Datetime, "addminutes") => Some(P::DatetimeAddMinutes),
        (TypeName::Datetime, "addseconds") => Some(P::DatetimeAddSeconds),
        (TypeName::Datetime, "format") => Some(P::DatetimeFormat),
        _ => None,
    }
}

fn time_instance_intrinsic(receiver_type: &TypeName, method: &str) -> Option<PlatformIntrinsic> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::Time, "addhours") => Some(P::TimeAddHours),
        (TypeName::Time, "addminutes") => Some(P::TimeAddMinutes),
        (TypeName::Time, "addseconds") => Some(P::TimeAddSeconds),
        (TypeName::Time, "addmilliseconds") => Some(P::TimeAddMilliseconds),
        (TypeName::Time, "hour") => Some(P::TimeHour),
        (TypeName::Time, "minute") => Some(P::TimeMinute),
        (TypeName::Time, "second") => Some(P::TimeSecond),
        (TypeName::Time, "millisecond") => Some(P::TimeMillisecond),
        (TypeName::Time, "format") => Some(P::TimeFormat),
        _ => None,
    }
}

fn scalar_instance_intrinsic(receiver_type: &TypeName, method: &str) -> Option<PlatformIntrinsic> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::Decimal, "setscale") => Some(P::DecimalSetScale),
        (TypeName::Decimal, "abs") => Some(P::DecimalAbs),
        (TypeName::Decimal, "scale") => Some(P::DecimalScale),
        (TypeName::Decimal | TypeName::Double | TypeName::Object, "tostring") => {
            Some(P::ObjectToString)
        }
        (TypeName::Id, "to15") => Some(P::IdTo15),
        (TypeName::Id, "to18") => Some(P::IdTo18),
        (TypeName::Id, "getsobjecttype") => Some(P::IdGetSObjectType),
        (TypeName::Blob, "tostring") => Some(P::BlobToString),
        (TypeName::Blob, "size") => Some(P::BlobSize),
        _ => None,
    }
}

fn regex_instance_intrinsic(receiver_type: &TypeName, method: &str) -> Option<PlatformIntrinsic> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::Pattern, "matcher") => Some(P::PatternMatcher),
        (TypeName::Matcher, "matches") => Some(P::MatcherMatches),
        (TypeName::Matcher, "find") => Some(P::MatcherFind),
        (TypeName::Matcher, "group") => Some(P::MatcherGroup),
        (TypeName::Matcher, "start") => Some(P::MatcherStart),
        (TypeName::Matcher, "end") => Some(P::MatcherEnd),
        _ => None,
    }
}

fn http_instance_intrinsic(receiver_type: &TypeName, method: &str) -> Option<PlatformIntrinsic> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::HttpRequest, "setendpoint") => Some(P::HttpRequestSetEndpoint),
        (TypeName::HttpRequest, "getendpoint") => Some(P::HttpRequestGetEndpoint),
        (TypeName::HttpRequest, "setmethod") => Some(P::HttpRequestSetMethod),
        (TypeName::HttpRequest, "getmethod") => Some(P::HttpRequestGetMethod),
        (TypeName::HttpRequest, "setbody") => Some(P::HttpRequestSetBody),
        (TypeName::HttpRequest, "getbody") => Some(P::HttpRequestGetBody),
        (TypeName::HttpRequest, "setheader") => Some(P::HttpRequestSetHeader),
        (TypeName::HttpRequest, "getheader") => Some(P::HttpRequestGetHeader),
        (TypeName::HttpRequest, "settimeout") => Some(P::HttpRequestSetTimeout),
        (TypeName::HttpRequest, "gettimeout") => Some(P::HttpRequestGetTimeout),
        (TypeName::HttpResponse, "setstatuscode") => Some(P::HttpResponseSetStatusCode),
        (TypeName::HttpResponse, "getstatuscode") => Some(P::HttpResponseGetStatusCode),
        (TypeName::HttpResponse, "setbody") => Some(P::HttpResponseSetBody),
        (TypeName::HttpResponse, "getbody") => Some(P::HttpResponseGetBody),
        (TypeName::HttpResponse, "setheader") => Some(P::HttpResponseSetHeader),
        (TypeName::HttpResponse, "getheader") => Some(P::HttpResponseGetHeader),
        (TypeName::HttpResponse, "setstatus") => Some(P::HttpResponseSetStatus),
        (TypeName::HttpResponse, "getstatus") => Some(P::HttpResponseGetStatus),
        (TypeName::Http, "send") => Some(P::HttpSend),
        (TypeName::HttpCalloutMock, "respond") => Some(P::HttpCalloutMockRespond),
        _ => None,
    }
}

fn visual_editor_instance_intrinsic(
    receiver_type: &TypeName,
    method: &str,
) -> Option<PlatformIntrinsic> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::VisualEditorDataRow, "getlabel") => Some(P::VisualEditorDataRowGetLabel),
        (TypeName::VisualEditorDataRow, "getvalue") => Some(P::VisualEditorDataRowGetValue),
        (TypeName::VisualEditorDynamicPickListRows, "addrow") => Some(P::VisualEditorRowsAddRow),
        (TypeName::VisualEditorDynamicPickListRows, "getdatarows") => {
            Some(P::VisualEditorRowsGetDataRows)
        }
        _ => None,
    }
}

fn async_context_instance_intrinsic(
    receiver_type: &TypeName,
    method: &str,
) -> Option<PlatformIntrinsic> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::QueueableContext | TypeName::BatchableContext, "getjobid") => {
            Some(P::AsyncContextGetJobId)
        }
        (TypeName::BatchableContext, "getchildjobid") => Some(P::BatchableContextGetChildJobId),
        (TypeName::FinalizerContext, "getasyncapexjobid") => {
            Some(P::FinalizerContextGetAsyncApexJobId)
        }
        (TypeName::FinalizerContext, "getexception") => Some(P::FinalizerContextGetException),
        (TypeName::FinalizerContext, "getresult") => Some(P::FinalizerContextGetResult),
        (TypeName::FinalizerContext, "getrequestid") => Some(P::FinalizerContextGetRequestId),
        (TypeName::SchedulableContext, "gettriggerid") => Some(P::SchedulableContextGetTriggerId),
        (TypeName::Request, "getrequestid") => Some(P::RequestGetRequestId),
        (TypeName::Request, "getquiddity") => Some(P::RequestGetQuiddity),
        _ => None,
    }
}

fn cache_and_type_instance_intrinsic(
    receiver_type: &TypeName,
    method: &str,
) -> Option<PlatformIntrinsic> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::CachePartition, "contains") => Some(P::CachePartitionContains),
        (TypeName::CachePartition, "get") => Some(P::CachePartitionGet),
        (TypeName::CachePartition, "isavailable") => Some(P::CachePartitionIsAvailable),
        (TypeName::CachePartition, "put") => Some(P::CachePartitionPut),
        (TypeName::CachePartition, "remove") => Some(P::CachePartitionRemove),
        (TypeName::Callable, "call") => Some(P::CallableCall),
        (TypeName::Type, "getname") => Some(P::TypeGetName),
        (TypeName::Type, "newinstance") => Some(P::TypeNewInstance),
        _ => None,
    }
}

fn schema_owner_method_descriptor(
    receiver_type: &TypeName,
    method: &str,
    span: Span,
) -> Option<(PlatformIntrinsic, TypeName)> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::SObjectType, "getdescribe") => {
            Some((P::SObjectTypeGetDescribe, TypeName::DescribeSObjectResult))
        }
        (TypeName::SObjectType, "getname") => Some((P::SObjectTypeGetName, TypeName::String)),
        (TypeName::SObjectType, "newsobject") => Some((
            P::SObjectTypeNewSObject,
            TypeName::Custom(crate::ast::NamedType::new("SObject".to_owned(), span)),
        )),
        (TypeName::SObjectType | TypeName::SObjectField, "tostring") => {
            Some((P::ObjectToString, TypeName::String))
        }
        (TypeName::SObjectField, "getdescribe") => {
            Some((P::SObjectFieldGetDescribe, TypeName::DescribeFieldResult))
        }
        (TypeName::SObjectFieldMap, "getmap") => Some((
            P::SObjectFieldMapGetMap,
            TypeName::Map(Box::new(TypeName::String), Box::new(TypeName::SObjectField)),
        )),
        (TypeName::FieldSetMap, "getmap") => Some((
            P::FieldSetMapGetMap,
            TypeName::Map(Box::new(TypeName::String), Box::new(TypeName::FieldSet)),
        )),
        _ => None,
    }
}

fn describe_method_descriptor(
    receiver_type: &TypeName,
    method: &str,
) -> Option<(PlatformIntrinsic, TypeName)> {
    describe_sobject_method_descriptor(receiver_type, method)
        .or_else(|| describe_field_method_descriptor(receiver_type, method))
}

fn describe_sobject_method_descriptor(
    receiver_type: &TypeName,
    method: &str,
) -> Option<(PlatformIntrinsic, TypeName)> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::DescribeSObjectResult, "getname") => {
            Some((P::DescribeGetName, TypeName::String))
        }
        (TypeName::DescribeSObjectResult, "getlocalname") => {
            Some((P::DescribeGetLocalName, TypeName::String))
        }
        (TypeName::DescribeSObjectResult, "getlabel") => {
            Some((P::DescribeGetLabel, TypeName::String))
        }
        (TypeName::DescribeSObjectResult, "getlabelplural") => {
            Some((P::DescribeGetLabelPlural, TypeName::String))
        }
        (TypeName::DescribeSObjectResult, "getkeyprefix") => {
            Some((P::DescribeGetKeyPrefix, TypeName::String))
        }
        (TypeName::DescribeSObjectResult, "iscustom") => {
            Some((P::DescribeIsCustom, TypeName::Boolean))
        }
        (TypeName::DescribeSObjectResult, "iscustomsetting") => {
            Some((P::DescribeIsCustomSetting, TypeName::Boolean))
        }
        (TypeName::DescribeSObjectResult, "isaccessible") => {
            Some((P::DescribeIsAccessible, TypeName::Boolean))
        }
        (TypeName::DescribeSObjectResult, "isdeletable") => {
            Some((P::DescribeIsDeletable, TypeName::Boolean))
        }
        (TypeName::DescribeSObjectResult, "isupdateable") => {
            Some((P::DescribeIsUpdateable, TypeName::Boolean))
        }
        _ => None,
    }
}

fn describe_field_method_descriptor(
    receiver_type: &TypeName,
    method: &str,
) -> Option<(PlatformIntrinsic, TypeName)> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::DescribeFieldResult, "getname") => {
            Some((P::DescribeFieldGetName, TypeName::String))
        }
        (TypeName::DescribeFieldResult, "getlocalname") => {
            Some((P::DescribeFieldGetLocalName, TypeName::String))
        }
        (TypeName::DescribeFieldResult, "getlabel") => {
            Some((P::DescribeFieldGetLabel, TypeName::String))
        }
        (TypeName::DescribeFieldResult, "getlength") => {
            Some((P::DescribeFieldGetLength, TypeName::Integer))
        }
        (TypeName::DescribeFieldResult, "getinlinehelptext") => {
            Some((P::DescribeFieldGetInlineHelpText, TypeName::String))
        }
        (TypeName::DescribeFieldResult, "getrelationshipname") => {
            Some((P::DescribeFieldGetRelationshipName, TypeName::String))
        }
        (TypeName::DescribeFieldResult, "getsoaptype") => {
            Some((P::DescribeFieldGetSoapType, TypeName::SoapType))
        }
        (TypeName::DescribeFieldResult, "gettype") => {
            Some((P::DescribeFieldGetType, TypeName::DisplayType))
        }
        (TypeName::DescribeFieldResult, "getreferenceto") => Some((
            P::DescribeFieldGetReferenceTo,
            TypeName::List(Box::new(TypeName::SObjectType)),
        )),
        (TypeName::DescribeFieldResult, "getpicklistvalues") => Some((
            P::DescribeFieldGetPicklistValues,
            TypeName::List(Box::new(TypeName::PicklistEntry)),
        )),
        (TypeName::DescribeFieldResult, "isnamefield") => {
            Some((P::DescribeFieldIsNameField, TypeName::Boolean))
        }
        (TypeName::DescribeFieldResult, "issortable") => {
            Some((P::DescribeFieldIsSortable, TypeName::Boolean))
        }
        (TypeName::DescribeFieldResult, "isaccessible") => {
            Some((P::DescribeFieldIsAccessible, TypeName::Boolean))
        }
        _ => None,
    }
}

fn field_set_method_descriptor(
    receiver_type: &TypeName,
    method: &str,
) -> Option<(PlatformIntrinsic, TypeName)> {
    use PlatformIntrinsic as P;
    match (receiver_type, method) {
        (TypeName::FieldSet, "getname") => Some((P::FieldSetGetName, TypeName::String)),
        (TypeName::FieldSet, "getlabel") => Some((P::FieldSetGetLabel, TypeName::String)),
        (TypeName::FieldSet, "getnamespace") => Some((P::FieldSetGetNamespace, TypeName::String)),
        (TypeName::FieldSet, "getfields") => Some((
            P::FieldSetGetFields,
            TypeName::List(Box::new(TypeName::FieldSetMember)),
        )),
        (TypeName::FieldSetMember, "getfieldpath") => {
            Some((P::FieldSetMemberGetFieldPath, TypeName::String))
        }
        (TypeName::FieldSetMember, "getlabel") => {
            Some((P::FieldSetMemberGetLabel, TypeName::String))
        }
        (TypeName::FieldSetMember, "getsobjectfield") => {
            Some((P::FieldSetMemberGetSObjectField, TypeName::SObjectField))
        }
        (TypeName::PicklistEntry, "getvalue") => Some((P::PicklistEntryGetValue, TypeName::String)),
        _ => None,
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
