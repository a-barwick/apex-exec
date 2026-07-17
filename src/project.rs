use crate::{
    ast::{
        AssignmentTarget, ClassDeclaration, ClassMember, CollectionInitializer, Expression,
        Program as AstProgram, ReturnType, Statement, TypeName,
    },
    diagnostic::Diagnostic,
    hir,
    span::{SourceId, Span},
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
        self.source_map.location(span)
    }
}

#[derive(Clone, Debug, Default)]
struct SourceMap {
    entries: Vec<SourceEntry>,
}

#[derive(Clone, Debug)]
struct SourceEntry {
    source_id: SourceId,
    path: PathBuf,
    source: String,
}

impl SourceMap {
    fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String {
        let Some(entry) = self.entry_for_source(diagnostic.span.source_id) else {
            return diagnostic.to_string();
        };
        let mut local = diagnostic.clone();
        let frames = std::mem::take(&mut local.stack_trace);
        let mut rendered = local.render(&entry.path.display().to_string(), &entry.source);
        if !frames.is_empty() {
            rendered.push_str("\nApex stack trace:");
            for frame in frames {
                if let Some(frame_entry) = self.entry_for_source(frame.span.source_id) {
                    let (line, column) = source_line_column(&frame_entry.source, frame.span.start);
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

    fn project_error(&self, diagnostic: Diagnostic) -> ProjectError {
        ProjectError::project_diagnostic(self.clone(), diagnostic)
    }

    fn location(&self, span: Span) -> Option<(PathBuf, usize)> {
        let entry = self.entry_for_source(span.source_id)?;
        let local = span.start.min(entry.source.len());
        let line = entry.source[..local]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        Some((entry.path.clone(), line))
    }

    fn entry_for_source(&self, source_id: SourceId) -> Option<&SourceEntry> {
        self.entries
            .iter()
            .find(|entry| entry.source_id == source_id)
    }
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
    source_map: Option<SourceMap>,
    diagnostic: Option<Box<Diagnostic>>,
}

impl ProjectError {
    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            path: None,
            source: String::new(),
            source_map: None,
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
            source_map: None,
            diagnostic: Some(Box::new(diagnostic)),
        }
    }

    fn project_diagnostic(source_map: SourceMap, diagnostic: Diagnostic) -> Self {
        Self {
            message: diagnostic.message.clone(),
            path: None,
            source: String::new(),
            source_map: Some(source_map),
            diagnostic: Some(Box::new(diagnostic)),
        }
    }

    pub fn render(&self) -> String {
        if let (Some(diagnostic), Some(source_map)) = (&self.diagnostic, &self.source_map) {
            return source_map.render_diagnostic(diagnostic);
        }
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
    source_id: SourceId,
    source: String,
    ast: AstProgram,
}

pub struct ProjectCompiler {
    units: HashMap<PathBuf, CachedUnit>,
    next_source_id: usize,
    last_fingerprints: BTreeMap<PathBuf, u64>,
    last_dependencies: DependencyGraph,
    last_compilation: Option<Compilation>,
}

impl Default for ProjectCompiler {
    fn default() -> Self {
        Self {
            units: HashMap::new(),
            next_source_id: 1,
            last_fingerprints: BTreeMap::new(),
            last_dependencies: DependencyGraph::default(),
            last_compilation: None,
        }
    }
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
            let source_id = if let Some(cached) = self.units.get(&file.path) {
                cached.source_id
            } else {
                let source_id = SourceId::new(self.next_source_id);
                self.next_source_id = self
                    .next_source_id
                    .checked_add(1)
                    .expect("project source identity space exhausted");
                source_id
            };
            let ast = crate::parse_with_source(&file.source, source_id).map_err(|diagnostic| {
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
                    source_id,
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
    for file in files {
        let unit = &units[&file.path];
        let ast = unit.ast.clone();
        merged.classes.extend(ast.classes);
        merged.methods.extend(ast.methods);
        merged.statements.extend(ast.statements);
        source_map.entries.push(SourceEntry {
            source_id: unit.source_id,
            path: file.path.clone(),
            source: unit.source.clone(),
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_units_keep_local_offsets_distinct_by_source_identity() {
        let able_source =
            "public class Able { public static Integer value() { return 1; } }".to_owned();
        let beta_source =
            "public class Beta { public static Integer value() { return 2; } }".to_owned();
        let able_path = PathBuf::from("Able.cls");
        let beta_path = PathBuf::from("Beta.cls");
        let able_id = SourceId::new(1);
        let beta_id = SourceId::new(2);
        let files = vec![
            SourceFile {
                path: able_path.clone(),
                source: able_source.clone(),
            },
            SourceFile {
                path: beta_path.clone(),
                source: beta_source.clone(),
            },
        ];
        let units = HashMap::from([
            (
                able_path.clone(),
                CachedUnit {
                    hash: source_hash(&able_source),
                    source_id: able_id,
                    source: able_source.clone(),
                    ast: crate::parse_with_source(&able_source, able_id).unwrap(),
                },
            ),
            (
                beta_path.clone(),
                CachedUnit {
                    hash: source_hash(&beta_source),
                    source_id: beta_id,
                    source: beta_source.clone(),
                    ast: crate::parse_with_source(&beta_source, beta_id).unwrap(),
                },
            ),
        ]);

        let (merged, source_map) = merge_units(&files, &units);
        assert_eq!(merged.classes[0].span.start, merged.classes[1].span.start);
        assert_eq!(merged.classes[0].span.end, merged.classes[1].span.end);
        assert_eq!(merged.classes[0].span.source_id, able_id);
        assert_eq!(merged.classes[1].span.source_id, beta_id);
        assert_eq!(
            source_map.location(merged.classes[0].span),
            Some((able_path, 1))
        );
        assert_eq!(
            source_map.location(merged.classes[1].span),
            Some((beta_path, 1))
        );

        let checked = crate::semantic::check(&merged).unwrap();
        for class in &checked.classes {
            let ClassMember::Method(method) = &class.members[0] else {
                panic!("expected method");
            };
            let Statement::Block { statements, .. } = method.body.as_ref().unwrap() else {
                panic!("expected method body");
            };
            let Statement::Return {
                value: Some(value), ..
            } = &statements[0]
            else {
                panic!("expected return");
            };
            assert!(checked.expression_type(value.span()).is_some());
        }
    }

    #[test]
    fn project_compiler_preserves_source_ids_when_reparsing_a_file() {
        let root = temporary_project("source-identity");
        let classes = root.join("force-app/main/default/classes");
        fs::create_dir_all(&classes).unwrap();
        fs::write(
            root.join("sfdx-project.json"),
            r#"{"packageDirectories":[{"path":"force-app","default":true}]}"#,
        )
        .unwrap();
        let able_path = classes.join("Able.cls");
        let beta_path = classes.join("Beta.cls");
        fs::write(&able_path, "public class Able {}").unwrap();
        fs::write(&beta_path, "public class Beta {}").unwrap();

        let mut compiler = ProjectCompiler::new();
        let first = compiler.compile(&root).unwrap();
        let first_ids = first
            .program
            .classes
            .iter()
            .map(|class| (class.name.canonical.clone(), class.span.source_id))
            .collect::<HashMap<_, _>>();

        fs::write(&beta_path, "public class Beta {\n}\n").unwrap();
        let second = compiler.compile(&root).unwrap();
        let second_ids = second
            .program
            .classes
            .iter()
            .map(|class| (class.name.canonical.clone(), class.span.source_id))
            .collect::<HashMap<_, _>>();

        assert_eq!(second.incremental.parsed_files, [beta_path]);
        assert_eq!(first_ids, second_ids);
        fs::remove_dir_all(root).unwrap();
    }

    fn temporary_project(label: &str) -> PathBuf {
        let unique = format!(
            "apex-exec-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }
}
