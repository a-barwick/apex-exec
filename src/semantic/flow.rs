use crate::ast::{Statement, TypeName};

pub(super) fn statement_definitely_returns_or_throws(statement: &Statement) -> bool {
    let completions = statement_completions(statement);
    !completions.normal
        && !completions.breaks
        && !completions.continues
        && (completions.returns || completions.throws)
}

#[derive(Clone, Copy, Debug, Default)]
struct Completions {
    normal: bool,
    returns: bool,
    throws: bool,
    breaks: bool,
    continues: bool,
}

impl Completions {
    fn normal() -> Self {
        Self {
            normal: true,
            ..Self::default()
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            normal: self.normal || other.normal,
            returns: self.returns || other.returns,
            throws: self.throws || other.throws,
            breaks: self.breaks || other.breaks,
            continues: self.continues || other.continues,
        }
    }

    fn then(self, next: Self) -> Self {
        Self {
            normal: self.normal && next.normal,
            returns: self.returns || (self.normal && next.returns),
            throws: self.throws || (self.normal && next.throws),
            breaks: self.breaks || (self.normal && next.breaks),
            continues: self.continues || (self.normal && next.continues),
        }
    }

    fn without_throw(self) -> Self {
        Self {
            throws: false,
            ..self
        }
    }
}

fn statement_completions(statement: &Statement) -> Completions {
    match statement {
        Statement::Return { .. } => Completions {
            returns: true,
            ..Completions::default()
        },
        Statement::Throw { .. } => Completions {
            throws: true,
            ..Completions::default()
        },
        Statement::Break { .. } => Completions {
            breaks: true,
            ..Completions::default()
        },
        Statement::Continue { .. } => Completions {
            continues: true,
            ..Completions::default()
        },
        Statement::Block { statements, .. } | Statement::Sequence { statements, .. } => statements
            .iter()
            .fold(Completions::normal(), |current, statement| {
                current.then(statement_completions(statement))
            }),
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => statement_completions(then_branch).union(
            else_branch
                .as_deref()
                .map_or_else(Completions::normal, statement_completions),
        ),
        Statement::While { body, .. }
        | Statement::For { body, .. }
        | Statement::ForEach { body, .. } => {
            let body = statement_completions(body);
            Completions {
                normal: true,
                returns: body.returns,
                throws: body.throws,
                breaks: false,
                continues: false,
            }
        }
        Statement::DoWhile { body, .. } => {
            let body = statement_completions(body);
            Completions {
                normal: body.normal || body.breaks || body.continues,
                returns: body.returns,
                throws: body.throws,
                breaks: false,
                continues: false,
            }
        }
        Statement::Try {
            try_block,
            catches,
            finally_block,
            ..
        } => {
            let try_completions = statement_completions(try_block);
            let mut pending = try_completions.without_throw();
            if catches.is_empty() {
                pending.throws = try_completions.throws;
            } else {
                for catch in catches {
                    pending = pending.union(statement_completions(&catch.body));
                }
                if !catches
                    .iter()
                    .any(|catch| catch.exception_type == TypeName::Exception)
                {
                    pending.throws = true;
                }
            }

            let Some(finally_block) = finally_block else {
                return pending;
            };
            let finally = statement_completions(finally_block);
            let mut result = Completions {
                normal: false,
                returns: finally.returns,
                throws: finally.throws,
                breaks: finally.breaks,
                continues: finally.continues,
            };
            if finally.normal {
                result = result.union(pending);
            }
            result
        }
        _ => Completions::normal(),
    }
}
