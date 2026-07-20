use crate::{
    ast::Program as AstProgram,
    compatibility::{CompatibilityProfile, EffectiveProfile, SourceProfiles},
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
mod security;

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
    pub security: crate::platform::SecurityPolicy,
    pub(crate) database_fixtures: Vec<crate::platform::Record>,
    pub profiles: Vec<EffectiveProfile>,
    source_map: SourceMap,
}

impl Compilation {
    pub fn invoke(&self, target: &str) -> Result<Vec<String>, ProjectError> {
        let (class, method) = target.split_once('.').ok_or_else(|| {
            ProjectError::message("invocation target must have the form Class.method")
        })?;
        let mut host = crate::runtime::RecordingHost::default();
        host.set_security_policy(self.security.clone());
        host.set_database_fixtures(self.database_fixtures.clone());
        crate::runtime::Interpreter::with_host(host)
            .invoke_static(&self.program, class, method)
            .map_err(|diagnostic| self.source_map.project_error(diagnostic))
    }

    pub(crate) fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String {
        self.source_map.render_diagnostic(diagnostic)
    }

    pub(crate) fn source_location(&self, span: Span) -> Option<(PathBuf, usize)> {
        self.source_map.location(span)
    }

    /// Maps a checked span to its project path and one-based line/column.
    pub fn source_position(&self, span: Span) -> Option<(PathBuf, usize, usize)> {
        self.source_map.position(span)
    }

    /// Returns the source text and path assigned to a compiler source identity.
    pub fn source_text(&self, source_id: SourceId) -> Option<(&Path, &str)> {
        self.source_map.source(source_id)
    }

    pub fn effective_profiles(&self) -> &[EffectiveProfile] {
        &self.profiles
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
    last_security: security::LoadedSecurityFixture,
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
            last_security: security::LoadedSecurityFixture::default(),
        }
    }
}

impl ProjectCompiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn compile(&mut self, path: impl AsRef<Path>) -> Result<Compilation, ProjectError> {
        let project = discover(path)?;
        let mut schema_roots = project.source_roots.clone();
        let local_schema = project.root.join(".apex-exec/schema");
        if local_schema.is_dir() {
            schema_roots.push(local_schema);
        }
        let schema = import_metadata(&schema_roots)
            .map_err(|error| ProjectError::message(error.to_string()))?;
        let security = security::load(&project.root).map_err(ProjectError::message)?;
        let fingerprints = project
            .files
            .iter()
            .map(|file| (file.path.clone(), source_fingerprint(file)))
            .collect::<BTreeMap<_, _>>();

        if fingerprints == self.last_fingerprints
            && schema == self.last_schema
            && security == self.last_security
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
            let hash = source_hash(&file.source);
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
            validate_source_unit(&file.path, &ast)?;
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
        let source_profiles = source_profiles(&project.files, &self.units, project.project_profile);
        let dependencies = build_dependency_graph(&project.files, &self.units);
        let changed = fingerprints
            .iter()
            .filter(|(path, fingerprint)| self.last_fingerprints.get(*path) != Some(*fingerprint))
            .map(|(path, _)| path.clone())
            .chain(removed)
            .collect::<BTreeSet<_>>();
        let mut invalidated = dependent_closure(&changed, &self.last_dependencies);
        invalidated.extend(dependent_closure(&changed, &dependencies));

        let program =
            crate::semantic::check_with_schema_and_profiles(&merged, &schema, source_profiles)
                .map_err(|diagnostic| source_map.project_error(diagnostic))?;
        let profiles = project.effective_profiles();
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
            security: security.policy.clone(),
            database_fixtures: security.records.clone(),
            profiles,
            source_map,
        };
        self.last_fingerprints = fingerprints;
        self.last_dependencies = dependencies;
        self.last_schema = compilation.schema.clone();
        self.last_security = security;
        self.last_compilation = Some(compilation.clone());
        Ok(compilation)
    }
}

fn validate_source_unit(path: &Path, ast: &crate::ast::Program) -> Result<(), ProjectError> {
    let expected_name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let is_trigger = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("trigger"));
    if is_trigger {
        validate_trigger_unit(path, ast, expected_name)
    } else {
        validate_type_unit(path, ast, expected_name)
    }
}

fn validate_trigger_unit(
    path: &Path,
    ast: &crate::ast::Program,
    expected_name: &str,
) -> Result<(), ProjectError> {
    if !ast.classes.is_empty()
        || !ast.methods.is_empty()
        || !ast.statements.is_empty()
        || ast.triggers.len() != 1
    {
        return Err(ProjectError::message(format!(
            "`{}` must contain exactly one top-level Apex trigger",
            path.display()
        )));
    }
    if !ast.triggers[0]
        .name
        .spelling
        .eq_ignore_ascii_case(expected_name)
    {
        return Err(ProjectError::message(format!(
            "trigger `{}` must be declared in `{expected_name}.trigger`",
            ast.triggers[0].name.spelling
        )));
    }
    Ok(())
}

fn validate_type_unit(
    path: &Path,
    ast: &crate::ast::Program,
    expected_name: &str,
) -> Result<(), ProjectError> {
    let top_level_types = ast
        .classes
        .iter()
        .filter(|class| class.enclosing_type.is_none())
        .collect::<Vec<_>>();
    if !ast.triggers.is_empty()
        || !ast.methods.is_empty()
        || !ast.statements.is_empty()
        || top_level_types.len() != 1
    {
        return Err(ProjectError::message(format!(
            "`{}` must contain exactly one top-level Apex class, interface, or enum",
            path.display()
        )));
    }
    if !top_level_types[0]
        .name
        .spelling
        .eq_ignore_ascii_case(expected_name)
    {
        return Err(ProjectError::message(format!(
            "type `{}` must be declared in `{expected_name}.cls`",
            top_level_types[0].name.spelling
        )));
    }
    Ok(())
}

pub fn compile(path: impl AsRef<Path>) -> Result<Compilation, ProjectError> {
    ProjectCompiler::new().compile(path)
}

/// Compiles an explicit source closure against an already imported schema.
///
/// Enterprise compatibility measurement uses this boundary to prove that each
/// Salesforce test's required source closure checks independently, without
/// copying or rewriting the pinned project.
pub(crate) fn compile_source_subset(
    root: PathBuf,
    files: &[SourceFile],
    schema: &SchemaCatalog,
) -> Result<Compilation, ProjectError> {
    if files.is_empty() {
        return Err(ProjectError::message(
            "an explicit source closure cannot be empty",
        ));
    }
    let mut units = HashMap::new();
    for (index, file) in files.iter().enumerate() {
        let source_id = SourceId::new(index + 1);
        let ast = crate::parse_with_source(&file.source, source_id).map_err(|diagnostic| {
            ProjectError::diagnostic(Some(file.path.clone()), file.source.clone(), diagnostic)
        })?;
        validate_source_unit(&file.path, &ast)?;
        units.insert(
            file.path.clone(),
            CachedUnit {
                hash: source_hash(&file.source),
                source_id,
                source: file.source.clone(),
                ast,
            },
        );
    }
    let (merged, source_map) = merge_units(files, &units);
    let dependencies = build_dependency_graph(files, &units);
    let default_profile = files.first().map(|file| file.profile).unwrap_or_default();
    let program = crate::semantic::check_with_schema_and_profiles(
        &merged,
        schema,
        source_profiles(files, &units, default_profile),
    )
    .map_err(|diagnostic| source_map.project_error(diagnostic))?;
    let mut profiles = files
        .iter()
        .map(|file| {
            let source = file
                .path
                .strip_prefix(&root)
                .unwrap_or(&file.path)
                .to_string_lossy()
                .replace('\\', "/");
            EffectiveProfile::new(source, file.profile, file.profile_origin)
        })
        .collect::<Vec<_>>();
    profiles.sort();
    Ok(Compilation {
        root,
        program,
        dependencies,
        incremental: IncrementalReport {
            parsed_files: files.iter().map(|file| file.path.clone()).collect(),
            reused_files: Vec::new(),
            invalidated_files: Vec::new(),
        },
        schema: schema.clone(),
        security: crate::platform::SecurityPolicy::default(),
        database_fixtures: Vec::new(),
        profiles,
        source_map,
    })
}

fn source_hash(source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn source_fingerprint(file: &SourceFile) -> u64 {
    let mut hasher = DefaultHasher::new();
    file.source.hash(&mut hasher);
    file.profile.hash(&mut hasher);
    hasher.finish()
}

fn source_profiles(
    files: &[SourceFile],
    units: &HashMap<PathBuf, CachedUnit>,
    default: CompatibilityProfile,
) -> SourceProfiles {
    let mut profiles = SourceProfiles::new(default);
    for file in files {
        profiles.insert(units[&file.path].source_id, file.profile);
    }
    profiles
}

fn merge_units(
    files: &[SourceFile],
    units: &HashMap<PathBuf, CachedUnit>,
) -> (AstProgram, SourceMap) {
    let mut merged = AstProgram {
        classes: Vec::new(),
        triggers: Vec::new(),
        methods: Vec::new(),
        statements: Vec::new(),
    };
    let mut source_map = SourceMap::default();
    for file in files {
        let unit = &units[&file.path];
        let ast = unit.ast.clone();
        merged.classes.extend(ast.classes);
        merged.triggers.extend(ast.triggers);
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
                profile: CompatibilityProfile::default(),
                profile_origin: crate::compatibility::ProfileOrigin::ProjectDefault,
            },
            SourceFile {
                path: beta_path.clone(),
                source: beta_source.clone(),
                profile: CompatibilityProfile::default(),
                profile_origin: crate::compatibility::ProfileOrigin::ProjectDefault,
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
            r#"{"packageDirectories":[{"path":"force-app","default":true}],"sourceApiVersion":"66.0"}"#,
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
