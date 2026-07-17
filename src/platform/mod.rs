//! Platform-owned schema and persistence boundaries.
//!
//! Schema normalization is intentionally independent from record persistence:
//! compiler and runtime services can inspect [`SchemaCatalog`] through
//! [`SchemaProvider`] without knowing which [`Storage`] implementation owns
//! record data.

pub mod schema;
pub mod storage;

pub use schema::{
    FieldSchema, FieldType, ObjectSchema, SchemaCatalog, SchemaError, SchemaProvider,
};
pub use storage::{DataValue, Record, RecordId, Storage, StorageTransaction};
