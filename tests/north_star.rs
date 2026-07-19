use apex_exec::{diagnostic::Diagnostic, parse, tokenize};

struct Fixture {
    name: &'static str,
    source: &'static str,
    lines: usize,
    bytes: usize,
    fnv1a64: u64,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "SOQL.cls",
        source: include_str!("north_star/corpus/SOQL.cls"),
        lines: 3_498,
        bytes: 121_807,
        fnv1a64: 0x7d5a_6e08_b156_f805,
    },
    Fixture {
        name: "Logger.cls",
        source: include_str!("north_star/corpus/Logger.cls"),
        lines: 4_104,
        bytes: 213_124,
        fnv1a64: 0x2f8c_262c_ee82_fe47,
    },
    Fixture {
        name: "Rollup.cls",
        source: include_str!("north_star/corpus/Rollup.cls"),
        lines: 3_120,
        bytes: 125_837,
        fnv1a64: 0x68c1_4e1b_699f_d984,
    },
    Fixture {
        name: "RollupService.cls",
        source: include_str!("north_star/corpus/RollupService.cls"),
        lines: 1_789,
        bytes: 71_611,
        fnv1a64: 0xfbdc_3ca2_2b7b_f8f5,
    },
    Fixture {
        name: "fflib_SObjectDomain.cls",
        source: include_str!("north_star/corpus/fflib_SObjectDomain.cls"),
        lines: 1_125,
        bytes: 35_164,
        fnv1a64: 0xac7a_17f3_d22f_4ef9,
    },
    Fixture {
        name: "Puff.cls",
        source: include_str!("north_star/corpus/Puff.cls"),
        lines: 763,
        bytes: 36_226,
        fnv1a64: 0x9d1a_e293_ee9d_ab30,
    },
    Fixture {
        name: "JSONParse.cls",
        source: include_str!("north_star/corpus/JSONParse.cls"),
        lines: 341,
        bytes: 10_767,
        fnv1a64: 0x7616_e58b_ed96_c436,
    },
];

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn fixture(name: &str) -> &'static Fixture {
    FIXTURES
        .iter()
        .find(|fixture| fixture.name == name)
        .expect("North Star fixture should be registered")
}

fn require_lex(name: &str) {
    let fixture = fixture(name);
    if let Err(diagnostic) = tokenize(fixture.source) {
        panic!("\n{}", diagnostic.render(fixture.name, fixture.source));
    }
}

fn require_parse(name: &str) {
    let fixture = fixture(name);
    if let Err(diagnostic) = parse(fixture.source) {
        panic!("\n{}", diagnostic.render(fixture.name, fixture.source));
    }
}

fn highest_stage(fixture: &Fixture) -> (&'static str, Option<Diagnostic>) {
    if let Err(diagnostic) = tokenize(fixture.source) {
        return ("source", Some(diagnostic));
    }
    if let Err(diagnostic) = parse(fixture.source) {
        return ("lex", Some(diagnostic));
    }
    ("parse", None)
}

#[test]
fn north_star_corpus_is_pinned_and_complete() {
    assert_eq!(FIXTURES.len(), 7);
    assert_eq!(
        FIXTURES.iter().map(|fixture| fixture.lines).sum::<usize>(),
        14_740
    );
    assert_eq!(
        FIXTURES.iter().map(|fixture| fixture.bytes).sum::<usize>(),
        614_536
    );

    for fixture in FIXTURES {
        assert_eq!(
            fixture.source.lines().count(),
            fixture.lines,
            "{} line count",
            fixture.name
        );
        assert_eq!(
            fixture.source.len(),
            fixture.bytes,
            "{} byte count",
            fixture.name
        );
        assert_eq!(
            fnv1a64(fixture.source.as_bytes()),
            fixture.fnv1a64,
            "{} content fingerprint; update provenance if this change is intentional",
            fixture.name
        );
    }
}

#[test]
fn reports_current_north_star_progress() {
    println!("{:<26} {:<8} next diagnostic", "fixture", "stage");
    for fixture in FIXTURES {
        let (stage, diagnostic) = highest_stage(fixture);
        let detail = diagnostic.map_or_else(
            || "accepted".to_owned(),
            |diagnostic| format!("{} at byte {}", diagnostic.message, diagnostic.span.start),
        );
        println!("{:<26} {:<8} {}", fixture.name, stage, detail);
    }
}

macro_rules! north_star_goals {
    ($lex_test:ident, $parse_test:ident, $name:literal) => {
        #[test]
        fn $lex_test() {
            require_lex($name);
        }

        #[test]
        fn $parse_test() {
            require_parse($name);
        }
    };
}

north_star_goals!(lexes_soql, parses_soql, "SOQL.cls");
north_star_goals!(lexes_logger, parses_logger, "Logger.cls");
north_star_goals!(lexes_rollup, parses_rollup, "Rollup.cls");
north_star_goals!(
    lexes_rollup_service,
    parses_rollup_service,
    "RollupService.cls"
);
north_star_goals!(
    lexes_fflib_sobject_domain,
    parses_fflib_sobject_domain,
    "fflib_SObjectDomain.cls"
);
north_star_goals!(lexes_puff, parses_puff, "Puff.cls");
north_star_goals!(lexes_json_parse, parses_json_parse, "JSONParse.cls");
