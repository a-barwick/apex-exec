//! Semantic editor services shared by the Language Server Protocol adapter.

use crate::{
    ast::{
        AssignmentTarget, ClassMember, Expression, Identifier, NamedType,
        visit::{self, Visitor},
    },
    diagnostic::Diagnostic,
    hir::{CallTarget, ClassMemberId, MemberTarget, ReferenceTarget},
    project::Compilation,
    span::Span,
    test_runner::TestReport,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub location: Location,
    pub new_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorDiagnostic {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageState {
    Covered,
    Uncovered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageOverlay {
    pub path: PathBuf,
    pub lines: Vec<(usize, CoverageState)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Symbol {
    Class(usize),
    Member(ClassMemberId),
    TopLevelMethod(usize),
}

#[derive(Clone, Copy, Debug)]
struct Occurrence {
    span: Span,
    symbol: Symbol,
    definition: bool,
}

/// Immutable semantic index for go-to-definition, references, and rename.
pub struct EditorIndex<'compilation> {
    compilation: &'compilation Compilation,
    occurrences: Vec<Occurrence>,
    definitions: HashMap<Symbol, Span>,
}

impl<'compilation> EditorIndex<'compilation> {
    pub fn new(compilation: &'compilation Compilation) -> Self {
        let mut builder = IndexBuilder::new(&compilation.program);
        builder.build();
        builder.occurrences.sort_by_key(|occurrence| {
            (
                occurrence.span.source_id,
                occurrence.span.start,
                occurrence.span.end,
            )
        });
        builder
            .occurrences
            .dedup_by_key(|occurrence| (occurrence.span, occurrence.symbol, occurrence.definition));
        Self {
            compilation,
            occurrences: builder.occurrences,
            definitions: builder.definitions,
        }
    }

    pub fn definition(
        &self,
        path: impl AsRef<Path>,
        line: usize,
        column: usize,
    ) -> Option<Location> {
        let symbol = self.symbol_at(path.as_ref(), line, column)?;
        self.location(*self.definitions.get(&symbol)?)
    }

    pub fn references(
        &self,
        path: impl AsRef<Path>,
        line: usize,
        column: usize,
        include_declaration: bool,
    ) -> Vec<Location> {
        let Some(symbol) = self.symbol_at(path.as_ref(), line, column) else {
            return Vec::new();
        };
        self.occurrences
            .iter()
            .filter(|occurrence| {
                occurrence.symbol == symbol && (include_declaration || !occurrence.definition)
            })
            .filter_map(|occurrence| self.location(occurrence.span))
            .collect()
    }

    pub fn rename(
        &self,
        path: impl AsRef<Path>,
        line: usize,
        column: usize,
        new_name: &str,
    ) -> Result<Vec<TextEdit>, String> {
        if !valid_identifier(new_name) {
            return Err(format!("`{new_name}` is not a valid Apex identifier"));
        }
        let symbol = self
            .symbol_at(path.as_ref(), line, column)
            .ok_or_else(|| "no renameable symbol at the requested position".to_owned())?;
        Ok(self
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.symbol == symbol)
            .filter_map(|occurrence| {
                self.location(occurrence.span).map(|location| TextEdit {
                    location,
                    new_text: new_name.to_owned(),
                })
            })
            .collect())
    }

    fn symbol_at(&self, path: &Path, line: usize, column: usize) -> Option<Symbol> {
        self.occurrences.iter().find_map(|occurrence| {
            let (source_path, source) = self.compilation.source_text(occurrence.span.source_id)?;
            if !same_path(source_path, path) {
                return None;
            }
            let offset = offset_at(source, line, column)?;
            (occurrence.span.start <= offset && offset < occurrence.span.end)
                .then_some(occurrence.symbol)
        })
    }

    fn location(&self, span: Span) -> Option<Location> {
        let (path, source) = self.compilation.source_text(span.source_id)?;
        Some(location_for_span(path.to_path_buf(), source, span))
    }
}

pub fn diagnostics(source: &str) -> Vec<EditorDiagnostic> {
    let diagnostic = crate::tokenize(source)
        .and_then(|tokens| {
            crate::parser::Parser::new(tokens)
                .expect("the lexer always emits a valid parser token stream")
                .parse_program()
        })
        .and_then(|program| crate::semantic::check(&program))
        .err();
    diagnostic
        .map(|diagnostic| vec![editor_diagnostic(source, &diagnostic)])
        .unwrap_or_default()
}

pub fn coverage_overlays(report: &TestReport) -> Vec<CoverageOverlay> {
    report
        .coverage
        .files
        .iter()
        .map(|file| {
            let covered = file
                .covered_line_numbers
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            CoverageOverlay {
                path: file.path.clone(),
                lines: file
                    .executable_line_numbers
                    .iter()
                    .map(|line| {
                        (
                            *line,
                            if covered.contains(line) {
                                CoverageState::Covered
                            } else {
                                CoverageState::Uncovered
                            },
                        )
                    })
                    .collect(),
            }
        })
        .collect()
}

struct IndexBuilder<'program> {
    program: &'program crate::hir::Program,
    classes: HashMap<String, usize>,
    occurrences: Vec<Occurrence>,
    definitions: HashMap<Symbol, Span>,
}

impl<'program> IndexBuilder<'program> {
    fn new(program: &'program crate::hir::Program) -> Self {
        Self {
            program,
            classes: program
                .classes
                .iter()
                .enumerate()
                .map(|(index, class)| (class.qualified_name.canonical.clone(), index))
                .collect(),
            occurrences: Vec::new(),
            definitions: HashMap::new(),
        }
    }

    fn build(&mut self) {
        for (class_id, class) in self.program.classes.iter().enumerate() {
            self.add(class.name.span, Symbol::Class(class_id), true);
            if let Some(superclass) = &class.superclass {
                self.visit_named_type(superclass);
            }
            for interface in &class.interfaces {
                self.visit_named_type(interface);
            }
            for (member_id, member) in class.members.iter().enumerate() {
                let symbol = Symbol::Member(ClassMemberId {
                    class_id,
                    member_id,
                });
                match member {
                    ClassMember::Field(field) => {
                        self.add(field.name.span, symbol, true);
                        self.visit_type_name(&field.ty);
                        if let Some(initializer) = &field.initializer {
                            self.visit_expression(initializer);
                        }
                    }
                    ClassMember::Property(property) => {
                        self.add(property.name.span, symbol, true);
                        self.visit_type_name(&property.ty);
                        for accessor in &property.accessors {
                            if let Some(body) = &accessor.body {
                                self.visit_statement(body);
                            }
                        }
                    }
                    ClassMember::Constructor(constructor) => {
                        self.add(constructor.name.span, symbol, true);
                        for parameter in &constructor.parameters {
                            self.visit_type_name(&parameter.ty);
                        }
                        self.visit_statement(&constructor.body);
                    }
                    ClassMember::Method(method) => {
                        self.add(method.name.span, symbol, true);
                        self.visit_return_type(&method.return_type);
                        for parameter in &method.parameters {
                            self.visit_type_name(&parameter.ty);
                        }
                        if let Some(body) = &method.body {
                            self.visit_statement(body);
                        }
                    }
                    ClassMember::Initializer(initializer) => {
                        self.visit_statement(&initializer.body);
                    }
                }
            }
        }
        for (method_id, method) in self.program.methods.iter().enumerate() {
            self.add(method.name.span, Symbol::TopLevelMethod(method_id), true);
            visit::walk_method_declaration(self, method);
        }
        for statement in &self.program.statements {
            self.visit_statement(statement);
        }
        for trigger in &self.program.triggers {
            self.visit_named_type(&trigger.object);
            self.visit_statement(&trigger.body);
        }
    }

    fn add(&mut self, span: Span, symbol: Symbol, definition: bool) {
        self.occurrences.push(Occurrence {
            span,
            symbol,
            definition,
        });
        if definition {
            self.definitions.insert(symbol, span);
        }
    }

    fn named_type(&mut self, named_type: &NamedType) {
        if let Some(class_id) = self.classes.get(&named_type.canonical).copied() {
            self.add(named_type.span, Symbol::Class(class_id), false);
        }
    }

    fn call(&mut self, span: Span, identifier: &Identifier) {
        let symbol = match self.program.call_target(span) {
            Some(CallTarget::TopLevelMethod(method_id)) => Some(Symbol::TopLevelMethod(method_id)),
            Some(
                CallTarget::StaticMethod(target)
                | CallTarget::InstanceMethod(target)
                | CallTarget::SuperMethod(target),
            ) => Some(Symbol::Member(target)),
            Some(CallTarget::Constructor {
                class_id,
                member_id: Some(member_id),
            }) => Some(Symbol::Member(ClassMemberId {
                class_id,
                member_id,
            })),
            _ => None,
        };
        if let Some(symbol) = symbol {
            self.add(identifier.span, symbol, false);
        }
    }
}

impl<'ast> Visitor<'ast> for IndexBuilder<'_> {
    fn visit_named_type(&mut self, named_type: &'ast NamedType) {
        self.named_type(named_type);
        visit::walk_named_type(self, named_type);
    }

    fn visit_identifier(&mut self, identifier: &'ast Identifier) {
        let symbol = match self.program.reference_target(identifier.span) {
            Some(
                ReferenceTarget::InstanceMember(target) | ReferenceTarget::StaticMember(target),
            ) => Some(Symbol::Member(target)),
            Some(ReferenceTarget::Super(class_id)) => Some(Symbol::Class(class_id)),
            _ => None,
        };
        if let Some(symbol) = symbol {
            self.add(identifier.span, symbol, false);
        }
    }

    fn visit_expression(&mut self, expression: &'ast Expression) {
        match expression {
            Expression::FunctionCall { name, .. } | Expression::MethodCall { method: name, .. } => {
                self.call(expression.span(), name);
            }
            Expression::MemberAccess { member, .. } => {
                let symbol = match self.program.member_target(expression.span()) {
                    Some(MemberTarget::Instance(target) | MemberTarget::Static(target)) => {
                        Some(Symbol::Member(target))
                    }
                    _ => None,
                };
                if let Some(symbol) = symbol {
                    self.add(member.span, symbol, false);
                }
            }
            _ => {}
        }
        visit::walk_expression(self, expression);
    }

    fn visit_assignment_target(&mut self, target: &'ast AssignmentTarget) {
        if let AssignmentTarget::Member { member, .. } = target {
            let symbol = match self.program.member_target(target.span()) {
                Some(MemberTarget::Instance(target) | MemberTarget::Static(target)) => {
                    Some(Symbol::Member(target))
                }
                _ => None,
            };
            if let Some(symbol) = symbol {
                self.add(member.span, symbol, false);
            }
        }
        visit::walk_assignment_target(self, target);
    }
}

fn editor_diagnostic(source: &str, diagnostic: &Diagnostic) -> EditorDiagnostic {
    let location = location_for_span(PathBuf::new(), source, diagnostic.span);
    EditorDiagnostic {
        message: diagnostic.message.clone(),
        line: location.line,
        column: location.column,
        end_line: location.end_line,
        end_column: location.end_column,
    }
}

fn location_for_span(path: PathBuf, source: &str, span: Span) -> Location {
    let (line, column) = line_column(source, span.start);
    let (end_line, end_column) = line_column(source, span.end);
    Location {
        path,
        line,
        column,
        end_line,
        end_column,
    }
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    (
        source[..offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1,
        source[line_start..offset].chars().count() + 1,
    )
}

fn offset_at(source: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 || column == 0 {
        return None;
    }
    let line_start = if line == 1 {
        0
    } else {
        source
            .match_indices('\n')
            .nth(line - 2)
            .map(|(index, _)| index + 1)?
    };
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |index| line_start + index);
    let relative = source[line_start..line_end]
        .char_indices()
        .nth(column - 1)
        .map_or(line_end - line_start, |(index, _)| index);
    Some(line_start + relative)
}

fn valid_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn same_path(left: &Path, right: &Path) -> bool {
    left == right || left.file_name().is_some() && left.file_name() == right.file_name()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn diagnostics_have_editor_ranges_and_disappear_for_valid_source() {
        let source = "Integer value = missing;";
        let reported = diagnostics(source);
        assert_eq!(reported.len(), 1);
        assert_eq!(reported[0].line, 1);
        assert!(reported[0].message.contains("unknown variable"));
        assert!(diagnostics("Integer value = 1;").is_empty());
    }

    #[test]
    fn definitions_references_and_rename_use_checked_case_insensitive_targets() {
        let root = temporary_project();
        let classes = root.join("force-app/main/default/classes");
        fs::create_dir_all(&classes).unwrap();
        fs::write(
            root.join("sfdx-project.json"),
            r#"{"packageDirectories":[{"path":"force-app","default":true}],"namespace":"","sourceApiVersion":"65.0"}"#,
        )
        .unwrap();
        fs::write(
            classes.join("Service.cls"),
            "public class Service { public static Integer value() { return 42; } }",
        )
        .unwrap();
        let entry_source =
            "public class Entry { public static Integer run() { return service.VALUE(); } }";
        fs::write(classes.join("Entry.cls"), entry_source).unwrap();
        let compilation = crate::project::compile(&root).unwrap();
        let index = EditorIndex::new(&compilation);
        let entry = classes.join("Entry.cls");
        let column = entry_source.find("VALUE").unwrap() + 1;

        let definition = index.definition(&entry, 1, column).unwrap();
        assert_eq!(definition.path.file_name().unwrap(), "Service.cls");
        let references = index.references(&entry, 1, column, true);
        assert_eq!(references.len(), 2);
        let edits = index.rename(&entry, 1, column, "answer").unwrap();
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|edit| edit.new_text == "answer"));
        assert!(index.rename(&entry, 1, column, "not valid").is_err());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generic_hierarchy_arguments_resolve_to_class_definitions() {
        let root = temporary_project();
        let classes = root.join("force-app/main/default/classes");
        fs::create_dir_all(&classes).unwrap();
        fs::write(
            root.join("sfdx-project.json"),
            r#"{"packageDirectories":[{"path":"force-app","default":true}],"namespace":"","sourceApiVersion":"65.0"}"#,
        )
        .unwrap();
        fs::write(classes.join("Scope.cls"), "public class Scope {}").unwrap();
        let batch_source = "\
public class BatchWork implements Database.Batchable<Scope> {
    public List<Scope> start(Database.BatchableContext context) {
        return new List<Scope>();
    }
    public void execute(Database.BatchableContext context, List<Scope> scope) {}
    public void finish(Database.BatchableContext context) {}
}";
        let batch = classes.join("BatchWork.cls");
        fs::write(&batch, batch_source).unwrap();

        let compilation = crate::project::compile(&root).unwrap();
        let index = EditorIndex::new(&compilation);
        let column = batch_source.find("<Scope>").unwrap() + 2;
        let definition = index
            .definition(&batch, 1, column)
            .expect("the Batchable generic argument should resolve");
        assert_eq!(definition.path.file_name().unwrap(), "Scope.cls");

        fs::remove_dir_all(root).unwrap();
    }

    fn temporary_project() -> PathBuf {
        let sequence = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("apex-exec-editor-{sequence}"))
    }
}
