use super::*;

fn check_source(source: &str) -> Result<(), Diagnostic> {
    let program = crate::parse(source)?;
    check(&program).map(|_| ())
}

#[test]
fn permits_nested_shadowing_but_rejects_same_scope_duplicates() {
    check_source("Integer value = 1; { Integer VALUE = 2; }").unwrap();

    let error = check_source("Integer value = 1; Integer VALUE = 2;").unwrap_err();
    assert_eq!(error.message, "duplicate variable `VALUE`");
}

#[test]
fn short_circuit_rhs_is_still_checked_statically() {
    let error = check_source("Boolean result = true || missing;").unwrap_err();
    assert_eq!(error.message, "unknown variable `missing`");
}

#[test]
fn checks_conditional_conditions_branches_and_result_types() {
    check_source(
        "Boolean flag = true; \
         Integer nullable = flag ? 1 : null; \
         Decimal promoted = flag ? 1 : 2.5; \
         Object common = flag ? 'text' : 1; \
         Object bothNull = flag ? null : null;",
    )
    .unwrap();

    let error = check_source("Integer value = 1 ? 2 : 3;").unwrap_err();
    assert_eq!(error.message, "expected Boolean, found Integer");

    let error = check_source("Integer value = true ? 1 : missing;").unwrap_err();
    assert_eq!(error.message, "unknown variable `missing`");

    let error = check_source("Object value = true ? System.debug('x') : 1;").unwrap_err();
    assert!(
        error
            .message
            .contains("conditional branches must produce values")
    );
}

#[test]
fn checks_instanceof_viability_generic_targets_and_always_true_tests() {
    check_source(
        "public virtual class Parent {} \
         public class Child extends Parent {} \
         public interface Marker {} \
         public class Tagged implements Marker {} \
         Object child = new Child(); \
         Boolean isChild = child instanceof Child; \
         Parent parent = new Parent(); \
         Boolean maybeChild = parent instanceof Child; \
         Object tagged = new Tagged(); \
         Boolean marked = tagged instanceof Marker; \
         Object values = new List<String>(); \
         Boolean strings = values instanceof List<String>; \
         Boolean integers = values instanceof List<Integer>; \
         Boolean absent = null instanceof String;",
    )
    .unwrap();

    let error =
        check_source("String value = 'x'; Boolean result = value instanceof String;").unwrap_err();
    assert!(error.message.contains("always true"));

    let error =
        check_source("String value = 'x'; Boolean result = value instanceof Integer;").unwrap_err();
    assert_eq!(
        error.message,
        "Integer is not a viable runtime type for String"
    );

    let error =
        check_source("Object value = 'x'; Boolean result = value instanceof Missing;").unwrap_err();
    assert_eq!(error.message, "unknown type `Missing`");

    let error = check_source(
        "List<String> values = new List<String>(); \
         Boolean result = values instanceof List<Integer>;",
    )
    .unwrap_err();
    assert_eq!(
        error.message,
        "List<Integer> is not a viable runtime type for List<String>"
    );
}

#[test]
fn null_assignment_does_not_permit_cross_type_equality() {
    check_source("Integer number = null; number = null;").unwrap();

    let error = check_source("Integer number = 1; Boolean same = number == true;").unwrap_err();
    assert_eq!(
        error.message,
        "operator `==` cannot be applied to Integer and Boolean"
    );
}

#[test]
fn loop_control_is_validated_against_lexical_loop_depth() {
    check_source("while (true) { if (false) continue; break; }").unwrap();

    let error = check_source("{ break; }").unwrap_err();
    assert_eq!(error.message, "`break` is only valid inside a loop");
}

#[test]
fn checks_collection_construction_indexing_and_generic_invariance() {
    check_source(
        "List<String> values = new List<String>{'a', null}; \
         Set<String> unique = new Set<String>(values); \
         Map<String, List<String>> grouped = new Map<String, List<String>>{ \
             'all' => values \
         }; \
         String first = values[0]; \
         values[0] = 'b'; \
         Integer[] numbers = new Integer[2]; \
         numbers[0]++;",
    )
    .unwrap();

    let error = check_source("List<String> values = new List<String>{1};").unwrap_err();
    assert_eq!(error.message, "cannot assign Integer to String");

    let error =
        check_source("List<Integer> values = new List<Integer>(); String value = values['zero'];")
            .unwrap_err();
    assert_eq!(error.message, "expected Integer, found String");
}

#[test]
fn checks_enhanced_for_types_scope_and_loop_control() {
    check_source(
        "Set<String> values = new Set<String>{'a'}; \
         for (String value : values) { if (value == 'a') continue; }",
    )
    .unwrap();

    let error = check_source(
        "List<String> values = new List<String>(); \
         for (Integer value : values) {}",
    )
    .unwrap_err();
    assert_eq!(error.message, "cannot assign String to Integer");

    let error = check_source(
        "Map<String, String> values = new Map<String, String>(); \
         for (String value : values) {}",
    )
    .unwrap_err();
    assert_eq!(
        error.message,
        "enhanced for-loop requires List or Set, found Map<String,String>"
    );
}

#[test]
fn checks_collection_method_signatures_and_return_types() {
    check_source(
        "List<String> values = new List<String>{'b'}; \
         values.add('c'); values.add(0, 'a'); values.addAll(new Set<String>{'d'}); \
         Boolean hasA = values.contains('a'); Integer position = values.indexOf('a'); \
         String first = values.get(0); String removed = values.remove(0); \
         values.set(0, 'z'); Integer count = values.size(); Boolean listEmpty = values.isEmpty(); \
         values.sort(); List<String> copy = values.clone(); copy.clear(); \
         Set<String> unique = new Set<String>(values); \
         Boolean changed = unique.add('q'); changed = unique.addAll(values); \
         changed = unique.containsAll(values); changed = unique.removeAll(values); \
         changed = unique.retainAll(copy); changed = unique.remove('q'); \
         Boolean setEmpty = unique.isEmpty(); Integer setSize = unique.size(); \
         Set<String> uniqueCopy = unique.clone(); uniqueCopy.clear(); \
         Map<String, String> labels = new Map<String, String>{'a' => 'A'}; \
         String prior = labels.put('b', 'B'); String found = labels.get('a'); \
         Boolean hasKey = labels.containsKey('a'); Set<String> keys = labels.keySet(); \
         String removedLabel = labels.remove('b'); Boolean mapEmpty = labels.isEmpty(); \
         Integer mapSize = labels.size(); List<String> labelValues = labels.values(); \
         Map<String, String> labelsCopy = labels.clone(); \
         Map<String, String> constructedCopy = new Map<String, String>(labelsCopy); \
         labels.putAll(constructedCopy); labels.clear();",
    )
    .unwrap();

    let error =
        check_source("List<String> values = new List<String>(); values.add(1);").unwrap_err();
    assert_eq!(
        error.message,
        "argument 1 to `add` on List<String> expects String, found Integer"
    );

    let error = check_source("Set<String> values = new Set<String>(); values.add();").unwrap_err();
    assert_eq!(
        error.message,
        "method `add` on Set<String> expects 1 arguments, found 0"
    );
}

#[test]
fn checks_string_math_and_system_signatures() {
    check_source(
        "List<String> values = new List<String>{String.valueOf(1)}; \
         String joined = String.join(values, ','); \
         String joinedSet = String.join(new Set<String>(values), ','); \
         Boolean blank = String.isBlank(null); Boolean notBlank = String.isNotBlank(joined); \
         Boolean empty = String.isEmpty(''); Boolean notEmpty = String.isNotEmpty(joined); \
         Integer length = joined.length(); Boolean contains = joined.contains('1'); \
         Boolean starts = joined.startsWith('1'); Boolean ends = joined.endsWith('1'); \
         Boolean exact = joined.equals('1'); Boolean same = joined.equalsIgnoreCase('1'); \
         Integer index = joined.indexOf('1'); \
         String piece = joined.substring(0, 1).trim().toUpperCase().toLowerCase(); \
         String replaced = joined.replace('1', 'one'); \
         Integer absolute = Math.abs(-1); Integer maximum = Math.max(1, 2); \
         Integer minimum = Math.min(1, 2); Integer remainder = Math.mod(5, 2); \
         System.debug(String.join(values, ''));",
    )
    .unwrap();

    let error = check_source("String value = Math.abs('wrong');").unwrap_err();
    assert_eq!(
        error.message,
        "argument 1 to `abs` on Math expects Integer, found String"
    );

    let error = check_source("Boolean value = System.debug('no');").unwrap_err();
    assert_eq!(error.message, "cannot assign void to Boolean");
}

#[test]
fn method_receivers_resolve_variables_before_static_types() {
    let error =
        check_source("String String = 'value'; String converted = String.valueOf(1);").unwrap_err();
    assert_eq!(error.message, "unknown method `valueOf` on String");
}

#[test]
fn collects_method_signatures_before_checking_bodies_and_resolves_recursion() {
    check_source(
        "Integer first(Integer value) { return second(value); } \
         Integer second(Integer value) { \
             if (value <= 0) return 0; \
             return first(value - 1); \
         } \
         System.debug(first(2));",
    )
    .unwrap();

    let error = check_source(
        "Integer choose(Integer value) { return value; } \
         String CHOOSE(Integer value) { return 'duplicate'; }",
    )
    .unwrap_err();
    assert!(error.message.contains("duplicate method overload"));
}

#[test]
fn ranks_exact_object_and_null_overloads() {
    check_source(
        "String kind(String value) { return 'String'; } \
         String kind(Object value) { return 'Object'; } \
         String exact = kind('value'); Object boxed = 1; String broad = kind(boxed); \
         String specificNull = kind(null);",
    )
    .unwrap();

    check_source(
        "String kind(Exception value) { return 'Exception'; } \
         String kind(Object value) { return 'Object'; } \
         NullPointerException error = new NullPointerException(); \
         String specificException = kind(error);",
    )
    .unwrap();

    let error = check_source(
        "String choose(String value) { return 'String'; } \
         String choose(Integer value) { return 'Integer'; } \
         String result = choose(null);",
    )
    .unwrap_err();
    assert!(error.message.contains("ambiguous overload"));

    let error = check_source(
        "String choose(String value) { return 'String'; } \
         String choose(Exception value) { return 'Exception'; } \
         String result = choose(null);",
    )
    .unwrap_err();
    assert!(error.message.contains("ambiguous overload"));

    let error = check_source(
        "String choose(Object left, MathException right) { return 'first'; } \
         String choose(Exception left, Object right) { return 'second'; } \
         MathException error = new MathException(); \
         String result = choose(error, error);",
    )
    .unwrap_err();
    assert!(error.message.contains("ambiguous overload"));
}

#[test]
fn ranks_top_level_overloads_using_user_type_subtyping() {
    check_source(
        "public virtual class Parent {} \
         public class Child extends Parent {} \
         String pick(Parent value) { return 'parent'; } \
         String pick(Child value) { return 'child'; } \
         Child child = new Child(); \
         String result = pick(child);",
    )
    .unwrap();
}

#[test]
fn validates_method_return_types_and_definite_completion() {
    check_source(
        "Integer complete(Boolean branch) { \
             if (branch) return 1; else throw new MathException('failed'); \
         } \
         Integer once() { do { return 1; } while (false); } \
         void done() { return; }",
    )
    .unwrap();

    let error =
        check_source("Integer incomplete(Boolean branch) { if (branch) return 1; }").unwrap_err();
    assert!(error.message.contains("every path"));

    let error = check_source(
        "Integer broken(Boolean stop) { \
             do { if (stop) break; return 1; } while (false); \
         }",
    )
    .unwrap_err();
    assert!(error.message.contains("every path"));

    let error = check_source("void wrong() { return 1; }").unwrap_err();
    assert_eq!(error.message, "void method cannot return a value");
}

#[test]
fn validates_exception_catches_accessors_and_casts() {
    check_source(
        "String recover(Object value) { \
             try { \
                 MathException error = (MathException) value; \
                 throw error; \
             } catch (MathException error) { \
                 return error.getTypeName() + error.getMessage() \
                     + error.getStackTraceString(); \
             } finally { System.debug('done'); } \
         } \
         Exception emptyMessage = new Exception(null); \
         throw null;",
    )
    .unwrap();

    let error = check_source(
        "void fail() { \
             try { throw new MathException(); } \
             catch (Exception error) {} \
             catch (MathException specific) {} \
         }",
    )
    .unwrap_err();
    assert!(error.message.contains("unreachable catch"));

    let error = check_source("void fail() { throw 'not an exception'; }").unwrap_err();
    assert!(error.message.contains("requires an Exception"));

    let error = check_source("try {} catch (String problem) {}").unwrap_err();
    assert!(error.message.contains("catch type must be an Exception"));

    let error = check_source("throw new Exception('first', 'second');").unwrap_err();
    assert!(error.message.contains("zero or one argument"));

    let error = check_source(
        "MathException math = new MathException(); \
         IllegalArgumentException unrelated = (IllegalArgumentException) math;",
    )
    .unwrap_err();
    assert!(error.message.contains("cannot cast"));
}

#[test]
fn method_local_names_do_not_resolve_anonymous_or_other_frame_locals() {
    let error = check_source(
        "Integer read() { return outside; } \
         Integer outside = 1; System.debug(read());",
    )
    .unwrap_err();
    assert_eq!(error.message, "unknown variable `outside`");

    let error = check_source("Integer same(Integer value) { Integer VALUE = 2; return value; }")
        .unwrap_err();
    assert_eq!(error.message, "duplicate variable `VALUE`");
}

#[test]
fn interface_method_collection_tracks_visited_nodes_even_for_cyclic_raw_ast_edges() {
    let mut program =
        crate::parse("public interface A { void a(); } public interface B { void b(); }").unwrap();
    let a_span = program.classes[0].name.span;
    let b_span = program.classes[1].name.span;
    program.classes[0]
        .interfaces
        .push(crate::ast::NamedType::new("B".to_owned(), b_span));
    program.classes[1]
        .interfaces
        .push(crate::ast::NamedType::new("A".to_owned(), a_span));

    let mut checker = Checker::new(SchemaCatalog::new());
    checker.collect_classes(&program).unwrap();
    let mut required = Vec::new();
    let mut visited = vec![false; checker.classes.len()];
    checker.collect_interface_methods(checker.class_ids["a"], &mut required, &mut visited);

    assert_eq!(required.len(), 2);
    assert!(visited[checker.class_ids["a"]]);
    assert!(visited[checker.class_ids["b"]]);
}

#[test]
fn hierarchy_cycle_validation_examines_each_acyclic_edge_once() {
    let mut source = String::from("public interface I0 {}");
    for index in 1..256 {
        source.push_str(&format!(
            " public interface I{index} extends I{} {{}}",
            index - 1
        ));
    }
    source.push_str(" public class Implementation implements I255 {}");
    let program = crate::parse(&source).unwrap();
    let mut checker = Checker::new(SchemaCatalog::new());
    checker.collect_classes(&program).unwrap();
    let mut graph = HierarchyGraph::new(checker.classes.len());
    for (class_id, class) in checker.classes.iter().enumerate() {
        checker.validate_type_declaration_header(class).unwrap();
        graph.add_edges(class_id, checker.validated_hierarchy_edges(class).unwrap());
    }

    let traversal = graph.validate_acyclic(&checker.classes).unwrap();
    assert_eq!(traversal.nodes_started, checker.classes.len());
    assert_eq!(traversal.edges_examined, graph.edge_count());
}

#[test]
fn subtype_traversal_is_iterative_and_bounded_by_the_reachable_hierarchy() {
    let mut source = String::from("public interface I0 {}");
    for index in 1..512 {
        source.push_str(&format!(
            " public interface I{index} extends I{} {{}}",
            index - 1
        ));
    }
    source.push_str(
        " public class Implementation implements I511 {} \
         public interface Unrelated {}",
    );
    let program = crate::parse(&source).unwrap();
    let mut checker = Checker::new(SchemaCatalog::new());
    checker.collect_classes(&program).unwrap();
    checker.validate_class_hierarchy().unwrap();

    let implementation_id = checker.class_ids["implementation"];
    let root_id = checker.class_ids["i0"];
    let traversal = checker.class_inheritance_traversal(implementation_id, root_id);
    assert_eq!(
        traversal,
        InheritanceTraversal {
            matched: true,
            nodes_visited: 513,
            edges_examined: 512,
        }
    );

    let unrelated_id = checker.class_ids["unrelated"];
    let traversal = checker.class_inheritance_traversal(implementation_id, unrelated_id);
    assert_eq!(
        traversal,
        InheritanceTraversal {
            matched: false,
            nodes_visited: 513,
            edges_examined: 512,
        }
    );

    let mut cyclic =
        crate::parse("public interface A {} public interface B {} public interface C {}").unwrap();
    let a_span = cyclic.classes[0].name.span;
    let b_span = cyclic.classes[1].name.span;
    cyclic.classes[0]
        .interfaces
        .push(crate::ast::NamedType::new("B".to_owned(), b_span));
    cyclic.classes[1]
        .interfaces
        .push(crate::ast::NamedType::new("A".to_owned(), a_span));
    let mut checker = Checker::new(SchemaCatalog::new());
    checker.collect_classes(&cyclic).unwrap();
    let traversal =
        checker.class_inheritance_traversal(checker.class_ids["a"], checker.class_ids["c"]);
    assert_eq!(
        traversal,
        InheritanceTraversal {
            matched: false,
            nodes_visited: 2,
            edges_examined: 2,
        }
    );
}
