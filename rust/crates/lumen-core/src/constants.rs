//! Global constants matching `windows/xray_fluent/constants.py`.

use std::path::PathBuf;
use std::sync::OnceLock;

pub const APP_NAME: &str = "Lumen";
pub const APP_VERSION: &str = "2.1.4";
pub const STATE_SCHEMA_VERSION: i64 = 1;
/// Bump when parsing/reconciliation semantics change; cached validators remain
/// useful, while the value lets future migrations force one clean reconcile.
pub const SUBSCRIPTION_PARSER_REVISION: i64 = 5;
/// Onboarding revision. Users whose saved revision is lower walk through the
/// setup part of the welcome flow on the next launch.
pub const ONBOARDING_REVISION: i64 = 2;
pub const RECOMMENDED_VPN_URL: &str = "https://t.me/zapretvpns_bot";
pub const SUBSCRIPTION_FETCHER_EXE_NAME: &str = "lumen-subscription-fetcher.exe";

pub const DIAGNOSTICS_UPLOAD_URL: &str = "https://diagnostics.lumen-kvn.eu.cc/api/ingest";
/// Noise filter tag, not a client authentication secret.
pub const DIAGNOSTICS_SECRET: &str =
    "07f7d005166286e354645dcbce892998987bd8d8d20f296026dbb01ff05a9b8a";

pub const PROXY_HOST: &str = "127.0.0.1";
pub const DEFAULT_SOCKS_PORT: u16 = 10808;
pub const DEFAULT_HTTP_PORT: u16 = 10809;
pub const DEFAULT_DISCORD_SOCKS_PORT: u16 = 10818;
pub const DEFAULT_XRAY_STATS_API_PORT: u16 = 19085;
pub const XRAY_GITHUB_RELEASES_API: &str =
    "https://api.github.com/repos/XTLS/Xray-core/releases";

pub const ROUTING_GLOBAL: &str = "global";
pub const ROUTING_RULE: &str = "rule";
pub const ROUTING_DIRECT: &str = "direct";
pub const ROUTING_MODES: [&str; 3] = [ROUTING_GLOBAL, ROUTING_RULE, ROUTING_DIRECT];

pub const XRAY_DEFAULT_CONFIG_NAME: &str = "default.json";
pub const SINGBOX_DEFAULT_CONFIG_NAME: &str = "default.json";
pub const SINGBOX_CLASH_API_PORT: u16 = 19090;
pub const SINGBOX_CLASH_API_SELECTOR: &str = "proxy";
pub const SINGBOX_XRAY_RELAY_PORT: u16 = 11808;
pub const SINGBOX_TUN_INTERFACE_NAME: &str = "singbox_tun";
pub const SINGBOX_LEGACY_TUN_INTERFACE_NAMES: [&str; 2] = ["tun0", "xftun*"];

pub const SPEED_TEST_DEFAULT_URL: &str = "https://cachefly.cachefly.net/50mb.test";
pub const SPEED_TEST_PING_URL: &str = "https://www.google.com/generate_204";
pub const SPEED_TEST_TIMEOUT: f64 = 10.0;
pub const SPEED_TEST_MIXED_CONCURRENCY: usize = 10;
pub const SPEED_TEST_STARTUP_TIMEOUT: f64 = 5.0;
pub const SPEED_TEST_PING_TIMEOUT: f64 = 3.0;
pub const SPEED_TEST_MAX_PING_MS: u32 = 3500;
pub const SPEED_TEST_DOWNLOAD_IDLE_TIMEOUT: f64 = 3.0;
pub const SPEED_TEST_SLOW_GRACE_SECONDS: f64 = 2.5;
pub const SPEED_TEST_MIN_BYTES_AFTER_GRACE: u64 = 128 * 1024;
pub const SPEED_TEST_MIN_MBPS_AFTER_GRACE: f64 = 0.2;

pub const PING_METHODS: [&str; 4] = ["tcping", "icmp", "http", "real"];
pub const ICMP_PING_TIMEOUT_MS: u32 = 1500;
pub const SPEED_TEST_MAX_CONCURRENCY: usize = 32;

pub const SS_PROTECT_PORT_START: u16 = 19200;
pub const SS_PROTECT_PORT_END: u16 = 19300;

static BASE_DIR: OnceLock<PathBuf> = OnceLock::new();
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Install root: directory containing the running executable.
pub fn base_dir() -> &'static PathBuf {
    BASE_DIR.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    })
}

fn install_data_dir() -> PathBuf {
    base_dir().join("data")
}

/// Resolve the writable user data directory (portable marker or `%LOCALAPPDATA%`),
/// running the legacy brand-name migration exactly like `data_paths.py`.
pub fn init_data_dir() -> &'static PathBuf {
    DATA_DIR.get_or_init(|| {
        let base = base_dir().clone();
        let install = install_data_dir();
        if crate::paths::portable_marker_present(&base)
            && crate::paths::is_writable(&install)
        {
            crate::paths::migrate_legacy_roots_near(&base, &install);
            return install;
        }
        let dir = crate::paths::user_data_dir(APP_NAME);
        let _ = crate::paths::seed_user_data(&install, &dir);
        dir
    })
}

pub fn data_dir() -> &'static PathBuf {
    init_data_dir()
}

pub fn templates_dir() -> PathBuf {
    data_dir().join("templates")
}
pub fn configs_dir() -> PathBuf {
    data_dir().join("configs")
}
pub fn singbox_templates_dir() -> PathBuf {
    templates_dir().join("sing-box")
}
pub fn xray_templates_dir() -> PathBuf {
    templates_dir().join("xray")
}
pub fn singbox_configs_dir() -> PathBuf {
    configs_dir().join("sing-box")
}
pub fn xray_configs_dir() -> PathBuf {
    configs_dir().join("xray")
}
pub fn runtime_dir() -> PathBuf {
    data_dir().join("runtime")
}
pub fn log_dir() -> PathBuf {
    data_dir().join("logs")
}
pub fn state_file() -> PathBuf {
    data_dir().join("state.enc")
}
pub fn xray_config_file() -> PathBuf {
    runtime_dir().join("xray_config.json")
}
pub fn singbox_config_file() -> PathBuf {
    runtime_dir().join("singbox_config.json")
}

/// Default xray core path; dev checkout keeps cores at repo `core/`.
pub fn xray_path_default() -> PathBuf {
    let candidate = base_dir().join("core").join("xray.exe");
    if candidate.exists() {
        return candidate;
    }
    base_dir()
        .parent()
        .map(|p| p.join("core").join("xray.exe"))
        .unwrap_or(candidate)
}

pub fn singbox_path_default() -> PathBuf {
    let candidate = base_dir()
        .join("core")
        .join("sing-box-extended")
        .join("sing-box.exe");
    if candidate.exists() {
        return candidate;
    }
    base_dir()
        .parent()
        .map(|p| p.join("core").join("sing-box-extended").join("sing-box.exe"))
        .unwrap_or(candidate)
}

pub fn assets_dir() -> PathBuf {
    let candidate = base_dir().join("assets");
    if candidate.exists() {
        return candidate;
    }
    base_dir().join("assets")
}

pub fn app_icon_path() -> PathBuf {
    assets_dir().join("Lumen.ico")
}
