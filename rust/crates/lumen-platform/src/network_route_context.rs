//! Cached physical default-route context with single-flight refresh.
//!
//! Port of `windows/xray_fluent/network_route_context.py`.

use std::sync::{Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::windows_network::query_windows_route_context;

const ROUTE_CONTEXT_TTL: Duration = Duration::from_secs(30);
const ROUTE_CONTEXT_WAIT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowsDefaultRouteContext {
    pub interface_alias: String,
    pub dns_servers: Vec<String>,
    pub interface_index: u32,
    pub next_hop: String,
    pub is_physical: bool,
    pub tun_active: bool,
}

#[derive(Default)]
struct CacheState {
    value: Option<WindowsDefaultRouteContext>,
    cached_at: Option<Instant>,
    refreshing: bool,
    generation: u64,
}

static CACHE: LazyLock<(Mutex<CacheState>, Condvar)> =
    LazyLock::new(|| (Mutex::new(CacheState::default()), Condvar::new()));

pub fn invalidate_windows_default_route_context() {
    let (lock, _) = &*CACHE;
    if let Ok(mut state) = lock.lock() {
        state.cached_at = None;
    }
}

fn query_now() -> Option<WindowsDefaultRouteContext> {
    let native = query_windows_route_context()?;
    Some(WindowsDefaultRouteContext {
        interface_alias: native.interface_alias,
        dns_servers: native.dns_servers,
        interface_index: native.interface_index,
        next_hop: native.next_hop,
        is_physical: true,
        tun_active: native.tun_active,
    })
}

/// Return the cached context (30 s TTL). Concurrent callers wait up to 3 s
/// for an in-flight refresh instead of querying the adapter table again.
pub fn get_windows_default_route_context(force_refresh: bool) -> Option<WindowsDefaultRouteContext> {
    if !cfg!(windows) {
        return None;
    }
    let (lock, cvar) = &*CACHE;
    let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
    if !force_refresh {
        if let Some(at) = state.cached_at {
            if at.elapsed() < ROUTE_CONTEXT_TTL {
                return state.value.clone();
            }
        }
    }
    if state.refreshing {
        let generation = state.generation;
        let (state, _) = cvar
            .wait_timeout_while(state, ROUTE_CONTEXT_WAIT_TIMEOUT, |s| {
                s.refreshing && s.generation == generation
            })
            .unwrap_or_else(|e| e.into_inner());
        return state.value.clone();
    }
    state.refreshing = true;
    drop(state);

    let value = std::panic::catch_unwind(query_now).ok().flatten();

    let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
    state.value = value.clone();
    // A transient failure must not block an "update all" batch for the TTL.
    state.cached_at = if value.is_some() { Some(Instant::now()) } else { None };
    state.refreshing = false;
    state.generation = state.generation.wrapping_add(1);
    cvar.notify_all();
    value
}
