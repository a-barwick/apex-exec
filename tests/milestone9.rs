use apex_exec::{
    ast::TriggerEvent as AstTriggerEvent,
    platform::DmlOperation,
    project,
    runtime::{Interpreter, RecordingHost, TransactionEvent, TriggerPhase, TriggerStage},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const ENTRY: &str = r#"
public class TriggerDemo {
    public static void run() {
        Invoice__c root = new Invoice__c();
        root.Name = 'Root';
        root.Amount__c = 10;
        Invoice__c second = new Invoice__c();
        second.Name = 'Second';
        second.Amount__c = 20;
        insert new List<Invoice__c>{root, second};

        List<Invoice__c> inserted = [
            SELECT Id, Name, Amount__c, Status__c
            FROM Invoice__c
            ORDER BY Name
        ];
        System.debug(inserted.size());
        System.debug(inserted[0].Name);
        System.debug(inserted[1].Name);
        System.debug(inserted[2].Name);

        Invoice__c patch = new Invoice__c();
        patch.Id = second.Id;
        patch.Amount__c = 30;
        update patch;
        Invoice__c updated = [
            SELECT Id, Amount__c, Status__c
            FROM Invoice__c
            WHERE Id = :second.Id
            LIMIT 1
        ];
        System.debug(updated.Amount__c);
        System.debug(updated.Status__c);

        delete second;
        System.debug([SELECT COUNT() FROM Invoice__c]);
        undelete second;
        Invoice__c restored = [
            SELECT Id, Status__c FROM Invoice__c WHERE Id = :second.Id LIMIT 1
        ];
        System.debug(restored.Status__c);
        System.debug([SELECT COUNT() FROM Invoice__c]);

        Invoice__c accepted = new Invoice__c();
        accepted.Name = 'Accepted';
        accepted.Amount__c = 1;
        insert accepted;
        try {
            Invoice__c first = new Invoice__c();
            first.Name = 'Transient';
            first.Amount__c = 2;
            Invoice__c rejected = new Invoice__c();
            rejected.Name = 'Reject';
            rejected.Amount__c = 3;
            insert new List<Invoice__c>{first, rejected};
        } catch (DmlException error) {
            System.debug(error.getMessage());
        }
        System.debug([SELECT COUNT() FROM Invoice__c]);
        System.debug(TriggerAudit.beforeCount);
        System.debug(TriggerAudit.afterCount);
        System.debug(TriggerAudit.largestBatch);
        System.debug(TriggerAudit.oldAmount);
    }
}
"#;

const AUDIT: &str = r#"
public class TriggerAudit {
    public static Integer beforeCount = 0;
    public static Integer afterCount = 0;
    public static Integer largestBatch = 0;
    public static Integer oldAmount = 0;

    public static void record(Boolean isBefore, Integer size) {
        if (isBefore) {
            beforeCount++;
        } else {
            afterCount++;
        }
        if (size > largestBatch) {
            largestBatch = size;
        }
    }
}
"#;

const TRIGGER: &str = r#"
trigger InvoiceTrigger on Invoice__c (
    before insert, after insert,
    before update, after update,
    before delete, after delete,
    before undelete, after undelete
) {
    TriggerAudit.record(Trigger.isBefore, Trigger.size);

    if (Trigger.isBefore && Trigger.isInsert) {
        for (Invoice__c invoice : Trigger.new) {
            if (invoice.Name == 'Reject') {
                throw new DmlException('rejected by trigger');
            }
            invoice.Name = 'B-' + invoice.Name;
        }
    }
    if (Trigger.isAfter && Trigger.isInsert) {
        for (Invoice__c invoice : Trigger.new) {
            if (invoice.Name == 'B-Root') {
                Invoice__c child = new Invoice__c();
                child.Name = 'Child';
                child.Amount__c = 5;
                insert child;
            }
        }
    }
    if (Trigger.isBefore && Trigger.isUpdate) {
        Invoice__c previous = Trigger.oldMap.get(Trigger.new[0].Id);
        TriggerAudit.oldAmount = previous.Amount__c;
        Trigger.new[0].Amount__c++;
        Trigger.new[0].Status__c = 'Updated';
    }
    if (Trigger.isBefore && Trigger.isUndelete) {
        Trigger.new[0].Status__c = 'Restored';
    }
}
"#;

#[test]
fn trigger_declaration_has_dedicated_structure_and_context_member_nodes() {
    let parsed = apex_exec::parse(
        "trigger AccountTrigger on Account (before insert, after update) {
            System.debug(Trigger.new);
        }",
    )
    .unwrap();

    assert_eq!(parsed.triggers.len(), 1);
    let trigger = &parsed.triggers[0];
    assert_eq!(trigger.name.canonical, "accounttrigger");
    assert_eq!(trigger.object.canonical, "account");
    assert_eq!(
        trigger.events,
        [AstTriggerEvent::BeforeInsert, AstTriggerEvent::AfterUpdate]
    );
}

#[test]
fn bulk_context_recursion_recycle_bin_and_caught_rollback_execute_together() {
    let root = test_project(&[
        ("TriggerDemo.cls", ENTRY),
        ("TriggerAudit.cls", AUDIT),
        ("InvoiceTrigger.trigger", TRIGGER),
    ]);
    let compilation = project::compile(&root).unwrap();
    let output = compilation.invoke("TriggerDemo.run").unwrap();

    assert_eq!(
        &output[..9],
        [
            "3", "B-Child", "B-Root", "B-Second", "31", "Updated", "2", "Restored", "3"
        ]
    );
    assert_eq!(output[9], "rejected by trigger");
    assert_eq!(output[10], "4");
    assert_eq!(output[13], "2");
    assert_eq!(output[14], "20");
    let before = output[11].parse::<i64>().unwrap();
    let after = output[12].parse::<i64>().unwrap();
    assert_eq!(before, after + 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn uncaught_trigger_failure_rolls_back_the_complete_apex_transaction() {
    let failing = r#"
public class FailingTransaction {
    public static void run() {
        Invoice__c first = new Invoice__c();
        first.Name = 'First';
        first.Amount__c = 1;
        insert first;
        Invoice__c rejected = new Invoice__c();
        rejected.Name = 'Reject';
        rejected.Amount__c = 2;
        insert rejected;
    }

    public static void inspect() {
        System.debug([SELECT COUNT() FROM Invoice__c]);
    }
}
"#;
    let root = test_project(&[
        ("FailingTransaction.cls", failing),
        ("TriggerAudit.cls", AUDIT),
        ("InvoiceTrigger.trigger", TRIGGER),
    ]);
    let compilation = project::compile(&root).unwrap();
    let mut host = RecordingHost::default();

    let error = Interpreter::with_host(&mut host)
        .invoke_static(&compilation.program, "FailingTransaction", "run")
        .unwrap_err();
    assert_eq!(error.exception_type.as_deref(), Some("DmlException"));
    assert!(error.message.contains("rejected by trigger"));

    let output = Interpreter::with_host(&mut host)
        .invoke_static(&compilation.program, "FailingTransaction", "inspect")
        .unwrap();
    assert_eq!(output, ["0"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn timeline_is_deterministic_and_nests_recursive_trigger_work_in_source_order() {
    let recursive = r#"
public class RecursiveInsert {
    public static void run() {
        Invoice__c root = new Invoice__c();
        root.Name = 'Root';
        root.Amount__c = 1;
        insert root;
    }
}
"#;
    let root = test_project(&[
        ("RecursiveInsert.cls", recursive),
        ("TriggerAudit.cls", AUDIT),
        ("InvoiceTrigger.trigger", TRIGGER),
    ]);
    let compilation = project::compile(&root).unwrap();
    let mut host = RecordingHost::default();
    Interpreter::with_host(&mut host)
        .invoke_static(&compilation.program, "RecursiveInsert", "run")
        .unwrap();

    let timeline = host.timeline_events();
    assert!(matches!(
        &timeline[0],
        TransactionEvent::Trigger(event)
            if event.stage == TriggerStage::Enter
                && event.phase == TriggerPhase::Before
                && event.depth == 1
    ));
    assert!(matches!(
        &timeline[3],
        TransactionEvent::Trigger(event)
            if event.stage == TriggerStage::Enter
                && event.phase == TriggerPhase::After
                && event.depth == 1
    ));
    assert!(timeline.iter().any(|event| matches!(
        event,
        TransactionEvent::Trigger(trigger)
            if trigger.stage == TriggerStage::Enter && trigger.depth == 2
    )));
    assert_eq!(
        host.dml_events()
            .iter()
            .map(|event| event.operation)
            .collect::<Vec<_>>(),
        [DmlOperation::Insert, DmlOperation::Insert]
    );
    assert!(
        host.trigger_events()
            .iter()
            .all(|event| event.succeeded != Some(false))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trigger_checker_rejects_bad_objects_duplicate_events_and_context_mutation() {
    let cases = [
        (
            "trigger Bad on Missing__c (before insert) {}",
            "unknown trigger SObject `Missing__c`",
        ),
        (
            "trigger Bad on Invoice__c (before insert, before insert) {}",
            "duplicate trigger event",
        ),
        (
            "trigger Bad on Invoice__c (before insert) { Trigger.new = null; }",
            "Trigger.new is read-only",
        ),
    ];
    for (source, expected) in cases {
        let root = test_project(&[("Bad.trigger", source)]);
        let error = project::compile(&root).unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "expected `{expected}` in `{error}`"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn trigger_context_is_rejected_outside_a_trigger() {
    let root = test_project(&[(
        "Outside.cls",
        "public class Outside { public static void run() { System.debug(Trigger.size); } }",
    )]);
    let error = project::compile(&root).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Trigger context is only available inside a trigger")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn undelete_restores_the_deleted_record_and_preserves_its_id() {
    let source = r#"
public class RecycleBinDemo {
    public static void run() {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'Recycle';
        invoice.Amount__c = 9;
        insert invoice;
        String originalId = invoice.Id;
        delete invoice;
        undelete invoice;
        Invoice__c restored = [
            SELECT Id, Name, Amount__c FROM Invoice__c WHERE Id = :originalId LIMIT 1
        ];
        System.debug(restored.Id == originalId);
        System.debug(restored.Name);
        System.debug(restored.Amount__c);
    }
}
"#;
    let root = test_project(&[("RecycleBinDemo.cls", source)]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("RecycleBinDemo.run").unwrap(),
        ["true", "Recycle", "9"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn upsert_dispatches_insert_and_update_trigger_events_for_mixed_bulk_input() {
    let source = r#"
public class UpsertDemo {
    public static void run() {
        Invoice__c existing = new Invoice__c();
        existing.Name = 'Existing';
        existing.Amount__c = 1;
        insert existing;

        Invoice__c patch = new Invoice__c();
        patch.Id = existing.Id;
        patch.Amount__c = 4;
        Invoice__c fresh = new Invoice__c();
        fresh.Name = 'Fresh';
        fresh.Amount__c = 2;
        upsert new List<Invoice__c>{patch, fresh};
        System.debug([SELECT COUNT() FROM Invoice__c]);
    }
}
"#;
    let root = test_project(&[
        ("UpsertDemo.cls", source),
        ("TriggerAudit.cls", AUDIT),
        ("InvoiceTrigger.trigger", TRIGGER),
    ]);
    let compilation = project::compile(&root).unwrap();
    let mut host = RecordingHost::default();
    assert_eq!(
        Interpreter::with_host(&mut host)
            .invoke_static(&compilation.program, "UpsertDemo", "run")
            .unwrap(),
        ["2"]
    );
    let operations = host
        .trigger_events()
        .iter()
        .filter(|event| event.stage == TriggerStage::Enter)
        .map(|event| event.operation)
        .collect::<Vec<_>>();
    assert!(operations.contains(&DmlOperation::Insert));
    assert!(operations.contains(&DmlOperation::Update));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn old_and_after_context_records_are_read_only_and_roll_back_the_dml() {
    let source = r#"
public class ReadOnlyDemo {
    public static void run() {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'ReadOnly';
        invoice.Amount__c = 1;
        insert invoice;
    }
}
"#;
    let trigger = r#"
trigger ReadOnlyTrigger on Invoice__c (after insert) {
    Trigger.new[0].Name = 'Illegal';
}
"#;
    let root = test_project(&[
        ("ReadOnlyDemo.cls", source),
        ("ReadOnlyTrigger.trigger", trigger),
    ]);
    let compilation = project::compile(&root).unwrap();
    let mut host = RecordingHost::default();
    let error = Interpreter::with_host(&mut host)
        .invoke_static(&compilation.program, "ReadOnlyDemo", "run")
        .unwrap_err();
    assert_eq!(error.exception_type.as_deref(), Some("FinalException"));
    assert!(error.message.contains("read-only"));
    assert!(
        host.trigger_events()
            .iter()
            .any(|event| event.succeeded == Some(false))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trigger_context_lists_and_maps_are_read_only() {
    let source = r#"
public class ContextCollectionDemo {
    public static void run() {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'Context';
        invoice.Amount__c = 1;
        insert invoice;
    }
}
"#;
    for (name, mutation) in [
        ("ReadOnlyList", "Trigger.new.clear();"),
        ("ReadOnlyMap", "Trigger.newMap.clear();"),
    ] {
        let trigger = format!("trigger {name} on Invoice__c (after insert) {{ {mutation} }}");
        let trigger_file = format!("{name}.trigger");
        let root = test_project(&[
            ("ContextCollectionDemo.cls", source),
            (&trigger_file, &trigger),
        ]);
        let compilation = project::compile(&root).unwrap();
        let error = compilation.invoke("ContextCollectionDemo.run").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("context collections are read-only")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn recursive_triggers_have_an_explicit_deterministic_depth_limit() {
    let source = r#"
public class RecursiveFailure {
    public static void run() {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'First';
        invoice.Amount__c = 1;
        insert invoice;
    }
}
"#;
    let trigger = r#"
trigger RecursiveTrigger on Invoice__c (after insert) {
    Invoice__c next = new Invoice__c();
    next.Name = 'Next';
    next.Amount__c = 1;
    insert next;
}
"#;
    let root = test_project(&[
        ("RecursiveFailure.cls", source),
        ("RecursiveTrigger.trigger", trigger),
    ]);
    let compilation = project::compile(&root).unwrap();
    let error = compilation.invoke("RecursiveFailure.run").unwrap_err();
    assert_eq!(error.kind(), project::ProjectErrorKind::Diagnostic);
    assert!(
        error
            .to_string()
            .contains("maximum recursive trigger depth")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn milestone_nine_example_runs_through_cli_and_has_full_production_coverage() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/milestone9-project");
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("TriggerDemo.run").unwrap(),
        ["Increased", "Restored", "1"]
    );
    let report = apex_exec::test_runner::run(
        &compilation,
        &apex_exec::test_runner::TestOptions {
            filter: None,
            jobs: 2,
        },
    )
    .unwrap();
    assert!(report.is_success());
    assert_eq!(report.passed(), 3);
    assert_eq!(report.coverage.covered_lines, report.coverage.total_lines);
    assert_eq!(
        report.coverage.covered_branches,
        report.coverage.total_branches
    );

    let output = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["invoke", root.to_str().unwrap(), "TriggerDemo.run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Increased\nRestored\n1\n"
    );
    assert!(output.stderr.is_empty());
}

fn test_project(files: &[(&str, &str)]) -> PathBuf {
    let root = temp_directory();
    let base = root.join("force-app/main/default");
    let classes = base.join("classes");
    let triggers = base.join("triggers");
    fs::create_dir_all(&classes).unwrap();
    fs::create_dir_all(&triggers).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}]}"#,
    )
    .unwrap();
    for (name, source) in files {
        let directory = if name.ends_with(".trigger") {
            &triggers
        } else {
            &classes
        };
        fs::write(directory.join(name), source).unwrap();
    }
    write_object(&base);
    root
}

fn write_object(base: &Path) {
    let object = base.join("objects/Invoice__c");
    fs::create_dir_all(object.join("fields")).unwrap();
    fs::write(
        object.join("Invoice__c.object-meta.xml"),
        "<CustomObject><nameField><type>Text</type><label>Name</label></nameField></CustomObject>",
    )
    .unwrap();
    fs::write(
        object.join("fields/Amount__c.field-meta.xml"),
        "<CustomField><fullName>Amount__c</fullName><precision>18</precision><scale>0</scale><required>true</required><type>Number</type></CustomField>",
    )
    .unwrap();
    fs::write(
        object.join("fields/Status__c.field-meta.xml"),
        "<CustomField><fullName>Status__c</fullName><length>80</length><required>false</required><type>Text</type></CustomField>",
    )
    .unwrap();
}

fn temp_directory() -> PathBuf {
    let unique = format!(
        "apex-exec-m9-{}-{}-{}",
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
