use apex_exec::{
    ast::{
        self, AnnotationKind, ClassMember, Expression, Modifier, Statement, SwitchLabels,
        visit::{self, Visitor},
    },
    check, execute, parse,
    runtime::Interpreter,
};
use std::{collections::BTreeMap, process::Command};

const NORTH_STAR: &[(&str, &str)] = &[
    ("SOQL.cls", include_str!("north_star/corpus/SOQL.cls")),
    ("Logger.cls", include_str!("north_star/corpus/Logger.cls")),
    ("Rollup.cls", include_str!("north_star/corpus/Rollup.cls")),
    (
        "RollupService.cls",
        include_str!("north_star/corpus/RollupService.cls"),
    ),
    (
        "fflib_SObjectDomain.cls",
        include_str!("north_star/corpus/fflib_SObjectDomain.cls"),
    ),
    ("Puff.cls", include_str!("north_star/corpus/Puff.cls")),
    (
        "JSONParse.cls",
        include_str!("north_star/corpus/JSONParse.cls"),
    ),
];

#[derive(Debug, Default, PartialEq, Eq)]
struct GrammarCensus {
    annotations: BTreeMap<String, usize>,
    annotation_arguments: usize,
    switches: usize,
    switch_arms: usize,
    switch_else_arms: usize,
    uninitialized_locals: usize,
    multi_declarator_locals: usize,
    multi_expression_for_clauses: usize,
    external_id_upserts: usize,
    multi_declarator_fields: usize,
    final_modifiers: usize,
    transient_modifiers: usize,
    soql_queries: usize,
    sosl_queries: usize,
    aggregate_select_items: usize,
    grouped_queries: usize,
    ordered_queries: usize,
    limited_queries: usize,
    offset_queries: usize,
}

impl GrammarCensus {
    fn count_modifiers(&mut self, modifiers: &[Modifier]) {
        self.final_modifiers += modifiers
            .iter()
            .filter(|modifier| **modifier == Modifier::Final)
            .count();
        self.transient_modifiers += modifiers
            .iter()
            .filter(|modifier| **modifier == Modifier::Transient)
            .count();
    }
}

impl<'ast> Visitor<'ast> for GrammarCensus {
    fn visit_class_declaration(&mut self, class: &'ast ast::ClassDeclaration) {
        self.count_modifiers(&class.modifiers);
        visit::walk_class_declaration(self, class);
    }

    fn visit_class_member(&mut self, member: &'ast ClassMember) {
        match member {
            ClassMember::Field(field) => self.count_modifiers(&field.modifiers),
            ClassMember::FieldGroup(group) => {
                self.multi_declarator_fields += 1;
                self.count_modifiers(&group.modifiers);
            }
            ClassMember::Property(property) => self.count_modifiers(&property.modifiers),
            ClassMember::Constructor(constructor) => {
                self.count_modifiers(&constructor.modifiers);
            }
            ClassMember::Method(method) => self.count_modifiers(&method.modifiers),
            ClassMember::Initializer(_) => {}
        }
        visit::walk_class_member(self, member);
    }

    fn visit_annotation(&mut self, annotation: &'ast ast::Annotation) {
        if matches!(
            annotation.kind,
            AnnotationKind::Other | AnnotationKind::SuppressWarnings | AnnotationKind::TestVisible
        ) {
            *self
                .annotations
                .entry(annotation.name.spelling.clone())
                .or_default() += 1;
            self.annotation_arguments += annotation.arguments.len();
        }
        visit::walk_annotation(self, annotation);
    }

    fn visit_statement(&mut self, statement: &'ast Statement) {
        match statement {
            Statement::LocalDeclaration {
                modifiers,
                declarators,
                ..
            } => {
                self.count_modifiers(modifiers);
                self.uninitialized_locals += declarators
                    .iter()
                    .filter(|declarator| declarator.initializer.is_none())
                    .count();
                self.multi_declarator_locals += usize::from(declarators.len() > 1);
            }
            Statement::Sequence { .. } => self.multi_expression_for_clauses += 1,
            Statement::Switch { arms, .. } => {
                self.switches += 1;
                self.switch_arms += arms.len();
                self.switch_else_arms += arms
                    .iter()
                    .filter(|arm| matches!(arm.labels, SwitchLabels::Else(_)))
                    .count();
            }
            Statement::Dml {
                external_id: Some(_),
                ..
            } => self.external_id_upserts += 1,
            _ => {}
        }
        visit::walk_statement(self, statement);
    }

    fn visit_expression(&mut self, expression: &'ast Expression) {
        match expression {
            Expression::Soql(query) => {
                self.soql_queries += 1;
                self.aggregate_select_items += query
                    .select
                    .iter()
                    .filter(|item| matches!(item, ast::SoqlSelectItem::Aggregate { .. }))
                    .count();
                self.grouped_queries += usize::from(!query.group_by.is_empty());
                self.ordered_queries += usize::from(!query.order_by.is_empty());
                self.limited_queries += usize::from(query.limit.is_some());
                self.offset_queries += usize::from(query.offset.is_some());
            }
            Expression::Sosl(_) => self.sosl_queries += 1,
            _ => {}
        }
        visit::walk_expression(self, expression);
    }
}

#[test]
fn north_star_grammar_census_is_comment_aware_and_stable() {
    let mut census = GrammarCensus::default();
    for (name, source) in NORTH_STAR {
        let program = parse(source).unwrap_or_else(|error| {
            panic!("{}", error.render(name, source));
        });
        census.visit_program(&program);
    }
    assert_eq!(
        census,
        GrammarCensus {
            annotations: BTreeMap::from([
                ("AuraEnabled".to_owned(), 11),
                ("InvocableMethod".to_owned(), 2),
                ("InvocableVariable".to_owned(), 36),
                ("NamespaceAccessible".to_owned(), 162),
                ("SuppressWarnings".to_owned(), 20),
                ("TestVisible".to_owned(), 28),
            ]),
            annotation_arguments: 115,
            switches: 8,
            switch_arms: 20,
            switch_else_arms: 2,
            uninitialized_locals: 48,
            multi_declarator_locals: 3,
            multi_expression_for_clauses: 0,
            external_id_upserts: 2,
            multi_declarator_fields: 3,
            final_modifiers: 75,
            transient_modifiers: 3,
            soql_queries: 5,
            sosl_queries: 0,
            aggregate_select_items: 1,
            grouped_queries: 0,
            ordered_queries: 0,
            limited_queries: 2,
            offset_queries: 0,
        }
    );
}

#[test]
fn annotations_switch_and_external_id_dml_preserve_lossless_syntax_and_phase_boundaries() {
    let annotation_source = "@AuraEnabled(cacheable=true label='Read') public class Service {}";
    let annotation_program = parse(annotation_source).unwrap();
    let annotation = &annotation_program.classes[0].annotations[0];
    assert_eq!(annotation.name.spelling, "AuraEnabled");
    assert_eq!(annotation.arguments.len(), 2);
    assert_eq!(
        &annotation_source[annotation.span.start..annotation.span.end],
        "@AuraEnabled(cacheable=true label='Read')"
    );
    let annotation_error = check(annotation_source).unwrap_err();
    assert!(annotation_error.message.contains("parsed but unsupported"));

    let switch_source =
        "switch on left ?? right { when 'a', null { System.debug('x'); } when else {} }";
    let switch_program = parse(switch_source).unwrap();
    let Statement::Switch { value, arms, span } = &switch_program.statements[0] else {
        panic!("expected a dedicated switch statement");
    };
    assert!(matches!(value, Expression::NullCoalesce { .. }));
    assert_eq!(arms.len(), 2);
    assert!(matches!(arms[1].labels, SwitchLabels::Else(_)));
    assert_eq!(&switch_source[span.start..span.end], switch_source);
    let switch_error = check(switch_source).unwrap_err();
    assert!(switch_error.message.contains("`switch on`/`when`"));

    let dml_source = "upsert records External_Key__c;";
    let dml_program = parse(dml_source).unwrap();
    let Statement::Dml {
        external_id: Some(external_id),
        ..
    } = &dml_program.statements[0]
    else {
        panic!("expected an external-ID DML node");
    };
    assert_eq!(external_id.spelling, "External_Key__c");
    let dml_error = check(dml_source).unwrap_err();
    assert!(dml_error.message.contains("unknown variable `records`"));
    assert!(
        !dml_error
            .message
            .contains("external-ID DML is parsed but unsupported")
    );
}

#[test]
fn uninitialized_multi_declarator_and_multi_for_forms_execute_in_source_order() {
    let source = r#"
        Integer marker = 0;
        Integer first = ++marker, second = first + ++marker, empty;
        Integer i;
        Integer j;
        Integer total;
        for (i = 0, j = 2; i < 2; i++, j--) {
            total = (total ?? 0) + j;
        }
        System.debug(marker);
        System.debug(first);
        System.debug(second);
        System.debug(empty);
        System.debug(total);
    "#;
    assert_eq!(execute(source).unwrap(), ["2", "1", "3", "null", "3"]);

    let duplicate = check("Integer first = 1, FIRST = 2;").unwrap_err();
    assert_eq!(duplicate.message, "duplicate variable `FIRST`");

    let checked = check("Integer i; Integer j; for (i = 0, j = 1; i < 1; i++, j--) {}").unwrap();
    let debug = Interpreter::new().debug_execute(&checked);
    assert!(debug.diagnostic.is_none());
    assert_eq!(
        debug.snapshots.len(),
        7,
        "the synthetic for-clause sequence must not add instrumentation"
    );
}

#[test]
fn remaining_modifiers_and_multi_fields_fail_in_the_semantic_phase() {
    for source in [
        "final Integer value = 1;",
        "transient Integer value = 1;",
        "public class Example { transient Integer value; }",
    ] {
        parse(source).unwrap();
        let error = check(source).unwrap_err();
        assert!(error.message.contains("parsed but unsupported"));
    }

    let fields = "public class Example { Integer first = 1, second; }";
    let program = parse(fields).unwrap();
    assert!(matches!(
        program.classes[0].members[0],
        ClassMember::FieldGroup(_)
    ));
    let error = check(fields).unwrap_err();
    assert!(error.message.contains("multi-declarator fields"));
}

#[test]
fn malformed_m21_forms_are_rejected_by_the_parser() {
    for source in [
        "@AuraEnabled(cacheable=) public class Example {}",
        "@TestSetup() public class Example {}",
        "switch on value { when else {} when 'late' {} }",
        "Integer first = 1, ;",
        "for (i = 0,; i < 1; i++) {}",
        "insert records External_Key__c;",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
}

#[test]
fn milestone21_example_runs_through_the_library_and_cli() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/milestone21.apex");
    let source = include_str!("../examples/milestone21.apex");
    assert_eq!(execute(source).unwrap(), ["6", "null"]);

    let output = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["run", path])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "6\nnull\n");
    assert!(output.stderr.is_empty());
}
