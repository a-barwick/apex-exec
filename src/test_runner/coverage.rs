use super::{CoverageReport, FileCoverage};
use crate::{
    ast::{ClassMember, Statement},
    project::Compilation,
    runtime::{BranchHits, ExecutionTrace},
    span::Span,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub(super) fn build_coverage<'a>(
    compilation: &Compilation,
    traces: impl Iterator<Item = &'a ExecutionTrace>,
) -> CoverageReport {
    let mut statement_spans = BTreeSet::new();
    let mut branch_spans = BTreeSet::new();
    collect_coverage_candidates(compilation, &mut statement_spans, &mut branch_spans);

    let mut executed_statements = BTreeSet::<Span>::new();
    let mut branches = BTreeMap::<Span, BranchHits>::new();
    for trace in traces {
        executed_statements.extend(trace.executed_statements.iter().copied());
        for (span, hits) in &trace.branches {
            let aggregate = branches.entry(*span).or_default();
            aggregate.true_hits += hits.true_hits;
            aggregate.false_hits += hits.false_hits;
        }
    }

    #[derive(Default)]
    struct FileBuilder {
        total_lines: BTreeSet<usize>,
        covered_lines: BTreeSet<usize>,
        total_branches: usize,
        covered_branches: usize,
    }

    let mut files = BTreeMap::<PathBuf, FileBuilder>::new();
    for span in statement_spans {
        let Some((path, line)) = compilation.source_location(span) else {
            continue;
        };
        let path = relative_path(compilation, path);
        let file = files.entry(path).or_default();
        file.total_lines.insert(line);
        if executed_statements.contains(&span) {
            file.covered_lines.insert(line);
        }
    }
    for span in branch_spans {
        let Some((path, _)) = compilation.source_location(span) else {
            continue;
        };
        let path = relative_path(compilation, path);
        let file = files.entry(path).or_default();
        file.total_branches += 2;
        let hits = branches.get(&span).copied().unwrap_or_default();
        file.covered_branches += usize::from(hits.true_hits > 0) + usize::from(hits.false_hits > 0);
    }

    let files = files
        .into_iter()
        .map(|(path, file)| FileCoverage {
            path,
            total_lines: file.total_lines.len(),
            covered_lines: file.covered_lines.len(),
            executable_line_numbers: file.total_lines.into_iter().collect(),
            covered_line_numbers: file.covered_lines.into_iter().collect(),
            total_branches: file.total_branches,
            covered_branches: file.covered_branches,
        })
        .collect::<Vec<_>>();
    CoverageReport {
        total_lines: files.iter().map(|file| file.total_lines).sum(),
        covered_lines: files.iter().map(|file| file.covered_lines).sum(),
        total_branches: files.iter().map(|file| file.total_branches).sum(),
        covered_branches: files.iter().map(|file| file.covered_branches).sum(),
        files,
    }
}

fn relative_path(compilation: &Compilation, path: PathBuf) -> PathBuf {
    path.strip_prefix(&compilation.root)
        .map_or_else(|_| path.clone(), PathBuf::from)
}

fn collect_coverage_candidates(
    compilation: &Compilation,
    statements: &mut BTreeSet<Span>,
    branches: &mut BTreeSet<Span>,
) {
    for class in &compilation.program.classes {
        if class
            .annotations
            .iter()
            .any(|annotation| annotation.kind.is_test())
        {
            continue;
        }
        for member in &class.members {
            match member {
                ClassMember::Constructor(constructor) => {
                    visit_statement(&constructor.body, statements, branches)
                }
                ClassMember::Method(method) => {
                    if let Some(body) = &method.body {
                        visit_statement(body, statements, branches);
                    }
                }
                ClassMember::Property(property) => {
                    for accessor in &property.accessors {
                        if let Some(body) = &accessor.body {
                            visit_statement(body, statements, branches);
                        }
                    }
                }
                ClassMember::Field(_) => {}
            }
        }
    }
    for trigger in &compilation.program.triggers {
        visit_statement(&trigger.body, statements, branches);
    }
}

fn visit_statement(
    statement: &Statement,
    statements: &mut BTreeSet<Span>,
    branches: &mut BTreeSet<Span>,
) {
    if !matches!(statement, Statement::Block { .. }) {
        statements.insert(statement.span());
    }
    match statement {
        Statement::Block {
            statements: body, ..
        } => {
            for statement in body {
                visit_statement(statement, statements, branches);
            }
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            branches.insert(condition.span());
            visit_statement(then_branch, statements, branches);
            if let Some(else_branch) = else_branch {
                visit_statement(else_branch, statements, branches);
            }
        }
        Statement::While {
            condition, body, ..
        }
        | Statement::DoWhile {
            condition, body, ..
        } => {
            branches.insert(condition.span());
            visit_statement(body, statements, branches);
        }
        Statement::For {
            initializer,
            condition,
            update,
            body,
            ..
        } => {
            if let Some(initializer) = initializer {
                visit_statement(initializer, statements, branches);
            }
            if let Some(condition) = condition {
                branches.insert(condition.span());
            }
            if let Some(update) = update {
                visit_statement(update, statements, branches);
            }
            visit_statement(body, statements, branches);
        }
        Statement::ForEach { body, .. } => visit_statement(body, statements, branches),
        Statement::Try {
            try_block,
            catches,
            finally_block,
            ..
        } => {
            visit_statement(try_block, statements, branches);
            for catch in catches {
                visit_statement(&catch.body, statements, branches);
            }
            if let Some(finally_block) = finally_block {
                visit_statement(finally_block, statements, branches);
            }
        }
        Statement::VariableDeclaration { .. }
        | Statement::Expression { .. }
        | Statement::Break { .. }
        | Statement::Continue { .. }
        | Statement::Throw { .. }
        | Statement::Return { .. }
        | Statement::Dml { .. } => {}
    }
}
