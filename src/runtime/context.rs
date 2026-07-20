use crate::{compatibility::CompatibilityProfile, hir::ClassSharing, platform::SharingMode};

/// Runtime modes that affect observable Apex behavior.
///
/// Instrumentation remains a separate policy: debug mode describes the entry
/// context, while `InstrumentationPolicy` decides which observations to retain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ExecutionContext {
    test: bool,
    debug: bool,
    profile: CompatibilityProfile,
    sharing: SharingMode,
}

impl ExecutionContext {
    pub(super) fn ordinary() -> Self {
        Self {
            test: false,
            debug: false,
            profile: CompatibilityProfile::default(),
            sharing: SharingMode::WithoutSharing,
        }
    }

    pub(super) fn test() -> Self {
        Self {
            test: true,
            debug: false,
            profile: CompatibilityProfile::default(),
            sharing: SharingMode::WithoutSharing,
        }
    }

    pub(super) fn debugger() -> Self {
        Self {
            test: false,
            debug: true,
            profile: CompatibilityProfile::default(),
            sharing: SharingMode::WithoutSharing,
        }
    }

    pub(super) const fn is_test(self) -> bool {
        self.test
    }

    pub(super) const fn is_debug(self) -> bool {
        self.debug
    }

    pub(super) const fn compatibility_profile(self) -> CompatibilityProfile {
        self.profile
    }

    pub(super) const fn with_compatibility_profile(self, profile: CompatibilityProfile) -> Self {
        Self { profile, ..self }
    }

    pub(super) const fn sharing_mode(self) -> SharingMode {
        self.sharing
    }

    pub(super) const fn for_class(self, sharing: ClassSharing) -> Self {
        let sharing = match sharing {
            ClassSharing::With => SharingMode::WithSharing,
            ClassSharing::Without => SharingMode::WithoutSharing,
            ClassSharing::Inherited | ClassSharing::Omitted => self.sharing,
        };
        Self { sharing, ..self }
    }

    pub(super) const fn for_entry_class(self, sharing: ClassSharing) -> Self {
        let sharing = match sharing {
            ClassSharing::With | ClassSharing::Inherited => SharingMode::WithSharing,
            ClassSharing::Without | ClassSharing::Omitted => SharingMode::WithoutSharing,
        };
        Self { sharing, ..self }
    }

    pub(super) const fn for_trigger(self) -> Self {
        Self {
            sharing: SharingMode::WithoutSharing,
            ..self
        }
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
        assert_eq!(
            ordinary
                .for_class(crate::hir::ClassSharing::With)
                .sharing_mode(),
            crate::platform::SharingMode::WithSharing
        );
        assert_eq!(
            ordinary
                .for_class(crate::hir::ClassSharing::With)
                .for_class(crate::hir::ClassSharing::Inherited)
                .sharing_mode(),
            crate::platform::SharingMode::WithSharing
        );
        assert_eq!(
            ordinary
                .for_entry_class(crate::hir::ClassSharing::Inherited)
                .sharing_mode(),
            crate::platform::SharingMode::WithSharing
        );
        assert_eq!(
            ordinary
                .for_entry_class(crate::hir::ClassSharing::Omitted)
                .sharing_mode(),
            crate::platform::SharingMode::WithoutSharing
        );

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
