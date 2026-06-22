//! Typed handle registry for `CapiInstance` and `CapiExecutor`.
//!
//! ISSUE-001 + ISSUE-010 closure. Replaces raw-pointer identity for
//! C-API objects with **monotonic u64 IDs** that are never reused, even
//! after destroy. The wire format across the FFI is unchanged
//! (`*mut vm_exec_instance_t` and `*mut vm_exec_executor_t` still cross
//! the boundary), but those pointer values are now reinterpreted as
//! opaque IDs into this registry rather than dereferenced as C-heap
//! pointers.
//!
//! ## Why this closes the residual UB
//!
//! - **Stale-pointer use** (the original ISSUE-001): impossible. The
//!   incoming "pointer" is just an ID; if the ID isn't in the registry,
//!   lookup returns `None` and the entry point errors out without ever
//!   touching memory.
//! - **Same-pointer-value reuse after free** (the ISSUE-001 residual
//!   that the earlier pointer-liveness partial fixes couldn't close):
//!   impossible. Allocator behavior is irrelevant —
//!   IDs are issued from a monotonic AtomicU64 counter that starts at 1
//!   and never repeats.
//! - **Type confusion across instance/executor pointers** (ISSUE-010):
//!   impossible. The registry has typed maps — an instance ID looked
//!   up in the executor map returns `None`, and vice versa.
//! - **Use-during-destroy race**: safe by construction. `lookup_*`
//!   returns an `Arc<CapiInstance>` clone; the in-flight caller's
//!   strong reference keeps the underlying object alive until the call
//!   completes, even if a concurrent destroy removes the registry's
//!   own strong reference. The drop happens on the LAST Arc release.
//!
//! ## Concurrency
//!
//! `RwLock<HashMap<u64, Arc<CapiX>>>` per kind. Reader-heavy workload
//! (every C-API call is a registry read; only construct/destroy take
//! the write lock). If lock contention shows up under sustained
//! high-throughput workloads, the next iteration is `dashmap` or
//! sharded maps — but the call rate from a single VM-host process is
//! much lower than the rate at which a parking_lot RwLock can serve
//! reads, so contention is unlikely to matter in practice.
//!
//! ## ID 0 is reserved
//!
//! The counter starts at 1, so an ID of 0 is never issued. Callers can
//! continue to treat NULL (= 0 cast to pointer) as "no instance" — the
//! null check at the top of every entry point still catches it before
//! lookup runs.
//!
//! ## Defense-in-depth notes (transitional)
//!
//! The registry is the single source of truth for C-API object identity
//! and liveness.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use crate::capi_executor::CapiExecutor;
use crate::capi_instance::CapiInstance;

/// The single process-wide handle registry. Lazy-initialised on first
/// use to avoid allocator-during-static-init issues across cdylib builds.
fn registry() -> &'static HandleRegistry {
    static REG: OnceLock<HandleRegistry> = OnceLock::new();
    REG.get_or_init(HandleRegistry::new)
}

struct HandleRegistry {
    instances: RwLock<HashMap<u64, Arc<CapiInstance>>>,
    executors: RwLock<HashMap<u64, Arc<CapiExecutor>>>,
    next_instance_id: AtomicU64,
    next_executor_id: AtomicU64,
}

impl HandleRegistry {
    fn new() -> Self {
        HandleRegistry {
            instances: RwLock::new(HashMap::new()),
            executors: RwLock::new(HashMap::new()),
            // Counters start at 1 so an ID of 0 is never issued; this
            // preserves the conventional "NULL means nothing" semantic
            // that callers and the in-tree null checks already rely on.
            next_instance_id: AtomicU64::new(1),
            next_executor_id: AtomicU64::new(1),
        }
    }
}

// ----- instance API -----

/// Register a `CapiInstance` and return its monotonic ID. The returned
/// ID is what the C-API constructor writes through its out-param,
/// reinterpreted as a `*mut vm_exec_instance_t`.
pub fn register_instance(instance: CapiInstance) -> u64 {
    let r = registry();
    let id = r.next_instance_id.fetch_add(1, Ordering::Relaxed);
    r.instances
        .write()
        .expect("handle_registry instance lock poisoned")
        .insert(id, Arc::new(instance));
    id
}

/// Look up an instance by ID. Returns `None` if the ID is not currently
/// registered (never issued, or already destroyed). The returned `Arc`
/// keeps the instance alive for the duration of the caller's borrow,
/// even if a concurrent thread destroys the same ID.
pub fn lookup_instance(id: u64) -> Option<Arc<CapiInstance>> {
    if id == 0 {
        return None;
    }
    registry()
        .instances
        .read()
        .expect("handle_registry instance lock poisoned")
        .get(&id)
        .cloned()
}

/// Remove an instance from the registry. Returns `true` if the ID was
/// present (the normal destroy case); returns `false` if the ID was
/// never issued or was already destroyed (safe double-destroy).
///
/// The actual `CapiInstance` is dropped when the LAST `Arc` reference
/// is released — the registry's own strong reference goes away here,
/// but any in-flight caller holding a clone keeps the object alive
/// until their call completes.
pub fn destroy_instance(id: u64) -> bool {
    if id == 0 {
        return false;
    }
    registry()
        .instances
        .write()
        .expect("handle_registry instance lock poisoned")
        .remove(&id)
        .is_some()
}

// ----- executor API (mirror of instance API) -----

pub fn register_executor(executor: CapiExecutor) -> u64 {
    let r = registry();
    let id = r.next_executor_id.fetch_add(1, Ordering::Relaxed);
    r.executors
        .write()
        .expect("handle_registry executor lock poisoned")
        .insert(id, Arc::new(executor));
    id
}

pub fn lookup_executor(id: u64) -> Option<Arc<CapiExecutor>> {
    if id == 0 {
        return None;
    }
    registry()
        .executors
        .read()
        .expect("handle_registry executor lock poisoned")
        .get(&id)
        .cloned()
}

pub fn destroy_executor(id: u64) -> bool {
    if id == 0 {
        return false;
    }
    registry()
        .executors
        .write()
        .expect("handle_registry executor lock poisoned")
        .remove(&id)
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    // Note: full round-trip tests through `register_instance` /
    // `lookup_instance` need a `CapiInstance` value, which requires a
    // mock `Box<dyn InstanceLegacy>`. Those tests live in
    // capi_instance.rs::tests where the mock type is defined.
    // This module focuses on properties that don't need the inner type.

    #[test]
    fn instance_and_executor_id_spaces_are_independent() {
        // Issuing an instance ID must not advance the executor counter.
        // (The counters are separate AtomicU64s; this test guards against
        // a future refactor accidentally unifying them and breaking the
        // typed-slot guarantee that closes ISSUE-010.)
        let r = registry();
        let inst_before = r.next_instance_id.load(Ordering::Relaxed);
        let exec_before = r.next_executor_id.load(Ordering::Relaxed);

        // Bump instance counter manually (simulates registration without
        // needing a real CapiInstance value here).
        r.next_instance_id.fetch_add(1, Ordering::Relaxed);

        let inst_after = r.next_instance_id.load(Ordering::Relaxed);
        let exec_after = r.next_executor_id.load(Ordering::Relaxed);
        assert_eq!(inst_after, inst_before + 1);
        assert_eq!(exec_after, exec_before);
    }

    #[test]
    fn lookup_zero_returns_none_for_both_kinds() {
        // ID 0 is reserved (never issued) so the conventional NULL
        // check at the head of every entry point still works after
        // the registry-backed redesign.
        assert!(lookup_instance(0).is_none());
        assert!(lookup_executor(0).is_none());
    }

    #[test]
    fn destroy_zero_returns_false_for_both_kinds() {
        assert!(!destroy_instance(0));
        assert!(!destroy_executor(0));
    }

    #[test]
    fn destroy_unknown_id_returns_false() {
        // A high ID that was never issued; destroy must report "not
        // present" rather than succeeding silently.
        assert!(!destroy_instance(u64::MAX));
        assert!(!destroy_executor(u64::MAX));
    }
}
