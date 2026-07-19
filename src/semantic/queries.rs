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
        CheckedSoslReturning, CheckedValue, DatabaseQueryKind, ExpressionType, QueryResultKind,
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
        let (select, has_aggregate, _) = self.check_soql_select_items(object_id, query)?;
        let group_by = self.check_soql_grouping(object_id, query, &select, has_aggregate)?;
        let condition = query
            .where_clause
            .as_ref()
            .map(|condition| self.check_query_condition(object_id, condition, None))
            .transpose()?;
        let having = self.check_soql_having(object_id, query, &select, &group_by, has_aggregate)?;
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
        let (result, ty) =
            query_result_type(&select, &group_by, has_aggregate, expected, object_type);
        self.queries.insert(
            query.span,
            CheckedQuery::Soql(Box::new(CheckedSoqlQuery {
                object_id,
                select,
                condition,
                group_by,
                having,
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

    fn check_soql_select_items(
        &mut self,
        object_id: usize,
        query: &SoqlQuery,
    ) -> Result<(Vec<CheckedSelectItem>, bool, bool), Diagnostic> {
        let mut select = Vec::new();
        let mut has_aggregate = false;
        let mut has_subquery = false;
        for (index, item) in query.select.iter().enumerate() {
            match item {
                SoqlSelectItem::Field(field) => {
                    select.push(CheckedSelectItem::Field(
                        self.check_query_field(object_id, field)?,
                    ));
                }
                SoqlSelectItem::Subquery { query: child, span } => {
                    has_subquery = true;
                    select.push(self.check_child_subquery(object_id, child, *span)?);
                }
                SoqlSelectItem::Aggregate {
                    function,
                    field,
                    alias,
                    span,
                } => {
                    has_aggregate = true;
                    select.push(self.check_aggregate_select(
                        object_id, *function, field, alias, *span, index,
                    )?);
                }
            }
        }
        if select.is_empty() {
            return Err(Diagnostic::new(
                "SOQL query must select at least one field",
                query.span,
            ));
        }
        if has_aggregate && has_subquery {
            return Err(Diagnostic::new(
                "aggregate SOQL queries cannot select child subqueries",
                query.span,
            ));
        }
        Ok((select, has_aggregate, has_subquery))
    }

    fn check_child_subquery(
        &mut self,
        object_id: usize,
        child: &SoqlQuery,
        span: crate::span::Span,
    ) -> Result<CheckedSelectItem, Diagnostic> {
        if child
            .select
            .iter()
            .any(|item| matches!(item, SoqlSelectItem::Subquery { .. }))
        {
            return Err(Diagnostic::new(
                "nested child SOQL subqueries are not supported",
                span,
            ));
        }
        let (child_object_id, reference_field_id) = self
            .schema
            .child_relationship(object_id, &child.from.spelling)
            .ok_or_else(|| {
                Diagnostic::new(
                    format!(
                        "unknown or ambiguous child relationship `{}`",
                        child.from.spelling
                    ),
                    child.from.span,
                )
            })?;
        let child_object = self
            .schema
            .object_at(child_object_id)
            .expect("checked child object index is valid");
        let mut normalized = child.clone();
        normalized.from = Identifier::new(child_object.api_name().to_owned(), child.from.span);
        self.soql_type(&normalized, None)?;
        let Some(CheckedQuery::Soql(checked)) = self.queries.remove(&normalized.span) else {
            return Err(Diagnostic::new(
                "child SOQL query did not produce a checked plan",
                span,
            ));
        };
        if checked.result != QueryResultKind::Records {
            return Err(Diagnostic::new(
                "child SOQL subqueries must return records",
                span,
            ));
        }
        Ok(CheckedSelectItem::Subquery {
            relationship: child.from.spelling.clone(),
            reference_field_id,
            query: checked,
        })
    }

    fn check_aggregate_select(
        &mut self,
        object_id: usize,
        function: SoqlAggregateFunction,
        field: &Option<FieldPath>,
        alias: &Option<Identifier>,
        span: crate::span::Span,
        index: usize,
    ) -> Result<CheckedSelectItem, Diagnostic> {
        let field = field
            .as_ref()
            .map(|field| self.check_query_field(object_id, field))
            .transpose()?;
        if matches!(
            function,
            SoqlAggregateFunction::Sum | SoqlAggregateFunction::Min | SoqlAggregateFunction::Max
        ) {
            let Some(field) = &field else {
                return Err(Diagnostic::new(
                    "SUM, MIN, and MAX require a field argument",
                    span,
                ));
            };
            if field.field_type != FieldType::Integer {
                return Err(Diagnostic::new(
                    "SUM, MIN, and MAX require an Integer field",
                    span,
                ));
            }
        }
        Ok(CheckedSelectItem::Aggregate {
            function,
            field,
            alias: alias
                .as_ref()
                .map_or_else(|| format!("expr{index}"), |alias| alias.spelling.clone()),
        })
    }

    fn check_soql_grouping(
        &mut self,
        object_id: usize,
        query: &SoqlQuery,
        select: &[CheckedSelectItem],
        has_aggregate: bool,
    ) -> Result<Vec<CheckedFieldPath>, Diagnostic> {
        let group_by = query
            .group_by
            .iter()
            .map(|field| self.check_query_field(object_id, field))
            .collect::<Result<Vec<_>, _>>()?;
        if has_aggregate {
            for item in select {
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
        Ok(group_by)
    }

    fn check_soql_having(
        &mut self,
        object_id: usize,
        query: &SoqlQuery,
        select: &[CheckedSelectItem],
        group_by: &[CheckedFieldPath],
        has_aggregate: bool,
    ) -> Result<Option<CheckedCondition>, Diagnostic> {
        let having = query
            .having
            .as_ref()
            .map(|condition| self.check_query_condition(object_id, condition, Some(select)))
            .transpose()?;
        if having.is_some() && !has_aggregate {
            return Err(Diagnostic::new(
                "`HAVING` requires an aggregate select item",
                query.having.as_ref().expect("having is present").span(),
            ));
        }
        if let Some(having) = &having {
            ensure_having_fields_are_grouped(having, group_by, query.span)?;
        }
        Ok(having)
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
                .map(|condition| self.check_query_condition(object_id, condition, None))
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
        let Some((field, relationship_segments)) = path.segments.split_last() else {
            return Err(Diagnostic::new(
                "query field path cannot be empty",
                path.span,
            ));
        };
        if relationship_segments.len() > 5 {
            return Err(Diagnostic::new(
                "parent relationship paths are limited to five levels",
                path.span,
            ));
        }
        let mut object_id = root_object_id;
        let mut relationships = Vec::with_capacity(relationship_segments.len());
        for relationship in relationship_segments {
            let object = self
                .schema
                .object_at(object_id)
                .expect("checked relationship object index is valid");
            let reference_name =
                relationship_field_name(&relationship.spelling).ok_or_else(|| {
                    Diagnostic::new(
                        "custom parent relationship paths must use a `__r` relationship name",
                        relationship.span,
                    )
                })?;
            let reference_field_id = object.field_index(&reference_name).ok_or_else(|| {
                Diagnostic::new(
                    format!(
                        "unknown relationship `{}` on SObject `{}`",
                        relationship.spelling,
                        object.api_name()
                    ),
                    relationship.span,
                )
            })?;
            let reference = object
                .field_at(reference_field_id)
                .expect("checked relationship field index is valid");
            let FieldType::Reference { target_object } = reference.data_type() else {
                return Err(Diagnostic::new(
                    format!("field `{reference_name}` is not a relationship"),
                    relationship.span,
                ));
            };
            let target_object_id = self.schema.object_index(target_object).ok_or_else(|| {
                Diagnostic::new(
                    format!("unknown relationship target SObject `{target_object}`"),
                    relationship.span,
                )
            })?;
            relationships.push(CheckedRelationship {
                reference_field_id,
                target_object_id,
                spelling: relationship.spelling.clone(),
            });
            object_id = target_object_id;
        }
        let object = self
            .schema
            .object_at(object_id)
            .expect("checked query field object is valid");
        let field_id = object.field_index(&field.spelling).ok_or_else(|| {
            Diagnostic::new(
                format!(
                    "unknown field `{}` on SObject `{}`",
                    field.spelling,
                    object.api_name()
                ),
                field.span,
            )
        })?;
        let schema = object
            .field_at(field_id)
            .expect("checked query field index is valid");
        Ok(CheckedFieldPath {
            root_object_id,
            relationships,
            field_id,
            field_type: schema.data_type().clone(),
        })
    }

    fn check_query_condition(
        &mut self,
        object_id: usize,
        condition: &SoqlCondition,
        aggregates: Option<&[CheckedSelectItem]>,
    ) -> Result<CheckedCondition, Diagnostic> {
        match condition {
            SoqlCondition::AggregateComparison {
                function,
                field,
                operator,
                right,
                span,
            } => self.check_aggregate_condition(
                object_id, *function, field, *operator, right, *span, aggregates,
            ),
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
            } => self.check_in_condition(object_id, field, *negated, values),
            SoqlCondition::Not { condition, .. } => Ok(CheckedCondition::Not(Box::new(
                self.check_query_condition(object_id, condition, aggregates)?,
            ))),
            SoqlCondition::Logical {
                left,
                operator,
                right,
                ..
            } => Ok(CheckedCondition::Logical {
                left: Box::new(self.check_query_condition(object_id, left, aggregates)?),
                operator: *operator,
                right: Box::new(self.check_query_condition(object_id, right, aggregates)?),
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_aggregate_condition(
        &mut self,
        object_id: usize,
        function: SoqlAggregateFunction,
        field: &Option<FieldPath>,
        operator: crate::ast::SoqlComparisonOperator,
        right: &SoqlValue,
        span: crate::span::Span,
        aggregates: Option<&[CheckedSelectItem]>,
    ) -> Result<CheckedCondition, Diagnostic> {
        let Some(aggregates) = aggregates else {
            return Err(Diagnostic::new(
                "aggregate expressions are only supported in `HAVING`",
                span,
            ));
        };
        let field = field
            .as_ref()
            .map(|field| self.check_query_field(object_id, field))
            .transpose()?;
        let alias = aggregates
            .iter()
            .find_map(|item| match item {
                CheckedSelectItem::Aggregate {
                    function: selected_function,
                    field: selected_field,
                    alias,
                } if *selected_function == function && selected_field == &field => {
                    Some(alias.clone())
                }
                _ => None,
            })
            .ok_or_else(|| {
                Diagnostic::new(
                    "`HAVING` aggregate expressions must also appear in `SELECT`",
                    span,
                )
            })?;
        if matches!(operator, crate::ast::SoqlComparisonOperator::Like) {
            return Err(Diagnostic::new(
                "`LIKE` is not valid for aggregate `HAVING` expressions",
                span,
            ));
        }
        let right = self.check_query_value(right, &FieldType::Integer)?;
        Ok(CheckedCondition::AggregateComparison {
            alias,
            operator,
            right,
        })
    }

    fn check_in_condition(
        &mut self,
        object_id: usize,
        field: &FieldPath,
        negated: bool,
        values: &SoqlInValues,
    ) -> Result<CheckedCondition, Diagnostic> {
        let field = self.check_query_field(object_id, field)?;
        let values = match values {
            SoqlInValues::Values(values) => CheckedInValues::Values(
                values
                    .iter()
                    .map(|value| self.check_query_value(value, &field.field_type))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            SoqlInValues::Bind(expression) => {
                self.check_query_collection_bind(expression, &field.field_type)?
            }
        };
        Ok(CheckedCondition::In {
            field,
            negated,
            values,
        })
    }

    fn check_query_collection_bind(
        &mut self,
        expression: &Expression,
        field_type: &FieldType,
    ) -> Result<CheckedInValues, Diagnostic> {
        let actual = self.expression_type(expression)?;
        let expected = super::apex_field_type(field_type);
        let compatible = match &actual {
            ExpressionType::Value(TypeName::List(element))
            | ExpressionType::Value(TypeName::Set(element)) => self.is_subtype(element, &expected),
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
        if !self.dynamic_query {
            return Ok(CheckedInValues::Bind(Box::new(expression.clone())));
        }
        let Expression::Variable(identifier) = expression else {
            return Err(Diagnostic::new(
                "dynamic SOQL collection binds must be simple variable names",
                expression.span(),
            ));
        };
        Ok(CheckedInValues::DynamicBind(identifier.canonical.clone()))
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
            SoqlValue::DateLiteral(literal) => {
                if !matches!(expected, FieldType::Date | FieldType::Datetime) {
                    return Err(query_value_mismatch(expected, "date literal", literal.span));
                }
                CheckedValue::DateLiteral(*literal)
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
                if self.dynamic_query {
                    let Expression::Variable(identifier) = expression.as_ref() else {
                        return Err(Diagnostic::new(
                            "dynamic SOQL binds must be simple variable names",
                            expression.span(),
                        ));
                    };
                    CheckedValue::DynamicBind(identifier.canonical.clone())
                } else {
                    CheckedValue::Bind(Box::new((**expression).clone()))
                }
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
        expected: Option<&TypeName>,
    ) -> Result<ExpressionType, Diagnostic> {
        let query_kind = match method.canonical.as_str() {
            "query" => Some(DatabaseQueryKind::Query),
            "countquery" => Some(DatabaseQueryKind::Count),
            "getquerylocator" => Some(DatabaseQueryKind::QueryLocator),
            _ => None,
        };
        if let Some(kind) = query_kind {
            return self.database_query_method_type(method, arguments, span, expected, kind);
        }
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

    fn database_query_method_type(
        &mut self,
        method: &Identifier,
        arguments: &[Expression],
        span: crate::span::Span,
        expected: Option<&TypeName>,
        kind: DatabaseQueryKind,
    ) -> Result<ExpressionType, Diagnostic> {
        if arguments.len() != 1 {
            return Err(Diagnostic::new(
                format!(
                    "Database.{} expects exactly one query argument",
                    method.spelling
                ),
                method.span,
            ));
        }
        let expected_object_id =
            self.check_database_query_argument(&arguments[0], expected, kind)?;
        self.calls.insert(
            span,
            CallTarget::DatabaseQuery {
                kind,
                expected_object_id,
            },
        );
        Ok(ExpressionType::Value(
            self.database_query_result_type(expected, span, kind),
        ))
    }

    fn check_database_query_argument(
        &mut self,
        argument: &Expression,
        expected: Option<&TypeName>,
        kind: DatabaseQueryKind,
    ) -> Result<Option<usize>, Diagnostic> {
        if kind != DatabaseQueryKind::QueryLocator {
            self.require_operand(argument, &TypeName::String, argument.span())?;
            if kind == DatabaseQueryKind::Query {
                return Ok(expected.and_then(|expected| {
                    let TypeName::List(element) = expected else {
                        return None;
                    };
                    let TypeName::Custom(name) = element.as_ref() else {
                        return None;
                    };
                    self.schema.object_index(&name.spelling)
                }));
            }
            return Ok(None);
        }
        let actual = self.expression_type(argument)?;
        let valid = actual == ExpressionType::Value(TypeName::String)
            || matches!(
                actual,
                ExpressionType::Value(TypeName::List(ref element))
                    if self.is_sobject_type(element)
                        || self.is_dynamic_sobject_type(element)
            );
        if !valid {
            return Err(Diagnostic::new(
                "Database.getQueryLocator requires a String or static SOQL record query",
                argument.span(),
            ));
        }
        Ok(None)
    }

    fn database_query_result_type(
        &self,
        expected: Option<&TypeName>,
        span: crate::span::Span,
        kind: DatabaseQueryKind,
    ) -> TypeName {
        match kind {
            DatabaseQueryKind::Query => expected
                .filter(|expected| {
                    matches!(
                        expected,
                        TypeName::List(element)
                            if self.is_sobject_type(element)
                                || self.is_dynamic_sobject_type(element)
                    )
                })
                .cloned()
                .unwrap_or_else(|| {
                    TypeName::List(Box::new(TypeName::Custom(crate::ast::NamedType::new(
                        "SObject".to_owned(),
                        span,
                    ))))
                }),
            DatabaseQueryKind::Count => TypeName::Integer,
            DatabaseQueryKind::QueryLocator => TypeName::QueryLocator,
        }
    }
}

fn query_result_type(
    select: &[CheckedSelectItem],
    group_by: &[CheckedFieldPath],
    has_aggregate: bool,
    expected: Option<&TypeName>,
    object_type: TypeName,
) -> (QueryResultKind, TypeName) {
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
    if count_scalar {
        return (QueryResultKind::Count, TypeName::Integer);
    }
    if has_aggregate {
        return (
            QueryResultKind::Aggregates,
            TypeName::List(Box::new(TypeName::AggregateResult)),
        );
    }
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
}

fn relationship_field_name(relationship: &str) -> Option<String> {
    relationship
        .strip_suffix("__r")
        .or_else(|| relationship.strip_suffix("__R"))
        .map(|prefix| format!("{prefix}__c"))
}

fn ensure_having_fields_are_grouped(
    condition: &CheckedCondition,
    group_by: &[CheckedFieldPath],
    span: crate::span::Span,
) -> Result<(), Diagnostic> {
    match condition {
        CheckedCondition::AggregateComparison { .. } => {}
        CheckedCondition::Comparison { left, .. } => {
            if !group_by.contains(left) {
                return Err(Diagnostic::new(
                    "`HAVING` fields must appear in `GROUP BY`",
                    span,
                ));
            }
        }
        CheckedCondition::In { field, .. } => {
            if !group_by.contains(field) {
                return Err(Diagnostic::new(
                    "`HAVING` fields must appear in `GROUP BY`",
                    span,
                ));
            }
        }
        CheckedCondition::Not(condition) => {
            ensure_having_fields_are_grouped(condition, group_by, span)?;
        }
        CheckedCondition::Logical { left, right, .. } => {
            ensure_having_fields_are_grouped(left, group_by, span)?;
            ensure_having_fields_are_grouped(right, group_by, span)?;
        }
    }
    Ok(())
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
        FieldType::Date => "Date",
        FieldType::Datetime => "Datetime",
        FieldType::Id => "Id",
        FieldType::Reference { .. } => "relationship",
    }
}
