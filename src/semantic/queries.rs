use super::Checker;
use crate::{
    ast::{
        Expression, FieldPath, Identifier, SoqlAggregateFunction, SoqlCondition, SoqlInValues,
        SoqlQuery, SoqlSelectItem, SoqlValue, SoslQuery, TypeName,
    },
    diagnostic::Diagnostic,
    hir::{
        CallTarget, CheckedCondition, CheckedFieldPath, CheckedInValues, CheckedOrderBy,
        CheckedQuery, CheckedRelationship, CheckedSelectItem, CheckedSoqlQuery, CheckedSoslQuery,
        CheckedSoslReturning, CheckedValue, ExpressionType, QueryResultKind,
    },
    platform::{DataValue, FieldType},
};

impl Checker {
    pub(super) fn sobject_relationship_target(
        &self,
        object_id: usize,
        relationship: &str,
    ) -> Option<(usize, usize)> {
        let reference_name = relationship_field_name(relationship)?;
        let object = self.schema.object_at(object_id)?;
        let reference_field_id = object.field_index(&reference_name)?;
        let reference = object.field_at(reference_field_id)?;
        let FieldType::Reference { target_object } = reference.data_type() else {
            return None;
        };
        let target_object_id = self.schema.object_index(target_object)?;
        Some((reference_field_id, target_object_id))
    }

    pub(super) fn soql_type(
        &mut self,
        query: &SoqlQuery,
        expected: Option<&TypeName>,
    ) -> Result<ExpressionType, Diagnostic> {
        let object_id = self
            .schema
            .object_index(&query.from.spelling)
            .ok_or_else(|| {
                Diagnostic::new(
                    format!("unknown SObject `{}` in SOQL query", query.from.spelling),
                    query.from.span,
                )
            })?;
        let object_type = TypeName::Custom(crate::ast::NamedType::new(
            query.from.spelling.clone(),
            query.from.span,
        ));
        let mut select = Vec::new();
        let mut has_aggregate = false;
        for (index, item) in query.select.iter().enumerate() {
            match item {
                SoqlSelectItem::Field(field) => {
                    select.push(CheckedSelectItem::Field(
                        self.check_query_field(object_id, field)?,
                    ));
                }
                SoqlSelectItem::Aggregate {
                    function,
                    field,
                    alias,
                    span,
                } => {
                    has_aggregate = true;
                    let field = field
                        .as_ref()
                        .map(|field| self.check_query_field(object_id, field))
                        .transpose()?;
                    match function {
                        SoqlAggregateFunction::Count => {}
                        SoqlAggregateFunction::Sum
                        | SoqlAggregateFunction::Min
                        | SoqlAggregateFunction::Max => {
                            let Some(field) = &field else {
                                return Err(Diagnostic::new(
                                    "SUM, MIN, and MAX require a field argument",
                                    *span,
                                ));
                            };
                            if field.field_type != FieldType::Integer {
                                return Err(Diagnostic::new(
                                    "SUM, MIN, and MAX require an Integer field",
                                    *span,
                                ));
                            }
                        }
                    }
                    select.push(CheckedSelectItem::Aggregate {
                        function: *function,
                        field,
                        alias: alias
                            .as_ref()
                            .map_or_else(|| format!("expr{index}"), |alias| alias.spelling.clone()),
                    });
                }
            }
        }
        if select.is_empty() {
            return Err(Diagnostic::new(
                "SOQL query must select at least one field",
                query.span,
            ));
        }

        let group_by = query
            .group_by
            .iter()
            .map(|field| self.check_query_field(object_id, field))
            .collect::<Result<Vec<_>, _>>()?;
        if has_aggregate {
            for item in &select {
                if let CheckedSelectItem::Field(field) = item
                    && !group_by.contains(field)
                {
                    return Err(Diagnostic::new(
                        "non-aggregate selected fields must appear in `GROUP BY`",
                        query.span,
                    ));
                }
            }
        } else if !group_by.is_empty() {
            return Err(Diagnostic::new(
                "`GROUP BY` requires an aggregate select item",
                query.span,
            ));
        }

        let condition = query
            .where_clause
            .as_ref()
            .map(|condition| self.check_query_condition(object_id, condition))
            .transpose()?;
        let order_by = query
            .order_by
            .iter()
            .map(|ordering| {
                Ok(CheckedOrderBy {
                    field: self.check_query_field(object_id, &ordering.field)?,
                    direction: ordering.direction,
                    nulls: ordering.nulls,
                })
            })
            .collect::<Result<Vec<_>, Diagnostic>>()?;
        let limit = query
            .limit
            .as_ref()
            .map(|value| self.check_query_value(value, &FieldType::Integer))
            .transpose()?;
        let offset = query
            .offset
            .as_ref()
            .map(|value| self.check_query_value(value, &FieldType::Integer))
            .transpose()?;

        let count_scalar = select.len() == 1
            && matches!(
                &select[0],
                CheckedSelectItem::Aggregate {
                    function: SoqlAggregateFunction::Count,
                    field: None,
                    ..
                }
            )
            && group_by.is_empty();
        let (result, ty) = if count_scalar {
            (QueryResultKind::Count, TypeName::Integer)
        } else if has_aggregate {
            (
                QueryResultKind::Aggregates,
                TypeName::List(Box::new(TypeName::AggregateResult)),
            )
        } else {
            let single = expected.is_some_and(|expected| expected == &object_type);
            (
                if single {
                    QueryResultKind::RecordSingle
                } else {
                    QueryResultKind::Records
                },
                if single {
                    object_type
                } else {
                    TypeName::List(Box::new(object_type))
                },
            )
        };
        self.queries.insert(
            query.span,
            CheckedQuery::Soql(Box::new(CheckedSoqlQuery {
                object_id,
                select,
                condition,
                group_by,
                order_by,
                limit,
                offset,
                result,
            })),
        );
        let ty = ExpressionType::Value(ty);
        self.expression_types.insert(query.span, ty.clone());
        Ok(ty)
    }

    pub(super) fn sosl_type(&mut self, query: &SoslQuery) -> Result<ExpressionType, Diagnostic> {
        let search = self.check_query_value(&query.search, &FieldType::String)?;
        let mut returning = Vec::new();
        for clause in &query.returning {
            let object_id = self
                .schema
                .object_index(&clause.object.spelling)
                .ok_or_else(|| {
                    Diagnostic::new(
                        format!("unknown SObject `{}` in SOSL query", clause.object.spelling),
                        clause.object.span,
                    )
                })?;
            let fields = clause
                .fields
                .iter()
                .map(|field| self.check_query_field(object_id, field))
                .collect::<Result<Vec<_>, _>>()?;
            let condition = clause
                .where_clause
                .as_ref()
                .map(|condition| self.check_query_condition(object_id, condition))
                .transpose()?;
            let order_by = clause
                .order_by
                .iter()
                .map(|ordering| {
                    Ok(CheckedOrderBy {
                        field: self.check_query_field(object_id, &ordering.field)?,
                        direction: ordering.direction,
                        nulls: ordering.nulls,
                    })
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            let limit = clause
                .limit
                .as_ref()
                .map(|value| self.check_query_value(value, &FieldType::Integer))
                .transpose()?;
            returning.push(CheckedSoslReturning {
                object_id,
                fields,
                condition,
                order_by,
                limit,
            });
        }
        self.queries.insert(
            query.span,
            CheckedQuery::Sosl(Box::new(CheckedSoslQuery {
                search,
                scope: query.scope,
                returning,
            })),
        );
        let dynamic =
            TypeName::Custom(crate::ast::NamedType::new("SObject".to_owned(), query.span));
        let ty = ExpressionType::Value(TypeName::List(Box::new(TypeName::List(Box::new(dynamic)))));
        self.expression_types.insert(query.span, ty.clone());
        Ok(ty)
    }

    fn check_query_field(
        &self,
        root_object_id: usize,
        path: &FieldPath,
    ) -> Result<CheckedFieldPath, Diagnostic> {
        let root = self
            .schema
            .object_at(root_object_id)
            .expect("checked root object index is valid");
        match path.segments.as_slice() {
            [field] => {
                let field_id = root.field_index(&field.spelling).ok_or_else(|| {
                    Diagnostic::new(
                        format!(
                            "unknown field `{}` on SObject `{}`",
                            field.spelling,
                            root.api_name()
                        ),
                        field.span,
                    )
                })?;
                let schema = root
                    .field_at(field_id)
                    .expect("checked field index is valid");
                Ok(CheckedFieldPath {
                    root_object_id,
                    relationship: None,
                    field_id,
                    field_type: schema.data_type().clone(),
                })
            }
            [relationship, field] => {
                let reference_name =
                    relationship_field_name(&relationship.spelling).ok_or_else(|| {
                        Diagnostic::new(
                            "parent relationship paths must use a `__r` relationship name",
                            relationship.span,
                        )
                    })?;
                let reference_field_id = root.field_index(&reference_name).ok_or_else(|| {
                    Diagnostic::new(
                        format!(
                            "unknown relationship `{}` on SObject `{}`",
                            relationship.spelling,
                            root.api_name()
                        ),
                        relationship.span,
                    )
                })?;
                let reference = root
                    .field_at(reference_field_id)
                    .expect("checked relationship field index is valid");
                let FieldType::Reference { target_object } = reference.data_type() else {
                    return Err(Diagnostic::new(
                        format!("field `{reference_name}` is not a relationship"),
                        relationship.span,
                    ));
                };
                let target_object_id =
                    self.schema.object_index(target_object).ok_or_else(|| {
                        Diagnostic::new(
                            format!("unknown relationship target SObject `{target_object}`"),
                            relationship.span,
                        )
                    })?;
                let target = self
                    .schema
                    .object_at(target_object_id)
                    .expect("checked relationship target index is valid");
                let field_id = target.field_index(&field.spelling).ok_or_else(|| {
                    Diagnostic::new(
                        format!(
                            "unknown field `{}` on related SObject `{}`",
                            field.spelling,
                            target.api_name()
                        ),
                        field.span,
                    )
                })?;
                let schema = target
                    .field_at(field_id)
                    .expect("checked related field index is valid");
                Ok(CheckedFieldPath {
                    root_object_id,
                    relationship: Some(CheckedRelationship {
                        reference_field_id,
                        target_object_id,
                        spelling: relationship.spelling.clone(),
                    }),
                    field_id,
                    field_type: schema.data_type().clone(),
                })
            }
            _ => Err(Diagnostic::new(
                "only direct fields and one parent relationship level are supported",
                path.span,
            )),
        }
    }

    fn check_query_condition(
        &mut self,
        object_id: usize,
        condition: &SoqlCondition,
    ) -> Result<CheckedCondition, Diagnostic> {
        match condition {
            SoqlCondition::Comparison {
                left,
                operator,
                right,
                ..
            } => {
                let left = self.check_query_field(object_id, left)?;
                if matches!(operator, crate::ast::SoqlComparisonOperator::Like)
                    && left.field_type != FieldType::String
                {
                    return Err(Diagnostic::new(
                        "`LIKE` requires a String field",
                        right.span(),
                    ));
                }
                let right = self.check_query_value(right, &left.field_type)?;
                Ok(CheckedCondition::Comparison {
                    left,
                    operator: *operator,
                    right,
                })
            }
            SoqlCondition::In {
                field,
                negated,
                values,
                ..
            } => {
                let field = self.check_query_field(object_id, field)?;
                let values = match values {
                    SoqlInValues::Values(values) => CheckedInValues::Values(
                        values
                            .iter()
                            .map(|value| self.check_query_value(value, &field.field_type))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    SoqlInValues::Bind(expression) => {
                        let actual = self.expression_type(expression)?;
                        let expected = super::apex_field_type(&field.field_type);
                        let compatible = match &actual {
                            ExpressionType::Value(TypeName::List(element))
                            | ExpressionType::Value(TypeName::Set(element)) => {
                                self.is_subtype(element, &expected)
                            }
                            _ => false,
                        };
                        if !compatible {
                            return Err(Diagnostic::new(
                                format!(
                                    "SOQL `IN` bind requires List or Set of {}, found {}",
                                    expected.apex_name(),
                                    actual.apex_name()
                                ),
                                expression.span(),
                            ));
                        }
                        CheckedInValues::Bind(Box::new((**expression).clone()))
                    }
                };
                Ok(CheckedCondition::In {
                    field,
                    negated: *negated,
                    values,
                })
            }
            SoqlCondition::Not { condition, .. } => Ok(CheckedCondition::Not(Box::new(
                self.check_query_condition(object_id, condition)?,
            ))),
            SoqlCondition::Logical {
                left,
                operator,
                right,
                ..
            } => Ok(CheckedCondition::Logical {
                left: Box::new(self.check_query_condition(object_id, left)?),
                operator: *operator,
                right: Box::new(self.check_query_condition(object_id, right)?),
            }),
        }
    }

    fn check_query_value(
        &mut self,
        value: &SoqlValue,
        expected: &FieldType,
    ) -> Result<CheckedValue, Diagnostic> {
        let checked = match value {
            SoqlValue::String(value, span) => {
                if !matches!(
                    expected,
                    FieldType::String | FieldType::Id | FieldType::Reference { .. }
                ) {
                    return Err(query_value_mismatch(expected, "String", *span));
                }
                CheckedValue::Literal(DataValue::String(value.clone()))
            }
            SoqlValue::Boolean(value, span) => {
                if expected != &FieldType::Boolean {
                    return Err(query_value_mismatch(expected, "Boolean", *span));
                }
                CheckedValue::Literal(DataValue::Boolean(*value))
            }
            SoqlValue::Integer(value, span) => {
                if expected != &FieldType::Integer {
                    return Err(query_value_mismatch(expected, "Integer", *span));
                }
                CheckedValue::Literal(DataValue::Integer(*value))
            }
            SoqlValue::Null(_) => CheckedValue::Literal(DataValue::Null),
            SoqlValue::Bind(expression, _) => {
                let actual = self.expression_type(expression)?;
                let expected_type = super::apex_field_type(expected);
                if !self.is_assignable(&expected_type, &actual) {
                    return Err(Diagnostic::new(
                        format!(
                            "SOQL bind for {} requires {}, found {}",
                            field_type_name(expected),
                            expected_type.apex_name(),
                            actual.apex_name()
                        ),
                        expression.span(),
                    ));
                }
                CheckedValue::Bind(Box::new((**expression).clone()))
            }
        };
        Ok(checked)
    }

    pub(super) fn check_dml_value(&mut self, value: &Expression) -> Result<(), Diagnostic> {
        let actual = self.expression_type(value)?;
        let compatible = match &actual {
            ExpressionType::Value(ty) => {
                self.is_sobject_type(ty)
                    || self.is_dynamic_sobject_type(ty)
                    || matches!(
                        ty,
                        TypeName::List(element)
                            if self.is_sobject_type(element)
                                || self.is_dynamic_sobject_type(element)
                    )
            }
            ExpressionType::Null | ExpressionType::Void => false,
        };
        if compatible {
            Ok(())
        } else {
            Err(Diagnostic::new(
                format!(
                    "DML requires an SObject or List<SObject>, found {}",
                    actual.apex_name()
                ),
                value.span(),
            ))
        }
    }

    pub(super) fn database_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
        span: crate::span::Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let operation = match method.canonical.as_str() {
            "insert" => crate::ast::DmlOperation::Insert,
            "update" => crate::ast::DmlOperation::Update,
            "upsert" => crate::ast::DmlOperation::Upsert,
            "delete" => crate::ast::DmlOperation::Delete,
            "undelete" => crate::ast::DmlOperation::Undelete,
            _ => {
                return Err(Diagnostic::new(
                    format!("unsupported Database method `{}`", method.spelling),
                    method.span,
                ));
            }
        };
        if !(arguments.len() == 1 || arguments.len() == 2) {
            return Err(Diagnostic::new(
                format!(
                    "Database.{} expects one record argument and optional allOrNone Boolean",
                    method.spelling
                ),
                method.span,
            ));
        }
        self.check_dml_value(&arguments[0])?;
        if arguments.len() == 2 {
            self.require_operand(&arguments[1], &TypeName::Boolean, arguments[1].span())?;
        }
        self.calls.insert(span, CallTarget::DatabaseDml(operation));
        Ok(ExpressionType::Void)
    }
}

fn relationship_field_name(relationship: &str) -> Option<String> {
    relationship
        .strip_suffix("__r")
        .or_else(|| relationship.strip_suffix("__R"))
        .map(|prefix| format!("{prefix}__c"))
}

fn query_value_mismatch(expected: &FieldType, actual: &str, span: crate::span::Span) -> Diagnostic {
    Diagnostic::new(
        format!(
            "SOQL value for {} field cannot be {actual}",
            field_type_name(expected)
        ),
        span,
    )
}

fn field_type_name(field_type: &FieldType) -> &'static str {
    match field_type {
        FieldType::Boolean => "Boolean",
        FieldType::Integer => "Integer",
        FieldType::String => "String",
        FieldType::Id => "Id",
        FieldType::Reference { .. } => "relationship",
    }
}
