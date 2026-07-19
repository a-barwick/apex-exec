/// Runtime modes that affect observable Apex behavior.
///
/// Instrumentation remains a separate policy: debug mode describes the entry
/// context, while `InstrumentationPolicy` decides which observations to retain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ExecutionContext {
    test: bool,
    debug: bool,
}

impl ExecutionContext {
    pub(super) const fn ordinary() -> Self {
        Self {
            test: false,
            debug: false,
        }
    }

    pub(super) const fn test() -> Self {
        Self {
            test: true,
            debug: false,
        }
    }

    pub(super) const fn debugger() -> Self {
        Self {
            test: false,
            debug: true,
        }
    }

    pub(super) const fn is_test(self) -> bool {
        self.test
    }

    pub(super) const fn is_debug(self) -> bool {
        self.debug
    }

    /// Deterministic queued work inherits the mode active at submission.
    pub(super) const fn for_async_job(self) -> Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionContext;

    #[test]
    fn entry_modes_are_explicit_and_async_inherits_them() {
        let ordinary = ExecutionContext::ordinary();
        assert!(!ordinary.is_test());
        assert!(!ordinary.is_debug());

        let test = ExecutionContext::test();
        assert!(test.is_test());
        assert!(!test.is_debug());
        assert_eq!(test.for_async_job(), test);

        let debugger = ExecutionContext::debugger();
        assert!(!debugger.is_test());
        assert!(debugger.is_debug());
        assert_eq!(debugger.for_async_job(), debugger);
    }
}
