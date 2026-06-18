//! Read runtime errors.

// use crate::service::with_service;
use libc::{c_char, c_int};
use multiversx_chain_vm_executor_wasmer::with_last_error;

use crate::{string_copy_str, string_length_str};

/// Gets the length in bytes of the last error if any.
///
/// This can be used to dynamically allocate a buffer with the correct number of
/// bytes needed to store a message.
#[unsafe(no_mangle)]
pub extern "C" fn vm_exec_last_error_length() -> c_int {
    with_last_error(string_length_str)
}

/// Gets the last error message if any into the provided buffer
/// `buffer` up to the given `length`.
///
/// The `length` parameter must be large enough to store the last
/// error message. Ideally, the value should come from
/// `wasmer_last_error_length()`.
///
/// The function returns the length of the string in bytes, `-1` if an
/// error occurs. Potential errors are:
///
///  * The buffer is a null pointer,
///  * The buffer is too small to hold the error message.
///
/// Note: The error message always has a trailing null character.
///
/// # Safety
///
/// C API function, works with raw object pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vm_exec_last_error_message(
    dest_buffer: *mut c_char,
    dest_buffer_len: c_int,
) -> c_int {
    with_last_error(|last_error| unsafe {
        string_copy_str(last_error, dest_buffer, dest_buffer_len)
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use multiversx_chain_vm_executor_wasmer::set_last_error;

    use crate::service_singleton::test::LAST_ERROR_TEST_MUTEX;

    use super::*;

    #[test]
    fn last_error_message_reads_error_written_from_another_thread() {
        let _guard = LAST_ERROR_TEST_MUTEX.lock().unwrap();
        const ERROR: &str = "cross-thread last error";

        std::thread::spawn(|| {
            set_last_error(ERROR.to_string());
        })
        .join()
        .unwrap();

        let mut buffer = [0 as c_char; 128];
        let copied =
            unsafe { vm_exec_last_error_message(buffer.as_mut_ptr(), buffer.len() as c_int) };

        assert_eq!(copied, (ERROR.len() + 1) as c_int);
        let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_str().unwrap();
        assert_eq!(message, ERROR);
    }
}
