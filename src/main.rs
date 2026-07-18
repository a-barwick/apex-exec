use apex_exec::{check, execute, parse, test_runner::TestOptions, tokenize};
use std::{
    env, fs,
    io::{self, BufRead, BufReader, IsTerminal},
    path::{Path, PathBuf},
    process::ExitCode,
};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(usage)?;
    if command == "--help" || command == "-h" {
        println!("{}", usage());
        return Ok(ExitCode::SUCCESS);
    }
    if !matches!(
        command.as_str(),
        "run" | "tokens" | "ast" | "check" | "invoke" | "test" | "repl" | "lsp" | "dap" | "oracle"
    ) {
        return Err(format!("unknown command `{command}`\n\n{}", usage()));
    }

    if command == "repl" {
        if args.next().is_some() {
            return Err(usage());
        }
        return run_repl();
    }

    if command == "lsp" {
        let root = args.next().map(PathBuf::from);
        if args.next().is_some() {
            return Err(usage());
        }
        apex_exec::lsp::serve(
            BufReader::new(io::stdin().lock()),
            io::stdout().lock(),
            root,
        )
        .map_err(|error| format!("LSP transport failed: {error}"))?;
        return Ok(ExitCode::SUCCESS);
    }

    if command == "dap" {
        if args.next().is_some() {
            return Err(usage());
        }
        apex_exec::dap::serve(BufReader::new(io::stdin().lock()), io::stdout().lock())
            .map_err(|error| format!("DAP transport failed: {error}"))?;
        return Ok(ExitCode::SUCCESS);
    }

    if command == "oracle" {
        return run_oracle(args);
    }

    let path = args.next().ok_or_else(usage)?;

    if command == "invoke" {
        let target = args.next().ok_or_else(usage)?;
        if args.next().is_some() {
            return Err(usage());
        }
        let compilation = apex_exec::project::compile(&path).map_err(|error| error.render())?;
        for line in compilation
            .invoke(&target)
            .map_err(|error| error.render())?
        {
            println!("{line}");
        }
        return Ok(ExitCode::SUCCESS);
    }

    if command == "test" {
        let (options, junit_path) = parse_test_options(args)?;
        let compilation = apex_exec::project::compile(&path).map_err(|error| error.render())?;
        let report = apex_exec::test_runner::run(&compilation, &options)?;
        println!("{}", report.render_console());
        if let Some(junit_path) = junit_path {
            fs::write(&junit_path, report.to_junit_xml()).map_err(|error| {
                format!(
                    "failed to write JUnit report `{}`: {error}",
                    junit_path.display()
                )
            })?;
        }
        return Ok(if report.is_success() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
    if args.next().is_some() {
        return Err(usage());
    }

    if command == "check" && Path::new(&path).is_dir() {
        let compilation = apex_exec::project::compile(&path).map_err(|error| error.render())?;
        println!(
            "OK ({} classes, {} source files)",
            compilation.program.classes.len(),
            compilation.dependencies.files().count()
        );
        return Ok(ExitCode::SUCCESS);
    }

    let source =
        fs::read_to_string(&path).map_err(|error| format!("failed to read `{path}`: {error}"))?;

    let result = match command.as_str() {
        "tokens" => tokenize(&source).map(|tokens| {
            for token in tokens {
                println!(
                    "{:?} @ {}..{}",
                    token.kind, token.span.start, token.span.end
                );
            }
        }),
        "ast" => parse(&source).map(|program| println!("{program:#?}")),
        "check" => check(&source).map(|_| println!("OK")),
        "run" => execute(&source).map(|lines| {
            for line in lines {
                println!("{line}");
            }
        }),
        _ => unreachable!(),
    };

    result
        .map(|_| ExitCode::SUCCESS)
        .map_err(|diagnostic| diagnostic.render(&path, &source))
}

fn run_oracle(mut args: impl Iterator<Item = String>) -> Result<ExitCode, String> {
    let manifest_path = PathBuf::from(args.next().ok_or_else(usage)?);
    let mut target_org = None;
    let mut snapshot_path = None;
    let mut record_path = None;
    let mut report_path = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--target-org" => {
                set_once(
                    &mut target_org,
                    args.next()
                        .ok_or_else(|| "`--target-org` requires an alias or username".to_owned())?,
                    "`--target-org` was provided more than once",
                )?;
            }
            "--salesforce-snapshot" => {
                set_once(
                    &mut snapshot_path,
                    PathBuf::from(
                        args.next()
                            .ok_or_else(|| "`--salesforce-snapshot` requires a path".to_owned())?,
                    ),
                    "`--salesforce-snapshot` was provided more than once",
                )?;
            }
            "--record-salesforce" => {
                set_once(
                    &mut record_path,
                    PathBuf::from(
                        args.next()
                            .ok_or_else(|| "`--record-salesforce` requires a path".to_owned())?,
                    ),
                    "`--record-salesforce` was provided more than once",
                )?;
            }
            "--report" => {
                set_once(
                    &mut report_path,
                    PathBuf::from(
                        args.next()
                            .ok_or_else(|| "`--report` requires a path".to_owned())?,
                    ),
                    "`--report` was provided more than once",
                )?;
            }
            _ => return Err(format!("unknown oracle option `{argument}`\n\n{}", usage())),
        }
    }
    if target_org.is_some() == snapshot_path.is_some() {
        return Err(
            "oracle requires exactly one of `--target-org` or `--salesforce-snapshot`".to_owned(),
        );
    }
    if record_path.is_some() && target_org.is_none() {
        return Err("`--record-salesforce` requires `--target-org`".to_owned());
    }

    let manifest = apex_exec::oracle::ConformanceManifest::load(&manifest_path)?;
    let local = apex_exec::oracle::run_local(&manifest);
    let salesforce = match (target_org, snapshot_path) {
        (Some(target_org), None) => {
            let snapshot = apex_exec::oracle::run_salesforce(&manifest, &target_org)?;
            if let Some(path) = record_path {
                snapshot.write(path)?;
            }
            snapshot
        }
        (None, Some(path)) => apex_exec::oracle::OracleSnapshot::load(path)?,
        _ => unreachable!("exclusive provider selection was validated"),
    };
    let report = apex_exec::oracle::compare(&manifest, &local, &salesforce)?;
    println!("{}", report.render_console());
    if let Some(path) = report_path {
        report.write(path)?;
    }
    Ok(if report.is_match() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn set_once<T>(slot: &mut Option<T>, value: T, duplicate: &str) -> Result<(), String> {
    if slot.is_some() {
        return Err(duplicate.to_owned());
    }
    *slot = Some(value);
    Ok(())
}

fn run_repl() -> Result<ExitCode, String> {
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    if interactive {
        eprintln!("Apex Exec REPL — :reset clears state, :source shows it, :quit exits");
    }
    let mut session = apex_exec::repl::ReplSession::new();
    let mut pending = String::new();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("failed to read REPL input: {error}"))?;
        if pending.is_empty() && line.starts_with(':') {
            match line.trim() {
                ":quit" | ":exit" => break,
                ":reset" => {
                    session.reset();
                    if interactive {
                        eprintln!("state reset");
                    }
                }
                ":source" => println!("{}", session.source()),
                command => eprintln!("unknown REPL command `{command}`"),
            }
            continue;
        }
        pending.push_str(&line);
        pending.push('\n');
        let mut candidate = session.source().to_owned();
        if !candidate.is_empty() {
            candidate.push('\n');
        }
        candidate.push_str(&pending);
        match session.evaluate(&pending) {
            Ok(evaluation) => {
                for output in evaluation.output {
                    println!("{output}");
                }
                pending.clear();
            }
            Err(diagnostic) if incomplete_repl_input(&diagnostic, &candidate) => {}
            Err(diagnostic) => {
                eprintln!("{}", diagnostic.render("<repl>", &candidate));
                pending.clear();
            }
        }
    }
    if !pending.trim().is_empty() {
        return Err("incomplete Apex input at end of REPL stream".to_owned());
    }
    Ok(ExitCode::SUCCESS)
}

fn incomplete_repl_input(diagnostic: &apex_exec::diagnostic::Diagnostic, source: &str) -> bool {
    diagnostic.span.start >= source.trim_end().len()
        && (diagnostic.message.contains("expected")
            || diagnostic.message.contains("unterminated")
            || diagnostic.message.contains("end of input"))
}

fn parse_test_options(
    mut args: impl Iterator<Item = String>,
) -> Result<(TestOptions, Option<PathBuf>), String> {
    let mut options = TestOptions::default();
    let mut junit_path = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--filter" => {
                if options.filter.is_some() {
                    return Err("test filter was provided more than once".to_owned());
                }
                options.filter = Some(
                    args.next()
                        .ok_or_else(|| "`--filter` requires a value".to_owned())?,
                );
            }
            "--junit" => {
                if junit_path.is_some() {
                    return Err("`--junit` was provided more than once".to_owned());
                }
                junit_path = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| "`--junit` requires a path".to_owned())?,
                ));
            }
            "--jobs" => {
                let value = args
                    .next()
                    .ok_or_else(|| "`--jobs` requires a positive integer".to_owned())?;
                options.jobs = value
                    .parse::<usize>()
                    .ok()
                    .filter(|jobs| *jobs > 0)
                    .ok_or_else(|| "`--jobs` requires a positive integer".to_owned())?;
            }
            _ if argument.starts_with('-') => {
                return Err(format!("unknown test option `{argument}`\n\n{}", usage()));
            }
            _ => {
                if options.filter.is_some() {
                    return Err("test filter was provided more than once".to_owned());
                }
                options.filter = Some(argument);
            }
        }
    }
    Ok((options, junit_path))
}

fn usage() -> String {
    "Usage:\n  apex-exec <run|tokens|ast|check> <script.apex>\n  apex-exec check <sfdx-project-or-package-directory>\n  apex-exec invoke <sfdx-project-or-package-directory> <Class.method>\n  apex-exec test <sfdx-project-or-package-directory> [Class[.method]|glob] [--filter <pattern>] [--jobs <count>] [--junit <path>]\n  apex-exec oracle <manifest.json> (--target-org <alias> | --salesforce-snapshot <path>) [--record-salesforce <path>] [--report <path>]\n  apex-exec repl\n  apex-exec lsp [sfdx-project-or-package-directory]\n  apex-exec dap"
        .to_owned()
}
