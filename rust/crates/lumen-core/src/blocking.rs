//! Blocking HTTP from code that may run on a Tokio runtime thread.
//!
//! The controller drives its transitions from an async event loop, while the
//! ported services (Clash API, DoH, HTTP ping) use `reqwest::blocking`. Building
//! or dropping a blocking client on a runtime thread panics ("Cannot drop a
//! runtime in a context where blocking is not allowed"), which aborted sing-box
//! connects before the system proxy and Firefox prefs were applied.

/// Run `work` on a plain thread when called from inside a Tokio runtime, and
/// in place otherwise.
pub fn outside_runtime<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    if tokio::runtime::Handle::try_current().is_err() {
        return work();
    }
    std::thread::scope(|scope| match scope.spawn(work).join() {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocking_client_round_trip() -> bool {
        reqwest::blocking::Client::builder().no_proxy().build().is_ok()
    }

    #[test]
    fn a_blocking_client_can_be_built_from_an_async_task() {
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let built = runtime.block_on(async { outside_runtime(blocking_client_round_trip) });
        assert!(built);
    }

    #[test]
    fn runs_in_place_without_a_runtime() {
        let id = std::thread::current().id();
        assert_eq!(outside_runtime(|| std::thread::current().id()), id);
    }
}
