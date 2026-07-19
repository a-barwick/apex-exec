use apex_exec::{
    check, execute, parse, project,
    test_runner::{TestOptions, run as run_tests},
};
use std::{path::Path, process::Command};

const PROJECT: &str = "examples/milestone20-project";

#[test]
fn type_references_preserve_qualified_segments_arguments_arrays_and_spans() {
    let source = "public class Owner implements pkg.Contracts.Work<List<Integer[]>> {}";
    let program = parse(source).unwrap();
    let interface = &program.classes[0].interfaces[0];
    let syntax = interface
        .syntax
        .as_ref()
        .expect("source syntax is retained");
    assert_eq!(
        syntax
            .segments
            .iter()
            .map(|segment| segment.spelling.as_str())
            .collect::<Vec<_>>(),
        ["pkg", "Contracts", "Work"]
    );
    assert_eq!(syntax.type_arguments.len(), 1);
    assert_eq!(syntax.type_arguments[0].type_arguments.len(), 1);
    assert_eq!(
        syntax.type_arguments[0].type_arguments[0]
            .array_suffixes
            .len(),
        1
    );
    assert_eq!(
        &source[syntax.span.start..syntax.span.end],
        "pkg.Contracts.Work<List<Integer[]>>"
    );
}

#[test]
fn hierarchy_identity_rejects_duplicates_and_conflicting_contracts() {
    let duplicate = check(
        r#"
        public interface Work { void run(); }
        public class Worker implements Work, wOrK { public void run() {} }
        "#,
    )
    .unwrap_err();
    assert!(duplicate.message.contains("more than once"));

    let conflict = check(
        r#"
        public interface Left { String value(); }
        public interface Right { Integer value(); }
        public class Broken implements Left, Right {
            public String value() { return 'x'; }
        }
        "#,
    )
    .unwrap_err();
    assert!(conflict.message.contains("conflicting return types"));
}

#[test]
fn nested_types_initializers_constructor_chaining_and_dispatch_execute_together() {
    let output = execute(
        r#"
        public class Outer {
            public static Integer staticOrder = 1;
            static { staticOrder *= 10; }

            public Integer value = 2;
            { value += 3; }

            public Outer() {
                this(4);
                value += 1;
            }
            public Outer(Integer seed) { value += seed; }

            public virtual class Base {
                public virtual Integer score() { return 1; }
            }
            public class Worker extends Base {
                public override Integer score() { return 7; }
            }
            public class Inner {
                public static Integer read() { return Outer.staticOrder; }
            }
        }

        Outer constructed = new Outer();
        Outer.Base worker = new Outer.Worker();
        System.debug(constructed.value);
        System.debug(worker.score());
        System.debug(Outer.Inner.read());
        "#,
    )
    .unwrap();
    assert_eq!(output, ["10", "7", "10"]);
}

#[test]
fn qualified_nested_identities_do_not_collide_or_flatten() {
    let output = execute(
        r#"
        public class First {
            public class Item {
                public Integer value() { return 1; }
            }
        }
        public class Second {
            public class Item {
                public Integer value() { return 2; }
            }
        }
        First.Item left = new First.Item();
        Second.Item right = new Second.Item();
        System.debug(left.value());
        System.debug(right.value());
        "#,
    )
    .unwrap();
    assert_eq!(output, ["1", "2"]);
}

#[test]
fn enums_support_constants_identity_methods_values_and_value_of() {
    let output = execute(
        r#"
        public enum Mode { FAST, Safe }
        Mode first = Mode.FAST;
        Mode second = Mode.valueOf('Safe');
        System.debug(first);
        System.debug(first.name());
        System.debug(second.ordinal());
        System.debug(Mode.values().size());
        System.debug(second == Mode.Safe);
        "#,
    )
    .unwrap();
    assert_eq!(output, ["Mode.FAST", "FAST", "1", "2", "true"]);

    let error = execute("public enum Mode { FAST } Mode.valueOf('fast');").unwrap_err();
    assert_eq!(
        error.exception_type.as_deref(),
        Some("IllegalArgumentException")
    );
}

#[test]
fn class_literals_cover_qualified_array_and_generic_types() {
    let output = execute(
        r#"
        public class Outer { public class Inner {} }
        Type nestedType = Outer.Inner.class;
        Type arrayType = Integer[].class;
        Type genericType = List<String>.class;
        System.debug(nestedType);
        System.debug(arrayType);
        System.debug(genericType);
        "#,
    )
    .unwrap();
    assert_eq!(output, ["Outer.Inner", "List<Integer>", "List<String>"]);
}

#[test]
fn custom_exception_subclasses_are_typed_catchable_and_preserve_messages() {
    let output = execute(
        r#"
        public class LocalFailure extends Exception {}
        try {
            throw new LocalFailure('broken');
        } catch (LocalFailure failure) {
            System.debug(failure.getTypeName());
            System.debug(failure.getMessage());
        }
        "#,
    )
    .unwrap();
    assert_eq!(output, ["LocalFailure", "broken"]);
}

#[test]
fn nested_access_and_constructor_delegation_fail_explicitly() {
    let access = check(
        r#"
        public class Owner { private class Hidden {} }
        Owner.Hidden leaked = new Owner.Hidden();
        "#,
    )
    .unwrap_err();
    assert!(access.message.contains("not accessible"));

    let cycle = check(
        r#"
        public class Cycle {
            public Cycle() { this(1); }
            public Cycle(Integer value) { this(); }
        }
        "#,
    )
    .unwrap_err();
    assert!(cycle.message.contains("cyclic"));
}

#[test]
fn explicit_super_constructor_chaining_preserves_base_initialization() {
    let output = execute(
        r#"
        public virtual class Base {
            public Integer value;
            public Base(Integer value) { this.value = value; }
        }
        public class Child extends Base {
            public Child() { super(9); }
        }
        Child child = new Child();
        System.debug(child.value);
        "#,
    )
    .unwrap();
    assert_eq!(output, ["9"]);
}

#[test]
fn milestone_project_invocation_tests_coverage_and_cli_are_complete() {
    let compilation = project::compile(PROJECT).unwrap();
    assert_eq!(
        compilation.invoke("NestedCompatibility.run").unwrap(),
        ["Running|1|3|7|NestedCompatibility.Job|1"]
    );
    let classes = Path::new(PROJECT).join("force-app/main/default/classes");
    let test_class = classes.join("NestedCompatibilityTest.cls");
    assert!(
        compilation
            .dependencies
            .dependencies_of(&test_class)
            .unwrap()
            .contains(&classes.join("NestedCompatibility.cls"))
    );

    let report = run_tests(&compilation, &TestOptions::default()).unwrap();
    assert!(report.is_success(), "{}", report.render_console());
    assert_eq!(report.tests.len(), 2);
    assert_eq!(report.coverage.covered_lines, report.coverage.total_lines);

    let binary = env!("CARGO_BIN_EXE_apex-exec");
    for arguments in [
        vec!["check", PROJECT],
        vec!["invoke", PROJECT, "NestedCompatibility.run"],
        vec!["test", PROJECT],
    ] {
        let output = Command::new(binary).args(&arguments).output().unwrap();
        assert!(
            output.status.success(),
            "CLI {:?} failed: {}",
            arguments,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
