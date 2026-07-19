use apex_exec::{
    project,
    runtime::{Interpreter, RecordingHost},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[test]
fn partial_insert_returns_ordered_results_errors_ids_and_one_limit_charge() {
    let source = r#"
public class PartialInsertDemo {
    public static void run() {
        M24Row__c first = new M24Row__c();
        first.Name = 'First';
        first.Amount__c = 1;
        M24Row__c invalid = new M24Row__c();
        invalid.Amount__c = 2;
        M24Row__c third = new M24Row__c();
        third.Name = 'Third';
        third.Amount__c = 3;

        List<Database.SaveResult> results = Database.insert(
            new List<M24Row__c>{first, invalid, third},
            false
        );
        System.debug(results.size());
        System.debug(results[0].isSuccess());
        System.debug(results[1].isSuccess());
        System.debug(results[2].isSuccess());
        System.debug(first.Id != null);
        System.debug(invalid.Id == null);
        System.debug(third.Id != null);
        System.debug(results[1].getId() == null);
        System.debug(results[1].getErrors().size());
        System.debug(String.valueOf(results[1].getErrors()[0].getStatusCode()));
        System.debug(
            results[1].getErrors()[0].getStatusCode()
                == StatusCode.REQUIRED_FIELD_MISSING
        );
        System.debug(results[1].getErrors()[0].getFields()[0]);
        System.debug(Limits.getDmlStatements());
        System.debug([SELECT COUNT() FROM M24Row__c]);
    }
}
"#;
    let root = test_project("PartialInsertDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    let mut host = RecordingHost::default();
    let output = Interpreter::with_host(&mut host)
        .invoke_static(&compilation.program, "PartialInsertDemo", "run")
        .unwrap();
    assert_eq!(
        output,
        [
            "3",
            "true",
            "false",
            "true",
            "true",
            "true",
            "true",
            "true",
            "1",
            "REQUIRED_FIELD_MISSING",
            "true",
            "Name",
            "1",
            "2",
        ]
    );
    assert_eq!(host.dml_events().len(), 1);
    assert_eq!(host.dml_events()[0].successful_records, 2);
    assert_eq!(host.dml_events()[0].failed_records, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn statements_and_all_or_none_true_remain_atomic_and_catchable() {
    let source = r#"
public class AtomicDmlDemo {
    public static void run() {
        M24Row__c valid = new M24Row__c();
        valid.Name = 'Valid';
        valid.Amount__c = 1;
        M24Row__c invalid = new M24Row__c();
        invalid.Amount__c = 2;
        try {
            Database.insert(new List<M24Row__c>{valid, invalid}, true);
        } catch (DmlException error) {
            System.debug(error.getTypeName());
        }
        System.debug(valid.Id == null);
        System.debug([SELECT COUNT() FROM M24Row__c]);

        try {
            insert new List<M24Row__c>{valid, invalid};
        } catch (DmlException error) {
            System.debug(error.getTypeName());
        }
        System.debug(valid.Id == null);
        System.debug([SELECT COUNT() FROM M24Row__c]);
    }
}
"#;
    let root = test_project("AtomicDmlDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("AtomicDmlDemo.run").unwrap(),
        ["DmlException", "true", "0", "DmlException", "true", "0"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn update_delete_and_undelete_return_their_distinct_result_shapes() {
    let source = r#"
public class ResultKindsDemo {
    public static void run() {
        M24Row__c stored = new M24Row__c();
        stored.Name = 'Stored';
        stored.Amount__c = 1;
        insert stored;

        M24Row__c patch = new M24Row__c();
        patch.Id = stored.Id;
        patch.Amount__c = 2;
        M24Row__c missing = new M24Row__c();
        List<Database.SaveResult> updates =
            Database.update(new List<M24Row__c>{patch, missing}, false);
        System.debug(updates[0].isSuccess());
        System.debug(updates[1].isSuccess());
        System.debug(String.valueOf(updates[1].getErrors()[0].getStatusCode()));

        List<Database.DeleteResult> deletes =
            Database.delete(new List<M24Row__c>{stored, missing}, false);
        System.debug(deletes[0].isSuccess());
        System.debug(deletes[1].isSuccess());

        M24Row__c neverDeleted = new M24Row__c();
        neverDeleted.Name = 'Never';
        neverDeleted.Amount__c = 4;
        insert neverDeleted;
        List<Database.UndeleteResult> undeletes =
            Database.undelete(new List<M24Row__c>{stored, neverDeleted}, false);
        System.debug(undeletes[0].isSuccess());
        System.debug(undeletes[1].isSuccess());
        System.debug([SELECT COUNT() FROM M24Row__c]);
    }
}
"#;
    let root = test_project("ResultKindsDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("ResultKindsDemo.run").unwrap(),
        [
            "true",
            "false",
            "INVALID_CROSS_REFERENCE_KEY",
            "true",
            "false",
            "true",
            "false",
            "2",
        ]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn external_id_upsert_updates_inserts_and_reports_created_flags() {
    let source = r#"
public class ExternalIdDemo {
    public static void run() {
        M24Row__c seed = new M24Row__c();
        seed.Name = 'Seed';
        seed.Amount__c = 1;
        seed.External_Key__c = 'seed';
        upsert seed External_Key__c;

        M24Row__c updateRow = new M24Row__c();
        updateRow.Name = 'Updated';
        updateRow.Amount__c = 2;
        updateRow.External_Key__c = 'SEED';
        M24Row__c insertRow = new M24Row__c();
        insertRow.Name = 'Inserted';
        insertRow.Amount__c = 3;
        insertRow.External_Key__c = 'new';
        M24Row__c missingKey = new M24Row__c();
        missingKey.Name = 'Missing';
        missingKey.Amount__c = 4;

        List<Database.UpsertResult> results = Database.upsert(
            new List<M24Row__c>{updateRow, insertRow, missingKey},
            M24Row__c.Fields.External_Key__c,
            false
        );
        System.debug(results[0].isSuccess());
        System.debug(results[0].isCreated());
        System.debug(results[1].isSuccess());
        System.debug(results[1].isCreated());
        System.debug(results[2].isSuccess());
        System.debug(String.valueOf(results[2].getErrors()[0].getStatusCode()));
        System.debug(updateRow.Id != null);
        System.debug(insertRow.Id != null);
        System.debug(missingKey.Id == null);

        M24Row__c duplicateOne = new M24Row__c();
        duplicateOne.Name = 'Duplicate One';
        duplicateOne.Amount__c = 5;
        duplicateOne.External_Key__c = 'duplicate';
        M24Row__c duplicateTwo = new M24Row__c();
        duplicateTwo.Name = 'Duplicate Two';
        duplicateTwo.Amount__c = 6;
        duplicateTwo.External_Key__c = 'DUPLICATE';
        insert new List<M24Row__c>{duplicateOne, duplicateTwo};
        M24Row__c ambiguous = new M24Row__c();
        ambiguous.Name = 'Ambiguous';
        ambiguous.Amount__c = 7;
        ambiguous.External_Key__c = 'duplicate';
        Database.UpsertResult ambiguousResult = Database.upsert(
            ambiguous,
            M24Row__c.Fields.External_Key__c,
            false
        );
        System.debug(ambiguousResult.isSuccess());
        System.debug(String.valueOf(ambiguousResult.getErrors()[0].getStatusCode()));
        System.debug(ambiguous.Id == null);
        System.debug([SELECT COUNT() FROM M24Row__c]);
    }
}
"#;
    let root = test_project("ExternalIdDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("ExternalIdDemo.run").unwrap(),
        [
            "true",
            "false",
            "true",
            "true",
            "false",
            "MISSING_ARGUMENT",
            "true",
            "true",
            "true",
            "false",
            "DUPLICATE_EXTERNAL_ID",
            "true",
            "4",
        ]
    );
    fs::remove_dir_all(root).unwrap();

    let unique_source = r#"
public class UniqueExternalIdDemo {
    public static void run() {
        M24Row__c first = new M24Row__c();
        first.Name = 'First';
        first.Amount__c = 1;
        first.External_Key__c = 'duplicate';
        M24Row__c second = new M24Row__c();
        second.Name = 'Second';
        second.Amount__c = 2;
        second.External_Key__c = 'DUPLICATE';
        List<Database.SaveResult> results = Database.insert(
            new List<M24Row__c>{first, second},
            false
        );
        System.debug(results[0].isSuccess());
        System.debug(results[1].isSuccess());
        System.debug(results[1].getErrors()[0].getStatusCode());
        System.debug([SELECT COUNT() FROM M24Row__c]);
    }
}
"#;
    let root = test_project("UniqueExternalIdDemo", unique_source, &[]);
    write_field(
        &root.join("force-app/main/default/objects/M24Row__c"),
        "External_Key__c",
        "<externalId>true</externalId><length>80</length><required>false</required><type>Text</type><unique>true</unique>",
    );
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("UniqueExternalIdDemo.run").unwrap(),
        ["true", "false", "DUPLICATE_VALUE", "1"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn partial_dml_preserves_bulk_trigger_boundaries_and_failure_restoration() {
    let source = r#"
public class TriggerBoundaryDemo {
    public static void run() {
        M24TriggerAudit.beforeRows = 0;
        M24TriggerAudit.afterRows = 0;
        M24Row__c valid = new M24Row__c();
        valid.Name = 'Valid';
        valid.Amount__c = 1;
        M24Row__c invalid = new M24Row__c();
        invalid.Amount__c = 2;
        List<Database.SaveResult> results =
            Database.insert(new List<M24Row__c>{valid, invalid}, false);
        System.debug(M24TriggerAudit.beforeRows);
        System.debug(M24TriggerAudit.afterRows);
        System.debug(valid.Amount__c);
        System.debug(invalid.Amount__c);
        System.debug(results[0].isSuccess());
        System.debug(results[1].isSuccess());
    }
}
"#;
    let trigger_audit = r#"
public class M24TriggerAudit {
    public static Integer beforeRows = 0;
    public static Integer afterRows = 0;
}
"#;
    let trigger = r#"
trigger M24RowTrigger on M24Row__c (before insert, after insert) {
    if (Trigger.isBefore) {
        M24TriggerAudit.beforeRows += Trigger.size;
        for (M24Row__c row : Trigger.new) {
            row.Amount__c += 1;
        }
    } else {
        M24TriggerAudit.afterRows += Trigger.size;
    }
}
"#;
    let root = test_project(
        "TriggerBoundaryDemo",
        source,
        &[
            ("classes/M24TriggerAudit.cls", trigger_audit),
            ("triggers/M24RowTrigger.trigger", trigger),
        ],
    );
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("TriggerBoundaryDemo.run").unwrap(),
        ["2", "1", "2", "2", "true", "false"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trigger_failure_rolls_back_only_its_partial_upsert_group() {
    let source = r#"
public class TriggerFailureDemo {
    public static void run() {
        M24Row__c stored = new M24Row__c();
        stored.Name = 'Stored';
        stored.Amount__c = 1;
        stored.External_Key__c = 'stored';
        insert stored;

        M24Row__c updateRow = new M24Row__c();
        updateRow.Name = 'Update';
        updateRow.Amount__c = 2;
        updateRow.External_Key__c = 'stored';
        M24Row__c insertRow = new M24Row__c();
        insertRow.Name = 'Insert';
        insertRow.Amount__c = 3;
        insertRow.External_Key__c = 'new';
        List<Database.UpsertResult> results = Database.upsert(
            new List<M24Row__c>{updateRow, insertRow},
            M24Row__c.Fields.External_Key__c,
            false
        );
        System.debug(results[0].isSuccess());
        System.debug(String.valueOf(results[0].getErrors()[0].getStatusCode()));
        System.debug(results[1].isSuccess());
        System.debug(results[1].isCreated());
        System.debug(updateRow.Id == null);
        System.debug(insertRow.Id != null);
        System.debug([SELECT COUNT() FROM M24Row__c]);
    }
}
"#;
    let trigger = r#"
trigger M24FailingUpdate on M24Row__c (before update) {
    throw new DmlException('update trigger rejected the group');
}
"#;
    let root = test_project(
        "TriggerFailureDemo",
        source,
        &[("triggers/M24FailingUpdate.trigger", trigger)],
    );
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("TriggerFailureDemo.run").unwrap(),
        [
            "false",
            "CANNOT_INSERT_UPDATE_ACTIVATE_ENTITY",
            "true",
            "true",
            "true",
            "true",
            "2",
        ]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dml_arguments_are_evaluated_once_and_invalid_forms_fail_during_checking() {
    let source = r#"
public class EvaluationDemo {
    private static Integer rowsCalls = 0;
    private static Integer modeCalls = 0;

    private static List<M24Row__c> rows() {
        rowsCalls++;
        M24Row__c row = new M24Row__c();
        row.Name = 'Once';
        row.Amount__c = 1;
        return new List<M24Row__c>{row};
    }

    private static Boolean partial() {
        modeCalls++;
        return false;
    }

    public static void run() {
        List<Database.SaveResult> results = Database.insert(rows(), partial());
        System.debug(results[0].isSuccess());
        System.debug(rowsCalls);
        System.debug(modeCalls);
    }
}
"#;
    let root = test_project("EvaluationDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("EvaluationDemo.run").unwrap(),
        ["true", "1", "1"]
    );
    fs::remove_dir_all(root).unwrap();

    let wrong_field = r#"
public class BadExternalId {
    public static void run() {
        M24Row__c row = new M24Row__c();
        Database.upsert(row, M24Row__c.Fields.Amount__c, false);
    }
}
"#;
    let root = test_project("BadExternalId", wrong_field, &[]);
    let error = project::compile(&root).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("not configured as an external ID")
    );
    fs::remove_dir_all(root).unwrap();

    let wrong_result_member = r#"
public class BadResultMember {
    public static void run() {
        M24Row__c row = new M24Row__c();
        Database.SaveResult result = Database.insert(row, false);
        result.isCreated();
    }
}
"#;
    let root = test_project("BadResultMember", wrong_result_member, &[]);
    let error = project::compile(&root).unwrap_err();
    assert!(error.to_string().contains("unknown method"));
    fs::remove_dir_all(root).unwrap();
}

fn test_project(class_name: &str, class_source: &str, extra_sources: &[(&str, &str)]) -> PathBuf {
    let root = temp_directory();
    let base = root.join("force-app/main/default");
    fs::create_dir_all(base.join("classes")).unwrap();
    fs::create_dir_all(base.join("triggers")).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}]}"#,
    )
    .unwrap();
    fs::write(
        base.join("classes").join(format!("{class_name}.cls")),
        class_source,
    )
    .unwrap();
    for (relative, source) in extra_sources {
        let path = base.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    write_schema(&base);
    root
}

fn write_schema(base: &Path) {
    let object = base.join("objects/M24Row__c");
    fs::create_dir_all(object.join("fields")).unwrap();
    fs::write(
        object.join("M24Row__c.object-meta.xml"),
        "<CustomObject><label>M24 Row</label><pluralLabel>M24 Rows</pluralLabel><nameField><label>Name</label><type>Text</type></nameField><deploymentStatus>Deployed</deploymentStatus><sharingModel>ReadWrite</sharingModel></CustomObject>",
    )
    .unwrap();
    write_field(
        &object,
        "Amount__c",
        "<precision>18</precision><required>true</required><scale>0</scale><type>Number</type>",
    );
    write_field(
        &object,
        "External_Key__c",
        "<externalId>true</externalId><length>80</length><required>false</required><type>Text</type><unique>false</unique>",
    );
}

fn write_field(object: &Path, name: &str, details: &str) {
    fs::write(
        object.join("fields").join(format!("{name}.field-meta.xml")),
        format!("<CustomField><fullName>{name}</fullName>{details}</CustomField>"),
    )
    .unwrap();
}

fn temp_directory() -> PathBuf {
    let unique = format!(
        "apex-exec-m24-{}-{}-{}",
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
