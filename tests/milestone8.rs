use apex_exec::{
    ast::{DmlOperation, Expression, SoqlAggregateFunction, SoqlSelectItem, Statement},
    project,
    runtime::{Interpreter, QueryKind, RecordingHost},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const COMPLETE_REPOSITORY: &str = r#"
public class RepositoryDemo {
    public static void run() {
        Customer__c customer = new Customer__c();
        customer.Name = 'Acme';
        insert customer;

        Invoice__c first = new Invoice__c();
        first.Name = 'INV-1';
        first.Amount__c = 10;
        first.Status__c = 'Open';
        first.Customer__c = customer.Id;

        Invoice__c second = new Invoice__c();
        second.Name = 'INV-2';
        second.Amount__c = 25;
        second.Status__c = 'Paid';
        second.Customer__c = customer.Id;
        List<Invoice__c> invoices = new List<Invoice__c>{first, second};
        insert invoices;

        Integer minimum = 10;
        Integer maximumRows = 5;
        List<Invoice__c> rows = [
            SELECT Id, Name, Amount__c, Customer__r.Name
            FROM Invoice__c
            WHERE Amount__c >= :minimum AND Status__c IN ('Open', 'Paid')
            ORDER BY Amount__c DESC NULLS LAST
            LIMIT :maximumRows OFFSET 0
        ];
        System.debug(rows.size());
        System.debug(rows[0].Name);
        System.debug(rows[0].Customer__r.Name);

        Invoice__c one = [
            SELECT Id, Name, Amount__c
            FROM Invoice__c
            WHERE Name = 'INV-1'
            LIMIT 1
        ];
        System.debug(one.Amount__c);

        Integer invoiceCount = [SELECT COUNT() FROM Invoice__c];
        System.debug(invoiceCount);

        List<AggregateResult> totals = [
            SELECT Status__c, SUM(Amount__c) total
            FROM Invoice__c
            GROUP BY Status__c
            ORDER BY Status__c ASC
        ];
        System.debug(totals.size());
        System.debug((Integer)totals[0].get('total'));

        List<List<SObject>> matches = [
            FIND 'INV-2'
            IN ALL FIELDS
            RETURNING Invoice__c(Id, Name ORDER BY Name LIMIT 2)
        ];
        System.debug(matches[0].size());
        System.debug(matches[0][0].get('Name'));

        first.Amount__c = 15;
        Database.update(first, true);
        Invoice__c updated = [
            SELECT Id, Amount__c FROM Invoice__c WHERE Id = :first.Id LIMIT 1
        ];
        System.debug(updated.Amount__c);

        delete second;
        System.debug([SELECT COUNT() FROM Invoice__c]);

        first.Amount__c = 16;
        upsert first;
        Invoice__c upserted = [
            SELECT Id, Amount__c FROM Invoice__c WHERE Id = :first.Id LIMIT 1
        ];
        System.debug(upserted.Amount__c);
    }
}
"#;

#[test]
fn dedicated_query_and_dml_nodes_preserve_structure_and_binds() {
    let parsed = apex_exec::parse(
        "Integer floor = 5;
         List<Thing__c> rows = [
             SELECT Id, SUM(Amount__c) total
             FROM Thing__c
             WHERE Amount__c >= :floor
             GROUP BY Id
             ORDER BY Id DESC
             LIMIT 2
         ];
         insert rows;",
    )
    .unwrap();

    let Statement::VariableDeclaration { initializer, .. } = &parsed.statements[1] else {
        panic!("expected query variable");
    };
    let Expression::Soql(query) = initializer else {
        panic!("expected dedicated SOQL expression");
    };
    assert_eq!(query.from.canonical, "thing__c");
    assert!(query.where_clause.is_some());
    assert_eq!(query.group_by.len(), 1);
    assert_eq!(query.order_by.len(), 1);
    assert!(matches!(
        query.select[1],
        SoqlSelectItem::Aggregate {
            function: SoqlAggregateFunction::Sum,
            ..
        }
    ));
    assert!(matches!(
        parsed.statements[2],
        Statement::Dml {
            operation: DmlOperation::Insert,
            ..
        }
    ));
}

#[test]
fn repository_queries_dml_aggregates_relationships_and_sosl_execute_together() {
    let root = test_project(COMPLETE_REPOSITORY);
    let compilation = project::compile(&root).unwrap();

    assert_eq!(
        compilation.invoke("RepositoryDemo.run").unwrap(),
        [
            "2", "INV-2", "Acme", "10", "2", "2", "10", "1", "INV-2", "15", "1", "16"
        ]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn milestone_eight_example_compiles_and_runs_through_the_cli() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/milestone8-project");
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("InvoiceDemo.run").unwrap(),
        ["INV-100", "Acme", "1"]
    );

    let output = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["invoke", root.to_str().unwrap(), "InvoiceDemo.run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "INV-100\nAcme\n1\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn recording_host_exposes_structured_query_and_dml_traces() {
    let root = test_project(COMPLETE_REPOSITORY);
    let compilation = project::compile(&root).unwrap();
    let mut host = RecordingHost::default();

    Interpreter::with_host(&mut host)
        .invoke_static(&compilation.program, "RepositoryDemo", "run")
        .unwrap();

    assert_eq!(host.dml_events().len(), 5);
    assert!(host.dml_events().iter().all(|event| event.succeeded));
    assert_eq!(host.dml_events()[1].records, 2);
    assert_eq!(host.query_events().len(), 8);
    assert_eq!(host.query_events()[4].kind, QueryKind::Sosl);
    assert!(host.query_events().iter().all(|event| event.succeeded));
    assert_eq!(host.query_events()[0].rows, 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn checker_rejects_unknown_schema_names_invalid_binds_and_bad_grouping() {
    let cases = [
        (
            "List<Invoice__c> rows = [SELECT Missing__c FROM Invoice__c];",
            "unknown field `Missing__c`",
        ),
        (
            "List<Invoice__c> rows = [SELECT Id FROM Missing__c];",
            "unknown SObject `Missing__c`",
        ),
        (
            "String minimum = 'ten'; List<Invoice__c> rows = [SELECT Id FROM Invoice__c WHERE Amount__c >= :minimum];",
            "SOQL bind for Integer requires Integer, found String",
        ),
        (
            "List<AggregateResult> rows = [SELECT Status__c, SUM(Amount__c) total FROM Invoice__c];",
            "must appear in `GROUP BY`",
        ),
        (
            "List<AggregateResult> rows = [SELECT SUM(Status__c) total FROM Invoice__c];",
            "require an Integer field",
        ),
        (
            "Integer value = 1; insert value;",
            "DML requires an SObject or List<SObject>",
        ),
    ];

    for (source, expected) in cases {
        let root = test_project(&format!(
            "public class RepositoryDemo {{ public static void run() {{ {source} }} }}"
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
fn malformed_query_syntax_is_rejected_at_the_query_grammar_boundary() {
    let cases = [
        (
            "List<SObject> rows = [SELECT Id Invoice__c];",
            "expected `FROM` after SOQL select list",
        ),
        (
            "List<SObject> rows = [FIND 'term' ALL FIELDS RETURNING Invoice__c(Id)];",
            "expected `IN` after SOSL search term",
        ),
        (
            "List<SObject> rows = [SELECT Id FROM Invoice__c WHERE Amount__c BETWEEN 1];",
            "expected a SOQL comparison operator",
        ),
    ];
    for (source, expected) in cases {
        let error = apex_exec::parse(source).unwrap_err();
        assert_eq!(error.message, expected);
    }
}

#[test]
fn dml_and_single_row_query_failures_are_typed_and_catchable() {
    let source = r#"
public class RepositoryDemo {
    public static void run() {
        Invoice__c missing = new Invoice__c();
        missing.Id = 'a01000000000001AAA';
        try {
            update missing;
        } catch (DmlException error) {
            System.debug(error.getTypeName());
        }

        try {
            Invoice__c absent = [
                SELECT Id FROM Invoice__c WHERE Name = 'absent' LIMIT 1
            ];
        } catch (QueryException error) {
            System.debug(error.getTypeName());
        }
    }
}
"#;
    let root = test_project(source);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("RepositoryDemo.run").unwrap(),
        ["DmlException", "QueryException"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_partial_dml_and_recycle_bin_semantics_fail_explicitly() {
    let source = r#"
public class RepositoryDemo {
    public static void run() {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'INV';
        invoice.Amount__c = 1;
        insert invoice;

        try {
            Database.update(invoice, false);
        } catch (DmlException error) {
            System.debug(error.getMessage());
        }

        delete invoice;
        try {
            undelete invoice;
        } catch (DmlException error) {
            System.debug(error.getMessage());
        }
    }
}
"#;
    let root = test_project(source);
    let compilation = project::compile(&root).unwrap();
    let output = compilation.invoke("RepositoryDemo.run").unwrap();
    assert!(output[0].contains("allOrNone=false"));
    assert!(output[1].contains("recycle-bin semantics"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn update_merges_only_assigned_fields_into_the_stored_record() {
    let source = r#"
public class RepositoryDemo {
    public static void run() {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'INV';
        invoice.Amount__c = 42;
        invoice.Status__c = 'Open';
        insert invoice;

        Invoice__c patch = new Invoice__c();
        patch.Id = invoice.Id;
        patch.Status__c = 'Paid';
        update patch;

        Invoice__c stored = [
            SELECT Id, Amount__c, Status__c
            FROM Invoice__c
            WHERE Id = :invoice.Id
            LIMIT 1
        ];
        System.debug(stored.Amount__c);
        System.debug(stored.Status__c);
    }
}
"#;
    let root = test_project(source);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("RepositoryDemo.run").unwrap(),
        ["42", "Paid"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn filters_support_like_boolean_precedence_collection_binds_and_offset() {
    let source = r#"
public class RepositoryDemo {
    public static void run() {
        Invoice__c alpha = new Invoice__c();
        alpha.Name = 'Alpha';
        alpha.Amount__c = 1;
        alpha.Status__c = 'Open';
        Invoice__c alpine = new Invoice__c();
        alpine.Name = 'Alpine';
        alpine.Amount__c = 2;
        alpine.Status__c = 'Open';
        Invoice__c beta = new Invoice__c();
        beta.Name = 'Beta';
        beta.Amount__c = 3;
        beta.Status__c = 'Closed';
        insert new List<Invoice__c>{alpha, alpine, beta};

        Set<String> statuses = new Set<String>{'Open'};
        List<Invoice__c> rows = [
            SELECT Id, Name
            FROM Invoice__c
            WHERE (Name LIKE 'Al%' OR Name = 'Beta')
                AND Status__c IN :statuses
            ORDER BY Name ASC NULLS LAST
            LIMIT 1 OFFSET 1
        ];
        System.debug(rows[0].Name);

        List<Invoice__c> notClosed = [
            SELECT Id FROM Invoice__c WHERE NOT (Status__c = 'Closed')
        ];
        System.debug(notClosed.size());
    }
}
"#;
    let root = test_project(source);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("RepositoryDemo.run").unwrap(),
        ["Alpine", "2"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn test_setup_dml_is_visible_to_each_test_but_isolated_between_tests() {
    let source = r#"
@IsTest
private class RepositoryDemo {
    @TestSetup
    static void seed() {
        Invoice__c invoice = new Invoice__c();
        invoice.Name = 'setup';
        invoice.Amount__c = 1;
        insert invoice;
    }

    @IsTest
    static void firstTest() {
        System.assertEquals(1, [SELECT COUNT() FROM Invoice__c]);
        Invoice__c extra = new Invoice__c();
        extra.Name = 'extra';
        extra.Amount__c = 2;
        insert extra;
        System.assertEquals(2, [SELECT COUNT() FROM Invoice__c]);
    }

    @IsTest
    static void secondTest() {
        System.assertEquals(1, [SELECT COUNT() FROM Invoice__c]);
    }
}
"#;
    let root = test_project(source);
    let compilation = project::compile(&root).unwrap();
    let report = apex_exec::test_runner::run(
        &compilation,
        &apex_exec::test_runner::TestOptions {
            filter: None,
            jobs: 2,
        },
    )
    .unwrap();
    assert!(report.is_success());
    assert_eq!(report.passed(), 2);
    fs::remove_dir_all(root).unwrap();
}

fn test_project(class_source: &str) -> PathBuf {
    let root = temp_directory();
    let base = root.join("force-app/main/default");
    let classes = base.join("classes");
    fs::create_dir_all(&classes).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}]}"#,
    )
    .unwrap();
    fs::write(classes.join("RepositoryDemo.cls"), class_source).unwrap();

    write_object(
        &base,
        "Customer__c",
        &[(
            "Category__c",
            "<length>80</length><required>false</required><type>Text</type>",
        )],
    );
    write_object(
        &base,
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
                "<referenceTo>Customer__c</referenceTo><relationshipName>Customer</relationshipName><required>false</required><type>Lookup</type>",
            ),
        ],
    );
    root
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
        "apex-exec-m8-{}-{}-{}",
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
