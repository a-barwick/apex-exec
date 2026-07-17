use super::ProjectError;
use std::{
    fs,
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
