use apex_exec::{check, execute, runtime::Interpreter};
use std::process::Command;

const SCENARIO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/scenarios/runtime_graph_cycles.apex"
);

#[test]
fn cyclic_runtime_graph_scenario_runs_through_the_library_and_cli() {
    let source = include_str!("scenarios/runtime_graph_cycles.apex");
    let expected = [
        "(<cycle>, 1)",
        "true",
        "false",
        "IllegalArgumentException",
        "JSON serialization does not support cyclic runtime values",
        "{<cycle>}",
        "{self=<cycle>}",
        "Node@0",
    ];

    assert_eq!(execute(source).unwrap(), expected);

    let output = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["run", SCENARIO])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        expected.join("\n") + "\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn isomorphic_and_non_isomorphic_cyclic_lists_sets_and_maps_terminate() {
    let output = execute(
        "
        List<Object> listLeft = new List<Object>();
        listLeft.add(listLeft);
        listLeft.add(1);
        List<Object> listRight = new List<Object>();
        listRight.add(listRight);
        listRight.add(1);
        System.debug(listLeft == listRight);
        listRight.set(1, 2);
        System.debug(listLeft == listRight);

        Set<Object> setLeft = new Set<Object>();
        setLeft.add(setLeft);
        setLeft.add(1);
        Set<Object> setRight = new Set<Object>();
        setRight.add(1);
        setRight.add(setRight);
        System.debug(setLeft == setRight);
        setRight.remove(1);
        setRight.add(2);
        System.debug(setLeft == setRight);

        Map<String, Object> mapLeft = new Map<String, Object>();
        mapLeft.put('self', mapLeft);
        mapLeft.put('leaf', 1);
        Map<String, Object> mapRight = new Map<String, Object>();
        mapRight.put('leaf', 1);
        mapRight.put('self', mapRight);
        System.debug(mapLeft == mapRight);
        mapRight.put('leaf', 2);
        System.debug(mapLeft == mapRight);

        List<Object> candidateLeft = new List<Object>();
        candidateLeft.add(candidateLeft);
        candidateLeft.add(1);
        List<Object> rejectedRight = new List<Object>();
        rejectedRight.add(rejectedRight);
        rejectedRight.add(2);
        List<Object> pollutedRight = new List<Object>();
        pollutedRight.add(rejectedRight);
        pollutedRight.add(1);
        Set<Object> backtrackingLeft = new Set<Object>();
        backtrackingLeft.add(candidateLeft);
        backtrackingLeft.add(rejectedRight);
        Set<Object> backtrackingRight = new Set<Object>();
        backtrackingRight.add(rejectedRight);
        backtrackingRight.add(pollutedRight);
        System.debug(backtrackingLeft == backtrackingRight);
        ",
    )
    .unwrap();

    assert_eq!(
        output,
        ["true", "false", "true", "false", "true", "false", "false"]
    );
}

#[test]
fn deeply_nested_collection_equality_uses_an_explicit_work_stack() {
    let output = execute(
        "
        List<Object> left = new List<Object>();
        List<Object> leftCursor = left;
        List<Object> right = new List<Object>();
        List<Object> rightCursor = right;
        for (Integer i = 0; i < 5000; i++) {
            List<Object> leftNext = new List<Object>();
            leftCursor.add(leftNext);
            leftCursor = leftNext;
            List<Object> rightNext = new List<Object>();
            rightCursor.add(rightNext);
            rightCursor = rightNext;
        }
        System.debug(left == right);
        rightCursor.add(1);
        System.debug(left == right);
        ",
    )
    .unwrap();

    assert_eq!(output, ["true", "false"]);
}

#[test]
fn json_cycles_are_catchable_for_each_structural_collection_kind() {
    let output = execute(
        "
        List<Object> listed = new List<Object>();
        listed.add(listed);
        Set<Object> setted = new Set<Object>();
        setted.add(setted);
        Map<String, Object> mapped = new Map<String, Object>();
        mapped.put('self', mapped);

        try {
            JSON.serialize(listed);
        } catch (IllegalArgumentException error) {
            System.debug(error.getTypeName());
        }
        try {
            JSON.serializePretty(setted);
        } catch (IllegalArgumentException error) {
            System.debug(error.getTypeName());
        }
        try {
            JSON.serialize(mapped);
        } catch (IllegalArgumentException error) {
            System.debug(error.getTypeName());
        }
        ",
    )
    .unwrap();

    assert_eq!(
        output,
        [
            "IllegalArgumentException",
            "IllegalArgumentException",
            "IllegalArgumentException"
        ]
    );
}

#[test]
fn debugger_snapshots_render_cycles_with_the_same_bounded_marker() {
    let program = check(
        "
        List<Object> values = new List<Object>();
        values.add(values);
        Integer done = 1;
        System.debug(done);
        ",
    )
    .unwrap();

    let execution = Interpreter::new().debug_execute(&program);

    assert!(execution.diagnostic.is_none());
    assert_eq!(execution.output, ["1"]);
    assert!(execution.snapshots.iter().any(|snapshot| {
        snapshot
            .variables
            .iter()
            .any(|variable| variable.name == "values" && variable.value == "(<cycle>)")
    }));
    assert!(execution.trace_status.retained_bytes <= 16 * 1024 * 1024);
}

#[test]
fn acyclic_display_and_json_output_remain_compatible() {
    let output = execute(
        "
        List<Object> values = new List<Object>{1, 'two'};
        Set<Object> unique = new Set<Object>{1, 'two'};
        Map<String, Object> mapped =
            new Map<String, Object>{'values' => values, 'ok' => true};
        System.debug(values);
        System.debug(unique);
        System.debug(mapped);
        System.debug(JSON.serialize(mapped));
        List<Object> shared = new List<Object>{1};
        List<Object> dag = new List<Object>{shared, shared};
        System.debug(dag);
        System.debug(JSON.serialize(dag));
        ",
    )
    .unwrap();

    assert_eq!(
        output,
        [
            "(1, two)",
            "{1, two}",
            "{values=(1, two), ok=true}",
            "{\"values\":[1,\"two\"],\"ok\":true}",
            "((1), (1))",
            "[[1],[1]]"
        ]
    );
}

#[test]
fn json_structural_limits_are_typed_and_catchable() {
    let output = execute(
        "
        List<Object> deep = new List<Object>();
        List<Object> cursor = deep;
        for (Integer i = 0; i < 65; i++) {
            List<Object> next = new List<Object>();
            cursor.add(next);
            cursor = next;
        }
        try {
            JSON.serialize(deep);
        } catch (IllegalArgumentException error) {
            System.debug(error.getTypeName());
            System.debug(error.getMessage());
        }

        List<Object> wide = new List<Object>();
        for (Integer i = 0; i < 4096; i++) {
            wide.add(i);
        }
        try {
            JSON.serialize(wide);
        } catch (IllegalArgumentException error) {
            System.debug(error.getTypeName());
            System.debug(error.getMessage());
        }
        ",
    )
    .unwrap();

    assert_eq!(
        output,
        [
            "IllegalArgumentException",
            "JSON serialization exceeded the runtime value depth limit",
            "IllegalArgumentException",
            "JSON serialization exceeded the runtime value node limit"
        ]
    );
}

#[test]
fn semantic_string_paths_preserve_long_acyclic_values() {
    let payload = "x".repeat(20 * 1024);
    let output = execute(&format!(
        "
        String raw = '{payload}';

        String valueOfText = String.valueOf(raw);
        System.debug(valueOfText.length());
        System.debug(valueOfText == raw);

        String joined = String.join(new List<String>{{raw}}, '');
        System.debug(joined.length());
        System.debug(joined == raw);

        String concatenated = raw + '';
        System.debug(concatenated.length());
        System.debug(concatenated == raw);

        Object boxed = raw;
        String objectText = boxed.toString();
        System.debug(objectText.length());
        System.debug(objectText == raw);

        String nested = String.valueOf(new List<Object>{{raw}});
        System.debug(nested.length());

        try {{
            System.assert(false, raw);
        }} catch (AssertException error) {{
            System.debug(error.getMessage().length());
        }}
        try {{
            System.assertEquals(raw, 'y');
        }} catch (AssertException error) {{
            System.debug(error.getMessage().length());
        }}

        System.debug(JSON.serialize(raw).length());
        "
    ))
    .unwrap();

    assert_eq!(
        output,
        [
            "20480", "true", "20480", "true", "20480", "true", "20480", "true", "20482", "20519",
            "20517", "20482"
        ]
    );

    let assertion = execute(&format!(
        "String raw = '{payload}'; System.assert(false, raw);"
    ))
    .unwrap_err();
    assert_eq!(
        assertion.message,
        format!("Assertion Failed: {payload} (condition is false)")
    );

    let equality_assertion = execute(&format!(
        "String raw = '{payload}'; System.assertEquals(raw, 'y');"
    ))
    .unwrap_err();
    assert_eq!(
        equality_assertion.message,
        format!("Assertion Failed: expected {payload}, actual y")
    );

    let program = check(&format!(
        "
        public class LongReturn {{
            public static String run() {{
                return '{payload}';
            }}
        }}
        "
    ))
    .unwrap();

    let invoked = Interpreter::new()
        .invoke_static(&program, "LongReturn", "run")
        .unwrap();
    assert_eq!(invoked, [payload.clone()]);

    let debugged = Interpreter::new().debug_invoke(&program, "LongReturn", "run");
    assert!(debugged.diagnostic.is_none());
    assert_eq!(debugged.output.len(), 1);
    assert_eq!(debugged.output[0].len(), 16 * 1024);
    assert!(debugged.output[0].ends_with('…'));
    assert!(debugged.trace_status.truncated);

    let debug_output = execute(&format!("String raw = '{payload}'; System.debug(raw);")).unwrap();
    assert_eq!(debug_output.len(), 1);
    assert_eq!(debug_output[0].len(), 16 * 1024);
    assert!(debug_output[0].ends_with('…'));
}
