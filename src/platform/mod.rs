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

mod database;
pub mod metadata;
pub mod schema;
pub mod sobject;
pub mod sqlite;
pub mod storage;

pub use database::{
    AggregateFunction, DatabaseError, DatabaseSnapshot, DmlError, DmlExternalId, DmlOperation,
    DmlRequest, DmlRow, DmlRowOutcome, DmlStatus, LocalDatabase, NullOrder, PreparedDmlOutcome,
    PreparedDmlRecord, QueryComparison, QueryCondition, QueryDateLiteral, QueryDateLiteralKind,
    QueryField, QueryInValues, QueryLogical, QueryOrder, QueryOutcome, QueryRecord,
    QueryRelationship, QuerySelect, QueryValue, SoqlRequest, SortOrder, SoslRequest,
    SoslReturningRequest,
};
pub use metadata::{MetadataError, import_metadata};
pub use schema::{
    FieldSchema, FieldType, ObjectSchema, SchemaCatalog, SchemaError, SchemaProvider,
};
pub use sobject::{SObject, SObjectError};
pub use sqlite::{SqliteError, SqliteStorage, SqliteStorageTransaction};
pub use storage::{DataValue, Record, RecordId, RecordIdError, Storage, StorageTransaction};
