/// A structured debug event emitted by the Apex `System.debug` intrinsic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugEvent {
    pub message: String,
}

/// Boundary between language execution and platform-owned side effects.
///
/// The initial host surface is deliberately narrow. M7 can extend this
/// boundary with schema and database services without coupling those services
/// to expression evaluation.
pub trait PlatformHost {
    fn debug(&mut self, event: DebugEvent);

    /// Drains debug messages for the existing convenience execution APIs.
    ///
    /// Hosts that stream events elsewhere can keep the default empty result.
    fn take_debug_output(&mut self) -> Vec<String> {
        Vec::new()
    }
}

impl<T: PlatformHost + ?Sized> PlatformHost for &mut T {
    fn debug(&mut self, event: DebugEvent) {
        (**self).debug(event);
    }

    fn take_debug_output(&mut self) -> Vec<String> {
        (**self).take_debug_output()
    }
}

/// Default host used by the public convenience APIs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordingHost {
    output: Vec<String>,
}

impl PlatformHost for RecordingHost {
    fn debug(&mut self, event: DebugEvent) {
        self.output.push(event.message);
    }

    fn take_debug_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.output)
    }
}
