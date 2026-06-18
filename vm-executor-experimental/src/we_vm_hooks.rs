use std::rc::Weak;

use multiversx_chain_vm_executor::{MemLength, MemPtr, VMHooks, VMHooksEarlyExit};
use wasmer::FunctionEnvMut;

use crate::{ExperimentalInstanceInner, ExperimentalInstanceState, ExperimentalVMHooksBuilder};

pub struct VMHooksWrapper {
    pub vm_hooks_builder: Box<dyn ExperimentalVMHooksBuilder>,
    pub wasmer_inner: Weak<ExperimentalInstanceInner>,
}

// SAFETY: this unsafe assertion is **structurally unsound** on the
// type level: `std::rc::Weak<T>` is `!Send + !Sync` regardless of T
// (it uses non-atomic refcount manipulation). Asserting Send + Sync
// here is a contract the *compiler cannot validate* — it would never
// have compiled with the type-derived auto-traits alone.
//
// The experimental executor is single-threaded in practice and the
// wasmer engine is configured to dispatch host calls on the same
// thread that constructed the wrapper. As long as that invariant
// holds, the assertion below is not observably unsound — but the
// audit finding is correct that the assertion is not type-safe.
//
// The proper fix is to swap `Rc / Rc::Weak` for `Arc / Arc::Weak`
// throughout the experimental executor (ExperimentalInstanceInner
// and ExperimentalInstanceState's wasmer_inner field both need the
// same change). That is a real refactor with performance / semantic
// implications and is tracked as a separate scope from the
// production wasmer path's fix.
//
// The production `vm-executor-wasmer` crate has been moved to
// `Arc<dyn VMHooksLegacy + Send + Sync>` with the unsafe assertion
// removed entirely. This experimental crate retains the assertion
// pending the Rc -> Arc refactor.
unsafe impl Send for VMHooksWrapper {}
unsafe impl Sync for VMHooksWrapper {}

pub fn convert_mem_ptr(raw: i32) -> MemPtr {
    raw as MemPtr
}

pub fn convert_mem_length(raw: i32) -> MemLength {
    raw as MemLength
}

pub fn with_vm_hooks<F, R>(
    mut env: FunctionEnvMut<VMHooksWrapper>,
    f: F,
) -> Result<R, VMHooksEarlyExit>
where
    F: FnOnce(&mut dyn VMHooks) -> Result<R, VMHooksEarlyExit>,
    R: Default + 'static,
{
    let (data, mut store_mut) = env.data_and_store_mut();

    let mut instance_state = ExperimentalInstanceState {
        wasmer_inner: data.wasmer_inner.clone(),
        store_mut: &mut store_mut,
    };

    let mut vm_hooks = data.vm_hooks_builder.create_vm_hooks(&mut instance_state);

    f(&mut *vm_hooks)
}
