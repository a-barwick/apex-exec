use crate::platform::{
    DatabaseError, DmlOperation, LocalDatabase, QueryOutcome, SObject, SchemaCatalog, SoqlRequest,
    SoslRequest,
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
}

/// Default host used by the public convenience APIs.
#[derive(Default)]
pub struct RecordingHost {
    output: Vec<String>,
    database: Option<LocalDatabase>,
    queries: Vec<QueryEvent>,
    dml: Vec<DmlEvent>,
}

impl RecordingHost {
    pub fn query_events(&self) -> &[QueryEvent] {
        &self.queries
    }

    pub fn dml_events(&self) -> &[DmlEvent] {
        &self.dml
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
        self.dml.push(DmlEvent {
            operation,
            objects,
            records: count,
            succeeded: result.is_ok(),
        });
        result
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
