//! Windows platform integration primitives shared by all Lumen crates.

pub mod dpapi;
pub mod idle;
pub mod system_language;

/// Lowercased primary user locale ("ru", "fa", "zh", …).
pub use system_language::system_language;
pub use idle::idle_seconds;
