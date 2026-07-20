use crate::platform::{
    DataValue, FieldPermissions, ObjectPermissions, Record, RecordAccess, RecordGrant, RecordId,
    SecurityGroup, SecurityPolicy, SecurityPrincipal, SecurityUser,
};
use serde::Deserialize;
use std::{collections::BTreeSet, fs, path::Path};

const SECURITY_FIXTURE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecurityFixtureFile {
    schema_version: u32,
    #[serde(default)]
    users: Vec<UserFixture>,
    #[serde(default)]
    role_parents: Vec<RoleParentFixture>,
    #[serde(default)]
    groups: Vec<GroupFixture>,
    #[serde(default)]
    object_permissions: Vec<ObjectPermissionFixture>,
    #[serde(default)]
    field_permissions: Vec<FieldPermissionFixture>,
    #[serde(default)]
    record_grants: Vec<RecordGrantFixture>,
    #[serde(default)]
    records: Vec<RecordFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UserFixture {
    id: String,
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoleParentFixture {
    role: String,
    parent: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GroupFixture {
    id: String,
    #[serde(default)]
    members: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObjectPermissionFixture {
    principal: String,
    object: String,
    #[serde(default)]
    readable: bool,
    #[serde(default)]
    creatable: bool,
    #[serde(default)]
    updatable: bool,
    #[serde(default)]
    deletable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FieldPermissionFixture {
    principal: String,
    object: String,
    field: String,
    #[serde(default)]
    readable: bool,
    #[serde(default)]
    creatable: bool,
    #[serde(default)]
    updatable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordGrantFixture {
    object: String,
    record_id: String,
    principal: PrincipalFixture,
    access: AccessFixture,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordFixture {
    object: String,
    id: String,
    #[serde(default)]
    fields: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
enum PrincipalFixture {
    User(String),
    Role(String),
    Group(String),
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum AccessFixture {
    Read,
    Edit,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct LoadedSecurityFixture {
    pub policy: SecurityPolicy,
    pub records: Vec<Record>,
}

pub(super) fn load(root: &Path) -> Result<LoadedSecurityFixture, String> {
    let path = root.join(".apex-exec/security.json");
    if !path.exists() {
        return Ok(LoadedSecurityFixture::default());
    }
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "could not read security fixture `{}`: {error}",
            path.display()
        )
    })?;
    let fixture: SecurityFixtureFile = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid security fixture `{}`: {error}", path.display()))?;
    if fixture.schema_version != SECURITY_FIXTURE_SCHEMA_VERSION {
        return Err(format!(
            "security fixture `{}` uses schema version {}; expected {}",
            path.display(),
            fixture.schema_version,
            SECURITY_FIXTURE_SCHEMA_VERSION
        ));
    }
    materialize(fixture)
}

fn materialize(fixture: SecurityFixtureFile) -> Result<LoadedSecurityFixture, String> {
    let mut policy = SecurityPolicy::new();
    for user in fixture.users {
        let mut value = SecurityUser::new(user.id);
        if let Some(role) = user.role {
            value = value.with_role(role);
        }
        policy.add_user(value);
    }
    for relation in fixture.role_parents {
        policy.set_role_parent(relation.role, relation.parent);
    }
    for group in fixture.groups {
        let mut value = SecurityGroup::new(group.id);
        for member in group.members {
            value.members.insert(member.to_ascii_lowercase());
        }
        policy.add_group(value);
    }
    for permission in fixture.object_permissions {
        policy.set_object_permissions(
            permission.principal,
            permission.object,
            ObjectPermissions {
                readable: permission.readable,
                creatable: permission.creatable,
                updatable: permission.updatable,
                deletable: permission.deletable,
            },
        );
    }
    for permission in fixture.field_permissions {
        policy.set_field_permissions(
            permission.principal,
            permission.object,
            permission.field,
            FieldPermissions {
                readable: permission.readable,
                creatable: permission.creatable,
                updatable: permission.updatable,
            },
        );
    }
    materialize_grants(&mut policy, fixture.record_grants)?;
    let records = materialize_records(fixture.records)?;
    Ok(LoadedSecurityFixture { policy, records })
}

fn materialize_grants(
    policy: &mut SecurityPolicy,
    grants: Vec<RecordGrantFixture>,
) -> Result<(), String> {
    for grant in grants {
        policy.grant_record(RecordGrant {
            object: grant.object,
            record_id: RecordId::parse(grant.record_id)
                .map_err(|error| format!("invalid record grant ID: {error}"))?,
            grantee: match grant.principal {
                PrincipalFixture::User(id) => SecurityPrincipal::User(id),
                PrincipalFixture::Role(id) => SecurityPrincipal::Role(id),
                PrincipalFixture::Group(id) => SecurityPrincipal::Group(id),
            },
            access: match grant.access {
                AccessFixture::Read => RecordAccess::Read,
                AccessFixture::Edit => RecordAccess::Edit,
            },
        });
    }
    Ok(())
}

fn materialize_records(sources: Vec<RecordFixture>) -> Result<Vec<Record>, String> {
    let mut records = Vec::with_capacity(sources.len());
    for source in sources {
        let mut record = Record::new(
            source.object,
            RecordId::parse(source.id)
                .map_err(|error| format!("invalid fixture record ID: {error}"))?,
        );
        for (field, value) in source.fields {
            record.set_field(field, fixture_value(value)?);
        }
        records.push(record);
    }
    Ok(records)
}

fn fixture_value(value: serde_json::Value) -> Result<DataValue, String> {
    match value {
        serde_json::Value::Null => Ok(DataValue::Null),
        serde_json::Value::Bool(value) => Ok(DataValue::Boolean(value)),
        serde_json::Value::Number(value) => value
            .as_i64()
            .map(DataValue::Integer)
            .ok_or_else(|| "security fixture numbers must be signed 64-bit integers".to_owned()),
        serde_json::Value::String(value) => Ok(DataValue::String(value)),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => Err(
            "security fixture record values support only null, Boolean, Integer, and String"
                .to_owned(),
        ),
    }
}
