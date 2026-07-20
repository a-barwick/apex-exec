use apex_exec::{
    oracle::{self, ConformanceManifest},
    platform::{FieldPermissions, ObjectPermissions, SecurityPolicy, SecurityUser},
    project,
    runtime::{Interpreter, RecordingHost},
    test_runner::{self, TestOptions},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);
const EXAMPLE_PROJECT: &str = "examples/milestone27-project";
const ORACLE_MANIFEST: &str = "examples/milestone27-oracle/oracle-manifest.json";

#[test]
fn with_without_and_inherited_sharing_propagate_across_call_boundaries() {
    let root = test_project(&[
        (
            "M27SharingDemo",
            r#"
public class M27SharingDemo {
    public static void run() {
        M27Row__c owned = new M27Row__c();
        owned.Name = 'Owned';
        M27Row__c other = new M27Row__c();
        other.Name = 'Other';
        other.OwnerId = '005000000000002AAA';
        insert new List<M27Row__c>{owned, other};

        System.debug(M27WithReader.directCount());
        System.debug(M27WithReader.inheritedCount());
        System.debug(M27WithoutReader.directCount());
        System.debug(M27WithoutReader.inheritedCount());
        System.debug(M27InheritedReader.countRows());
    }
}
"#,
        ),
        (
            "M27WithReader",
            r#"
public with sharing class M27WithReader {
    public static Integer directCount() {
        return [SELECT COUNT() FROM M27Row__c];
    }
    public static Integer inheritedCount() {
        return M27InheritedReader.countRows();
    }
}
"#,
        ),
        (
            "M27WithoutReader",
            r#"
public without sharing class M27WithoutReader {
    public static Integer directCount() {
        return [SELECT COUNT() FROM M27Row__c];
    }
    public static Integer inheritedCount() {
        return M27InheritedReader.countRows();
    }
}
"#,
        ),
        (
            "M27InheritedReader",
            r#"
public inherited sharing class M27InheritedReader {
    public static Integer countRows() {
        return [SELECT COUNT() FROM M27Row__c];
    }
    public static Integer entryCount() {
        M27Row__c owned = new M27Row__c();
        owned.Name = 'Owned entry';
        M27Row__c other = new M27Row__c();
        other.Name = 'Other entry';
        other.OwnerId = '005000000000002AAA';
        insert new List<M27Row__c>{owned, other};
        return [SELECT COUNT() FROM M27Row__c];
    }
}
"#,
        ),
    ]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("M27SharingDemo.run").unwrap(),
        ["1", "1", "2", "2", "2"]
    );
    assert_eq!(
        compilation.invoke("M27InheritedReader.entryCount").unwrap(),
        ["1"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn system_run_as_switches_the_deterministic_owner_visibility_context() {
    let root = test_project(&[
        (
            "M27RunAsDemo",
            r#"
@IsTest
private class M27RunAsDemo {
    @IsTest
    public static void run() {
        User second = new User();
        second.Username = 'm27-user@example.invalid';
        second.Alias = 'm27user';
        second.Email = 'm27-user@example.invalid';
        second.EmailEncodingKey = 'UTF-8';
        second.LastName = 'M27 User';
        second.LanguageLocaleKey = 'en_US';
        second.LocaleSidKey = 'en_US';
        second.ProfileId = String.valueOf(UserInfo.getProfileId());
        second.TimeZoneSidKey = 'America/Los_Angeles';
        insert second;

        M27Row__c currentOwner = new M27Row__c();
        currentOwner.Name = 'Current owner';
        M27Row__c secondOwner = new M27Row__c();
        secondOwner.Name = 'Second owner';
        secondOwner.OwnerId = second.Id;
        insert new List<M27Row__c>{currentOwner, secondOwner};

        System.runAs(second) {
            System.debug(
                String.valueOf(UserInfo.getUserId()) == String.valueOf(second.Id)
            );
            System.debug(M27WithReader.directCount());
            System.debug(M27WithoutReader.directCount());
            System.enqueueJob(new M27RunAsJob(second.Id));
        }
        Test.stopTest();
    }
}
"#,
        ),
        (
            "M27RunAsJob",
            r#"
public class M27RunAsJob implements Queueable {
    private String expectedUserId;
    public M27RunAsJob(String expectedUserId) {
        this.expectedUserId = expectedUserId;
    }
    public void execute(QueueableContext context) {
        System.debug(String.valueOf(UserInfo.getUserId()) == expectedUserId);
    }
}
"#,
        ),
        (
            "M27WithReader",
            "public with sharing class M27WithReader { public static Integer directCount() { return [SELECT COUNT() FROM M27Row__c]; } }",
        ),
        (
            "M27WithoutReader",
            "public without sharing class M27WithoutReader { public static Integer directCount() { return [SELECT COUNT() FROM M27Row__c]; } }",
        ),
    ]);
    write_user_schema(&root);
    let compilation = project::compile(&root).unwrap();
    let report = test_runner::run(
        &compilation,
        &TestOptions {
            filter: Some("M27RunAsDemo.run".to_owned()),
            jobs: 1,
        },
    )
    .unwrap();
    assert_eq!(report.tests.len(), 1);
    assert!(report.tests[0].failure.is_none());
    assert_eq!(report.tests[0].output, ["true", "1", "2", "true"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn query_dml_and_strip_inaccessible_enforce_the_configured_user_profile() {
    let root = test_project(&[(
        "M27SecurityDemo",
        r#"
public without sharing class M27SecurityDemo {
    private static Integer queryTextCalls = 0;
    private static Integer accessCalls = 0;

    private static String queryText() {
        queryTextCalls++;
        return 'SELECT Id, Public__c FROM M27Row__c ORDER BY Name';
    }

    private static AccessLevel accessMode() {
        accessCalls++;
        return AccessLevel.USER_MODE;
    }

    public static void run() {
        M27Row__c owned = new M27Row__c();
        owned.Name = 'Owned';
        owned.Public__c = 'visible';
        owned.Secret__c = 'hidden';
        M27Row__c other = new M27Row__c();
        other.Name = 'Other';
        other.Public__c = 'visible';
        other.Secret__c = 'other';
        other.OwnerId = '005000000000002AAA';
        insert new List<M27Row__c>{owned, other};

        try {
            List<M27Row__c> blocked = [
                SELECT Id, Secret__c FROM M27Row__c WITH SECURITY_ENFORCED
            ];
        } catch (QueryException error) {
            System.debug(error.getTypeName());
        }

        List<M27Row__c> securityEnforced = [
            SELECT Id, Public__c
            FROM M27Row__c
            WHERE Secret__c = 'hidden'
            WITH SECURITY_ENFORCED
        ];
        System.debug(securityEnforced.size());

        try {
            List<M27Row__c> blockedWhere = [
                SELECT Id, Public__c
                FROM M27Row__c
                WHERE Secret__c = 'hidden'
                WITH USER_MODE
            ];
        } catch (QueryException error) {
            System.debug(error.getTypeName());
        }

        List<M27Row__c> systemRows = [
            SELECT Id, Public__c, Secret__c FROM M27Row__c WITH SYSTEM_MODE
        ];
        System.debug(systemRows.size());

        List<M27Row__c> dynamicRows = Database.query(queryText(), accessMode());
        System.debug(dynamicRows.size());
        System.debug(queryTextCalls);
        System.debug(accessCalls);

        M27Row__c denied = new M27Row__c();
        denied.Name = 'Denied';
        denied.Secret__c = 'not creatable';
        Database.SaveResult deniedResult =
            Database.insert(denied, false, AccessLevel.USER_MODE);
        System.debug(deniedResult.isSuccess());
        System.debug(String.valueOf(deniedResult.getErrors()[0].getStatusCode()));

        M27Row__c allowed = new M27Row__c();
        allowed.Name = 'Allowed';
        allowed.Public__c = 'safe';
        Database.SaveResult allowedResult =
            Database.insert(allowed, false, AccessLevel.USER_MODE);
        System.debug(allowedResult.isSuccess());

        M27Row__c systemBypass = new M27Row__c();
        systemBypass.Name = 'System';
        systemBypass.Secret__c = 'permitted in system mode';
        Database.SaveResult systemInsert =
            Database.insert(systemBypass, false, AccessLevel.SYSTEM_MODE);
        System.debug(systemInsert.isSuccess());

        owned.Public__c = 'changed';
        Database.SaveResult deniedUpdate =
            Database.update(owned, false, AccessLevel.USER_MODE);
        System.debug(deniedUpdate.isSuccess());
        Database.SaveResult systemUpdate =
            Database.update(owned, false, AccessLevel.SYSTEM_MODE);
        System.debug(systemUpdate.isSuccess());
        try {
            Database.update(owned, true, AccessLevel.USER_MODE);
        } catch (DmlException error) {
            System.debug(error.getTypeName());
        }
        Database.DeleteResult deniedDelete =
            Database.delete(owned, false, AccessLevel.USER_MODE);
        System.debug(deniedDelete.isSuccess());
        Database.DeleteResult systemDelete =
            Database.delete(owned, false, AccessLevel.SYSTEM_MODE);
        System.debug(systemDelete.isSuccess());

        M27Row__c keywordAllowed = new M27Row__c();
        keywordAllowed.Name = 'Keyword user';
        keywordAllowed.Public__c = 'safe';
        insert as user keywordAllowed;
        System.debug(keywordAllowed.Id != null);
        M27Row__c keywordDenied = new M27Row__c();
        keywordDenied.Name = 'Keyword denied';
        keywordDenied.Secret__c = 'blocked';
        try {
            insert as user keywordDenied;
        } catch (DmlException error) {
            System.debug(error.getTypeName());
        }
        M27Row__c keywordSystem = new M27Row__c();
        keywordSystem.Name = 'Keyword system';
        keywordSystem.Secret__c = 'allowed';
        insert as system keywordSystem;
        System.debug(keywordSystem.Id != null);

        SObjectAccessDecision decision =
            Security.stripInaccessible(AccessType.READABLE, systemRows);
        List<SObject> sanitized = decision.getRecords();
        System.debug(sanitized.size());
        System.debug(sanitized[0].get('Secret__c') == null);
        System.debug(
            decision.getRemovedFields().get('M27Row__c').contains('Secret__c')
        );
        try {
            Security.stripInaccessible(AccessType.UPDATABLE, systemRows);
        } catch (NoAccessException error) {
            System.debug(error.getTypeName());
        }
    }
}
"#,
    )]);
    let compilation = project::compile(&root).unwrap();
    let mut host = RecordingHost::default();
    host.set_security_policy(security_policy());
    let output = Interpreter::with_host(&mut host)
        .invoke_static(&compilation.program, "M27SecurityDemo", "run")
        .unwrap();
    assert_eq!(
        output,
        [
            "QueryException",
            "1",
            "QueryException",
            "2",
            "1",
            "1",
            "1",
            "false",
            "INSUFFICIENT_ACCESS_OR_READONLY",
            "true",
            "true",
            "false",
            "true",
            "DmlException",
            "false",
            "true",
            "true",
            "DmlException",
            "true",
            "2",
            "true",
            "true",
            "NoAccessException",
        ]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_security_fixture_drives_default_cli_and_test_hosts() {
    let root = test_project(&[(
        "M27ConfiguredSecurity",
        r#"
public class M27ConfiguredSecurity {
    public static Integer run() {
        M27Row__c row = new M27Row__c();
        row.Name = 'Configured';
        row.Public__c = 'visible';
        insert row;
        return [
            SELECT COUNT()
            FROM M27Row__c
            WHERE Public__c = 'visible'
            WITH USER_MODE
        ];
    }
}
"#,
    )]);
    write_security_fixture(&root);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("M27ConfiguredSecurity.run").unwrap(),
        ["1"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_access_clauses_and_invalid_overloads_fail_in_the_correct_phase() {
    let malformed = test_project(&[(
        "MalformedSecurityQuery",
        "public class MalformedSecurityQuery { public static void run() { List<M27Row__c> rows = [SELECT Id FROM M27Row__c WITH UNKNOWN_MODE]; } }",
    )]);
    let error = project::compile(&malformed).unwrap_err().render();
    assert!(
        error.contains("SECURITY_ENFORCED")
            && error.contains("USER_MODE")
            && error.contains("SYSTEM_MODE"),
        "{error}"
    );
    fs::remove_dir_all(malformed).unwrap();

    let invalid_overload = test_project(&[(
        "InvalidSecurityOverload",
        "public class InvalidSecurityOverload { public static void run() { Database.query('SELECT Id FROM M27Row__c', true); } }",
    )]);
    let error = project::compile(&invalid_overload).unwrap_err().render();
    assert!(error.contains("AccessLevel"), "{error}");
    assert!(error.contains("found Boolean"), "{error}");
    fs::remove_dir_all(invalid_overload).unwrap();

    let invalid_strip = test_project(&[(
        "InvalidStripInput",
        "public class InvalidStripInput { public static void run() { Security.stripInaccessible(AccessType.READABLE, new List<String>()); } }",
    )]);
    let error = project::compile(&invalid_strip).unwrap_err().render();
    assert!(error.contains("must be a List of SObjects"));
    fs::remove_dir_all(invalid_strip).unwrap();

    let invalid_run_as = test_project(&[(
        "InvalidRunAsContext",
        "public class InvalidRunAsContext { public static void run(User user) { System.runAs(user) {} } }",
    )]);
    write_user_schema(&invalid_run_as);
    let error = project::compile(&invalid_run_as).unwrap_err().render();
    assert!(error.contains("only valid in an @IsTest class"), "{error}");
    fs::remove_dir_all(invalid_run_as).unwrap();
}

#[test]
fn strip_inaccessible_sanitizes_parent_and_child_relationship_graphs() {
    let root = test_project(&[(
        "M27RelationshipSecurity",
        r#"
public without sharing class M27RelationshipSecurity {
    public static void run() {
        M27Parent__c parent = new M27Parent__c();
        parent.Name = 'Parent';
        parent.Secret__c = 'parent secret';
        insert parent;
        M27Child__c child = new M27Child__c();
        child.Name = 'Child';
        child.Parent__c = parent.Id;
        child.Secret__c = 'child secret';
        insert child;

        List<M27Parent__c> source = [
            SELECT Id, Secret__c,
                (SELECT Id, Secret__c FROM Children__r)
            FROM M27Parent__c
            WITH SYSTEM_MODE
        ];
        SObjectAccessDecision decision =
            Security.stripInaccessible(AccessType.READABLE, source);
        M27Parent__c sanitized = (M27Parent__c)decision.getRecords()[0];
        System.debug(sanitized.get('Secret__c') == null);
        System.debug(sanitized.Children__r.size());
        System.debug(sanitized.Children__r[0].get('Secret__c') == null);
        System.debug(
            decision.getRemovedFields().get('M27Parent__c').contains('Secret__c')
        );
        System.debug(
            decision.getRemovedFields().get('M27Child__c').contains('Secret__c')
        );
    }
}
"#,
    )]);
    let compilation = project::compile(&root).unwrap();
    let mut host = RecordingHost::default();
    host.set_security_policy(relationship_security_policy());
    let output = Interpreter::with_host(&mut host)
        .invoke_static(&compilation.program, "M27RelationshipSecurity", "run")
        .unwrap();
    assert_eq!(output, ["true", "1", "true", "true", "true"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn milestone27_cli_checks_invokes_and_tests_the_sharing_slice() {
    let binary = env!("CARGO_BIN_EXE_apex-exec");
    let check = Command::new(binary)
        .args(["check", EXAMPLE_PROJECT])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&check.stdout).trim(),
        "OK (5 classes, 5 source files)"
    );

    let invoke = Command::new(binary)
        .args(["invoke", EXAMPLE_PROJECT, "M27SharingDemo.run"])
        .output()
        .unwrap();
    assert!(
        invoke.status.success(),
        "{}",
        String::from_utf8_lossy(&invoke.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&invoke.stdout).trim(), "1|1|2|2|2");

    let tests = Command::new(binary)
        .args([
            "test",
            EXAMPLE_PROJECT,
            "--filter",
            "M27SharingDemoTest.sharingPropagation",
            "--jobs",
            "1",
        ])
        .output()
        .unwrap();
    assert!(
        tests.status.success(),
        "{}",
        String::from_utf8_lossy(&tests.stderr)
    );
    assert!(String::from_utf8_lossy(&tests.stdout).contains("1 passed, 0 failed"));
}

#[test]
fn milestone27_oracle_fixture_is_locally_reproducible() {
    let manifest = ConformanceManifest::load(ORACLE_MANIFEST).unwrap();
    let local = oracle::run_local(&manifest);
    assert_eq!(local.fixtures.len(), 1);
    assert!(local.fixtures[0].compile.success);
    assert_eq!(local.fixtures[0].tests.len(), 1);
    assert_eq!(local.fixtures[0].tests[0].outcome, "pass");
}

fn security_policy() -> SecurityPolicy {
    let user = "005000000000001AAA";
    let mut policy = SecurityPolicy::new();
    policy.add_user(SecurityUser::new(user));
    policy.set_object_permissions(
        user,
        "M27Row__c",
        ObjectPermissions {
            readable: true,
            creatable: true,
            updatable: false,
            deletable: false,
        },
    );
    policy.set_field_permissions(
        user,
        "M27Row__c",
        "Id",
        FieldPermissions {
            readable: true,
            ..FieldPermissions::default()
        },
    );
    policy.set_field_permissions(user, "M27Row__c", "Name", FieldPermissions::all());
    policy.set_field_permissions(
        user,
        "M27Row__c",
        "Public__c",
        FieldPermissions {
            readable: true,
            creatable: true,
            updatable: false,
        },
    );
    policy.set_field_permissions(user, "M27Row__c", "Secret__c", FieldPermissions::default());
    policy
}

fn relationship_security_policy() -> SecurityPolicy {
    let user = "005000000000001AAA";
    let mut policy = SecurityPolicy::new();
    policy.add_user(SecurityUser::new(user));
    for object in ["M27Parent__c", "M27Child__c"] {
        policy.set_object_permissions(user, object, ObjectPermissions::all());
        for field in ["Id", "Name", "OwnerId"] {
            policy.set_field_permissions(user, object, field, FieldPermissions::all());
        }
        policy.set_field_permissions(user, object, "Secret__c", FieldPermissions::default());
    }
    policy.set_field_permissions(user, "M27Child__c", "Parent__c", FieldPermissions::all());
    policy
}

fn test_project(classes: &[(&str, &str)]) -> PathBuf {
    let root = temp_directory();
    let base = root.join("force-app/main/default");
    fs::create_dir_all(base.join("classes")).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}],"sourceApiVersion":"66.0"}"#,
    )
    .unwrap();
    for (name, source) in classes {
        fs::write(base.join("classes").join(format!("{name}.cls")), source).unwrap();
    }
    write_schema(&base);
    root
}

fn write_schema(base: &Path) {
    let object = base.join("objects/M27Row__c");
    fs::create_dir_all(object.join("fields")).unwrap();
    fs::write(
        object.join("M27Row__c.object-meta.xml"),
        "<CustomObject><label>M27 Row</label><pluralLabel>M27 Rows</pluralLabel><nameField><label>Name</label><type>Text</type></nameField><deploymentStatus>Deployed</deploymentStatus><sharingModel>Private</sharingModel></CustomObject>",
    )
    .unwrap();
    write_field(&object, "Public__c");
    write_field(&object, "Secret__c");

    let parent = base.join("objects/M27Parent__c");
    fs::create_dir_all(parent.join("fields")).unwrap();
    fs::write(
        parent.join("M27Parent__c.object-meta.xml"),
        "<CustomObject><label>M27 Parent</label><pluralLabel>M27 Parents</pluralLabel><nameField><label>Name</label><type>Text</type></nameField><deploymentStatus>Deployed</deploymentStatus><sharingModel>Private</sharingModel></CustomObject>",
    )
    .unwrap();
    write_field(&parent, "Secret__c");

    let child = base.join("objects/M27Child__c");
    fs::create_dir_all(child.join("fields")).unwrap();
    fs::write(
        child.join("M27Child__c.object-meta.xml"),
        "<CustomObject><label>M27 Child</label><pluralLabel>M27 Children</pluralLabel><nameField><label>Name</label><type>Text</type></nameField><deploymentStatus>Deployed</deploymentStatus><sharingModel>Private</sharingModel></CustomObject>",
    )
    .unwrap();
    write_field(&child, "Secret__c");
    fs::write(
        child.join("fields/Parent__c.field-meta.xml"),
        "<CustomField><fullName>Parent__c</fullName><deleteConstraint>SetNull</deleteConstraint><referenceTo>M27Parent__c</referenceTo><relationshipLabel>Children</relationshipLabel><relationshipName>Children</relationshipName><required>false</required><type>Lookup</type></CustomField>",
    )
    .unwrap();
}

fn write_field(object: &Path, name: &str) {
    fs::write(
        object.join("fields").join(format!("{name}.field-meta.xml")),
        format!(
            "<CustomField><fullName>{name}</fullName><length>80</length><required>false</required><type>Text</type></CustomField>"
        ),
    )
    .unwrap();
}

fn write_user_schema(root: &Path) {
    let object = root.join(".apex-exec/schema/objects");
    fs::create_dir_all(&object).unwrap();
    fs::write(
        object.join("User.object"),
        r#"<CustomObject>
<fields><fullName>Username</fullName><required>true</required><type>Text</type></fields>
<fields><fullName>Alias</fullName><required>true</required><type>Text</type></fields>
<fields><fullName>Email</fullName><required>true</required><type>Text</type></fields>
<fields><fullName>EmailEncodingKey</fullName><required>true</required><type>Text</type></fields>
<fields><fullName>LastName</fullName><required>true</required><type>Text</type></fields>
<fields><fullName>LanguageLocaleKey</fullName><required>true</required><type>Text</type></fields>
<fields><fullName>LocaleSidKey</fullName><required>true</required><type>Text</type></fields>
<fields><fullName>ProfileId</fullName><referenceTo>Profile</referenceTo><required>true</required><type>Lookup</type></fields>
<fields><fullName>TimeZoneSidKey</fullName><required>true</required><type>Text</type></fields>
</CustomObject>"#,
    )
    .unwrap();
}

fn write_security_fixture(root: &Path) {
    let directory = root.join(".apex-exec");
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("security.json"),
        r#"{
  "schemaVersion": 1,
  "objectPermissions": [
    {
      "principal": "*",
      "object": "M27Row__c",
      "readable": true,
      "creatable": true,
      "updatable": false,
      "deletable": false
    }
  ],
  "fieldPermissions": [
    {
      "principal": "*",
      "object": "M27Row__c",
      "field": "Id",
      "readable": true,
      "creatable": false,
      "updatable": false
    },
    {
      "principal": "*",
      "object": "M27Row__c",
      "field": "Public__c",
      "readable": true,
      "creatable": true,
      "updatable": false
    }
  ]
}"#,
    )
    .unwrap();
}

fn temp_directory() -> PathBuf {
    let unique = format!(
        "apex-exec-m27-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed)
    );
    let path = std::env::temp_dir().join(unique);
    fs::create_dir_all(&path).unwrap();
    path
}
