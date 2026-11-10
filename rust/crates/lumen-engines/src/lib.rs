//! Proxy core process management (xray, sing-box) and API clients.
//!
//! xray-core and sing-box are NOT reimplemented: they stay the upstream Go
//! binaries. This crate builds their configs, runs and supervises them and
//! talks to their local APIs.

pub mod core_resource_updater;
pub mod discord_proxy_manager;
pub mod native_test_config;
pub mod singbox;
pub mod speed_test;
pub mod xray;
pub mod zapret_manager;
