use crate::diagnostic::Diagnostic;

// Keep the language-level guard comfortably below the native stack budget.
// A static dependency adds several interpreter frames before this counter is
// checked again, so the previous value of 64 was not portable to small test
// threads after M20 added initializer-block dispatch.
pub(super) const MAX_CLASS_INITIALIZATION_DEPTH: usize = 32;

/// State for one class's lazy static storage and field initialization.
#[derive(Clone, Debug, Default)]
pub(super) enum ClassInitializationState {
    #[default]
    Uninitialized,
    Initializing,
    Initialized,
    Failed(Diagnostic),
}
