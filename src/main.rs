use apex_exec::{check, execute, parse, test_runner::TestOptions, tokenize};
use std::{
    env, fs,
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
    let path = args.next().ok_or_else(usage)?;
    if !matches!(
        command.as_str(),
        "run" | "tokens" | "ast" | "check" | "invoke" | "test"
    ) {
        return Err(format!("unknown command `{command}`\n\n{}", usage()));
    }

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
    "Usage:\n  apex-exec <run|tokens|ast|check> <script.apex>\n  apex-exec check <sfdx-project-or-package-directory>\n  apex-exec invoke <sfdx-project-or-package-directory> <Class.method>\n  apex-exec test <sfdx-project-or-package-directory> [Class[.method]|glob] [--filter <pattern>] [--jobs <count>] [--junit <path>]"
        .to_owned()
}
