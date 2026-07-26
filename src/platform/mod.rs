//! Platform-owned schema and persistence boundaries.
//!
//! Schema normalization is intentionally independent from record persistence:
//! compiler and runtime services can inspect [`SchemaCatalog`] through
//! [`SchemaProvider`] without knowing which [`Storage`] implementation owns
//! record data.
//!
//! SFDX metadata is imported into the normalized catalog, while SObject values
//! and the SQLite adapter consume the contracts without leaking database types
//! into compiler-facing schema APIs.

mod cache;
mod context;
mod database;
mod describe;
mod logging;
pub mod metadata;
pub mod schema;
pub mod security;
pub mod sobject;
pub mod sqlite;
mod standard_schema;
pub mod storage;

pub use cache::CacheVisibility;
pub use context::{
    ParentJobResult, PlatformEnum, PlatformEnumDescriptor, Quiddity, TriggerOperation,
};
pub use database::{
    AggregateFunction, DatabaseError, DatabaseSnapshot, DmlError, DmlExternalId, DmlOperation,
    DmlRequest, DmlRow, DmlRowOutcome, DmlStatus, LocalDatabase, NullOrder, PreparedDmlOutcome,
    PreparedDmlRecord, QueryComparison, QueryCondition, QueryDateLiteral, QueryDateLiteralKind,
    QueryField, QueryInValues, QueryLogical, QueryOrder, QueryOutcome, QueryRecord,
    QueryRelationship, QuerySelect, QueryValue, SoqlRequest, SortOrder, SoslRequest,
    SoslReturningRequest,
};
pub use describe::{DisplayType, SoapType};
pub use logging::LoggingLevel;
pub use metadata::{MetadataError, import_metadata};
pub use schema::{
    FieldSchema, FieldSetSchema, FieldType, ObjectSchema, SchemaCatalog, SchemaError,
    SchemaProvider, SharingModel, SummaryDefinition, SummaryFilter, SummaryFilterOperator,
    SummaryOperation,
};
pub use security::{
    AccessLevel, AccessType, FieldPermissions, ObjectPermissions, QueryAccessMode, RecordAccess,
    RecordGrant, SecurityError, SecurityGroup, SecurityPolicy, SecurityPrincipal, SecurityUser,
    SharingMode,
};
pub use sobject::{SObject, SObjectError};
pub use sqlite::{SqliteError, SqliteStorage, SqliteStorageTransaction};
pub use storage::{DataValue, Record, RecordId, RecordIdError, Storage, StorageTransaction};
