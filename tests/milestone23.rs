use apex_exec::{
    project,
    runtime::{Interpreter, RecordingHost},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const QUERY_FIDELITY_DEMO: &str = r#"
public class QueryFidelityDemo {
    private static Integer queryTextCalls = 0;

    private static String queryText() {
        queryTextCalls++;
        return 'SELECT Id, Name FROM Invoice__c WHERE Status__c = :status ORDER BY Name';
    }

    public static void run() {
        Region__c region = new Region__c();
        region.Name = 'West';
        insert region;

        Customer__c customer = new Customer__c();
        customer.Name = 'Acme';
        customer.Region__c = region.Id;
        insert customer;

        Invoice__c first = new Invoice__c();
        first.Name = 'I-1';
        first.Amount__c = 10;
        first.Status__c = 'Open';
        first.Customer__c = customer.Id;
        Invoice__c second = new Invoice__c();
        second.Name = 'I-2';
        second.Amount__c = 20;
        second.Status__c = 'Open';
        second.Customer__c = customer.Id;
        Invoice__c third = new Invoice__c();
        third.Name = 'I-3';
        third.Amount__c = 30;
        third.Status__c = 'Closed';
        third.Customer__c = customer.Id;
        insert new List<Invoice__c>{first, second, third};

        Customer__c other = new Customer__c();
        other.Name = 'Other';
        insert other;
        Invoice__c outsider = new Invoice__c();
        outsider.Name = 'Outside';
        outsider.Amount__c = 99;
        outsider.Status__c = 'Archived';
        outsider.Customer__c = other.Id;
        insert outsider;

        List<Invoice__c> today = [
            SELECT Id FROM Invoice__c WHERE CreatedDate = TODAY
        ];
        System.debug(today.size());

        List<Customer__c> parents = [
            SELECT Id, Name,
                (SELECT Id, Name FROM Invoices__r ORDER BY Amount__c DESC LIMIT 1)
            FROM Customer__c
            WHERE Name = 'Acme'
        ];
        System.debug(parents[0].Invoices__r.size());
        System.debug(parents[0].Invoices__r[0].Name);

        List<Invoice__c> related = [
            SELECT Id, Customer__r.Region__r.Name
            FROM Invoice__c
            WHERE Name = 'I-1'
        ];
        System.debug(related[0].Customer__r.Region__r.Name);

        List<AggregateResult> groups = [
            SELECT Status__c, COUNT(Id) total
            FROM Invoice__c
            GROUP BY Status__c
            HAVING COUNT(Id) > 1
            ORDER BY Status__c
        ];
        System.debug(groups.size());
        System.debug((Integer)groups[0].get('total'));

        String status = 'Open';
        List<Invoice__c> dynamicRows = Database.query(queryText());
        System.debug(dynamicRows.size());
        List<Invoice__c> qualifiedRows = System.Database.query(
            'SELECT Id FROM Invoice__c WHERE Status__c = :status'
        );
        System.debug(qualifiedRows.size());
        System.debug(Database.countQuery(
            'SELECT COUNT() FROM Invoice__c WHERE CreatedDate = TODAY'
        ));
        System.debug(queryTextCalls);

        try {
            Database.query('SELECT Missing__c FROM Invoice__c');
        } catch (QueryException error) {
            System.debug(error.getTypeName());
        }
    }
}
"#;

#[test]
fn static_query_fidelity_executes_dates_children_relationships_and_having() {
    let root = test_project(QUERY_FIDELITY_DEMO);
    let compilation = project::compile(&root).unwrap();
    let mut host = RecordingHost::default();
    let output = Interpreter::with_host(&mut host)
        .invoke_static(&compilation.program, "QueryFidelityDemo", "run")
        .unwrap();
    assert_eq!(
        output,
        [
            "4",
            "1",
            "I-3",
            "West",
            "1",
            "2",
            "2",
            "2",
            "4",
            "1",
            "QueryException"
        ]
    );
    assert_eq!(host.query_events().len(), 7);
    assert!(host.query_events().iter().all(|event| event.succeeded));
    assert_eq!(
        host.query_events()[1].object_scans,
        2,
        "one parent scan plus one batched child scan is independent of parent count"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn query_locator_drives_batch_scope_without_an_interpreter_shortcut() {
    let batch_source = r#"
public class LocatorBatch implements Database.Batchable<Invoice__c> {
    public Database.QueryLocator start(Database.BatchableContext context) {
        return Database.getQueryLocator(
            'SELECT Id, Status__c FROM Invoice__c WHERE Status__c = \'Open\' ORDER BY Name'
        );
    }

    public void execute(Database.BatchableContext context, List<Invoice__c> scope) {
        for (Invoice__c invoice : scope) {
            invoice.Status__c = 'Processed';
        }
        update scope;
    }

    public void finish(Database.BatchableContext context) {}
}
"#;
    let demo_source = r#"
public class LocatorDemo {
    public static void run() {
        Invoice__c first = new Invoice__c();
        first.Name = 'A';
        first.Amount__c = 1;
        first.Status__c = 'Open';
        Invoice__c second = new Invoice__c();
        second.Name = 'B';
        second.Amount__c = 2;
        second.Status__c = 'Open';
        insert new List<Invoice__c>{first, second};

        Test.startTest();
        Database.executeBatch(new LocatorBatch(), 1);
        Test.stopTest();
        System.debug(Database.countQuery(
            'SELECT COUNT() FROM Invoice__c WHERE Status__c = \'Processed\''
        ));
    }
}
"#;
    let root = test_project(demo_source);
    fs::rename(
        root.join("force-app/main/default/classes/QueryFidelityDemo.cls"),
        root.join("force-app/main/default/classes/LocatorDemo.cls"),
    )
    .unwrap();
    fs::write(
        root.join("force-app/main/default/classes/LocatorBatch.cls"),
        batch_source,
    )
    .unwrap();
    let compilation = project::compile(&root).unwrap();
    assert_eq!(compilation.invoke("LocatorDemo.run").unwrap(), ["2"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_and_invalid_query_forms_fail_in_the_correct_phase() {
    let parse_error = apex_exec::parse(
        "List<Invoice__c> rows = [SELECT TYPEOF Customer__c WHEN Customer__c THEN Name END FROM Invoice__c];",
    )
    .unwrap_err();
    assert!(parse_error.message.contains("TYPEOF"));

    let cases = [
        (
            "List<Invoice__c> rows = [SELECT Id FROM Invoice__c WHERE Status__c = TODAY];",
            "date literal",
        ),
        (
            "List<AggregateResult> rows = [SELECT Status__c, COUNT(Id) total FROM Invoice__c GROUP BY Status__c HAVING SUM(Amount__c) > 1];",
            "must also appear in `SELECT`",
        ),
        (
            "List<Customer__c> rows = [SELECT Id, (SELECT Id, (SELECT Id FROM Invoices__r) FROM Invoices__r) FROM Customer__c];",
            "nested child SOQL subqueries are not supported",
        ),
    ];
    for (body, expected) in cases {
        let root = test_project(&format!(
            "public class QueryFidelityDemo {{ public static void run() {{ {body} }} }}"
        ));
        let error = project::compile(&root).unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "expected `{expected}` in `{error}`"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn milestone23_example_runs_through_the_cli() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/milestone23-project");
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("QueryFidelityDemo.run").unwrap(),
        [
            "4",
            "1",
            "I-3",
            "West",
            "1",
            "2",
            "2",
            "2",
            "4",
            "1",
            "QueryException"
        ]
    );

    let output = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["invoke", root.to_str().unwrap(), "QueryFidelityDemo.run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "4\n1\nI-3\nWest\n1\n2\n2\n2\n4\n1\nQueryException\n"
    );
    assert!(output.stderr.is_empty());
}

fn test_project(class_source: &str) -> PathBuf {
    let root = temp_directory();
    let base = root.join("force-app/main/default");
    let classes = base.join("classes");
    fs::create_dir_all(&classes).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}],"sourceApiVersion":"66.0"}"#,
    )
    .unwrap();
    fs::write(classes.join("QueryFidelityDemo.cls"), class_source).unwrap();
    write_schema(&base);
    root
}

fn write_schema(base: &Path) {
    write_object(base, "Region__c", &[]);
    write_object(
        base,
        "Customer__c",
        &[(
            "Region__c",
            "<referenceTo>Region__c</referenceTo><relationshipName>Customers</relationshipName><required>false</required><type>Lookup</type>",
        )],
    );
    write_object(
        base,
        "Invoice__c",
        &[
            (
                "Amount__c",
                "<precision>18</precision><scale>0</scale><required>true</required><type>Number</type>",
            ),
            (
                "Status__c",
                "<length>80</length><required>false</required><type>Text</type>",
            ),
            (
                "Customer__c",
                "<referenceTo>Customer__c</referenceTo><relationshipName>Invoices</relationshipName><required>false</required><type>Lookup</type>",
            ),
            ("Due__c", "<required>false</required><type>Date</type>"),
        ],
    );
}

fn write_object(base: &Path, api_name: &str, fields: &[(&str, &str)]) {
    let object = base.join("objects").join(api_name);
    fs::create_dir_all(object.join("fields")).unwrap();
    fs::write(
        object.join(format!("{api_name}.object-meta.xml")),
        "<CustomObject><nameField><type>Text</type><label>Name</label></nameField></CustomObject>",
    )
    .unwrap();
    for (field, details) in fields {
        fs::write(
            object
                .join("fields")
                .join(format!("{field}.field-meta.xml")),
            format!("<CustomField><fullName>{field}</fullName>{details}</CustomField>"),
        )
        .unwrap();
    }
}

fn temp_directory() -> PathBuf {
    let unique = format!(
        "apex-exec-m23-{}-{}-{}",
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
