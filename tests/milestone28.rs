use apex_exec::{
    ast::AnnotationKind,
    check, execute, parse,
    platform::{
        DataValue, FieldSchema, FieldType, LocalDatabase, ObjectSchema, QueryAccessMode,
        QueryField, QueryOutcome, QuerySelect, Record, RecordId, SchemaCatalog, SharingMode,
        SoqlRequest, SummaryDefinition, SummaryFilter, SummaryFilterOperator, SummaryOperation,
    },
    project,
    test_runner::{self, TestOptions},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[test]
fn is_test_parallel_option_is_lossless_checked_and_class_scoped() {
    let source = r#"
@IsTest(IsParallel=true SeeAllData=false)
private class ParallelTests {
    @IsTest
    static void succeeds() {
        System.assert(true);
    }
}
"#;
    let parsed = parse(source).unwrap();
    assert!(matches!(
        parsed.classes[0].annotations[0].kind,
        AnnotationKind::IsTest {
            see_all_data: Some(false),
            is_parallel: Some(true)
        }
    ));
    check(source).unwrap();

    let method_option = check(
        "@IsTest private class WrongScope {
            @IsTest(IsParallel=true) static void invalid() {}
        }",
    )
    .unwrap_err();
    assert!(
        method_option
            .message
            .contains("only valid on an `@IsTest` class")
    );

    for invalid in [
        "@IsTest(IsParallel='yes') private class Invalid {}",
        "@IsTest(IsParallel=true, isparallel=false) private class Invalid {}",
        "@IsTest(Unknown=true) private class Invalid {}",
        "@IsTest(true) private class Invalid {}",
    ] {
        assert!(parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn suppress_warnings_is_a_validated_runtime_neutral_declaration_annotation() {
    let source = r#"
@SuppressWarnings('PMD.ApexDoc, PMD.CyclomaticComplexity')
public class SuppressedDemo {
    @SuppressWarnings('PMD.FieldNamingConventions')
    private static Integer value = 40;

    @SuppressWarnings('PMD.ApexDoc')
    public SuppressedDemo() {}

    @SuppressWarnings('PMD.PropertyNamingConventions')
    public Integer ignored { get; set; }

    @SuppressWarnings('PMD.NcssMethodCount')
    public static void run() {
        System.debug(value + 2);
    }
}
"#;
    let parsed = parse(source).unwrap();
    assert_eq!(
        parsed.classes[0].annotations[0].kind,
        AnnotationKind::SuppressWarnings
    );
    let root = test_project("SuppressedDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(compilation.invoke("SuppressedDemo.run").unwrap(), ["42"]);

    for invalid in [
        "@SuppressWarnings public class MissingValue {}",
        "@SuppressWarnings(value='PMD.ApexDoc') public class NamedValue {}",
        "@SuppressWarnings(true) public class WrongType {}",
        "@SuppressWarnings('PMD.ApexDoc', 'PMD.NcssMethodCount') public class TooMany {}",
    ] {
        let error = parse(invalid).unwrap_err();
        assert!(
            error
                .message
                .contains("requires exactly one positional String literal"),
            "{invalid}: {error}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn test_visible_opens_annotated_members_only_to_test_classes() {
    let source = r#"
public class TestVisibleDemo {
    @TestVisible
    private static Integer value = 40;

    @TestVisible
    private TestVisibleDemo() {}

    @TestVisible
    private Integer bonus { get; set; }

    @TestVisible
    private static Integer add(Integer left, Integer right) {
        return left + right;
    }

    @TestVisible
    private class Nested {
        public static Integer two() {
            return 2;
        }
    }
}
"#;
    let test_source = r#"
@IsTest
private class TestVisibleDemoTest {
    @IsTest
    static void accessesAnnotatedMembersAndNestedTypes() {
        TestVisibleDemo instance = new TestVisibleDemo();
        instance.bonus = TestVisibleDemo.Nested.two();
        System.assertEquals(42, TestVisibleDemo.add(TestVisibleDemo.value, instance.bonus));
    }
}
"#;
    let root = test_project(
        "TestVisibleDemo",
        source,
        &[("TestVisibleDemoTest", test_source)],
    );
    let compilation = project::compile(&root).unwrap();
    let report = test_runner::run(
        &compilation,
        &TestOptions {
            filter: Some("TestVisibleDemoTest.accessesAnnotatedMembersAndNestedTypes".to_owned()),
            jobs: 1,
        },
    )
    .unwrap();
    assert!(report.is_success());

    let invalid_root = test_project(
        "TestVisibleBoundary",
        "public class TestVisibleBoundary {
            @TestVisible private static Integer secret = 42;
        }",
        &[(
            "NonTestCaller",
            "public class NonTestCaller {
                public static Integer read() {
                    return TestVisibleBoundary.secret;
                }
            }",
        )],
    );
    let error = project::compile(&invalid_root).unwrap_err().render();
    assert!(error.contains("member is not accessible"), "{error}");

    for invalid in [
        "@TestVisible() private class EmptyArguments {}",
        "@TestVisible('tests') private class PositionalArgument {}",
    ] {
        let error = parse(invalid).unwrap_err();
        assert!(
            error.message.contains("does not accept arguments"),
            "{invalid}: {error}"
        );
    }
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(invalid_root).unwrap();
}

#[test]
fn comparable_contract_drives_stable_bounded_list_sorting() {
    let source = r#"
public class ComparableDemo {
    public static Integer comparisons = 0;

    public class Ranked implements System.Comparable {
        public Integer rank;
        public String label;

        public Ranked(Integer rank, String label) {
            this.rank = rank;
            this.label = label;
        }

        public Integer compareTo(Object other) {
            ComparableDemo.comparisons++;
            Ranked that = (Ranked) other;
            if (this.rank < that.rank) {
                return -1;
            }
            if (this.rank > that.rank) {
                return 1;
            }
            return 0;
        }
    }

    public class CompareFailure extends Exception {}

    public class Faulty implements Comparable {
        public Integer value;

        public Faulty(Integer value) {
            this.value = value;
        }

        public Integer compareTo(Object other) {
            throw new CompareFailure('compare failed');
        }
    }

    public static void run() {
        List<Ranked> values = new List<Ranked>{
            new Ranked(3, 'last'),
            new Ranked(1, 'first'),
            new Ranked(2, 'stable-a'),
            new Ranked(2, 'stable-b')
        };
        values.sort();
        for (Ranked value : values) {
            System.debug(value.label + ':' + value.rank);
        }
        System.debug('comparisons:' + comparisons);

        List<Faulty> faulty = new List<Faulty>{new Faulty(2), new Faulty(1)};
        try {
            faulty.sort();
        } catch (CompareFailure error) {
            System.debug(error.getMessage() + ':' + faulty[0].value);
        }
    }
}
"#;
    let root = test_project("ComparableDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("ComparableDemo.run").unwrap(),
        [
            "first:1",
            "stable-a:2",
            "stable-b:2",
            "last:3",
            "comparisons:5",
            "compare failed:2",
        ]
    );

    let invalid_contract = check(
        "public class InvalidComparable implements System.Comparable {
            public Long compareTo(Object other) { return 0; }
        }",
    )
    .unwrap_err();
    assert!(
        invalid_contract
            .message
            .contains("Integer compareTo(Object)")
    );
    let non_comparable = check(
        "public class NonComparableSort {
            public static void run() {
                List<NonComparableSort> values = new List<NonComparableSort>();
                values.sort();
            }
        }",
    )
    .unwrap_err();
    assert!(
        non_comparable
            .message
            .contains("requires primitive or Comparable list elements")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn database_stateful_preserves_batch_instance_state_between_transactions() {
    let output = execute(
        r#"
public class StatefulBatch implements Database.Batchable<Integer>, Database.Stateful {
    public Integer total = 0;

    public List<Integer> start(Database.BatchableContext context) {
        total = 10;
        System.debug('stateful:start:' + total);
        return new List<Integer>{1, 2, 3};
    }

    public void execute(Database.BatchableContext context, List<Integer> scope) {
        total += scope.size();
        System.debug('stateful:execute:' + total);
    }

    public void finish(Database.BatchableContext context) {
        System.debug('stateful:finish:' + total);
    }
}

public class StatelessBatch implements Database.Batchable<Integer> {
    public Integer total = 0;

    public List<Integer> start(Database.BatchableContext context) {
        total = 10;
        System.debug('stateless:start:' + total);
        return new List<Integer>{1, 2, 3};
    }

    public void execute(Database.BatchableContext context, List<Integer> scope) {
        total += scope.size();
        System.debug('stateless:execute:' + total);
    }

    public void finish(Database.BatchableContext context) {
        System.debug('stateless:finish:' + total);
    }
}

Test.startTest();
Database.executeBatch(new StatefulBatch(), 2);
Database.executeBatch(new StatelessBatch(), 2);
Test.stopTest();
"#,
    )
    .unwrap();
    assert_eq!(
        output,
        [
            "stateful:start:10",
            "stateful:execute:12",
            "stateful:execute:13",
            "stateful:finish:13",
            "stateless:start:10",
            "stateless:execute:2",
            "stateless:execute:1",
            "stateless:finish:0",
        ]
    );

    let error =
        check("public class InvalidStateful implements Database.Stateful<Integer> {}").unwrap_err();
    assert!(
        error.message.contains("does not accept generic arguments"),
        "{error}"
    );
}

#[test]
fn sobject_type_switch_checks_and_executes_with_single_evaluation() {
    let root = test_project(
        "M28SwitchDemo",
        r#"
public class M28SwitchDemo {
    private static Integer evaluations = 0;

    private static SObject selected(String kind) {
        evaluations++;
        if (kind == 'alpha') {
            M28Alpha__c alpha = new M28Alpha__c();
            alpha.Name = 'Alpha';
            return alpha;
        }
        if (kind == 'beta') {
            M28Beta__c beta = new M28Beta__c();
            beta.Name = 'Beta';
            return beta;
        }
        return null;
    }

    private static String classify(String kind) {
        SObject absent;
        String outcome = 'unmatched';
        switch on absent ?? selected(kind) {
            when M28Alpha__c alpha {
                outcome = 'alpha:' + alpha.Name;
            }
            when Schema.M28Beta__c beta {
                outcome = 'beta:' + beta.Name;
            }
            when else {
                outcome = 'other';
            }
        }
        return outcome + ':' + evaluations;
    }

    public static void run() {
        System.debug(classify('alpha'));
        System.debug(classify('beta'));
        System.debug(classify('none'));
    }
}
"#,
        &[(
            "M28SwitchDemoTest",
            r#"
@IsTest(IsParallel=true)
private class M28SwitchDemoTest {
    @IsTest
    static void matchesEveryArm() {
        M28SwitchDemo.run();
    }
}
"#,
        )],
    );
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("M28SwitchDemo.run").unwrap(),
        ["alpha:Alpha:1", "beta:Beta:2", "other:3"]
    );
    let report = test_runner::run(
        &compilation,
        &TestOptions {
            filter: Some("M28SwitchDemoTest.matchesEveryArm".to_owned()),
            jobs: 4,
        },
    )
    .unwrap();
    assert_eq!(report.tests.len(), 1);
    assert!(report.is_success());
    assert_eq!(
        report.tests[0].output,
        ["alpha:Alpha:1", "beta:Beta:2", "other:3"]
    );
    assert!(
        report.coverage.covered_branches >= 6,
        "typed switch patterns and null coalescing should emit branch observations"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sobject_type_switch_rejects_invalid_patterns_and_scope_leaks() {
    let cases = [
        (
            "DuplicatePattern",
            "duplicate switch type pattern",
            r#"
public class DuplicatePattern {
    public static void run() {
        SObject value = new M28Alpha__c();
        switch on value {
            when M28Alpha__c first {}
            when Schema.M28Alpha__c second {}
        }
    }
}
"#,
        ),
        (
            "PrimitivePattern",
            "must name a concrete SObject",
            r#"
public class PrimitivePattern {
    public static void run() {
        SObject value = new M28Alpha__c();
        switch on value { when String text {} }
    }
}
"#,
        ),
        (
            "WrongSwitchValue",
            "requires an SObject value",
            r#"
public class WrongSwitchValue {
    public static void run() {
        switch on 'value' { when M28Alpha__c alpha {} }
    }
}
"#,
        ),
        (
            "ScalarLabels",
            "scalar `switch when` labels",
            r#"
public class ScalarLabels {
    public static void run() {
        SObject value = new M28Alpha__c();
        switch on value { when 'value' {} }
    }
}
"#,
        ),
        (
            "PatternScope",
            "unknown variable `alpha`",
            r#"
public class PatternScope {
    public static void run() {
        SObject value = new M28Alpha__c();
        switch on value { when M28Alpha__c alpha {} }
        System.debug(alpha);
    }
}
"#,
        ),
    ];
    for (class_name, expected, source) in cases {
        let root = test_project(class_name, source, &[]);
        let error = project::compile(&root).unwrap_err().render();
        assert!(error.contains(expected), "{class_name}: {error}");
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn exact_equality_uses_identity_with_equality_precedence_and_single_evaluation() {
    let source = r#"
public class ExactEqualityDemo {
    private static Integer evaluations = 0;

    private static Object selected() {
        evaluations++;
        return 'Delete';
    }

    public static void run() {
        List<Integer> first = new List<Integer>{1};
        List<Integer> alias = first;
        List<Integer> copy = new List<Integer>{1};
        Object expected = 'Delete';
        Object wrongCase = 'DELETE';

        System.debug(first == copy);
        System.debug(first === alias);
        System.debug(first === copy);
        System.debug(first !== copy);
        System.debug(expected === selected());
        System.debug(expected === wrongCase);
        System.debug(null === null);
        System.debug(first === alias && first !== copy || false);
        System.debug(evaluations);
    }
}
"#;
    let root = test_project("ExactEqualityDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("ExactEqualityDemo.run").unwrap(),
        [
            "true", "true", "false", "true", "true", "false", "true", "true", "1"
        ]
    );

    let error = check(
        "public class InvalidExactEquality {
            public static Boolean compare() { return 'text' === 1; }
        }",
    )
    .unwrap_err();
    assert!(
        error
            .message
            .contains("cannot be applied to String and Integer")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn all_rows_queries_expose_soft_deleted_records_and_is_deleted() {
    let source = r#"
public class AllRowsDemo {
    public static void run() {
        M28Alpha__c deletedRecord = new M28Alpha__c();
        deletedRecord.Name = 'Deleted';
        M28Alpha__c activeRecord = new M28Alpha__c();
        activeRecord.Name = 'Visible';
        insert new List<M28Alpha__c>{deletedRecord, activeRecord};
        delete deletedRecord;

        List<M28Alpha__c> visible = [
            SELECT Id, Name, IsDeleted
            FROM M28Alpha__c
            ORDER BY Name
        ];
        List<M28Alpha__c> everyRecord = [
            SELECT Id, Name, IsDeleted
            FROM M28Alpha__c
            ORDER BY Name
            ALL ROWS
        ];
        List<M28Alpha__c> deletedOnly = [
            SELECT Id, Name, IsDeleted
            FROM M28Alpha__c
            WHERE IsDeleted = true
            ALL ROWS
        ];

        System.debug(visible.size() + ':' + visible[0].Name + ':' + visible[0].IsDeleted);
        System.debug(everyRecord.size() + ':' + everyRecord[0].Name + ':' + everyRecord[0].IsDeleted);
        System.debug(deletedOnly.size() + ':' + deletedOnly[0].Name + ':' + deletedOnly[0].IsDeleted);

        undelete deletedRecord;
        System.debug([SELECT COUNT() FROM M28Alpha__c]);
    }
}
"#;
    let root = test_project("AllRowsDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("AllRowsDemo.run").unwrap(),
        ["1:Visible:false", "2:Deleted:true", "1:Deleted:true", "2",]
    );

    let invalid_root = test_project(
        "InvalidDeletedWrite",
        "public class InvalidDeletedWrite {
            public static void run(M28Alpha__c record) {
                record.IsDeleted = true;
            }
        }",
        &[],
    );
    let error = project::compile(&invalid_root).unwrap_err().render();
    assert!(error.contains("field `M28Alpha__c.IsDeleted` is read-only"));

    assert!(
        parse(
            "public class InvalidAllRows {
            public static void run() {
                List<SObject> rows = [SELECT Id FROM M28Alpha__c ALL];
            }
        }"
        )
        .unwrap_err()
        .message
        .contains("expected `ROWS` after `ALL`")
    );
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(invalid_root).unwrap();
}

#[test]
fn rollup_summaries_compute_count_sum_min_and_max_with_one_child_scan() {
    let mut invoice = ObjectSchema::with_key_prefix("Invoice__c", "a10").unwrap();
    invoice
        .insert_field(FieldSchema::new("Id", FieldType::Id, false))
        .unwrap();
    invoice
        .insert_field(FieldSchema::new(
            "PaidLines__c",
            FieldType::Summary {
                result_type: Box::new(FieldType::Integer),
                definition: SummaryDefinition {
                    child_object: "InvoiceLine__c".to_owned(),
                    foreign_key_field: "Invoice__c".to_owned(),
                    operation: SummaryOperation::Count,
                    summarized_field: None,
                    filters: vec![SummaryFilter {
                        field: "Paid__c".to_owned(),
                        operator: SummaryFilterOperator::Equal,
                        value: "true".to_owned(),
                    }],
                },
            },
            false,
        ))
        .unwrap();
    for (field, operation) in [
        ("Total__c", SummaryOperation::Sum),
        ("Smallest__c", SummaryOperation::Min),
        ("Largest__c", SummaryOperation::Max),
    ] {
        invoice
            .insert_field(FieldSchema::new(
                field,
                FieldType::Summary {
                    result_type: Box::new(FieldType::Integer),
                    definition: SummaryDefinition {
                        child_object: "InvoiceLine__c".to_owned(),
                        foreign_key_field: "Invoice__c".to_owned(),
                        operation,
                        summarized_field: Some("Amount__c".to_owned()),
                        filters: Vec::new(),
                    },
                },
                true,
            ))
            .unwrap();
    }

    let mut line = ObjectSchema::with_key_prefix("InvoiceLine__c", "a11").unwrap();
    line.insert_field(FieldSchema::new("Id", FieldType::Id, false))
        .unwrap();
    line.insert_field(FieldSchema::new(
        "Invoice__c",
        FieldType::Reference {
            target_object: "Invoice__c".to_owned(),
        },
        false,
    ))
    .unwrap();
    line.insert_field(FieldSchema::new("Amount__c", FieldType::Integer, true))
        .unwrap();
    line.insert_field(FieldSchema::new("Paid__c", FieldType::Boolean, false))
        .unwrap();

    let schema = SchemaCatalog::from_objects([invoice, line]).unwrap();
    let invoice_id = RecordId::generate("a10", 1).unwrap();
    let mut records = vec![Record::new("Invoice__c", invoice_id.clone())];
    for (sequence, amount, paid) in [(1, 10_i64, true), (2, 20, false), (3, 30, true)] {
        let mut record = Record::new(
            "InvoiceLine__c",
            RecordId::generate("a11", sequence).unwrap(),
        );
        record.set_field("Invoice__c", invoice_id.clone());
        record.set_field("Amount__c", amount);
        record.set_field("Paid__c", paid);
        records.push(record);
    }

    let mut database = LocalDatabase::new(schema).unwrap();
    database.load_fixture(records).unwrap();
    let selected = ["PaidLines__c", "Total__c", "Smallest__c", "Largest__c"]
        .into_iter()
        .map(|field| {
            QuerySelect::Field(QueryField {
                relationships: Vec::new(),
                field: field.to_owned(),
            })
        })
        .collect();
    let outcome = database
        .execute_soql(&SoqlRequest {
            object: "Invoice__c".to_owned(),
            select: selected,
            condition: None,
            access: QueryAccessMode::Default,
            sharing: SharingMode::WithoutSharing,
            user_id: "005000000000001AAA".to_owned(),
            visible_record_ids: None,
            group_by: Vec::new(),
            having: None,
            order_by: Vec::new(),
            limit: None,
            offset: 0,
            all_rows: false,
            count_scalar: false,
            now_millis: 0,
        })
        .unwrap();
    let QueryOutcome::Records(rows) = outcome else {
        panic!("roll-up query should return records");
    };
    assert_eq!(rows.len(), 1);
    let record = &rows[0].record;
    assert_eq!(record.field("PaidLines__c"), Some(&DataValue::Integer(2)));
    assert_eq!(record.field("Total__c"), Some(&DataValue::Integer(60)));
    assert_eq!(record.field("Smallest__c"), Some(&DataValue::Integer(10)));
    assert_eq!(record.field("Largest__c"), Some(&DataValue::Integer(30)));
    assert_eq!(
        database.last_query_object_scans(),
        2,
        "one parent scan plus one shared child scan is the deterministic cost bound"
    );
}

fn test_project(class_name: &str, source: &str, extra_classes: &[(&str, &str)]) -> PathBuf {
    let root = temp_directory();
    let base = root.join("force-app/main/default");
    fs::create_dir_all(base.join("classes")).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}],"sourceApiVersion":"66.0"}"#,
    )
    .unwrap();
    fs::write(
        base.join("classes").join(format!("{class_name}.cls")),
        source,
    )
    .unwrap();
    for (name, source) in extra_classes {
        fs::write(base.join("classes").join(format!("{name}.cls")), source).unwrap();
    }
    write_object(&base, "M28Alpha__c", "M28 Alpha");
    write_object(&base, "M28Beta__c", "M28 Beta");
    root
}

fn write_object(base: &Path, api_name: &str, label: &str) {
    let object = base.join("objects").join(api_name);
    fs::create_dir_all(&object).unwrap();
    fs::write(
        object.join(format!("{api_name}.object-meta.xml")),
        format!(
            "<CustomObject><label>{label}</label><pluralLabel>{label}s</pluralLabel><nameField><label>Name</label><type>Text</type></nameField><deploymentStatus>Deployed</deploymentStatus><sharingModel>ReadWrite</sharingModel></CustomObject>"
        ),
    )
    .unwrap();
}

fn temp_directory() -> PathBuf {
    let unique = format!(
        "apex-exec-m28-{}-{}-{}",
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
