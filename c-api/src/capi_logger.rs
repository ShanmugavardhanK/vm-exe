use std::{any::Any, sync::Once};

use crate::{set_log_level, u64_to_log_level};
use meta::capi_safe_unwind;

use crate::{basic_types::vm_exec_result_t, service_singleton::with_service};

static PANIC_HANDLER: Once = Once::new();

pub fn set_panic_handler() {
    // Initialize the panic handler only once
    PANIC_HANDLER.call_once(|| {
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            previous_hook(panic_info);
        }));
    });
}

pub fn record_panic_payload(payload: &(dyn Any + Send)) {
    let message = match payload.downcast_ref::<&str>() {
        Some(message) => (*message).to_string(),
        None => match payload.downcast_ref::<String>() {
            Some(message) => message.clone(),
            None => "non-string panic payload".to_string(),
        },
    };

    let _ = std::panic::catch_unwind(|| {
        with_service(|service| {
            service.update_last_error_str(format!("Rust panic in VM executor C API: {message}"))
        });
    });
}

/// Sets the log level.
#[unsafe(no_mangle)]
#[capi_safe_unwind(vm_exec_result_t::VM_EXEC_ERROR)]
pub extern "C" fn vm_exec_set_log_level(value: u64) -> vm_exec_result_t {
    let result = u64_to_log_level(value);

    match result {
        Ok(level) => {
            set_log_level(level);
            vm_exec_result_t::VM_EXEC_OK
        }
        Err(message) => {
            with_service(|service| service.update_last_error_str(message.to_string()));
            vm_exec_result_t::VM_EXEC_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[meta::capi_safe_unwind(crate::vm_exec_result_t::VM_EXEC_ERROR)]
    extern "C" fn panic_for_test() -> crate::vm_exec_result_t {
        panic!("diagnostic panic payload")
    }

    #[test]
    fn capi_safe_unwind_records_panic_payload() {
        let _guard = crate::service_singleton::test::LAST_ERROR_TEST_MUTEX
            .lock()
            .unwrap();
        let result = panic_for_test();

        assert!(matches!(result, crate::vm_exec_result_t::VM_EXEC_ERROR));
        let last_error = with_service(|service| service.get_last_error_string());
        assert!(last_error.contains("Rust panic in VM executor C API"));
        assert!(last_error.contains("diagnostic panic payload"));
    }
}
