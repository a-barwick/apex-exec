use super::{
    Collection, CollectionId, Interpreter, ObjectId, PlatformHost, PlatformValue, SObjectId, Value,
};
use std::collections::HashSet;

pub(super) const MAX_VALUE_GRAPH_DEPTH: usize = 64;
pub(super) const MAX_VALUE_GRAPH_NODES: usize = 4_096;
pub(super) const MAX_VALUE_GRAPH_ELEMENTS: usize = 4_096;
pub(super) const MAX_VALUE_DISPLAY_BYTES: usize = 16 * 1024;

const CYCLE_MARKER: &str = "<cycle>";
const TRUNCATION_MARKER: &str = "…";

#[derive(Clone, Copy)]
struct TraversalLimits {
    depth: usize,
    nodes: usize,
    elements: usize,
    output_bytes: usize,
}

impl TraversalLimits {
    const fn bounded(output_bytes: usize) -> Self {
        Self {
            depth: MAX_VALUE_GRAPH_DEPTH,
            nodes: MAX_VALUE_GRAPH_NODES,
            elements: MAX_VALUE_GRAPH_ELEMENTS,
            output_bytes,
        }
    }

    const fn equality() -> Self {
        Self {
            depth: usize::MAX,
            nodes: usize::MAX,
            elements: usize::MAX,
            output_bytes: usize::MAX,
        }
    }

    const fn semantic() -> Self {
        Self::bounded(usize::MAX)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TraversalError {
    Cycle,
    DepthLimit,
    NodeLimit,
    ElementLimit,
    OutputLimit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TraversalStats {
    pub(super) nodes: usize,
    pub(super) elements: usize,
    pub(super) max_depth: usize,
    pub(super) output_bytes: usize,
    pub(super) equality_comparisons: usize,
    pub(super) equality_pairs: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum GraphIdentity {
    Collection(CollectionId),
    Object(ObjectId),
    SObject(SObjectId),
}

/// Shared state for every traversal over the runtime value graph.
///
/// Display and JSON use the identity path plus fixed depth, node, element, and
/// output limits. Equality uses the same state as a coinductive visited-pair
/// trail; its explicit work stack avoids consuming the host call stack.
pub(super) struct ValueGraphTraversal {
    limits: TraversalLimits,
    active: HashSet<GraphIdentity>,
    equality_pairs: HashSet<(CollectionId, CollectionId)>,
    equality_trail: Vec<(CollectionId, CollectionId)>,
    stats: TraversalStats,
    truncated: bool,
}

impl ValueGraphTraversal {
    pub(super) fn for_display() -> Self {
        Self::new(TraversalLimits::bounded(MAX_VALUE_DISPLAY_BYTES))
    }

    pub(super) fn for_json() -> Self {
        Self::new(TraversalLimits::bounded(usize::MAX))
    }

    fn for_semantic_string() -> Self {
        Self::new(TraversalLimits::semantic())
    }

    fn for_equality() -> Self {
        Self::new(TraversalLimits::equality())
    }

    fn new(limits: TraversalLimits) -> Self {
        Self {
            limits,
            active: HashSet::new(),
            equality_pairs: HashSet::new(),
            equality_trail: Vec::new(),
            stats: TraversalStats::default(),
            truncated: false,
        }
    }

    pub(super) fn visit_node(&mut self, depth: usize) -> Result<(), TraversalError> {
        if depth > self.limits.depth {
            return Err(TraversalError::DepthLimit);
        }
        if self.stats.nodes >= self.limits.nodes {
            return Err(TraversalError::NodeLimit);
        }
        self.stats.nodes += 1;
        self.stats.max_depth = self.stats.max_depth.max(depth);
        Ok(())
    }

    pub(super) fn visit_element(&mut self) -> Result<(), TraversalError> {
        if self.stats.elements >= self.limits.elements {
            return Err(TraversalError::ElementLimit);
        }
        self.stats.elements += 1;
        Ok(())
    }

    pub(super) fn enter_identity(&mut self, identity: GraphIdentity) -> Result<(), TraversalError> {
        if self.active.insert(identity) {
            Ok(())
        } else {
            Err(TraversalError::Cycle)
        }
    }

    pub(super) fn leave_identity(&mut self, identity: GraphIdentity) {
        debug_assert!(self.active.remove(&identity));
    }

    pub(super) fn write(&mut self, output: &mut String, text: &str) -> Result<(), TraversalError> {
        let remaining = self
            .limits
            .output_bytes
            .saturating_sub(self.stats.output_bytes);
        if text.len() <= remaining {
            output.push_str(text);
            self.stats.output_bytes = self.stats.output_bytes.saturating_add(text.len());
            return Ok(());
        }
        self.write_prefix_with_marker(output, text, remaining);
        self.truncated = true;
        Err(TraversalError::OutputLimit)
    }

    pub(super) fn write_truncation_marker(&mut self, output: &mut String) {
        if self.truncated {
            return;
        }
        let remaining = self
            .limits
            .output_bytes
            .saturating_sub(self.stats.output_bytes);
        self.write_prefix_with_marker(output, "", remaining);
        self.truncated = true;
    }

    pub(super) fn is_truncated(&self) -> bool {
        self.truncated
    }

    pub(super) fn stats(&self) -> TraversalStats {
        self.stats
    }

    fn write_prefix_with_marker(&mut self, output: &mut String, text: &str, remaining: usize) {
        if remaining < TRUNCATION_MARKER.len() {
            let end = utf8_prefix_len(text, remaining);
            output.push_str(&text[..end]);
            self.stats.output_bytes = self.stats.output_bytes.saturating_add(end);
            return;
        }
        let prefix_limit = remaining - TRUNCATION_MARKER.len();
        let end = utf8_prefix_len(text, prefix_limit);
        output.push_str(&text[..end]);
        output.push_str(TRUNCATION_MARKER);
        self.stats.output_bytes = self
            .stats
            .output_bytes
            .saturating_add(end + TRUNCATION_MARKER.len());
    }

    fn note_equality_comparison(&mut self) {
        self.stats.equality_comparisons += 1;
    }

    fn equality_checkpoint(&self) -> usize {
        self.equality_trail.len()
    }

    fn visit_equality_pair(&mut self, pair: (CollectionId, CollectionId)) -> bool {
        if !self.equality_pairs.insert(pair) {
            return false;
        }
        self.equality_trail.push(pair);
        self.stats.equality_pairs += 1;
        true
    }

    fn rollback_equality(&mut self, checkpoint: usize) {
        while self.equality_trail.len() > checkpoint {
            let pair = self.equality_trail.pop().expect("length was checked above");
            self.equality_pairs.remove(&pair);
        }
    }
}

fn utf8_prefix_len(text: &str, limit: usize) -> usize {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CycleBehavior {
    Mark,
    Error,
}

pub(super) struct RenderedValue {
    pub(super) text: String,
    #[cfg(test)]
    pub(super) stats: TraversalStats,
    pub(super) truncated: bool,
}

impl<'program, H: PlatformHost> Interpreter<'program, H> {
    /// Converts a value into observable Apex/API String content.
    ///
    /// Structural traversal remains cycle-safe and bounded by depth, node, and
    /// element counts, but already-materialized String bytes are not silently
    /// constrained by the debugger's presentation limit.
    pub(super) fn stringify_value(&self, value: &Value) -> String {
        let mut traversal = ValueGraphTraversal::for_semantic_string();
        let mut text = String::new();
        self.render_value_with_traversal(value, 0, &mut traversal, &mut text, CycleBehavior::Mark)
            .expect("semantic String traversal converts every bound into a marker");
        text
    }

    pub(super) fn render_value(&self, value: &Value) -> RenderedValue {
        let mut traversal = ValueGraphTraversal::for_display();
        let mut text = String::new();
        self.render_value_with_traversal(value, 0, &mut traversal, &mut text, CycleBehavior::Mark)
            .expect("display traversal converts every bound into a marker");
        RenderedValue {
            text,
            #[cfg(test)]
            stats: traversal.stats(),
            truncated: traversal.is_truncated(),
        }
    }

    pub(super) fn render_value_with_traversal(
        &self,
        value: &Value,
        depth: usize,
        traversal: &mut ValueGraphTraversal,
        output: &mut String,
        cycle_behavior: CycleBehavior,
    ) -> Result<(), TraversalError> {
        if !accept_step(
            traversal.visit_node(depth),
            traversal,
            output,
            cycle_behavior,
        )? {
            return Ok(());
        }
        let result = match value {
            Value::Platform(id) => self.render_platform(*id, traversal, output, cycle_behavior),
            Value::Collection(id) => {
                self.render_collection(*id, depth, traversal, output, cycle_behavior)
            }
            Value::Object(id) => self.render_object(*id, traversal, output, cycle_behavior),
            Value::SObject(id) => {
                self.render_sobject(*id, depth, traversal, output, cycle_behavior)
            }
            Value::AggregateResult(id) => {
                self.render_aggregate(*id, traversal, output, cycle_behavior)
            }
            Value::Exception(exception) => render_exception(exception, traversal, output),
            _ => render_leaf(value, traversal, output),
        };
        match result {
            Err(error) if cycle_behavior == CycleBehavior::Mark => {
                handle_output_error(error, traversal, output, cycle_behavior)
            }
            result => result,
        }
    }

    fn render_aggregate(
        &self,
        id: super::AggregateResultId,
        traversal: &mut ValueGraphTraversal,
        output: &mut String,
        cycle_behavior: CycleBehavior,
    ) -> Result<(), TraversalError> {
        traversal.write(output, "AggregateResult:{")?;
        let mut first = true;
        for (name, value) in self.store.aggregate_result(id) {
            if !accept_step(traversal.visit_element(), traversal, output, cycle_behavior)? {
                break;
            }
            write_separator(traversal, output, &mut first)?;
            traversal.write(output, name)?;
            traversal.write(output, "=")?;
            traversal.write(output, &format!("{value:?}"))?;
        }
        finish_container(traversal, output, "}")
    }

    fn render_platform(
        &self,
        id: super::PlatformValueId,
        traversal: &mut ValueGraphTraversal,
        output: &mut String,
        cycle_behavior: CycleBehavior,
    ) -> Result<(), TraversalError> {
        match self.store.platform(id) {
            PlatformValue::Blob(bytes) => {
                traversal.write(output, "Blob[")?;
                traversal.write(output, &bytes.len().to_string())?;
                traversal.write(output, "]")
            }
            PlatformValue::Pattern(pattern) => traversal.write(output, pattern),
            PlatformValue::Matcher { .. } => traversal.write(output, "Matcher"),
            PlatformValue::Http => traversal.write(output, "Http"),
            PlatformValue::HttpRequest(request) => {
                traversal.write(output, "HttpRequest[")?;
                traversal.write(output, &request.method)?;
                traversal.write(output, " ")?;
                traversal.write(output, &request.endpoint)?;
                traversal.write(output, "]")
            }
            PlatformValue::HttpResponse(response) => {
                traversal.write(output, "HttpResponse[")?;
                traversal.write(output, &response.status_code.to_string())?;
                traversal.write(output, " ")?;
                traversal.write(output, &response.status)?;
                traversal.write(output, "]")
            }
            PlatformValue::SObjectType(object_id) | PlatformValue::DescribeSObject(object_id) => {
                let name = self
                    .program()
                    .schema()
                    .object_at(*object_id)
                    .map_or("Schema", |object| object.api_name());
                traversal.write(output, name)
            }
            PlatformValue::AsyncContext { ty, job_id } => {
                traversal.write(output, &ty.apex_name())?;
                traversal.write(output, "[")?;
                traversal.write(output, job_id)?;
                traversal.write(output, "]")
            }
        }
        .or_else(|error| handle_output_error(error, traversal, output, cycle_behavior))
    }

    fn render_collection(
        &self,
        id: CollectionId,
        depth: usize,
        traversal: &mut ValueGraphTraversal,
        output: &mut String,
        cycle_behavior: CycleBehavior,
    ) -> Result<(), TraversalError> {
        let identity = GraphIdentity::Collection(id);
        if !enter_identity(identity, traversal, output, cycle_behavior)? {
            return Ok(());
        }
        let result = match self.collection(id) {
            Collection::List { elements, .. } => {
                self.render_sequence(elements, depth, traversal, output, cycle_behavior, "(", ")")
            }
            Collection::Set { elements, .. } => {
                self.render_sequence(elements, depth, traversal, output, cycle_behavior, "{", "}")
            }
            Collection::Map { entries, .. } => {
                self.render_map(entries, depth, traversal, output, cycle_behavior)
            }
        };
        traversal.leave_identity(identity);
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn render_sequence(
        &self,
        elements: &[Value],
        depth: usize,
        traversal: &mut ValueGraphTraversal,
        output: &mut String,
        cycle_behavior: CycleBehavior,
        opening: &str,
        closing: &str,
    ) -> Result<(), TraversalError> {
        traversal.write(output, opening)?;
        let mut first = true;
        for value in elements {
            if !accept_step(traversal.visit_element(), traversal, output, cycle_behavior)? {
                break;
            }
            write_separator(traversal, output, &mut first)?;
            self.render_value_with_traversal(value, depth + 1, traversal, output, cycle_behavior)?;
            if traversal.is_truncated() {
                break;
            }
        }
        finish_container(traversal, output, closing)
    }

    fn render_map(
        &self,
        entries: &[(Value, Value)],
        depth: usize,
        traversal: &mut ValueGraphTraversal,
        output: &mut String,
        cycle_behavior: CycleBehavior,
    ) -> Result<(), TraversalError> {
        traversal.write(output, "{")?;
        let mut first = true;
        for (key, value) in entries {
            if !accept_step(traversal.visit_element(), traversal, output, cycle_behavior)? {
                break;
            }
            write_separator(traversal, output, &mut first)?;
            self.render_value_with_traversal(key, depth + 1, traversal, output, cycle_behavior)?;
            traversal.write(output, "=")?;
            self.render_value_with_traversal(value, depth + 1, traversal, output, cycle_behavior)?;
            if traversal.is_truncated() {
                break;
            }
        }
        finish_container(traversal, output, "}")
    }

    fn render_object(
        &self,
        id: ObjectId,
        traversal: &mut ValueGraphTraversal,
        output: &mut String,
        cycle_behavior: CycleBehavior,
    ) -> Result<(), TraversalError> {
        let identity = GraphIdentity::Object(id);
        if !enter_identity(identity, traversal, output, cycle_behavior)? {
            return Ok(());
        }
        let instance = self.store.object(id);
        let result = (|| {
            traversal.write(output, &self.classes()[instance.class_id].name.spelling)?;
            traversal.write(output, "@")?;
            traversal.write(output, &id.0.to_string())
        })();
        traversal.leave_identity(identity);
        result
    }

    fn render_sobject(
        &self,
        id: SObjectId,
        depth: usize,
        traversal: &mut ValueGraphTraversal,
        output: &mut String,
        cycle_behavior: CycleBehavior,
    ) -> Result<(), TraversalError> {
        let identity = GraphIdentity::SObject(id);
        if !enter_identity(identity, traversal, output, cycle_behavior)? {
            return Ok(());
        }
        let instance = self.store.sobject(id);
        let object = self
            .program()
            .schema()
            .object_at(instance.object_id)
            .expect("runtime SObject schema index is valid");
        let result = (|| {
            traversal.write(output, object.api_name())?;
            traversal.write(output, ":{")?;
            let mut first = true;
            for (field_id, value) in &instance.fields {
                if !accept_step(traversal.visit_element(), traversal, output, cycle_behavior)? {
                    break;
                }
                let field = object
                    .field_at(*field_id)
                    .expect("runtime SObject field index is valid");
                write_separator(traversal, output, &mut first)?;
                traversal.write(output, field.api_name())?;
                traversal.write(output, "=")?;
                self.render_value_with_traversal(
                    value,
                    depth + 1,
                    traversal,
                    output,
                    cycle_behavior,
                )?;
                if traversal.is_truncated() {
                    break;
                }
            }
            finish_container(traversal, output, "}")
        })();
        traversal.leave_identity(identity);
        result
    }

    pub(super) fn values_equal<'value>(
        &'value self,
        left: &'value Value,
        right: &'value Value,
    ) -> bool {
        EqualityEngine::new(self, left, right).run().0
    }

    pub(super) fn operator_values_equal<'value>(
        &'value self,
        left: &'value Value,
        right: &'value Value,
    ) -> bool {
        match (left, right) {
            (Value::String(left), Value::String(right)) => {
                left.to_lowercase() == right.to_lowercase()
            }
            _ => self.values_equal(left, right),
        }
    }

    #[cfg(test)]
    pub(super) fn values_equal_with_stats<'value>(
        &'value self,
        left: &'value Value,
        right: &'value Value,
    ) -> (bool, TraversalStats) {
        EqualityEngine::new(self, left, right).run()
    }
}

fn render_leaf(
    value: &Value,
    traversal: &mut ValueGraphTraversal,
    output: &mut String,
) -> Result<(), TraversalError> {
    match value {
        Value::String(value) | Value::Id(value) => traversal.write(output, value),
        Value::Boolean(value) => traversal.write(output, &value.to_string()),
        Value::Integer(value) => traversal.write(output, &value.to_string()),
        Value::Decimal(value) => traversal.write(output, &value.normalize().to_string()),
        Value::Date(value) => traversal.write(output, &value.format("%Y-%m-%d").to_string()),
        Value::Datetime(value) => {
            traversal.write(output, &value.format("%Y-%m-%d %H:%M:%S").to_string())
        }
        Value::Time(value) => traversal.write(output, &value.format("%H:%M:%S%.3f").to_string()),
        Value::Null(_) => traversal.write(output, "null"),
        Value::Void => traversal.write(output, "void"),
        _ => unreachable!("non-leaf values are dispatched before leaf rendering"),
    }
}

fn render_exception(
    exception: &crate::diagnostic::Diagnostic,
    traversal: &mut ValueGraphTraversal,
    output: &mut String,
) -> Result<(), TraversalError> {
    let exception_type = exception.exception_type.as_deref().unwrap_or("Exception");
    traversal.write(output, exception_type)?;
    if !exception.message.is_empty() {
        traversal.write(output, ": ")?;
        traversal.write(output, &exception.message)?;
    }
    Ok(())
}

fn accept_step(
    result: Result<(), TraversalError>,
    traversal: &mut ValueGraphTraversal,
    output: &mut String,
    cycle_behavior: CycleBehavior,
) -> Result<bool, TraversalError> {
    match result {
        Ok(()) => Ok(true),
        Err(_error) if cycle_behavior == CycleBehavior::Mark => {
            traversal.write_truncation_marker(output);
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn enter_identity(
    identity: GraphIdentity,
    traversal: &mut ValueGraphTraversal,
    output: &mut String,
    cycle_behavior: CycleBehavior,
) -> Result<bool, TraversalError> {
    match traversal.enter_identity(identity) {
        Ok(()) => Ok(true),
        Err(TraversalError::Cycle) if cycle_behavior == CycleBehavior::Mark => {
            traversal.write(output, CYCLE_MARKER)?;
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn handle_output_error(
    error: TraversalError,
    traversal: &mut ValueGraphTraversal,
    output: &mut String,
    cycle_behavior: CycleBehavior,
) -> Result<(), TraversalError> {
    if cycle_behavior == CycleBehavior::Mark {
        traversal.write_truncation_marker(output);
        Ok(())
    } else {
        Err(error)
    }
}

fn write_separator(
    traversal: &mut ValueGraphTraversal,
    output: &mut String,
    first: &mut bool,
) -> Result<(), TraversalError> {
    if *first {
        *first = false;
        Ok(())
    } else {
        traversal.write(output, ", ")
    }
}

fn finish_container(
    traversal: &mut ValueGraphTraversal,
    output: &mut String,
    closing: &str,
) -> Result<(), TraversalError> {
    if traversal.is_truncated() {
        Ok(())
    } else {
        traversal.write(output, closing)
    }
}

enum EqualityTask<'value> {
    Compare(&'value Value, &'value Value),
    FinishPair {
        checkpoint: usize,
    },
    ListAfter {
        left: &'value [Value],
        right: &'value [Value],
        next: usize,
    },
    SetAfterCandidate {
        left: &'value [Value],
        right: &'value [Value],
        left_index: usize,
        right_index: usize,
    },
    MapAfterKey {
        left: &'value [(Value, Value)],
        right: &'value [(Value, Value)],
        left_index: usize,
        right_index: usize,
        checkpoint: usize,
    },
    MapAfterValue {
        left: &'value [(Value, Value)],
        right: &'value [(Value, Value)],
        left_index: usize,
        right_index: usize,
        checkpoint: usize,
    },
}

struct EqualityEngine<'value, 'program, H> {
    interpreter: &'value Interpreter<'program, H>,
    traversal: ValueGraphTraversal,
    tasks: Vec<EqualityTask<'value>>,
    last: bool,
}

impl<'value, 'program, H: PlatformHost> EqualityEngine<'value, 'program, H> {
    fn new(
        interpreter: &'value Interpreter<'program, H>,
        left: &'value Value,
        right: &'value Value,
    ) -> Self {
        Self {
            interpreter,
            traversal: ValueGraphTraversal::for_equality(),
            tasks: vec![EqualityTask::Compare(left, right)],
            last: false,
        }
    }

    fn run(mut self) -> (bool, TraversalStats) {
        while let Some(task) = self.tasks.pop() {
            match task {
                EqualityTask::Compare(left, right) => self.compare(left, right),
                EqualityTask::FinishPair { checkpoint } => self.finish_pair(checkpoint),
                EqualityTask::ListAfter { left, right, next } => {
                    self.after_list(left, right, next);
                }
                EqualityTask::SetAfterCandidate {
                    left,
                    right,
                    left_index,
                    right_index,
                } => self.after_set_candidate(left, right, left_index, right_index),
                EqualityTask::MapAfterKey {
                    left,
                    right,
                    left_index,
                    right_index,
                    checkpoint,
                } => {
                    self.after_map_key(left, right, left_index, right_index, checkpoint);
                }
                EqualityTask::MapAfterValue {
                    left,
                    right,
                    left_index,
                    right_index,
                    checkpoint,
                } => {
                    self.after_map_value(left, right, left_index, right_index, checkpoint);
                }
            }
        }
        (self.last, self.traversal.stats())
    }

    fn compare(&mut self, left: &'value Value, right: &'value Value) {
        self.traversal.note_equality_comparison();
        self.last = match (left, right) {
            (Value::String(left), Value::String(right)) => left == right,
            (Value::Boolean(left), Value::Boolean(right)) => left == right,
            (Value::Integer(left), Value::Integer(right)) => left == right,
            (Value::Decimal(left), Value::Decimal(right)) => left == right,
            (Value::Date(left), Value::Date(right)) => left == right,
            (Value::Datetime(left), Value::Datetime(right)) => left == right,
            (Value::Time(left), Value::Time(right)) => left == right,
            (Value::Id(left), Value::Id(right)) => left.eq_ignore_ascii_case(right),
            (Value::Platform(left), Value::Platform(right)) => left == right,
            (Value::Collection(left), Value::Collection(right)) => {
                self.start_collection(*left, *right);
                return;
            }
            (Value::Object(left), Value::Object(right)) => left == right,
            (Value::SObject(left), Value::SObject(right)) => left == right,
            (Value::Exception(left), Value::Exception(right)) => left == right,
            (Value::Null(_), Value::Null(_)) => true,
            (Value::Void, Value::Void) => true,
            _ => false,
        };
    }

    fn start_collection(&mut self, left_id: CollectionId, right_id: CollectionId) {
        if left_id == right_id {
            self.last = true;
            return;
        }
        let checkpoint = self.traversal.equality_checkpoint();
        if !self.traversal.visit_equality_pair((left_id, right_id)) {
            self.last = true;
            return;
        }
        let left = self.interpreter.collection(left_id);
        let right = self.interpreter.collection(right_id);
        match (left, right) {
            (
                Collection::List { elements: left, .. },
                Collection::List {
                    elements: right, ..
                },
            ) => self.start_list(left, right, checkpoint),
            (
                Collection::Set { elements: left, .. },
                Collection::Set {
                    elements: right, ..
                },
            ) => self.start_set(left, right, checkpoint),
            (Collection::Map { entries: left, .. }, Collection::Map { entries: right, .. }) => {
                self.start_map(left, right, checkpoint);
            }
            _ => {
                self.traversal.rollback_equality(checkpoint);
                self.last = false;
            }
        }
    }

    fn start_list(&mut self, left: &'value [Value], right: &'value [Value], checkpoint: usize) {
        if left.len() != right.len() {
            self.traversal.rollback_equality(checkpoint);
            self.last = false;
            return;
        }
        self.tasks.push(EqualityTask::FinishPair { checkpoint });
        if left.is_empty() {
            self.last = true;
            return;
        }
        self.tasks.push(EqualityTask::ListAfter {
            left,
            right,
            next: 1,
        });
        self.tasks.push(EqualityTask::Compare(&left[0], &right[0]));
    }

    fn after_list(&mut self, left: &'value [Value], right: &'value [Value], next: usize) {
        if !self.last || next == left.len() {
            return;
        }
        self.tasks.push(EqualityTask::ListAfter {
            left,
            right,
            next: next + 1,
        });
        self.tasks
            .push(EqualityTask::Compare(&left[next], &right[next]));
    }

    fn start_set(&mut self, left: &'value [Value], right: &'value [Value], checkpoint: usize) {
        if left.len() != right.len() {
            self.traversal.rollback_equality(checkpoint);
            self.last = false;
            return;
        }
        self.tasks.push(EqualityTask::FinishPair { checkpoint });
        if left.is_empty() {
            self.last = true;
            return;
        }
        self.schedule_set_candidate(left, right, 0, 0);
    }

    fn schedule_set_candidate(
        &mut self,
        left: &'value [Value],
        right: &'value [Value],
        left_index: usize,
        right_index: usize,
    ) {
        self.tasks.push(EqualityTask::SetAfterCandidate {
            left,
            right,
            left_index,
            right_index,
        });
        self.tasks.push(EqualityTask::Compare(
            &left[left_index],
            &right[right_index],
        ));
    }

    fn after_set_candidate(
        &mut self,
        left: &'value [Value],
        right: &'value [Value],
        left_index: usize,
        right_index: usize,
    ) {
        if self.last {
            if left_index + 1 < left.len() {
                self.schedule_set_candidate(left, right, left_index + 1, 0);
            }
        } else if right_index + 1 < right.len() {
            self.schedule_set_candidate(left, right, left_index, right_index + 1);
        }
    }

    fn start_map(
        &mut self,
        left: &'value [(Value, Value)],
        right: &'value [(Value, Value)],
        checkpoint: usize,
    ) {
        if left.len() != right.len() {
            self.traversal.rollback_equality(checkpoint);
            self.last = false;
            return;
        }
        self.tasks.push(EqualityTask::FinishPair { checkpoint });
        if left.is_empty() {
            self.last = true;
            return;
        }
        self.schedule_map_candidate(left, right, 0, 0);
    }

    fn schedule_map_candidate(
        &mut self,
        left: &'value [(Value, Value)],
        right: &'value [(Value, Value)],
        left_index: usize,
        right_index: usize,
    ) {
        let checkpoint = self.traversal.equality_checkpoint();
        self.tasks.push(EqualityTask::MapAfterKey {
            left,
            right,
            left_index,
            right_index,
            checkpoint,
        });
        self.tasks.push(EqualityTask::Compare(
            &left[left_index].0,
            &right[right_index].0,
        ));
    }

    fn after_map_key(
        &mut self,
        left: &'value [(Value, Value)],
        right: &'value [(Value, Value)],
        left_index: usize,
        right_index: usize,
        checkpoint: usize,
    ) {
        if self.last {
            self.tasks.push(EqualityTask::MapAfterValue {
                left,
                right,
                left_index,
                right_index,
                checkpoint,
            });
            self.tasks.push(EqualityTask::Compare(
                &left[left_index].1,
                &right[right_index].1,
            ));
        } else {
            self.traversal.rollback_equality(checkpoint);
            self.try_next_map_candidate(left, right, left_index, right_index);
        }
    }

    fn after_map_value(
        &mut self,
        left: &'value [(Value, Value)],
        right: &'value [(Value, Value)],
        left_index: usize,
        right_index: usize,
        checkpoint: usize,
    ) {
        if self.last {
            if left_index + 1 < left.len() {
                self.schedule_map_candidate(left, right, left_index + 1, 0);
            }
        } else {
            self.traversal.rollback_equality(checkpoint);
            self.try_next_map_candidate(left, right, left_index, right_index);
        }
    }

    fn try_next_map_candidate(
        &mut self,
        left: &'value [(Value, Value)],
        right: &'value [(Value, Value)],
        left_index: usize,
        right_index: usize,
    ) {
        if right_index + 1 < right.len() {
            self.schedule_map_candidate(left, right, left_index, right_index + 1);
        } else {
            self.last = false;
        }
    }

    fn finish_pair(&mut self, checkpoint: usize) {
        if !self.last {
            self.traversal.rollback_equality(checkpoint);
        }
    }
}
