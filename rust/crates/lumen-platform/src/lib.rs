//! Windows platform integration primitives shared by all Lumen crates.

pub mod dpapi;
pub mod icmp;
pub mod idle;
pub mod inet_settings;
pub mod network_monitor;
pub mod network_route_context;
pub mod os_info;
pub mod power_reconnect;
pub mod process_conflicts;
pub mod process_traffic_collector;
#[cfg(windows)]
pub mod proxy_manager;
pub mod route_leases;
pub mod runtime_priority;
pub mod startup;
pub mod subprocess;
pub mod system_language;
pub mod win_proc_monitor;
pub mod win_process_list;
pub mod windows_network;

/// Lowercased primary user locale ("ru", "fa", "zh", ...).
pub use system_language::system_language;
pub use idle::idle_seconds;
pub use subprocess::{CREATE_NO_WINDOW, decode_output, hidden_command, run_text, run_text_owned};
