use crate::{
    ast::Program as AstProgram,
    diagnostic::Diagnostic,
    hir,
    platform::{SchemaCatalog, import_metadata},
    span::{SourceId, Span},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

mod dependency;
mod diagnostics;
mod discovery;

pub use dependency::DependencyGraph;
use dependency::{build_dependency_graph, dependent_closure};
use diagnostics::SourceMap;
pub use diagnostics::{ProjectError, ProjectErrorKind};
pub use discovery::{DiscoveredProject, SourceFile, discover};

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
    pub schema: SchemaCatalog,
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
    last_schema: SchemaCatalog,
}

impl Default for ProjectCompiler {
    fn default() -> Self {
        Self {
            units: HashMap::new(),
            next_source_id: 1,
            last_fingerprints: BTreeMap::new(),
            last_dependencies: DependencyGraph::default(),
            last_compilation: None,
            last_schema: SchemaCatalog::new(),
        }
    }
}

impl ProjectCompiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn compile(&mut self, path: impl AsRef<Path>) -> Result<Compilation, ProjectError> {
        let project = discover(path)?;
        let schema = import_metadata(&project.source_roots)
            .map_err(|error| ProjectError::message(error.to_string()))?;
        let fingerprints = project
            .files
            .iter()
            .map(|file| (file.path.clone(), source_hash(&file.source)))
            .collect::<BTreeMap<_, _>>();

        if fingerprints == self.last_fingerprints
            && schema == self.last_schema
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

        let program = crate::semantic::check_with_schema(&merged, &schema)
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
            schema,
            source_map,
        };
        self.last_fingerprints = fingerprints;
        self.last_dependencies = dependencies;
        self.last_schema = compilation.schema.clone();
        self.last_compilation = Some(compilation.clone());
        Ok(compilation)
    }
}

pub fn compile(path: impl AsRef<Path>) -> Result<Compilation, ProjectError> {
    ProjectCompiler::new().compile(path)
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
        source_map.insert(unit.source_id, file.path.clone(), unit.source.clone());
    }
    (merged, source_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ClassMember, Statement};
    use std::fs;

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
