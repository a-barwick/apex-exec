use crate::diagnostic::Diagnostic;

pub(super) const MAX_CLASS_INITIALIZATION_DEPTH: usize = 64;

/// State for one class's lazy static storage and field initialization.
#[derive(Clone, Debug, Default)]
pub(super) enum ClassInitializationState {
    #[default]
    Uninitialized,
    Initializing,
    Initialized,
    Failed(Diagnostic),
}
