use crate::{
    compatibility::CompatibilityProfile,
    platform::{
        AccessType, DatabaseError, DatabaseSnapshot, DmlOperation, DmlRequest, DmlRow,
        DmlRowOutcome, LocalDatabase, PreparedDmlOutcome, QueryOutcome, SchemaCatalog,
        SecurityError, SecurityPolicy, SoqlRequest, SoslRequest,
    },
    runtime::security::{secure_dml_request, secure_soql_request},
};
use std::collections::{BTreeMap, VecDeque};

pub const M11_ASYNC_PROFILE: &str = "m11-deterministic-async";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserContext {
    pub user_id: String,
    pub username: String,
}

impl Default for UserContext {
    fn default() -> Self {
        Self {
            user_id: "005000000000001AAA".to_owned(),
            username: "local@example.invalid".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequestData {
    pub endpoint: String,
    pub method: String,
    pub body: String,
    pub headers: BTreeMap<String, String>,
    pub timeout_ms: i64,
}

impl Default for HttpRequestData {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            method: "GET".to_owned(),
            body: String::new(),
            headers: BTreeMap::new(),
            timeout_ms: 10_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponseData {
    pub status_code: i64,
    pub status: String,
    pub body: String,
    pub headers: BTreeMap<String, String>,
}

impl Default for HttpResponseData {
    fn default() -> Self {
        Self {
            status_code: 200,
            status: "OK".to_owned(),
            body: String::new(),
            headers: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LimitUsage {
    pub queries: i64,
    pub dml_statements: i64,
    pub callouts: i64,
}

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
    pub object_scans: usize,
    pub succeeded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmlEvent {
    pub operation: DmlOperation,
    pub objects: Vec<String>,
    pub records: usize,
    pub successful_records: usize,
    pub failed_records: usize,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncJobKind {
    Queueable,
    Future,
    Batch,
    Scheduled,
    PlatformEvent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncStage {
    Queued,
    Started,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsyncEvent {
    pub job_id: String,
    pub parent_job_id: Option<String>,
    pub kind: AsyncJobKind,
    pub stage: AsyncStage,
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
        _rows: Vec<DmlRow>,
    ) -> Result<Vec<DmlRowOutcome>, DatabaseError> {
        Err(DatabaseError::unavailable())
    }

    fn prepare_dml(
        &mut self,
        _schema: &SchemaCatalog,
        _request: &DmlRequest,
    ) -> Result<Vec<PreparedDmlOutcome>, DatabaseError> {
        Err(DatabaseError::unavailable())
    }

    fn record_dml(&mut self, _event: DmlEvent) {}

    fn begin_dml_retry_scope(&mut self) {}

    fn reset_dml_retry_limits(&mut self) {}

    fn end_dml_retry_scope(&mut self) {}

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

    fn async_event(&mut self, _event: AsyncEvent) {}

    /// Number of transaction timeline events currently visible to a debugger.
    fn transaction_event_count(&self) -> usize {
        0
    }

    /// Deterministic UTC wall clock, represented as Unix epoch milliseconds.
    fn now_millis(&mut self) -> i64 {
        1_735_689_600_000 // 2025-01-01T00:00:00Z
    }

    fn random_u64(&mut self) -> u64 {
        0x4d59_5df4_d0f3_3173
    }

    fn user_context(&self) -> UserContext {
        UserContext::default()
    }

    fn security_object_access(
        &self,
        _user_id: &str,
        _object: &str,
        _access: AccessType,
    ) -> Result<bool, SecurityError> {
        Err(SecurityError::Unavailable)
    }

    fn security_field_access(
        &self,
        _user_id: &str,
        _object: &str,
        _field: &str,
        _access: AccessType,
    ) -> Result<bool, SecurityError> {
        Err(SecurityError::Unavailable)
    }

    fn send_http(
        &mut self,
        _request: &HttpRequestData,
        profile: CompatibilityProfile,
    ) -> Result<HttpResponseData, String> {
        Err(format!(
            "HTTP callout has no configured mock in compatibility profile `{}`",
            profile.identity()
        ))
    }

    fn limit_usage(&self) -> LimitUsage {
        LimitUsage::default()
    }

    fn begin_test_window(&mut self) {}

    fn end_test_window(&mut self) {}
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
        rows: Vec<DmlRow>,
    ) -> Result<Vec<DmlRowOutcome>, DatabaseError> {
        (**self).dml(schema, operation, rows)
    }

    fn prepare_dml(
        &mut self,
        schema: &SchemaCatalog,
        request: &DmlRequest,
    ) -> Result<Vec<PreparedDmlOutcome>, DatabaseError> {
        (**self).prepare_dml(schema, request)
    }

    fn record_dml(&mut self, event: DmlEvent) {
        (**self).record_dml(event);
    }

    fn begin_dml_retry_scope(&mut self) {
        (**self).begin_dml_retry_scope();
    }

    fn reset_dml_retry_limits(&mut self) {
        (**self).reset_dml_retry_limits();
    }

    fn end_dml_retry_scope(&mut self) {
        (**self).end_dml_retry_scope();
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

    fn async_event(&mut self, event: AsyncEvent) {
        (**self).async_event(event);
    }

    fn transaction_event_count(&self) -> usize {
        (**self).transaction_event_count()
    }

    fn now_millis(&mut self) -> i64 {
        (**self).now_millis()
    }

    fn random_u64(&mut self) -> u64 {
        (**self).random_u64()
    }

    fn user_context(&self) -> UserContext {
        (**self).user_context()
    }

    fn security_object_access(
        &self,
        user_id: &str,
        object: &str,
        access: AccessType,
    ) -> Result<bool, SecurityError> {
        (**self).security_object_access(user_id, object, access)
    }

    fn security_field_access(
        &self,
        user_id: &str,
        object: &str,
        field: &str,
        access: AccessType,
    ) -> Result<bool, SecurityError> {
        (**self).security_field_access(user_id, object, field, access)
    }

    fn send_http(
        &mut self,
        request: &HttpRequestData,
        profile: CompatibilityProfile,
    ) -> Result<HttpResponseData, String> {
        (**self).send_http(request, profile)
    }

    fn limit_usage(&self) -> LimitUsage {
        (**self).limit_usage()
    }

    fn begin_test_window(&mut self) {
        (**self).begin_test_window();
    }

    fn end_test_window(&mut self) {
        (**self).end_test_window();
    }
}

/// Default host used by the public convenience APIs.
pub struct RecordingHost {
    output: Vec<String>,
    database: Option<LocalDatabase>,
    queries: Vec<QueryEvent>,
    query_statements: usize,
    dml: Vec<DmlEvent>,
    dml_statements: usize,
    triggers: Vec<TriggerEvent>,
    async_events: Vec<AsyncEvent>,
    timeline: Vec<TransactionEvent>,
    checkpoints: Vec<DatabaseSnapshot>,
    now_millis: i64,
    random_state: u64,
    user: UserContext,
    security: SecurityPolicy,
    database_fixtures: Vec<crate::platform::Record>,
    http_responses: VecDeque<HttpResponseData>,
    callout_requests: Vec<HttpRequestData>,
    callouts: i64,
    dml_retry_limit_baselines: Vec<(usize, i64)>,
    test_window_baseline: Option<LimitUsage>,
}

impl RecordingHost {
    pub fn set_now_millis(&mut self, now_millis: i64) {
        self.now_millis = now_millis;
    }

    pub fn set_user_context(&mut self, user: UserContext) {
        self.user = user;
    }

    pub fn set_security_policy(&mut self, security: SecurityPolicy) {
        self.security = security;
    }

    pub fn security_policy(&self) -> &SecurityPolicy {
        &self.security
    }

    pub fn set_database_fixtures(&mut self, records: Vec<crate::platform::Record>) {
        self.database_fixtures = records;
        self.database = None;
    }

    pub fn enqueue_http_response(&mut self, response: HttpResponseData) {
        self.http_responses.push_back(response);
    }

    pub fn callout_requests(&self) -> &[HttpRequestData] {
        &self.callout_requests
    }

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

    pub fn async_events(&self) -> &[AsyncEvent] {
        &self.async_events
    }

    fn database(&mut self, schema: &SchemaCatalog) -> Result<&mut LocalDatabase, DatabaseError> {
        if self.database.is_none() {
            let mut database = LocalDatabase::new(schema.clone())?;
            if !self.database_fixtures.is_empty() {
                database.load_fixture(self.database_fixtures.clone())?;
            }
            self.database = Some(database);
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

impl Default for RecordingHost {
    fn default() -> Self {
        Self {
            output: Vec::new(),
            database: None,
            queries: Vec::new(),
            query_statements: 0,
            dml: Vec::new(),
            dml_statements: 0,
            triggers: Vec::new(),
            async_events: Vec::new(),
            timeline: Vec::new(),
            checkpoints: Vec::new(),
            now_millis: 1_735_689_600_000,
            random_state: 0x4d59_5df4_d0f3_3173,
            user: UserContext::default(),
            security: SecurityPolicy::default(),
            database_fixtures: Vec::new(),
            http_responses: VecDeque::new(),
            callout_requests: Vec::new(),
            callouts: 0,
            dml_retry_limit_baselines: Vec::new(),
            test_window_baseline: None,
        }
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
        self.query_statements += 1;
        let security = self.security.clone();
        let database = self.database(schema)?;
        let secured = secure_soql_request(&security, schema, database, request)?;
        let result = database.execute_soql(&secured);
        let object_scans = database.last_query_object_scans();
        let rows = result.as_ref().map_or(0, outcome_rows);
        self.queries.push(QueryEvent {
            kind: QueryKind::Soql,
            objects: vec![request.object.clone()],
            rows,
            object_scans,
            succeeded: result.is_ok(),
        });
        result
    }

    fn sosl(
        &mut self,
        schema: &SchemaCatalog,
        request: &SoslRequest,
    ) -> Result<QueryOutcome, DatabaseError> {
        self.query_statements += 1;
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
            object_scans: request.returning.len(),
            succeeded: result.is_ok(),
        });
        result
    }

    fn dml(
        &mut self,
        schema: &SchemaCatalog,
        operation: DmlOperation,
        mut rows: Vec<DmlRow>,
    ) -> Result<Vec<DmlRowOutcome>, DatabaseError> {
        for row in &mut rows {
            let record = &mut row.record;
            if operation == DmlOperation::Insert
                && schema
                    .field(record.object_api_name(), "CreatedDate")
                    .is_ok()
            {
                record
                    .set(
                        schema,
                        "CreatedDate",
                        crate::platform::DataValue::Datetime(self.now_millis),
                    )
                    .map_err(|error| DatabaseError::new(error.to_string()))?;
            }
            if matches!(
                operation,
                DmlOperation::Insert | DmlOperation::Update | DmlOperation::Upsert
            ) && schema
                .field(record.object_api_name(), "LastModifiedDate")
                .is_ok()
            {
                record
                    .set(
                        schema,
                        "LastModifiedDate",
                        crate::platform::DataValue::Datetime(self.now_millis),
                    )
                    .map_err(|error| DatabaseError::new(error.to_string()))?;
            }
        }
        let objects = rows
            .iter()
            .map(|row| row.record.object_api_name().to_owned())
            .collect::<Vec<_>>();
        let records = rows.len();
        let result = self.database(schema)?.execute_dml(operation, rows);
        let successful_records = result.as_ref().map_or(0, |outcomes| {
            outcomes
                .iter()
                .filter(|outcome| outcome.is_success())
                .count()
        });
        let event = DmlEvent {
            operation,
            objects,
            records,
            successful_records,
            failed_records: records - successful_records,
            succeeded: result
                .as_ref()
                .is_ok_and(|outcomes| outcomes.iter().all(DmlRowOutcome::is_success)),
        };
        self.dml.push(event.clone());
        self.timeline.push(TransactionEvent::Dml(event));
        result
    }

    fn prepare_dml(
        &mut self,
        schema: &SchemaCatalog,
        request: &DmlRequest,
    ) -> Result<Vec<PreparedDmlOutcome>, DatabaseError> {
        let security = self.security.clone();
        let database = self.database(schema)?;
        let (request, mut denied) = secure_dml_request(&security, schema, database, request)?;
        let mut prepared = database.prepare_dml(&request)?;
        prepared.append(&mut denied);
        prepared.sort_by_key(PreparedDmlOutcome::input_index);
        Ok(prepared)
    }

    fn record_dml(&mut self, _event: DmlEvent) {
        self.dml_statements += 1;
    }

    fn begin_dml_retry_scope(&mut self) {
        self.dml_retry_limit_baselines
            .push((self.query_statements, self.callouts));
    }

    fn reset_dml_retry_limits(&mut self) {
        if let Some((queries, callouts)) = self.dml_retry_limit_baselines.last().copied() {
            self.query_statements = queries;
            self.callouts = callouts;
        }
    }

    fn end_dml_retry_scope(&mut self) {
        self.dml_retry_limit_baselines
            .pop()
            .expect("DML retry scopes are balanced");
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

    fn async_event(&mut self, event: AsyncEvent) {
        self.async_events.push(event);
    }

    fn transaction_event_count(&self) -> usize {
        self.timeline.len()
    }

    fn now_millis(&mut self) -> i64 {
        self.now_millis
    }

    fn random_u64(&mut self) -> u64 {
        // xorshift64*: stable and intentionally not cryptographic.
        let mut state = self.random_state;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.random_state = state;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn user_context(&self) -> UserContext {
        self.user.clone()
    }

    fn security_object_access(
        &self,
        user_id: &str,
        object: &str,
        access: AccessType,
    ) -> Result<bool, SecurityError> {
        Ok(self
            .security
            .object_permissions(user_id, object)?
            .permits(access))
    }

    fn security_field_access(
        &self,
        user_id: &str,
        object: &str,
        field: &str,
        access: AccessType,
    ) -> Result<bool, SecurityError> {
        Ok(self
            .security
            .field_permissions(user_id, object, field)?
            .permits(access))
    }

    fn send_http(
        &mut self,
        request: &HttpRequestData,
        profile: CompatibilityProfile,
    ) -> Result<HttpResponseData, String> {
        self.callouts += 1;
        self.callout_requests.push(request.clone());
        self.http_responses.pop_front().ok_or_else(|| {
            format!(
                "HTTP callout has no configured mock in compatibility profile `{}`",
                profile.identity()
            )
        })
    }

    fn limit_usage(&self) -> LimitUsage {
        let absolute = LimitUsage {
            queries: i64::try_from(self.query_statements).unwrap_or(i64::MAX),
            dml_statements: i64::try_from(self.dml_statements).unwrap_or(i64::MAX),
            callouts: self.callouts,
        };
        let baseline = self.test_window_baseline.unwrap_or_default();
        LimitUsage {
            queries: absolute.queries - baseline.queries,
            dml_statements: absolute.dml_statements - baseline.dml_statements,
            callouts: absolute.callouts - baseline.callouts,
        }
    }

    fn begin_test_window(&mut self) {
        self.test_window_baseline = Some(LimitUsage {
            queries: i64::try_from(self.query_statements).unwrap_or(i64::MAX),
            dml_statements: i64::try_from(self.dml_statements).unwrap_or(i64::MAX),
            callouts: self.callouts,
        });
    }

    fn end_test_window(&mut self) {
        self.test_window_baseline = None;
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
