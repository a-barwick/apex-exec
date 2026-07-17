use super::Checker;
use crate::{
    ast::{Expression, Identifier, TypeName},
    diagnostic::Diagnostic,
    hir::{
        ExceptionIntrinsic, ExpressionType, IntrinsicId, ListIntrinsic, MapIntrinsic,
        MathIntrinsic, SetIntrinsic, StaticStringIntrinsic, StringIntrinsic, SystemIntrinsic,
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
            ListIntrinsic::Clone => {
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
                if !matches!(element, TypeName::String | TypeName::Integer) {
                    return Err(Diagnostic::new(
                        format!(
                            "method `sort` requires List<String> or List<Integer>, found {}",
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
            MapIntrinsic::Clone => {
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
            "startswith" => StringIntrinsic::StartsWith,
            "endswith" => StringIntrinsic::EndsWith,
            "equals" => StringIntrinsic::Equals,
            "equalsignorecase" => StringIntrinsic::EqualsIgnoreCase,
            "indexof" => StringIntrinsic::IndexOf,
            "substring" => StringIntrinsic::Substring,
            "trim" => StringIntrinsic::Trim,
            "tolowercase" => StringIntrinsic::ToLowerCase,
            "touppercase" => StringIntrinsic::ToUpperCase,
            "replace" => StringIntrinsic::Replace,
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
            StringIntrinsic::Replace => {
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
            _ => return Err(unknown_static_method("Math", method)),
        };
        let arity = match intrinsic {
            MathIntrinsic::Abs => 1,
            MathIntrinsic::Max | MathIntrinsic::Min | MathIntrinsic::Mod => 2,
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
            ExpressionType::Value(TypeName::Integer),
        ))
    }

    pub(super) fn static_system_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
    ) -> Result<(IntrinsicId, ExpressionType), Diagnostic> {
        let intrinsic = match method.canonical.as_str() {
            "debug" => SystemIntrinsic::Debug,
            "assert" => SystemIntrinsic::Assert,
            "assertequals" => SystemIntrinsic::AssertEquals,
            "assertnotequals" => SystemIntrinsic::AssertNotEquals,
            _ => return Err(unknown_static_method("System", method)),
        };
        let result = match intrinsic {
            SystemIntrinsic::Debug => {
                require_static_arity("System", method, arguments.len(), &[1], arguments)?;
                self.require_non_void_argument("System", &method.spelling, 0, &arguments[0])?;
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
        }?;
        Ok((IntrinsicId::System(intrinsic), result))
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
