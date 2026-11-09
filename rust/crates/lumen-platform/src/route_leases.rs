//! Process-wide route ownership shared by ping, speed test and direct HTTP.
//!
//! Port of `windows/xray_fluent/route_leases.py`.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

struct RouteLease {
    refs: usize,
    owned: bool,
    delete: Box<dyn Fn() + Send>,
}

static LEASES: LazyLock<Mutex<HashMap<String, RouteLease>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Acquire a reference to the route identified by `key`.
///
/// * an existing lease just gains a reference;
/// * a route that already exists in the OS is borrowed (not owned, never deleted);
/// * otherwise `create` is called and the lease owns the route.
pub fn acquire_route<C, E, D>(key: &str, create: C, exists: E, delete: D) -> bool
where
    C: FnOnce() -> bool,
    E: Fn() -> bool,
    D: Fn() + Send + 'static,
{
    let mut leases = LEASES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(lease) = leases.get_mut(key) {
        lease.refs += 1;
        return true;
    }
    let owned = if exists() {
        false
    } else {
        let created = create();
        if !created && !exists() {
            return false;
        }
        created
    };
    leases.insert(key.to_string(), RouteLease { refs: 1, owned, delete: Box::new(delete) });
    true
}

/// Drop one reference; the last owner deletes the route while still holding
/// the lock so that a new lease cannot race the cleanup.
pub fn release_route(key: &str) {
    let mut leases = LEASES.lock().unwrap_or_else(|e| e.into_inner());
    let Some(lease) = leases.get_mut(key) else {
        return;
    };
    lease.refs = lease.refs.saturating_sub(1);
    if lease.refs > 0 {
        return;
    }
    if let Some(lease) = leases.remove(key) {
        if lease.owned {
            let delete = lease.delete;
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| delete()));
        }
    }
}

/// Number of live references for `key` (diagnostics/tests).
pub fn lease_refs(key: &str) -> usize {
    LEASES.lock().map(|l| l.get(key).map(|x| x.refs).unwrap_or(0)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn owned_route_deleted_by_last_release() {
        let deletes = Arc::new(AtomicUsize::new(0));
        let d = deletes.clone();
        assert!(acquire_route("t1", || true, || false, move || { d.fetch_add(1, Ordering::SeqCst); }));
        assert!(acquire_route("t1", || panic!("no create"), || false, || {}));
        assert_eq!(lease_refs("t1"), 2);
        release_route("t1");
        assert_eq!(deletes.load(Ordering::SeqCst), 0);
        release_route("t1");
        assert_eq!(deletes.load(Ordering::SeqCst), 1);
        assert_eq!(lease_refs("t1"), 0);
    }

    #[test]
    fn borrowed_route_never_deleted() {
        let deletes = Arc::new(AtomicUsize::new(0));
        let d = deletes.clone();
        assert!(acquire_route("t2", || true, || true, move || { d.fetch_add(1, Ordering::SeqCst); }));
        release_route("t2");
        assert_eq!(deletes.load(Ordering::SeqCst), 0);
        assert!(!acquire_route("t3", || false, || false, || {}));
    }
}
