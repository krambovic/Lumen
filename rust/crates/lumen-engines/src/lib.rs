//! Proxy core process management (xray, sing-box) and API clients.
//!
//! xray-core and sing-box are NOT reimplemented: they stay the upstream Go
//! binaries. This crate builds their configs, runs and supervises them and
//! talks to their local APIs.

pub mod singbox;
pub mod xray;
