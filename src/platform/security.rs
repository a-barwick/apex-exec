use super::{DataValue, ObjectSchema, Record, RecordId, SharingModel};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SharingMode {
    WithSharing,
    #[default]
    WithoutSharing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccessLevel {
    UserMode,
    #[default]
    SystemMode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QueryAccessMode {
    #[default]
    Default,
    SecurityEnforced,
    UserMode,
    SystemMode,
}

impl AccessLevel {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "USER_MODE" => Some(Self::UserMode),
            "SYSTEM_MODE" => Some(Self::SystemMode),
            _ => None,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::UserMode => "USER_MODE",
            Self::SystemMode => "SYSTEM_MODE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessType {
    Readable,
    Creatable,
    Updatable,
}

impl AccessType {
    pub fn from_apex_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "READABLE" => Some(Self::Readable),
            "CREATABLE" => Some(Self::Creatable),
            "UPDATABLE" => Some(Self::Updatable),
            _ => None,
        }
    }

    pub fn apex_name(self) -> &'static str {
        match self {
            Self::Readable => "READABLE",
            Self::Creatable => "CREATABLE",
            Self::Updatable => "UPDATABLE",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjectPermissions {
    pub readable: bool,
    pub creatable: bool,
    pub updatable: bool,
    pub deletable: bool,
}

impl ObjectPermissions {
    pub const fn all() -> Self {
        Self {
            readable: true,
            creatable: true,
            updatable: true,
            deletable: true,
        }
    }

    pub const fn permits(self, access: AccessType) -> bool {
        match access {
            AccessType::Readable => self.readable,
            AccessType::Creatable => self.creatable,
            AccessType::Updatable => self.updatable,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FieldPermissions {
    pub readable: bool,
    pub creatable: bool,
    pub updatable: bool,
}

impl FieldPermissions {
    pub const fn all() -> Self {
        Self {
            readable: true,
            creatable: true,
            updatable: true,
        }
    }

    pub const fn permits(self, access: AccessType) -> bool {
        match access {
            AccessType::Readable => self.readable,
            AccessType::Creatable => self.creatable,
            AccessType::Updatable => self.updatable,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecurityUser {
    pub user_id: String,
    pub role_id: Option<String>,
}

impl SecurityUser {
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            role_id: None,
        }
    }

    pub fn with_role(mut self, role_id: impl Into<String>) -> Self {
        self.role_id = Some(role_id.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecurityGroup {
    pub group_id: String,
    pub members: BTreeSet<String>,
}

impl SecurityGroup {
    pub fn new(group_id: impl Into<String>) -> Self {
        Self {
            group_id: group_id.into(),
            members: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecurityPrincipal {
    User(String),
    Role(String),
    Group(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecordAccess {
    Read,
    Edit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordGrant {
    pub object: String,
    pub record_id: RecordId,
    pub grantee: SecurityPrincipal,
    pub access: RecordAccess,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SecurityPolicy {
    configured: bool,
    users: BTreeMap<String, SecurityUser>,
    role_parents: BTreeMap<String, String>,
    groups: BTreeMap<String, SecurityGroup>,
    object_permissions: BTreeMap<(String, String), ObjectPermissions>,
    field_permissions: BTreeMap<(String, String, String), FieldPermissions>,
    grants: Vec<RecordGrant>,
}

impl SecurityPolicy {
    pub fn new() -> Self {
        Self {
            configured: true,
            ..Self::default()
        }
    }

    pub fn is_configured(&self) -> bool {
        self.configured
    }

    pub fn add_user(&mut self, user: SecurityUser) {
        self.users.insert(canonical(&user.user_id), user);
    }

    pub fn set_role_parent(
        &mut self,
        role_id: impl Into<String>,
        parent_role_id: impl Into<String>,
    ) {
        let role_id = role_id.into();
        let parent_role_id = parent_role_id.into();
        self.role_parents
            .insert(canonical(&role_id), canonical(&parent_role_id));
    }

    pub fn add_group(&mut self, group: SecurityGroup) {
        self.groups.insert(canonical(&group.group_id), group);
    }

    pub fn add_group_member(
        &mut self,
        group_id: impl AsRef<str>,
        user_id: impl AsRef<str>,
    ) -> Result<(), SecurityError> {
        let group_key = canonical(group_id.as_ref());
        let group = self
            .groups
            .get_mut(&group_key)
            .ok_or_else(|| SecurityError::UnknownGroup(group_id.as_ref().to_owned()))?;
        group.members.insert(canonical(user_id.as_ref()));
        Ok(())
    }

    pub fn set_object_permissions(
        &mut self,
        user_id: impl AsRef<str>,
        object: impl AsRef<str>,
        permissions: ObjectPermissions,
    ) {
        self.object_permissions.insert(
            (canonical(user_id.as_ref()), canonical(object.as_ref())),
            permissions,
        );
    }

    pub fn set_field_permissions(
        &mut self,
        user_id: impl AsRef<str>,
        object: impl AsRef<str>,
        field: impl AsRef<str>,
        permissions: FieldPermissions,
    ) {
        self.field_permissions.insert(
            (
                canonical(user_id.as_ref()),
                canonical(object.as_ref()),
                canonical(field.as_ref()),
            ),
            permissions,
        );
    }

    pub fn grant_record(&mut self, grant: RecordGrant) {
        self.grants.push(grant);
    }

    pub fn object_permissions(
        &self,
        user_id: &str,
        object: &str,
    ) -> Result<ObjectPermissions, SecurityError> {
        self.require_configured()?;
        Ok(self
            .object_permissions
            .get(&(canonical(user_id), canonical(object)))
            .or_else(|| {
                self.object_permissions
                    .get(&(canonical("*"), canonical(object)))
            })
            .copied()
            .unwrap_or_default())
    }

    pub fn field_permissions(
        &self,
        user_id: &str,
        object: &str,
        field: &str,
    ) -> Result<FieldPermissions, SecurityError> {
        self.require_configured()?;
        Ok(self
            .field_permissions
            .get(&(canonical(user_id), canonical(object), canonical(field)))
            .or_else(|| {
                self.field_permissions
                    .get(&(canonical("*"), canonical(object), canonical(field)))
            })
            .copied()
            .unwrap_or_default())
    }

    pub fn can_access_record(
        &self,
        object: &ObjectSchema,
        record: &Record,
        user_id: &str,
        required: RecordAccess,
    ) -> Result<bool, SecurityError> {
        match object.sharing_model() {
            SharingModel::PublicReadWrite => return Ok(true),
            SharingModel::PublicReadOnly if required == RecordAccess::Read => return Ok(true),
            SharingModel::ControlledByParent => {
                return Err(SecurityError::Unsupported(format!(
                    "controlled-by-parent visibility for `{}`",
                    object.api_name()
                )));
            }
            SharingModel::Private | SharingModel::PublicReadOnly => {}
        }

        let owner_id = record.field("OwnerId").and_then(data_id);
        if owner_id.is_some_and(|owner| owner.eq_ignore_ascii_case(user_id)) {
            return Ok(true);
        }
        if !self.configured {
            return Ok(false);
        }
        if let Some(owner) = owner_id
            && self.user_has_role_visibility(user_id, owner)?
        {
            return Ok(true);
        }
        Ok(self.grants.iter().any(|grant| {
            grant.object.eq_ignore_ascii_case(object.api_name())
                && grant.record_id == *record.id()
                && grant.access >= required
                && self.principal_includes(&grant.grantee, user_id)
        }))
    }

    fn require_configured(&self) -> Result<(), SecurityError> {
        if self.configured {
            Ok(())
        } else {
            Err(SecurityError::Unavailable)
        }
    }

    fn user_has_role_visibility(
        &self,
        viewer_id: &str,
        owner_id: &str,
    ) -> Result<bool, SecurityError> {
        self.role_visibility_walk(viewer_id, owner_id)
            .map(|(visible, _)| visible)
    }

    fn role_visibility_walk(
        &self,
        viewer_id: &str,
        owner_id: &str,
    ) -> Result<(bool, usize), SecurityError> {
        self.require_configured()?;
        let Some(viewer) = self.users.get(&canonical(viewer_id)) else {
            return Ok((false, 0));
        };
        let Some(owner) = self.users.get(&canonical(owner_id)) else {
            return Ok((false, 0));
        };
        let (Some(viewer_role), Some(owner_role)) = (&viewer.role_id, &owner.role_id) else {
            return Ok((false, 0));
        };
        let viewer_role = canonical(viewer_role);
        let mut cursor = canonical(owner_role);
        let mut visited = BTreeSet::new();
        while visited.insert(cursor.clone()) {
            let Some(parent) = self.role_parents.get(&cursor) else {
                return Ok((false, visited.len()));
            };
            if parent == &viewer_role {
                return Ok((true, visited.len()));
            }
            cursor = parent.clone();
        }
        Err(SecurityError::Unsupported(
            "cyclic role hierarchy".to_owned(),
        ))
    }

    fn principal_includes(&self, principal: &SecurityPrincipal, user_id: &str) -> bool {
        match principal {
            SecurityPrincipal::User(grantee) => grantee.eq_ignore_ascii_case(user_id),
            SecurityPrincipal::Role(role) => self
                .users
                .get(&canonical(user_id))
                .and_then(|user| user.role_id.as_deref())
                .is_some_and(|user_role| user_role.eq_ignore_ascii_case(role)),
            SecurityPrincipal::Group(group) => self
                .groups
                .get(&canonical(group))
                .is_some_and(|group| group.members.contains(&canonical(user_id))),
        }
    }
}

fn data_id(value: &DataValue) -> Option<&str> {
    match value {
        DataValue::Id(value) => Some(value.as_str()),
        DataValue::String(value) => Some(value),
        _ => None,
    }
}

fn canonical(value: &str) -> String {
    value.to_ascii_lowercase()
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SecurityError {
    Unavailable,
    UnknownGroup(String),
    Unsupported(String),
    AccessDenied(String),
}

impl fmt::Display for SecurityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str(
                "security policy is not configured; user-mode access cannot run as system mode",
            ),
            Self::UnknownGroup(group) => write!(formatter, "unknown security group `{group}`"),
            Self::Unsupported(capability) => {
                write!(formatter, "unsupported security behavior: {capability}")
            }
            Self::AccessDenied(message) => formatter.write_str(message),
        }
    }
}

impl Error for SecurityError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{FieldSchema, FieldType, ObjectSchema};

    fn private_object() -> ObjectSchema {
        let mut object = ObjectSchema::new("Case__c").with_sharing_model(SharingModel::Private);
        object
            .insert_field(FieldSchema::new("OwnerId", FieldType::Id, true))
            .unwrap();
        object
    }

    fn record(owner: &str) -> Record {
        let mut record = Record::new("Case__c", RecordId::new("a00000000000001AAA"));
        record.set_field("OwnerId", owner);
        record
    }

    #[test]
    fn owner_role_group_and_explicit_share_visibility_are_deterministic() {
        let object = private_object();
        let record = record("005OWNER0000001AAA");
        let mut policy = SecurityPolicy::new();
        policy.add_user(SecurityUser::new("005OWNER0000001AAA").with_role("00RCHILD"));
        policy.add_user(SecurityUser::new("005MANAGER000001AAA").with_role("00RPARENT"));
        policy.add_user(SecurityUser::new("005GROUP0000001AAA"));
        policy.set_role_parent("00RCHILD", "00RPARENT");
        let mut group = SecurityGroup::new("00GGROUP");
        group.members.insert(canonical("005GROUP0000001AAA"));
        policy.add_group(group);

        assert!(
            policy
                .can_access_record(&object, &record, "005OWNER0000001AAA", RecordAccess::Edit)
                .unwrap()
        );
        assert!(
            policy
                .can_access_record(&object, &record, "005MANAGER000001AAA", RecordAccess::Read)
                .unwrap()
        );
        assert!(
            !policy
                .can_access_record(&object, &record, "005GROUP0000001AAA", RecordAccess::Read)
                .unwrap()
        );

        policy.grant_record(RecordGrant {
            object: "Case__c".to_owned(),
            record_id: record.id().clone(),
            grantee: SecurityPrincipal::Group("00GGROUP".to_owned()),
            access: RecordAccess::Read,
        });
        assert!(
            policy
                .can_access_record(&object, &record, "005GROUP0000001AAA", RecordAccess::Read)
                .unwrap()
        );
        assert!(
            !policy
                .can_access_record(&object, &record, "005GROUP0000001AAA", RecordAccess::Edit)
                .unwrap()
        );
    }

    #[test]
    fn unconfigured_permissions_fail_explicitly() {
        assert_eq!(
            SecurityPolicy::default()
                .object_permissions("005USER", "Case__c")
                .unwrap_err(),
            SecurityError::Unavailable
        );
    }

    #[test]
    fn owd_read_only_private_and_controlled_by_parent_are_distinct() {
        let owner = "005OWNER0000001AAA";
        let viewer = "005VIEWER0000001AAA";
        let record = record(owner);
        let policy = SecurityPolicy::new();

        let read_only =
            ObjectSchema::new("Case__c").with_sharing_model(SharingModel::PublicReadOnly);
        assert!(
            policy
                .can_access_record(&read_only, &record, viewer, RecordAccess::Read)
                .unwrap()
        );
        assert!(
            !policy
                .can_access_record(&read_only, &record, viewer, RecordAccess::Edit)
                .unwrap()
        );

        assert!(
            !policy
                .can_access_record(&private_object(), &record, viewer, RecordAccess::Read)
                .unwrap()
        );

        let controlled =
            ObjectSchema::new("Case__c").with_sharing_model(SharingModel::ControlledByParent);
        assert!(matches!(
            policy
                .can_access_record(&controlled, &record, viewer, RecordAccess::Read)
                .unwrap_err(),
            SecurityError::Unsupported(message) if message.contains("controlled-by-parent")
        ));
    }

    #[test]
    fn crud_and_fls_default_deny_and_match_each_access_type() {
        let user = "005USER00000001AAA";
        let mut policy = SecurityPolicy::new();
        policy.add_user(SecurityUser::new(user));
        policy.set_object_permissions(
            user,
            "Case__c",
            ObjectPermissions {
                readable: true,
                creatable: false,
                updatable: true,
                deletable: false,
            },
        );
        policy.set_field_permissions(
            user,
            "Case__c",
            "Subject__c",
            FieldPermissions {
                readable: false,
                creatable: true,
                updatable: false,
            },
        );

        let object = policy.object_permissions(user, "case__C").unwrap();
        assert!(object.permits(AccessType::Readable));
        assert!(!object.permits(AccessType::Creatable));
        assert!(object.permits(AccessType::Updatable));
        assert!(!object.deletable);

        let field = policy
            .field_permissions(user, "CASE__c", "subject__C")
            .unwrap();
        assert!(!field.permits(AccessType::Readable));
        assert!(field.permits(AccessType::Creatable));
        assert!(!field.permits(AccessType::Updatable));
        assert_eq!(
            policy
                .field_permissions(user, "Case__c", "Unconfigured__c")
                .unwrap(),
            FieldPermissions::default()
        );
    }

    #[test]
    fn role_visibility_cost_is_bounded_by_the_unique_hierarchy_path() {
        let mut policy = SecurityPolicy::new();
        policy.add_user(SecurityUser::new("OWNER").with_role("ROLE-0"));
        policy.add_user(SecurityUser::new("VIEWER").with_role("ROLE-64"));
        for index in 0..64 {
            policy.set_role_parent(format!("ROLE-{index}"), format!("ROLE-{}", index + 1));
        }

        assert_eq!(
            policy.role_visibility_walk("VIEWER", "OWNER").unwrap(),
            (true, 64)
        );

        policy.set_role_parent("ROLE-63", "ROLE-0");
        assert!(matches!(
            policy.role_visibility_walk("VIEWER", "OWNER"),
            Err(SecurityError::Unsupported(message)) if message == "cyclic role hierarchy"
        ));
    }
}
