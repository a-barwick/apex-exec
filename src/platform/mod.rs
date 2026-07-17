//! Platform-owned schema and persistence boundaries.
//!
//! Schema normalization is intentionally independent from record persistence:
//! compiler and runtime services can inspect [`SchemaCatalog`] through
//! [`SchemaProvider`] without knowing which [`Storage`] implementation owns
//! record data.
//!
//! This module is an M7 architectural foundation. The contracts are not yet
//! populated from SFDX metadata or wired into Apex type checking and execution,
//! and the crate does not yet provide a SQLite adapter.

pub mod schema;
pub mod storage;

pub use schema::{
    FieldSchema, FieldType, ObjectSchema, SchemaCatalog, SchemaError, SchemaProvider,
};
pub use storage::{DataValue, Record, RecordId, Storage, StorageTransaction};
