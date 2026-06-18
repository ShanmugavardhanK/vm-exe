use crate::executor_interface::{
    ExecutorError, ExecutorLastError, ExecutorLegacy, ExecutorService, VMHooksLegacy,
};
use log::trace;
use std::sync::{LazyLock, Mutex};

use crate::WasmerExecutor;

static LAST_ERROR: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));

#[derive(Default)]
pub struct BasicExecutorService {
    // Retained for source compatibility with callers that construct the
    // service directly. The synchronized process-wide slot below is the
    // authoritative C-API diagnostic state.
    pub last_error: String,
}

impl BasicExecutorService {
    pub fn new() -> Self {
        Self {
            last_error: String::new(),
        }
    }
}

impl ExecutorLastError for BasicExecutorService {
    fn update_last_error_str(&mut self, err_str: String) {
        set_last_error(err_str);
    }

    fn get_last_error_string(&self) -> String {
        get_last_error_string()
    }
}

impl ExecutorService for BasicExecutorService {
    fn new_executor(
        &self,
        vm_hooks_builder: Box<dyn VMHooksLegacy + Send + Sync>,
    ) -> Result<Box<dyn ExecutorLegacy>, ExecutorError> {
        trace!("Initializing WasmerExecutor ...");
        Ok(Box::new(WasmerExecutor::new(vm_hooks_builder)))
    }
}

pub fn set_last_error(err_str: String) {
    *LAST_ERROR
        .lock()
        .expect("BasicExecutorService.last_error mutex poisoned") = err_str;
}

pub fn get_last_error_string() -> String {
    LAST_ERROR
        .lock()
        .expect("BasicExecutorService.last_error mutex poisoned")
        .clone()
}

pub fn with_last_error<R>(f: impl FnOnce(&str) -> R) -> R {
    let guard = LAST_ERROR
        .lock()
        .expect("BasicExecutorService.last_error mutex poisoned");
    f(&guard)
}
