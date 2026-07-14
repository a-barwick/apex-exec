use apex_exec::{check, execute, parse, tokenize};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(usage)?;
    if command == "--help" || command == "-h" {
        println!("{}", usage());
        return Ok(());
    }
    let path = args.next().ok_or_else(usage)?;
    if args.next().is_some() {
        return Err(usage());
    }
    if !matches!(command.as_str(), "run" | "tokens" | "ast" | "check") {
        return Err(format!("unknown command `{command}`\n\n{}", usage()));
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

    result.map_err(|diagnostic| diagnostic.render(&path, &source))
}

fn usage() -> String {
    "Usage: apex-exec <run|tokens|ast|check> <script.apex>".to_owned()
}
