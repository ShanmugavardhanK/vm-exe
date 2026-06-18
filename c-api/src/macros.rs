macro_rules! return_if_ptr_null {
    ($ptr_var:ident, $err_msg:expr, $err_return_val:expr) => {
        if $ptr_var.is_null() {
            with_service(|service| service.update_last_error_str($err_msg.to_string()));
            return $err_return_val;
        }
    };
    ($ptr_var:ident, $err_msg:expr) => {
        return_if_ptr_null!($ptr_var, $err_msg, vm_exec_result_t::VM_EXEC_ERROR)
    };
}

// `cast_input_ptr!` (mutable variant) was previously used to derive
// `&mut CapiInstance` / `&mut CapiExecutor` from raw FFI pointers under
// the heap-pointer-as-identity model. The handle-registry redesign
// (ISSUE-001 closure) makes it unreachable for those cases — pointers
// are now opaque IDs into the typed registry, looked up via
// `cast_capi_instance_ptr!` / `cast_capi_executor_ptr!` below. The
// macro was removed in this round; the const variant
// (`cast_input_const_ptr!`) is retained because it still services the
// non-handle pointer parameters (`options_ptr`, etc.) that genuinely
// reference C-side structs across the FFI.

/// Look up a `CapiInstance` from a `*mut vm_exec_instance_t` value.
///
/// ISSUE-001 / ISSUE-010 closure. The pointer crossing the FFI is no
/// longer a real heap pointer — it's an opaque u64 ID into the
/// `handle_registry`, reinterpreted as a pointer for FFI wire
/// compatibility. This macro:
///   1. Null-checks the "pointer" (null = ID 0 = never issued).
///   2. Casts it to u64.
///   3. Looks up the ID in the typed instance registry.
///   4. Returns an `Arc<CapiInstance>` on hit, or sets last-error and
///      returns the caller-supplied error code on miss.
///
/// Why this closes the residual UB the previous pointer-liveness
/// defenses couldn't:
///   - IDs are issued from a monotonic AtomicU64 and never reused, so
///     the "same pointer value, different identity after free+realloc"
///     window from the original ISSUE-001 is structurally absent.
///   - The registry has typed maps (`instances` vs `executors`); an
///     instance ID looked up in the executor map returns None — no
///     possibility of treating one as the other (closes ISSUE-010).
///   - Lookup returns an `Arc` clone; even if a concurrent thread
///     destroys the same ID mid-call, the in-flight caller's strong
///     reference keeps the `CapiInstance` alive until the call
///     completes (no UAF possible).
///
/// `handle_registry` is now the single source of truth for C-API object
/// identity and liveness.
macro_rules! cast_capi_instance_ptr {
    ($ptr_var:ident, $err_return_val:expr) => {{
        if $ptr_var.is_null() {
            with_service(|service| {
                service.update_last_error_str("instance ptr is null".to_string())
            });
            return $err_return_val;
        }
        let id = $ptr_var as u64;
        match $crate::handle_registry::lookup_instance(id) {
            Some(arc) => arc,
            None => {
                with_service(|service| {
                    service.update_last_error_str(
                        "instance handle not found in registry (stale or never registered)"
                            .to_string(),
                    )
                });
                return $err_return_val;
            }
        }
    }};
    ($ptr_var:ident) => {
        cast_capi_instance_ptr!($ptr_var, vm_exec_result_t::VM_EXEC_ERROR)
    };
}

/// Look up a `CapiExecutor` from a `*mut vm_exec_executor_t` value.
/// Symmetric to `cast_capi_instance_ptr!` — see that doc-comment for
/// the full ISSUE-001 / ISSUE-010 closure rationale.
macro_rules! cast_capi_executor_ptr {
    ($ptr_var:ident, $err_return_val:expr) => {{
        if $ptr_var.is_null() {
            with_service(|service| {
                service.update_last_error_str("executor ptr is null".to_string())
            });
            return $err_return_val;
        }
        let id = $ptr_var as u64;
        match $crate::handle_registry::lookup_executor(id) {
            Some(arc) => arc,
            None => {
                with_service(|service| {
                    service.update_last_error_str(
                        "executor handle not found in registry (stale or never registered)"
                            .to_string(),
                    )
                });
                return $err_return_val;
            }
        }
    }};
    ($ptr_var:ident) => {
        cast_capi_executor_ptr!($ptr_var, vm_exec_result_t::VM_EXEC_ERROR)
    };
}

macro_rules! cast_input_const_ptr {
    ($ptr_var:ident, $expected_ty:ty, $err_msg:expr, $err_return_val:expr) => {
        if $ptr_var.is_null() {
            with_service(|service| service.update_last_error_str($err_msg.to_string()));
            return $err_return_val;
        } else if ($ptr_var as usize) % std::mem::align_of::<$expected_ty>() != 0 {
            with_service(|service| {
                service.update_last_error_str("input ptr is misaligned".to_string())
            });
            return $err_return_val;
        } else {
            unsafe { &*($ptr_var as *const $expected_ty) }
        }
    };
    ($ptr_var:ident, $expected_ty:ty, $err_msg:expr) => {
        cast_input_const_ptr!(
            $ptr_var,
            $expected_ty,
            $err_msg,
            vm_exec_result_t::VM_EXEC_ERROR
        )
    };
}
