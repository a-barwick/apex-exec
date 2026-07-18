//! Deterministic statement debugger used by DAP and direct library clients.

use crate::{
    diagnostic::Diagnostic,
    project::Compilation,
    runtime::{
        DebugExecution, DebugSnapshot, DebugVariable, DmlEvent, Interpreter, TransactionEvent,
    },
    span::Span,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourcePosition {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Breakpoint {
    pub line: usize,
    pub verified: bool,
    pub actual_line: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Entry,
    Breakpoint,
    Step,
    Exception,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stop {
    pub reason: StopReason,
    pub position: Option<SourcePosition>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackFrame {
    pub id: usize,
    pub name: String,
    pub position: SourcePosition,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseInspection {
    pub visible_transaction_events: usize,
    pub dml_events: Vec<DmlEvent>,
}

/// A navigable debugger session backed by one immutable deterministic trace.
pub struct DebuggerSession {
    execution: DebugExecution,
    locations: Vec<SourcePosition>,
    frame_locations: Vec<Vec<SourcePosition>>,
    breakpoints: BTreeMap<PathBuf, BTreeSet<usize>>,
    cursor: Option<usize>,
}

impl DebuggerSession {
    pub fn for_script(path: impl Into<PathBuf>, source: &str) -> Result<Self, Diagnostic> {
        let path = path.into();
        let program = crate::check(source)?;
        let execution = Interpreter::new().debug_execute(&program);
        Ok(Self::from_execution(execution, |span| {
            Some(position_in_source(path.clone(), source, span))
        }))
    }

    pub fn for_project(compilation: &Compilation, target: &str) -> Result<Self, String> {
        let (class, method) = target
            .split_once('.')
            .ok_or_else(|| "debug target must have the form Class.method".to_owned())?;
        let execution = Interpreter::new().debug_invoke(&compilation.program, class, method);
        Ok(Self::from_execution(execution, |span| {
            compilation
                .source_position(span)
                .map(|(path, line, column)| SourcePosition { path, line, column })
        }))
    }

    fn from_execution(
        execution: DebugExecution,
        mut locate: impl FnMut(Span) -> Option<SourcePosition>,
    ) -> Self {
        let locations = execution
            .snapshots
            .iter()
            .map(|snapshot| locate(snapshot.span).unwrap_or_else(unknown_position))
            .collect();
        let frame_locations = execution
            .snapshots
            .iter()
            .map(|snapshot| {
                snapshot
                    .frames
                    .iter()
                    .map(|frame| locate(frame.span).unwrap_or_else(unknown_position))
                    .collect()
            })
            .collect();
        Self {
            execution,
            locations,
            frame_locations,
            breakpoints: BTreeMap::new(),
            cursor: None,
        }
    }

    pub fn set_breakpoints(
        &mut self,
        path: impl AsRef<Path>,
        requested_lines: &[usize],
    ) -> Vec<Breakpoint> {
        let path = path.as_ref();
        let executable = self
            .locations
            .iter()
            .filter(|location| same_path(&location.path, path))
            .map(|location| location.line)
            .collect::<BTreeSet<_>>();
        let mut accepted = BTreeSet::new();
        let breakpoints = requested_lines
            .iter()
            .map(|line| {
                let actual_line = executable.range(*line..).next().copied();
                if let Some(actual_line) = actual_line {
                    accepted.insert(actual_line);
                }
                Breakpoint {
                    line: *line,
                    verified: actual_line.is_some(),
                    actual_line,
                }
            })
            .collect();
        self.breakpoints.insert(path.to_path_buf(), accepted);
        breakpoints
    }

    pub fn start(&mut self, stop_on_entry: bool) -> Stop {
        if self.execution.snapshots.is_empty() {
            return self.finish_stop();
        }
        if stop_on_entry {
            self.cursor = Some(0);
            return self.stop(StopReason::Entry);
        }
        self.continue_execution()
    }

    pub fn continue_execution(&mut self) -> Stop {
        let start = self.cursor.map_or(0, |cursor| cursor + 1);
        for index in start..self.locations.len() {
            if self.is_breakpoint(&self.locations[index]) {
                self.cursor = Some(index);
                return self.stop(StopReason::Breakpoint);
            }
        }
        self.finish_stop()
    }

    pub fn step_in(&mut self) -> Stop {
        self.move_to_next(|_, _| true)
    }

    pub fn step_over(&mut self) -> Stop {
        let depth = self
            .current_snapshot()
            .map_or(0, |snapshot| snapshot.frames.len());
        self.move_to_next(|_, snapshot| snapshot.frames.len() <= depth)
    }

    pub fn step_out(&mut self) -> Stop {
        let depth = self
            .current_snapshot()
            .map_or(0, |snapshot| snapshot.frames.len());
        self.move_to_next(|_, snapshot| snapshot.frames.len() < depth)
    }

    pub fn stack_frames(&self) -> Vec<StackFrame> {
        let Some(cursor) = self.cursor else {
            return Vec::new();
        };
        self.execution.snapshots[cursor]
            .frames
            .iter()
            .zip(&self.frame_locations[cursor])
            .enumerate()
            .map(|(id, (frame, position))| StackFrame {
                id: id + 1,
                name: frame.name.clone(),
                position: position.clone(),
            })
            .collect()
    }

    pub fn variables(&self) -> &[DebugVariable] {
        self.current_snapshot()
            .map_or(&[], |snapshot| snapshot.variables.as_slice())
    }

    pub fn output(&self) -> &[String] {
        &self.execution.output
    }

    pub fn diagnostic(&self) -> Option<&Diagnostic> {
        self.execution.diagnostic.as_ref()
    }

    pub fn visible_timeline(&self) -> &[TransactionEvent] {
        let visible = self
            .current_snapshot()
            .map_or(self.execution.timeline.len(), |snapshot| {
                snapshot.transaction_event_count
            })
            .min(self.execution.timeline.len());
        &self.execution.timeline[..visible]
    }

    pub fn inspect_database(&self) -> DatabaseInspection {
        let timeline = self.visible_timeline();
        DatabaseInspection {
            visible_transaction_events: timeline.len(),
            dml_events: timeline
                .iter()
                .filter_map(|event| match event {
                    TransactionEvent::Dml(event) => Some(event.clone()),
                    TransactionEvent::Trigger(_) => None,
                })
                .collect(),
        }
    }

    fn move_to_next(&mut self, predicate: impl Fn(usize, &DebugSnapshot) -> bool) -> Stop {
        let start = self.cursor.map_or(0, |cursor| cursor + 1);
        for (index, snapshot) in self.execution.snapshots.iter().enumerate().skip(start) {
            if predicate(index, snapshot) {
                self.cursor = Some(index);
                return self.stop(StopReason::Step);
            }
        }
        self.finish_stop()
    }

    fn current_snapshot(&self) -> Option<&DebugSnapshot> {
        self.cursor
            .and_then(|cursor| self.execution.snapshots.get(cursor))
    }

    fn is_breakpoint(&self, location: &SourcePosition) -> bool {
        self.breakpoints
            .iter()
            .any(|(path, lines)| same_path(path, &location.path) && lines.contains(&location.line))
    }

    fn stop(&self, reason: StopReason) -> Stop {
        Stop {
            reason,
            position: self.cursor.map(|cursor| self.locations[cursor].clone()),
            description: None,
        }
    }

    fn finish_stop(&mut self) -> Stop {
        self.cursor = None;
        if let Some(diagnostic) = &self.execution.diagnostic {
            Stop {
                reason: StopReason::Exception,
                position: None,
                description: Some(diagnostic.to_string()),
            }
        } else {
            Stop {
                reason: StopReason::Complete,
                position: None,
                description: None,
            }
        }
    }
}

fn position_in_source(path: PathBuf, source: &str, span: Span) -> SourcePosition {
    let offset = span.start.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    SourcePosition {
        path,
        line: source[..offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1,
        column: source[line_start..offset].chars().count() + 1,
    }
}

fn unknown_position() -> SourcePosition {
    SourcePosition {
        path: PathBuf::from("<unknown>"),
        line: 1,
        column: 1,
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    left == right
        || (left.file_name().is_some()
            && left.file_name() == right.file_name()
            && (left.is_relative() || right.is_relative()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoints_steps_frames_variables_and_completion_are_deterministic() {
        let source = "Integer total = 1;\ntotal++;\nSystem.debug(total);";
        let mut debugger = DebuggerSession::for_script("sample.apex", source).unwrap();
        assert_eq!(
            debugger.set_breakpoints("sample.apex", &[2, 99]),
            [
                Breakpoint {
                    line: 2,
                    verified: true,
                    actual_line: Some(2)
                },
                Breakpoint {
                    line: 99,
                    verified: false,
                    actual_line: None
                }
            ]
        );
        assert_eq!(debugger.start(true).reason, StopReason::Entry);
        assert!(debugger.variables().is_empty());
        assert_eq!(debugger.continue_execution().reason, StopReason::Breakpoint);
        assert_eq!(debugger.variables()[0].value, "1");
        assert_eq!(debugger.step_over().position.unwrap().line, 3);
        assert_eq!(debugger.output(), ["2"]);
        assert_eq!(debugger.continue_execution().reason, StopReason::Complete);
    }

    #[test]
    fn runtime_failures_end_as_exception_stops() {
        let mut debugger =
            DebuggerSession::for_script("failure.apex", "Integer value = 1 / 0;").unwrap();
        assert_eq!(debugger.start(false).reason, StopReason::Exception);
        assert_eq!(
            debugger.diagnostic().unwrap().exception_type.as_deref(),
            Some("MathException")
        );
    }
}
