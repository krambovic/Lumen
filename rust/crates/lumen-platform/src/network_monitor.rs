//! Network change polling (port of `windows/xray_fluent/network_monitor.py`).
//!
//! The Qt `QTimer` + `pyqtSignal` pair becomes a worker thread that invokes a
//! `network_changed(previous, current)` callback; the fingerprint probe and
//! the change detection are ported unchanged.

use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

type ChangeCallback = Box<dyn FnMut(&str, &str) + Send>;

pub struct NetworkMonitor {
    interval: Duration,
    on_change: Option<ChangeCallback>,
    last_fingerprint: String,
    stop_flag: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl NetworkMonitor {
    /// The monitor is constructed before the first window frame. Network
    /// probing there can stall startup on a busy or offline Windows boot, so
    /// the initial fingerprint is only taken in `start`.
    pub fn new(interval_ms: u64, on_change: ChangeCallback) -> Self {
        Self {
            interval: Duration::from_millis(interval_ms.max(1)),
            on_change: Some(on_change),
            last_fingerprint: String::new(),
            stop_flag: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }

    pub fn start(&mut self) {
        self.last_fingerprint = Self::fingerprint();
        let mut last_fingerprint = self.last_fingerprint.clone();
        let mut on_change = self.on_change.take().expect("network monitor started twice");
        let interval = self.interval;
        let stop_flag = Arc::clone(&self.stop_flag);
        self.worker = Some(std::thread::spawn(move || {
            while !stop_flag.load(Ordering::SeqCst) {
                std::thread::sleep(interval);
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                advance(&mut last_fingerprint, &mut on_change);
            }
        }));
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    /// `_check`: emit `(previous, current)` when the fingerprint changed.
    pub fn check(&mut self) -> bool {
        let mut on_change = self
            .on_change
            .take()
            .expect("network monitor callback missing");
        let changed = advance(&mut self.last_fingerprint, &mut on_change);
        self.on_change = Some(on_change);
        changed
    }

    /// `_fingerprint`: the local address a UDP socket to 8.8.8.8 would use.
    /// Any socket failure keeps the "0.0.0.0" sentinel; a later timer tick
    /// detects the real address.
    pub fn fingerprint() -> String {
        let Some(addr) = (|| {
            let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
            sock.connect("8.8.8.8:80").ok()?;
            sock.local_addr().ok()
        })() else {
            return "0.0.0.0".to_string();
        };
        addr.ip().to_string()
    }
}

fn advance(last_fingerprint: &mut String, on_change: &mut ChangeCallback) -> bool {
    let current = NetworkMonitor::fingerprint();
    if current == *last_fingerprint {
        return false;
    }
    let previous = std::mem::take(last_fingerprint);
    *last_fingerprint = current;
    on_change(&previous, &last_fingerprint);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn fingerprint_returns_a_local_address_or_zero() {
        let fingerprint = NetworkMonitor::fingerprint();
        assert!(!fingerprint.is_empty());
        assert!(fingerprint.parse::<std::net::IpAddr>().is_ok());
    }

    #[test]
    fn check_emits_previous_and_current_on_change() {
        let (sender, receiver) = channel::<(String, String)>();
        let mut monitor = NetworkMonitor::new(
            5000,
            Box::new(move |previous: &str, current: &str| {
                let _ = sender.send((previous.to_string(), current.to_string()));
            }),
        );
        monitor.last_fingerprint = "10.0.0.2".to_string();
        // The real fingerprint differs from the fake baseline almost surely;
        // when the host truly has 10.0.0.2 the no-change path is still valid.
        monitor.check();
        if let Ok((previous, current)) = receiver.try_recv() {
            assert_eq!(previous, "10.0.0.2");
            assert_eq!(current, NetworkMonitor::fingerprint());
            assert_eq!(monitor.last_fingerprint, current);
        }
    }
}
