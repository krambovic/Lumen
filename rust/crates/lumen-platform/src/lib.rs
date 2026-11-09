//! Windows platform integration primitives shared by all Lumen crates.

pub mod dpapi;
pub mod idle;
pub mod inet_settings;
pub mod network_route_context;
pub mod os_info;
pub mod route_leases;
pub mod subprocess;
pub mod system_language;
pub mod windows_network;

/// Lowercased primary user locale ("ru", "fa", "zh", ...).
pub use system_language::system_language;
pub use idle::idle_seconds;
pub use subprocess::{CREATE_NO_WINDOW, decode_output, hidden_command, run_text, run_text_owned};
