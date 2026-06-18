use libc::{c_char, c_int};
use std::{ptr, slice};

/// The `wasmer_result_t` enum is a type that represents either a
/// success, or a failure.
#[allow(non_camel_case_types)]
#[repr(C)]
pub enum vm_exec_result_t {
    /// Represents a success.
    VM_EXEC_OK = 1,

    /// Represents a failure.
    VM_EXEC_ERROR = 2,
}

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct vm_exec_byte_array {
    pub bytes: *const u8,
    pub bytes_len: u32,
}

impl vm_exec_byte_array {
    /// Get the data as a slice
    pub fn as_slice(&self) -> &[u8] {
        unsafe { get_slice_checked(self.bytes, self.bytes_len as usize) }
    }
}

/// Defence-in-depth byte-count cap for `get_slice_checked`. A caller
/// passing a `len` whose byte size exceeds this cap gets back `&[]`
/// rather than a slice descriptor pointing at memory of dubious size.
/// 256 MiB is well above any legitimate FFI hand-off (the largest
/// caller-shaped buffer in this codebase is a compiled-cache blob
/// capped at 32 MiB upstream — see capi_instance_cache.rs).
/// See issues/ISSUE-025.
pub(crate) const MAX_SLICE_CHECKED_BYTES: usize = 256 * 1024 * 1024;

/// Gets a slice from a pointer and a length, returning an empty slice if the
/// pointer is null or if `len * size_of::<T>()` exceeds
/// `MAX_SLICE_CHECKED_BYTES`. Silent degradation to empty slice is
/// consistent with the existing null-pointer behaviour.
#[inline]
pub(crate) unsafe fn get_slice_checked<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
    if ptr.is_null() {
        return &[];
    }
    let byte_size = len.saturating_mul(std::mem::size_of::<T>());
    if byte_size > MAX_SLICE_CHECKED_BYTES {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

fn string_len_with_null(len: usize) -> Option<c_int> {
    c_int::try_from(len).ok()?.checked_add(1)
}

pub(crate) fn string_length_str(s: &str) -> c_int {
    if s.is_empty() {
        0
    } else {
        string_len_with_null(s.len()).unwrap_or(-1)
    }
}

/// Copies a String to destination pointer, over the C API.
pub(crate) unsafe fn string_copy(
    s: String,
    dest_buffer: *mut c_char,
    dest_buffer_len: c_int,
) -> c_int {
    unsafe { string_copy_str(&s, dest_buffer, dest_buffer_len) }
}

pub(crate) unsafe fn string_copy_str(
    s: &str,
    dest_buffer: *mut c_char,
    dest_buffer_len: c_int,
) -> c_int {
    unsafe {
        if dest_buffer.is_null() {
            // buffer pointer is null
            return -1;
        }

        // Defence-in-depth: a negative `c_int` cast to `usize` would wrap to
        // a huge positive value, producing a slice with a bogus length below.
        // Reject negative lengths explicitly. Today no caller passes a
        // negative value, but the boundary is C and the contract should be
        // explicit. See issues/ISSUE-007.
        if dest_buffer_len < 0 {
            return -1;
        }

        let string_len_with_null = match string_len_with_null(s.len()) {
            Some(len) => len,
            None => return -1,
        };

        let dest_buffer_len = dest_buffer_len as usize;

        if s.len() >= dest_buffer_len {
            // buffer is too small to hold the error message
            return -1;
        }

        let dest_buffer = slice::from_raw_parts_mut(dest_buffer as *mut u8, dest_buffer_len);

        ptr::copy_nonoverlapping(s.as_ptr(), dest_buffer.as_mut_ptr(), s.len());

        // Add a trailing null so people using the string as a `char *` don't
        // accidentally read into garbage.
        dest_buffer[s.len()] = 0;

        string_len_with_null
    }
}

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct vm_exec_byte_array_list {
    pub arrays: *const vm_exec_byte_array,
    pub arrays_len: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_copy_rejects_negative_dest_buffer_len() {
        let mut buf = [0u8; 16];
        let result =
            unsafe { string_copy("test".to_string(), buf.as_mut_ptr() as *mut c_char, -1) };
        assert_eq!(result, -1);
        assert_eq!(
            buf, [0u8; 16],
            "buffer must not be written on negative-length rejection"
        );
    }

    #[test]
    fn string_copy_rejects_null_dest_buffer() {
        let result = unsafe { string_copy("test".to_string(), ptr::null_mut(), 16) };
        assert_eq!(result, -1);
    }

    #[test]
    fn string_copy_rejects_too_small_buffer() {
        let mut buf = [0u8; 4];
        let result = unsafe {
            string_copy(
                "longer than buf".to_string(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as c_int,
            )
        };
        assert_eq!(result, -1);
    }

    #[test]
    fn string_copy_writes_and_null_terminates() {
        let mut buf = [0xFFu8; 16];
        let result = unsafe {
            string_copy(
                "hi".to_string(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as c_int,
            )
        };
        assert_eq!(result, 3); // 2 bytes + null terminator
        assert_eq!(&buf[..3], b"hi\0");
    }

    #[test]
    fn string_len_with_null_rejects_c_int_overflow() {
        assert_eq!(string_len_with_null(c_int::MAX as usize), None);
    }

    #[test]
    fn string_len_with_null_accepts_largest_representable_payload() {
        assert_eq!(
            string_len_with_null(c_int::MAX as usize - 1),
            Some(c_int::MAX)
        );
    }

    #[test]
    fn get_slice_checked_returns_empty_on_null() {
        let result: &[u8] = unsafe { get_slice_checked(ptr::null::<u8>(), 16) };
        assert!(result.is_empty());
    }

    #[test]
    fn get_slice_checked_returns_empty_on_absurd_len() {
        let buf = [1u8; 8];
        // len far above MAX_SLICE_CHECKED_BYTES; must NOT produce a
        // multi-GiB slice descriptor over the 8-byte buf.
        let result: &[u8] = unsafe { get_slice_checked(buf.as_ptr(), MAX_SLICE_CHECKED_BYTES + 1) };
        assert!(result.is_empty());
    }

    #[test]
    fn get_slice_checked_returns_slice_within_cap() {
        let buf = [0xAAu8; 4];
        let result: &[u8] = unsafe { get_slice_checked(buf.as_ptr(), 4) };
        assert_eq!(result, &[0xAA, 0xAA, 0xAA, 0xAA]);
    }
}
