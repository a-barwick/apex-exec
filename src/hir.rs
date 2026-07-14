use crate::{ast, span::Span};
use std::{collections::HashMap, ops::Deref};

/// The checked program consumed by execution.
///
/// Parsed syntax stays immutable and free of semantic annotations. Resolution
/// results live here so the runtime never repeats overload selection.
#[derive(Clone, Debug)]
pub struct Program {
    ast: ast::Program,
    expression_types: HashMap<Span, ExpressionType>,
    calls: HashMap<Span, CallTarget>,
    references: HashMap<Span, ReferenceTarget>,
    members: HashMap<Span, MemberTarget>,
}

impl Program {
    pub(crate) fn new(
        ast: ast::Program,
        expression_types: HashMap<Span, ExpressionType>,
        calls: HashMap<Span, CallTarget>,
        references: HashMap<Span, ReferenceTarget>,
        members: HashMap<Span, MemberTarget>,
    ) -> Self {
        Self {
            ast,
            expression_types,
            calls,
            references,
            members,
        }
    }

    pub fn ast(&self) -> &ast::Program {
        &self.ast
    }

    pub fn expression_type(&self, span: Span) -> Option<&ExpressionType> {
        self.expression_types.get(&span)
    }

    pub fn call_target(&self, span: Span) -> Option<CallTarget> {
        self.calls.get(&span).copied()
    }

    pub fn reference_target(&self, span: Span) -> Option<ReferenceTarget> {
        self.references.get(&span).copied()
    }

    pub fn member_target(&self, span: Span) -> Option<MemberTarget> {
        self.members.get(&span).copied()
    }
}

impl Deref for Program {
    type Target = ast::Program;

    fn deref(&self) -> &Self::Target {
        &self.ast
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpressionType {
    Value(ast::TypeName),
    Null,
    Void,
}

impl ExpressionType {
    pub fn apex_name(&self) -> String {
        match self {
            Self::Value(ty) => ty.apex_name(),
            Self::Null => "null".to_owned(),
            Self::Void => "void".to_owned(),
        }
    }

    pub(crate) fn name(&self) -> String {
        self.apex_name()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallTarget {
    TopLevelMethod(usize),
    StaticMethod(ClassMemberId),
    InstanceMethod(ClassMemberId),
    SuperMethod(ClassMemberId),
    Constructor {
        class_id: usize,
        member_id: Option<usize>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClassMemberId {
    pub class_id: usize,
    pub member_id: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceTarget {
    Local,
    This,
    Super(usize),
    InstanceMember(ClassMemberId),
    StaticMember(ClassMemberId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberTarget {
    Instance(ClassMemberId),
    Static(ClassMemberId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expression, Statement};

    #[test]
    fn records_types_and_call_targets_outside_the_parsed_ast() {
        let parsed = crate::parse(
            "Integer doubleIt(Integer value) { return value * 2; } Integer result = doubleIt(3);",
        )
        .unwrap();
        let Statement::VariableDeclaration { initializer, .. } = &parsed.statements[0] else {
            panic!("expected variable declaration");
        };
        let Expression::FunctionCall { span, .. } = initializer else {
            panic!("expected function call");
        };

        let checked = crate::semantic::check(&parsed).unwrap();
        assert_eq!(
            checked.expression_type(*span),
            Some(&ExpressionType::Value(ast::TypeName::Integer))
        );
        assert_eq!(
            checked.call_target(*span),
            Some(CallTarget::TopLevelMethod(0))
        );
    }
}
