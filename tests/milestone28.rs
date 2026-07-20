use apex_exec::{
    ast::{AnnotationKind, ClassMember},
    check, execute, parse,
    platform::{
        DataValue, FieldSchema, FieldType, LocalDatabase, ObjectSchema, QueryAccessMode,
        QueryField, QueryOutcome, QuerySelect, Record, RecordId, SchemaCatalog, SharingMode,
        SoqlRequest, SummaryDefinition, SummaryFilter, SummaryFilterOperator, SummaryOperation,
    },
    project,
    runtime::{HttpResponseData, Interpreter, NetworkContext, RecordingHost},
    test_runner::{self, TestOptions},
};
use std::{
    collections::BTreeMap,
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
fn lexical_family_types_share_private_members_without_crossing_top_level_boundaries() {
    let source = r#"
public class LexicalPrivateDemo {
    private static final Integer BASE = 40;
    private static Integer observations = 0;

    private static Integer outerTwo() {
        return 2;
    }

    private class Left {
        private static Integer forty() {
            return 40;
        }

        private static Integer two() {
            return 2;
        }
    }

    private class Right {
        private static Integer total() {
            observations++;
            return Left.forty() + Left.two() + BASE + outerTwo() + observations;
        }
    }

    public static void run() {
        System.debug(Right.total());
    }
}
"#;
    let root = test_project("LexicalPrivateDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("LexicalPrivateDemo.run").unwrap(),
        ["85"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn nested_type_identity_is_stable_across_qualified_collection_spelling() {
    let source = r#"
public class NestedTypeIdentityDemo {
    public List<RecordInput> records { get; private set; }

    public static void run() {
        NestedTypeIdentityDemo demo = new NestedTypeIdentityDemo();
        demo.records = new List<NestedTypeIdentityDemo.RecordInput>();
        List<RecordInput> localRecords =
            new List<NestedTypeIdentityDemo.RecordInput>();
        localRecords.add(new RecordInput());
        demo.records.addAll(localRecords);
        System.debug(demo.records.size());
    }

    public class RecordInput {}
}
"#;
    let root = test_project("NestedTypeIdentityDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("NestedTypeIdentityDemo.run").unwrap(),
        ["1"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn list_casts_follow_covariant_sobject_identity_with_bounded_validation() {
    let source = r#"
public class SObjectListCastDemo {
    public static void run() {
        List<SObject> generic = new List<SObject>{new M28Alpha__c()};
        List<M28Alpha__c> narrowed = (List<M28Alpha__c>) generic;
        System.debug(narrowed.size());

        List<M28Alpha__c> concrete =
            new List<M28Alpha__c>{new M28Alpha__c()};
        List<SObject> widened = concrete;
        System.debug(((List<M28Alpha__c>) widened).size());

        List<SObject> mixed = new List<SObject>{
            new M28Alpha__c(),
            new M28Beta__c(),
            new M28Alpha__c()
        };
        try {
            List<M28Alpha__c> invalid = (List<M28Alpha__c>) mixed;
            System.debug(invalid.size());
        } catch (TypeException error) {
            System.debug(error.getMessage());
        }
    }
}
"#;
    let root = test_project("SObjectListCastDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("SObjectListCastDemo.run").unwrap(),
        [
            "1",
            "1",
            "List element at index 1 is not compatible with M28Alpha__c",
        ]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dynamic_sobject_exposes_only_the_typed_id_pseudo_field() {
    let source = r#"
public class DynamicSObjectIdDemo {
    public static void run() {
        SObject record = new M28Alpha__c();
        record.Id = Id.valueOf('a00000000000001AAA');
        Id identifier = record.Id;
        System.debug(identifier);
    }
}
"#;
    let root = test_project("DynamicSObjectIdDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("DynamicSObjectIdDemo.run").unwrap(),
        ["a00000000000001AAA"]
    );
    fs::remove_dir_all(root).unwrap();

    let invalid = check(
        "SObject record;
        Object unsupported = record.Name;",
    )
    .unwrap_err();
    assert!(
        invalid
            .message
            .contains("dynamic SObject fields require get/put access"),
        "{invalid}"
    );
}

#[test]
fn typed_sobject_constructors_validate_and_initialize_named_fields() {
    let source = r#"
public class NamedSObjectConstructorDemo {
    public static void run() {
        M28Alpha__c record = new Schema.M28Alpha__c(
            Id = 'a00000000000001AAA',
            Name = 'initialized'
        );
        System.debug(record.Id + ':' + record.Name);
    }
}
"#;
    let root = test_project("NamedSObjectConstructorDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation
            .invoke("NamedSObjectConstructorDemo.run")
            .unwrap(),
        ["a00000000000001AAA:initialized"]
    );
    fs::remove_dir_all(root).unwrap();

    for (source, expected) in [
        (
            "M28Alpha__c row = new M28Alpha__c(Missing__c = 'value');",
            "unknown field `Missing__c`",
        ),
        (
            "M28Alpha__c row = new M28Alpha__c(Name = 'first', name = 'second');",
            "duplicate SObject constructor field `name`",
        ),
        (
            "M28Alpha__c row = new M28Alpha__c('value');",
            "expects `field = value` arguments",
        ),
    ] {
        let root = test_project(
            "InvalidNamedSObjectConstructor",
            &format!(
                "public class InvalidNamedSObjectConstructor {{
                    public static void run() {{ {source} }}
                }}"
            ),
            &[],
        );
        let error = project::compile(&root).unwrap_err().to_string();
        assert!(error.contains(expected), "{source}: {error}");
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn sobject_list_map_constructor_indexes_ids_and_rejects_invalid_rows() {
    let source = r#"
public class SObjectListMapConstructorDemo {
    public static void run() {
        M28Alpha__c first = new M28Alpha__c(
            Id = 'a00000000000001AAA',
            Name = 'First'
        );
        M28Alpha__c second = new M28Alpha__c(
            Id = 'a00000000000002AAA',
            Name = 'Second'
        );
        List<M28Alpha__c> rows = new List<M28Alpha__c>{first, second};

        Map<Id, M28Alpha__c> byId = new Map<Id, M28Alpha__c>(rows);
        Map<String, M28Alpha__c> byText = new Map<String, M28Alpha__c>(rows);
        System.debug(byId.size() + ':' + byId.get(first.Id).Name);
        System.debug(byText.containsKey(String.valueOf(second.Id)));

        try {
            Map<Id, M28Alpha__c> duplicate = new Map<Id, M28Alpha__c>(
                new List<M28Alpha__c>{first, first}
            );
        } catch (Exception error) {
            System.debug(error.getTypeName() + ':' + error.getMessage());
        }

        try {
            Map<Id, M28Alpha__c> missingId = new Map<Id, M28Alpha__c>(
                new List<M28Alpha__c>{first, new M28Alpha__c(Name = 'No Id')}
            );
        } catch (Exception error) {
            System.debug(error.getTypeName() + ':' + error.getMessage());
        }

        try {
            Map<Id, M28Alpha__c> nullRow = new Map<Id, M28Alpha__c>(
                new List<M28Alpha__c>{first, null}
            );
        } catch (Exception error) {
            System.debug(error.getTypeName() + ':' + error.getMessage());
        }
    }
}
"#;
    let root = test_project("SObjectListMapConstructorDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation
            .invoke("SObjectListMapConstructorDemo.run")
            .unwrap(),
        [
            "2:First",
            "true",
            "ListException:Row with duplicate Id at index: 1",
            "ListException:Row with null Id at index: 1",
            "ListException:Null row at index: 1",
        ]
    );
    fs::remove_dir_all(root).unwrap();

    let root = test_project(
        "InvalidSObjectListMapConstructor",
        "public class InvalidSObjectListMapConstructor {
            public static void run() {
                Map<Id, Integer> invalid = new Map<Id, Integer>(
                    new List<Integer>{1}
                );
            }
        }",
        &[],
    );
    let error = project::compile(&root).unwrap_err().to_string();
    assert!(
        error.contains("expects Map<Id,Integer>, found List<Integer>"),
        "{error}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn id_in_binds_extract_ids_from_typed_sobject_collections() {
    let source = r#"
public class SObjectIdBindDemo {
    public static void run() {
        M28Alpha__c first = new M28Alpha__c(Name = 'first');
        M28Alpha__c second = new M28Alpha__c(Name = 'second');
        insert new List<M28Alpha__c>{first, second};

        Map<String, M28Alpha__c> selected = new Map<String, M28Alpha__c>{
            'first' => first
        };
        List<M28Alpha__c> rows = [
            SELECT Id, Name
            FROM M28Alpha__c
            WHERE Id IN :selected.values()
        ];
        System.debug(rows.size() + ':' + rows[0].Name);
    }
}
"#;
    let root = test_project("SObjectIdBindDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("SObjectIdBindDemo.run").unwrap(),
        ["1:first"]
    );
    fs::remove_dir_all(root).unwrap();

    let root = test_project(
        "InvalidSObjectBindDemo",
        "public class InvalidSObjectBindDemo {
            public static void run() {
                List<M28Alpha__c> rows = new List<M28Alpha__c>();
                List<M28Alpha__c> matches = [
                    SELECT Id FROM M28Alpha__c WHERE Name IN :rows
                ];
            }
        }",
        &[],
    );
    let error = project::compile(&root).unwrap_err().to_string();
    assert!(
        error.contains("SOQL `IN` bind requires List or Set of String"),
        "{error}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn id_and_string_assignments_convert_text_and_validate_ids() {
    let source = r#"
public class IdStringWideningDemo {
    public static void run() {
        Id identifier = Id.valueOf('a00000000000001AAA');
        String text = identifier;
        M28Alpha__c record = new M28Alpha__c(Name = identifier);
        System.debug(text + ':' + record.Name);

        String validText = '001000000000001AAA';
        Id converted = validText;
        List<Id> identifiers = new List<Id>();
        identifiers.add(validText);
        System.debug(converted + ':' + identifiers[0]);
        System.debug(validText instanceof Id);
        System.debug('not-an-id' instanceof Id);
        String absent = null;
        System.debug(absent instanceof Id);

        try {
            String invalidText = 'not-an-id';
            Id invalid = invalidText;
            System.debug(invalid);
        } catch (StringException error) {
            System.debug(error.getMessage());
        }
    }
}
"#;
    let root = test_project("IdStringWideningDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("IdStringWideningDemo.run").unwrap(),
        [
            "a00000000000001AAA:a00000000000001AAA",
            "001000000000001AAA:001000000000001AAA",
            "true",
            "false",
            "false",
            "Invalid id: not-an-id",
        ]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn queueable_implementations_are_assignable_to_the_platform_interface_type() {
    let source = r#"
public class QueueableParameterDemo {
    public class Work implements System.Queueable {
        public void execute(System.QueueableContext context) {}
    }

    public static Boolean accepts(System.Queueable work) {
        return work != null;
    }

    public static void run() {
        System.debug(accepts(new Work()));
    }
}
"#;
    let root = test_project("QueueableParameterDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("QueueableParameterDemo.run").unwrap(),
        ["true"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn long_value_of_and_datetime_epoch_conversion_are_checked() {
    let source = r#"
public class LongValueDemo {
    public static void run() {
        Long timestamp = Long.valueOf('1735689600000');
        Datetime value = Datetime.valueOf(timestamp);
        System.debug(timestamp);
        System.debug(value.getTime());
    }
}
"#;
    let root = test_project("LongValueDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("LongValueDemo.run").unwrap(),
        ["1735689600000", "1735689600000"]
    );
    fs::remove_dir_all(root).unwrap();

    let root = test_project(
        "InvalidLongValueDemo",
        "public class InvalidLongValueDemo {
            public static void run() {
                Long invalid = Long.valueOf('not-a-long');
            }
        }",
        &[],
    );
    let compilation = project::compile(&root).unwrap();
    let error = compilation
        .invoke("InvalidLongValueDemo.run")
        .unwrap_err()
        .to_string();
    assert!(error.contains("invalid Long `not-a-long`"), "{error}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn platform_event_metadata_includes_the_standard_event_uuid_field() {
    let source = r#"
public class PlatformEventUuidDemo {
    public static void run() {
        M28Signal__e event = new M28Signal__e();
        String uuid = event.EventUuid;
        Schema.SObjectField token = Schema.M28Signal__e.EventUuid;
        System.debug(uuid == null);
        System.debug(token.getDescribe().getName());
    }
}
"#;
    let root = test_project("PlatformEventUuidDemo", source, &[]);
    write_object(
        &root.join("force-app/main/default"),
        "M28Signal__e",
        "M28 Signal",
    );
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("PlatformEventUuidDemo.run").unwrap(),
        ["true", "EventUuid"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn flow_version_view_schema_includes_runtime_fields_and_definition_relationship() {
    let source = r#"
public class FlowVersionViewSchemaDemo {
    public static void run() {
        List<Schema.FlowVersionView> versions = [
            SELECT
                ApiVersionRuntime,
                DurableId,
                FlowDefinitionViewId,
                FlowDefinitionView.ApiName,
                RunInMode,
                Status,
                VersionNumber
            FROM FlowVersionView
        ];
        System.debug(versions.size());
    }
}
"#;
    let root = test_project("FlowVersionViewSchemaDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("FlowVersionViewSchemaDemo.run").unwrap(),
        ["0"]
    );
    let field = compilation
        .program
        .schema()
        .field("FlowVersionView", "FlowDefinitionViewId")
        .unwrap();
    assert_eq!(
        field.data_type(),
        &FieldType::Reference {
            target_object: "FlowDefinitionView".to_owned()
        }
    );
    assert_eq!(field.relationship_name(), Some("FlowDefinitionView"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unqualified_sobject_names_expose_the_static_sobject_type_token() {
    let source = r#"
public class UnqualifiedSObjectTypeDemo {
    public static void run() {
        Schema.SObjectType token = M28Alpha__c.SObjectType;
        System.debug(token.getDescribe().getName());
    }
}
"#;
    let root = test_project("UnqualifiedSObjectTypeDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation
            .invoke("UnqualifiedSObjectTypeDemo.run")
            .unwrap(),
        ["M28Alpha__c"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dml_options_are_nullable_mutable_and_drive_database_all_or_none() {
    let source = r#"
public class DmlOptionsDemo {
    public static void run() {
        Database.DmlOptions options = new Database.DmlOptions();
        System.debug(options.AllowFieldTruncation == null);
        System.debug(options.OptAllOrNone == null);
        options.AllowFieldTruncation = true;
        options.OptAllOrNone = false;

        M28Alpha__c valid = new M28Alpha__c();
        valid.Name = 'valid';
        valid.setOptions(options);
        List<Database.SaveResult> results = Database.insert(
            new List<SObject>{valid, null},
            options
        );
        System.debug(options.AllowFieldTruncation);
        System.debug(results[0].isSuccess() + ':' + results[1].isSuccess());
    }
}
"#;
    let root = test_project("DmlOptionsDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("DmlOptionsDemo.run").unwrap(),
        ["true", "true", "true", "true:false"]
    );
    fs::remove_dir_all(root).unwrap();

    let invalid = check(
        "Database.DmlOptions options = new Database.DmlOptions();
        options.Unsupported = true;",
    )
    .unwrap_err();
    assert!(
        invalid.message.contains("unknown member `Unsupported`"),
        "{invalid}"
    );
}

#[test]
fn enterprise_string_helpers_are_typed_utf16_aware_and_regex_checked() {
    let source = r#"
public class EnterpriseStringDemo {
    public static void run() {
        System.debug('namespace.Type'.substringBefore('.'));
        System.debug('namespace.Type'.substringAfter('.'));
        System.debug('one.two.three'.substringAfterLast('.'));
        System.debug('/apex/Page?x=1'.substringBetween('apex/', '?'));
        System.debug('Mixed Case'.containsIgnoreCase('case'));
        System.debug('A😀B'.left(3));
        System.debug('a , b, c'.replaceAll('( ,)|(,)|(, )', ','));

        List<String> pieces = 'a.b.'.split('\\.');
        System.debug(pieces.size() + ':' + pieces[0] + ':' + pieces[1]);
        System.debug(String.format('{0}:{1}', new List<Object>{'value', 42}));
        System.debug(String.escapeSingleQuotes('O\'Brien'));
        System.debug(System.JSON.serialize(new Map<String, Object>{'key' => 'value'}));
        System.debug('a' < 'B');
        System.debug('A' < 'a');
        System.debug('a' > (String) null);
        System.debug((String) null > 'a');

        try {
            'value'.split('[');
            System.assert(false);
        } catch (StringException expected) {
            System.assert(expected.getMessage().contains('regular expression'));
        }
    }
}
"#;
    let root = test_project("EnterpriseStringDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("EnterpriseStringDemo.run").unwrap(),
        [
            "namespace",
            "Type",
            "three",
            "Page",
            "true",
            "A😀",
            "a, b, c",
            "2:a:b",
            "value:42",
            "O\\'Brien",
            "{\"key\":\"value\"}",
            "true",
            "false",
            "true",
            "false",
        ]
    );
    fs::remove_dir_all(root).unwrap();
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
fn database_allows_callouts_gates_async_http_at_the_checked_contract() {
    let allowed = check(
        r#"
public class AllowedCalloutJob implements System.Queueable, Database.AllowsCallouts {
    public void execute(System.QueueableContext context) {
        HttpRequest request = new HttpRequest();
        request.setEndpoint('https://example.test/status');
        HttpResponse response = new Http().send(request);
        System.debug('allowed:' + response.getStatusCode());
    }
}

Test.startTest();
System.enqueueJob(new AllowedCalloutJob());
Test.stopTest();
"#,
    )
    .unwrap();
    let mut allowed_host = RecordingHost::default();
    allowed_host.enqueue_http_response(HttpResponseData {
        status_code: 200,
        status: "OK".to_owned(),
        body: String::new(),
        headers: BTreeMap::new(),
    });
    let output = Interpreter::with_host(&mut allowed_host)
        .execute(&allowed)
        .unwrap();
    assert_eq!(output, ["allowed:200"]);
    assert_eq!(allowed_host.callout_requests().len(), 1);

    let denied = check(
        r#"
public class DeniedCalloutJob implements System.Queueable {
    public void execute(System.QueueableContext context) {
        HttpRequest request = new HttpRequest();
        request.setEndpoint('https://example.test/status');
        HttpResponse response = new Http().send(request);
    }
}

Test.startTest();
System.enqueueJob(new DeniedCalloutJob());
Test.stopTest();
"#,
    )
    .unwrap();
    let mut denied_host = RecordingHost::default();
    denied_host.enqueue_http_response(HttpResponseData {
        status_code: 200,
        status: "OK".to_owned(),
        body: String::new(),
        headers: BTreeMap::new(),
    });
    let error = Interpreter::with_host(&mut denied_host)
        .execute(&denied)
        .unwrap_err();
    assert_eq!(error.exception_type.as_deref(), Some("CalloutException"));
    assert!(error.message.contains("Database.AllowsCallouts"), "{error}");
    assert!(
        denied_host.callout_requests().is_empty(),
        "denied async callout must fail before crossing the host boundary"
    );

    let generic_marker =
        check("public class InvalidCalloutMarker implements Database.AllowsCallouts<String> {}")
            .unwrap_err();
    assert!(
        generic_marker
            .message
            .contains("does not accept generic arguments"),
        "{generic_marker}"
    );
}

#[test]
fn user_batchable_context_implements_and_dispatches_the_platform_contract() {
    let output = execute(
        r#"
public class MockBatchableContext implements Database.BatchableContext {
    private Id jobId;
    private Id childJobId;

    public MockBatchableContext(Id jobId, Id childJobId) {
        this.jobId = jobId;
        this.childJobId = childJobId;
    }

    public Id getJobId() {
        return this.jobId;
    }

    public Id getChildJobId() {
        return this.childJobId;
    }
}

Database.BatchableContext context = new MockBatchableContext(
    Id.valueOf('707000000000001'),
    Id.valueOf('707000000000002')
);
System.debug(context.getJobId());
System.debug(context.getChildJobId());
"#,
    )
    .unwrap();
    assert_eq!(output, ["707000000000001", "707000000000002"]);

    let missing_method = check(
        "public class InvalidBatchableContext implements Database.BatchableContext {
            public Id getJobId() { return null; }
        }",
    )
    .unwrap_err();
    assert!(
        missing_method.message.contains("Id getChildJobId()"),
        "{missing_method}"
    );
}

#[test]
fn user_async_context_mocks_and_request_values_preserve_platform_types() {
    let output = execute(
        r#"
public class MockFinalizerContext implements System.FinalizerContext {
    private Id jobId;

    public MockFinalizerContext(Id jobId) {
        this.jobId = jobId;
    }

    public Id getAsyncApexJobId() {
        return this.jobId;
    }

    public Exception getException() {
        return null;
    }

    public System.ParentJobResult getResult() {
        return System.ParentJobResult.SUCCESS;
    }

    public String getRequestId() {
        return System.Request.getCurrent().getRequestId();
    }
}

public class MockQueueableContext implements System.QueueableContext {
    public Id getJobId() {
        return Id.valueOf('707000000000002');
    }
}

public class MockSchedulableContext implements System.SchedulableContext {
    public Id getTriggerId() {
        return Id.valueOf('08e000000000003');
    }
}

System.FinalizerContext finalizer =
    new MockFinalizerContext(Id.valueOf('707000000000001'));
System.QueueableContext queueable = new MockQueueableContext();
System.SchedulableContext schedulable = new MockSchedulableContext();
System.debug(finalizer.getAsyncApexJobId());
System.debug(finalizer.getResult().name());
System.debug(
    finalizer.getRequestId() == System.Request.getCurrent().getRequestId()
);
System.debug(System.Request.getCurrent().getQuiddity().name());
System.debug(queueable.getJobId());
System.debug(schedulable.getTriggerId());
System.debug(System.FinalizerContext.class.getName());
"#,
    )
    .unwrap();
    assert_eq!(
        output,
        [
            "707000000000001",
            "SUCCESS",
            "true",
            "UNDEFINED",
            "707000000000002",
            "08e000000000003",
            "System.FinalizerContext",
        ]
    );

    let missing_method = check(
        "public class InvalidFinalizerContext implements System.FinalizerContext {
            public Id getAsyncApexJobId() { return null; }
            public Exception getException() { return null; }
            public System.ParentJobResult getResult() {
                return System.ParentJobResult.SUCCESS;
            }
        }",
    )
    .unwrap_err();
    assert!(
        missing_method.message.contains("String getRequestId()"),
        "{missing_method}"
    );
}

#[test]
fn transient_instance_fields_execute_normally_and_are_omitted_from_json() {
    let output = execute(
        r#"
public class SerializableState {
    public Integer retained = 7;
    public transient final String ephemeral = 'secret';
}

SerializableState state = new SerializableState();
System.debug(state.ephemeral);
System.debug(JSON.serialize(state));
"#,
    )
    .unwrap();
    assert_eq!(output, ["secret", "{\"retained\":7}"]);

    let invalid_local = check("transient Integer localValue = 1;").unwrap_err();
    assert!(
        invalid_local.message.contains("local modifier `transient`"),
        "{invalid_local}"
    );
}

#[test]
fn logging_level_is_a_typed_platform_enum_with_stable_ordering() {
    let output = execute(
        r#"
System.LoggingLevel level = System.LoggingLevel.valueOf('WARN');
System.debug(level.name());
System.debug(level.ordinal());
System.debug(LoggingLevel.values().size());
System.debug(LoggingLevel.values().get(7).name());
System.debug(System.LoggingLevel.FINEST, 'leveled');
try {
    LoggingLevel.valueOf('NOT_A_LEVEL');
} catch (TypeException error) {
    System.debug(error.getTypeName());
}
"#,
    )
    .unwrap();
    assert_eq!(
        output,
        ["WARN", "2", "8", "FINEST", "leveled", "TypeException"]
    );

    let invalid = check(
        "public class InvalidLoggingLevel {
            public void assign() {
                System.LoggingLevel.ERROR = System.LoggingLevel.INFO;
            }
        }",
    )
    .unwrap_err();
    assert!(
        invalid.message.contains("constants are read-only"),
        "{invalid}"
    );
}

#[test]
fn trigger_operation_is_typed_ordered_and_switchable() {
    let output = execute(
        r#"
System.TriggerOperation operation = System.TriggerOperation.AFTER_UPDATE;
switch on operation {
    when BEFORE_INSERT, BEFORE_UPDATE {
        System.debug('before');
    }
    when AFTER_UPDATE {
        System.debug(operation.name());
        System.debug(operation.ordinal());
    }
    when else {
        System.debug('else');
    }
}
System.debug(operation == System.TriggerOperation.AFTER_UPDATE);
"#,
    )
    .unwrap();
    assert_eq!(output, ["AFTER_UPDATE", "3", "true"]);

    let invalid = check(
        "System.TriggerOperation operation = System.TriggerOperation.BEFORE_INSERT;
        switch on operation {
            when BEFORE_INSERT {}
            when System.TriggerOperation.BEFORE_INSERT {}
        }",
    )
    .unwrap_err();
    assert!(
        invalid.message.contains("duplicate scalar switch label"),
        "{invalid}"
    );
}

#[test]
fn final_locals_allow_exactly_one_assignment_per_lexical_binding() {
    let output = execute(
        r#"
final Integer initialized = 40;
final Integer assignedLater;
assignedLater = 2;
{
    Integer assignedLater = 3;
    assignedLater++;
    System.debug(assignedLater);
}
System.debug(initialized + assignedLater);
"#,
    )
    .unwrap();
    assert_eq!(output, ["4", "42"]);

    for invalid in [
        "final Integer value = 1; value = 2;",
        "final Integer value; System.debug(value);",
        "final Integer value; value = 1; value++;",
        "final final Integer value = 1;",
    ] {
        assert!(check(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn final_fields_are_assignable_in_declarations_initializers_and_constructors_only() {
    let output = execute(
        r#"
public class FinalFieldRules {
    private final Integer constructorValue;
    private final Integer declaredValue = 1;
    private static final Integer staticValue;

    {
        this.constructorValue = 2;
    }

    static {
        FinalFieldRules.staticValue = 7;
    }

    public FinalFieldRules(Boolean firstBranch) {
        if (firstBranch) {
            this.constructorValue = 3;
        } else {
            this.constructorValue = 4;
        }
        this.declaredValue = 5;
    }

    public void show() {
        System.debug(this.constructorValue);
        System.debug(this.declaredValue);
        System.debug(FinalFieldRules.staticValue);
    }
}

new FinalFieldRules(true).show();
"#,
    )
    .unwrap();
    assert_eq!(output, ["3", "5", "7"]);

    let invalid = check(
        "public class InvalidFinalFieldMutation {
            private final Integer value;
            public InvalidFinalFieldMutation() {
                this.value = 1;
            }
            public void mutate() {
                this.value = 2;
            }
        }",
    )
    .unwrap_err();
    assert!(invalid.message.contains("member `value` is read-only"));
}

#[test]
fn final_properties_support_lazy_static_accessors() {
    let output = execute(
        r#"
public class FinalPropertyRules {
    public static final String LAZY_VALUE {
        get {
            if (LAZY_VALUE == null) {
                LAZY_VALUE = 'ready';
            }
            return LAZY_VALUE;
        }
        private set;
    }
}

System.debug(FinalPropertyRules.LAZY_VALUE);
System.debug(FinalPropertyRules.LAZY_VALUE);
"#,
    )
    .unwrap();
    assert_eq!(output, ["ready", "ready"]);
}

#[test]
fn double_values_are_typed_castable_and_finite() {
    let output = execute(
        r#"
Double literalValue = 123456.0987;
Object untypedValue = JSON.deserializeUntyped('123.45');
Double castValue = (Double) untypedValue;
Double parsedValue = Double.valueOf('678.09');
Object boxedDouble = castValue;

System.debug(literalValue);
System.debug(castValue == 123.45);
System.debug(parsedValue.toString());
System.debug(boxedDouble instanceof Double);
System.debug(Double.class.getName());
System.debug(JSON.serialize(new List<Double>{ castValue, parsedValue }));
"#,
    )
    .unwrap();
    assert_eq!(
        output,
        [
            "123456.0987",
            "true",
            "678.09",
            "true",
            "Double",
            "[123.45,678.09]",
        ]
    );
}

#[test]
fn typed_json_deserialization_converts_scalars_collections_and_user_objects() {
    let output = execute(
        r#"
public class TypedJsonEnvelope {
    public String name;
    public Date created;
    public List<Double> values;
}

TypedJsonEnvelope envelope = (TypedJsonEnvelope) System.JSON.deserialize(
    '{"name":"ready","created":"2026-07-19","values":[1.25,2.5]}',
    TypedJsonEnvelope.class
);
List<String> labels = (List<String>) JSON.deserialize(
    '["first","second"]',
    List<String>.class
);

System.debug(envelope.name);
System.debug(envelope.created);
System.debug(envelope.values.get(1));
System.debug(labels.get(0));
"#,
    )
    .unwrap();
    assert_eq!(output, ["ready", "2026-07-19", "2.5", "first"]);
}

#[test]
fn visual_editor_dynamic_picklists_return_typed_rows() {
    let output = execute(
        r#"
public class LocalDynamicPicklist extends VisualEditor.DynamicPickList {
    public override VisualEditor.DataRow getDefaultValue() {
        return new VisualEditor.DataRow('Default Label', 'default-value');
    }

    public override VisualEditor.DynamicPickListRows getValues() {
        VisualEditor.DynamicPickListRows rows =
            new VisualEditor.DynamicPickListRows();
        rows.addRow(new VisualEditor.DataRow('First Label', 'first-value'));
        rows.addRow(new VisualEditor.DataRow('Second Label', 'second-value'));
        return rows;
    }
}

LocalDynamicPicklist picklist = new LocalDynamicPicklist();
System.debug(picklist.getDefaultValue().getValue());
List<VisualEditor.DataRow> rows = picklist.getValues().getDataRows();
System.debug(rows.size());
System.debug(rows.get(1).getLabel());
"#,
    )
    .unwrap();
    assert_eq!(output, ["default-value", "2", "Second Label"]);

    let invalid = check(
        "public class InvalidPicklist extends VisualEditor.DynamicPickList {
            public override String getValues() { return ''; }
        }",
    )
    .unwrap_err();
    assert!(
        invalid
            .message
            .contains("does not override an inherited method"),
        "{invalid}"
    );
}

#[test]
fn aura_enabled_options_are_validated_and_runtime_neutral() {
    let source = r#"
public class AuraSurface {
    @AuraEnabled
    public String label = 'ready';

    @AuraEnabled
    public String description { get; set; }

    @AuraEnabled(cacheable=true continuation=false)
    public static String fetch() {
        return 'ok';
    }
}
"#;
    let parsed = parse(source).unwrap();
    let aura_kinds = parsed.classes[0]
        .members
        .iter()
        .filter_map(|member| match member {
            ClassMember::Field(field) => field.annotations.first(),
            ClassMember::Property(property) => property.annotations.first(),
            ClassMember::Method(method) => method.annotations.first(),
            _ => None,
        })
        .map(|annotation| annotation.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        aura_kinds,
        [
            AnnotationKind::AuraEnabled {
                cacheable: None,
                continuation: None,
            },
            AnnotationKind::AuraEnabled {
                cacheable: None,
                continuation: None,
            },
            AnnotationKind::AuraEnabled {
                cacheable: Some(true),
                continuation: Some(false),
            },
        ]
    );
    let root = test_project("AuraSurface", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(compilation.invoke("AuraSurface.fetch").unwrap(), ["ok"]);
    fs::remove_dir_all(root).unwrap();

    for invalid in [
        "@AuraEnabled public class Invalid {}",
        "public class Invalid {
            @AuraEnabled private String hidden;
        }",
        "public class Invalid {
            @AuraEnabled(cacheable=true) public String field;
        }",
        "public class Invalid {
            @AuraEnabled public String instanceMethod() { return 'no'; }
        }",
    ] {
        assert!(check(invalid).is_err(), "{invalid}");
    }
    for invalid in [
        "public class Invalid {
            @AuraEnabled(true) public static void run() {}
        }",
        "public class Invalid {
            @AuraEnabled(cacheable='yes') public static void run() {}
        }",
        "public class Invalid {
            @AuraEnabled(unknown=true) public static void run() {}
        }",
    ] {
        assert!(parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn http_callout_mock_contract_intercepts_test_callouts_before_the_host() {
    let source = r#"
public class LocalHttpMock implements System.HttpCalloutMock {
    public System.HttpResponse respond(System.HttpRequest request) {
        System.HttpResponse response = new System.HttpResponse();
        response.setStatusCode(201);
        response.setBody(request.getEndpoint());
        return response;
    }
}
"#;
    let test_source = r#"
@IsTest
private class LocalHttpMockTest {
    @IsTest
    static void interceptsTheCallout() {
        System.Test.setMock(
            System.HttpCalloutMock.class,
            new LocalHttpMock()
        );
        HttpRequest request = new HttpRequest();
        request.setEndpoint('https://example.test/mock');
        HttpResponse response = new Http().send(request);
        System.assertEquals(201, response.getStatusCode());
        System.assertEquals(
            'https://example.test/mock',
            response.getBody()
        );
        System.assertEquals(
            'M28Alpha__c',
            Schema.getGlobalDescribe().get('M28Alpha__c').toString()
        );
    }
}
"#;
    let root = test_project(
        "LocalHttpMock",
        source,
        &[("LocalHttpMockTest", test_source)],
    );
    let compilation = project::compile(&root).unwrap();
    let report = test_runner::run(
        &compilation,
        &TestOptions {
            filter: Some("LocalHttpMockTest.interceptsTheCallout".to_owned()),
            jobs: 1,
        },
    )
    .unwrap();
    assert!(report.is_success());
    fs::remove_dir_all(root).unwrap();

    let invalid = check(
        "public class InvalidHttpMock implements System.HttpCalloutMock {
            public HttpResponse wrong(HttpRequest request) {
                return new HttpResponse();
            }
        }",
    )
    .unwrap_err();
    assert!(
        invalid
            .message
            .contains("HttpResponse respond(HttpRequest)"),
        "{invalid}"
    );
}

#[test]
fn callable_contract_and_type_reflection_dispatch_checked_runtime_identities() {
    let source = r#"
public class ReflectiveCallable implements System.Callable {
    public Object call(String action, Map<String, Object> arguments) {
        return action + ':' + arguments.get('value');
    }
}
"#;
    let test_source = r#"
@IsTest
private class ReflectiveCallableTest {
    @IsTest
    static void constructsAndDispatches() {
        System.Type callableType = System.Type.forName('ReflectiveCallable');
        System.assertEquals('ReflectiveCallable', callableType.getName());
        System.Callable callable = (System.Callable) callableType.newInstance();
        System.assertEquals(
            'run:42',
            (String) callable.call(
                'run',
                new Map<String, Object>{'value' => 42}
            )
        );

        SObject reflectedRecord = (SObject) System.Type
            .forName('Schema.M28Alpha__c')
            .newInstance();
        System.assertEquals(
            'M28Alpha__c',
            reflectedRecord.getSObjectType().toString()
        );
        System.assertEquals(null, System.Type.forName('MissingType'));
    }
}
"#;
    let root = test_project(
        "ReflectiveCallable",
        source,
        &[("ReflectiveCallableTest", test_source)],
    );
    let compilation = project::compile(&root).unwrap();
    let report = test_runner::run(
        &compilation,
        &TestOptions {
            filter: Some("ReflectiveCallableTest.constructsAndDispatches".to_owned()),
            jobs: 1,
        },
    )
    .unwrap();
    assert!(report.is_success(), "{report:?}");
    fs::remove_dir_all(root).unwrap();

    let invalid = check(
        "public class InvalidCallable implements System.Callable {
            public Object invoke(String action, Map<String, Object> arguments) {
                return null;
            }
        }",
    )
    .unwrap_err();
    assert!(
        invalid
            .message
            .contains("Object call(String, Map<String,Object>)"),
        "{invalid}"
    );
}

#[test]
fn platform_cache_partition_is_typed_and_deterministically_unavailable() {
    let source = r#"
@IsTest
private class PlatformCacheBoundaryTest {
    @IsTest
    static void exposesTypedVisibilityAndPartitionContracts() {
        Cache.Partition organization = Cache.Org.getPartition('M28');
        Cache.Partition session = Cache.Session.getPartition('M28');

        System.assertEquals('NAMESPACE', Cache.Visibility.NAMESPACE.name());
        System.assertEquals('ALL', Cache.Visibility.ALL.name());
        System.assertEquals(false, organization.isAvailable());
        System.assertEquals(false, session.contains('missing'));
        System.assertEquals(null, organization.get('missing'));

        organization.put('key', 'value');
        organization.put('key', 'value', 60, Cache.Visibility.NAMESPACE, false);
        session.remove('key');

        try {
            throw new Cache.Org.OrgCacheException('org unavailable');
        } catch (Cache.Org.OrgCacheException expected) {
            System.assertEquals('org unavailable', expected.getMessage());
        }
        try {
            throw new Cache.Session.SessionCacheException('session unavailable');
        } catch (Cache.Session.SessionCacheException expected) {
            System.assertEquals('session unavailable', expected.getMessage());
        }
    }
}
"#;
    let root = test_project("PlatformCacheBoundaryTest", source, &[]);
    let compilation = project::compile(&root).unwrap();
    let report = test_runner::run(
        &compilation,
        &TestOptions {
            filter: Some(
                "PlatformCacheBoundaryTest.exposesTypedVisibilityAndPartitionContracts".to_owned(),
            ),
            jobs: 1,
        },
    )
    .unwrap();
    assert!(report.is_success(), "{report:?}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn custom_metadata_accessors_and_bounded_deep_clone_are_typed() {
    let source = r#"
public class CustomMetadataDemo {
    public static void run() {
        Map<String, M28Setting__mdt> settings = M28Setting__mdt.getAll();
        Map<String, M28Setting__mdt> copy = settings.deepClone();
        System.debug(settings.size() + ':' + copy.size());
        System.debug(M28Setting__mdt.getInstance('Missing') == null);

        List<M28Setting__mdt> queried = [
            SELECT
                DeveloperName,
                TargetType__r.DeveloperName,
                TargetField__r.QualifiedApiName
            FROM M28Setting__mdt
            WHERE TargetType__r.DeveloperName = 'Invoice'
        ];
        for (M28Setting__mdt setting : queried) {
            System.debug(setting.TargetType__r.DeveloperName);
            System.debug(setting.TargetField__r.QualifiedApiName);
        }
        System.debug(queried.size());

        List<Integer> values = new List<Integer>{1};
        List<Integer> clonedValues = values.deepClone();
        clonedValues.add(2);
        System.debug(values.size() + ':' + clonedValues.size());

        try {
            new List<M28Alpha__c>{new M28Alpha__c()}.deepClone();
            System.assert(false);
        } catch (TypeException error) {
            System.debug(error.getMessage());
        }
    }
}
"#;
    let root = test_project("CustomMetadataDemo", source, &[]);
    write_object(
        &root.join("force-app/main/default"),
        "M28Setting__mdt",
        "M28 Setting",
    );
    let fields = root.join("force-app/main/default/objects/M28Setting__mdt/fields");
    fs::create_dir_all(&fields).unwrap();
    fs::write(
        fields.join("TargetType__c.field-meta.xml"),
        "<CustomField><fullName>TargetType__c</fullName><referenceTo>EntityDefinition</referenceTo><type>MetadataRelationship</type></CustomField>",
    )
    .unwrap();
    fs::write(
        fields.join("TargetField__c.field-meta.xml"),
        "<CustomField><fullName>TargetField__c</fullName><metadataRelationshipControllingField>M28Setting__mdt.TargetType__c</metadataRelationshipControllingField><referenceTo>FieldDefinition</referenceTo><type>MetadataRelationship</type></CustomField>",
    )
    .unwrap();
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("CustomMetadataDemo.run").unwrap(),
        [
            "0:0",
            "true",
            "0",
            "1:2",
            "deepClone currently requires scalar or empty collections",
        ]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn schema_tokens_describe_fields_and_field_sets_are_typed_and_structural() {
    let source = r#"
public class SchemaDescribeDemo {
    public static void run() {
        Schema.SObjectType directType = Schema.M28Alpha__c.SObjectType;
        Schema.SObjectType globalType = Schema.getGlobalDescribe().get('M28Alpha__c');
        System.assertEquals(directType, globalType);

        Schema.SObjectField directField = Schema.M28Alpha__c.Code__c;
        Schema.DescribeSObjectResult objectDescribe = directType.getDescribe();
        Schema.SObjectField mappedField = objectDescribe.fields.getMap().get('Code__c');
        System.assertEquals(directField, mappedField);

        Schema.DescribeFieldResult fieldDescribe = directField.getDescribe();
        System.assertEquals('Code__c', fieldDescribe.getName());
        System.assertEquals('Code', fieldDescribe.getLabel());
        System.assertEquals(12, fieldDescribe.getLength());
        System.assertEquals(Schema.SoapType.STRING, fieldDescribe.getSoapType());
        System.assertEquals(Schema.DisplayType.STRING, fieldDescribe.getType());

        SObject record = directType.newSObject();
        record.put(directField, 'value');
        System.assertEquals('value', record.get(mappedField));

        Schema.FieldSet fieldSet = objectDescribe.fieldSets.getMap().get('Summary');
        System.assertEquals('Summary Fields', fieldSet.getLabel());
        List<Schema.FieldSetMember> members = fieldSet.getFields();
        System.assertEquals(1, members.size());
        System.assertEquals(directField, members[0].getSObjectField());

        Map<Schema.SObjectType, Schema.SObjectField> tokenMap =
            new Map<Schema.SObjectType, Schema.SObjectField>{ directType => directField };
        System.debug(
            tokenMap.get(globalType).getDescribe().getLocalName()
            + ':' + members[0].getFieldPath()
        );
    }
}
"#;
    let root = test_project("SchemaDescribeDemo", source, &[]);
    let object = root.join("force-app/main/default/objects/M28Alpha__c");
    fs::create_dir_all(object.join("fields")).unwrap();
    fs::write(
        object.join("fields/Code__c.field-meta.xml"),
        "<CustomField><fullName>Code__c</fullName><label>Code</label><length>12</length><type>Text</type></CustomField>",
    )
    .unwrap();
    fs::create_dir_all(object.join("fieldSets")).unwrap();
    fs::write(
        object.join("fieldSets/Summary.fieldSet-meta.xml"),
        "<FieldSet><fullName>Summary</fullName><displayedFields><field>Code__c</field><isRequired>false</isRequired></displayedFields><label>Summary Fields</label></FieldSet>",
    )
    .unwrap();
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("SchemaDescribeDemo.run").unwrap(),
        ["Code__c:Code__c"]
    );
    fs::remove_dir_all(root).unwrap();
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
fn scalar_switch_dispatches_string_and_enum_labels_with_single_evaluation() {
    let source = r#"
public class ScalarSwitchDemo {
    private enum Mode {
        FIRST,
        SECOND
    }

    private static Integer evaluations = 0;

    private static String selected() {
        evaluations++;
        return 'second';
    }

    public static void run() {
        switch on selected() {
            when 'first' {
                System.debug('wrong');
            }
            when 'second' {
                System.debug('string:' + evaluations);
            }
            when else {
                System.debug('else');
            }
        }

        Mode mode = Mode.SECOND;
        switch on mode {
            when FIRST {
                System.debug('wrong');
            }
            when SECOND {
                System.debug('enum:' + mode.name());
            }
        }
    }
}
"#;
    let root = test_project("ScalarSwitchDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("ScalarSwitchDemo.run").unwrap(),
        ["string:1", "enum:SECOND"]
    );
    fs::remove_dir_all(root).unwrap();

    for invalid in [
        "public class InvalidSwitch {
            public static void run() {
                switch on 'x' {
                    when 'x' {}
                    when 'x' {}
                }
            }
        }",
        "public class InvalidSwitch {
            private enum Mode { ONE }
            public static void run() {
                switch on Mode.ONE {
                    when TWO {}
                }
            }
        }",
    ] {
        assert!(check(invalid).is_err(), "{invalid}");
    }
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
            "scalar switch requires String, Integer, Long, or enum",
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
        Object cacheValue = 'sentinel';

        System.debug(first == copy);
        System.debug(cacheValue == 'sentinel');
        cacheValue = 5;
        System.debug(cacheValue == 'sentinel');
        System.debug('sentinel' == cacheValue);
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
            "true", "true", "false", "false", "true", "false", "true", "true", "false", "true",
            "true", "1"
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
fn network_context_and_governor_counters_are_explicit_and_measured() {
    let checked = check(
        r#"
        System.debug(System.Network.getNetworkId());
        System.debug(System.Network.getLoginUrl(System.Network.getNetworkId()));
        System.debug(System.Network.getLogoutUrl(System.Network.getNetworkId()));
        System.debug(System.Network.getSelfRegUrl(System.Network.getNetworkId()));
        System.debug(System.Limits.getLimitAggregateQueries());
        System.debug(System.Limits.getLimitFetchCallsOnApexCursor());
        System.debug(System.Limits.getLimitApexCursorRows());
        System.debug(System.Limits.getLimitAsyncCalls());
        System.debug(System.Limits.getLimitCallouts());
        System.debug(System.Limits.getLimitCpuTime());
        System.debug(System.Limits.getLimitDmlRows());
        System.debug(System.Limits.getLimitDmlStatements());
        System.debug(System.Limits.getLimitEmailInvocations());
        System.debug(System.Limits.getLimitFutureCalls());
        System.debug(System.Limits.getLimitHeapSize());
        System.debug(System.Limits.getLimitMobilePushApexCalls());
        System.debug(System.Limits.getLimitPublishImmediateDML());
        System.debug(System.Limits.getLimitQueries());
        System.debug(System.Limits.getLimitQueryLocatorRows());
        System.debug(System.Limits.getLimitQueryRows());
        System.debug(System.Limits.getLimitQueueableJobs());
        System.debug(System.Limits.getLimitSoslQueries());
        "#,
    )
    .unwrap();
    let mut host = RecordingHost::default();
    host.set_network_context(Some(NetworkContext {
        network_id: "0DB000000000001".to_owned(),
        login_url: Some("https://community.example.test/login".to_owned()),
        logout_url: Some("https://community.example.test/logout".to_owned()),
        self_registration_url: None,
    }));
    let output = Interpreter::with_host(&mut host).execute(&checked).unwrap();
    assert_eq!(
        output,
        [
            "0DB000000000001",
            "https://community.example.test/login",
            "https://community.example.test/logout",
            "null",
            "300",
            "100",
            "50000000",
            "200",
            "100",
            "10000",
            "10000",
            "150",
            "10",
            "50",
            "6000000",
            "10",
            "150",
            "100",
            "10000",
            "50000",
            "50",
            "20",
        ]
    );

    let error =
        execute("System.debug(System.Network.getLoginUrl('0DB000000000001'));").unwrap_err();
    assert_eq!(error.exception_type.as_deref(), Some("TypeException"));
    assert!(
        error
            .message
            .contains("requires a configured network context"),
        "{}",
        error.message
    );

    let source = r#"
public class LimitsUsageDemo {
    public static void run() {
        insert new List<M28Alpha__c>{
            new M28Alpha__c(Name = 'First'),
            new M28Alpha__c(Name = 'Second')
        };
        List<M28Alpha__c> rows = [SELECT Id FROM M28Alpha__c ORDER BY Name];
        System.debug(rows.size());
        System.debug(System.Limits.getDmlStatements());
        System.debug(System.Limits.getDmlRows());
        System.debug(System.Limits.getQueries());
        System.debug(System.Limits.getQueryRows());
        System.debug(System.Limits.getSoslQueries());
    }
}
"#;
    let root = test_project("LimitsUsageDemo", source, &[]);
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("LimitsUsageDemo.run").unwrap(),
        ["2", "1", "2", "1", "2", "0"]
    );
    fs::remove_dir_all(root).unwrap();
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
