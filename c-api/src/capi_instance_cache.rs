use std::slice;

use meta::capi_safe_unwind;
use multiversx_chain_vm_executor::CompilationOptionsLegacy;

use crate::{
    capi_executor::vm_exec_executor_t,
    capi_instance::{CapiInstance, vm_exec_compilation_options_t, vm_exec_instance_t},
    handle_registry,
    service_singleton::with_service,
    vm_exec_result_t,
};

/// Frees a cache buffer that was previously produced by
/// [`vm_exec_instance_cache`].
///
/// The cache function constructs a Rust `Vec<u8>` and hands its raw
/// pointer + length to the C caller via `mem::forget`. Today the Go
/// caller frees it with `C.free`, which only works because Rust's
/// default `GlobalAlloc` happens to be the same system malloc that
/// `libc::free` understands. Any future `#[global_allocator]` switch
/// (jemallocator, mimalloc, custom) makes that pairing undefined
/// behavior because the chunk metadata is allocator-specific.
///
/// This export reclaims the Vec back through the SAME allocator that
/// produced it, regardless of which global allocator is configured.
/// The Go side should call this instead of `C.free` once the rebuilt
/// `.so/.dylib` files include the symbol. See issues/ISSUE-009.
///
/// # Safety
///
/// `ptr` must be the exact pointer (and `len` the exact length) that
/// `vm_exec_instance_cache` wrote into its out-params; otherwise
/// `Vec::from_raw_parts` produces UB.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vm_exec_cache_free(ptr: *mut u8, len: u32) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let len = len as usize;
    // SAFETY: caller must pass back the exact (ptr, len) tuple produced
    // by vm_exec_instance_cache's mem::forget. The cache Vec is built
    // with `bytes.len() == bytes.capacity()` (it's a freshly-allocated
    // result, not a partially-filled buffer), so reusing `len` for both
    // size and capacity here is correct.
    drop(unsafe { Vec::from_raw_parts(ptr, len, len) });
}

/// Defence-in-depth cap on compiled-cache length accepted at the FFI.
/// Compiled cache is typically 4-8× the source size; 32 MiB gives
/// generous headroom over the 8 MiB source cap (see capi_instance.rs
/// MAX_WASM_BYTES_LEN). See issues/ISSUE-024.
const MAX_CACHE_BYTES_LEN: u32 = 32 * 1024 * 1024;

/// Caches an instance.
///
/// # Safety
///
/// C API function, works with raw object pointers.
#[allow(clippy::cast_ptr_alignment)]
#[unsafe(no_mangle)]
#[capi_safe_unwind(vm_exec_result_t::VM_EXEC_ERROR)]
pub unsafe extern "C" fn vm_exec_instance_cache(
    instance_ptr: *const vm_exec_instance_t,
    cache_bytes_ptr: *mut *const u8,
    cache_bytes_len: *mut u32,
) -> vm_exec_result_t {
    // ISSUE-001 closure: was previously `cast_input_const_ptr!` which
    // dereferenced the raw pointer as `*const CapiInstance` — segfaults
    // under the registry-backed model where the "pointer" is an opaque
    // u64 ID (typically a small integer like 0x8). Route through the
    // typed handle registry so the ID is looked up against the live
    // instance map, never touched as memory.
    let capi_instance = cast_capi_instance_ptr!(instance_ptr);

    // ISSUE-006: validate output parameters before writing through them.
    // Without these checks a Go-side bug passing null out-params would
    // segfault inside the cache() success path.
    return_if_ptr_null!(cache_bytes_ptr, "cache_bytes_ptr out-param is null");
    return_if_ptr_null!(cache_bytes_len, "cache_bytes_len out-param is null");

    let _operation_guard = capi_instance.enter_operation();
    let result = capi_instance.content.cache();
    match result {
        Ok(bytes) => {
            unsafe {
                *cache_bytes_ptr = bytes.as_ptr();
                *cache_bytes_len = bytes.len() as u32;
            }
            std::mem::forget(bytes);
            vm_exec_result_t::VM_EXEC_OK
        }
        Err(message) => {
            with_service(|service| service.update_last_error_str(message));
            vm_exec_result_t::VM_EXEC_ERROR
        }
    }
}

/// Creates a new VM executor instance from cache.
///
/// All of the context comes from the provided VM executor.
///
/// # Safety
///
/// C API function, works with raw object pointers.
#[allow(clippy::cast_ptr_alignment, unused_variables)]
#[unsafe(no_mangle)]
#[capi_safe_unwind(vm_exec_result_t::VM_EXEC_ERROR)]
pub unsafe extern "C" fn vm_exec_instance_from_cache(
    executor_ptr: *mut vm_exec_executor_t,
    instance_ptr_ptr: *mut *mut vm_exec_instance_t,
    cache_bytes_ptr: *mut u8,
    cache_bytes_len: u32,
    options_ptr: *const vm_exec_compilation_options_t,
) -> vm_exec_result_t {
    let capi_executor = cast_capi_executor_ptr!(executor_ptr);

    // ISSUE-040: symmetric with vm_exec_new_instance; validate the
    // returned-instance out-param before any possible write to it.
    return_if_ptr_null!(instance_ptr_ptr, "instance out-param is null");

    if cache_bytes_ptr.is_null() {
        with_service(|service| {
            service.update_last_error_str("cache bytes ptr is null".to_string())
        });
        return vm_exec_result_t::VM_EXEC_ERROR;
    }

    if cache_bytes_len > MAX_CACHE_BYTES_LEN {
        with_service(|service| {
            service.update_last_error_str(format!(
                "cache bytes length {cache_bytes_len} exceeds maximum {MAX_CACHE_BYTES_LEN}"
            ))
        });
        return vm_exec_result_t::VM_EXEC_ERROR;
    }

    let cache_bytes: &[u8] =
        unsafe { slice::from_raw_parts(cache_bytes_ptr, cache_bytes_len as usize) };
    // ISSUE-005 (post-validation): null + alignment via the const-ptr cast macro.
    let compilation_options: &CompilationOptionsLegacy = cast_input_const_ptr!(
        options_ptr,
        CompilationOptionsLegacy,
        "compilation options ptr is null"
    );
    // ISSUE-001 closure: lock the executor's content Mutex for the
    // new_instance_from_cache call (the trait method is &self but
    // Mutex provides serial access regardless).
    let instance_result = {
        let content_guard = capi_executor
            .content
            .lock()
            .expect("CapiExecutor.content mutex poisoned");
        content_guard.new_instance_from_cache(cache_bytes, compilation_options)
    };
    match instance_result {
        Ok(instance_box) => {
            let capi_instance = CapiInstance::new(instance_box);
            // ISSUE-001 closure: register in the typed handle registry,
            // reinterpret the u64 ID as *mut vm_exec_instance_t for FFI.
            let id = handle_registry::register_instance(capi_instance);
            let raw = id as *mut vm_exec_instance_t;
            unsafe {
                *instance_ptr_ptr = raw;
            }
            vm_exec_result_t::VM_EXEC_OK
        }
        Err(message) => {
            with_service(|service| service.update_last_error_str(message.to_string()));
            vm_exec_result_t::VM_EXEC_ERROR
        }
    }
}
