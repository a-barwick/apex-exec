use crate::platform::{
    AccessLevel, AccessType, DataValue, DatabaseError, DmlError, DmlOperation, DmlRequest, DmlRow,
    DmlStatus, LocalDatabase, PreparedDmlOutcome, QueryAccessMode, QueryCondition, QueryField,
    QuerySelect, RecordAccess, SchemaCatalog, SecurityError, SecurityPolicy, SharingMode,
    SoqlRequest,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
struct DmlEnforcement {
    permissions: bool,
    sharing: bool,
    owner_was_defaulted: bool,
}

pub(super) fn secure_soql_request(
    policy: &SecurityPolicy,
    schema: &SchemaCatalog,
    database: &mut LocalDatabase,
    request: &SoqlRequest,
) -> Result<SoqlRequest, DatabaseError> {
    let mut secured = request.clone();
    let enforce_permissions = matches!(
        request.access,
        QueryAccessMode::UserMode | QueryAccessMode::SecurityEnforced
    );
    if enforce_permissions {
        authorize_query_permissions(policy, schema, request)
            .map_err(|error| DatabaseError::new(error.to_string()))?;
    }
    let enforce_sharing = request.access == QueryAccessMode::UserMode
        || (request.access != QueryAccessMode::SystemMode
            && request.sharing == SharingMode::WithSharing);
    if enforce_sharing {
        let object = schema
            .object(&request.object)
            .map_err(|error| DatabaseError::new(error.to_string()))?;
        let mut visible = BTreeSet::new();
        for record in database.records_for_security(&request.object)? {
            if policy
                .can_access_record(object, &record, &request.user_id, RecordAccess::Read)
                .map_err(|error| DatabaseError::new(error.to_string()))?
            {
                visible.insert(record.id().clone());
            }
        }
        secured.visible_record_ids = Some(visible);
    }
    for select in &mut secured.select {
        if let QuerySelect::Subquery { query, .. } = select {
            **query = secure_soql_request(policy, schema, database, query)?;
        }
    }
    Ok(secured)
}

pub(super) fn secure_dml_request(
    policy: &SecurityPolicy,
    schema: &SchemaCatalog,
    database: &mut LocalDatabase,
    request: &DmlRequest,
) -> Result<(DmlRequest, Vec<PreparedDmlOutcome>), DatabaseError> {
    if request.access == AccessLevel::UserMode && request.external_id.is_some() {
        return Err(DatabaseError::new(
            "unsupported security behavior: user-mode external-ID upsert",
        ));
    }
    let mut secured = request.clone();
    let mut defaulted_owner_rows = BTreeSet::new();
    for row in &mut secured.rows {
        let can_default_owner = matches!(request.operation, DmlOperation::Insert)
            || (request.operation == DmlOperation::Upsert && row.record.id().is_none());
        if can_default_owner
            && schema
                .field(row.record.object_api_name(), "OwnerId")
                .is_ok()
            && row
                .record
                .get(schema, "OwnerId")
                .map_err(|error| DatabaseError::new(error.to_string()))?
                .is_none()
        {
            row.record
                .set(
                    schema,
                    "OwnerId",
                    DataValue::String(request.user_id.clone()),
                )
                .map_err(|error| DatabaseError::new(error.to_string()))?;
            defaulted_owner_rows.insert(row.input_index);
        }
    }

    let enforce_permissions = request.access == AccessLevel::UserMode;
    let enforce_sharing = enforce_permissions || request.sharing == SharingMode::WithSharing;
    if !enforce_permissions && !enforce_sharing {
        return Ok((secured, Vec::new()));
    }

    let mut records = BTreeMap::new();
    for object in secured
        .rows
        .iter()
        .map(|row| row.record.object_api_name())
        .collect::<BTreeSet<_>>()
    {
        let scanned = database.records_for_security(object)?;
        records.insert(
            object.to_ascii_lowercase(),
            scanned
                .into_iter()
                .map(|record| (record.id().clone(), record))
                .collect::<BTreeMap<_, _>>(),
        );
    }

    let mut allowed = Vec::new();
    let mut denied = Vec::new();
    let rows = std::mem::take(&mut secured.rows);
    for row in rows {
        let result = authorize_dml_row(
            policy,
            schema,
            &records,
            &secured,
            &row,
            DmlEnforcement {
                permissions: enforce_permissions,
                sharing: enforce_sharing,
                owner_was_defaulted: defaulted_owner_rows.contains(&row.input_index),
            },
        );
        match result {
            Ok(()) => allowed.push(row),
            Err(error) => denied.push(PreparedDmlOutcome::Failed {
                input_index: row.input_index,
                errors: vec![DmlError::new(
                    DmlStatus::InsufficientAccessOrReadonly,
                    error.to_string(),
                    [],
                )],
            }),
        }
    }
    secured.rows = allowed;
    Ok((secured, denied))
}

fn authorize_dml_row(
    policy: &SecurityPolicy,
    schema: &SchemaCatalog,
    records: &BTreeMap<String, BTreeMap<crate::platform::RecordId, crate::platform::Record>>,
    request: &DmlRequest,
    row: &DmlRow,
    enforcement: DmlEnforcement,
) -> Result<(), SecurityError> {
    let object = schema
        .object(row.record.object_api_name())
        .map_err(|error| SecurityError::AccessDenied(error.to_string()))?;
    let existing = row.record.id().and_then(|id| {
        records
            .get(&object.api_name().to_ascii_lowercase())
            .and_then(|records| records.get(id))
    });
    let access = match request.operation {
        DmlOperation::Insert => AccessType::Creatable,
        DmlOperation::Upsert if existing.is_none() => AccessType::Creatable,
        DmlOperation::Update | DmlOperation::Upsert => AccessType::Updatable,
        DmlOperation::Delete | DmlOperation::Undelete => AccessType::Updatable,
    };
    if enforcement.permissions {
        let permissions = policy.object_permissions(&request.user_id, object.api_name())?;
        let object_allowed = match request.operation {
            DmlOperation::Delete | DmlOperation::Undelete => permissions.deletable,
            _ => permissions.permits(access),
        };
        if !object_allowed {
            return Err(SecurityError::AccessDenied(format!(
                "{} access denied for object `{}`",
                access.apex_name(),
                object.api_name()
            )));
        }
        if matches!(
            request.operation,
            DmlOperation::Insert | DmlOperation::Update | DmlOperation::Upsert
        ) {
            for (field, _) in row.record.fields() {
                if field.eq_ignore_ascii_case("Id")
                    || field.eq_ignore_ascii_case("CreatedDate")
                    || field.eq_ignore_ascii_case("LastModifiedDate")
                    || (enforcement.owner_was_defaulted && field.eq_ignore_ascii_case("OwnerId"))
                {
                    continue;
                }
                if !policy
                    .field_permissions(&request.user_id, object.api_name(), field)?
                    .permits(access)
                {
                    return Err(SecurityError::AccessDenied(format!(
                        "{} access denied for field `{}.{field}`",
                        access.apex_name(),
                        object.api_name()
                    )));
                }
            }
        }
    }
    if enforcement.sharing
        && let Some(existing) = existing
        && !policy.can_access_record(object, existing, &request.user_id, RecordAccess::Edit)?
    {
        return Err(SecurityError::AccessDenied(format!(
            "record access denied for `{}:{}`",
            object.api_name(),
            existing.id()
        )));
    }
    Ok(())
}

fn authorize_query_permissions(
    policy: &SecurityPolicy,
    schema: &SchemaCatalog,
    request: &SoqlRequest,
) -> Result<(), SecurityError> {
    let permissions = policy.object_permissions(&request.user_id, &request.object)?;
    if !permissions.readable {
        return Err(SecurityError::AccessDenied(format!(
            "read access denied for object `{}`",
            request.object
        )));
    }
    for select in &request.select {
        match select {
            QuerySelect::Field(field) => {
                authorize_query_field(policy, schema, request, field, AccessType::Readable)?;
            }
            QuerySelect::Aggregate {
                field: Some(field), ..
            } => {
                authorize_query_field(policy, schema, request, field, AccessType::Readable)?;
            }
            QuerySelect::Subquery { query, .. } => {
                authorize_query_permissions(policy, schema, query)?;
            }
            QuerySelect::Aggregate { field: None, .. } => {}
        }
    }
    if request.access == QueryAccessMode::UserMode {
        for field in query_support_fields(request) {
            authorize_query_field(policy, schema, request, field, AccessType::Readable)?;
        }
    }
    Ok(())
}

fn authorize_query_field(
    policy: &SecurityPolicy,
    _schema: &SchemaCatalog,
    request: &SoqlRequest,
    field: &QueryField,
    access: AccessType,
) -> Result<(), SecurityError> {
    let mut object = request.object.as_str();
    for relationship in &field.relationships {
        let reference =
            policy.field_permissions(&request.user_id, object, &relationship.reference_field)?;
        if !reference.permits(access) {
            return Err(SecurityError::AccessDenied(format!(
                "{} access denied for field `{}.{}`",
                access.apex_name(),
                object,
                relationship.reference_field
            )));
        }
        object = &relationship.target_object;
        if !policy
            .object_permissions(&request.user_id, object)?
            .permits(AccessType::Readable)
        {
            return Err(SecurityError::AccessDenied(format!(
                "read access denied for related object `{object}`"
            )));
        }
    }
    if !policy
        .field_permissions(&request.user_id, object, &field.field)?
        .permits(access)
    {
        return Err(SecurityError::AccessDenied(format!(
            "{} access denied for field `{}.{}`",
            access.apex_name(),
            object,
            field.field
        )));
    }
    Ok(())
}

fn query_support_fields(request: &SoqlRequest) -> Vec<&QueryField> {
    let mut fields = Vec::new();
    if let Some(condition) = &request.condition {
        collect_condition_fields(condition, &mut fields);
    }
    if let Some(condition) = &request.having {
        collect_condition_fields(condition, &mut fields);
    }
    fields.extend(request.group_by.iter());
    fields.extend(request.order_by.iter().map(|order| &order.field));
    fields
}

fn collect_condition_fields<'a>(condition: &'a QueryCondition, fields: &mut Vec<&'a QueryField>) {
    match condition {
        QueryCondition::Comparison { left, .. } => fields.push(left),
        QueryCondition::In { field, .. } => fields.push(field),
        QueryCondition::Not(condition) => collect_condition_fields(condition, fields),
        QueryCondition::Logical { left, right, .. } => {
            collect_condition_fields(left, fields);
            collect_condition_fields(right, fields);
        }
    }
}
