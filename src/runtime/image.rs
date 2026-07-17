use crate::{
    ast::{ClassDeclaration, MethodDeclaration},
    hir::Program,
};

/// Immutable checked compiler output shared by one execution.
///
/// Runtime-owned scopes, heaps, statics, traces, and host state remain on the
/// interpreter. This view only borrows checked program data, so preparing an
/// isolated execution does not clone the complete AST and HIR side tables.
#[derive(Clone, Copy)]
pub(super) struct RuntimeImage<'program> {
    program: &'program Program,
}

impl<'program> RuntimeImage<'program> {
    pub(super) fn new(program: &'program Program) -> Self {
        Self { program }
    }

    pub(super) fn program(self) -> &'program Program {
        self.program
    }

    pub(super) fn methods(self) -> &'program [MethodDeclaration] {
        &self.program.methods
    }

    pub(super) fn classes(self) -> &'program [ClassDeclaration] {
        &self.program.classes
    }
}
