//! Instantiate a module, call functions, and read exports.

use crate::{
    capi_vm_hook_pointers::vm_exec_vm_hook_c_func_pointers, capi_vm_hooks::CapiVMHooks,
    handle_registry, service_singleton::with_service, vm_exec_result_t,
};
use libc::c_void;
use meta::capi_safe_unwind;
use multiversx_chain_vm_executor::ExecutorLegacy;
use multiversx_chain_vm_executor_wasmer::force_sighandler_reinstall;
use std::sync::Mutex;

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct vm_exec_executor_t;

pub struct CapiExecutor {
    /// Inner executor under a Mutex.
    ///
    /// ISSUE-001 closure: `ExecutorLegacy` has both `&mut self` methods
    /// (set_vm_hooks_ptr, set_opcode_cost — infrequent, executor-setup
    /// time) and `&self` methods (new_instance, new_instance_from_cache).
    /// Storing the trait object behind `Arc<CapiExecutor>` in the handle
    /// registry means we can't get a `&mut Box<dyn ExecutorLegacy>` from
    /// the Arc; wrapping in `Mutex` gives us that mutable access via
    /// `lock()`. The Mutex also serializes access to the wasmer impl's
    /// internal `RefCell` state (which is `!Sync` on its own), making
    /// the `unsafe impl Send + Sync for CapiExecutor` below sound.
    pub content: Mutex<Box<dyn ExecutorLegacy>>,
}

// ISSUE-001 closure: required by Arc<CapiExecutor> in handle_registry.
// See the equivalent unsafe impl on CapiInstance for the full rationale.
// In short: the previous raw-pointer C-API was already de-facto
// thread-safe; this makes the pre-existing assertion type-system-visible.
unsafe impl Send for CapiExecutor {}
unsafe impl Sync for CapiExecutor {}

/// Creates a new VM executor.
///
/// # Safety
///
/// C API function, works with raw object pointers.
#[allow(clippy::cast_ptr_alignment)]
#[unsafe(no_mangle)]
#[capi_safe_unwind(vm_exec_result_t::VM_EXEC_ERROR)]
pub unsafe extern "C" fn vm_exec_new_executor(
    executor: *mut *mut vm_exec_executor_t,
    vm_hook_pointers_ptr_ptr: *mut *mut vm_exec_vm_hook_c_func_pointers,
) -> vm_exec_result_t {
    return_if_ptr_null!(vm_hook_pointers_ptr_ptr, "VM hooks ptr is null");

    // unpacking the vm hooks object pointer
    let vm_hook_pointers_ptr = unsafe { *vm_hook_pointers_ptr_ptr };
    return_if_ptr_null!(vm_hook_pointers_ptr, "VM hooks inner ptr is null");
    let vm_hook_pointers = unsafe { (*vm_hook_pointers_ptr).clone() };

    // create executor
    let executor_result =
        with_service(|service| service.new_executor(Box::new(CapiVMHooks::new(vm_hook_pointers))));
    match executor_result {
        Ok(executor_box) => {
            let capi_executor = CapiExecutor {
                content: Mutex::new(executor_box),
            };
            // ISSUE-001 closure: register the executor in the typed
            // handle registry. The returned u64 ID is reinterpreted as
            // a `*mut vm_exec_executor_t` so the FFI wire format
            // doesn't change. Subsequent C-API calls cast the "pointer"
            // back to u64 and look up the Arc<CapiExecutor>.
            let id = handle_registry::register_executor(capi_executor);
            let raw = id as *mut vm_exec_executor_t;
            unsafe {
                *executor = raw;
            }
            vm_exec_result_t::VM_EXEC_OK
        }
        Err(message) => {
            with_service(|service| service.update_last_error_str(message.to_string()));
            vm_exec_result_t::VM_EXEC_ERROR
        }
    }
}

/// Forces reinstalling the sighandlers.
///
/// # Safety
///
/// C API function, works with raw object pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vm_force_sighandler_reinstall() {
    force_sighandler_reinstall();
}

/// Sets the data that can be hold by an instance context.
///
/// An instance context (represented by the opaque
/// `wasmer_instance_context_t` structure) can hold user-defined
/// data. This function sets the data. This function is complementary
/// of `wasmer_instance_context_data_get()`.
///
/// This function does nothing if `instance` is a null pointer.
///
/// # Safety
///
/// C API function, works with raw object pointers.
#[allow(clippy::cast_ptr_alignment)]
#[unsafe(no_mangle)]
#[capi_safe_unwind(vm_exec_result_t::VM_EXEC_ERROR)]
pub unsafe extern "C" fn vm_exec_executor_set_vm_hooks_ptr(
    executor_ptr: *mut vm_exec_executor_t,
    vm_hooks_ptr: *mut c_void,
) -> vm_exec_result_t {
    let result = std::panic::catch_unwind(|| {
        let capi_executor = cast_capi_executor_ptr!(executor_ptr);

        // ISSUE-001 closure: content is now Mutex<Box<dyn ExecutorLegacy>>;
        // lock to get &mut for the trait's &mut self method.
        let mut content_guard = capi_executor
            .content
            .lock()
            .expect("CapiExecutor.content mutex poisoned");
        let result = content_guard.set_vm_hooks_ptr(vm_hooks_ptr);
        match result {
            Ok(()) => vm_exec_result_t::VM_EXEC_OK,
            Err(message) => {
                with_service(|service| service.update_last_error_str(message.to_string()));
                vm_exec_result_t::VM_EXEC_ERROR
            }
        }
    });

    match result {
        Ok(result) => result,
        Err(_) => vm_exec_result_t::VM_EXEC_ERROR,
    }
}

/// Destroys a VM executor object. Safe under concurrent and repeated
/// invocation because the incoming "pointer" is an opaque handle ID and
/// the typed registry removes it at most once.
///
/// # Safety
///
/// C API function, works with raw object pointers.
#[allow(clippy::cast_ptr_alignment)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vm_exec_executor_destroy(executor_ptr: *mut vm_exec_executor_t) {
    if executor_ptr.is_null() {
        return;
    }
    // ISSUE-001 closure: the "pointer" is now an opaque ID into the
    // typed handle registry. Reading at the address would be wild —
    // the address might be 1, 2, 3 (small-integer IDs encoded as
    // pointer values). Skip every prior raw-pointer-deref step and route through
    // `handle_registry::destroy_executor` instead.
    //
    // What the registry call does:
    //   - id = 0 (null already short-circuited above) → no-op return false
    //   - id never issued → no-op return false (safe double-destroy
    //     of a stale ID; cannot deref freed memory because there was
    //     no allocation at that address to begin with)
    //   - id present → HashMap::remove drops the registry's strong Arc
    //     reference. The CapiExecutor is freed when the LAST Arc drops,
    //     which is "immediately" if no in-flight caller holds one, or
    //     "when the last in-flight caller's call returns" otherwise.
    //     Either way: no UAF possible, no double-free possible.
    let id = executor_ptr as u64;
    let _removed = handle_registry::destroy_executor(id);
    // We deliberately don't surface the boolean. C-API destroy is
    // documented as best-effort; a redundant destroy of an unknown ID
    // is silent. Logging here would be noise on shutdown paths.
}
