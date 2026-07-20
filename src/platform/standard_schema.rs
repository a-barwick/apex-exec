use super::{FieldSchema, FieldType, ObjectSchema};

#[derive(Clone, Copy)]
enum StandardField<'a> {
    Boolean(&'a str),
    Integer(&'a str),
    String(&'a str),
    Id(&'a str),
    Date(&'a str),
    Datetime(&'a str),
    Reference {
        name: &'a str,
        target: &'a str,
        relationship: &'a str,
    },
}

pub(super) fn standard_objects() -> Vec<ObjectSchema> {
    use StandardField as F;
    vec![
        object("Account", &[F::Id("Id"), F::String("Name")]),
        object("AggregateResult", &[F::Id("Id")]),
        object(
            "ApexClass",
            &[
                F::Id("Id"),
                F::String("Name"),
                F::String("NamespacePrefix"),
                F::String("Body"),
                F::Integer("ApiVersion"),
                F::Reference {
                    name: "CreatedById",
                    target: "User",
                    relationship: "CreatedBy",
                },
                F::Datetime("CreatedDate"),
                F::Reference {
                    name: "LastModifiedById",
                    target: "User",
                    relationship: "LastModifiedBy",
                },
                F::Datetime("LastModifiedDate"),
            ],
        ),
        object(
            "ApexEmailNotification",
            &[
                F::Id("Id"),
                F::String("Email"),
                F::Reference {
                    name: "UserId",
                    target: "User",
                    relationship: "User",
                },
            ],
        ),
        object(
            "ApexTrigger",
            &[
                F::Id("Id"),
                F::String("Name"),
                F::String("NamespacePrefix"),
                F::String("Body"),
                F::Integer("ApiVersion"),
                F::Reference {
                    name: "CreatedById",
                    target: "User",
                    relationship: "CreatedBy",
                },
                F::Datetime("CreatedDate"),
                F::Reference {
                    name: "LastModifiedById",
                    target: "User",
                    relationship: "LastModifiedBy",
                },
                F::Datetime("LastModifiedDate"),
            ],
        ),
        object(
            "AsyncApexJob",
            &[
                F::Id("Id"),
                F::String("JobType"),
                F::Integer("JobItemsProcessed"),
                F::String("MethodName"),
                F::Integer("NumberOfErrors"),
                F::String("Status"),
                F::Datetime("CreatedDate"),
                F::Reference {
                    name: "CreatedById",
                    target: "User",
                    relationship: "CreatedBy",
                },
                F::Reference {
                    name: "ApexClassId",
                    target: "ApexClass",
                    relationship: "ApexClass",
                },
            ],
        ),
        object(
            "AuthSession",
            &[
                F::Id("Id"),
                F::String("LoginType"),
                F::Reference {
                    name: "LoginHistoryId",
                    target: "LoginHistory",
                    relationship: "LoginHistory",
                },
                F::String("LogoutUrl"),
                F::Id("ParentId"),
                F::String("SessionSecurityLevel"),
                F::String("SessionType"),
                F::String("SourceIp"),
                F::Id("UsersId"),
            ],
        ),
        object(
            "CaseComment",
            &[F::Id("Id"), F::Id("ParentId"), F::String("CommentBody")],
        ),
        object(
            "CronTrigger",
            &[F::Id("Id"), F::String("State"), F::Datetime("NextFireTime")],
        ),
        object(
            "CustomPermission",
            &[F::Id("Id"), F::String("DeveloperName")],
        ),
        object(
            "EntityDefinition",
            &[
                F::Id("Id"),
                F::String("DeveloperName"),
                F::String("QualifiedApiName"),
            ],
        ),
        object(
            "FieldDefinition",
            &[
                F::Id("Id"),
                F::String("DeveloperName"),
                F::String("QualifiedApiName"),
            ],
        ),
        object(
            "FlowDefinitionView",
            &[
                F::Id("Id"),
                F::Id("ActiveVersionId"),
                F::String("ApiName"),
                F::String("Description"),
                F::String("DurableId"),
                F::String("Label"),
                F::String("LastModifiedBy"),
                F::Datetime("LastModifiedDate"),
                F::String("ManageableState"),
                F::String("ProcessType"),
                F::String("RecordTriggerType"),
                F::Reference {
                    name: "TriggerObjectOrEventId",
                    target: "EntityDefinition",
                    relationship: "TriggerObjectOrEvent",
                },
                F::Integer("TriggerOrder"),
                F::String("TriggerType"),
            ],
        ),
        object(
            "FlowVersionView",
            &[
                F::Id("Id"),
                F::String("ApiName"),
                F::Integer("ApiVersionRuntime"),
                F::String("Description"),
                F::String("DurableId"),
                F::Reference {
                    name: "FlowDefinitionViewId",
                    target: "FlowDefinitionView",
                    relationship: "FlowDefinitionView",
                },
                F::String("Label"),
                F::String("RunInMode"),
                F::Integer("VersionNumber"),
                F::String("Status"),
            ],
        ),
        object(
            "Group",
            &[
                F::Id("Id"),
                F::String("Name"),
                F::String("DeveloperName"),
                F::String("Type"),
            ],
        ),
        object("Lead", &[F::Id("Id"), F::String("Name")]),
        object(
            "LoginHistory",
            &[
                F::Id("Id"),
                F::String("Application"),
                F::String("Browser"),
                F::String("Platform"),
                F::Reference {
                    name: "UserId",
                    target: "User",
                    relationship: "User",
                },
            ],
        ),
        object(
            "Network",
            &[F::Id("Id"), F::String("Name"), F::String("UrlPathPrefix")],
        ),
        object(
            "OmniProcess",
            &[
                F::Id("Id"),
                F::Reference {
                    name: "CreatedById",
                    target: "User",
                    relationship: "CreatedBy",
                },
                F::Datetime("CreatedDate"),
                F::Reference {
                    name: "LastModifiedById",
                    target: "User",
                    relationship: "LastModifiedBy",
                },
                F::Datetime("LastModifiedDate"),
                F::Boolean("IsIntegrationProcedure"),
                F::String("OmniProcessType"),
                F::String("UniqueName"),
            ],
        ),
        object(
            "Organization",
            &[
                F::Id("Id"),
                F::Reference {
                    name: "CreatedById",
                    target: "User",
                    relationship: "CreatedBy",
                },
                F::Datetime("CreatedDate"),
                F::String("InstanceName"),
                F::Boolean("IsSandbox"),
                F::String("Name"),
                F::String("NamespacePrefix"),
                F::String("OrganizationType"),
                F::Date("TrialExpirationDate"),
            ],
        ),
        object(
            "PermissionSet",
            &[
                F::Id("Id"),
                F::String("Name"),
                F::String("Label"),
                F::Boolean("IsOwnedByProfile"),
            ],
        ),
        object(
            "PermissionSetAssignment",
            &[
                F::Id("Id"),
                F::Reference {
                    name: "AssigneeId",
                    target: "User",
                    relationship: "Assignee",
                },
                F::Reference {
                    name: "PermissionSetId",
                    target: "PermissionSet",
                    relationship: "PermissionSet",
                },
            ],
        ),
        object(
            "Profile",
            &[
                F::Id("Id"),
                F::String("Name"),
                F::Reference {
                    name: "UserLicenseId",
                    target: "UserLicense",
                    relationship: "UserLicense",
                },
            ],
        ),
        object("Topic", &[F::Id("Id"), F::String("Name")]),
        object(
            "TopicAssignment",
            &[
                F::Id("Id"),
                F::Reference {
                    name: "TopicId",
                    target: "Topic",
                    relationship: "Topic",
                },
                F::Id("EntityId"),
            ],
        ),
        object(
            "User",
            &[
                F::Id("Id"),
                F::String("Name"),
                F::String("Username"),
                F::String("FirstName"),
                F::String("LastName"),
                F::String("FederationIdentifier"),
                F::String("SmallPhotoUrl"),
                F::Reference {
                    name: "ProfileId",
                    target: "Profile",
                    relationship: "Profile",
                },
                F::Reference {
                    name: "UserRoleId",
                    target: "UserRole",
                    relationship: "UserRole",
                },
            ],
        ),
        object(
            "UserLicense",
            &[
                F::Id("Id"),
                F::String("Name"),
                F::String("LicenseDefinitionKey"),
            ],
        ),
        object("UserRole", &[F::Id("Id"), F::String("Name")]),
        object(
            "UserRecordAccess",
            &[
                F::Id("Id"),
                F::Id("UserId"),
                F::Id("RecordId"),
                F::Boolean("HasDeleteAccess"),
            ],
        ),
    ]
}

fn object(api_name: &str, fields: &[StandardField<'_>]) -> ObjectSchema {
    let mut object = ObjectSchema::new(api_name);
    for field in fields {
        object
            .insert_field(match *field {
                StandardField::Boolean(name) => FieldSchema::new(name, FieldType::Boolean, true),
                StandardField::Integer(name) => FieldSchema::new(name, FieldType::Integer, true),
                StandardField::String(name) => FieldSchema::new(name, FieldType::String, true),
                StandardField::Id(name) => FieldSchema::new(name, FieldType::Id, true),
                StandardField::Date(name) => FieldSchema::new(name, FieldType::Date, true),
                StandardField::Datetime(name) => FieldSchema::new(name, FieldType::Datetime, true),
                StandardField::Reference {
                    name,
                    target,
                    relationship,
                } => FieldSchema::new(
                    name,
                    FieldType::Reference {
                        target_object: target.to_owned(),
                    },
                    true,
                )
                .with_relationship_name(relationship),
            })
            .expect("curated standard schema has unique fields");
    }
    object
}
