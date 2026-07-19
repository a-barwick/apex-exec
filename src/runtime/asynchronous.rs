use super::{
    AsyncEvent, AsyncJobKind, AsyncStage, AsyncWork, Collection, CollectionId, CurrentAsync,
    EvaluatedArgument, Interpreter, ObjectId, PendingAsyncJob, PlatformHost, PlatformValue,
    SObjectId, Slot, TypeName, Value, runtime_exception,
};
use crate::{
    diagnostic::Diagnostic,
    hir::ClassMemberId,
    platform::{DmlOperation as PlatformDmlOperation, RecordId},
    span::Span,
};
use std::collections::{BTreeMap, HashMap};

const MAX_ASYNC_JOBS_PER_DRAIN: usize = 100;
const DEFAULT_BATCH_SCOPE_SIZE: usize = 200;
const MAX_BATCH_SCOPE_SIZE: usize = 2_000;

#[derive(Default)]
struct CloneMemo {
    collections: HashMap<CollectionId, CollectionId>,
    objects: HashMap<ObjectId, ObjectId>,
    sobjects: HashMap<SObjectId, SObjectId>,
}

impl<'program, H: PlatformHost> Interpreter<'program, H> {
    pub(super) fn enqueue_future(
        &mut self,
        target: ClassMemberId,
        arguments: Vec<EvaluatedArgument>,
        span: Span,
    ) -> Result<String, Diagnostic> {
        let mut memo = CloneMemo::default();
        let arguments = arguments
            .into_iter()
            .map(|argument| {
                Ok(EvaluatedArgument {
                    value: self.clone_async_value(argument.value, span, &mut memo)?,
                    span: argument.span,
                })
            })
            .collect::<Result<Vec<_>, Diagnostic>>()?;
        self.enqueue_work(
            AsyncJobKind::Future,
            AsyncWork::Future { target, arguments },
            span,
        )
    }

    pub(super) fn enqueue_queueable(
        &mut self,
        value: Value,
        span: Span,
    ) -> Result<String, Diagnostic> {
        let receiver = self.snapshot_async_object(value, span)?;
        self.enqueue_work(
            AsyncJobKind::Queueable,
            AsyncWork::Queueable { receiver },
            span,
        )
    }

    pub(super) fn enqueue_batch(
        &mut self,
        value: Value,
        scope_size: i64,
        span: Span,
    ) -> Result<String, Diagnostic> {
        let scope_size = usize::try_from(scope_size)
            .ok()
            .filter(|size| (1..=MAX_BATCH_SCOPE_SIZE).contains(size))
            .ok_or_else(|| {
                async_exception(
                    format!("batch scope size must be between 1 and {MAX_BATCH_SCOPE_SIZE}"),
                    span,
                )
            })?;
        let receiver = self.snapshot_async_object(value, span)?;
        self.enqueue_work(
            AsyncJobKind::Batch,
            AsyncWork::Batch {
                receiver,
                scope_size,
            },
            span,
        )
    }

    pub(super) fn enqueue_scheduled(
        &mut self,
        value: Value,
        span: Span,
    ) -> Result<String, Diagnostic> {
        let receiver = self.snapshot_async_object(value, span)?;
        self.enqueue_work(
            AsyncJobKind::Scheduled,
            AsyncWork::Scheduled { receiver },
            span,
        )
    }

    pub(super) fn enqueue_platform_events(
        &mut self,
        value: Value,
        span: Span,
    ) -> Result<String, Diagnostic> {
        let mut memo = CloneMemo::default();
        let snapshot = self.clone_async_value(value, span, &mut memo)?;
        let records = match snapshot {
            Value::SObject(id) => vec![id],
            Value::Collection(id) => match self.store.collection(id) {
                Collection::List { elements, .. } => elements
                    .iter()
                    .map(|value| match value {
                        Value::SObject(id) => Ok(*id),
                        _ => Err(async_exception(
                            "platform event collection contains a null or non-SObject value",
                            span,
                        )),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => {
                    return Err(async_exception(
                        "EventBus.publish requires an event or List of events",
                        span,
                    ));
                }
            },
            _ => {
                return Err(async_exception(
                    "EventBus.publish requires a non-null platform event",
                    span,
                ));
            }
        };
        if records.is_empty() {
            return Err(async_exception(
                "EventBus.publish requires at least one platform event",
                span,
            ));
        }
        self.enqueue_work(
            AsyncJobKind::PlatformEvent,
            AsyncWork::PlatformEvent { records },
            span,
        )
    }

    fn snapshot_async_object(&mut self, value: Value, span: Span) -> Result<ObjectId, Diagnostic> {
        let mut memo = CloneMemo::default();
        match self.clone_async_value(value, span, &mut memo)? {
            Value::Object(id) => Ok(id),
            Value::Null(_) => Err(async_exception(
                "async submission requires a non-null class instance",
                span,
            )),
            _ => Err(async_exception(
                "async submission requires a class instance",
                span,
            )),
        }
    }

    fn enqueue_work(
        &mut self,
        kind: AsyncJobKind,
        work: AsyncWork,
        span: Span,
    ) -> Result<String, Diagnostic> {
        if self.async_queue.len() >= MAX_ASYNC_JOBS_PER_DRAIN {
            return Err(async_exception(
                format!(
                    "deterministic async queue is limited to {MAX_ASYNC_JOBS_PER_DRAIN} pending jobs"
                ),
                span,
            ));
        }
        let id = RecordId::generate("707", self.next_async_sequence)
            .expect("async key prefix and sequence are valid")
            .to_string();
        self.next_async_sequence += 1;
        let parent_id = self
            .current_async
            .as_ref()
            .map(|context| context.id.clone());
        self.async_queue.push_back(PendingAsyncJob {
            id: id.clone(),
            parent_id: parent_id.clone(),
            kind,
            work,
            span,
            execution_context: self.execution_context.for_async_job(),
        });
        self.host.async_event(AsyncEvent {
            job_id: id.clone(),
            parent_job_id: parent_id,
            kind,
            stage: AsyncStage::Queued,
        });
        Ok(id)
    }

    pub(super) fn drain_async_jobs(&mut self, span: Span) -> Result<(), Diagnostic> {
        let mut drained = 0usize;
        while let Some(job) = self.async_queue.pop_front() {
            drained += 1;
            if drained > MAX_ASYNC_JOBS_PER_DRAIN {
                return Err(async_exception(
                    format!(
                        "async drain exceeded the deterministic limit of {MAX_ASYNC_JOBS_PER_DRAIN} jobs"
                    ),
                    span,
                ));
            }

            self.host.async_event(AsyncEvent {
                job_id: job.id.clone(),
                parent_job_id: job.parent_id.clone(),
                kind: job.kind,
                stage: AsyncStage::Started,
            });
            let previous = self.current_async.replace(CurrentAsync {
                id: job.id.clone(),
                kind: job.kind,
            });
            let previous_execution_context =
                std::mem::replace(&mut self.execution_context, job.execution_context);
            let result = self.execute_async_transaction(&job);
            self.execution_context = previous_execution_context;
            self.current_async = previous;
            self.host.async_event(AsyncEvent {
                job_id: job.id,
                parent_job_id: job.parent_id,
                kind: job.kind,
                stage: if result.is_ok() {
                    AsyncStage::Completed
                } else {
                    AsyncStage::Failed
                },
            });
            result?;
        }
        Ok(())
    }

    fn execute_async_transaction(&mut self, job: &PendingAsyncJob) -> Result<(), Diagnostic> {
        self.begin_transaction(job.span)?;
        let execution = self.execute_async_job(job);
        self.finish_transaction(execution, job.span)
    }

    fn execute_async_job(&mut self, job: &PendingAsyncJob) -> Result<(), Diagnostic> {
        match &job.work {
            AsyncWork::Queueable { receiver } => {
                let class_id = self.store.object(*receiver).class_id;
                let target = self
                    .program()
                    .async_contract(class_id)
                    .and_then(|contract| contract.queueable)
                    .ok_or_else(|| {
                        Diagnostic::new("missing checked Queueable contract", job.span)
                    })?;
                let context = self.async_context_value(TypeName::QueueableContext, &job.id);
                self.evaluate_class_method_arguments(
                    target,
                    Some(*receiver),
                    vec![EvaluatedArgument {
                        value: context,
                        span: job.span,
                    }],
                    job.span,
                    true,
                    false,
                )?;
                Ok(())
            }
            AsyncWork::Future { target, arguments } => {
                self.evaluate_class_method_arguments(
                    *target,
                    None,
                    arguments.clone(),
                    job.span,
                    false,
                    false,
                )?;
                Ok(())
            }
            AsyncWork::Batch {
                receiver,
                scope_size,
            } => self.execute_batch_job(*receiver, *scope_size, &job.id, job.span),
            AsyncWork::Scheduled { receiver } => {
                let class_id = self.store.object(*receiver).class_id;
                let target = self
                    .program()
                    .async_contract(class_id)
                    .and_then(|contract| contract.schedulable)
                    .ok_or_else(|| {
                        Diagnostic::new("missing checked Schedulable contract", job.span)
                    })?;
                let context = self.async_context_value(TypeName::SchedulableContext, &job.id);
                self.evaluate_class_method_arguments(
                    target,
                    Some(*receiver),
                    vec![EvaluatedArgument {
                        value: context,
                        span: job.span,
                    }],
                    job.span,
                    true,
                    false,
                )?;
                Ok(())
            }
            AsyncWork::PlatformEvent { records } => self.deliver_platform_events(records, job.span),
        }
    }

    fn execute_batch_job(
        &mut self,
        receiver: ObjectId,
        scope_size: usize,
        job_id: &str,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let class_id = self.store.object(receiver).class_id;
        let contract = self
            .program()
            .async_contract(class_id)
            .and_then(|contract| contract.batch.clone())
            .ok_or_else(|| Diagnostic::new("missing checked Batchable contract", span))?;
        let context = self.async_context_value(TypeName::BatchableContext, job_id);
        let start_result = self.evaluate_class_method_arguments(
            contract.start,
            Some(receiver),
            vec![EvaluatedArgument {
                value: context.clone(),
                span,
            }],
            span,
            true,
            false,
        )?;
        let elements = match start_result {
            Value::Collection(id) => match self.store.collection(id) {
                Collection::List { elements, .. } => elements.clone(),
                _ => {
                    return Err(async_exception("Batchable.start must return a List", span));
                }
            },
            Value::Null(_) => {
                return Err(async_exception("Batchable.start returned null", span));
            }
            _ => {
                return Err(async_exception(
                    "Batchable.start returned a non-List value",
                    span,
                ));
            }
        };

        for chunk in elements.chunks(scope_size) {
            let scope = self.store.allocate_collection(Collection::List {
                element_type: contract.scope_type.clone(),
                elements: chunk.to_vec(),
                iteration_depth: 0,
            });
            self.evaluate_class_method_arguments(
                contract.execute,
                Some(receiver),
                vec![
                    EvaluatedArgument {
                        value: context.clone(),
                        span,
                    },
                    EvaluatedArgument { value: scope, span },
                ],
                span,
                true,
                false,
            )?;
        }
        self.evaluate_class_method_arguments(
            contract.finish,
            Some(receiver),
            vec![EvaluatedArgument {
                value: context,
                span,
            }],
            span,
            true,
            false,
        )?;
        Ok(())
    }

    fn deliver_platform_events(
        &mut self,
        records: &[SObjectId],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let mut groups = BTreeMap::<String, Vec<SObjectId>>::new();
        for record in records {
            let instance = self.store.sobject(*record);
            let object = self
                .program()
                .schema()
                .object_at(instance.object_id)
                .expect("checked platform event type is valid");
            if !object.api_name().to_ascii_lowercase().ends_with("__e") {
                return Err(async_exception(
                    format!("`{}` is not a platform event type", object.api_name()),
                    span,
                ));
            }
            groups
                .entry(object.api_name().to_ascii_lowercase())
                .or_default()
                .push(*record);
        }
        for (canonical, handles) in groups {
            let object = self
                .program()
                .schema()
                .objects()
                .find(|object| object.api_name().eq_ignore_ascii_case(&canonical))
                .expect("grouped platform event schema exists")
                .api_name()
                .to_owned();
            self.execute_trigger_phase(
                PlatformDmlOperation::Insert,
                super::TriggerPhase::After,
                &object,
                &handles,
                &[],
                span,
            )?;
        }
        Ok(())
    }

    fn async_context_value(&mut self, ty: TypeName, job_id: &str) -> Value {
        self.store.allocate_platform(PlatformValue::AsyncContext {
            ty,
            job_id: job_id.to_owned(),
        })
    }

    pub(super) fn current_async_kind(&self) -> Option<AsyncJobKind> {
        self.current_async.as_ref().map(|context| context.kind)
    }

    fn clone_async_value(
        &mut self,
        value: Value,
        span: Span,
        memo: &mut CloneMemo,
    ) -> Result<Value, Diagnostic> {
        match value {
            value @ (Value::String(_) | Value::Boolean(_) | Value::Integer(_) | Value::Long(_)) => {
                Ok(value)
            }
            value @ (Value::Decimal(_) | Value::Date(_) | Value::Datetime(_)) => Ok(value),
            value @ (Value::Time(_) | Value::Id(_) | Value::Null(_)) => Ok(value),
            Value::Collection(source) => {
                if let Some(snapshot) = memo.collections.get(&source) {
                    return Ok(Value::Collection(*snapshot));
                }
                let source_value = self.store.collection(source).clone();
                let empty = match &source_value {
                    Collection::List { element_type, .. } => Collection::List {
                        element_type: element_type.clone(),
                        elements: Vec::new(),
                        iteration_depth: 0,
                    },
                    Collection::Set { element_type, .. } => Collection::Set {
                        element_type: element_type.clone(),
                        elements: Vec::new(),
                        iteration_depth: 0,
                    },
                    Collection::Map {
                        key_type,
                        value_type,
                        ..
                    } => Collection::Map {
                        key_type: key_type.clone(),
                        value_type: value_type.clone(),
                        entries: Vec::new(),
                    },
                };
                let Value::Collection(snapshot) = self.store.allocate_collection(empty) else {
                    unreachable!()
                };
                memo.collections.insert(source, snapshot);
                let cloned = match source_value {
                    Collection::List {
                        element_type,
                        elements,
                        ..
                    } => Collection::List {
                        element_type,
                        elements: elements
                            .into_iter()
                            .map(|value| self.clone_async_value(value, span, memo))
                            .collect::<Result<Vec<_>, _>>()?,
                        iteration_depth: 0,
                    },
                    Collection::Set {
                        element_type,
                        elements,
                        ..
                    } => Collection::Set {
                        element_type,
                        elements: elements
                            .into_iter()
                            .map(|value| self.clone_async_value(value, span, memo))
                            .collect::<Result<Vec<_>, _>>()?,
                        iteration_depth: 0,
                    },
                    Collection::Map {
                        key_type,
                        value_type,
                        entries,
                    } => Collection::Map {
                        key_type,
                        value_type,
                        entries: entries
                            .into_iter()
                            .map(|(key, value)| {
                                Ok((
                                    self.clone_async_value(key, span, memo)?,
                                    self.clone_async_value(value, span, memo)?,
                                ))
                            })
                            .collect::<Result<Vec<_>, Diagnostic>>()?,
                    },
                };
                *self.store.collection_mut(snapshot) = cloned;
                Ok(Value::Collection(snapshot))
            }
            Value::Object(source) => {
                if let Some(snapshot) = memo.objects.get(&source) {
                    return Ok(Value::Object(*snapshot));
                }
                let class_id = self.store.object(source).class_id;
                let fields = self.store.object(source).fields.clone();
                let snapshot = self.store.allocate_object(class_id);
                memo.objects.insert(source, snapshot);
                for (target, slot) in fields {
                    let value = self.clone_async_value(slot.value, span, memo)?;
                    self.store
                        .object_mut(snapshot)
                        .fields
                        .insert(target, Slot { ty: slot.ty, value });
                }
                Ok(Value::Object(snapshot))
            }
            Value::SObject(source) => {
                if let Some(snapshot) = memo.sobjects.get(&source) {
                    return Ok(Value::SObject(*snapshot));
                }
                let instance = self.store.sobject(source).clone();
                let Value::SObject(snapshot) = self.store.allocate_sobject(instance.object_id)
                else {
                    unreachable!()
                };
                memo.sobjects.insert(source, snapshot);
                let fields = instance
                    .fields
                    .into_iter()
                    .map(|(field, value)| Ok((field, self.clone_async_value(value, span, memo)?)))
                    .collect::<Result<BTreeMap<_, _>, Diagnostic>>()?;
                let relationships = instance
                    .relationships
                    .into_iter()
                    .map(|(field, related)| {
                        let Value::SObject(related) =
                            self.clone_async_value(Value::SObject(related), span, memo)?
                        else {
                            unreachable!()
                        };
                        Ok((field, related))
                    })
                    .collect::<Result<BTreeMap<_, _>, Diagnostic>>()?;
                let snapshot_instance = self.store.sobject_mut(snapshot);
                snapshot_instance.fields = fields;
                snapshot_instance.relationships = relationships;
                Ok(Value::SObject(snapshot))
            }
            Value::Platform(source) => match self.store.platform(source).clone() {
                PlatformValue::Blob(bytes) => {
                    Ok(self.store.allocate_platform(PlatformValue::Blob(bytes)))
                }
                _ => Err(async_exception(
                    "async payload contains a non-serializable platform value",
                    span,
                )),
            },
            Value::AggregateResult(_) | Value::Exception(_) | Value::Void => Err(async_exception(
                "async payload contains a non-serializable value",
                span,
            )),
        }
    }
}

pub(super) fn default_batch_scope_size() -> i64 {
    DEFAULT_BATCH_SCOPE_SIZE as i64
}

fn async_exception(message: impl Into<String>, span: Span) -> Diagnostic {
    runtime_exception("AsyncException", message, span)
}
