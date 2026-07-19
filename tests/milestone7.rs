use apex_exec::{
    platform::{DataValue, Record, RecordId, SqliteStorage, Storage, StorageTransaction},
    project::{self, ProjectCompiler},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[test]
fn example_project_imports_schema_compiles_and_executes_typed_and_dynamic_sobjects() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/milestone7-project");
    let compilation = project::compile(&root).unwrap();
    let invoice = compilation.schema.object("invoice__C").unwrap();
    assert_eq!(invoice.fields().len(), 6);
    assert_eq!(
        compilation.invoke("InvoiceDemo.run").unwrap(),
        ["Approved", "125"]
    );

    let output = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["invoke", root.to_str().unwrap(), "InvoiceDemo.run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "Approved\n125\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn schema_backed_apex_rejects_unknown_fields_and_static_type_mismatches() {
    let root = test_project(
        "public class InvoiceService {
            public static void run() {
                Invoice__c invoice = new Invoice__c();
                invoice.Missing__c = 'value';
            }
        }",
        "Number",
    );
    let error = project::compile(&root).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unknown field `Missing__c` on SObject `Invoice__c`")
    );

    fs::write(
        class_path(&root),
        "public class InvoiceService {
            public static void run() {
                Invoice__c invoice = new Invoice__c();
                invoice.Amount__c = 'not an integer';
            }
        }",
    )
    .unwrap();
    let error = project::compile(&root).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot assign String to Integer")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dynamic_sobjects_validate_api_names_and_fields_at_runtime() {
    let root = test_project(
        "public class InvoiceService {
            public static void run() {
                SObject invoice = new SObject('Invoice__c');
                invoice.put('Missing__c', 1);
            }
        }",
        "Number",
    );
    let compilation = project::compile(&root).unwrap();
    let error = compilation.invoke("InvoiceService.run").unwrap_err();
    assert!(error.to_string().contains("unknown field `Missing__c`"));

    fs::write(
        class_path(&root),
        "public class InvoiceService {
            public static void run() {
                SObject invoice = new SObject('Missing__c');
            }
        }",
    )
    .unwrap();
    let compilation = project::compile(&root).unwrap();
    let error = compilation.invoke("InvoiceService.run").unwrap_err();
    assert!(error.to_string().contains("unknown SObject `Missing__c`"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sobjects_alias_typed_nulls_increment_fields_and_reject_dynamic_type_errors() {
    let root = test_project(
        "public class InvoiceService {
            private static Invoice__c current = new Invoice__c();

            public static void run() {
                System.debug(current.Amount__c);
                current.Amount__c = 1;
                Invoice__c alias = current;
                alias.Amount__c++;
                System.debug(current.Amount__c);

                SObject dynamicInvoice = current;
                try {
                    dynamicInvoice.put('Amount__c', 'wrong');
                } catch (TypeException error) {
                    System.debug(error.getTypeName());
                }
            }
        }",
        "Number",
    );
    let compilation = project::compile(&root).unwrap();
    assert_eq!(
        compilation.invoke("InvoiceService.run").unwrap(),
        ["null", "2", "TypeException"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn imported_schema_drives_sqlite_crud_transactions_and_fixture_reset() {
    let root = test_project(
        "public class InvoiceService { public static void run() {} }",
        "Number",
    );
    let compilation = project::compile(&root).unwrap();
    let invoice = compilation.schema.object("Invoice__c").unwrap();
    let id = RecordId::generate(invoice.key_prefix(), 1).unwrap();
    let mut record = Record::new("Invoice__c", id.clone());
    record.set_field("Name", "INV-0001");
    record.set_field("Amount__c", 125_i64);
    let mut storage = SqliteStorage::in_memory(compilation.schema.clone()).unwrap();
    storage.load_fixture([record]).unwrap();

    let mut transaction = storage.begin_transaction().unwrap();
    let stored = transaction.read("invoice__c", &id).unwrap().unwrap();
    assert_eq!(stored.field("amount__C"), Some(&DataValue::Integer(125)));
    transaction.savepoint("test_case").unwrap();
    assert!(transaction.delete("Invoice__c", &id).unwrap());
    transaction.rollback_to("test_case").unwrap();
    transaction.release_savepoint("test_case").unwrap();
    assert!(transaction.read("Invoice__c", &id).unwrap().is_some());
    transaction.rollback().unwrap();

    storage.reset().unwrap();
    let mut empty = storage.begin_transaction().unwrap();
    assert!(empty.read("Invoice__c", &id).unwrap().is_none());
    empty.commit().unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn incremental_compiler_observes_metadata_only_changes() {
    let root = test_project(
        "public class InvoiceService {
            public static void run() {
                Invoice__c invoice = new Invoice__c();
                invoice.Amount__c = 42;
            }
        }",
        "Number",
    );
    let mut compiler = ProjectCompiler::new();
    let initial = compiler.compile(&root).unwrap();
    assert_eq!(initial.incremental.parsed_files.len(), 1);
    let cached = compiler.compile(&root).unwrap();
    assert!(cached.incremental.parsed_files.is_empty());

    fs::write(field_path(&root), field_metadata("Text")).unwrap();
    let error = compiler.compile(&root).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot assign Integer to String")
    );
    fs::remove_dir_all(root).unwrap();
}

fn test_project(class_source: &str, amount_type: &str) -> PathBuf {
    let root = temp_directory();
    let classes = root.join("force-app/main/default/classes");
    let object = root.join("force-app/main/default/objects/Invoice__c");
    fs::create_dir_all(&classes).unwrap();
    fs::create_dir_all(object.join("fields")).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}],"sourceApiVersion":"66.0"}"#,
    )
    .unwrap();
    fs::write(class_path(&root), class_source).unwrap();
    fs::write(
        object.join("Invoice__c.object-meta.xml"),
        r#"<CustomObject><nameField><type>AutoNumber</type></nameField></CustomObject>"#,
    )
    .unwrap();
    fs::write(field_path(&root), field_metadata(amount_type)).unwrap();
    root
}

fn field_metadata(field_type: &str) -> String {
    let number_details = if field_type == "Number" {
        "<precision>18</precision><scale>0</scale>"
    } else {
        "<length>80</length>"
    };
    format!(
        "<CustomField><fullName>Amount__c</fullName>{number_details}<required>true</required><type>{field_type}</type></CustomField>"
    )
}

fn class_path(root: &Path) -> PathBuf {
    root.join("force-app/main/default/classes/InvoiceService.cls")
}

fn field_path(root: &Path) -> PathBuf {
    root.join("force-app/main/default/objects/Invoice__c/fields/Amount__c.field-meta.xml")
}

fn temp_directory() -> PathBuf {
    let unique = format!(
        "apex-exec-m7-{}-{}-{}",
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
