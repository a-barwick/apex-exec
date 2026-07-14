use crate::{
    ast::{Expression, Program, Statement},
    diagnostic::Diagnostic,
};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    String(String),
    Boolean(bool),
    Integer(i64),
}

impl Value {
    fn display(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Boolean(value) => value.to_string(),
            Self::Integer(value) => value.to_string(),
        }
    }
}

pub struct Interpreter {
    variables: HashMap<String, Value>,
    output: Vec<String>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            output: Vec::new(),
        }
    }

    pub fn execute(mut self, program: &Program) -> Result<Vec<String>, Diagnostic> {
        for statement in &program.statements {
            match statement {
                Statement::VariableDeclaration {
                    name, initializer, ..
                } => {
                    let value = self.evaluate(initializer)?;
                    self.variables.insert(name.canonical.clone(), value);
                }
                Statement::Assignment { name, value, .. } => {
                    let value = self.evaluate(value)?;
                    self.variables.insert(name.canonical.clone(), value);
                }
                Statement::Debug { variable, .. } => {
                    let value = self.variables.get(&variable.canonical).ok_or_else(|| {
                        Diagnostic::new(
                            format!("unknown variable `{}`", variable.spelling),
                            variable.span,
                        )
                    })?;
                    self.output.push(value.display());
                }
            }
        }
        Ok(self.output)
    }

    fn evaluate(&self, expression: &Expression) -> Result<Value, Diagnostic> {
        match expression {
            Expression::StringLiteral(value, _) => Ok(Value::String(value.clone())),
            Expression::BooleanLiteral(value, _) => Ok(Value::Boolean(*value)),
            Expression::IntegerLiteral(value, _) => Ok(Value::Integer(*value)),
            Expression::Variable(identifier) => self
                .variables
                .get(&identifier.canonical)
                .cloned()
                .ok_or_else(|| {
                    Diagnostic::new(
                        format!("unknown variable `{}`", identifier.spelling),
                        identifier.span,
                    )
                }),
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}
