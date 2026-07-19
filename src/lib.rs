pub mod ast;
pub mod ci;
pub mod compatibility;
pub mod dap;
pub mod debugger;
pub mod diagnostic;
pub mod editor;
pub mod enterprise;
pub mod hir;
pub mod hybrid;
pub mod lexer;
pub mod lsp;
pub mod oracle;
pub mod parser;
pub mod platform;
pub mod project;
mod protocol;
pub mod repl;
pub mod runtime;
pub mod semantic;
pub mod span;
pub mod test_runner;
pub mod token;

use ast::Program;
use diagnostic::Diagnostic;
use span::SourceId;
use token::Token;

pub fn tokenize(source: &str) -> Result<Vec<Token>, Diagnostic> {
    lexer::Lexer::new(source).tokenize()
}

pub(crate) fn tokenize_with_source(
    source: &str,
    source_id: SourceId,
) -> Result<Vec<Token>, Diagnostic> {
    lexer::Lexer::with_source(source, source_id).tokenize()
}

pub fn parse(source: &str) -> Result<Program, Diagnostic> {
    let tokens = tokenize(source)?;
    parser_from_lexer(tokens).parse_program()
}

pub(crate) fn parse_with_source(source: &str, source_id: SourceId) -> Result<Program, Diagnostic> {
    let tokens = tokenize_with_source(source, source_id)?;
    parser_from_lexer(tokens).parse_program()
}

pub(crate) fn parse_dynamic_soql(source: &str) -> Result<ast::SoqlQuery, Diagnostic> {
    let tokens = tokenize(source)?;
    parser_from_lexer(tokens).parse_soql_query()
}

fn parser_from_lexer(tokens: Vec<Token>) -> parser::Parser {
    parser::Parser::new(tokens)
        .expect("the lexer always emits one ordered, single-source, terminal EOF token stream")
}

pub fn check(source: &str) -> Result<hir::Program, Diagnostic> {
    let program = parse(source)?;
    semantic::check(&program)
}

pub fn execute(source: &str) -> Result<Vec<String>, Diagnostic> {
    let program = check(source)?;
    runtime::Interpreter::new().execute(&program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_parse_retains_anonymous_source_identity() {
        let program = parse("Integer value = 1;").unwrap();
        assert_eq!(program.statements[0].span().source_id, SourceId::ANONYMOUS);
    }
}
