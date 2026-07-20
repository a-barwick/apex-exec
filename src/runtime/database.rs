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
        CheckedSelectItem, CheckedSoqlQuery, CheckedSoslQuery, CheckedValue, DatabaseDmlTarget,
        DatabaseQueryKind, DmlErrorMethod, DmlResultMethod, QueryResultKind,
    },
    platform::{
        AggregateFunction, DataValue, DmlError, DmlExternalId, DmlOperation, DmlRequest, DmlRow,
        DmlRowOutcome, DmlStatus, NullOrder, PreparedDmlOutcome, PreparedDmlRecord,
        QueryComparison, QueryCondition, QueryDateLiteral, QueryDateLiteralKind, QueryField,
        QueryInValues, QueryLogical, QueryOrder, QueryOutcome, QueryRecord, QueryRelationship,
        QuerySelect, QueryValue, SObject, SoqlRequest, SortOrder, SoslRequest,
        SoslReturningRequest,
    },
    span::Span,
};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use std::collections::BTreeMap;

const MAX_PARTIAL_DML_ATTEMPTS: usize = 3;

struct CollectedDmlRows {
    input_count: usize,
    handles: BTreeMap<usize, SObjectId>,
    rows: Vec<DmlRow>,
    outcomes: Vec<DmlRowOutcome>,
}

struct PreparedTriggerImages {
    object: String,
    old_by_index: BTreeMap<usize, SObjectId>,
    old_handles: Vec<SObjectId>,
}

impl<'program, H: PlatformHost> Interpreter<'program, H> {
    pub(super) fn evaluate_database_query_call(
        &mut self,
        kind: DatabaseQueryKind,
        expected_object_id: Option<usize>,
        access_level_argument: Option<usize>,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let argument = self.evaluate(&arguments[0])?;
        let access = access_level_argument
            .map(|index| self.evaluate(&arguments[index]))
            .transpose()?
            .map(|value| self.runtime_access_level(value, arguments[1].span()))
            .transpose()?;
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
        let mut checked = self.check_dynamic_query(&source, expected_object_id, kind, span)?;
        if let Some(access) = access {
            checked.access = match access {
                crate::platform::AccessLevel::UserMode => {
                    crate::platform::QueryAccessMode::UserMode
                }
                crate::platform::AccessLevel::SystemMode => {
                    crate::platform::QueryAccessMode::SystemMode
                }
            };
        }
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
        target: DatabaseDmlTarget,
        expression: &Expression,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let value = self.evaluate(expression)?;
        self.execute_dml_value(
            target,
            value,
            true,
            target
                .statement_access
                .unwrap_or(crate::platform::AccessLevel::SystemMode),
            span,
        )
        .map(|_| ())
    }

    fn collect_dml_rows(
        &self,
        value: Value,
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<CollectedDmlRows, Diagnostic> {
        let values = match value {
            Value::SObject(id) => vec![Value::SObject(id)],
            Value::Collection(id) => match self.store.collection(id) {
                Collection::List { elements, .. } => elements.clone(),
                _ => {
                    return Err(runtime_exception(
                        "DmlException",
                        "DML requires an SObject or List<SObject>",
                        span,
                    ));
                }
            },
            value => vec![value],
        };
        let mut handles = BTreeMap::new();
        let mut rows = Vec::new();
        let mut outcomes = Vec::new();
        for (input_index, value) in values.iter().enumerate() {
            if let Value::SObject(handle) = value {
                handles.insert(input_index, *handle);
                rows.push(DmlRow {
                    input_index,
                    record: self.platform_sobject(*handle, schema, span)?,
                });
            } else {
                outcomes.push(DmlRowOutcome::failure(
                    input_index,
                    vec![DmlError::new(
                        DmlStatus::MissingArgument,
                        "DML row is null or is not an SObject",
                        [],
                    )],
                ));
            }
        }
        Ok(CollectedDmlRows {
            input_count: values.len(),
            handles,
            rows,
            outcomes,
        })
    }

    fn dml_external_id(
        &self,
        target: DatabaseDmlTarget,
        schema: &crate::platform::SchemaCatalog,
    ) -> Option<DmlExternalId> {
        target.external_id.map(|(object_id, field_id)| {
            let object = schema
                .object_at(object_id.index())
                .expect("checked external-ID object is valid");
            let field = object
                .field_at(field_id.index())
                .expect("checked external-ID field is valid");
            DmlExternalId {
                object: object.api_name().to_owned(),
                field: field.api_name().to_owned(),
            }
        })
    }

    pub(super) fn execute_dml_value(
        &mut self,
        target: DatabaseDmlTarget,
        value: Value,
        all_or_none: bool,
        access: crate::platform::AccessLevel,
        span: Span,
    ) -> Result<Vec<DmlRowOutcome>, Diagnostic> {
        if self.trigger_depth >= 16 {
            return Err(runtime_exception(
                "DmlException",
                "maximum recursive trigger depth of 16 exceeded",
                span,
            ));
        }
        let schema = self.program().schema().clone();
        let mut collected = self.collect_dml_rows(value, &schema, span)?;
        if all_or_none && !collected.outcomes.is_empty() {
            return Err(dml_outcome_exception(&collected.outcomes[0], span));
        }
        let operation = map_dml_operation(target.operation);
        let original_instances = collected
            .handles
            .values()
            .map(|id| (*id, self.store.sobject(*id).clone()))
            .collect::<BTreeMap<_, _>>();
        let request = DmlRequest {
            operation,
            all_or_none,
            access,
            sharing: if (target.access_level_argument.is_some()
                || target.statement_access.is_some())
                && access == crate::platform::AccessLevel::SystemMode
            {
                crate::platform::SharingMode::WithoutSharing
            } else {
                self.execution_context.sharing_mode()
            },
            user_id: self.current_user_context().user_id,
            external_id: self.dml_external_id(target, &schema),
            rows: std::mem::take(&mut collected.rows),
        };
        let objects = request
            .rows
            .iter()
            .map(|row| row.record.object_api_name().to_owned())
            .collect::<Vec<_>>();
        let record_count = collected.input_count;
        self.begin_transaction(span)?;
        let result = self.execute_dml_transaction(
            &request,
            &collected.handles,
            collected.outcomes,
            &schema,
            span,
        );
        if result.is_err() {
            for (id, instance) in original_instances {
                *self.store.sobject_mut(id) = instance;
            }
        }
        let result = self.finish_transaction(result, span);
        let (successful_records, failed_records, succeeded) = match &result {
            Ok(outcomes) => {
                let successful = outcomes
                    .iter()
                    .filter(|outcome| outcome.is_success())
                    .count();
                (
                    successful,
                    outcomes.len() - successful,
                    successful == outcomes.len(),
                )
            }
            Err(_) => (0, record_count, false),
        };
        self.host.record_dml(super::DmlEvent {
            operation,
            objects,
            records: record_count,
            successful_records,
            failed_records,
            succeeded,
        });
        result
    }

    pub(super) fn dml_outcomes_value(
        &mut self,
        outcomes: Vec<DmlRowOutcome>,
        result_type: &TypeName,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match result_type {
            TypeName::List(element) => {
                let result_type = element.as_ref();
                let elements = outcomes
                    .into_iter()
                    .map(|outcome| {
                        self.store.allocate_platform(PlatformValue::DmlResult {
                            ty: result_type.clone(),
                            outcome,
                        })
                    })
                    .collect();
                Ok(self.store.allocate_collection(Collection::List {
                    element_type: result_type.clone(),
                    elements,
                    iteration_depth: 0,
                }))
            }
            TypeName::SaveResult
            | TypeName::UpsertResult
            | TypeName::DeleteResult
            | TypeName::UndeleteResult => {
                let [outcome] = <[DmlRowOutcome; 1]>::try_from(outcomes).map_err(|_| {
                    Diagnostic::new("scalar Database DML returned multiple row outcomes", span)
                })?;
                Ok(self.store.allocate_platform(PlatformValue::DmlResult {
                    ty: result_type.clone(),
                    outcome,
                }))
            }
            _ => Err(Diagnostic::new(
                "Database DML call has an invalid checked result type",
                span,
            )),
        }
    }

    fn execute_dml_transaction(
        &mut self,
        request: &DmlRequest,
        handles: &BTreeMap<usize, SObjectId>,
        mut outcomes: Vec<DmlRowOutcome>,
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<Vec<DmlRowOutcome>, Diagnostic> {
        let expected_outcome_count = handles.len() + outcomes.len();
        let prepared = self
            .host
            .prepare_dml(schema, request)
            .map_err(|error| runtime_exception("DmlException", error.to_string(), span))?;
        if prepared.len() != request.rows.len() {
            return Err(Diagnostic::new(
                "platform DML preflight returned an invalid record count",
                span,
            ));
        }
        let mut ready = Vec::new();
        for outcome in prepared {
            match outcome {
                PreparedDmlOutcome::Ready(record) => ready.push(record),
                PreparedDmlOutcome::Failed {
                    input_index,
                    errors,
                } => outcomes.push(DmlRowOutcome::failure(input_index, errors)),
            }
        }
        if request.all_or_none
            && let Some(failed) = outcomes.iter().find(|outcome| !outcome.is_success())
        {
            return Err(dml_outcome_exception(failed, span));
        }
        ensure_one_dml_object(&ready, span)?;
        for operation in [
            DmlOperation::Insert,
            DmlOperation::Update,
            DmlOperation::Delete,
            DmlOperation::Undelete,
        ] {
            let group = ready
                .iter()
                .filter(|record| record.operation == operation)
                .cloned()
                .collect::<Vec<_>>();
            if group.is_empty() {
                continue;
            }
            outcomes.extend(self.execute_dml_group(
                operation,
                &group,
                handles,
                schema,
                request.all_or_none,
                span,
            )?);
        }
        outcomes.sort_by_key(|outcome| outcome.input_index);
        if outcomes.len() != expected_outcome_count {
            return Err(Diagnostic::new(
                "platform DML returned an incomplete outcome set",
                span,
            ));
        }
        Ok(outcomes)
    }

    fn execute_dml_group(
        &mut self,
        operation: DmlOperation,
        group: &[PreparedDmlRecord],
        handles: &BTreeMap<usize, SObjectId>,
        schema: &crate::platform::SchemaCatalog,
        all_or_none: bool,
        span: Span,
    ) -> Result<Vec<DmlRowOutcome>, Diagnostic> {
        if !all_or_none {
            return self.execute_partial_dml_group(operation, group, handles, schema, span);
        }
        let group_handles = group
            .iter()
            .map(|record| handles[&record.input_index])
            .collect::<Vec<_>>();
        let originals = group_handles
            .iter()
            .map(|handle| (*handle, self.store.sobject(*handle).clone()))
            .collect::<Vec<_>>();
        self.begin_transaction(span)?;
        let result = self.execute_dml_group_inner(operation, group, &group_handles, schema, span);
        if result.is_err() {
            for (handle, original) in &originals {
                *self.store.sobject_mut(*handle) = original.clone();
            }
        }
        match self.finish_transaction(result, span) {
            Ok(outcomes) => {
                if let Some(failed) = outcomes.iter().find(|outcome| !outcome.is_success()) {
                    Err(dml_outcome_exception(failed, span))
                } else {
                    for (record, (handle, original)) in group.iter().zip(&originals) {
                        if outcomes.iter().any(|outcome| {
                            outcome.input_index == record.input_index && !outcome.is_success()
                        }) {
                            *self.store.sobject_mut(*handle) = original.clone();
                        }
                    }
                    Ok(outcomes)
                }
            }
            Err(error) => Err(error),
        }
    }

    fn execute_partial_dml_group(
        &mut self,
        operation: DmlOperation,
        group: &[PreparedDmlRecord],
        handles: &BTreeMap<usize, SObjectId>,
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<Vec<DmlRowOutcome>, Diagnostic> {
        self.host.begin_dml_retry_scope();
        let result = self.execute_partial_dml_attempt(operation, group, handles, schema, 1, span);
        self.host.end_dml_retry_scope();
        result
    }

    fn execute_partial_dml_attempt(
        &mut self,
        operation: DmlOperation,
        group: &[PreparedDmlRecord],
        handles: &BTreeMap<usize, SObjectId>,
        schema: &crate::platform::SchemaCatalog,
        attempt: usize,
        span: Span,
    ) -> Result<Vec<DmlRowOutcome>, Diagnostic> {
        let group_handles = group
            .iter()
            .map(|record| handles[&record.input_index])
            .collect::<Vec<_>>();
        let originals = self.capture_sobject_images(&group_handles);
        self.begin_transaction(span)?;
        let first = self.execute_dml_group_inner(operation, group, &group_handles, schema, span);
        let outcomes = match first {
            Ok(outcomes) if outcomes.iter().all(DmlRowOutcome::is_success) => {
                return self.finish_transaction(Ok(outcomes), span);
            }
            Ok(outcomes) => outcomes,
            Err(error) => {
                self.restore_sobject_images(&originals);
                self.rollback_transaction(span)?;
                if attempt == MAX_PARTIAL_DML_ATTEMPTS {
                    return Err(partial_dml_retry_exception(span));
                }
                return Ok(group_failure_outcomes(group, &error));
            }
        };
        self.restore_sobject_images(&originals);
        self.rollback_transaction(span)?;
        if attempt == MAX_PARTIAL_DML_ATTEMPTS {
            return Err(partial_dml_retry_exception(span));
        }
        let mut final_outcomes = outcomes
            .iter()
            .filter(|outcome| !outcome.is_success())
            .cloned()
            .collect::<Vec<_>>();
        let retry_group = group
            .iter()
            .filter(|record| {
                outcomes.iter().any(|outcome| {
                    outcome.input_index == record.input_index && outcome.is_success()
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        if !retry_group.is_empty() {
            self.host.reset_dml_retry_limits();
            final_outcomes.extend(self.execute_partial_dml_attempt(
                operation,
                &retry_group,
                handles,
                schema,
                attempt + 1,
                span,
            )?);
        }
        final_outcomes.sort_by_key(|outcome| outcome.input_index);
        Ok(final_outcomes)
    }

    fn capture_sobject_images(
        &self,
        handles: &[SObjectId],
    ) -> Vec<(SObjectId, super::SObjectInstance)> {
        handles
            .iter()
            .map(|handle| (*handle, self.store.sobject(*handle).clone()))
            .collect()
    }

    fn restore_sobject_images(&mut self, images: &[(SObjectId, super::SObjectInstance)]) {
        for (handle, original) in images {
            *self.store.sobject_mut(*handle) = original.clone();
        }
    }

    fn execute_dml_group_inner(
        &mut self,
        operation: DmlOperation,
        group: &[PreparedDmlRecord],
        group_handles: &[SObjectId],
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<Vec<DmlRowOutcome>, Diagnostic> {
        let images = self.prepare_dml_trigger_images(group, group_handles, schema, span)?;
        self.execute_trigger_phase(
            operation,
            TriggerPhase::Before,
            &images.object,
            group_handles,
            &images.old_handles,
            span,
        )?;
        let rows = self.dml_rows_from_group(group, group_handles, schema, span)?;
        let outcomes = self
            .host
            .dml(schema, operation, rows)
            .map_err(|error| runtime_exception("DmlException", error.to_string(), span))?;
        validate_group_outcomes(group, &outcomes, span)?;
        let (after_handles, after_old_handles) = self.apply_dml_outcomes(
            group,
            group_handles,
            &outcomes,
            &images.old_by_index,
            schema,
            span,
        )?;
        if !after_handles.is_empty() {
            self.execute_trigger_phase(
                operation,
                TriggerPhase::After,
                &images.object,
                &after_handles,
                &after_old_handles,
                span,
            )?;
        }
        Ok(outcomes)
    }

    fn prepare_dml_trigger_images(
        &mut self,
        group: &[PreparedDmlRecord],
        group_handles: &[SObjectId],
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<PreparedTriggerImages, Diagnostic> {
        let mut old_by_index = BTreeMap::new();
        for (record, handle) in group.iter().zip(group_handles) {
            if let Some(value) = &record.new {
                self.update_runtime_sobject(*handle, value, schema, span)?;
            }
            if let Some(value) = &record.old {
                old_by_index.insert(
                    record.input_index,
                    self.allocate_platform_sobject(value, schema, span)?,
                );
            }
        }
        let object = prepared_record_object(&group[0]).to_owned();
        let old_handles = group
            .iter()
            .filter_map(|record| old_by_index.get(&record.input_index).copied())
            .collect::<Vec<_>>();
        Ok(PreparedTriggerImages {
            object,
            old_by_index,
            old_handles,
        })
    }

    fn dml_rows_from_group(
        &self,
        group: &[PreparedDmlRecord],
        group_handles: &[SObjectId],
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<Vec<DmlRow>, Diagnostic> {
        group
            .iter()
            .zip(group_handles)
            .map(|(record, handle)| {
                Ok(DmlRow {
                    input_index: record.input_index,
                    record: self.platform_sobject(*handle, schema, span)?,
                })
            })
            .collect()
    }

    fn apply_dml_outcomes(
        &mut self,
        group: &[PreparedDmlRecord],
        group_handles: &[SObjectId],
        outcomes: &[DmlRowOutcome],
        old_by_index: &BTreeMap<usize, SObjectId>,
        schema: &crate::platform::SchemaCatalog,
        span: Span,
    ) -> Result<(Vec<SObjectId>, Vec<SObjectId>), Diagnostic> {
        let mut after_handles = Vec::new();
        let mut after_old_handles = Vec::new();
        for outcome in outcomes {
            if let Some(value) = &outcome.record {
                let handle = handles_for_group(group, group_handles, outcome.input_index);
                self.update_runtime_sobject(handle, value, schema, span)?;
                after_handles.push(handle);
                if let Some(old) = old_by_index.get(&outcome.input_index) {
                    after_old_handles.push(*old);
                }
            }
        }
        Ok((after_handles, after_old_handles))
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
            let saved_execution_context = self.execution_context;
            self.execution_context = self.execution_context.for_trigger();
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
            self.execution_context = saved_execution_context;

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

    pub(super) fn evaluate_dml_result_method(
        &mut self,
        target: DmlResultMethod,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let receiver = match evaluated_receiver {
            Some(receiver) => receiver,
            None => self.evaluate(receiver)?,
        };
        let Value::Platform(id) = receiver else {
            return Err(runtime_exception(
                "NullPointerException",
                "Database result receiver is null",
                span,
            ));
        };
        let PlatformValue::DmlResult { ty, outcome } = self.store.platform(id).clone() else {
            return Err(Diagnostic::new(
                "invalid checked Database result receiver",
                span,
            ));
        };
        match target {
            DmlResultMethod::IsSuccess => Ok(Value::Boolean(outcome.is_success())),
            DmlResultMethod::GetId => Ok(outcome.id.map_or_else(
                || Value::Null(Some(TypeName::Id)),
                |id| Value::Id(id.to_string()),
            )),
            DmlResultMethod::GetErrors => {
                let elements = outcome
                    .errors
                    .into_iter()
                    .map(|error| self.store.allocate_platform(PlatformValue::DmlError(error)))
                    .collect();
                Ok(self.store.allocate_collection(Collection::List {
                    element_type: TypeName::DatabaseError,
                    elements,
                    iteration_depth: 0,
                }))
            }
            DmlResultMethod::IsCreated if ty == TypeName::UpsertResult => {
                Ok(Value::Boolean(outcome.created))
            }
            DmlResultMethod::IsCreated => Err(Diagnostic::new(
                "isCreated target attached to a non-UpsertResult",
                span,
            )),
        }
    }

    pub(super) fn evaluate_dml_error_method(
        &mut self,
        target: DmlErrorMethod,
        receiver: &Expression,
        evaluated_receiver: Option<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let receiver = match evaluated_receiver {
            Some(receiver) => receiver,
            None => self.evaluate(receiver)?,
        };
        let Value::Platform(id) = receiver else {
            return Err(runtime_exception(
                "NullPointerException",
                "Database.Error receiver is null",
                span,
            ));
        };
        let PlatformValue::DmlError(error) = self.store.platform(id).clone() else {
            return Err(Diagnostic::new(
                "invalid checked Database.Error receiver",
                span,
            ));
        };
        match target {
            DmlErrorMethod::GetStatusCode => Ok(self
                .store
                .allocate_platform(PlatformValue::DmlStatus(error.status))),
            DmlErrorMethod::GetMessage => Ok(Value::String(error.message)),
            DmlErrorMethod::GetFields => {
                let elements = error.fields.into_iter().map(Value::String).collect();
                Ok(self.store.allocate_collection(Collection::List {
                    element_type: TypeName::String,
                    elements,
                    iteration_depth: 0,
                }))
            }
        }
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
            access: query.access,
            sharing: self.execution_context.sharing_mode(),
            user_id: self.current_user_context().user_id,
            visible_record_ids: None,
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

fn ensure_one_dml_object(prepared: &[PreparedDmlRecord], span: Span) -> Result<(), Diagnostic> {
    let first_object = prepared.first().map(prepared_record_object);
    if prepared.iter().any(|record| {
        first_object
            .is_some_and(|first| !prepared_record_object(record).eq_ignore_ascii_case(first))
    }) {
        Err(runtime_exception(
            "DmlException",
            "mixed-SObject bulk DML is not supported",
            span,
        ))
    } else {
        Ok(())
    }
}

fn prepared_record_object(record: &PreparedDmlRecord) -> &str {
    record
        .new
        .as_ref()
        .or(record.old.as_ref())
        .expect("prepared DML record has an old or new value")
        .object_api_name()
}

fn validate_group_outcomes(
    group: &[PreparedDmlRecord],
    outcomes: &[DmlRowOutcome],
    span: Span,
) -> Result<(), Diagnostic> {
    if outcomes.len() != group.len() {
        return Err(Diagnostic::new(
            "platform DML returned an invalid record count",
            span,
        ));
    }
    let mut expected = group
        .iter()
        .map(|record| record.input_index)
        .collect::<Vec<_>>();
    let mut actual = outcomes
        .iter()
        .map(|outcome| outcome.input_index)
        .collect::<Vec<_>>();
    expected.sort_unstable();
    actual.sort_unstable();
    if actual != expected || actual.windows(2).any(|indices| indices[0] == indices[1]) {
        return Err(Diagnostic::new(
            "platform DML returned invalid row indexes",
            span,
        ));
    }
    Ok(())
}

fn handles_for_group(
    group: &[PreparedDmlRecord],
    handles: &[SObjectId],
    input_index: usize,
) -> SObjectId {
    group
        .iter()
        .position(|record| record.input_index == input_index)
        .and_then(|position| handles.get(position).copied())
        .expect("validated DML outcome index belongs to the concrete group")
}

fn dml_outcome_exception(outcome: &DmlRowOutcome, span: Span) -> Diagnostic {
    let message = outcome
        .errors
        .first()
        .map(|error| format!("{}: {}", error.status.apex_name(), error.message))
        .unwrap_or_else(|| "DML row failed without a structured error".to_owned());
    runtime_exception("DmlException", message, span)
}

fn trigger_failure_error(error: &Diagnostic) -> DmlError {
    DmlError::new(
        DmlStatus::CannotInsertUpdateActivateEntity,
        error.message.clone(),
        [],
    )
}

fn group_failure_outcomes(group: &[PreparedDmlRecord], error: &Diagnostic) -> Vec<DmlRowOutcome> {
    let failure = trigger_failure_error(error);
    group
        .iter()
        .map(|record| DmlRowOutcome::failure(record.input_index, vec![failure.clone()]))
        .collect()
}

fn partial_dml_retry_exception(span: Span) -> Diagnostic {
    runtime_exception(
        "DmlException",
        "Too many batch retries in the presence of Apex triggers and partial failures.",
        span,
    )
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
