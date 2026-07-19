use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

pub const ENTERPRISE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateIdentity {
    pub name: String,
    pub repository: String,
    pub git_commit: String,
    pub git_tag: String,
    pub api_version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnterpriseInput {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnterpriseManifest {
    pub schema_version: u32,
    pub tool_version: String,
    pub candidate: CandidateIdentity,
    pub project: PathBuf,
    pub package_roots: Vec<PathBuf>,
    pub test_roots: Vec<PathBuf>,
    pub inputs: Vec<EnterpriseInput>,
    pub minimum_test_methods: usize,
    #[serde(skip)]
    source_path: PathBuf,
    #[serde(skip)]
    project_root: PathBuf,
}

impl EnterpriseManifest {
    pub fn generate(
        project: impl AsRef<Path>,
        candidate: CandidateIdentity,
        package_roots: Vec<PathBuf>,
        test_roots: Vec<PathBuf>,
    ) -> Result<Self, String> {
        let project_root = canonical(project.as_ref())?;
        validate_roots(&project_root, &package_roots, &test_roots)?;
        let inputs = collect_inputs(&project_root, &package_roots)?;
        let manifest = Self {
            schema_version: ENTERPRISE_SCHEMA_VERSION,
            tool_version: env!("CARGO_PKG_VERSION").to_owned(),
            candidate,
            project: PathBuf::from("."),
            package_roots,
            test_roots,
            inputs,
            minimum_test_methods: 100,
            source_path: project_root.join("apex-exec-enterprise.json"),
            project_root,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let requested = path.as_ref();
        let source = fs::read_to_string(requested).map_err(|error| {
            format!(
                "failed to read enterprise manifest `{}`: {error}",
                requested.display()
            )
        })?;
        let mut manifest = serde_json::from_str::<Self>(&source).map_err(|error| {
            format!(
                "invalid enterprise manifest `{}`: {error}",
                requested.display()
            )
        })?;
        manifest.source_path = canonical(requested)?;
        let parent = manifest
            .source_path
            .parent()
            .expect("a manifest path has a parent");
        manifest.project_root = canonical(&parent.join(&manifest.project))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = absolute(path.as_ref())?;
        let parent = path
            .parent()
            .ok_or_else(|| "enterprise manifest output must have a parent".to_owned())?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create enterprise manifest directory `{}`: {error}",
                parent.display()
            )
        })?;
        let parent = canonical(parent)?;
        let mut portable = self.clone();
        portable.project = relative_between(&parent, &self.project_root);
        portable.source_path = PathBuf::new();
        portable.project_root = PathBuf::new();
        portable.validate_serialized()?;
        let mut json = serde_json::to_string_pretty(&portable)
            .map_err(|error| format!("failed to serialize enterprise manifest: {error}"))?;
        json.push('\n');
        fs::write(&path, json).map_err(|error| {
            format!(
                "failed to write enterprise manifest `{}`: {error}",
                path.display()
            )
        })
    }

    pub fn verify_inputs(&self) -> Result<(), String> {
        self.validate()?;
        let actual = collect_inputs(&self.project_root, &self.package_roots)?;
        if actual == self.inputs {
            return Ok(());
        }
        let expected = self
            .inputs
            .iter()
            .map(|input| (&input.path, &input.sha256))
            .collect::<std::collections::BTreeMap<_, _>>();
        let current = actual
            .iter()
            .map(|input| (&input.path, &input.sha256))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut differences = Vec::new();
        for path in expected
            .keys()
            .chain(current.keys())
            .collect::<BTreeSet<_>>()
        {
            match (expected.get(*path), current.get(*path)) {
                (Some(expected), Some(actual)) if expected != actual => {
                    differences.push(format!("modified `{}`", path.display()));
                }
                (Some(_), None) => differences.push(format!("missing `{}`", path.display())),
                (None, Some(_)) => differences.push(format!("unrecorded `{}`", path.display())),
                _ => {}
            }
        }
        Err(format!(
            "enterprise candidate input verification failed: {}",
            differences.join(", ")
        ))
    }

    pub fn sha256(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|error| format!("failed to serialize enterprise identity: {error}"))?;
        Ok(sha256(&bytes))
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    fn validate(&self) -> Result<(), String> {
        self.validate_serialized()?;
        if self.project_root.as_os_str().is_empty() {
            return Err("enterprise manifest has no resolved project root".to_owned());
        }
        validate_roots(&self.project_root, &self.package_roots, &self.test_roots)
    }

    fn validate_serialized(&self) -> Result<(), String> {
        if self.schema_version != ENTERPRISE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported enterprise manifest schema version {}; expected {}",
                self.schema_version, ENTERPRISE_SCHEMA_VERSION
            ));
        }
        if self.tool_version != env!("CARGO_PKG_VERSION") {
            return Err(format!(
                "enterprise manifest requires apex-exec {}, but this binary is {}",
                self.tool_version,
                env!("CARGO_PKG_VERSION")
            ));
        }
        validate_candidate(&self.candidate)?;
        if self.minimum_test_methods < 100 {
            return Err("enterprise raw denominator must require at least 100 tests".to_owned());
        }
        if self.inputs.is_empty() {
            return Err("enterprise manifest must record at least one input".to_owned());
        }
        validate_project_path(&self.project)?;
        let mut previous = None;
        for input in &self.inputs {
            validate_relative(&input.path, "input")?;
            validate_sha256(&input.sha256, "input")?;
            if previous.is_some_and(|path: &PathBuf| path >= &input.path) {
                return Err(
                    "enterprise inputs must be unique and sorted by relative path".to_owned(),
                );
            }
            previous = Some(&input.path);
        }
        Ok(())
    }
}

fn validate_candidate(candidate: &CandidateIdentity) -> Result<(), String> {
    if candidate.name.trim().is_empty()
        || candidate.repository.trim().is_empty()
        || candidate.git_tag.trim().is_empty()
        || candidate.api_version.trim().is_empty()
    {
        return Err("enterprise candidate identity fields cannot be empty".to_owned());
    }
    if !candidate.repository.starts_with("https://") {
        return Err("enterprise candidate repository must use an HTTPS URL".to_owned());
    }
    if candidate.git_commit.len() != 40
        || !candidate
            .git_commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("enterprise candidate commit must be a lowercase 40-character SHA".to_owned());
    }
    let mut version_parts = candidate.api_version.split('.');
    let major = version_parts.next().unwrap_or_default();
    let minor = version_parts.next().unwrap_or_default();
    if version_parts.next().is_some()
        || major.parse::<u32>().is_err()
        || minor.parse::<u32>().is_err()
    {
        return Err("enterprise candidate API version must have `major.minor` form".to_owned());
    }
    Ok(())
}

fn validate_roots(
    project_root: &Path,
    package_roots: &[PathBuf],
    test_roots: &[PathBuf],
) -> Result<(), String> {
    if package_roots.is_empty() {
        return Err("enterprise manifest requires at least one package root".to_owned());
    }
    if test_roots.is_empty() {
        return Err("enterprise manifest requires at least one test root".to_owned());
    }
    validate_sorted_roots(package_roots, "package")?;
    validate_sorted_roots(test_roots, "test")?;
    let resolved_packages = package_roots
        .iter()
        .map(|root| canonical_directory(project_root, root, "package"))
        .collect::<Result<Vec<_>, _>>()?;
    for root in test_roots {
        let resolved = canonical_directory(project_root, root, "test")?;
        if !resolved_packages
            .iter()
            .any(|package| resolved.starts_with(package))
        {
            return Err(format!(
                "enterprise test root `{}` must be inside a package root",
                root.display()
            ));
        }
    }
    Ok(())
}

fn validate_sorted_roots(roots: &[PathBuf], label: &str) -> Result<(), String> {
    let mut previous = None;
    for root in roots {
        validate_relative(root, label)?;
        if previous.is_some_and(|path: &PathBuf| path >= root) {
            return Err(format!(
                "enterprise {label} roots must be unique and sorted"
            ));
        }
        previous = Some(root);
    }
    Ok(())
}

fn canonical_directory(project_root: &Path, root: &Path, label: &str) -> Result<PathBuf, String> {
    let resolved = canonical(&project_root.join(root))?;
    if !resolved.starts_with(project_root) || !resolved.is_dir() {
        return Err(format!(
            "enterprise {label} root `{}` must resolve to a directory below the project",
            root.display()
        ));
    }
    Ok(resolved)
}

fn collect_inputs(
    project_root: &Path,
    package_roots: &[PathBuf],
) -> Result<Vec<EnterpriseInput>, String> {
    let mut paths = Vec::new();
    for root in package_roots {
        collect_files(
            project_root,
            &canonical_directory(project_root, root, "package")?,
            &mut paths,
        )?;
    }
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|path| {
            let bytes = fs::read(project_root.join(&path))
                .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?;
            Ok(EnterpriseInput {
                path,
                sha256: sha256(&bytes),
            })
        })
        .collect()
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to scan `{}`: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to scan `{}`: {error}", directory.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect `{}`: {error}", path.display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "enterprise candidate inventory refuses symlink `{}`",
                path.display()
            ));
        }
        if file_type.is_dir() {
            collect_files(root, &path, files)?;
        } else if file_type.is_file() {
            files.push(
                path.strip_prefix(root)
                    .expect("walked input remains below project root")
                    .to_owned(),
            );
        }
    }
    Ok(())
}

fn validate_relative(path: &Path, label: &str) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!(
            "enterprise {label} path `{}` must be a safe relative path",
            path.display()
        ));
    }
    Ok(())
}

fn validate_project_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            !matches!(
                component,
                Component::Normal(_) | Component::CurDir | Component::ParentDir
            )
        })
    {
        return Err(format!(
            "enterprise project path `{}` must be relative",
            path.display()
        ));
    }
    Ok(())
}

fn validate_sha256(digest: &str, label: &str) -> Result<(), String> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!("enterprise {label} has an invalid SHA-256 digest"));
    }
    Ok(())
}

fn sha256(bytes: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(bytes.as_ref());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|error| format!("failed to resolve `{}`: {error}", path.display()))
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .map_err(|error| format!("failed to resolve current directory: {error}"))
    }
}

fn relative_between(base: &Path, target: &Path) -> PathBuf {
    let base_parts = base.components().collect::<Vec<_>>();
    let target_parts = target.components().collect::<Vec<_>>();
    let common = base_parts
        .iter()
        .zip(&target_parts)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();
    for _ in common..base_parts.len() {
        relative.push("..");
    }
    for component in &target_parts[common..] {
        relative.push(component.as_os_str());
    }
    if relative.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        relative
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn manifest_round_trip_binds_every_candidate_byte() {
        let root = fixture_root("round-trip");
        write(
            &root.join("force-app/main/default/classes/Demo.cls"),
            "@IsTest private class Demo { @IsTest static void passes() {} }",
        );
        write(
            &root.join("force-app/main/default/classes/Demo.cls-meta.xml"),
            "<ApexClass/>",
        );
        let manifest = EnterpriseManifest::generate(
            &root,
            candidate(),
            vec![PathBuf::from("force-app")],
            vec![PathBuf::from("force-app/main/default/classes")],
        )
        .unwrap();
        assert_eq!(manifest.inputs.len(), 2);
        let output = root.join("evidence/manifest.json");
        manifest.write(&output).unwrap();
        let loaded = EnterpriseManifest::load(&output).unwrap();
        loaded.verify_inputs().unwrap();
        assert_eq!(loaded.candidate, candidate());

        write(
            &root.join("force-app/main/default/classes/Demo.cls"),
            "@IsTest private class Demo { @IsTest static void changed() {} }",
        );
        assert!(loaded.verify_inputs().unwrap_err().contains("modified"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_rejects_unsafe_roots_and_weak_denominators() {
        let root = fixture_root("invalid");
        fs::create_dir_all(root.join("force-app/tests")).unwrap();
        let error = EnterpriseManifest::generate(
            &root,
            candidate(),
            vec![PathBuf::from("force-app")],
            vec![PathBuf::from("../outside")],
        )
        .unwrap_err();
        assert!(error.contains("safe relative"));

        write(
            &root.join("force-app/tests/Demo.cls"),
            "public class Demo {}",
        );
        let mut manifest = EnterpriseManifest::generate(
            &root,
            candidate(),
            vec![PathBuf::from("force-app")],
            vec![PathBuf::from("force-app/tests")],
        )
        .unwrap();
        manifest.minimum_test_methods = 99;
        assert!(manifest.validate().unwrap_err().contains("at least 100"));
        fs::remove_dir_all(root).unwrap();
    }

    fn candidate() -> CandidateIdentity {
        CandidateIdentity {
            name: "Candidate".to_owned(),
            repository: "https://example.com/candidate.git".to_owned(),
            git_commit: "a".repeat(40),
            git_tag: "v1.0.0".to_owned(),
            api_version: "65.0".to_owned(),
        }
    }

    fn fixture_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "apex-exec-enterprise-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}
