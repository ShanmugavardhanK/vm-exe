//! Thread-safety contract for `CapiVMHooks`.
//!
//! `CapiVMHooks` is auto-generated (see `capi_vm_hooks.rs` — "DO NOT
//! EDIT"). Its struct fields include a `*mut c_void` (the Go-side
//! opaque runtime handle) and a struct of C function pointers. Raw
//! pointers are `!Send` and `!Sync` in Rust by default, so the
//! auto-generated struct cannot be placed inside the
//! `Arc<dyn VMHooksLegacy + Send + Sync>` the wasmer executor now
//! requires.
//!
//! This file holds the *only* thread-safety assertion for
//! `CapiVMHooks`. It is deliberately NOT in `capi_vm_hooks.rs` so the
//! next vmhooks-generator run does not wipe it. If
//! `capi_vm_hooks.rs` changes shape (e.g. a new field is added that
//! breaks the contract below), the new field must be reviewed
//! against the contract before the generator output is accepted.
//!
//! # Safety contract held by mx-chain-go
//!
//! 1. **Exclusive runtime ownership.** mx-chain-go allocates the
//!    object that `vm_hooks_ptr` points to and retains exclusive
//!    ownership for the entire lifetime of the corresponding
//!    `CapiVMHooks` instance. No other Go process or Rust crate
//!    aliases that pointer.
//!
//! 2. **No concurrent contract execution.** A single contract
//!    execution does not span concurrent goroutines on the Go side.
//!    Host-call dispatch through `c_func_pointers_ptr` is serialised
//!    by the surrounding chain-execution pipeline. Even when the
//!    wasmer engine internally uses worker threads, the VM hooks are
//!    re-entrancy-safe (each call returns before another can start).
//!
//! 3. **Pointer outlives consumer.** mx-chain-go releases the
//!    runtime object only after all `CapiVMHooks` instances that
//!    reference it have been destroyed via
//!    `vm_exec_instance_destroy`.
//!
//! These guarantees are documented at the FFI boundary between
//! mx-chain-go and mx-vm-executor-rs. The `unsafe impl Send` /
//! `unsafe impl Sync` below encode that contract on the Rust side so
//! the wasmer executor can place `CapiVMHooks` behind a
//! `dyn VMHooksLegacy + Send + Sync` trait object.
//!
//! Previously this contract was hidden behind a blanket
//! `unsafe impl Send for VMHooksWrapper {}` in
//! `vm-executor-wasmer/src/wasmer_vm_hooks.rs` — invisible at the
//! per-impl review level. The audit flagged that blanket assertion;
//! moving the assertion here makes the contract reviewable per
//! concrete type.

use crate::capi_vm_hooks::CapiVMHooks;

unsafe impl Send for CapiVMHooks {}
unsafe impl Sync for CapiVMHooks {}
