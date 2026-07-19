use super::{
    ActiveCall, Collection, Flow, Interpreter, PlatformHost, PlatformValue, RuntimeTriggerEvent,
    SObjectId, TriggerContext, TriggerPhase, TriggerStage, Value, runtime_exception,
};
use crate::{
    ast::{
        DmlOperation as AstDmlOperation, Expression, SoqlAggregateFunction,
        TriggerEvent as AstTriggerEvent, TypeName,
    },
    diagnostic::Diagnostic,
    hir::{
        CheckedCondition, CheckedFieldPath, CheckedInValues, CheckedOrderBy, CheckedQuery,
        CheckedSelectItem, CheckedSoqlQuery, CheckedSoslQuery, CheckedValue, DatabaseQueryKind,
        QueryResultKind,
    },
    platform::{
        AggregateFunction, DataValue, DmlOperation, NullOrder, QueryComparison, QueryCondition,
        QueryDateLiteral, QueryDateLiteralKind, QueryField, QueryInValues, QueryLogical,
        QueryOrder, QueryOutcome, QueryRecord, QueryRelationship, QuerySelect, QueryValue, SObject,
        SoqlRequest, SortOrder, SoslRequest, SoslReturningRequest,
    },
    span::Span,
};
use chrono::{Duration, NaiveDate, TimeZone, Utc};

impl<'program, H: PlatformHost> Interpreter<'program, H> {
    pub(super) fn evaluate_database_query_call(
        &mut self,
        kind: DatabaseQueryKind,
        expected_object_id: Option<usize>,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let argument = self.evaluate(&arguments[0])?;
        if let Some(locator) = self.static_query_locator(kind, &argument) {
            return Ok(locator);
        }
        let Value::String(source) = argument else {
            return Err(runtime_exception(
                "QueryException",
                "dynamic SOQL query text must evaluate to a non-null String",
                span,
            ));
        };
        let checked = self.check_dynamic_query(&source, expected_object_id, kind, span)?;
        let request = self.soql_request(&checked, span)?;
        let schema = self.program().schema().clone();
        let outcome = self
            .host
            .soql(&schema, &request)
            .map_err(|error| runtime_exception("QueryException", error.to_string(), span))?;
        let value = self.query_outcome_value(outcome, checked.result, span)?;
        self.finish_dynamic_query_value(kind, expected_object_id, value, span)
    }

    fn static_query_locator(&mut self, kind: DatabaseQueryKind, argument: &Value) -> Option<Value> {
        if kind != DatabaseQueryKind::QueryLocator {
            return None;
        }
        let Value::Collection(collection) = argument else {
            return None;
        };
        Some(
            self.store
                .allocate_platform(PlatformValue::QueryLocator(*collection)),
        )
    }

    fn check_dynamic_query(
        &self,
        source: &str,
        expected_object_id: Option<usize>,
        kind: DatabaseQueryKind,
        span: Span,
    ) -> Result<CheckedSoqlQuery, Diagnostic> {
        let parsed = crate::parse_dynamic_soql(source).map_err(|error| {
            runtime_exception(
                "QueryException",
                format!("invalid dynamic SOQL: {}", error.message),
                span,
            )
        })?;
        let bindings = self.visible_query_binding_types();
        let expected_type = self.dynamic_query_expected_type(expected_object_id, span);
        let checked = crate::semantic::check_dynamic_soql(
            &parsed,
            self.program().schema(),
            expected_type.as_ref(),
            bindings,
        )
        .map_err(|error| {
            runtime_exception(
                "QueryException",
                format!("invalid dynamic SOQL: {}", error.message),
                span,
            )
        })?;
        if !dynamic_query_result_is_valid(kind, checked.result) {
            return Err(runtime_exception(
                "QueryException",
                match kind {
                    DatabaseQueryKind::Query => {
                        "Database.query requires a record-returning SOQL query"
                    }
                    DatabaseQueryKind::Count => "Database.countQuery requires scalar COUNT() SOQL",
                    DatabaseQueryKind::QueryLocator => {
                        "Database.getQueryLocator requires a record-returning SOQL query"
                    }
                },
                span,
            ));
        }
        Ok(checked)
    }

    fn visible_query_binding_types(&self) -> std::collections::HashMap<String, TypeName> {
        let mut bindings = std::collections::HashMap::new();
        for scope in &self.scopes {
            for (name, slot) in scope {
                bindings.insert(name.clone(), slot.ty.clone());
            }
        }
        bindings
    }

    fn dynamic_query_expected_type(
        &self,
        expected_object_id: Option<usize>,
        span: Span,
    ) -> Option<TypeName> {
        expected_object_id.map(|object_id| {
            let object = self
                .program()
                .schema()
                .object_at(object_id)
                .expect("checked dynamic query object is valid");
            TypeName::List(Box::new(TypeName::Custom(crate::ast::NamedType::new(
                object.api_name().to_owned(),
                span,
            ))))
        })
    }

    fn finish_dynamic_query_value(
        &mut self,
        kind: DatabaseQueryKind,
        expected_object_id: Option<usize>,
        value: Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match kind {
            DatabaseQueryKind::Count => Ok(value),
            DatabaseQueryKind::Query => {
                self.retag_dynamic_record_list(expected_object_id, &value, span);
                Ok(value)
            }
            DatabaseQueryKind::QueryLocator => {
                let Value::Collection(collection) = value else {
                    unreachable!("record query allocates a List")
                };
                Ok(self
                    .store
                    .allocate_platform(PlatformValue::QueryLocator(collection)))
            }
        }
    }

    fn retag_dynamic_record_list(
        &mut self,
        expected_object_id: Option<usize>,
        value: &Value,
        span: Span,
    ) {
        let (Some(object_id), Value::Collection(collection)) = (expected_object_id, value) else {
            return;
        };
        let object = self
            .program()
            .schema()
            .object_at(object_id)
            .expect("checked expected object is valid");
        let ty = TypeName::Custom(crate::ast::NamedType::new(
            object.api_name().to_owned(),
            span,
        ));
        let Collection::List { element_type, .. } = self.store.collection_mut(*collection) else {
            unreachable!("record query allocates a List")
        };
        *element_type = ty;
    }

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
        if query.result == QueryResultKind::RecordSingle
            && self.program().query_allows_empty_single_result(span)
        {
            let object = self
                .program()
                .schema()
                .object_at(query.object_id)
                .expect("checked query object is valid");
            let ty = TypeName::Custom(crate::ast::NamedType::new(
                object.api_name().to_owned(),
                span,
            ));
            self.null_aware_query_outcome_value(outcome, query.result, ty, span)
        } else {
            self.query_outcome_value(outcome, query.result, span)
        }
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
        if self.trigger_depth >= 16 {
            return Err(runtime_exception(
                "DmlException",
                "maximum recursive trigger depth of 16 exceeded",
                span,
            ));
        }
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
        let original_instances = handles
            .iter()
            .map(|id| (*id, self.store.sobject(*id).clone()))
            .collect::<Vec<_>>();
        self.begin_transaction(span)?;
        let result = self.execute_dml_transaction(operation, &handles, records, &schema, span);
        if result.is_err() {
            for (id, instance) in original_instances {
                *self.store.sobject_mut(id) = instance;
            }
        }
        self.finish_transaction(result, span)
    }

    fn execute_dml_transaction(
        &mut self,
        requested_operation: DmlOperation,
        handles: &[SObjectId],
        records: Vec<SObject>,
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let prepared = self
            .host
            .prepare_dml(schema, requested_operation, &records)
            .map_err(|error| runtime_exception("DmlException", error.to_string(), span))?;
        if prepared.len() != handles.len() {
            return Err(Diagnostic::new(
                "platform DML preflight returned an invalid record count",
                span,
            ));
        }
        let first_object = prepared.first().and_then(|record| {
            record
                .new
                .as_ref()
                .or(record.old.as_ref())
                .map(|value| value.object_api_name())
        });
        if prepared.iter().any(|record| {
            record
                .new
                .as_ref()
                .or(record.old.as_ref())
                .is_some_and(|value| {
                    first_object
                        .is_some_and(|first| !value.object_api_name().eq_ignore_ascii_case(first))
                })
        }) {
            return Err(runtime_exception(
                "DmlException",
                "mixed-SObject bulk DML is not supported",
                span,
            ));
        }

        for operation in [
            DmlOperation::Insert,
            DmlOperation::Update,
            DmlOperation::Delete,
            DmlOperation::Undelete,
        ] {
            let indices = prepared
                .iter()
                .enumerate()
                .filter_map(|(index, record)| (record.operation == operation).then_some(index))
                .collect::<Vec<_>>();
            if indices.is_empty() {
                continue;
            }
            let group_handles = indices
                .iter()
                .map(|index| handles[*index])
                .collect::<Vec<_>>();
            let mut old_handles = Vec::new();
            for index in &indices {
                if let Some(value) = &prepared[*index].new {
                    self.update_runtime_sobject(handles[*index], value, schema, span)?;
                }
                if let Some(value) = &prepared[*index].old {
                    old_handles.push(self.allocate_platform_sobject(value, schema, span)?);
                }
            }
            let object = prepared[indices[0]]
                .new
                .as_ref()
                .or(prepared[indices[0]].old.as_ref())
                .expect("prepared DML record has an old or new value")
                .object_api_name()
                .to_owned();

            self.execute_trigger_phase(
                operation,
                TriggerPhase::Before,
                &object,
                &group_handles,
                &old_handles,
                span,
            )?;

            let group_records = group_handles
                .iter()
                .map(|id| self.platform_sobject(*id, schema, span))
                .collect::<Result<Vec<_>, _>>()?;
            let persisted = self
                .host
                .dml(schema, operation, group_records)
                .map_err(|error| runtime_exception("DmlException", error.to_string(), span))?;
            if persisted.len() != group_handles.len() {
                return Err(Diagnostic::new(
                    "platform DML returned an invalid record count",
                    span,
                ));
            }
            for (id, value) in group_handles.iter().copied().zip(&persisted) {
                self.update_runtime_sobject(id, value, schema, span)?;
            }

            self.execute_trigger_phase(
                operation,
                TriggerPhase::After,
                &object,
                &group_handles,
                &old_handles,
                span,
            )?;
        }
        Ok(())
    }

    pub(super) fn execute_trigger_phase(
        &mut self,
        operation: DmlOperation,
        phase: TriggerPhase,
        object: &str,
        new_handles: &[SObjectId],
        old_handles: &[SObjectId],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let event = ast_trigger_event(operation, phase);
        let triggers = self
            .program()
            .triggers
            .iter()
            .filter(|trigger| {
                trigger.object.spelling.eq_ignore_ascii_case(object)
                    && trigger.events.contains(&event)
            })
            .cloned()
            .collect::<Vec<_>>();
        for trigger in triggers {
            let saved_context = self.trigger_context.take();
            let saved_read_only = self.read_only_sobjects.clone();
            let context = self.build_trigger_context(
                event,
                object,
                new_handles,
                old_handles,
                trigger.name.span,
            )?;
            self.read_only_sobjects.extend(old_handles.iter().copied());
            if phase == TriggerPhase::After {
                self.read_only_sobjects.extend(new_handles.iter().copied());
            }
            self.trigger_context = Some(context);
            self.trigger_depth += 1;
            let depth = self.trigger_depth;
            self.host.trigger(RuntimeTriggerEvent {
                trigger: trigger.name.spelling.clone(),
                object: object.to_owned(),
                operation,
                phase,
                stage: TriggerStage::Enter,
                depth,
                records: new_handles.len().max(old_handles.len()),
                succeeded: None,
            });

            let caller_scopes =
                std::mem::replace(&mut self.scopes, vec![std::collections::HashMap::new()]);
            let saved_receiver = self.current_receiver.take();
            let saved_declaring = self.current_declaring_class.take();
            self.call_stack.push(ActiveCall {
                method: trigger.name.spelling.clone(),
                call_span: span,
            });
            let result = match self.execute_statement(&trigger.body) {
                Ok(Flow::Normal | Flow::Return(None)) => Ok(()),
                Ok(Flow::Return(Some(_)) | Flow::Break | Flow::Continue) => Err(Diagnostic::new(
                    "invalid control flow escaped trigger validation",
                    trigger.span,
                )),
                Err(mut error) => {
                    self.attach_stack_if_missing(&mut error);
                    Err(error)
                }
            };
            self.call_stack.pop();
            self.scopes = caller_scopes;
            self.current_receiver = saved_receiver;
            self.current_declaring_class = saved_declaring;

            self.host.trigger(RuntimeTriggerEvent {
                trigger: trigger.name.spelling.clone(),
                object: object.to_owned(),
                operation,
                phase,
                stage: TriggerStage::Exit,
                depth,
                records: new_handles.len().max(old_handles.len()),
                succeeded: Some(result.is_ok()),
            });
            self.trigger_depth -= 1;
            self.trigger_context = saved_context;
            self.read_only_sobjects = saved_read_only;
            result?;
        }
        Ok(())
    }

    fn build_trigger_context(
        &mut self,
        event: AstTriggerEvent,
        object: &str,
        new_handles: &[SObjectId],
        old_handles: &[SObjectId],
        span: Span,
    ) -> Result<TriggerContext, Diagnostic> {
        let object_type = TypeName::Custom(crate::ast::NamedType::new(object.to_owned(), span));
        let list_type = TypeName::List(Box::new(object_type.clone()));
        let map_type = TypeName::Map(Box::new(TypeName::String), Box::new(object_type.clone()));
        let operation = event.operation();
        let new_available = operation != AstDmlOperation::Delete;
        let old_available = matches!(operation, AstDmlOperation::Update | AstDmlOperation::Delete);
        let new_map_available = matches!(
            operation,
            AstDmlOperation::Update | AstDmlOperation::Undelete
        ) || (operation == AstDmlOperation::Insert && !event.is_before());
        let old_map_available =
            matches!(operation, AstDmlOperation::Update | AstDmlOperation::Delete);
        let new_list = if new_available {
            self.trigger_list(object_type.clone(), new_handles)
        } else {
            Value::Null(Some(list_type.clone()))
        };
        let old_list = if old_available {
            self.trigger_list(object_type.clone(), old_handles)
        } else {
            Value::Null(Some(list_type))
        };
        let new_map = if new_map_available {
            self.trigger_map(object_type.clone(), new_handles)
        } else {
            Value::Null(Some(map_type.clone()))
        };
        let old_map = if old_map_available {
            self.trigger_map(object_type, old_handles)
        } else {
            Value::Null(Some(map_type))
        };
        Ok(TriggerContext {
            event,
            new_list,
            old_list,
            new_map,
            old_map,
            size: new_handles.len().max(old_handles.len()),
        })
    }

    fn trigger_list(&mut self, element_type: TypeName, handles: &[SObjectId]) -> Value {
        let value = self.store.allocate_collection(Collection::List {
            element_type,
            elements: handles.iter().copied().map(Value::SObject).collect(),
            iteration_depth: 0,
        });
        let Value::Collection(id) = value else {
            unreachable!("collection allocation returns a collection")
        };
        self.read_only_collections.insert(id);
        Value::Collection(id)
    }

    fn trigger_map(&mut self, value_type: TypeName, handles: &[SObjectId]) -> Value {
        let entries = handles
            .iter()
            .filter_map(|id| {
                let instance = self.store.sobject(*id);
                let object = self
                    .program()
                    .schema()
                    .object_at(instance.object_id)
                    .expect("trigger SObject type is valid");
                let id_field = object.field_index("Id")?;
                let Value::String(value) = instance.fields.get(&id_field)? else {
                    return None;
                };
                Some((Value::String(value.clone()), Value::SObject(*id)))
            })
            .collect();
        let value = self.store.allocate_collection(Collection::Map {
            key_type: TypeName::String,
            value_type,
            entries,
        });
        let Value::Collection(id) = value else {
            unreachable!("collection allocation returns a collection")
        };
        self.read_only_collections.insert(id);
        Value::Collection(id)
    }

    fn allocate_platform_sobject(
        &mut self,
        value: &SObject,
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<SObjectId, Diagnostic> {
        let object_id = schema
            .object_index(value.object_api_name())
            .ok_or_else(|| runtime_exception("DmlException", "unknown SObject type", span))?;
        let Value::SObject(id) = self.store.allocate_sobject(object_id) else {
            unreachable!("SObject allocation returns an SObject")
        };
        self.update_runtime_sobject(id, value, schema, span)?;
        Ok(id)
    }

    pub(super) fn evaluate_aggregate_result_get(
        &mut self,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let receiver = match evaluated_receiver {
            Some(receiver) => receiver,
            None => self.evaluate(receiver)?,
        };
        let Value::AggregateResult(id) = receiver else {
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
        let now_millis = self.host.now_millis();
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
                .map(|item| self.query_select(item, span))
                .collect::<Result<Vec<_>, _>>()?,
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
            having: query
                .having
                .as_ref()
                .map(|condition| self.query_condition(condition, span))
                .transpose()?,
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
            now_millis,
        })
    }

    fn sosl_request(
        &mut self,
        query: &CheckedSoslQuery,
        span: Span,
    ) -> Result<SoslRequest, Diagnostic> {
        let search = match self.query_value(&query.search, span)? {
            QueryValue::Data(DataValue::String(value)) => value,
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

    fn query_select(
        &mut self,
        item: &CheckedSelectItem,
        span: Span,
    ) -> Result<QuerySelect, Diagnostic> {
        Ok(match item {
            CheckedSelectItem::Field(field) => QuerySelect::Field(self.query_field(field)),
            CheckedSelectItem::Subquery {
                relationship,
                reference_field_id,
                query,
            } => {
                let child = self
                    .program()
                    .schema()
                    .object_at(query.object_id)
                    .expect("checked child query object is valid");
                let reference_field = child
                    .field_at(*reference_field_id)
                    .expect("checked child reference field is valid")
                    .api_name()
                    .to_owned();
                QuerySelect::Subquery {
                    relationship: relationship.clone(),
                    reference_field,
                    query: Box::new(self.soql_request(query, span)?),
                }
            }
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
        })
    }

    fn query_field(&self, field: &CheckedFieldPath) -> QueryField {
        let schema = self.program().schema();
        let object_id = field
            .relationships
            .last()
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
        let mut current_object_id = field.root_object_id;
        let relationships = field
            .relationships
            .iter()
            .map(|relationship| {
                let current = schema
                    .object_at(current_object_id)
                    .expect("checked query relationship source is valid");
                current_object_id = relationship.target_object_id;
                QueryRelationship {
                    reference_field: current
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
            })
            .collect();
        QueryField {
            relationships,
            field: field_name,
        }
    }

    fn query_condition(
        &mut self,
        condition: &CheckedCondition,
        span: Span,
    ) -> Result<QueryCondition, Diagnostic> {
        Ok(match condition {
            CheckedCondition::AggregateComparison {
                alias,
                operator,
                right,
            } => QueryCondition::Comparison {
                left: QueryField {
                    relationships: Vec::new(),
                    field: alias.clone(),
                },
                operator: query_comparison(*operator),
                right: self.query_value(right, span)?,
            },
            CheckedCondition::Comparison {
                left,
                operator,
                right,
            } => QueryCondition::Comparison {
                left: self.query_field(left),
                operator: query_comparison(*operator),
                right: self.query_value(right, span)?,
            },
            CheckedCondition::In {
                field,
                negated,
                values,
            } => QueryCondition::In {
                field: self.query_field(field),
                negated: *negated,
                values: QueryInValues::Values(self.query_in_values(values, span)?),
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

    fn query_in_values(
        &mut self,
        values: &CheckedInValues,
        span: Span,
    ) -> Result<Vec<QueryValue>, Diagnostic> {
        match values {
            CheckedInValues::Values(values) => values
                .iter()
                .map(|value| self.query_value(value, span))
                .collect(),
            CheckedInValues::Bind(expression) => {
                let value = self.evaluate(expression)?;
                self.query_collection_values(value, "SOQL", span)
            }
            CheckedInValues::DynamicBind(name) => {
                let value = self
                    .lookup_canonical(name)
                    .ok_or_else(|| {
                        runtime_exception(
                            "QueryException",
                            format!("dynamic SOQL bind `{name}` is unavailable"),
                            span,
                        )
                    })?
                    .value
                    .clone();
                self.query_collection_values(value, "dynamic SOQL", span)
            }
        }
    }

    fn query_collection_values(
        &self,
        value: Value,
        subject: &str,
        span: Span,
    ) -> Result<Vec<QueryValue>, Diagnostic> {
        let Value::Collection(id) = value else {
            return Err(runtime_exception(
                "QueryException",
                format!("{subject} `IN` bind must evaluate to a collection"),
                span,
            ));
        };
        let elements = match self.store.collection(id) {
            Collection::List { elements, .. } | Collection::Set { elements, .. } => elements,
            Collection::Map { .. } => unreachable!("checker rejected Map bind"),
        };
        elements
            .iter()
            .map(|value| self.value_to_data(value, span).map(QueryValue::Data))
            .collect()
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

    fn query_value(&mut self, value: &CheckedValue, span: Span) -> Result<QueryValue, Diagnostic> {
        match value {
            CheckedValue::Literal(value) => Ok(QueryValue::Data(value.clone())),
            CheckedValue::DateLiteral(literal) => Ok(QueryValue::DateLiteral(QueryDateLiteral {
                kind: match literal.kind {
                    crate::ast::SoqlDateLiteralKind::Yesterday => QueryDateLiteralKind::Yesterday,
                    crate::ast::SoqlDateLiteralKind::Today => QueryDateLiteralKind::Today,
                    crate::ast::SoqlDateLiteralKind::Tomorrow => QueryDateLiteralKind::Tomorrow,
                    crate::ast::SoqlDateLiteralKind::LastNDays => QueryDateLiteralKind::LastNDays,
                    crate::ast::SoqlDateLiteralKind::NextNDays => QueryDateLiteralKind::NextNDays,
                    crate::ast::SoqlDateLiteralKind::ThisWeek => QueryDateLiteralKind::ThisWeek,
                    crate::ast::SoqlDateLiteralKind::LastWeek => QueryDateLiteralKind::LastWeek,
                    crate::ast::SoqlDateLiteralKind::NextWeek => QueryDateLiteralKind::NextWeek,
                    crate::ast::SoqlDateLiteralKind::ThisMonth => QueryDateLiteralKind::ThisMonth,
                    crate::ast::SoqlDateLiteralKind::LastMonth => QueryDateLiteralKind::LastMonth,
                    crate::ast::SoqlDateLiteralKind::NextMonth => QueryDateLiteralKind::NextMonth,
                    crate::ast::SoqlDateLiteralKind::ThisYear => QueryDateLiteralKind::ThisYear,
                    crate::ast::SoqlDateLiteralKind::LastYear => QueryDateLiteralKind::LastYear,
                    crate::ast::SoqlDateLiteralKind::NextYear => QueryDateLiteralKind::NextYear,
                },
                amount: literal.amount,
            })),
            CheckedValue::Bind(expression) => {
                let value = self.evaluate(expression)?;
                self.value_to_data(&value, span).map(QueryValue::Data)
            }
            CheckedValue::DynamicBind(name) => {
                let value = self
                    .lookup_canonical(name)
                    .ok_or_else(|| {
                        runtime_exception(
                            "QueryException",
                            format!("dynamic SOQL bind `{name}` is unavailable"),
                            span,
                        )
                    })?
                    .value
                    .clone();
                self.value_to_data(&value, span).map(QueryValue::Data)
            }
        }
    }

    fn query_usize(&mut self, value: &CheckedValue, span: Span) -> Result<usize, Diagnostic> {
        match self.query_value(value, span)? {
            QueryValue::Data(DataValue::Integer(value)) if value >= 0 => usize::try_from(value)
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
            Value::Date(value) => Ok(DataValue::Date(date_to_epoch_days(*value, span)?)),
            Value::Datetime(value) => Ok(DataValue::Datetime(value.timestamp_millis())),
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

    fn null_aware_query_outcome_value(
        &mut self,
        outcome: QueryOutcome,
        result: QueryResultKind,
        empty_single_type: TypeName,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if matches!(&outcome, QueryOutcome::Records(records) if records.is_empty()) {
            Ok(Value::Null(Some(empty_single_type)))
        } else {
            self.query_outcome_value(outcome, result, span)
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
            let related = self.allocate_query_record(record, span)?;
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
        let mut children = Vec::new();
        for (relationship, records) in row.children {
            let child_object_id = schema
                .child_relationship(object_id, &relationship)
                .map(|(child_object_id, _)| child_object_id)
                .ok_or_else(|| {
                    runtime_exception(
                        "QueryException",
                        format!(
                            "query result contains unknown child relationship `{relationship}`"
                        ),
                        span,
                    )
                })?;
            let child_object = schema
                .object_at(child_object_id)
                .expect("query child object index is valid");
            let elements = records
                .into_iter()
                .map(|record| self.allocate_query_record(record, span))
                .collect::<Result<Vec<_>, _>>()?;
            let value = self.store.allocate_collection(Collection::List {
                element_type: TypeName::Custom(crate::ast::NamedType::new(
                    child_object.api_name().to_owned(),
                    span,
                )),
                elements,
                iteration_depth: 0,
            });
            let Value::Collection(collection) = value else {
                unreachable!("child query allocation returns a List")
            };
            children.push((relationship, collection));
        }
        self.store.sobject_mut(id).children.extend(children);
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
            Value::Date(value) => Ok(DataValue::Date(date_to_epoch_days(*value, span)?)),
            Value::Datetime(value) => Ok(DataValue::Datetime(value.timestamp_millis())),
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

fn ast_trigger_event(operation: DmlOperation, phase: TriggerPhase) -> AstTriggerEvent {
    match (phase, operation) {
        (TriggerPhase::Before, DmlOperation::Insert) => AstTriggerEvent::BeforeInsert,
        (TriggerPhase::Before, DmlOperation::Update) => AstTriggerEvent::BeforeUpdate,
        (TriggerPhase::Before, DmlOperation::Delete) => AstTriggerEvent::BeforeDelete,
        (TriggerPhase::Before, DmlOperation::Undelete) => AstTriggerEvent::BeforeUndelete,
        (TriggerPhase::After, DmlOperation::Insert) => AstTriggerEvent::AfterInsert,
        (TriggerPhase::After, DmlOperation::Update) => AstTriggerEvent::AfterUpdate,
        (TriggerPhase::After, DmlOperation::Delete) => AstTriggerEvent::AfterDelete,
        (TriggerPhase::After, DmlOperation::Undelete) => AstTriggerEvent::AfterUndelete,
        (_, DmlOperation::Upsert) => unreachable!("upsert is split during DML preflight"),
    }
}

pub(super) fn data_to_value(value: &DataValue) -> Value {
    match value {
        DataValue::Null => Value::Null(None),
        DataValue::Boolean(value) => Value::Boolean(*value),
        DataValue::Integer(value) => Value::Integer(*value),
        DataValue::String(value) => Value::String(value.clone()),
        DataValue::Date(value) => Value::Date(
            NaiveDate::from_ymd_opt(1970, 1, 1).expect("Unix epoch date is valid")
                + Duration::days(i64::from(*value)),
        ),
        DataValue::Datetime(value) => Value::Datetime(
            Utc.timestamp_millis_opt(*value)
                .single()
                .expect("stored Datetime milliseconds are representable"),
        ),
        DataValue::Id(value) => Value::String(value.to_string()),
    }
}

fn date_to_epoch_days(value: NaiveDate, span: Span) -> Result<i32, Diagnostic> {
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("Unix epoch date is valid");
    i32::try_from(value.signed_duration_since(epoch).num_days()).map_err(|_| {
        runtime_exception(
            "QueryException",
            "Date value is outside the supported storage range",
            span,
        )
    })
}

fn dynamic_query_result_is_valid(kind: DatabaseQueryKind, result: QueryResultKind) -> bool {
    match kind {
        DatabaseQueryKind::Query | DatabaseQueryKind::QueryLocator => {
            result == QueryResultKind::Records
        }
        DatabaseQueryKind::Count => result == QueryResultKind::Count,
    }
}

fn query_comparison(operator: crate::ast::SoqlComparisonOperator) -> QueryComparison {
    match operator {
        crate::ast::SoqlComparisonOperator::Equal => QueryComparison::Equal,
        crate::ast::SoqlComparisonOperator::NotEqual => QueryComparison::NotEqual,
        crate::ast::SoqlComparisonOperator::Less => QueryComparison::Less,
        crate::ast::SoqlComparisonOperator::LessEqual => QueryComparison::LessEqual,
        crate::ast::SoqlComparisonOperator::Greater => QueryComparison::Greater,
        crate::ast::SoqlComparisonOperator::GreaterEqual => QueryComparison::GreaterEqual,
        crate::ast::SoqlComparisonOperator::Like => QueryComparison::Like,
    }
}
