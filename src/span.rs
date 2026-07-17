#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(usize);

impl SourceId {
    pub const ANONYMOUS: Self = Self(0);

    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    pub const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub source_id: SourceId,
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self::new_in(SourceId::ANONYMOUS, start, end)
    }

    pub const fn new_in(source_id: SourceId, start: usize, end: usize) -> Self {
        Self {
            source_id,
            start,
            end,
        }
    }

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
