use apex_exec::{
    check, execute, project,
    runtime::Interpreter,
    test_runner::{self, TestOptions},
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn ordinary_and_debug_entry_points_are_not_test_contexts() {
    assert_eq!(
        execute("System.debug(Test.isRunningTest());").unwrap(),
        ["false"]
    );

    let program = check("System.debug(Test.isRunningTest());").unwrap();
    let debug = Interpreter::new().debug_execute(&program);
    assert!(debug.diagnostic.is_none());
    assert_eq!(debug.output, ["false"]);
}

#[test]
fn queued_work_inherits_test_mode_and_restores_it_after_success_or_failure() {
    let root = context_test_project();
    let compilation = project::compile(&root).unwrap();
    let report = test_runner::run(
        &compilation,
        &TestOptions {
            filter: None,
            jobs: 1,
        },
    )
    .unwrap();

    assert_eq!(report.tests.len(), 2);
    assert!(
        report.is_success(),
        "test-mode regressions failed: {:#?}",
        report.tests
    );
}

#[test]
fn queued_work_from_an_ordinary_entry_inherits_non_test_mode() {
    let output = execute(
        r#"
        public class OrdinaryContextJob implements Queueable {
            public void execute(QueueableContext context) {
                System.debug(Test.isRunningTest());
            }
        }

        System.enqueueJob(new OrdinaryContextJob());
        Test.stopTest();
        "#,
    )
    .unwrap();

    assert_eq!(output, ["false"]);
}

#[test]
fn unused_classes_are_lazy_and_successful_initializers_run_once() {
    let output = execute(
        r#"
        public class InitCounter {
            public static Integer calls = 0;
            public static Integer next() {
                calls++;
                return calls;
            }
        }

        public class UsedClass {
            public static Integer first = InitCounter.next();
            public static Integer second = InitCounter.next();
            public static Integer read() {
                return first + second;
            }
        }

        public class UnusedBrokenClass {
            public static Integer broken = 1 / 0;
        }

        System.debug(UsedClass.read());
        System.debug(UsedClass.read());
        System.debug(InitCounter.calls);
        "#,
    )
    .unwrap();

    assert_eq!(output, ["3", "3", "2"]);
}

#[test]
fn failed_initialization_is_cached_and_not_retried() {
    let output = execute(
        r#"
        public class FailureCounter {
            public static Integer attempts = 0;
            public static Integer fail() {
                attempts++;
                return 1 / 0;
            }
        }

        public class BrokenClass {
            public static Integer value = FailureCounter.fail();
        }

        try {
            System.debug(BrokenClass.value);
        } catch (MathException error) {
            System.debug(error.getTypeName());
        }
        try {
            System.debug(BrokenClass.value);
        } catch (MathException error) {
            System.debug(error.getTypeName());
        }
        System.debug(FailureCounter.attempts);
        "#,
    )
    .unwrap();

    assert_eq!(output, ["MathException", "MathException", "1"]);
}

#[test]
fn cross_class_cycles_are_typed_bounded_and_catchable() {
    let output = execute(
        r#"
        public class CycleA {
            public static Integer value = CycleB.value;
        }

        public class CycleB {
            public static Integer value = CycleA.value;
        }

        try {
            System.debug(CycleA.value);
        } catch (TypeException error) {
            System.debug(error.getMessage());
        }
        try {
            System.debug(CycleA.value);
        } catch (TypeException error) {
            System.debug(error.getTypeName());
        }
        System.debug('continued');
        "#,
    )
    .unwrap();

    assert_eq!(
        output,
        [
            "circular static initialization: CycleA -> CycleB -> CycleA",
            "TypeException",
            "continued",
        ]
    );
}

#[test]
fn deep_static_dependency_chains_have_a_catchable_depth_limit() {
    let mut source = String::new();
    for index in 0..300 {
        source.push_str(&format!(
            "public class Deep{index} {{ public static Integer value = Deep{}.value; }} ",
            index + 1
        ));
    }
    source.push_str(
        "public class Deep300 { public static Integer value = 1; } \
         try { System.debug(Deep0.value); } \
         catch (TypeException error) { System.debug(error.getTypeName()); } \
         System.debug('continued');",
    );

    assert_eq!(execute(&source).unwrap(), ["TypeException", "continued"]);
}

#[test]
fn a_caught_dependency_cycle_does_not_poison_the_owner_initializer() {
    let output = execute(
        r#"
        public class RecoveringA {
            public static Integer value = recover();
            public static Integer recover() {
                try {
                    return FailedB.value;
                } catch (TypeException error) {
                    return 7;
                }
            }
        }

        public class FailedB {
            public static Integer value = RecoveringA.value;
        }

        System.debug(RecoveringA.value);
        System.debug(RecoveringA.value);
        try {
            System.debug(FailedB.value);
        } catch (TypeException error) {
            System.debug(error.getTypeName());
        }
        "#,
    )
    .unwrap();

    assert_eq!(output, ["7", "7", "TypeException"]);
}

#[test]
fn lazy_initialization_preserves_operand_order_and_preallocates_static_slots() {
    let output = execute(
        r#"
        public class OrderCounter {
            public static Integer calls = 0;
            public static Integer next() {
                calls++;
                return calls;
            }
        }

        public class StaticFieldOrder {
            public static Integer marker = OrderCounter.next();
            public static Integer value = 0;
        }

        public class StaticCallOrder {
            public static Integer marker = OrderCounter.next();
            public static Integer echo(Integer value) {
                return value;
            }
        }

        public class ConstructorOrder {
            public static Integer marker = OrderCounter.next();
            public Integer value;
            public ConstructorOrder(Integer value) {
                this.value = value;
            }
        }

        public class StaticPropertyOrder {
            public static Integer marker = OrderCounter.next();
            public static Integer Value { get; set; }
        }

        public class Defaults {
            public static Integer first = second;
            public static Integer second = 5;
            public static Integer Value { get; set; }
            public static Integer observed = Value;
        }

        public class StaticMutation {
            public static Integer value = 10;
        }

        public virtual class BaseInitialization {
            public static Integer marker = OrderCounter.next();
        }

        public class ChildInitialization extends BaseInitialization {
            public static Integer marker = OrderCounter.next();
        }

        StaticFieldOrder.value = OrderCounter.next();
        System.debug(StaticFieldOrder.value);
        System.debug(StaticFieldOrder.marker);

        System.debug(StaticCallOrder.echo(OrderCounter.next()));
        System.debug(StaticCallOrder.marker);

        ConstructorOrder instance = new ConstructorOrder(OrderCounter.next());
        System.debug(instance.value);
        System.debug(ConstructorOrder.marker);

        StaticPropertyOrder.Value = OrderCounter.next();
        System.debug(StaticPropertyOrder.Value);
        System.debug(StaticPropertyOrder.marker);

        System.debug(Defaults.first == null);
        System.debug(Defaults.second);
        System.debug(Defaults.observed == null);

        System.debug(StaticMutation.value++);
        System.debug(StaticMutation.value);

        System.debug(ChildInitialization.marker);
        System.debug(BaseInitialization.marker);
        "#,
    )
    .unwrap();

    assert_eq!(
        output,
        [
            "1", "2", "3", "4", "5", "6", "7", "8", "true", "5", "true", "10", "11", "10", "9",
        ]
    );
}

fn context_test_project() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("apex-exec-s0-04-context-{unique}"));
    let classes = root.join("force-app/main/default/classes");
    fs::create_dir_all(&classes).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}]}"#,
    )
    .unwrap();
    fs::write(
        classes.join("AsyncContextProbe.cls"),
        r#"
        public class AsyncContextProbe {
            public static Integer observations = 0;

            @future
            public static void observe() {
                System.assert(Test.isRunningTest());
                observations++;
            }

            @future
            public static void fail() {
                System.assert(Test.isRunningTest());
                Integer broken = 1 / 0;
            }
        }
        "#,
    )
    .unwrap();
    fs::write(
        classes.join("ContextModeTest.cls"),
        r#"
        @IsTest
        private class ContextModeTest {
            private static Boolean initializedInTest = Test.isRunningTest();

            @IsTest
            static void inheritsAndRestoresTestMode() {
                System.assert(initializedInTest);
                System.assert(Test.isRunningTest());
                AsyncContextProbe.observe();
                Test.stopTest();
                System.assertEquals(1, AsyncContextProbe.observations);
                System.assert(Test.isRunningTest());
            }

            @IsTest
            static void restoresTestModeAfterAsyncFailure() {
                AsyncContextProbe.fail();
                try {
                    Test.stopTest();
                } catch (MathException error) {
                    System.assertEquals('MathException', error.getTypeName());
                }
                System.assert(Test.isRunningTest());
            }
        }
        "#,
    )
    .unwrap();
    root
}
