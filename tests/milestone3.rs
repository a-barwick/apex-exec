use apex_exec::{check, execute, parse, token::TokenKind, tokenize};

const ACCEPTANCE_PROGRAM: &str = include_str!("../examples/collections.apex");

fn assert_check_error_contains(source: &str, expected_fragments: &[&str]) {
    let error = check(source).unwrap_err();
    let message = error.message.to_ascii_lowercase();
    for fragment in expected_fragments {
        assert!(
            message.contains(&fragment.to_ascii_lowercase()),
            "expected checker error `{}` to contain `{fragment}`",
            error.message
        );
    }
}

fn assert_runtime_error_contains(source: &str, expected_fragments: &[&str]) {
    let error = execute(source).unwrap_err();
    let message = error.message.to_ascii_lowercase();
    for fragment in expected_fragments {
        assert!(
            message.contains(&fragment.to_ascii_lowercase()),
            "expected runtime error `{}` to contain `{fragment}`",
            error.message
        );
    }
}

#[test]
fn executes_the_unchanged_milestone_three_acceptance_program_through_every_stage() {
    let tokens = tokenize(ACCEPTANCE_PROGRAM).unwrap();
    assert!(tokens.iter().any(|token| token.kind == TokenKind::New));

    let parsed = parse(ACCEPTANCE_PROGRAM).unwrap();
    assert_eq!(parsed.statements.len(), 3);

    let checked = check(ACCEPTANCE_PROGRAM).unwrap();
    assert_eq!(checked.statements.len(), 3);

    let expected = (0..100).map(|value| value.to_string()).collect::<String>();
    assert_eq!(execute(ACCEPTANCE_PROGRAM).unwrap(), [expected]);
}

#[test]
fn list_assignment_aliases_but_copy_construction_is_independent() {
    let source = r#"
        List<String> original = new List<String>{'a'};
        List<String> alias = original;
        alias.add('b');

        List<String> copied = new List<String>(original);
        copied.add('c');
        copied.add('d');

        System.debug(original.size());
        System.debug(alias.size());
        System.debug(copied.size());
    "#;

    assert_eq!(execute(source).unwrap(), ["2", "2", "4"]);
}

#[test]
fn arrays_and_lists_share_literal_sized_index_and_add_behavior() {
    let source = r#"
        Integer[] sized = new Integer[2];
        System.debug(sized[0]);
        sized[0] = 4;

        List<Integer> alias = sized;
        alias.add(8);
        System.debug(sized[0]);
        System.debug(sized.size());
        System.debug(sized[2]);

        Integer[] literal = new Integer[]{1, 2};
        literal[1]++;
        System.debug(literal[1]);
    "#;

    assert_eq!(execute(source).unwrap(), ["null", "4", "3", "8", "3"]);
}

#[test]
fn sets_deduplicate_and_report_whether_mutation_changed_them() {
    let source = r#"
        Set<String> values = new Set<String>{'a', 'a', 'A'};
        System.debug(values.size());
        System.debug(values.add('a'));
        System.debug(values.add('b'));
        System.debug(values.remove('missing'));
        System.debug(values.remove('a'));
        System.debug(values.size());
    "#;

    assert_eq!(
        execute(source).unwrap(),
        ["2", "false", "true", "false", "true", "2"]
    );
}

#[test]
fn list_sort_orders_null_before_supported_scalar_values() {
    let source = r#"
        List<Integer> numbers = new List<Integer>{2, null, 1};
        numbers.sort();
        List<String> words = new List<String>{'b', null, 'a'};
        words.sort();
        System.debug(numbers);
        System.debug(words);
    "#;

    assert_eq!(execute(source).unwrap(), ["(null, 1, 2)", "(null, a, b)"]);
}

#[test]
fn list_method_surface_executes_with_apex_shaped_results() {
    let source = r#"
        List<String> values = new List<String>{'b', 'a', 'a'};
        System.debug(values.contains('a'));
        System.debug(values.indexOf('a'));
        System.debug(values.isEmpty());
        System.debug(values.get(0));
        System.debug(values.remove(0));
        values.set(0, 'c');
        values.add(1, 'b');
        values.addAll(new Set<String>{'d'});
        values.sort();
        System.debug(values);

        List<String> copy = values.clone();
        copy.clear();
        System.debug(copy.isEmpty());
        System.debug(values.size());
    "#;

    assert_eq!(
        execute(source).unwrap(),
        ["true", "1", "false", "b", "b", "(a, b, c, d)", "true", "4"]
    );
}

#[test]
fn set_method_surface_executes_with_change_results_and_independent_clones() {
    let source = r#"
        Set<String> values = new Set<String>{'a'};
        System.debug(values.isEmpty());
        System.debug(values.contains('a'));
        System.debug(values.addAll(new List<String>{'a', 'b', 'c'}));
        System.debug(values.containsAll(new Set<String>{'a', 'b'}));
        System.debug(values.removeAll(new List<String>{'b', 'missing'}));
        System.debug(values.retainAll(new Set<String>{'c'}));
        System.debug(values);

        Set<String> copy = values.clone();
        copy.clear();
        System.debug(copy.isEmpty());
        System.debug(values.size());
    "#;

    assert_eq!(
        execute(source).unwrap(),
        [
            "false", "true", "true", "true", "true", "true", "{c}", "true", "1"
        ]
    );
}

#[test]
fn map_method_surface_executes_with_typed_results_and_independent_copies() {
    let source = r#"
        Map<String, Integer> values = new Map<String, Integer>{'a' => 1, 'b' => 2};
        System.debug(values.isEmpty());
        System.debug(values.containsKey('a'));
        System.debug(values.remove('a'));
        System.debug(values.remove('missing'));

        Map<String, Integer> clone = values.clone();
        clone.put('c', 3);
        Map<String, Integer> constructed = new Map<String, Integer>(values);
        constructed.putAll(clone);
        System.debug(values.size());
        System.debug(clone.size());
        System.debug(constructed.keySet());
        System.debug(constructed.values());

        clone.clear();
        System.debug(clone.isEmpty());
        System.debug(values.get('b'));
    "#;

    assert_eq!(
        execute(source).unwrap(),
        [
            "false", "true", "1", "null", "1", "2", "{b, c}", "(2, 3)", "true", "2"
        ]
    );
}

#[test]
fn maps_replace_values_return_typed_null_and_preserve_nested_list_identity() {
    let source = r#"
        Map<String, Integer> counts = new Map<String, Integer>{'one' => 1, 'one' => 2};
        System.debug(counts.size());
        System.debug(counts.get('one'));
        System.debug(counts.put('one', 3));
        System.debug(counts.get('one'));

        Integer missing = counts.get('missing');
        System.debug(missing);

        Map<String, List<Integer>> nested = new Map<String, List<Integer>>{
            'values' => new List<Integer>{1}
        };
        nested.get('values').add(2);
        System.debug(nested.get('values').get(1));
    "#;

    assert_eq!(execute(source).unwrap(), ["1", "2", "2", "3", "null", "2"]);
}

#[test]
fn enhanced_for_iterates_lists_and_sets_and_honors_loop_control() {
    let source = r#"
        List<Integer> ordered = new List<Integer>{1, 2, 3, 4, 5};
        Integer listTotal = 0;
        for (Integer value : ordered) {
            if (value == 2) continue;
            if (value == 5) break;
            listTotal = listTotal + value;
        }

        Set<Integer> unique = new Set<Integer>{1, 2, 2};
        Integer setTotal = 0;
        for (Integer value : unique) {
            setTotal = setTotal + value;
        }

        System.debug(listTotal);
        System.debug(setTotal);
    "#;

    assert_eq!(execute(source).unwrap(), ["8", "3"]);
}

#[test]
fn core_string_math_and_system_methods_dispatch_case_insensitively() {
    let source = r#"
        String padded = ' apex ';
        SyStEm.DeBuG(padded.TrIm().ToUpPeRcAsE());
        System.debug(padded.CoNtAiNs('pex'));
        System.debug(String.IsBlAnK('   '));
        System.debug(String.IsEmPtY(''));
        System.debug(String.JoIn(new List<String>{'a', 'b'}, '-'));
        System.debug(String.VaLuEoF(false));
        System.debug('values=' + new List<Integer>{1, 2});
        System.debug('Apex' == 'aPeX');
        System.debug('Apex'.equals('aPeX'));
        System.debug('Apex'.equals(null));
        System.debug('Apex'.equalsIgnoreCase(null));
        System.debug(Math.AbS(-7));
        System.debug(Math.MaX(2, 5));
        System.debug(Math.MiN(2, 5));
        System.debug(Math.MoD(7, 4));
    "#;

    assert_eq!(
        execute(source).unwrap(),
        [
            "APEX",
            "true",
            "true",
            "true",
            "a-b",
            "false",
            "values=(1, 2)",
            "true",
            "false",
            "false",
            "false",
            "7",
            "5",
            "2",
            "3"
        ]
    );
}

#[test]
fn remaining_string_predicates_and_transforms_execute() {
    let source = r#"
        String value = 'Apex Exec';
        System.debug(value.startsWith('Apex'));
        System.debug(value.endsWith('Exec'));
        System.debug(value.toLowerCase());
        System.debug(value.replace('Exec', 'Runtime'));
        System.debug(String.isNotBlank(' x '));
        System.debug(String.isNotEmpty('x'));
        System.debug(String.isNotBlank(null));
        System.debug(String.isNotEmpty(null));
    "#;

    assert_eq!(
        execute(source).unwrap(),
        [
            "true",
            "true",
            "apex exec",
            "Apex Runtime",
            "true",
            "true",
            "false",
            "false"
        ]
    );
}

#[test]
fn checker_rejects_generic_mismatch_and_invalid_method_arguments() {
    assert_check_error_contains(
        "List<String> values = new List<Integer>();",
        &["cannot assign", "list", "string", "integer"],
    );
    assert_check_error_contains(
        "List<String> values = new List<String>(); values.add(1);",
        &["add", "string", "integer"],
    );
    let source = "List<String> values = new List<String>(); values.add();";
    assert_check_error_contains(source, &["add", "argument", "0"]);
    let error = check(source).unwrap_err();
    let start = source.find("add").unwrap();
    assert_eq!(error.span.start, start);
    assert_eq!(error.span.end, start + "add".len());
}

#[test]
fn checker_rejects_set_and_map_indexing() {
    assert_check_error_contains(
        "Set<String> values = new Set<String>(); System.debug(values[0]);",
        &["index", "set"],
    );
    assert_check_error_contains(
        "Map<String, Integer> values = new Map<String, Integer>(); System.debug(values[0]);",
        &["index", "map"],
    );
}

#[test]
fn checker_rejects_direct_map_iteration_and_for_each_scope_escape() {
    assert_check_error_contains(
        "Map<String, Integer> values = new Map<String, Integer>(); for (String key : values) {}",
        &["map", "list", "set"],
    );
    assert_check_error_contains(
        "List<String> values = new List<String>(); for (String item : values) {} System.debug(item);",
        &["unknown variable", "item"],
    );
}

#[test]
fn checker_rejects_using_a_void_method_result_as_a_value() {
    assert_check_error_contains(
        "List<String> values = new List<String>(); Boolean changed = values.add('x');",
        &["void", "boolean"],
    );
}

#[test]
fn runtime_reports_list_bounds_failures() {
    assert_runtime_error_contains(
        "List<Integer> values = new List<Integer>{1}; System.debug(values[1]);",
        &["index", "bounds"],
    );
}

#[test]
fn runtime_rejects_invalid_array_sizes() {
    assert_runtime_error_contains(
        "Integer[] values = new Integer[-1]; System.debug(values.size());",
        &["size", "negative"],
    );
    assert_runtime_error_contains(
        "Integer[] values = new Integer[9223372036854775807]; System.debug(values.size());",
        &["size", "too large"],
    );
}

#[test]
fn runtime_reports_null_method_receivers() {
    assert_runtime_error_contains(
        "List<String> values = null; values.add('x');",
        &["null", "add"],
    );
}

#[test]
fn runtime_rejects_collection_mutation_during_iteration() {
    assert_runtime_error_contains(
        "List<Integer> values = new List<Integer>{1, 2}; for (Integer value : values) { values.add(3); }",
        &["modify", "iterat"],
    );
}
