use super::{Collection, Interpreter, PlatformHost, SObjectId, Value, runtime_exception};
use crate::{
    ast::{DmlOperation as AstDmlOperation, Expression, SoqlAggregateFunction},
    diagnostic::Diagnostic,
    hir::{
        CheckedCondition, CheckedFieldPath, CheckedInValues, CheckedOrderBy, CheckedQuery,
        CheckedSelectItem, CheckedSoqlQuery, CheckedSoslQuery, CheckedValue, QueryResultKind,
    },
    platform::{
        AggregateFunction, DataValue, DmlOperation, NullOrder, QueryComparison, QueryCondition,
        QueryField, QueryInValues, QueryLogical, QueryOrder, QueryOutcome, QueryRecord,
        QueryRelationship, QuerySelect, SObject, SoqlRequest, SortOrder, SoslRequest,
        SoslReturningRequest,
    },
    span::Span,
};

impl<'program, H: PlatformHost> Interpreter<'program, H> {
    pub(super) fn evaluate_soql(&mut self, span: Span) -> Result<Value, Diagnostic> {
        let Some(CheckedQuery::Soql(query)) = self.program().checked_query(span).cloned() else {
            return Err(Diagnostic::new("missing checked SOQL plan", span));
        };
        let request = self.soql_request(&query, span)?;
        let schema = self.program().schema().clone();
        let outcome = self
            .host
            .soql(&schema, &request)
            .map_err(|error| runtime_exception("QueryException", error.to_string(), span))?;
        self.query_outcome_value(outcome, query.result, span)
    }

    pub(super) fn evaluate_sosl(&mut self, span: Span) -> Result<Value, Diagnostic> {
        let Some(CheckedQuery::Sosl(query)) = self.program().checked_query(span).cloned() else {
            return Err(Diagnostic::new("missing checked SOSL plan", span));
        };
        let request = self.sosl_request(&query, span)?;
        let schema = self.program().schema().clone();
        let outcome = self
            .host
            .sosl(&schema, &request)
            .map_err(|error| runtime_exception("QueryException", error.to_string(), span))?;
        self.query_outcome_value(outcome, QueryResultKind::Records, span)
    }

    pub(super) fn execute_dml(
        &mut self,
        operation: AstDmlOperation,
        expression: &Expression,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let value = self.evaluate(expression)?;
        self.execute_dml_value(operation, value, span)
    }

    pub(super) fn execute_dml_value(
        &mut self,
        operation: AstDmlOperation,
        value: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let handles = match value {
            Value::SObject(id) => vec![id],
            Value::Collection(id) => match self.store.collection(id) {
                Collection::List { elements, .. } => elements
                    .iter()
                    .map(|value| match value {
                        Value::SObject(id) => Ok(*id),
                        _ => Err(runtime_exception(
                            "DmlException",
                            "DML collection contains a null or non-SObject value",
                            span,
                        )),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => {
                    return Err(runtime_exception(
                        "DmlException",
                        "DML requires an SObject or List<SObject>",
                        span,
                    ));
                }
            },
            _ => {
                return Err(runtime_exception(
                    "DmlException",
                    "DML requires a non-null SObject or List<SObject>",
                    span,
                ));
            }
        };
        let schema = self.program().schema().clone();
        let records = handles
            .iter()
            .map(|id| self.platform_sobject(*id, &schema, span))
            .collect::<Result<Vec<_>, _>>()?;
        let operation = map_dml_operation(operation);
        let persisted = self
            .host
            .dml(&schema, operation, records)
            .map_err(|error| runtime_exception("DmlException", error.to_string(), span))?;
        for (id, value) in handles.into_iter().zip(persisted) {
            self.update_runtime_sobject(id, &value, &schema, span)?;
        }
        Ok(())
    }

    pub(super) fn evaluate_aggregate_result_get(
        &mut self,
        receiver: &Expression,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Value::AggregateResult(id) = self.evaluate(receiver)? else {
            return Err(runtime_exception(
                "NullPointerException",
                "AggregateResult receiver is null",
                span,
            ));
        };
        let [name] = arguments else {
            return Err(Diagnostic::new(
                "invalid checked AggregateResult.get call",
                span,
            ));
        };
        let Value::String(name) = self.evaluate(name)? else {
            return Err(runtime_exception(
                "QueryException",
                "AggregateResult field name must be a non-null String",
                span,
            ));
        };
        Ok(self
            .store
            .aggregate_result(id)
            .get(&name.to_ascii_lowercase())
            .map(data_to_value)
            .unwrap_or(Value::Null(Some(crate::ast::TypeName::Object))))
    }

    fn soql_request(
        &mut self,
        query: &CheckedSoqlQuery,
        span: Span,
    ) -> Result<SoqlRequest, Diagnostic> {
        let schema = self.program().schema();
        let object = schema
            .object_at(query.object_id)
            .expect("checked query object is valid")
            .api_name()
            .to_owned();
        Ok(SoqlRequest {
            object,
            select: query
                .select
                .iter()
                .map(|item| self.query_select(item))
                .collect(),
            condition: query
                .condition
                .as_ref()
                .map(|condition| self.query_condition(condition, span))
                .transpose()?,
            group_by: query
                .group_by
                .iter()
                .map(|field| self.query_field(field))
                .collect(),
            order_by: query
                .order_by
                .iter()
                .map(|ordering| self.query_order(ordering))
                .collect(),
            limit: query
                .limit
                .as_ref()
                .map(|value| self.query_usize(value, span))
                .transpose()?,
            offset: query
                .offset
                .as_ref()
                .map(|value| self.query_usize(value, span))
                .transpose()?
                .unwrap_or(0),
            count_scalar: query.result == QueryResultKind::Count,
        })
    }

    fn sosl_request(
        &mut self,
        query: &CheckedSoslQuery,
        span: Span,
    ) -> Result<SoslRequest, Diagnostic> {
        let search = match self.query_value(&query.search, span)? {
            DataValue::String(value) => value,
            _ => {
                return Err(runtime_exception(
                    "QueryException",
                    "SOSL search term must be a non-null String",
                    span,
                ));
            }
        };
        let mut returning = Vec::new();
        for clause in &query.returning {
            let object = self
                .program()
                .schema()
                .object_at(clause.object_id)
                .expect("checked SOSL object is valid")
                .api_name()
                .to_owned();
            returning.push(SoslReturningRequest {
                object,
                fields: clause
                    .fields
                    .iter()
                    .map(|field| self.query_field(field))
                    .collect(),
                condition: clause
                    .condition
                    .as_ref()
                    .map(|condition| self.query_condition(condition, span))
                    .transpose()?,
                order_by: clause
                    .order_by
                    .iter()
                    .map(|ordering| self.query_order(ordering))
                    .collect(),
                limit: clause
                    .limit
                    .as_ref()
                    .map(|value| self.query_usize(value, span))
                    .transpose()?,
            });
        }
        Ok(SoslRequest {
            search,
            name_fields_only: matches!(query.scope, crate::ast::SoslScope::NameFields),
            returning,
        })
    }

    fn query_select(&self, item: &CheckedSelectItem) -> QuerySelect {
        match item {
            CheckedSelectItem::Field(field) => QuerySelect::Field(self.query_field(field)),
            CheckedSelectItem::Aggregate {
                function,
                field,
                alias,
            } => QuerySelect::Aggregate {
                function: match function {
                    SoqlAggregateFunction::Count => AggregateFunction::Count,
                    SoqlAggregateFunction::Sum => AggregateFunction::Sum,
                    SoqlAggregateFunction::Min => AggregateFunction::Min,
                    SoqlAggregateFunction::Max => AggregateFunction::Max,
                },
                field: field.as_ref().map(|field| self.query_field(field)),
                alias: alias.clone(),
            },
        }
    }

    fn query_field(&self, field: &CheckedFieldPath) -> QueryField {
        let schema = self.program().schema();
        let object_id = field
            .relationship
            .as_ref()
            .map_or(field.root_object_id, |relationship| {
                relationship.target_object_id
            });
        let field_name = schema
            .object_at(object_id)
            .expect("checked query field object is valid")
            .field_at(field.field_id)
            .expect("checked query field is valid")
            .api_name()
            .to_owned();
        let relationship = field.relationship.as_ref().map(|relationship| {
            let root = schema
                .object_at(field.root_object_id)
                .expect("checked query root object is valid");
            QueryRelationship {
                reference_field: root
                    .field_at(relationship.reference_field_id)
                    .expect("checked query relationship field is valid")
                    .api_name()
                    .to_owned(),
                target_object: schema
                    .object_at(relationship.target_object_id)
                    .expect("checked query relationship target is valid")
                    .api_name()
                    .to_owned(),
                spelling: relationship.spelling.clone(),
            }
        });
        QueryField {
            relationship,
            field: field_name,
        }
    }

    fn query_condition(
        &mut self,
        condition: &CheckedCondition,
        span: Span,
    ) -> Result<QueryCondition, Diagnostic> {
        Ok(match condition {
            CheckedCondition::Comparison {
                left,
                operator,
                right,
            } => QueryCondition::Comparison {
                left: self.query_field(left),
                operator: match operator {
                    crate::ast::SoqlComparisonOperator::Equal => QueryComparison::Equal,
                    crate::ast::SoqlComparisonOperator::NotEqual => QueryComparison::NotEqual,
                    crate::ast::SoqlComparisonOperator::Less => QueryComparison::Less,
                    crate::ast::SoqlComparisonOperator::LessEqual => QueryComparison::LessEqual,
                    crate::ast::SoqlComparisonOperator::Greater => QueryComparison::Greater,
                    crate::ast::SoqlComparisonOperator::GreaterEqual => {
                        QueryComparison::GreaterEqual
                    }
                    crate::ast::SoqlComparisonOperator::Like => QueryComparison::Like,
                },
                right: self.query_value(right, span)?,
            },
            CheckedCondition::In {
                field,
                negated,
                values,
            } => QueryCondition::In {
                field: self.query_field(field),
                negated: *negated,
                values: QueryInValues::Values(match values {
                    CheckedInValues::Values(values) => values
                        .iter()
                        .map(|value| self.query_value(value, span))
                        .collect::<Result<Vec<_>, _>>()?,
                    CheckedInValues::Bind(expression) => {
                        let value = self.evaluate(expression)?;
                        let Value::Collection(id) = value else {
                            return Err(runtime_exception(
                                "QueryException",
                                "SOQL `IN` bind must evaluate to a collection",
                                span,
                            ));
                        };
                        let elements = match self.store.collection(id) {
                            Collection::List { elements, .. }
                            | Collection::Set { elements, .. } => elements.clone(),
                            Collection::Map { .. } => unreachable!("checker rejected Map bind"),
                        };
                        elements
                            .iter()
                            .map(|value| self.value_to_data(value, span))
                            .collect::<Result<Vec<_>, _>>()?
                    }
                }),
            },
            CheckedCondition::Not(condition) => {
                QueryCondition::Not(Box::new(self.query_condition(condition, span)?))
            }
            CheckedCondition::Logical {
                left,
                operator,
                right,
            } => QueryCondition::Logical {
                left: Box::new(self.query_condition(left, span)?),
                operator: match operator {
                    crate::ast::SoqlLogicalOperator::And => QueryLogical::And,
                    crate::ast::SoqlLogicalOperator::Or => QueryLogical::Or,
                },
                right: Box::new(self.query_condition(right, span)?),
            },
        })
    }

    fn query_order(&self, order: &CheckedOrderBy) -> QueryOrder {
        QueryOrder {
            field: self.query_field(&order.field),
            direction: match order.direction {
                crate::ast::SortDirection::Ascending => SortOrder::Ascending,
                crate::ast::SortDirection::Descending => SortOrder::Descending,
            },
            nulls: order.nulls.map(|nulls| match nulls {
                crate::ast::NullsOrder::First => NullOrder::First,
                crate::ast::NullsOrder::Last => NullOrder::Last,
            }),
        }
    }

    fn query_value(&mut self, value: &CheckedValue, span: Span) -> Result<DataValue, Diagnostic> {
        match value {
            CheckedValue::Literal(value) => Ok(value.clone()),
            CheckedValue::Bind(expression) => {
                let value = self.evaluate(expression)?;
                self.value_to_data(&value, span)
            }
        }
    }

    fn query_usize(&mut self, value: &CheckedValue, span: Span) -> Result<usize, Diagnostic> {
        match self.query_value(value, span)? {
            DataValue::Integer(value) if value >= 0 => usize::try_from(value)
                .map_err(|_| runtime_exception("QueryException", "query limit is too large", span)),
            _ => Err(runtime_exception(
                "QueryException",
                "query LIMIT/OFFSET must be a non-negative Integer",
                span,
            )),
        }
    }

    fn value_to_data(&self, value: &Value, span: Span) -> Result<DataValue, Diagnostic> {
        match value {
            Value::String(value) => Ok(DataValue::String(value.clone())),
            Value::Boolean(value) => Ok(DataValue::Boolean(*value)),
            Value::Integer(value) => Ok(DataValue::Integer(*value)),
            Value::Null(_) => Ok(DataValue::Null),
            _ => Err(runtime_exception(
                "QueryException",
                "query bind evaluated to an unsupported value",
                span,
            )),
        }
    }

    fn query_outcome_value(
        &mut self,
        outcome: QueryOutcome,
        result: QueryResultKind,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match outcome {
            QueryOutcome::Count(value) => Ok(Value::Integer(value)),
            QueryOutcome::Records(records) => {
                let values = records
                    .into_iter()
                    .map(|record| self.allocate_query_record(record, span))
                    .collect::<Result<Vec<_>, _>>()?;
                if result == QueryResultKind::RecordSingle {
                    let [value] = values.as_slice() else {
                        return Err(runtime_exception(
                            "QueryException",
                            format!(
                                "single-row SOQL assignment returned {} records",
                                values.len()
                            ),
                            span,
                        ));
                    };
                    Ok(value.clone())
                } else {
                    let object_type = values.first().and_then(|value| match value {
                        Value::SObject(id) => {
                            let object = self.store.sobject(*id);
                            let schema = self
                                .program()
                                .schema()
                                .object_at(object.object_id)
                                .expect("query result object is valid");
                            Some(crate::ast::TypeName::Custom(crate::ast::NamedType::new(
                                schema.api_name().to_owned(),
                                span,
                            )))
                        }
                        _ => None,
                    });
                    Ok(self.store.allocate_collection(Collection::List {
                        element_type: object_type.unwrap_or_else(|| {
                            crate::ast::TypeName::Custom(crate::ast::NamedType::new(
                                "SObject".to_owned(),
                                span,
                            ))
                        }),
                        elements: values,
                        iteration_depth: 0,
                    }))
                }
            }
            QueryOutcome::Aggregates(rows) => {
                let elements = rows
                    .into_iter()
                    .map(|row| self.store.allocate_aggregate_result(row))
                    .collect();
                Ok(self.store.allocate_collection(Collection::List {
                    element_type: crate::ast::TypeName::AggregateResult,
                    elements,
                    iteration_depth: 0,
                }))
            }
            QueryOutcome::Search(groups) => {
                let dynamic = crate::ast::TypeName::Custom(crate::ast::NamedType::new(
                    "SObject".to_owned(),
                    span,
                ));
                let mut outer = Vec::new();
                for group in groups {
                    let elements = group
                        .into_iter()
                        .map(|record| self.allocate_query_record(record, span))
                        .collect::<Result<Vec<_>, _>>()?;
                    outer.push(self.store.allocate_collection(Collection::List {
                        element_type: dynamic.clone(),
                        elements,
                        iteration_depth: 0,
                    }));
                }
                Ok(self.store.allocate_collection(Collection::List {
                    element_type: crate::ast::TypeName::List(Box::new(dynamic)),
                    elements: outer,
                    iteration_depth: 0,
                }))
            }
        }
    }

    fn allocate_query_record(&mut self, row: QueryRecord, span: Span) -> Result<Value, Diagnostic> {
        let schema = self.program().schema().clone();
        let object_id = schema
            .object_index(row.record.object_api_name())
            .ok_or_else(|| {
                runtime_exception(
                    "QueryException",
                    format!(
                        "unknown query result SObject `{}`",
                        row.record.object_api_name()
                    ),
                    span,
                )
            })?;
        let mut relationships = Vec::new();
        for (reference_field, record) in row.relationships {
            let root = schema
                .object_at(object_id)
                .expect("query result object index is valid");
            let Some(reference_field_id) = root.field_index(&reference_field) else {
                continue;
            };
            let related = self.allocate_record(record, &schema, span)?;
            let Value::SObject(related) = related else {
                unreachable!("records allocate SObjects")
            };
            relationships.push((reference_field_id, related));
        }
        let value = self.allocate_record(row.record, &schema, span)?;
        let Value::SObject(id) = value else {
            unreachable!("records allocate SObjects")
        };
        self.store
            .sobject_mut(id)
            .relationships
            .extend(relationships);
        Ok(Value::SObject(id))
    }

    fn allocate_record(
        &mut self,
        record: crate::platform::Record,
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let object_id = schema
            .object_index(record.object_api_name())
            .ok_or_else(|| {
                runtime_exception(
                    "QueryException",
                    format!("unknown stored SObject `{}`", record.object_api_name()),
                    span,
                )
            })?;
        let value = self.store.allocate_sobject(object_id);
        let Value::SObject(id) = value else {
            unreachable!("SObject allocation returns SObject")
        };
        let object = schema
            .object_at(object_id)
            .expect("stored object index is valid");
        if let Some(id_field) = object.field_index("Id") {
            self.store
                .sobject_mut(id)
                .fields
                .insert(id_field, Value::String(record.id().to_string()));
        }
        for (name, value) in record.fields() {
            if let Some(field_id) = object.field_index(name) {
                self.store
                    .sobject_mut(id)
                    .fields
                    .insert(field_id, data_to_value(value));
            }
        }
        Ok(Value::SObject(id))
    }

    fn platform_sobject(
        &self,
        id: SObjectId,
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<SObject, Diagnostic> {
        let instance = self.store.sobject(id);
        let object = schema
            .object_at(instance.object_id)
            .expect("runtime SObject type is valid");
        let mut value = SObject::new(schema, object.api_name())
            .map_err(|error| runtime_exception("DmlException", error.to_string(), span))?;
        for (field_id, field_value) in &instance.fields {
            let field = object
                .field_at(*field_id)
                .expect("runtime SObject field is valid");
            value
                .set(
                    schema,
                    field.api_name(),
                    self.value_to_dml_data(field_value, span)?,
                )
                .map_err(|error| runtime_exception("DmlException", error.to_string(), span))?;
        }
        Ok(value)
    }

    fn update_runtime_sobject(
        &mut self,
        id: SObjectId,
        value: &SObject,
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let object_id = schema
            .object_index(value.object_api_name())
            .ok_or_else(|| {
                runtime_exception("DmlException", "unknown persisted SObject type", span)
            })?;
        let object = schema
            .object_at(object_id)
            .expect("persisted object index is valid");
        let mut fields = std::collections::BTreeMap::new();
        for (name, value) in value.fields() {
            if let Some(field_id) = object.field_index(name) {
                fields.insert(field_id, data_to_value(value));
            }
        }
        let instance = self.store.sobject_mut(id);
        instance.object_id = object_id;
        instance.fields = fields;
        Ok(())
    }

    fn value_to_dml_data(&self, value: &Value, span: Span) -> Result<DataValue, Diagnostic> {
        match value {
            Value::String(value) => Ok(DataValue::String(value.clone())),
            Value::Boolean(value) => Ok(DataValue::Boolean(*value)),
            Value::Integer(value) => Ok(DataValue::Integer(*value)),
            Value::Null(_) => Ok(DataValue::Null),
            _ => Err(runtime_exception(
                "DmlException",
                "SObject field contains an unsupported DML value",
                span,
            )),
        }
    }
}

fn map_dml_operation(operation: AstDmlOperation) -> DmlOperation {
    match operation {
        AstDmlOperation::Insert => DmlOperation::Insert,
        AstDmlOperation::Update => DmlOperation::Update,
        AstDmlOperation::Upsert => DmlOperation::Upsert,
        AstDmlOperation::Delete => DmlOperation::Delete,
        AstDmlOperation::Undelete => DmlOperation::Undelete,
    }
}

pub(super) fn data_to_value(value: &DataValue) -> Value {
    match value {
        DataValue::Null => Value::Null(None),
        DataValue::Boolean(value) => Value::Boolean(*value),
        DataValue::Integer(value) => Value::Integer(*value),
        DataValue::String(value) => Value::String(value.clone()),
        DataValue::Id(value) => Value::String(value.to_string()),
    }
}
