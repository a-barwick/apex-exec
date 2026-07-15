use crate::{
    ast::{
        AssignmentTarget, CatchClause, ClassDeclaration, ClassMember, CollectionInitializer,
        Expression, Identifier, MethodDeclaration, NamedType, Program as AstProgram, ReturnType,
        Statement, TypeName,
    },
    diagnostic::Diagnostic,
    hir,
    span::Span,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fmt, fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub source: String,
}

#[derive(Clone, Debug)]
pub struct DiscoveredProject {
    pub root: PathBuf,
    pub source_roots: Vec<PathBuf>,
    pub files: Vec<SourceFile>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DependencyGraph {
    edges: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
}

impl DependencyGraph {
    pub fn dependencies_of(&self, path: &Path) -> Option<&BTreeSet<PathBuf>> {
        self.edges.get(path)
    }

    pub fn files(&self) -> impl Iterator<Item = &PathBuf> {
        self.edges.keys()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IncrementalReport {
    pub parsed_files: Vec<PathBuf>,
    pub reused_files: Vec<PathBuf>,
    pub invalidated_files: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct Compilation {
    pub root: PathBuf,
    pub program: hir::Program,
    pub dependencies: DependencyGraph,
    pub incremental: IncrementalReport,
    source_map: SourceMap,
}

impl Compilation {
    pub fn invoke(&self, target: &str) -> Result<Vec<String>, ProjectError> {
        let (class, method) = target.split_once('.').ok_or_else(|| {
            ProjectError::message("invocation target must have the form Class.method")
        })?;
        crate::runtime::Interpreter::new()
            .invoke_static(&self.program, class, method)
            .map_err(|diagnostic| self.source_map.project_error(diagnostic))
    }

    pub(crate) fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String {
        self.source_map.render_diagnostic(diagnostic)
    }

    pub(crate) fn source_location(&self, span: Span) -> Option<(PathBuf, usize)> {
        self.source_map.location(span.start)
    }
}

#[derive(Clone, Debug, Default)]
struct SourceMap {
    entries: Vec<SourceEntry>,
}

#[derive(Clone, Debug)]
struct SourceEntry {
    path: PathBuf,
    source: String,
    start: usize,
    end: usize,
}

impl SourceMap {
    fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String {
        let Some(entry) = self.entry_for_offset(diagnostic.span.start) else {
            return diagnostic.to_string();
        };
        let mut local = diagnostic.clone();
        local.span = local_span(local.span, entry.start);
        let frames = std::mem::take(&mut local.stack_trace);
        let mut rendered = local.render(&entry.path.display().to_string(), &entry.source);
        if !frames.is_empty() {
            rendered.push_str("\nApex stack trace:");
            for frame in frames {
                if let Some(frame_entry) = self.entry_for_offset(frame.span.start) {
                    let offset = frame.span.start.saturating_sub(frame_entry.start);
                    let (line, column) = source_line_column(&frame_entry.source, offset);
                    rendered.push_str(&format!(
                        "\n  at {} ({}:{}:{})",
                        frame.method,
                        frame_entry.path.display(),
                        line,
                        column
                    ));
                } else {
                    rendered.push_str(&format!("\n  at {}", frame.method));
                }
            }
        }
        rendered
    }

    fn project_error(&self, mut diagnostic: Diagnostic) -> ProjectError {
        let entry = self.entry_for_offset(diagnostic.span.start);
        let Some(entry) = entry else {
            return ProjectError::diagnostic(None, String::new(), diagnostic);
        };
        diagnostic.span = local_span(diagnostic.span, entry.start);
        for frame in &mut diagnostic.stack_trace {
            if frame.span.start >= entry.start && frame.span.start <= entry.end {
                frame.span = Span::new(
                    frame.span.start - entry.start,
                    frame.span.end.saturating_sub(entry.start),
                );
            }
        }
        ProjectError::diagnostic(Some(entry.path.clone()), entry.source.clone(), diagnostic)
    }

    fn location(&self, offset: usize) -> Option<(PathBuf, usize)> {
        let entry = self.entry_for_offset(offset)?;
        let local = offset.saturating_sub(entry.start).min(entry.source.len());
        let line = entry.source[..local]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        Some((entry.path.clone(), line))
    }

    fn entry_for_offset(&self, offset: usize) -> Option<&SourceEntry> {
        self.entries
            .iter()
            .find(|entry| offset >= entry.start && offset <= entry.end)
    }
}

fn local_span(span: Span, entry_start: usize) -> Span {
    Span::new(
        span.start.saturating_sub(entry_start),
        span.end.saturating_sub(entry_start),
    )
}

fn source_line_column(source: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line = source[..offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let column = source[line_start..offset].chars().count() + 1;
    (line, column)
}

#[derive(Clone, Debug)]
pub struct ProjectError {
    message: String,
    path: Option<PathBuf>,
    source: String,
    diagnostic: Option<Box<Diagnostic>>,
}

impl ProjectError {
    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            path: None,
            source: String::new(),
            diagnostic: None,
        }
    }

    fn io(path: &Path, action: &str, error: std::io::Error) -> Self {
        Self::message(format!("failed to {action} `{}`: {error}", path.display()))
    }

    fn diagnostic(path: Option<PathBuf>, source: String, diagnostic: Diagnostic) -> Self {
        Self {
            message: diagnostic.message.clone(),
            path,
            source,
            diagnostic: Some(Box::new(diagnostic)),
        }
    }

    pub fn render(&self) -> String {
        match (&self.diagnostic, &self.path) {
            (Some(diagnostic), Some(path)) => {
                diagnostic.render(&path.display().to_string(), &self.source)
            }
            (Some(diagnostic), None) => diagnostic.to_string(),
            (None, _) => self.message.clone(),
        }
    }
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ProjectError {}

#[derive(Clone, Debug)]
struct CachedUnit {
    hash: u64,
    source: String,
    ast: AstProgram,
}

#[derive(Default)]
pub struct ProjectCompiler {
    units: HashMap<PathBuf, CachedUnit>,
    last_fingerprints: BTreeMap<PathBuf, u64>,
    last_dependencies: DependencyGraph,
    last_compilation: Option<Compilation>,
}

impl ProjectCompiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn compile(&mut self, path: impl AsRef<Path>) -> Result<Compilation, ProjectError> {
        let project = discover(path)?;
        let fingerprints = project
            .files
            .iter()
            .map(|file| (file.path.clone(), source_hash(&file.source)))
            .collect::<BTreeMap<_, _>>();

        if fingerprints == self.last_fingerprints
            && let Some(previous) = &self.last_compilation
        {
            let mut cached = previous.clone();
            cached.incremental = IncrementalReport {
                parsed_files: Vec::new(),
                reused_files: fingerprints.keys().cloned().collect(),
                invalidated_files: Vec::new(),
            };
            return Ok(cached);
        }

        let removed = self
            .last_fingerprints
            .keys()
            .filter(|path| !fingerprints.contains_key(*path))
            .cloned()
            .collect::<Vec<_>>();
        let mut parsed_files = Vec::new();
        let mut reused_files = Vec::new();
        for file in &project.files {
            let hash = fingerprints[&file.path];
            if self
                .units
                .get(&file.path)
                .is_some_and(|cached| cached.hash == hash)
            {
                reused_files.push(file.path.clone());
                continue;
            }
            let ast = crate::parse(&file.source).map_err(|diagnostic| {
                ProjectError::diagnostic(Some(file.path.clone()), file.source.clone(), diagnostic)
            })?;
            if !ast.methods.is_empty() || !ast.statements.is_empty() || ast.classes.len() != 1 {
                return Err(ProjectError::message(format!(
                    "`{}` must contain exactly one top-level Apex class or interface",
                    file.path.display()
                )));
            }
            let expected_name = file
                .path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if !ast.classes[0]
                .name
                .spelling
                .eq_ignore_ascii_case(expected_name)
            {
                return Err(ProjectError::message(format!(
                    "type `{}` must be declared in `{expected_name}.cls`",
                    ast.classes[0].name.spelling
                )));
            }
            self.units.insert(
                file.path.clone(),
                CachedUnit {
                    hash,
                    source: file.source.clone(),
                    ast,
                },
            );
            parsed_files.push(file.path.clone());
        }
        self.units.retain(|path, _| fingerprints.contains_key(path));

        let (merged, source_map) = merge_units(&project.files, &self.units);
        let dependencies = build_dependency_graph(&project.files, &self.units);
        let changed = parsed_files
            .iter()
            .chain(&removed)
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut invalidated = dependent_closure(&changed, &self.last_dependencies);
        invalidated.extend(dependent_closure(&changed, &dependencies));

        let program = crate::semantic::check(&merged)
            .map_err(|diagnostic| source_map.project_error(diagnostic))?;
        let compilation = Compilation {
            root: project.root,
            program,
            dependencies: dependencies.clone(),
            incremental: IncrementalReport {
                parsed_files,
                reused_files,
                invalidated_files: invalidated.into_iter().collect(),
            },
            source_map,
        };
        self.last_fingerprints = fingerprints;
        self.last_dependencies = dependencies;
        self.last_compilation = Some(compilation.clone());
        Ok(compilation)
    }
}

pub fn compile(path: impl AsRef<Path>) -> Result<Compilation, ProjectError> {
    ProjectCompiler::new().compile(path)
}

pub fn discover(path: impl AsRef<Path>) -> Result<DiscoveredProject, ProjectError> {
    let requested = path.as_ref();
    let root = find_project_root(requested)?;
    let config_path = root.join("sfdx-project.json");
    let config = fs::read_to_string(&config_path)
        .map_err(|error| ProjectError::io(&config_path, "read", error))?;
    let package_paths = extract_package_paths(&config)?;
    let mut source_roots = package_paths
        .into_iter()
        .map(|path| root.join(path))
        .collect::<Vec<_>>();
    source_roots.sort();
    source_roots.dedup();
    let mut paths = Vec::new();
    for source_root in &source_roots {
        collect_class_files(source_root, &mut paths)?;
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Err(ProjectError::message(format!(
            "no `.cls` files found in SFDX project `{}`",
            root.display()
        )));
    }
    let files = paths
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path)
                .map_err(|error| ProjectError::io(&path, "read", error))?;
            Ok(SourceFile { path, source })
        })
        .collect::<Result<Vec<_>, ProjectError>>()?;
    Ok(DiscoveredProject {
        root,
        source_roots,
        files,
    })
}

fn find_project_root(requested: &Path) -> Result<PathBuf, ProjectError> {
    let mut cursor = if requested.is_file() {
        requested.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        requested.to_path_buf()
    };
    loop {
        if cursor.join("sfdx-project.json").is_file() {
            return Ok(cursor);
        }
        if !cursor.pop() {
            return Err(ProjectError::message(format!(
                "could not find `sfdx-project.json` from `{}`",
                requested.display()
            )));
        }
    }
}

fn collect_class_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), ProjectError> {
    let entries =
        fs::read_dir(directory).map_err(|error| ProjectError::io(directory, "scan", error))?;
    for entry in entries {
        let entry = entry.map_err(|error| ProjectError::io(directory, "scan", error))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| ProjectError::io(&path, "inspect", error))?;
        if file_type.is_dir() {
            collect_class_files(&path, files)?;
        } else if file_type.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("cls"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn extract_package_paths(config: &str) -> Result<Vec<String>, ProjectError> {
    let marker = "\"packageDirectories\"";
    let marker_start = config.find(marker).ok_or_else(|| {
        ProjectError::message("sfdx-project.json is missing `packageDirectories`")
    })?;
    let array_start = config[marker_start + marker.len()..]
        .find('[')
        .map(|offset| marker_start + marker.len() + offset)
        .ok_or_else(|| ProjectError::message("`packageDirectories` must be an array"))?;
    let array_end = matching_json_delimiter(config, array_start, '[', ']')
        .ok_or_else(|| ProjectError::message("unterminated `packageDirectories` array"))?;
    let array = &config[array_start + 1..array_end];
    let mut paths = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = array[cursor..].find("\"path\"") {
        cursor += relative + "\"path\"".len();
        let colon = array[cursor..]
            .find(':')
            .map(|offset| cursor + offset + 1)
            .ok_or_else(|| ProjectError::message("invalid package directory `path`"))?;
        let quote = array[colon..]
            .find('"')
            .map(|offset| colon + offset)
            .ok_or_else(|| ProjectError::message("package directory `path` must be a string"))?;
        let (path, end) = parse_json_string(array, quote)?;
        paths.push(path);
        cursor = end;
    }
    if paths.is_empty() {
        return Err(ProjectError::message(
            "`packageDirectories` must contain at least one `path`",
        ));
    }
    Ok(paths)
}

fn matching_json_delimiter(text: &str, start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut string = false;
    let mut escaped = false;
    for (offset, ch) in text[start..].char_indices() {
        if string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                string = false;
            }
            continue;
        }
        if ch == '"' {
            string = true;
        } else if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(start + offset);
            }
        }
    }
    None
}

fn parse_json_string(text: &str, quote: usize) -> Result<(String, usize), ProjectError> {
    let mut result = String::new();
    let mut escaped = false;
    for (offset, ch) in text[quote + 1..].char_indices() {
        if escaped {
            match ch {
                '"' | '\\' | '/' => result.push(ch),
                'n' => result.push('\n'),
                'r' => result.push('\r'),
                't' => result.push('\t'),
                _ => {
                    return Err(ProjectError::message(
                        "unsupported JSON escape in package path",
                    ));
                }
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Ok((result, quote + 1 + offset + 1));
        } else {
            result.push(ch);
        }
    }
    Err(ProjectError::message("unterminated package directory path"))
}

fn source_hash(source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn merge_units(
    files: &[SourceFile],
    units: &HashMap<PathBuf, CachedUnit>,
) -> (AstProgram, SourceMap) {
    let mut merged = AstProgram {
        classes: Vec::new(),
        methods: Vec::new(),
        statements: Vec::new(),
    };
    let mut source_map = SourceMap::default();
    let mut offset = 0usize;
    for file in files {
        let unit = &units[&file.path];
        let mut ast = unit.ast.clone();
        shift_program(&mut ast, offset);
        merged.classes.extend(ast.classes);
        merged.methods.extend(ast.methods);
        merged.statements.extend(ast.statements);
        source_map.entries.push(SourceEntry {
            path: file.path.clone(),
            source: unit.source.clone(),
            start: offset,
            end: offset + unit.source.len(),
        });
        offset += unit.source.len() + 1;
    }
    (merged, source_map)
}

fn build_dependency_graph(
    files: &[SourceFile],
    units: &HashMap<PathBuf, CachedUnit>,
) -> DependencyGraph {
    let owners = files
        .iter()
        .flat_map(|file| {
            units[&file.path]
                .ast
                .classes
                .iter()
                .map(|class| (class.name.canonical.clone(), file.path.clone()))
                .collect::<Vec<_>>()
        })
        .collect::<HashMap<_, _>>();
    let mut edges = BTreeMap::new();
    for file in files {
        let mut names = BTreeSet::new();
        for class in &units[&file.path].ast.classes {
            collect_class_dependencies(class, &owners, &mut names);
        }
        let dependencies = names
            .into_iter()
            .filter_map(|name| owners.get(&name).cloned())
            .filter(|dependency| dependency != &file.path)
            .collect();
        edges.insert(file.path.clone(), dependencies);
    }
    DependencyGraph { edges }
}

fn dependent_closure(changed: &BTreeSet<PathBuf>, graph: &DependencyGraph) -> BTreeSet<PathBuf> {
    let mut result = changed.clone();
    let mut queue = changed.iter().cloned().collect::<VecDeque<_>>();
    while let Some(dependency) = queue.pop_front() {
        for (path, dependencies) in &graph.edges {
            if dependencies.contains(&dependency) && result.insert(path.clone()) {
                queue.push_back(path.clone());
            }
        }
    }
    result
}

fn collect_class_dependencies(
    class: &ClassDeclaration,
    owners: &HashMap<String, PathBuf>,
    dependencies: &mut BTreeSet<String>,
) {
    if let Some(parent) = &class.superclass {
        dependencies.insert(parent.canonical.clone());
    }
    dependencies.extend(class.interfaces.iter().map(|name| name.canonical.clone()));
    for member in &class.members {
        match member {
            ClassMember::Field(field) => {
                collect_type_dependency(&field.ty, dependencies);
                if let Some(initializer) = &field.initializer {
                    collect_expression_dependencies(initializer, owners, dependencies);
                }
            }
            ClassMember::Property(property) => {
                collect_type_dependency(&property.ty, dependencies);
                for accessor in &property.accessors {
                    if let Some(body) = &accessor.body {
                        collect_statement_dependencies(body, owners, dependencies);
                    }
                }
            }
            ClassMember::Constructor(constructor) => {
                for parameter in &constructor.parameters {
                    collect_type_dependency(&parameter.ty, dependencies);
                }
                collect_statement_dependencies(&constructor.body, owners, dependencies);
            }
            ClassMember::Method(method) => {
                if let ReturnType::Value(ty) = &method.return_type {
                    collect_type_dependency(ty, dependencies);
                }
                for parameter in &method.parameters {
                    collect_type_dependency(&parameter.ty, dependencies);
                }
                if let Some(body) = &method.body {
                    collect_statement_dependencies(body, owners, dependencies);
                }
            }
        }
    }
}

fn collect_type_dependency(ty: &TypeName, dependencies: &mut BTreeSet<String>) {
    match ty {
        TypeName::Custom(name) => {
            dependencies.insert(name.canonical.clone());
        }
        TypeName::List(element) | TypeName::Set(element) => {
            collect_type_dependency(element, dependencies)
        }
        TypeName::Map(key, value) => {
            collect_type_dependency(key, dependencies);
            collect_type_dependency(value, dependencies);
        }
        _ => {}
    }
}

fn collect_statement_dependencies(
    statement: &Statement,
    owners: &HashMap<String, PathBuf>,
    dependencies: &mut BTreeSet<String>,
) {
    match statement {
        Statement::VariableDeclaration {
            ty, initializer, ..
        } => {
            collect_type_dependency(ty, dependencies);
            collect_expression_dependencies(initializer, owners, dependencies);
        }
        Statement::Expression { expression, .. } => {
            collect_expression_dependencies(expression, owners, dependencies)
        }
        Statement::Block { statements, .. } => {
            for statement in statements {
                collect_statement_dependencies(statement, owners, dependencies);
            }
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_expression_dependencies(condition, owners, dependencies);
            collect_statement_dependencies(then_branch, owners, dependencies);
            if let Some(else_branch) = else_branch {
                collect_statement_dependencies(else_branch, owners, dependencies);
            }
        }
        Statement::While {
            condition, body, ..
        }
        | Statement::DoWhile {
            condition, body, ..
        } => {
            collect_expression_dependencies(condition, owners, dependencies);
            collect_statement_dependencies(body, owners, dependencies);
        }
        Statement::For {
            initializer,
            condition,
            update,
            body,
            ..
        } => {
            if let Some(initializer) = initializer {
                collect_statement_dependencies(initializer, owners, dependencies);
            }
            if let Some(condition) = condition {
                collect_expression_dependencies(condition, owners, dependencies);
            }
            if let Some(update) = update {
                collect_statement_dependencies(update, owners, dependencies);
            }
            collect_statement_dependencies(body, owners, dependencies);
        }
        Statement::ForEach {
            element_type,
            iterable,
            body,
            ..
        } => {
            collect_type_dependency(element_type, dependencies);
            collect_expression_dependencies(iterable, owners, dependencies);
            collect_statement_dependencies(body, owners, dependencies);
        }
        Statement::Try {
            try_block,
            catches,
            finally_block,
            ..
        } => {
            collect_statement_dependencies(try_block, owners, dependencies);
            for catch in catches {
                collect_type_dependency(&catch.exception_type, dependencies);
                collect_statement_dependencies(&catch.body, owners, dependencies);
            }
            if let Some(finally_block) = finally_block {
                collect_statement_dependencies(finally_block, owners, dependencies);
            }
        }
        Statement::Throw { value, .. } => {
            collect_expression_dependencies(value, owners, dependencies)
        }
        Statement::Return { value, .. } => {
            if let Some(value) = value {
                collect_expression_dependencies(value, owners, dependencies);
            }
        }
        Statement::Break { .. } | Statement::Continue { .. } => {}
    }
}

fn collect_expression_dependencies(
    expression: &Expression,
    owners: &HashMap<String, PathBuf>,
    dependencies: &mut BTreeSet<String>,
) {
    match expression {
        Expression::Variable(identifier) => {
            if owners.contains_key(&identifier.canonical) {
                dependencies.insert(identifier.canonical.clone());
            }
        }
        Expression::Assignment { target, value, .. } => {
            match target {
                AssignmentTarget::Variable(identifier) => {
                    if owners.contains_key(&identifier.canonical) {
                        dependencies.insert(identifier.canonical.clone());
                    }
                }
                AssignmentTarget::Index {
                    collection, index, ..
                } => {
                    collect_expression_dependencies(collection, owners, dependencies);
                    collect_expression_dependencies(index, owners, dependencies);
                }
                AssignmentTarget::Member { receiver, .. } => {
                    collect_expression_dependencies(receiver, owners, dependencies)
                }
            }
            collect_expression_dependencies(value, owners, dependencies);
        }
        Expression::NewCollection {
            ty, initializer, ..
        } => {
            collect_type_dependency(ty, dependencies);
            match initializer {
                CollectionInitializer::Arguments(values)
                | CollectionInitializer::Elements(values) => {
                    for value in values {
                        collect_expression_dependencies(value, owners, dependencies);
                    }
                }
                CollectionInitializer::MapEntries(entries) => {
                    for entry in entries {
                        collect_expression_dependencies(&entry.key, owners, dependencies);
                        collect_expression_dependencies(&entry.value, owners, dependencies);
                    }
                }
                CollectionInitializer::SizedArray(size) => {
                    collect_expression_dependencies(size, owners, dependencies)
                }
            }
        }
        Expression::NewException {
            exception_type,
            arguments,
            ..
        }
        | Expression::NewObject {
            ty: exception_type,
            arguments,
            ..
        } => {
            collect_type_dependency(exception_type, dependencies);
            for argument in arguments {
                collect_expression_dependencies(argument, owners, dependencies);
            }
        }
        Expression::Index {
            collection, index, ..
        } => {
            collect_expression_dependencies(collection, owners, dependencies);
            collect_expression_dependencies(index, owners, dependencies);
        }
        Expression::FunctionCall { arguments, .. } => {
            for argument in arguments {
                collect_expression_dependencies(argument, owners, dependencies);
            }
        }
        Expression::MethodCall {
            receiver,
            arguments,
            ..
        } => {
            collect_expression_dependencies(receiver, owners, dependencies);
            for argument in arguments {
                collect_expression_dependencies(argument, owners, dependencies);
            }
        }
        Expression::MemberAccess { receiver, .. }
        | Expression::Cast {
            expression: receiver,
            ..
        }
        | Expression::Unary {
            operand: receiver, ..
        }
        | Expression::Postfix {
            operand: receiver, ..
        } => collect_expression_dependencies(receiver, owners, dependencies),
        Expression::Binary { left, right, .. } => {
            collect_expression_dependencies(left, owners, dependencies);
            collect_expression_dependencies(right, owners, dependencies);
        }
        Expression::StringLiteral(..)
        | Expression::BooleanLiteral(..)
        | Expression::IntegerLiteral(..)
        | Expression::NullLiteral(..) => {}
    }
    if let Expression::Cast { ty, .. } = expression {
        collect_type_dependency(ty, dependencies);
    }
}

fn shift_program(program: &mut AstProgram, offset: usize) {
    for class in &mut program.classes {
        shift_class(class, offset);
    }
    for method in &mut program.methods {
        shift_method(method, offset);
    }
    for statement in &mut program.statements {
        shift_statement(statement, offset);
    }
}

fn shift_class(class: &mut ClassDeclaration, offset: usize) {
    for annotation in &mut class.annotations {
        shift_span(&mut annotation.span, offset);
    }
    shift_identifier(&mut class.name, offset);
    if let Some(parent) = &mut class.superclass {
        shift_named_type(parent, offset);
    }
    for interface in &mut class.interfaces {
        shift_named_type(interface, offset);
    }
    for member in &mut class.members {
        match member {
            ClassMember::Field(field) => {
                shift_type(&mut field.ty, offset);
                shift_identifier(&mut field.name, offset);
                if let Some(initializer) = &mut field.initializer {
                    shift_expression(initializer, offset);
                }
                shift_span(&mut field.span, offset);
            }
            ClassMember::Property(property) => {
                shift_type(&mut property.ty, offset);
                shift_identifier(&mut property.name, offset);
                for accessor in &mut property.accessors {
                    if let Some(body) = &mut accessor.body {
                        shift_statement(body, offset);
                    }
                    shift_span(&mut accessor.span, offset);
                }
                shift_span(&mut property.span, offset);
            }
            ClassMember::Constructor(constructor) => {
                shift_identifier(&mut constructor.name, offset);
                for parameter in &mut constructor.parameters {
                    shift_type(&mut parameter.ty, offset);
                    shift_identifier(&mut parameter.name, offset);
                    shift_span(&mut parameter.span, offset);
                }
                shift_statement(&mut constructor.body, offset);
                shift_span(&mut constructor.span, offset);
            }
            ClassMember::Method(method) => shift_method(method, offset),
        }
    }
    shift_span(&mut class.span, offset);
}

fn shift_method(method: &mut MethodDeclaration, offset: usize) {
    for annotation in &mut method.annotations {
        shift_span(&mut annotation.span, offset);
    }
    if let ReturnType::Value(ty) = &mut method.return_type {
        shift_type(ty, offset);
    }
    shift_identifier(&mut method.name, offset);
    for parameter in &mut method.parameters {
        shift_type(&mut parameter.ty, offset);
        shift_identifier(&mut parameter.name, offset);
        shift_span(&mut parameter.span, offset);
    }
    if let Some(body) = &mut method.body {
        shift_statement(body, offset);
    }
    shift_span(&mut method.span, offset);
}

fn shift_statement(statement: &mut Statement, offset: usize) {
    match statement {
        Statement::VariableDeclaration {
            ty,
            name,
            initializer,
            span,
        } => {
            shift_type(ty, offset);
            shift_identifier(name, offset);
            shift_expression(initializer, offset);
            shift_span(span, offset);
        }
        Statement::Expression { expression, span } => {
            shift_expression(expression, offset);
            shift_span(span, offset);
        }
        Statement::Block { statements, span } => {
            for statement in statements {
                shift_statement(statement, offset);
            }
            shift_span(span, offset);
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
            span,
        } => {
            shift_expression(condition, offset);
            shift_statement(then_branch, offset);
            if let Some(else_branch) = else_branch {
                shift_statement(else_branch, offset);
            }
            shift_span(span, offset);
        }
        Statement::While {
            condition,
            body,
            span,
        }
        | Statement::DoWhile {
            condition,
            body,
            span,
        } => {
            shift_expression(condition, offset);
            shift_statement(body, offset);
            shift_span(span, offset);
        }
        Statement::For {
            initializer,
            condition,
            update,
            body,
            span,
        } => {
            if let Some(initializer) = initializer {
                shift_statement(initializer, offset);
            }
            if let Some(condition) = condition {
                shift_expression(condition, offset);
            }
            if let Some(update) = update {
                shift_statement(update, offset);
            }
            shift_statement(body, offset);
            shift_span(span, offset);
        }
        Statement::ForEach {
            element_type,
            name,
            iterable,
            body,
            span,
        } => {
            shift_type(element_type, offset);
            shift_identifier(name, offset);
            shift_expression(iterable, offset);
            shift_statement(body, offset);
            shift_span(span, offset);
        }
        Statement::Try {
            try_block,
            catches,
            finally_block,
            span,
        } => {
            shift_statement(try_block, offset);
            for catch in catches {
                shift_catch(catch, offset);
            }
            if let Some(finally_block) = finally_block {
                shift_statement(finally_block, offset);
            }
            shift_span(span, offset);
        }
        Statement::Throw { value, span } => {
            shift_expression(value, offset);
            shift_span(span, offset);
        }
        Statement::Return { value, span } => {
            if let Some(value) = value {
                shift_expression(value, offset);
            }
            shift_span(span, offset);
        }
        Statement::Break { span } | Statement::Continue { span } => shift_span(span, offset),
    }
}

fn shift_catch(catch: &mut CatchClause, offset: usize) {
    shift_type(&mut catch.exception_type, offset);
    shift_identifier(&mut catch.name, offset);
    shift_statement(&mut catch.body, offset);
    shift_span(&mut catch.span, offset);
}

fn shift_expression(expression: &mut Expression, offset: usize) {
    match expression {
        Expression::StringLiteral(_, span)
        | Expression::BooleanLiteral(_, span)
        | Expression::IntegerLiteral(_, span)
        | Expression::NullLiteral(span) => shift_span(span, offset),
        Expression::Variable(identifier) => shift_identifier(identifier, offset),
        Expression::Assignment {
            target,
            value,
            span,
        } => {
            shift_assignment_target(target, offset);
            shift_expression(value, offset);
            shift_span(span, offset);
        }
        Expression::NewCollection {
            ty,
            initializer,
            span,
        } => {
            shift_type(ty, offset);
            match initializer {
                CollectionInitializer::Arguments(values)
                | CollectionInitializer::Elements(values) => {
                    for value in values {
                        shift_expression(value, offset);
                    }
                }
                CollectionInitializer::MapEntries(entries) => {
                    for entry in entries {
                        shift_expression(&mut entry.key, offset);
                        shift_expression(&mut entry.value, offset);
                        shift_span(&mut entry.span, offset);
                    }
                }
                CollectionInitializer::SizedArray(size) => shift_expression(size, offset),
            }
            shift_span(span, offset);
        }
        Expression::NewException {
            exception_type,
            arguments,
            span,
        }
        | Expression::NewObject {
            ty: exception_type,
            arguments,
            span,
        } => {
            shift_type(exception_type, offset);
            for argument in arguments {
                shift_expression(argument, offset);
            }
            shift_span(span, offset);
        }
        Expression::Index {
            collection,
            index,
            span,
        } => {
            shift_expression(collection, offset);
            shift_expression(index, offset);
            shift_span(span, offset);
        }
        Expression::FunctionCall {
            name,
            arguments,
            span,
        } => {
            shift_identifier(name, offset);
            for argument in arguments {
                shift_expression(argument, offset);
            }
            shift_span(span, offset);
        }
        Expression::MethodCall {
            receiver,
            method,
            arguments,
            span,
        } => {
            shift_expression(receiver, offset);
            shift_identifier(method, offset);
            for argument in arguments {
                shift_expression(argument, offset);
            }
            shift_span(span, offset);
        }
        Expression::MemberAccess {
            receiver,
            member,
            span,
        } => {
            shift_expression(receiver, offset);
            shift_identifier(member, offset);
            shift_span(span, offset);
        }
        Expression::Cast {
            ty,
            expression,
            span,
        } => {
            shift_type(ty, offset);
            shift_expression(expression, offset);
            shift_span(span, offset);
        }
        Expression::Unary {
            operand,
            operator_span,
            span,
            ..
        }
        | Expression::Postfix {
            operand,
            operator_span,
            span,
            ..
        } => {
            shift_expression(operand, offset);
            shift_span(operator_span, offset);
            shift_span(span, offset);
        }
        Expression::Binary {
            left,
            right,
            operator_span,
            span,
            ..
        } => {
            shift_expression(left, offset);
            shift_expression(right, offset);
            shift_span(operator_span, offset);
            shift_span(span, offset);
        }
    }
}

fn shift_assignment_target(target: &mut AssignmentTarget, offset: usize) {
    match target {
        AssignmentTarget::Variable(identifier) => shift_identifier(identifier, offset),
        AssignmentTarget::Index {
            collection,
            index,
            span,
        } => {
            shift_expression(collection, offset);
            shift_expression(index, offset);
            shift_span(span, offset);
        }
        AssignmentTarget::Member {
            receiver,
            member,
            span,
        } => {
            shift_expression(receiver, offset);
            shift_identifier(member, offset);
            shift_span(span, offset);
        }
    }
}

fn shift_type(ty: &mut TypeName, offset: usize) {
    match ty {
        TypeName::Custom(name) => shift_named_type(name, offset),
        TypeName::List(element) | TypeName::Set(element) => shift_type(element, offset),
        TypeName::Map(key, value) => {
            shift_type(key, offset);
            shift_type(value, offset);
        }
        _ => {}
    }
}

fn shift_identifier(identifier: &mut Identifier, offset: usize) {
    shift_span(&mut identifier.span, offset);
}

fn shift_named_type(name: &mut NamedType, offset: usize) {
    shift_span(&mut name.span, offset);
}

fn shift_span(span: &mut Span, offset: usize) {
    span.start += offset;
    span.end += offset;
}
