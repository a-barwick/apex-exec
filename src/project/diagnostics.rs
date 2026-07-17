use crate::{
    diagnostic::Diagnostic,
    span::{SourceId, Span},
};
use std::{
    fmt,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default)]
pub(super) struct SourceMap {
    entries: Vec<SourceEntry>,
}

#[derive(Clone, Debug)]
struct SourceEntry {
    source_id: SourceId,
    path: PathBuf,
    source: String,
}

impl SourceMap {
    pub(super) fn insert(&mut self, source_id: SourceId, path: PathBuf, source: String) {
        self.entries.push(SourceEntry {
            source_id,
            path,
            source,
        });
    }

    pub(super) fn render_diagnostic(&self, diagnostic: &Diagnostic) -> String {
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

    pub(super) fn project_error(&self, diagnostic: Diagnostic) -> ProjectError {
        ProjectError::project_diagnostic(self.clone(), diagnostic)
    }

    pub(super) fn location(&self, span: Span) -> Option<(PathBuf, usize)> {
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
    kind: ProjectErrorKind,
    message: String,
    path: Option<PathBuf>,
    source: String,
    source_map: Option<SourceMap>,
    diagnostic: Option<Box<Diagnostic>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProjectErrorKind {
    Project,
    Io,
    Diagnostic,
}

impl ProjectError {
    pub(super) fn message(message: impl Into<String>) -> Self {
        Self {
            kind: ProjectErrorKind::Project,
            message: message.into(),
            path: None,
            source: String::new(),
            source_map: None,
            diagnostic: None,
        }
    }

    pub(super) fn io(path: &Path, action: &str, error: std::io::Error) -> Self {
        Self {
            kind: ProjectErrorKind::Io,
            message: format!("failed to {action} `{}`: {error}", path.display()),
            path: Some(path.to_path_buf()),
            source: String::new(),
            source_map: None,
            diagnostic: None,
        }
    }

    pub(super) fn diagnostic(
        path: Option<PathBuf>,
        source: String,
        diagnostic: Diagnostic,
    ) -> Self {
        Self {
            kind: ProjectErrorKind::Diagnostic,
            message: diagnostic.message.clone(),
            path,
            source,
            source_map: None,
            diagnostic: Some(Box::new(diagnostic)),
        }
    }

    fn project_diagnostic(source_map: SourceMap, diagnostic: Diagnostic) -> Self {
        Self {
            kind: ProjectErrorKind::Diagnostic,
            message: diagnostic.message.clone(),
            path: None,
            source: String::new(),
            source_map: Some(source_map),
            diagnostic: Some(Box::new(diagnostic)),
        }
    }

    pub fn kind(&self) -> ProjectErrorKind {
        self.kind
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_machine_readable_project_error_categories() {
        let project = ProjectError::message("bad project");
        assert_eq!(project.kind(), ProjectErrorKind::Project);
        assert_eq!(project.path(), None);

        let path = Path::new("sfdx-project.json");
        let io = ProjectError::io(
            path,
            "read",
            std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
        );
        assert_eq!(io.kind(), ProjectErrorKind::Io);
        assert_eq!(io.path(), Some(path));

        let diagnostic =
            ProjectError::diagnostic(None, String::new(), Diagnostic::new("bad", Span::new(0, 1)));
        assert_eq!(diagnostic.kind(), ProjectErrorKind::Diagnostic);
    }
}
