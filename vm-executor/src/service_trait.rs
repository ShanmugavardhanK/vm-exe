use crate::{ExecutorLegacy, VMHooksLegacy};

pub type ExecutorError = Box<dyn std::error::Error>;

pub trait ExecutorLastError {
    /// Updates the last known error.
    fn update_last_error_str(&mut self, err_str: String);

    /// Returns the last known error.
    fn get_last_error_string(&self) -> String;
}

pub trait ExecutorService: ExecutorLastError {
    /// Creates a new VM executor.
    ///
    /// The `vm_hooks_builder` must be `Send + Sync` because the wasmer
    /// executor stores it in an `Arc<dyn VMHooksLegacy + Send + Sync>`
    /// that wasmer may dispatch host calls against from any thread.
    /// Concrete `VMHooksLegacy` impls that hold non-thread-safe
    /// interior state (RefCell, raw pointers used as opaque handles)
    /// must declare their thread-safety contract explicitly via
    /// `unsafe impl Send/Sync` — visible at the concrete-type site
    /// rather than blanket-asserted on the wrapper.
    fn new_executor(
        &self,
        vm_hooks_builder: Box<dyn VMHooksLegacy + Send + Sync>,
    ) -> Result<Box<dyn ExecutorLegacy>, ExecutorError>;
}
