use super::{CachedUnit, SourceFile};
use crate::ast::{
    AssignmentTarget, Expression, NamedType, Program,
    visit::{self, Visitor},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    path::{Path, PathBuf},
};

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

    /// Returns `paths` and every source file that depends on them, transitively.
    pub fn dependent_closure(&self, paths: impl IntoIterator<Item = PathBuf>) -> BTreeSet<PathBuf> {
        dependent_closure(&paths.into_iter().collect(), self)
    }

    /// Returns whether the graph contains an exact source path.
    pub fn contains_file(&self, path: &Path) -> bool {
        self.edges.contains_key(path)
    }
}

pub(super) fn build_dependency_graph(
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
        let names = DependencyCollector::collect(&units[&file.path].ast);
        let dependencies = names
            .into_iter()
            .filter_map(|name| owners.get(&name).cloned())
            .filter(|dependency| dependency != &file.path)
            .collect();
        edges.insert(file.path.clone(), dependencies);
    }
    DependencyGraph { edges }
}

pub(super) fn dependent_closure(
    changed: &BTreeSet<PathBuf>,
    graph: &DependencyGraph,
) -> BTreeSet<PathBuf> {
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

#[derive(Default)]
struct DependencyCollector {
    names: BTreeSet<String>,
}

impl DependencyCollector {
    fn collect(program: &Program) -> BTreeSet<String> {
        let mut collector = Self::default();
        collector.visit_program(program);
        collector.names
    }

    fn record_reference(&mut self, canonical: &str) {
        self.names.insert(canonical.to_owned());
    }
}

impl<'ast> Visitor<'ast> for DependencyCollector {
    fn visit_named_type(&mut self, named_type: &'ast NamedType) {
        self.record_reference(&named_type.canonical);
        visit::walk_named_type(self, named_type);
    }

    fn visit_expression(&mut self, expression: &'ast Expression) {
        if let Expression::Variable(identifier) = expression {
            self.record_reference(&identifier.canonical);
        }
        visit::walk_expression(self, expression);
    }

    fn visit_assignment_target(&mut self, target: &'ast AssignmentTarget) {
        if let AssignmentTarget::Variable(identifier) = target {
            self.record_reference(&identifier.canonical);
        }
        visit::walk_assignment_target(self, target);
    }
}
