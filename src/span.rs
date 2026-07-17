/// Identity of the source unit that owns a [`Span`].
///
/// Single-source APIs use [`SourceId::ANONYMOUS`]. Project compilation assigns
/// stable IDs to cached paths for the lifetime of one compiler session. These
/// values namespace file-local offsets; they are not persistent artifact IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(usize);

impl SourceId {
    /// Reserved identity used by the public single-source compiler pipeline.
    pub const ANONYMOUS: Self = Self(0);

    /// Creates a caller-assigned source identity.
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the underlying session-local numeric identity.
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Half-open byte range within one source unit.
///
/// `start` and `end` are local to [`Span::source_id`], so identical offsets in
/// different files remain distinct HIR keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    /// Source unit containing this range.
    pub source_id: SourceId,
    /// Inclusive byte offset where the range begins.
    pub start: usize,
    /// Exclusive byte offset where the range ends.
    pub end: usize,
}

impl Span {
    /// Creates a span in the anonymous single-source coordinate space.
    pub const fn new(start: usize, end: usize) -> Self {
        Self::new_in(SourceId::ANONYMOUS, start, end)
    }

    /// Creates a file-local span under an explicit source identity.
    pub const fn new_in(source_id: SourceId, start: usize, end: usize) -> Self {
        Self {
            source_id,
            start,
            end,
        }
    }

    /// Extends this span through the end of another span in the same source.
    ///
    /// # Panics
    ///
    /// Panics when the spans have different source identities.
    pub const fn merge(self, other: Self) -> Self {
        assert!(
            self.source_id.0 == other.source_id.0,
            "cannot merge spans from different sources"
        );
        Self::new_in(self.source_id, self.start, other.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_spans_remain_the_default_public_identity() {
        assert_eq!(Span::new(2, 5).source_id, SourceId::ANONYMOUS);
    }

    #[test]
    fn merging_preserves_source_identity() {
        let source_id = SourceId::new(7);
        assert_eq!(
            Span::new_in(source_id, 2, 5).merge(Span::new_in(source_id, 8, 11)),
            Span::new_in(source_id, 2, 11)
        );
    }
}
