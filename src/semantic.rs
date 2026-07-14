use crate::{
    ast::{Expression, Program, Statement, TypeName},
    diagnostic::Diagnostic,
};
use std::collections::HashMap;

pub fn check(program: &Program) -> Result<(), Diagnostic> {
    let mut symbols: HashMap<String, TypeName> = HashMap::new();
    for statement in &program.statements {
        match statement {
            Statement::VariableDeclaration {
                ty,
                name,
                initializer,
                ..
            } => {
                if symbols.contains_key(&name.canonical) {
                    return Err(Diagnostic::new(
                        format!("duplicate variable `{}`", name.spelling),
                        name.span,
                    ));
                }
                let initializer_type = expression_type(initializer, &symbols)?;
                if *ty != initializer_type {
                    return Err(Diagnostic::new(
                        format!(
                            "cannot assign {} to {}",
                            initializer_type.apex_name(),
                            ty.apex_name()
                        ),
                        initializer.span(),
                    ));
                }
                symbols.insert(name.canonical.clone(), *ty);
            }
            Statement::Assignment { name, value, .. } => {
                let expected = symbols.get(&name.canonical).copied().ok_or_else(|| {
                    Diagnostic::new(format!("unknown variable `{}`", name.spelling), name.span)
                })?;
                let actual = expression_type(value, &symbols)?;
                if expected != actual {
                    return Err(Diagnostic::new(
                        format!(
                            "cannot assign {} to {}",
                            actual.apex_name(),
                            expected.apex_name()
                        ),
                        value.span(),
                    ));
                }
            }
            Statement::Debug { variable, .. } => {
                if !symbols.contains_key(&variable.canonical) {
                    return Err(Diagnostic::new(
                        format!("unknown variable `{}`", variable.spelling),
                        variable.span,
                    ));
                }
            }
        }
    }
    Ok(())
}

fn expression_type(
    expression: &Expression,
    symbols: &HashMap<String, TypeName>,
) -> Result<TypeName, Diagnostic> {
    match expression {
        Expression::StringLiteral(..) => Ok(TypeName::String),
        Expression::BooleanLiteral(..) => Ok(TypeName::Boolean),
        Expression::IntegerLiteral(..) => Ok(TypeName::Integer),
        Expression::Variable(identifier) => {
            symbols.get(&identifier.canonical).copied().ok_or_else(|| {
                Diagnostic::new(
                    format!("unknown variable `{}`", identifier.spelling),
                    identifier.span,
                )
            })
        }
    }
}
