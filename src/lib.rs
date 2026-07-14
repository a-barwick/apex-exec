pub mod ast;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod runtime;
pub mod semantic;
pub mod span;
pub mod token;

use ast::Program;
use diagnostic::Diagnostic;
use token::Token;

pub fn tokenize(source: &str) -> Result<Vec<Token>, Diagnostic> {
    lexer::Lexer::new(source).tokenize()
}

pub fn parse(source: &str) -> Result<Program, Diagnostic> {
    let tokens = tokenize(source)?;
    parser::Parser::new(tokens).parse_program()
}

pub fn check(source: &str) -> Result<Program, Diagnostic> {
    let program = parse(source)?;
    semantic::check(&program)?;
    Ok(program)
}

pub fn execute(source: &str) -> Result<Vec<String>, Diagnostic> {
    let program = check(source)?;
    runtime::Interpreter::new().execute(&program)
}
