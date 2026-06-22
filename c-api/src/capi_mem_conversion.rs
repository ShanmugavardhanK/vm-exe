use multiversx_chain_vm_executor::{MemLength, MemPtr};
use multiversx_chain_vm_executor_wasmer::set_last_error;

const MEM_PTR_OUT_OF_RANGE: &str = "VM hook memory pointer does not fit the Go int32 bridge";
const MEM_LENGTH_OUT_OF_RANGE: &str = "VM hook memory length does not fit the Go int32 bridge";
const INVALID_BRIDGE_MEMORY_VALUE: i32 = -1;

pub(crate) fn mem_ptr_to_i32(mem_ptr: MemPtr) -> i32 {
    match i32::try_from(mem_ptr) {
        Ok(value) => value,
        Err(_) => {
            set_last_error(format!("{MEM_PTR_OUT_OF_RANGE}: {mem_ptr}"));
            INVALID_BRIDGE_MEMORY_VALUE
        }
    }
}

pub(crate) fn mem_length_to_i32(mem_length: MemLength) -> i32 {
    match i32::try_from(mem_length) {
        Ok(value) => value,
        Err(_) => {
            set_last_error(format!("{MEM_LENGTH_OUT_OF_RANGE}: {mem_length}"));
            INVALID_BRIDGE_MEMORY_VALUE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multiversx_chain_vm_executor_wasmer::with_last_error;

    use crate::service_singleton::test::LAST_ERROR_TEST_MUTEX;

    #[test]
    fn mem_ptr_to_i32_accepts_representable_values() {
        assert_eq!(mem_ptr_to_i32(0), 0);
        assert_eq!(mem_ptr_to_i32(42), 42);
        assert_eq!(mem_ptr_to_i32(i32::MAX as MemPtr), i32::MAX);
        assert_eq!(mem_ptr_to_i32(-1), -1);
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn mem_ptr_to_i32_rejects_values_above_i32_max() {
        let _guard = LAST_ERROR_TEST_MUTEX.lock().unwrap();
        let value = i32::MAX as MemPtr + 1;

        assert_eq!(mem_ptr_to_i32(value), INVALID_BRIDGE_MEMORY_VALUE);
        with_last_error(|last_error| {
            assert_eq!(last_error, format!("{MEM_PTR_OUT_OF_RANGE}: {value}"));
        });
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn mem_ptr_to_i32_rejects_values_below_i32_min() {
        let _guard = LAST_ERROR_TEST_MUTEX.lock().unwrap();
        let value = i32::MIN as MemPtr - 1;

        assert_eq!(mem_ptr_to_i32(value), INVALID_BRIDGE_MEMORY_VALUE);
        with_last_error(|last_error| {
            assert_eq!(last_error, format!("{MEM_PTR_OUT_OF_RANGE}: {value}"));
        });
    }

    #[test]
    fn mem_length_to_i32_accepts_representable_values() {
        assert_eq!(mem_length_to_i32(0), 0);
        assert_eq!(mem_length_to_i32(42), 42);
        assert_eq!(mem_length_to_i32(i32::MAX as MemLength), i32::MAX);
        assert_eq!(mem_length_to_i32(-1), -1);
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn mem_length_to_i32_rejects_values_above_i32_max() {
        let _guard = LAST_ERROR_TEST_MUTEX.lock().unwrap();
        let value = i32::MAX as MemLength + 1;

        assert_eq!(mem_length_to_i32(value), INVALID_BRIDGE_MEMORY_VALUE);
        with_last_error(|last_error| {
            assert_eq!(last_error, format!("{MEM_LENGTH_OUT_OF_RANGE}: {value}"));
        });
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn mem_length_to_i32_rejects_values_below_i32_min() {
        let _guard = LAST_ERROR_TEST_MUTEX.lock().unwrap();
        let value = i32::MIN as MemLength - 1;

        assert_eq!(mem_length_to_i32(value), INVALID_BRIDGE_MEMORY_VALUE);
        with_last_error(|last_error| {
            assert_eq!(last_error, format!("{MEM_LENGTH_OUT_OF_RANGE}: {value}"));
        });
    }
}
