use apex_exec::project::ProjectCompiler;
use apex_exec::{check, execute, parse};
use std::{fs, path::PathBuf, process::Command};

const SERVICE_LAYER: &str = "
public interface Greeter {
    String greet();
}

public virtual class BaseCounter {
    protected Integer value = 1;

    public virtual Integer next() {
        value++;
        return value;
    }
}

public class Counter extends BaseCounter implements Greeter {
    public static Integer made = 0;
    public String Label { get; private set; }

    public Counter(Integer start) {
        value = start;
        Label = 'count';
        made++;
    }

    public override Integer next() {
        return super.next() + 1;
    }

    public String greet() {
        return Label + ':' + String.valueOf(next());
    }

    public static Integer total() {
        return made;
    }
}

Counter counter = new Counter(2);
System.debug(counter.greet());
System.debug(Counter.total());
";

#[test]
fn parses_checks_and_executes_classes_across_the_full_pipeline() {
    let parsed = parse(SERVICE_LAYER).unwrap();
    assert_eq!(parsed.classes.len(), 3);
    assert_eq!(parsed.statements.len(), 3);

    let checked = check(SERVICE_LAYER).unwrap();
    assert_eq!(checked.classes.len(), 3);
    assert_eq!(execute(SERVICE_LAYER).unwrap(), ["count:4", "1"]);
}

#[test]
fn class_names_members_and_overrides_are_case_insensitive() {
    let source = "
        public virtual class Parent {
            public virtual String Name() { return 'parent'; }
        }
        public class Child extends pArEnT {
            public override String nAmE() { return 'child'; }
        }
        PARENT value = new CHILD();
        System.debug(value.NAME());
        Child child = (Child) value;
        System.debug(child.name());
        Parent plain = new Parent();
        try {
            Child invalid = (Child) plain;
        } catch (TypeException error) {
            System.debug(error.getTypeName());
        }
    ";
    assert_eq!(
        execute(source).unwrap(),
        ["child", "child", "TypeException"]
    );
}

#[test]
fn executes_overloaded_constructors_and_custom_property_accessors() {
    let source = "
        public class BoxedInteger {
            private Integer stored;

            public Integer Value {
                get { return stored; }
                set { stored = value; }
            }

            public BoxedInteger() { Value = 1; }
            public BoxedInteger(Integer start) { Value = start; }
        }

        BoxedInteger first = new BoxedInteger();
        BoxedInteger second = new BoxedInteger(4);
        System.debug(first.Value);
        System.debug(second.Value++);
        System.debug(second.Value);
    ";

    assert_eq!(execute(source).unwrap(), ["1", "4", "5"]);
}

#[test]
fn enforces_access_static_and_abstract_contracts() {
    let private_setter = "
        public class AccountService {
            public String Status { get; private set; }
        }
        AccountService service = new AccountService();
        service.Status = 'ready';
    ";
    assert_eq!(
        check(private_setter).unwrap_err().message,
        "member is not accessible"
    );

    let static_mismatch = "
        public class Worker {
            public Integer value = 1;
            public static Integer read() { return value; }
        }
    ";
    assert_eq!(
        check(static_mismatch).unwrap_err().message,
        "instance member `value` is unavailable in a static context"
    );

    let missing_interface = "
        public interface Work { void run(); }
        public class Worker implements Work {}
    ";
    assert_eq!(
        check(missing_interface).unwrap_err().message,
        "non-abstract class `Worker` must implement method `run`"
    );

    let abstract_construction = "
        public abstract class Job { public abstract void run(); }
        Job job = new Job();
    ";
    assert_eq!(
        check(abstract_construction).unwrap_err().message,
        "cannot construct abstract type `Job`"
    );

    let sharing = "public with sharing class SharedService {}";
    assert_eq!(
        check(sharing).unwrap_err().message,
        "sharing modifiers are parsed but not supported by the active compatibility profile"
    );
}

#[test]
fn rejects_invalid_override_and_inheritance_cycles() {
    let non_virtual = "
        public virtual class Parent { public String name() { return 'p'; } }
        public class Child extends Parent {
            public override String name() { return 'c'; }
        }
    ";
    assert_eq!(
        check(non_virtual).unwrap_err().message,
        "method `name` overrides a non-virtual method"
    );

    let sealed_parent = "
        public class Parent {}
        public class Child extends Parent {}
    ";
    assert_eq!(
        check(sealed_parent).unwrap_err().message,
        "cannot extend non-virtual class `Parent`"
    );

    let cycle = "
        public virtual class Left extends Right {}
        public virtual class Right extends Left {}
    ";
    assert_eq!(
        check(cycle).unwrap_err().message,
        "cyclic inheritance involving `Left`"
    );
}

#[test]
fn discovers_compiles_invokes_and_incrementally_rechecks_an_sfdx_project() {
    let root = temporary_project("incremental");
    let classes = root.join("force-app/main/default/classes");
    fs::create_dir_all(&classes).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}]}"#,
    )
    .unwrap();
    fs::write(
        classes.join("Sequence.cls"),
        "public interface Sequence { Integer next(); }",
    )
    .unwrap();
    fs::write(
        classes.join("Counter.cls"),
        "public class Counter implements Sequence {
            private Integer value;
            public Counter(Integer start) { value = start; }
            public Integer next() { value++; return value; }
        }",
    )
    .unwrap();
    fs::write(
        classes.join("Entry.cls"),
        "public class Entry {
            public static String run() {
                Sequence counter = new Counter(4);
                return String.valueOf(counter.next());
            }
        }",
    )
    .unwrap();

    let mut compiler = ProjectCompiler::new();
    let first = compiler.compile(&root).unwrap();
    assert_eq!(first.incremental.parsed_files.len(), 3);
    assert_eq!(first.invoke("Entry.run").unwrap(), ["5"]);

    let entry = classes.join("Entry.cls");
    let counter = classes.join("Counter.cls");
    assert!(
        first
            .dependencies
            .dependencies_of(&entry)
            .unwrap()
            .contains(&counter)
    );

    let cli_check = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["check", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(cli_check.status.success());
    assert_eq!(
        String::from_utf8(cli_check.stdout).unwrap(),
        "OK (3 classes, 3 source files)\n"
    );
    let cli_invoke = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["invoke", root.to_str().unwrap(), "Entry.run"])
        .output()
        .unwrap();
    assert!(cli_invoke.status.success());
    assert_eq!(String::from_utf8(cli_invoke.stdout).unwrap(), "5\n");

    let unchanged = compiler.compile(&root).unwrap();
    assert!(unchanged.incremental.parsed_files.is_empty());
    assert_eq!(unchanged.incremental.reused_files.len(), 3);

    fs::write(
        &counter,
        "public class Counter implements Sequence {
            private Integer value;
            public Counter(Integer start) { value = start + 1; }
            public Integer next() { value++; return value; }
        }",
    )
    .unwrap();
    let changed = compiler.compile(&root).unwrap();
    assert_eq!(changed.incremental.parsed_files, [counter.clone()]);
    assert!(changed.incremental.invalidated_files.contains(&counter));
    assert!(changed.incremental.invalidated_files.contains(&entry));
    assert_eq!(changed.invoke("Entry.run").unwrap(), ["6"]);

    fs::remove_dir_all(root).unwrap();
}

fn temporary_project(label: &str) -> PathBuf {
    let unique = format!(
        "apex-exec-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    std::env::temp_dir().join(unique)
}
