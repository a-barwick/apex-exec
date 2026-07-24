use apex_exec::{
    check, execute,
    oracle::{ConformanceManifest, OracleProvider, compare, run_local},
    project,
    test_runner::{TestOptions, run as run_tests},
    token::TokenKind,
    tokenize,
};
use std::process::Command;

const PROJECT: &str = "examples/milestone19-project";
const MANIFEST: &str = "examples/milestone19-project/oracle-manifest.json";
const EXPECTED: &str = "1099511628031|15|15|3|false|2|1|48:3:8:2:1:2";

#[test]
fn lexes_every_m19_operator_with_maximal_munch_and_exact_spans() {
    let source = "a+=1;b-=1;c*=1;d/=1;e%=1;f&=1;g|=1;h^=1;i<<=1;j>>=1;k>>>=1;\
                  a&b|c^~d<<e>>f>>>g;Long value=1L;";
    let tokens = tokenize(source).unwrap();
    for (kind, spelling) in [
        (TokenKind::PlusEqual, "+="),
        (TokenKind::MinusEqual, "-="),
        (TokenKind::StarEqual, "*="),
        (TokenKind::SlashEqual, "/="),
        (TokenKind::PercentEqual, "%="),
        (TokenKind::AmpersandEqual, "&="),
        (TokenKind::PipeEqual, "|="),
        (TokenKind::CaretEqual, "^="),
        (TokenKind::ShiftLeftEqual, "<<="),
        (TokenKind::ShiftRightEqual, ">>="),
        (TokenKind::UnsignedShiftRightEqual, ">>>="),
        (TokenKind::Ampersand, "&"),
        (TokenKind::Pipe, "|"),
        (TokenKind::Caret, "^"),
        (TokenKind::Tilde, "~"),
        (TokenKind::ShiftLeft, "<<"),
        (TokenKind::ShiftRight, ">>"),
        (TokenKind::UnsignedShiftRight, ">>>"),
        (TokenKind::LongLiteral(1), "1L"),
    ] {
        let token = tokens
            .iter()
            .find(|token| token.kind == kind)
            .unwrap_or_else(|| panic!("missing {spelling}"));
        assert_eq!(&source[token.span.start..token.span.end], spelling);
    }
}

#[test]
fn adjacent_generic_closers_and_shift_precedence_share_the_token_stream() {
    let output = execute(
        r#"
        Map<String,List<List<Integer>>> nested =
            new Map<String,List<List<Integer>>>();
        Integer precedence = 1 | 2 ^ 3 & 1;
        Integer shifted = 1 + 2 << 3 + 1;
        System.debug(nested.size());
        System.debug(precedence);
        System.debug(shifted);
        "#,
    )
    .unwrap();
    assert_eq!(output, ["0", "3", "48"]);
}

#[test]
fn rejects_invalid_m19_operands_and_reports_integral_overflow() {
    for (source, expected) in [
        ("Decimal value = 1.5 & 1;", "cannot be applied"),
        ("Long value = 1L << 1L;", "cannot be applied"),
        ("Boolean value = true & 1;", "cannot be applied"),
        (
            "Integer value = 1; value += 1L;",
            "cannot assign Long to Integer",
        ),
        ("String value = 'x'; value <<= 1;", "cannot be applied"),
    ] {
        let error = check(source).unwrap_err();
        assert!(
            error.message.contains(expected),
            "{} did not contain {expected}",
            error.message
        );
    }

    for source in [
        "Integer value = 2147483647 + 1;",
        "Long value = 9223372036854775807L + 1L;",
        "Integer value = -2147483648; value--;",
        "Long value = -9223372036854775808L; value--;",
    ] {
        let error = execute(source).unwrap_err();
        assert_eq!(error.exception_type.as_deref(), Some("MathException"));
        assert!(error.message.contains("overflow"), "{}", error.message);
    }

    let error =
        execute("Long value = 2147483648L; Integer narrowed = (Integer)value;").unwrap_err();
    assert_eq!(error.exception_type.as_deref(), Some("TypeException"));
    assert!(error.message.contains("out of range"));
}

#[test]
fn every_compound_family_preserves_value_and_failure_semantics() {
    let output = execute(
        r#"
        Integer bits = 15;
        bits &= 7;
        bits |= 8;
        bits ^= 3;
        bits <<= 2;
        bits >>= 1;
        bits >>>= 2;

        Long wide = -1L;
        wide >>>= 60;
        String label = 'value=';
        label += wide;

        Integer stable = 5;
        try {
            stable += 1 / 0;
        } catch (MathException expected) {
            System.debug(stable);
        }
        try {
            stable /= 0;
        } catch (MathException expected) {
            System.debug(stable);
        }
        try {
            stable %= 0;
        } catch (MathException expected) {
            System.debug(stable);
        }

        System.debug(bits);
        System.debug(wide);
        System.debug(label);
        System.debug(1 == 1L);
        System.debug(1L == 1.0);
        "#,
    )
    .unwrap();
    assert_eq!(
        output,
        ["5", "5", "5", "6", "15", "value=15", "true", "true"]
    );
}

#[test]
fn scalar_switch_dispatches_the_maximum_long_label() {
    let output = execute(
        r#"
        Long value = 9223372036854775807L;
        switch on value {
            when 9223372036854775806L {
                System.debug('wrong');
            }
            when 9223372036854775807L {
                System.debug('maximum');
            }
            when else {
                System.debug('wrong');
            }
        }
        "#,
    )
    .unwrap();
    assert_eq!(output, ["maximum"]);
}

#[test]
fn project_invocation_tests_and_coverage_complete_the_m19_slice() {
    let compilation = project::compile(PROJECT).unwrap();
    assert_eq!(
        compilation.invoke("BitwiseProfile.run").unwrap(),
        [EXPECTED, EXPECTED]
    );

    let report = run_tests(&compilation, &TestOptions::default()).unwrap();
    assert!(report.is_success(), "{}", report.render_console());
    assert_eq!(report.tests.len(), 4);
    assert_eq!(report.coverage.covered_lines, report.coverage.total_lines);
    assert_eq!(
        report.coverage.covered_branches,
        report.coverage.total_branches
    );
}

#[test]
fn oracle_fixture_covers_compile_output_and_test_dimensions() {
    let manifest = ConformanceManifest::load(MANIFEST).unwrap();
    let local = run_local(&manifest);
    assert_eq!(local.fixtures.len(), 2);
    assert!(local.fixtures.iter().all(|fixture| fixture.compile.success));
    assert_eq!(local.fixtures[0].output, [EXPECTED, EXPECTED]);
    assert!(
        local.fixtures[1]
            .tests
            .iter()
            .all(|test| test.outcome == "pass")
    );

    let mut salesforce_shaped = local.clone();
    salesforce_shaped.provider = OracleProvider::Salesforce;
    salesforce_shaped.target = "recorded-m19-reference".to_owned();
    let report = compare(&manifest, &local, &salesforce_shaped).unwrap();
    assert!(report.is_match());
    assert_eq!(report.coverage.matched, 4);
    assert_eq!(report.coverage.total, 4);
}

#[test]
fn cli_runs_the_complete_m19_slice_end_to_end() {
    let binary = env!("CARGO_BIN_EXE_apex-exec");
    for arguments in [
        vec!["check", PROJECT],
        vec!["invoke", PROJECT, "BitwiseProfile.run"],
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
