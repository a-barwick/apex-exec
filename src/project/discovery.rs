use super::ProjectError;
use crate::compatibility::{ApiVersion, CompatibilityProfile, EffectiveProfile, ProfileOrigin};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub source: String,
    pub profile: CompatibilityProfile,
    pub profile_origin: ProfileOrigin,
}

#[derive(Clone, Debug)]
pub struct DiscoveredProject {
    pub root: PathBuf,
    pub source_roots: Vec<PathBuf>,
    pub files: Vec<SourceFile>,
    pub project_profile: CompatibilityProfile,
}

pub fn discover(path: impl AsRef<Path>) -> Result<DiscoveredProject, ProjectError> {
    let requested = path.as_ref();
    let root = find_project_root(requested)?;
    let config_path = root.join("sfdx-project.json");
    let config = fs::read_to_string(&config_path)
        .map_err(|error| ProjectError::io(&config_path, "read", error))?;
    let package_paths = extract_package_paths(&config)?;
    let project_profile = extract_project_profile(&config, &config_path)?;
    let mut source_roots = package_paths
        .into_iter()
        .map(|path| root.join(path))
        .collect::<Vec<_>>();
    source_roots.sort();
    source_roots.dedup();
    let mut paths = Vec::new();
    for source_root in &source_roots {
        collect_apex_files(source_root, &mut paths)?;
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Err(ProjectError::message(format!(
            "no `.cls` or `.trigger` files found in SFDX project `{}`",
            root.display()
        )));
    }
    let files = paths
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path)
                .map_err(|error| ProjectError::io(&path, "read", error))?;
            let (profile, profile_origin) = source_profile(&path, project_profile)?;
            Ok(SourceFile {
                path,
                source,
                profile,
                profile_origin,
            })
        })
        .collect::<Result<Vec<_>, ProjectError>>()?;
    Ok(DiscoveredProject {
        root,
        source_roots,
        files,
        project_profile,
    })
}

impl DiscoveredProject {
    pub fn effective_profiles(&self) -> Vec<EffectiveProfile> {
        let mut profiles = self
            .files
            .iter()
            .map(|file| {
                let source = file
                    .path
                    .strip_prefix(&self.root)
                    .unwrap_or(&file.path)
                    .to_string_lossy()
                    .replace('\\', "/");
                EffectiveProfile::new(source, file.profile, file.profile_origin)
            })
            .collect::<Vec<_>>();
        profiles.sort();
        profiles
    }
}

fn extract_project_profile(
    config: &str,
    config_path: &Path,
) -> Result<CompatibilityProfile, ProjectError> {
    let json = serde_json::from_str::<Value>(config).map_err(|error| {
        ProjectError::message(format!(
            "invalid Salesforce project configuration `{}`: {error}",
            config_path.display()
        ))
    })?;
    let version = json
        .get("sourceApiVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ProjectError::message(format!(
                "Salesforce project configuration `{}` must declare `sourceApiVersion`",
                config_path.display()
            ))
        })?;
    parse_profile(version, &format!("project `{}`", config_path.display()))
}

fn source_profile(
    source_path: &Path,
    project_profile: CompatibilityProfile,
) -> Result<(CompatibilityProfile, ProfileOrigin), ProjectError> {
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ProjectError::message(format!(
                "Apex source path `{}` has no UTF-8 filename",
                source_path.display()
            ))
        })?;
    let sidecar = source_path.with_file_name(format!("{file_name}-meta.xml"));
    if !sidecar.is_file() {
        return Ok((project_profile, ProfileOrigin::ProjectDefault));
    }
    let metadata =
        fs::read_to_string(&sidecar).map_err(|error| ProjectError::io(&sidecar, "read", error))?;
    let version = one_xml_text(&metadata, "apiVersion").map_err(|message| {
        ProjectError::message(format!(
            "invalid Apex metadata sidecar `{}`: {message}",
            sidecar.display()
        ))
    })?;
    Ok((
        parse_profile(&version, &format!("sidecar `{}`", sidecar.display()))?,
        ProfileOrigin::Sidecar,
    ))
}

fn parse_profile(value: &str, source: &str) -> Result<CompatibilityProfile, ProjectError> {
    let version = value
        .parse::<ApiVersion>()
        .map_err(|message| ProjectError::message(format!("{source}: {message}")))?;
    CompatibilityProfile::for_api_version(version)
        .map_err(|message| ProjectError::message(format!("{source}: {message}")))
}

fn one_xml_text(source: &str, tag: &str) -> Result<String, String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut matches = source.match_indices(&open);
    let Some((start, _)) = matches.next() else {
        return Err(format!("missing `<{tag}>`"));
    };
    if matches.next().is_some() {
        return Err(format!("contains more than one `<{tag}>`"));
    }
    let value_start = start + open.len();
    let value_end = source[value_start..]
        .find(&close)
        .map(|offset| value_start + offset)
        .ok_or_else(|| format!("missing `</{tag}>`"))?;
    if source[value_end + close.len()..].contains(&close) {
        return Err(format!("contains more than one `</{tag}>`"));
    }
    let value = source[value_start..value_end].trim();
    if value.is_empty() {
        return Err(format!("`<{tag}>` cannot be empty"));
    }
    Ok(value.to_owned())
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

fn collect_apex_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), ProjectError> {
    let entries =
        fs::read_dir(directory).map_err(|error| ProjectError::io(directory, "scan", error))?;
    for entry in entries {
        let entry = entry.map_err(|error| ProjectError::io(directory, "scan", error))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| ProjectError::io(&path, "inspect", error))?;
        if file_type.is_dir() {
            collect_apex_files(&path, files)?;
        } else if file_type.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("cls")
                        || extension.eq_ignore_ascii_case("trigger")
                })
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
