#[macro_use]
mod macros;

mod basic_types;
pub mod capi_breakpoints;
pub mod capi_error;
pub mod capi_executor;
pub mod capi_instance;
pub mod capi_instance_cache;
pub mod capi_logger;
mod capi_mem_conversion;
pub mod capi_memory;
pub mod capi_metering;
pub mod capi_vm_hook_pointers;
pub mod capi_vm_hooks;
mod capi_vm_hooks_thread_safety;
pub mod handle_registry;
pub mod service_singleton;
pub mod wasmer_logger;

pub use basic_types::*;
pub use wasmer_logger::{init, set_log_level, u64_to_log_level};

/// ABI version of the VM executor C API exposed by this crate. Increment
/// on every breaking change to a `#[no_mangle] pub unsafe extern "C"`
/// signature, struct layout, or semantic contract that crosses the FFI
/// boundary into mx-chain-vm-go (or any other C-API consumer).
///
/// Consumers should fetch this at process init via [`vm_exec_api_version`]
/// and refuse to start on mismatch. See issues/ISSUE-020.
///
/// History:
///   v1 — baseline (May 2026 audit pass).
pub const VM_EXEC_API_VERSION: u32 = 1;

/// Returns the ABI version of the linked vm-executor C API. Consumers
/// must compare this against the version they were built against and
/// refuse to operate on mismatch. See issues/ISSUE-020.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vm_exec_api_version() -> u32 {
    VM_EXEC_API_VERSION
}
