use crate::span::Span;
use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
};

/// Maximum number of pre-statement snapshots retained by one debugger launch.
pub(crate) const MAX_DEBUG_SNAPSHOTS: usize = 4_096;

/// Maximum estimated bytes retained by debugger snapshots, including their
/// variable/frame structures and owned text.
pub(crate) const MAX_DEBUG_RETAINED_BYTES: usize = 16 * 1024 * 1024;

/// Maximum visible variables retained in one debugger snapshot.
pub(crate) const MAX_DEBUG_VARIABLES_PER_SNAPSHOT: usize = 256;

/// Maximum call frames retained in one debugger snapshot.
pub(crate) const MAX_DEBUG_FRAMES_PER_SNAPSHOT: usize = 128;

/// Maximum UTF-8 bytes retained for one debugger-rendered value.
pub(crate) const MAX_DEBUG_RENDERED_VALUE_BYTES: usize = 16 * 1024;

const MAX_DEBUG_METADATA_BYTES: usize = 1_024;
const TRUNCATION_MARKER: &str = "…";

/// A debugger-visible variable captured immediately before an Apex statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugVariable {
    pub name: String,
    pub type_name: String,
    pub value: String,
}

/// One source-mapped Apex frame in a debugger snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugFrame {
    pub name: String,
    pub span: Span,
}

/// Immutable runtime state captured at one executable statement boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugSnapshot {
    pub span: Span,
    pub frames: Vec<DebugFrame>,
    pub variables: Vec<DebugVariable>,
    pub transaction_event_count: usize,
}

/// Bounded-retention facts for one completed debugger trace.
///
/// A trace retains at most 4,096 snapshots and an estimated 16 MiB of snapshot
/// structures and owned text. Each snapshot retains at most 256 variables and
/// 128 frames, and each rendered value retains at most 16 KiB of UTF-8.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugTraceStatus {
    /// Estimated snapshot structure and owned-text bytes retained by the trace.
    pub retained_bytes: usize,
    /// Whether any snapshot, frame, variable, or rendered text was omitted.
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BranchHits {
    pub true_hits: usize,
    pub false_hits: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExecutionTrace {
    pub executed_statements: BTreeSet<Span>,
    pub branches: BTreeMap<Span, BranchHits>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InstrumentationPolicy {
    #[default]
    None,
    Coverage,
    Debugger,
}

pub(crate) enum StatementInstrumentation {
    None,
    CaptureDebugger { retained_byte_budget: usize },
}

pub(crate) struct InstrumentationState {
    policy: InstrumentationPolicy,
    trace: ExecutionTrace,
    snapshots: Vec<DebugSnapshot>,
    retained_debug_bytes: usize,
    debug_trace_truncated: bool,
}

impl InstrumentationState {
    pub(crate) fn new(policy: InstrumentationPolicy) -> Self {
        Self {
            policy,
            trace: ExecutionTrace::default(),
            snapshots: Vec::new(),
            retained_debug_bytes: 0,
            debug_trace_truncated: false,
        }
    }

    pub(crate) fn configure(&mut self, policy: InstrumentationPolicy) {
        *self = Self::new(policy);
    }

    pub(crate) fn before_statement(
        &mut self,
        span: Span,
        capture_debugger: bool,
    ) -> StatementInstrumentation {
        match self.policy {
            InstrumentationPolicy::None => StatementInstrumentation::None,
            InstrumentationPolicy::Coverage => {
                self.trace.executed_statements.insert(span);
                StatementInstrumentation::None
            }
            InstrumentationPolicy::Debugger if capture_debugger => {
                let retained_byte_budget =
                    MAX_DEBUG_RETAINED_BYTES.saturating_sub(self.retained_debug_bytes);
                if self.snapshots.len() >= MAX_DEBUG_SNAPSHOTS
                    || retained_byte_budget < size_of::<DebugSnapshot>()
                {
                    self.debug_trace_truncated = true;
                    StatementInstrumentation::None
                } else {
                    StatementInstrumentation::CaptureDebugger {
                        retained_byte_budget,
                    }
                }
            }
            InstrumentationPolicy::Debugger => StatementInstrumentation::None,
        }
    }

    pub(crate) fn record_branch(&mut self, span: Span, outcome: bool) {
        if self.policy != InstrumentationPolicy::Coverage {
            return;
        }
        let hits = self.trace.branches.entry(span).or_default();
        if outcome {
            hits.true_hits += 1;
        } else {
            hits.false_hits += 1;
        }
    }

    pub(crate) fn record_debug_snapshot(
        &mut self,
        snapshot: DebugSnapshot,
        retained_bytes: usize,
        truncated: bool,
    ) {
        debug_assert_eq!(self.policy, InstrumentationPolicy::Debugger);
        debug_assert!(self.snapshots.len() < MAX_DEBUG_SNAPSHOTS);
        let retained_debug_bytes = self.retained_debug_bytes.saturating_add(retained_bytes);
        if retained_debug_bytes > MAX_DEBUG_RETAINED_BYTES {
            self.debug_trace_truncated = true;
            return;
        }
        self.retained_debug_bytes = retained_debug_bytes;
        self.debug_trace_truncated |= truncated;
        self.snapshots.push(snapshot);
    }

    pub(crate) fn take_trace(&mut self) -> ExecutionTrace {
        std::mem::take(&mut self.trace)
    }

    pub(crate) fn take_debug_trace(&mut self) -> (Vec<DebugSnapshot>, DebugTraceStatus) {
        let snapshots = std::mem::take(&mut self.snapshots)
            .into_boxed_slice()
            .into_vec();
        (
            snapshots,
            DebugTraceStatus {
                retained_bytes: self.retained_debug_bytes,
                truncated: self.debug_trace_truncated,
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn policy(&self) -> InstrumentationPolicy {
        self.policy
    }

    #[cfg(test)]
    pub(crate) fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    #[cfg(test)]
    pub(crate) fn trace(&self) -> &ExecutionTrace {
        &self.trace
    }
}

pub(crate) struct DebugSnapshotBuilder {
    snapshot: DebugSnapshot,
    retained_byte_budget: usize,
    retained_bytes: usize,
    truncated: bool,
}

impl DebugSnapshotBuilder {
    pub(crate) fn new(
        span: Span,
        transaction_event_count: usize,
        retained_byte_budget: usize,
    ) -> Self {
        debug_assert!(retained_byte_budget >= size_of::<DebugSnapshot>());
        Self {
            snapshot: DebugSnapshot {
                span,
                frames: Vec::new(),
                variables: Vec::new(),
                transaction_event_count,
            },
            retained_byte_budget,
            retained_bytes: size_of::<DebugSnapshot>(),
            truncated: false,
        }
    }

    pub(crate) fn remaining_variable_slots(&self) -> usize {
        MAX_DEBUG_VARIABLES_PER_SNAPSHOT.saturating_sub(self.snapshot.variables.len())
    }

    pub(crate) fn can_push_variable(&self) -> bool {
        self.snapshot.variables.len() < MAX_DEBUG_VARIABLES_PER_SNAPSHOT
            && self
                .retained_bytes
                .saturating_add(size_of::<DebugVariable>())
                <= self.retained_byte_budget
    }

    pub(crate) fn can_push_frame(&self) -> bool {
        self.snapshot.frames.len() < MAX_DEBUG_FRAMES_PER_SNAPSHOT
            && self.retained_bytes.saturating_add(size_of::<DebugFrame>())
                <= self.retained_byte_budget
    }

    pub(crate) fn mark_truncated(&mut self) {
        self.truncated = true;
    }

    pub(crate) fn push_variable(&mut self, name: String, type_name: String, value: String) -> bool {
        if self.snapshot.variables.len() >= MAX_DEBUG_VARIABLES_PER_SNAPSHOT
            || !self.reserve_structure::<DebugVariable>()
        {
            self.truncated = true;
            return false;
        }
        let name = self.retain_text(name, MAX_DEBUG_METADATA_BYTES);
        let type_name = self.retain_text(type_name, MAX_DEBUG_METADATA_BYTES);
        let value = self.retain_text(value, MAX_DEBUG_RENDERED_VALUE_BYTES);
        self.snapshot.variables.push(DebugVariable {
            name,
            type_name,
            value,
        });
        true
    }

    pub(crate) fn push_frame(&mut self, name: String, span: Span) -> bool {
        if self.snapshot.frames.len() >= MAX_DEBUG_FRAMES_PER_SNAPSHOT
            || !self.reserve_structure::<DebugFrame>()
        {
            self.truncated = true;
            return false;
        }
        let name = self.retain_text(name, MAX_DEBUG_METADATA_BYTES);
        self.snapshot.frames.push(DebugFrame { name, span });
        true
    }

    pub(crate) fn finish(mut self) -> (DebugSnapshot, usize, bool) {
        self.snapshot.frames = std::mem::take(&mut self.snapshot.frames)
            .into_boxed_slice()
            .into_vec();
        self.snapshot.variables = std::mem::take(&mut self.snapshot.variables)
            .into_boxed_slice()
            .into_vec();
        (self.snapshot, self.retained_bytes, self.truncated)
    }

    fn reserve_structure<T>(&mut self) -> bool {
        let bytes = size_of::<T>();
        let retained_bytes = self.retained_bytes.saturating_add(bytes);
        if retained_bytes > self.retained_byte_budget {
            return false;
        }
        self.retained_bytes = retained_bytes;
        true
    }

    fn retain_text(&mut self, mut text: String, per_text_limit: usize) -> String {
        let remaining = self
            .retained_byte_budget
            .saturating_sub(self.retained_bytes);
        let limit = remaining.min(per_text_limit);
        if text.len() > limit {
            self.truncated = true;
            truncate_utf8(&mut text, limit);
        }
        self.retained_bytes = self.retained_bytes.saturating_add(text.len());
        text.into_boxed_str().into_string()
    }
}

fn truncate_utf8(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    let marker_bytes = TRUNCATION_MARKER.len();
    if max_bytes < marker_bytes {
        let mut end = max_bytes;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        return;
    }
    let mut end = max_bytes - marker_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str(TRUNCATION_MARKER);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_builder_bounds_structure_and_owned_text() {
        let retained_byte_budget = size_of::<DebugSnapshot>() + 4_096;
        let mut builder = DebugSnapshotBuilder::new(Span::new(0, 1), 0, retained_byte_budget);
        assert!(builder.push_frame("f".repeat(1_000), Span::new(0, 1)));
        assert!(builder.push_variable(
            "value".repeat(100),
            "String".repeat(100),
            "x".repeat(MAX_DEBUG_RENDERED_VALUE_BYTES + 1)
        ));
        let (snapshot, retained_bytes, truncated) = builder.finish();

        assert!(retained_bytes <= retained_byte_budget);
        assert!(snapshot.frames[0].name.len() <= MAX_DEBUG_METADATA_BYTES);
        assert!(snapshot.variables[0].name.len() <= MAX_DEBUG_METADATA_BYTES);
        assert!(snapshot.variables[0].type_name.len() <= MAX_DEBUG_METADATA_BYTES);
        assert!(snapshot.variables[0].value.len() <= MAX_DEBUG_RENDERED_VALUE_BYTES);
        assert_eq!(
            snapshot.variables[0].value.capacity(),
            snapshot.variables[0].value.len()
        );
        assert!(truncated);
    }

    #[test]
    fn snapshot_builder_bounds_frame_and_variable_counts() {
        let mut builder = DebugSnapshotBuilder::new(Span::new(0, 1), 0, MAX_DEBUG_RETAINED_BYTES);
        for index in 0..=MAX_DEBUG_FRAMES_PER_SNAPSHOT {
            builder.push_frame(format!("frame{index}"), Span::new(index, index + 1));
        }
        for index in 0..=MAX_DEBUG_VARIABLES_PER_SNAPSHOT {
            builder.push_variable(
                format!("value{index}"),
                "Integer".to_owned(),
                index.to_string(),
            );
        }
        let (snapshot, retained_bytes, truncated) = builder.finish();

        assert_eq!(snapshot.frames.len(), MAX_DEBUG_FRAMES_PER_SNAPSHOT);
        assert_eq!(snapshot.variables.len(), MAX_DEBUG_VARIABLES_PER_SNAPSHOT);
        assert!(retained_bytes <= MAX_DEBUG_RETAINED_BYTES);
        assert!(truncated);
    }

    #[test]
    fn utf8_truncation_never_splits_a_scalar() {
        let mut text = "ab😀cd".to_owned();
        truncate_utf8(&mut text, 5);
        assert_eq!(text, "ab…");
        assert!(text.is_char_boundary(text.len()));
    }
}
