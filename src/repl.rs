//! Persistent deterministic Apex REPL state.
//!
//! The REPL commits a snippet only after the complete accumulated source
//! checks and executes successfully. Replaying accepted source from the start
//! keeps compiler/runtime ownership simple while producing the same final
//! deterministic state for the supported language profile.

use crate::{diagnostic::Diagnostic, execute};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplEvaluation {
    pub output: Vec<String>,
    pub accepted_source: String,
}

#[derive(Clone, Debug, Default)]
pub struct ReplSession {
    source: String,
    output: Vec<String>,
}

impl ReplSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks and evaluates one snippet against all previously accepted input.
    ///
    /// Failed snippets do not alter session state. Only newly emitted debug
    /// output is returned.
    pub fn evaluate(&mut self, snippet: &str) -> Result<ReplEvaluation, Diagnostic> {
        let mut candidate = self.source.clone();
        if !candidate.is_empty() && !candidate.ends_with('\n') {
            candidate.push('\n');
        }
        candidate.push_str(snippet);
        let output = execute(&candidate)?;
        let new_output = output
            .strip_prefix(self.output.as_slice())
            .unwrap_or(output.as_slice())
            .to_vec();
        self.source = candidate;
        self.output = output;
        Ok(ReplEvaluation {
            output: new_output,
            accepted_source: self.source.clone(),
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn reset(&mut self) {
        self.source.clear();
        self.output.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_snippets_share_variables_and_emit_only_new_output() {
        let mut repl = ReplSession::new();
        assert_eq!(
            repl.evaluate("Integer total = 40;").unwrap().output,
            Vec::<String>::new()
        );
        assert_eq!(
            repl.evaluate("total = total + 2; System.debug(total);")
                .unwrap()
                .output,
            ["42"]
        );
        assert_eq!(
            repl.evaluate("System.debug(total + 1);").unwrap().output,
            ["43"]
        );
    }

    #[test]
    fn rejected_snippets_are_transactional_and_reset_discards_history() {
        let mut repl = ReplSession::new();
        repl.evaluate("String message = 'ready';").unwrap();
        assert!(repl.evaluate("message = 1;").is_err());
        assert_eq!(
            repl.evaluate("System.debug(message);").unwrap().output,
            ["ready"]
        );
        repl.reset();
        assert!(repl.evaluate("System.debug(message);").is_err());
    }
}
