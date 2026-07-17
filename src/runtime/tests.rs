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
    let string_null = typed_value(Value::Null(None), &TypeName::String);
    let integer_null = typed_value(Value::Null(None), &TypeName::Integer);

    assert!(string_null.has_string_type());
    assert!(!integer_null.has_string_type());
    assert!(interpreter.values_equal(&string_null, &Value::Null(None)));
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
