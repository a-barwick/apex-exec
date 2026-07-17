use crate::platform::{
    DatabaseError, DatabaseSnapshot, DmlOperation, LocalDatabase, PreparedDmlRecord, QueryOutcome,
    SObject, SchemaCatalog, SoqlRequest, SoslRequest,
};

/// A structured debug event emitted by the Apex `System.debug` intrinsic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugEvent {
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryKind {
    Soql,
    Sosl,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryEvent {
    pub kind: QueryKind,
    pub objects: Vec<String>,
    pub rows: usize,
    pub succeeded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmlEvent {
    pub operation: DmlOperation,
    pub objects: Vec<String>,
    pub records: usize,
    pub succeeded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerPhase {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerStage {
    Enter,
    Exit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerEvent {
    pub trigger: String,
    pub object: String,
    pub operation: DmlOperation,
    pub phase: TriggerPhase,
    pub stage: TriggerStage,
    pub depth: usize,
    pub records: usize,
    pub succeeded: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionEvent {
    Trigger(TriggerEvent),
    Dml(DmlEvent),
}

/// Boundary between language execution and platform-owned side effects.
///
/// The initial host surface is deliberately narrow. M7 can extend this
/// boundary with schema and database services without coupling those services
/// to expression evaluation.
pub trait PlatformHost {
    fn debug(&mut self, event: DebugEvent);

    /// Drains debug messages for the existing convenience execution APIs.
    ///
    /// Hosts that stream events elsewhere can keep the default empty result.
    fn take_debug_output(&mut self) -> Vec<String> {
        Vec::new()
    }

    fn soql(
        &mut self,
        _schema: &SchemaCatalog,
        _request: &SoqlRequest,
    ) -> Result<QueryOutcome, DatabaseError> {
        Err(DatabaseError::unavailable())
    }

    fn sosl(
        &mut self,
        _schema: &SchemaCatalog,
        _request: &SoslRequest,
    ) -> Result<QueryOutcome, DatabaseError> {
        Err(DatabaseError::unavailable())
    }

    fn dml(
        &mut self,
        _schema: &SchemaCatalog,
        _operation: DmlOperation,
        _records: Vec<SObject>,
    ) -> Result<Vec<SObject>, DatabaseError> {
        Err(DatabaseError::unavailable())
    }

    fn prepare_dml(
        &mut self,
        _schema: &SchemaCatalog,
        operation: DmlOperation,
        records: &[SObject],
    ) -> Result<Vec<PreparedDmlRecord>, DatabaseError> {
        Ok(records
            .iter()
            .cloned()
            .map(|record| {
                let concrete = match operation {
                    DmlOperation::Upsert if record.id().is_some() => DmlOperation::Update,
                    DmlOperation::Upsert => DmlOperation::Insert,
                    operation => operation,
                };
                PreparedDmlRecord {
                    operation: concrete,
                    old: (concrete == DmlOperation::Delete).then(|| record.clone()),
                    new: (concrete != DmlOperation::Delete).then_some(record),
                }
            })
            .collect())
    }

    fn begin_unit(&mut self, _schema: &SchemaCatalog) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn commit_unit(&mut self) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn rollback_unit(&mut self) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn trigger(&mut self, _event: TriggerEvent) {}
}

impl<T: PlatformHost + ?Sized> PlatformHost for &mut T {
    fn debug(&mut self, event: DebugEvent) {
        (**self).debug(event);
    }

    fn take_debug_output(&mut self) -> Vec<String> {
        (**self).take_debug_output()
    }

    fn soql(
        &mut self,
        schema: &SchemaCatalog,
        request: &SoqlRequest,
    ) -> Result<QueryOutcome, DatabaseError> {
        (**self).soql(schema, request)
    }

    fn sosl(
        &mut self,
        schema: &SchemaCatalog,
        request: &SoslRequest,
    ) -> Result<QueryOutcome, DatabaseError> {
        (**self).sosl(schema, request)
    }

    fn dml(
        &mut self,
        schema: &SchemaCatalog,
        operation: DmlOperation,
        records: Vec<SObject>,
    ) -> Result<Vec<SObject>, DatabaseError> {
        (**self).dml(schema, operation, records)
    }

    fn prepare_dml(
        &mut self,
        schema: &SchemaCatalog,
        operation: DmlOperation,
        records: &[SObject],
    ) -> Result<Vec<PreparedDmlRecord>, DatabaseError> {
        (**self).prepare_dml(schema, operation, records)
    }

    fn begin_unit(&mut self, schema: &SchemaCatalog) -> Result<(), DatabaseError> {
        (**self).begin_unit(schema)
    }

    fn commit_unit(&mut self) -> Result<(), DatabaseError> {
        (**self).commit_unit()
    }

    fn rollback_unit(&mut self) -> Result<(), DatabaseError> {
        (**self).rollback_unit()
    }

    fn trigger(&mut self, event: TriggerEvent) {
        (**self).trigger(event);
    }
}

/// Default host used by the public convenience APIs.
#[derive(Default)]
pub struct RecordingHost {
    output: Vec<String>,
    database: Option<LocalDatabase>,
    queries: Vec<QueryEvent>,
    dml: Vec<DmlEvent>,
    triggers: Vec<TriggerEvent>,
    timeline: Vec<TransactionEvent>,
    checkpoints: Vec<DatabaseSnapshot>,
}

impl RecordingHost {
    pub fn query_events(&self) -> &[QueryEvent] {
        &self.queries
    }

    pub fn dml_events(&self) -> &[DmlEvent] {
        &self.dml
    }

    pub fn trigger_events(&self) -> &[TriggerEvent] {
        &self.triggers
    }

    pub fn timeline_events(&self) -> &[TransactionEvent] {
        &self.timeline
    }

    fn database(&mut self, schema: &SchemaCatalog) -> Result<&mut LocalDatabase, DatabaseError> {
        if self.database.is_none() {
            self.database = Some(LocalDatabase::new(schema.clone())?);
        } else if self
            .database
            .as_ref()
            .is_some_and(|database| database.schema() != schema)
        {
            self.database
                .as_mut()
                .expect("database presence was checked")
                .migrate(schema.clone())?;
        }
        Ok(self.database.as_mut().expect("database was initialized"))
    }
}

impl PlatformHost for RecordingHost {
    fn debug(&mut self, event: DebugEvent) {
        self.output.push(event.message);
    }

    fn take_debug_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.output)
    }

    fn soql(
        &mut self,
        schema: &SchemaCatalog,
        request: &SoqlRequest,
    ) -> Result<QueryOutcome, DatabaseError> {
        let result = self.database(schema)?.execute_soql(request);
        let rows = result.as_ref().map_or(0, outcome_rows);
        self.queries.push(QueryEvent {
            kind: QueryKind::Soql,
            objects: vec![request.object.clone()],
            rows,
            succeeded: result.is_ok(),
        });
        result
    }

    fn sosl(
        &mut self,
        schema: &SchemaCatalog,
        request: &SoslRequest,
    ) -> Result<QueryOutcome, DatabaseError> {
        let result = self.database(schema)?.execute_sosl(request);
        let rows = result.as_ref().map_or(0, outcome_rows);
        self.queries.push(QueryEvent {
            kind: QueryKind::Sosl,
            objects: request
                .returning
                .iter()
                .map(|returning| returning.object.clone())
                .collect(),
            rows,
            succeeded: result.is_ok(),
        });
        result
    }

    fn dml(
        &mut self,
        schema: &SchemaCatalog,
        operation: DmlOperation,
        records: Vec<SObject>,
    ) -> Result<Vec<SObject>, DatabaseError> {
        let objects = records
            .iter()
            .map(|record| record.object_api_name().to_owned())
            .collect::<Vec<_>>();
        let count = records.len();
        let result = self.database(schema)?.execute_dml(operation, records);
        let event = DmlEvent {
            operation,
            objects,
            records: count,
            succeeded: result.is_ok(),
        };
        self.dml.push(event.clone());
        self.timeline.push(TransactionEvent::Dml(event));
        result
    }

    fn prepare_dml(
        &mut self,
        schema: &SchemaCatalog,
        operation: DmlOperation,
        records: &[SObject],
    ) -> Result<Vec<PreparedDmlRecord>, DatabaseError> {
        self.database(schema)?.prepare_dml(operation, records)
    }

    fn begin_unit(&mut self, schema: &SchemaCatalog) -> Result<(), DatabaseError> {
        let snapshot = self.database(schema)?.snapshot()?;
        self.checkpoints.push(snapshot);
        Ok(())
    }

    fn commit_unit(&mut self) -> Result<(), DatabaseError> {
        self.checkpoints
            .pop()
            .ok_or_else(|| DatabaseError::new("no active transaction checkpoint"))?;
        Ok(())
    }

    fn rollback_unit(&mut self) -> Result<(), DatabaseError> {
        let snapshot = self
            .checkpoints
            .pop()
            .ok_or_else(|| DatabaseError::new("no active transaction checkpoint"))?;
        let database = self
            .database
            .as_mut()
            .ok_or_else(|| DatabaseError::new("transaction database is unavailable"))?;
        database.restore(snapshot)
    }

    fn trigger(&mut self, event: TriggerEvent) {
        self.triggers.push(event.clone());
        self.timeline.push(TransactionEvent::Trigger(event));
    }
}

fn outcome_rows(outcome: &QueryOutcome) -> usize {
    match outcome {
        QueryOutcome::Records(rows) => rows.len(),
        QueryOutcome::Count(_) => 1,
        QueryOutcome::Aggregates(rows) => rows.len(),
        QueryOutcome::Search(groups) => groups.iter().map(Vec::len).sum(),
    }
}
