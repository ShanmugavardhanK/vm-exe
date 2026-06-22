use std::sync::Arc;

use crate::executor_interface::{MemLength, MemPtr, VMHooksLegacy};
use wasmer::WasmerEnv;

/// VMHooksWrapper carries the host VM hooks across the wasmer
/// instance boundary. The wrapper is `Clone` and gets handed to
/// closures wasmer may invoke from any host-call thread.
///
/// Previously the wrapper held `Arc<dyn VMHooksLegacy>` (no Send+Sync
/// trait-object bounds) and asserted `unsafe impl Send/Sync` blanketly.
/// That blanket assertion bypassed the compiler's auto-trait checks
/// for every concrete `VMHooksLegacy` impl ever wrapped — any future
/// impl that introduced a `RefCell` / non-atomic interior mutability
/// would silently break the assertion's promise.
///
/// The trait-object now carries `+ Send + Sync` bounds. The compiler
/// enforces propagation: each concrete `VMHooksLegacy` impl that is
/// placed inside this Arc must independently satisfy Send+Sync.
/// Unsafe assertions, where required, live on the concrete types
/// (visible to reviewers) rather than on the wrapper (invisible at
/// the call site).
#[derive(Clone, Debug)]
pub struct VMHooksWrapper {
    pub vm_hooks: Arc<dyn VMHooksLegacy + Send + Sync>,
}

impl WasmerEnv for VMHooksWrapper {}

impl VMHooksWrapper {
    pub(crate) fn convert_mem_ptr(&self, raw: i32) -> MemPtr {
        raw as MemPtr
    }

    pub(crate) fn convert_mem_length(&self, raw: i32) -> MemLength {
        raw as MemLength
    }
}
