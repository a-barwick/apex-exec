use super::instrumentation::{
    MAX_DEBUG_RENDERED_VALUE_BYTES, MAX_DEBUG_RETAINED_BYTES, MAX_DEBUG_SNAPSHOTS,
};
use super::value_graph::{MAX_VALUE_GRAPH_DEPTH, MAX_VALUE_GRAPH_ELEMENTS, MAX_VALUE_GRAPH_NODES};
use super::*;

#[derive(Default)]
struct ObservingHost {
    events: Vec<DebugEvent>,
}

impl PlatformHost for ObservingHost {
    fn debug(&mut self, event: DebugEvent) {
        self.events.push(event);
    }
}

fn execute_source(source: &str) -> Result<Vec<String>, Diagnostic> {
    let program = crate::check(source)?;
    Interpreter::new().execute(&program)
}

#[test]
fn ordinary_statement_execution_does_not_capture_debug_snapshots() {
    let program = crate::check(
        "Integer total = 0; \
         for (Integer i = 0; i < 20000; i++) total = total + i;",
    )
    .unwrap();
    let mut interpreter = Interpreter::new();

    interpreter.execute_anonymous_entry(&program).unwrap();

    assert_eq!(
        interpreter.instrumentation.policy(),
        InstrumentationPolicy::None
    );
    assert_eq!(interpreter.instrumentation.snapshot_count(), 0);
    assert!(
        interpreter
            .instrumentation
            .trace()
            .executed_statements
            .is_empty()
    );
    assert!(interpreter.instrumentation.trace().branches.is_empty());
}

#[test]
fn ordinary_static_invocation_does_not_capture_instrumentation() {
    let program = crate::check(
        "public class Worker { \
             public static Integer run() { \
                 Integer total = 0; \
                 for (Integer i = 0; i < 20000; i++) total = total + i; \
                 return total; \
             } \
         }",
    )
    .unwrap();
    let mut interpreter = Interpreter::new();

    let value = interpreter
        .invoke_static_entry(&program, "Worker", "run")
        .unwrap();

    assert_eq!(value, Value::Integer(199_990_000));
    assert_eq!(
        interpreter.instrumentation.policy(),
        InstrumentationPolicy::None
    );
    assert_eq!(interpreter.instrumentation.snapshot_count(), 0);
    assert!(
        interpreter
            .instrumentation
            .trace()
            .executed_statements
            .is_empty()
    );
    assert!(interpreter.instrumentation.trace().branches.is_empty());
}

#[test]
fn lazy_initialization_cost_is_independent_of_unused_classes() {
    let mut source = String::new();
    for index in 0..128 {
        source.push_str(&format!(
            "public class Unused{index} {{ public static Integer broken = 1 / 0; }} "
        ));
    }
    source.push_str(
        "public class Used { public static Integer value = 7; } \
         System.debug(Used.value); System.debug(Used.value);",
    );
    let program = crate::check(&source).unwrap();
    let mut interpreter = Interpreter::new();

    interpreter.execute_anonymous_entry(&program).unwrap();

    assert_eq!(interpreter.host.take_debug_output(), ["7", "7"]);
    assert_eq!(interpreter.store.initialized_class_count(), 1);
    assert_eq!(interpreter.store.static_slot_count(), 1);
}

#[test]
fn coverage_policy_records_only_coverage_facts() {
    let program = crate::check(
        "Integer total = 0; \
         for (Integer i = 0; i < 2; i++) total = total + i;",
    )
    .unwrap();
    let mut interpreter = Interpreter::new();
    interpreter
        .instrumentation
        .configure(InstrumentationPolicy::Coverage);

    interpreter.execute_anonymous_entry(&program).unwrap();

    assert_eq!(interpreter.instrumentation.snapshot_count(), 0);
    assert!(
        !interpreter
            .instrumentation
            .trace()
            .executed_statements
            .is_empty()
    );
    assert!(
        !interpreter.instrumentation.trace().branches.is_empty(),
        "the loop condition should retain both branch outcomes"
    );
}

#[test]
fn debugger_snapshot_count_is_bounded_and_reports_truncation() {
    let source = format!(
        "Integer i = 0; while (i < {}) i++;",
        MAX_DEBUG_SNAPSHOTS + 100
    );
    let program = crate::check(&source).unwrap();

    let execution = Interpreter::new().debug_execute(&program);

    assert!(execution.diagnostic.is_none());
    assert_eq!(execution.snapshots.len(), MAX_DEBUG_SNAPSHOTS);
    assert!(execution.trace_status.truncated);
    assert!(execution.trace_status.retained_bytes <= MAX_DEBUG_RETAINED_BYTES);
}

#[test]
fn debugger_reports_runtime_failures_after_its_snapshot_limit() {
    let source = format!(
        "Integer i = 0; while (i < {}) i++; Integer bad = 1 / 0;",
        MAX_DEBUG_SNAPSHOTS + 100
    );
    let program = crate::check(&source).unwrap();

    let execution = Interpreter::new().debug_execute(&program);

    assert_eq!(
        execution
            .diagnostic
            .as_ref()
            .and_then(|diagnostic| diagnostic.exception_type.as_deref()),
        Some("MathException")
    );
    assert_eq!(execution.snapshots.len(), MAX_DEBUG_SNAPSHOTS);
    assert!(execution.trace_status.truncated);
}

#[test]
fn debugger_rendered_values_are_bounded_and_keep_pre_statement_state() {
    let long_value = "x".repeat(MAX_DEBUG_RENDERED_VALUE_BYTES + 100);
    let program =
        crate::check(&format!("String value = '{long_value}'; Integer done = 1;")).unwrap();

    let execution = Interpreter::new().debug_execute(&program);

    let value = execution.snapshots[1]
        .variables
        .iter()
        .find(|variable| variable.name == "value")
        .unwrap();
    assert!(value.value.len() <= MAX_DEBUG_RENDERED_VALUE_BYTES);
    assert!(value.value.ends_with('…'));
    assert!(execution.trace_status.truncated);
    assert!(execution.trace_status.retained_bytes <= MAX_DEBUG_RETAINED_BYTES);
}

#[test]
fn system_debug_crosses_the_platform_host_boundary() {
    let program = crate::check("System.debug('hosted');").unwrap();
    let mut host = ObservingHost::default();

    let output = Interpreter::with_host(&mut host).execute(&program).unwrap();

    assert!(output.is_empty());
    assert_eq!(
        host.events,
        [DebugEvent {
            message: "hosted".to_owned()
        }]
    );
}

#[test]
fn typed_nulls_retain_static_string_behavior_and_compare_as_null() {
    let interpreter = Interpreter::new();
    let string_null = typed_value(Value::Null(None), &TypeName::String, Span::new(0, 0)).unwrap();
    let integer_null = typed_value(Value::Null(None), &TypeName::Integer, Span::new(0, 0)).unwrap();

    assert!(string_null.has_string_type());
    assert!(!integer_null.has_string_type());
    assert!(interpreter.values_equal(&string_null, &Value::Null(None)));
}

#[test]
fn display_traversal_enforces_deterministic_cost_budgets() {
    let mut interpreter = Interpreter::new();
    let long = Value::String("x".repeat(MAX_DEBUG_RENDERED_VALUE_BYTES + 100));
    let rendered = interpreter.render_value(&long);
    assert_eq!(rendered.text.len(), MAX_DEBUG_RENDERED_VALUE_BYTES);
    assert!(rendered.text.ends_with('…'));
    assert!(rendered.truncated);
    assert_eq!(rendered.stats.output_bytes, MAX_DEBUG_RENDERED_VALUE_BYTES);

    let multibyte = Value::String("😀".repeat(MAX_DEBUG_RENDERED_VALUE_BYTES));
    let rendered = interpreter.render_value(&multibyte);
    assert!(rendered.text.ends_with('…'));
    assert!(rendered.truncated);
    assert!(rendered.text.len() <= MAX_DEBUG_RENDERED_VALUE_BYTES);
    assert_eq!(rendered.stats.output_bytes, rendered.text.len());

    let mut nested = Value::Integer(1);
    for _ in 0..=MAX_VALUE_GRAPH_DEPTH {
        nested = interpreter.allocate(Collection::List {
            element_type: TypeName::Object,
            elements: vec![nested],
            iteration_depth: 0,
        });
    }
    let rendered = interpreter.render_value(&nested);
    assert!(rendered.truncated);
    assert_eq!(rendered.stats.max_depth, MAX_VALUE_GRAPH_DEPTH);
    assert!(rendered.stats.nodes <= MAX_VALUE_GRAPH_NODES);
    assert!(rendered.stats.elements <= MAX_VALUE_GRAPH_ELEMENTS);
    assert!(rendered.text.len() <= MAX_DEBUG_RENDERED_VALUE_BYTES);

    let wide = interpreter.allocate(Collection::List {
        element_type: TypeName::Object,
        elements: vec![Value::Integer(0); MAX_VALUE_GRAPH_ELEMENTS + 1],
        iteration_depth: 0,
    });
    let rendered = interpreter.render_value(&wide);
    assert!(rendered.truncated);
    assert_eq!(rendered.stats.nodes, MAX_VALUE_GRAPH_NODES);
    assert_eq!(rendered.stats.elements, MAX_VALUE_GRAPH_ELEMENTS);
    assert!(rendered.text.len() <= MAX_DEBUG_RENDERED_VALUE_BYTES);
}

#[test]
fn cyclic_equality_visits_each_collection_pair_once() {
    let mut interpreter = Interpreter::new();
    let Value::Collection(left) = interpreter.allocate(Collection::List {
        element_type: TypeName::Object,
        elements: Vec::new(),
        iteration_depth: 0,
    }) else {
        unreachable!()
    };
    let Value::Collection(right) = interpreter.allocate(Collection::List {
        element_type: TypeName::Object,
        elements: Vec::new(),
        iteration_depth: 0,
    }) else {
        unreachable!()
    };
    let Collection::List { elements, .. } = interpreter.collection_mut(left) else {
        unreachable!()
    };
    elements.push(Value::Collection(left));
    let Collection::List { elements, .. } = interpreter.collection_mut(right) else {
        unreachable!()
    };
    elements.push(Value::Collection(right));

    let left = Value::Collection(left);
    let right = Value::Collection(right);
    let (equal, stats) = interpreter.values_equal_with_stats(&left, &right);

    assert!(equal);
    assert_eq!(stats.equality_pairs, 1);
    assert_eq!(stats.equality_comparisons, 2);
}

#[test]
fn internal_sobject_field_cycles_render_safely_and_fail_json_explicitly() {
    let compilation = crate::project::compile(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/milestone7-project"
    ))
    .unwrap();
    let mut interpreter = Interpreter::new();
    interpreter.image = Some(RuntimeImage::new(&compilation.program));
    let Value::SObject(id) = interpreter.store.allocate_sobject(0) else {
        unreachable!()
    };
    interpreter
        .store
        .sobject_mut(id)
        .fields
        .insert(0, Value::SObject(id));
    let value = Value::SObject(id);

    assert!(interpreter.stringify_value(&value).contains("=<cycle>"));
    let error = interpreter
        .value_to_json(&value, Span::new(0, 1))
        .unwrap_err();
    assert_eq!(
        error.exception_type.as_deref(),
        Some("IllegalArgumentException")
    );
    assert!(error.message.contains("cyclic runtime values"));
}

#[test]
fn continue_in_a_for_loop_still_executes_the_update_clause() {
    let output = execute_source(
        "Integer i = 0; Integer total = 0; \
         for (; i < 4; i++) { if (i < 2) continue; total = total + i; } \
         System.debug(i); System.debug(total);",
    )
    .unwrap();

    assert_eq!(output, ["4", "5"]);
}

#[test]
fn conditional_short_circuits_records_side_effects_and_rejects_null_conditions() {
    let output = execute_source(
        "Integer hits = 0; \
         Integer first = true ? (hits = hits + 1) : (hits = hits + 100); \
         Integer second = false ? (hits = hits + 100) : (hits = hits + 10); \
         Integer safe = true ? 7 : 1 / 0; \
         System.debug(first); System.debug(second); System.debug(safe); System.debug(hits);",
    )
    .unwrap();
    assert_eq!(output, ["1", "11", "7", "11"]);

    let error =
        execute_source("Boolean condition = null; Integer value = condition ? 1 : 2;").unwrap_err();
    assert_eq!(
        error.exception_type.as_deref(),
        Some("NullPointerException")
    );
}

#[test]
fn null_aware_expressions_evaluate_once_skip_lazy_paths_and_chain() {
    let output = execute_source(
        "public class Box { \
             public static Integer receiverHits = 0; \
             public static Integer argumentHits = 0; \
             public String name; \
             public Box next; \
             public Box(String value) { name = value; } \
             public static Box make(Boolean present) { \
                 receiverHits++; \
                 return present ? new Box('present') : null; \
             } \
             public static Integer explode() { \
                 argumentHits++; \
                 return 1 / 0; \
             } \
             public String label(Integer suffix) { return name + suffix; } \
         } \
         String memberFallback = Box.make(false)?.name ?? 'member-fallback'; \
         String methodFallback = Box.make(false)?.label(Box.explode()) ?? 'method-fallback'; \
         String chainedFallback = Box.make(false)?.next?.name ?? 'chain-fallback'; \
         String present = Box.make(true)?.label(7) ?? 'unreachable'; \
         Integer coalesced = 5 ?? Box.explode(); \
         System.debug(memberFallback); \
         System.debug(methodFallback); \
         System.debug(chainedFallback); \
         System.debug(present); \
         System.debug(coalesced); \
         System.debug(Box.receiverHits); \
         System.debug(Box.argumentHits);",
    )
    .unwrap();

    assert_eq!(
        output,
        [
            "member-fallback",
            "method-fallback",
            "chain-fallback",
            "present7",
            "5",
            "4",
            "0"
        ]
    );
}

#[test]
fn null_aware_non_null_paths_preserve_argument_exceptions() {
    let error = execute_source(
        "public class Box { \
             public static Integer explode() { return 1 / 0; } \
             public String label(Integer value) { return 'value=' + value; } \
         } \
         Box instance = new Box(); \
         String value = instance?.label(Box.explode()) ?? 'fallback';",
    )
    .unwrap_err();

    assert_eq!(error.exception_type.as_deref(), Some("MathException"));
}

#[test]
fn safe_navigation_handles_intrinsics_and_void_methods() {
    let output = execute_source(
        "public class Worker { \
             public static Integer touches = 0; \
             public void touch() { touches++; } \
         } \
         String text = null; \
         List<Integer> values = null; \
         Worker worker = null; \
         worker?.touch(); \
         System.debug(text?.trim().toUpperCase() ?? 'EMPTY'); \
         System.debug(values?.size() ?? 0); \
         text = '  ready  '; \
         values = new List<Integer>{1, 2}; \
         worker = new Worker(); \
         worker?.touch(); \
         System.debug(text?.trim().toUpperCase() ?? 'EMPTY'); \
         System.debug(values?.size() ?? 0); \
         System.debug(Worker.touches);",
    )
    .unwrap();

    assert_eq!(output, ["EMPTY", "0", "READY", "2", "1"]);
}

#[test]
fn member_dispatch_and_collection_copy_construction_evaluate_each_input_once() {
    let output = execute_source(
        "public class Factory { \
             public static Integer receiverHits = 0; \
             public static Integer argumentHits = 0; \
             public static Integer sourceHits = 0; \
             public Integer value; \
             public Factory(Integer input) { value = input; } \
             public static Factory make(Boolean present) { \
                 receiverHits++; \
                 return present ? new Factory(5) : null; \
             } \
             public static Integer suffix() { argumentHits++; return 2; } \
             public static List<Integer> source() { \
                 sourceHits++; \
                 return new List<Integer>{1, 2}; \
             } \
             public Integer add(Integer suffix) { return value + suffix; } \
         } \
         Integer present = Factory.make(true)?.add(Factory.suffix()); \
         Integer absent = Factory.make(false)?.add(Factory.suffix()) ?? 0; \
         Integer member = Factory.make(true)?.value ?? 0; \
         List<Integer> copied = new List<Integer>(Factory.source()); \
         System.debug(present); System.debug(absent); System.debug(member); \
         System.debug(copied.size()); System.debug(Factory.receiverHits); \
         System.debug(Factory.argumentHits); System.debug(Factory.sourceHits);",
    )
    .unwrap();

    assert_eq!(output, ["7", "0", "5", "2", "3", "1", "1"]);
}

#[test]
fn instanceof_uses_runtime_identity_handles_generics_and_evaluates_once() {
    let output = execute_source(
        "public virtual class Parent {} \
         public class Child extends Parent {} \
         public interface Marker {} \
         public class Tagged implements Marker {} \
         Object child = new Child(); Parent parent = new Parent(); Object tagged = new Tagged(); \
         Object strings = new List<String>{'x'}; Object absent = null; Integer hits = 0; \
         Boolean once = ((hits = hits + 1) == 1 ? child : parent) instanceof Child; \
         System.debug(once); System.debug(hits); \
         System.debug(parent instanceof Child); System.debug(tagged instanceof Marker); \
         System.debug(strings instanceof List<String>); \
         System.debug(strings instanceof List<Integer>); System.debug(absent instanceof String);",
    )
    .unwrap();

    assert_eq!(
        output,
        ["true", "1", "false", "true", "true", "false", "false"]
    );
}

#[test]
fn return_unwinds_nested_blocks_and_loops() {
    let output = execute_source(
        "System.debug('before'); while (true) { { return; } } System.debug('after');",
    )
    .unwrap();

    assert_eq!(output, ["before"]);
}

#[test]
fn collection_assignment_aliases_while_copy_construction_is_independent() {
    let output = execute_source(
        "List<Integer> original = new List<Integer>{1}; \
         List<Integer> alias = original; alias.add(2); \
         List<Integer> copied = new List<Integer>(original); copied.set(0, 9); \
         System.debug(original); System.debug(alias); System.debug(copied);",
    )
    .unwrap();

    assert_eq!(output, ["(1, 2)", "(1, 2)", "(9, 2)"]);
}

#[test]
fn sized_arrays_support_index_mutation_and_remain_elastic() {
    let output = execute_source(
        "Integer[] values = new Integer[2]; \
         values[0] = 3; values[1] = 4; values[0]++; values.add(5); \
         System.debug(values); System.debug(values.size());",
    )
    .unwrap();

    assert_eq!(output, ["(4, 4, 5)", "3"]);
}

#[test]
fn set_and_map_methods_are_deterministic_and_return_previous_values() {
    let output = execute_source(
        "Set<String> names = new Set<String>{'Ada', 'Ada', 'Grace'}; \
         Boolean changed = names.add('Linus'); Boolean duplicate = names.add('Ada'); \
         Map<String, Integer> counts = new Map<String, Integer>{'a' => 1, 'a' => 2}; \
         Integer previous = counts.put('a', 3); Integer missing = counts.get('none'); \
         System.debug(names); System.debug(changed); System.debug(duplicate); \
         System.debug(counts); System.debug(previous); System.debug(missing);",
    )
    .unwrap();

    assert_eq!(
        output,
        ["{Ada, Grace, Linus}", "true", "false", "{a=3}", "2", "null"]
    );
}

#[test]
fn enhanced_for_iterates_snapshots_but_rejects_alias_mutation() {
    let output = execute_source(
        "List<Integer> values = new List<Integer>{1, 2, 3}; Integer total = 0; \
         for (Integer value : values) { if (value == 2) continue; total = total + value; } \
         System.debug(total);",
    )
    .unwrap();
    assert_eq!(output, ["4"]);

    let error = execute_source(
        "List<Integer> values = new List<Integer>{1}; List<Integer> alias = values; \
         for (Integer value : values) alias.add(2);",
    )
    .unwrap_err();
    assert_eq!(
        error.message,
        "cannot modify a collection while it is being iterated"
    );
}

#[test]
fn self_bulk_operations_use_source_snapshots() {
    let output = execute_source(
        "List<Integer> values = new List<Integer>{1, 2}; values.addAll(values); \
         Map<String, Integer> counts = new Map<String, Integer>{'a' => 1}; \
         counts.putAll(counts); System.debug(values); System.debug(counts);",
    )
    .unwrap();

    assert_eq!(output, ["(1, 2, 1, 2)", "{a=1}"]);
}

#[test]
fn map_key_and_value_accessors_return_independent_snapshots() {
    let output = execute_source(
        "Map<String, Integer> source = new Map<String, Integer>{'a' => 1}; \
         Set<String> keys = source.keySet(); List<Integer> values = source.values(); \
         keys.add('b'); values.add(2); \
         System.debug(source.size()); System.debug(keys); System.debug(values);",
    )
    .unwrap();

    assert_eq!(output, ["1", "{a, b}", "(1, 2)"]);
}

#[test]
fn string_math_and_system_calls_cover_utf16_indices() {
    let output = execute_source(
        "String emoji = 'A😀B'; \
         System.debug(emoji.length()); System.debug(emoji.substring(1, 3)); \
         System.debug(emoji.indexOf('B')); System.debug('  Ada  '.trim().toUpperCase()); \
         System.debug('Apex'.equals('Apex')); System.debug('Apex'.equalsIgnoreCase('aPeX')); \
         System.debug(String.join(new List<String>{'1', '2', '3'}, '-')); \
         System.debug(Math.abs(-4)); System.debug(Math.max(2, 5)); \
         System.debug(Math.min(2, 5)); System.debug(Math.mod(7, 3));",
    )
    .unwrap();

    assert_eq!(
        output,
        [
            "4", "😀", "3", "ADA", "true", "true", "1-2-3", "4", "5", "2", "1"
        ]
    );

    let error =
        execute_source("String value = '😀'; System.debug(value.substring(0, 1));").unwrap_err();
    assert_eq!(error.message, "String index splits a UTF-16 surrogate pair");
}

#[test]
fn reports_collection_bounds_null_and_negative_size_failures() {
    let bounds =
        execute_source("List<Integer> values = new List<Integer>{1}; System.debug(values[1]);")
            .unwrap_err();
    assert_eq!(bounds.message, "list index 1 is out of bounds for size 1");

    let null_receiver =
        execute_source("List<Integer> values = null; System.debug(values.size());").unwrap_err();
    assert_eq!(
        null_receiver.message,
        "attempt to de-reference a null value while calling `size`"
    );

    let negative_size =
        execute_source("Integer size = -1; Integer[] values = new Integer[size];").unwrap_err();
    assert_eq!(negative_size.message, "array size cannot be negative");
}
