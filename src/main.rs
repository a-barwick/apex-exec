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
        "run"
            | "tokens"
            | "ast"
            | "check"
            | "invoke"
            | "test"
            | "repl"
            | "lsp"
            | "dap"
            | "oracle"
            | "ci"
            | "hybrid"
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

    if command == "ci" {
        return run_ci(args);
    }

    if command == "hybrid" {
        return run_hybrid(args);
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

fn run_ci(mut args: impl Iterator<Item = String>) -> Result<ExitCode, String> {
    let subcommand = args.next().ok_or_else(usage)?;
    match subcommand.as_str() {
        "manifest" => {
            let project = PathBuf::from(args.next().ok_or_else(usage)?);
            let mut output = None;
            let mut changed_files = Vec::new();
            let mut changed_list = None;
            let mut shards = 1usize;
            let mut jobs = 1usize;
            let mut junit = None;
            let mut sarif = None;
            let mut coverage = None;
            let mut min_line_coverage = None;
            let mut min_branch_coverage = None;
            let mut max_duration_ms = None;
            let mut compatibility_report = None;
            let mut min_compatibility = None;
            while let Some(argument) = args.next() {
                match argument.as_str() {
                    "--output" => set_once(
                        &mut output,
                        PathBuf::from(required_value(&mut args, "--output")?),
                        "`--output` was provided more than once",
                    )?,
                    "--changed" => {
                        changed_files.push(PathBuf::from(required_value(&mut args, "--changed")?))
                    }
                    "--changed-list" => set_once(
                        &mut changed_list,
                        PathBuf::from(required_value(&mut args, "--changed-list")?),
                        "`--changed-list` was provided more than once",
                    )?,
                    "--shards" => {
                        shards = positive_usize(required_value(&mut args, "--shards")?, "--shards")?
                    }
                    "--jobs" => {
                        jobs = positive_usize(required_value(&mut args, "--jobs")?, "--jobs")?
                    }
                    "--junit" => set_once(
                        &mut junit,
                        PathBuf::from(required_value(&mut args, "--junit")?),
                        "`--junit` was provided more than once",
                    )?,
                    "--sarif" => set_once(
                        &mut sarif,
                        PathBuf::from(required_value(&mut args, "--sarif")?),
                        "`--sarif` was provided more than once",
                    )?,
                    "--coverage" => set_once(
                        &mut coverage,
                        PathBuf::from(required_value(&mut args, "--coverage")?),
                        "`--coverage` was provided more than once",
                    )?,
                    "--min-line-coverage" => {
                        set_once(
                            &mut min_line_coverage,
                            percentage_value(
                                required_value(&mut args, "--min-line-coverage")?,
                                "--min-line-coverage",
                            )?,
                            "`--min-line-coverage` was provided more than once",
                        )?;
                    }
                    "--min-branch-coverage" => {
                        set_once(
                            &mut min_branch_coverage,
                            percentage_value(
                                required_value(&mut args, "--min-branch-coverage")?,
                                "--min-branch-coverage",
                            )?,
                            "`--min-branch-coverage` was provided more than once",
                        )?;
                    }
                    "--max-duration-ms" => {
                        set_once(
                            &mut max_duration_ms,
                            required_value(&mut args, "--max-duration-ms")?
                                .parse::<u64>()
                                .map_err(|_| {
                                    "`--max-duration-ms` requires a non-negative integer".to_owned()
                                })?,
                            "`--max-duration-ms` was provided more than once",
                        )?;
                    }
                    "--compatibility-report" => set_once(
                        &mut compatibility_report,
                        PathBuf::from(required_value(&mut args, "--compatibility-report")?),
                        "`--compatibility-report` was provided more than once",
                    )?,
                    "--min-compatibility" => {
                        set_once(
                            &mut min_compatibility,
                            percentage_value(
                                required_value(&mut args, "--min-compatibility")?,
                                "--min-compatibility",
                            )?,
                            "`--min-compatibility` was provided more than once",
                        )?;
                    }
                    _ => {
                        return Err(format!(
                            "unknown CI manifest option `{argument}`\n\n{}",
                            usage()
                        ));
                    }
                }
            }
            let output = output.ok_or_else(|| "`ci manifest` requires `--output`".to_owned())?;
            if let Some(path) = changed_list {
                changed_files.extend(read_changed_list(&path)?);
            }
            let mut manifest = apex_exec::ci::CiManifest::generate(project)?;
            manifest.changed_files = changed_files;
            manifest.shard.total = shards;
            manifest.jobs = jobs;
            if junit.is_some() {
                manifest.reports.junit = junit;
            }
            if sarif.is_some() {
                manifest.reports.sarif = sarif;
            }
            if coverage.is_some() {
                manifest.reports.coverage = coverage;
            }
            manifest.policy.min_line_coverage = min_line_coverage;
            manifest.policy.min_branch_coverage = min_branch_coverage;
            manifest.policy.max_duration_ms = max_duration_ms;
            manifest.policy.compatibility_report = compatibility_report;
            manifest.policy.min_compatibility = min_compatibility;
            manifest.refresh_inputs()?;
            manifest.write(&output)?;
            println!(
                "Wrote hermetic CI manifest {} ({} inputs)",
                output.display(),
                manifest.inputs.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        "run" => {
            let manifest_path = PathBuf::from(args.next().ok_or_else(usage)?);
            let mut options = apex_exec::ci::CiRunOptions::default();
            let mut changed_list = None;
            while let Some(argument) = args.next() {
                match argument.as_str() {
                    "--cache-dir" => {
                        set_once(
                            &mut options.cache_dir,
                            PathBuf::from(required_value(&mut args, "--cache-dir")?),
                            "`--cache-dir` was provided more than once",
                        )?;
                    }
                    "--shard" => {
                        set_once(
                            &mut options.shard,
                            parse_shard(&required_value(&mut args, "--shard")?)?,
                            "`--shard` was provided more than once",
                        )?;
                    }
                    "--changed-list" => set_once(
                        &mut changed_list,
                        PathBuf::from(required_value(&mut args, "--changed-list")?),
                        "`--changed-list` was provided more than once",
                    )?,
                    "--no-cache" => options.no_cache = true,
                    "--replay" => options.replay_only = true,
                    _ => {
                        return Err(format!("unknown CI run option `{argument}`\n\n{}", usage()));
                    }
                }
            }
            if options.no_cache && options.replay_only {
                return Err("`--no-cache` and `--replay` cannot be combined".to_owned());
            }
            let mut manifest = apex_exec::ci::CiManifest::load(manifest_path)?;
            if let Some(path) = changed_list {
                manifest.changed_files = read_changed_list(&path)?;
            }
            let result = apex_exec::ci::run(&manifest, &options)?;
            println!("{}", result.render_console());
            Ok(if result.is_success() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        "integrations" => {
            let manifest = PathBuf::from(args.next().ok_or_else(usage)?);
            let output = PathBuf::from(args.next().ok_or_else(usage)?);
            if args.next().is_some() {
                return Err(usage());
            }
            for path in apex_exec::ci::write_integrations(output, manifest)? {
                println!("Wrote {}", path.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(format!(
            "unknown CI subcommand `{subcommand}`\n\n{}",
            usage()
        )),
    }
}

fn run_hybrid(mut args: impl Iterator<Item = String>) -> Result<ExitCode, String> {
    let manifest_path = PathBuf::from(args.next().ok_or_else(usage)?);
    let mut target_org = None;
    let mut snapshot_path = None;
    let mut record_path = None;
    let mut report_path = None;
    let mut ci_options = apex_exec::ci::CiRunOptions::default();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--target-org" => set_once(
                &mut target_org,
                required_value(&mut args, "--target-org")?,
                "`--target-org` was provided more than once",
            )?,
            "--validation-snapshot" => set_once(
                &mut snapshot_path,
                PathBuf::from(required_value(&mut args, "--validation-snapshot")?),
                "`--validation-snapshot` was provided more than once",
            )?,
            "--record-validation" => set_once(
                &mut record_path,
                PathBuf::from(required_value(&mut args, "--record-validation")?),
                "`--record-validation` was provided more than once",
            )?,
            "--report" => set_once(
                &mut report_path,
                PathBuf::from(required_value(&mut args, "--report")?),
                "`--report` was provided more than once",
            )?,
            "--cache-dir" => set_once(
                &mut ci_options.cache_dir,
                PathBuf::from(required_value(&mut args, "--cache-dir")?),
                "`--cache-dir` was provided more than once",
            )?,
            "--no-cache" => ci_options.no_cache = true,
            "--replay" => ci_options.replay_only = true,
            _ => {
                return Err(format!("unknown hybrid option `{argument}`\n\n{}", usage()));
            }
        }
    }
    if target_org.is_some() == snapshot_path.is_some() {
        return Err(
            "hybrid validation requires exactly one of `--target-org` or `--validation-snapshot`"
                .to_owned(),
        );
    }
    if record_path.is_some() && target_org.is_none() {
        return Err("`--record-validation` requires `--target-org`".to_owned());
    }
    if ci_options.no_cache && ci_options.replay_only {
        return Err("`--no-cache` and `--replay` cannot be combined".to_owned());
    }
    let source = match (target_org, snapshot_path) {
        (Some(target), None) => apex_exec::hybrid::ValidationSource::TargetOrg(target),
        (None, Some(path)) => apex_exec::hybrid::ValidationSource::Snapshot(path),
        _ => unreachable!("exclusive validation source was checked"),
    };
    let manifest = apex_exec::ci::CiManifest::load(manifest_path)?;
    let outcome = apex_exec::hybrid::run(
        &manifest,
        &source,
        &apex_exec::hybrid::HybridRunOptions { ci: ci_options },
    )?;
    println!("{}", outcome.report.render_console());
    if let Some(path) = record_path {
        outcome.validation_snapshot.write(path)?;
    }
    if let Some(path) = report_path {
        outcome.report.write(path)?;
    }
    Ok(if outcome.report.is_ready() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn required_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("`{option}` requires a value"))
}

fn positive_usize(value: String, option: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("`{option}` requires a positive integer"))
}

fn percentage_value(value: String, option: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && (0.0..=100.0).contains(value))
        .ok_or_else(|| format!("`{option}` requires a percentage between 0 and 100"))
}

fn parse_shard(value: &str) -> Result<apex_exec::ci::CiShard, String> {
    let (index, total) = value
        .split_once('/')
        .ok_or_else(|| "`--shard` requires zero-based INDEX/TOTAL".to_owned())?;
    let index = index
        .parse::<usize>()
        .map_err(|_| "`--shard` requires zero-based INDEX/TOTAL".to_owned())?;
    let total = positive_usize(total.to_owned(), "--shard")?;
    if index >= total {
        return Err("`--shard` index must be below its total".to_owned());
    }
    Ok(apex_exec::ci::CiShard { index, total })
}

fn read_changed_list(path: &Path) -> Result<Vec<PathBuf>, String> {
    let source = fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read changed-file list `{}`: {error}",
            path.display()
        )
    })?;
    Ok(source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect())
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
    "Usage:\n  apex-exec <run|tokens|ast|check> <script.apex>\n  apex-exec check <sfdx-project-or-package-directory>\n  apex-exec invoke <sfdx-project-or-package-directory> <Class.method>\n  apex-exec test <sfdx-project-or-package-directory> [Class[.method]|glob] [--filter <pattern>] [--jobs <count>] [--junit <path>]\n  apex-exec oracle <manifest.json> (--target-org <alias> | --salesforce-snapshot <path>) [--record-salesforce <path>] [--report <path>]\n  apex-exec ci manifest <project> --output <manifest.json> [--changed <relative-path> | --changed-list <path>] [--shards <count>] [--jobs <count>] [--junit <path>] [--sarif <path>] [--coverage <path>] [--min-line-coverage <percent>] [--min-branch-coverage <percent>] [--max-duration-ms <ms>] [--compatibility-report <path> --min-compatibility <percent>]\n  apex-exec ci run <manifest.json> [--changed-list <path>] [--cache-dir <path>] [--shard <index>/<total>] [--replay | --no-cache]\n  apex-exec ci integrations <manifest.json> <output-directory>\n  apex-exec hybrid <ci-manifest.json> (--target-org <alias> | --validation-snapshot <path>) [--record-validation <path>] [--report <path>] [--cache-dir <path>] [--replay | --no-cache]\n  apex-exec repl\n  apex-exec lsp [sfdx-project-or-package-directory]\n  apex-exec dap"
        .to_owned()
}
