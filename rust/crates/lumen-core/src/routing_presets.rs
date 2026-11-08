//! Built-in routing presets (`routing_presets.py`).

use crate::models::RoutingSettings;
use crate::presets::SERVICE_PRESETS;

pub const ROUTING_PRESET_GLOBAL: &str = "global";
pub const ROUTING_PRESET_BLOCKED: &str = "blocked";
pub const ROUTING_PRESET_EXCEPT_RU: &str = "except_ru";
pub const ROUTING_PRESET_BLOCKED_CN: &str = "blocked_cn";
pub const ROUTING_PRESET_EXCEPT_CN: &str = "except_cn";
pub const ROUTING_PRESET_EXCEPT_IR: &str = "except_ir";

pub const BUILT_IN_ROUTING_PRESET_IDS: [&str; 6] = [
    ROUTING_PRESET_GLOBAL,
    ROUTING_PRESET_BLOCKED,
    ROUTING_PRESET_EXCEPT_RU,
    ROUTING_PRESET_BLOCKED_CN,
    ROUTING_PRESET_EXCEPT_CN,
    ROUTING_PRESET_EXCEPT_IR,
];

pub const REGION_RUSSIA: &str = "russia";
pub const REGION_CHINA: &str = "china";
pub const REGION_IRAN: &str = "iran";
pub const REGIONAL_PRESETS: [&str; 3] = [REGION_RUSSIA, REGION_CHINA, REGION_IRAN];

pub const RU_DIRECT_RULES: [&str; 2] = ["geosite:category-ru", "geoip:ru"];
pub const CN_DIRECT_RULES: [&str; 2] = ["geosite:cn", "geoip:cn"];
pub const IR_DIRECT_RULES: [&str; 2] = ["geosite:ir", "geoip:ir"];
pub const BLOCKED_PROXY_RULES: [&str; 3] = [
    "geosite:ru-blocked",
    "geoip:ru-blocked",
    "geoip:ru-blocked-community",
];
pub const BLOCKED_DIRECT_RULES: [&str; 2] = ["geosite:lumen-exclude", "geoip:lumen-exclude"];
pub const CN_BLOCKED_PROXY_RULES: [&str; 9] = [
    "geosite:gfw", "geosite:greatfire", "geosite:google", "geoip:facebook", "geoip:fastly",
    "geoip:google", "geoip:netflix", "geoip:telegram", "geoip:twitter",
];

/// Rules managed by the built-in quick presets; users' own geosite:/geoip:
/// rules must survive a preset switch. The two legacy entries remove data left
/// by older Lumen builds.
pub const INTERNAL_PRESET_DOMAIN_RULES: [&str; 15] = [
    "geosite:category-ru",
    "geoip:ru",
    "geosite:cn",
    "geoip:cn",
    "geosite:ir",
    "geoip:ir",
    "geosite:ru-blocked",
    "geoip:ru-blocked",
    "geoip:ru-blocked-community",
    "geosite:lumen-exclude",
    "geoip:lumen-exclude",
    "geosite:gfw",
    "geosite:greatfire",
    "geosite:ru-available-only-inside",
    "geosite:category-media-ru-blocked",
];

pub fn normalize_regional_preset(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if REGIONAL_PRESETS.contains(&normalized.as_str()) {
        normalized
    } else {
        REGION_RUSSIA.into()
    }
}

pub fn regional_routing_preset_ids(region: &str) -> Vec<&'static str> {
    let region = normalize_regional_preset(region);
    if region == REGION_CHINA {
        return vec![ROUTING_PRESET_GLOBAL, ROUTING_PRESET_BLOCKED_CN, ROUTING_PRESET_EXCEPT_CN];
    }
    if region == REGION_IRAN {
        return vec![ROUTING_PRESET_GLOBAL, ROUTING_PRESET_EXCEPT_IR];
    }
    vec![ROUTING_PRESET_GLOBAL, ROUTING_PRESET_BLOCKED, ROUTING_PRESET_EXCEPT_RU]
}

pub fn equivalent_regional_preset(preset_id: &str, region: &str) -> String {
    let choices = regional_routing_preset_ids(region);
    let value = preset_id.trim().to_lowercase();
    if value == ROUTING_PRESET_GLOBAL {
        return ROUTING_PRESET_GLOBAL.into();
    }
    if value.starts_with("except_") {
        return choices[choices.len() - 1].to_string();
    }
    if value == ROUTING_PRESET_BLOCKED || value == ROUTING_PRESET_BLOCKED_CN {
        if choices.len() == 3 {
            return choices[1].to_string();
        }
        return ROUTING_PRESET_GLOBAL.into();
    }
    value
}

pub fn normalize_routing_preset_for_region(preset_id: &str, region: &str) -> String {
    let value = preset_id.trim().to_lowercase();
    if !BUILT_IN_ROUTING_PRESET_IDS.contains(&value.as_str()) {
        return value;
    }
    equivalent_regional_preset(&value, region)
}

pub fn is_internal_preset_rule(value: &str) -> bool {
    INTERNAL_PRESET_DOMAIN_RULES.contains(&value.trim().to_lowercase().as_str())
}

pub fn preset_requires_regional_geodata(preset_id: &str) -> bool {
    let value = preset_id.trim().to_lowercase();
    BUILT_IN_ROUTING_PRESET_IDS.contains(&value.as_str()) && value != ROUTING_PRESET_GLOBAL
}

pub fn custom_domain_rules(items: &[String]) -> Vec<String> {
    items.iter().filter(|item| !is_internal_preset_rule(item)).cloned().collect()
}

pub fn preset_domain_rules(preset_id: &str) -> (Vec<String>, Vec<String>) {
    match preset_id {
        ROUTING_PRESET_BLOCKED => (
            BLOCKED_DIRECT_RULES.iter().map(|s| s.to_string()).collect(),
            BLOCKED_PROXY_RULES.iter().map(|s| s.to_string()).collect(),
        ),
        ROUTING_PRESET_EXCEPT_RU => (
            RU_DIRECT_RULES.iter().map(|s| s.to_string()).collect(),
            Vec::new(),
        ),
        ROUTING_PRESET_BLOCKED_CN => (
            Vec::new(),
            CN_BLOCKED_PROXY_RULES.iter().map(|s| s.to_string()).collect(),
        ),
        ROUTING_PRESET_EXCEPT_CN => (
            CN_DIRECT_RULES.iter().map(|s| s.to_string()).collect(),
            Vec::new(),
        ),
        ROUTING_PRESET_EXCEPT_IR => (
            IR_DIRECT_RULES.iter().map(|s| s.to_string()).collect(),
            Vec::new(),
        ),
        _ => (Vec::new(), Vec::new()),
    }
}

pub fn proxy_default_services() -> std::collections::BTreeMap<String, String> {
    SERVICE_PRESETS
        .iter()
        .filter(|p| p.default_action == "proxy")
        .map(|p| (p.id.to_string(), "proxy".to_string()))
        .collect()
}

/// Migrate the blocked preset's unmatched-traffic fallback only; built-in
/// service actions resolve from preset metadata at runtime.
pub fn repair_builtin_preset_service_routes(routing: &RoutingSettings) -> RoutingSettings {
    let mut routing = routing.clone();
    let preset = routing.preset_id.trim().to_lowercase();
    if preset != ROUTING_PRESET_BLOCKED && preset != ROUTING_PRESET_BLOCKED_CN {
        return routing;
    }
    let mut default_outbound = routing.tun_default_outbound.trim().to_lowercase();
    if !routing.tun_default_outbound_user_selected {
        default_outbound = "direct".into();
    }
    if default_outbound != routing.tun_default_outbound {
        routing.tun_default_outbound = default_outbound;
    }
    routing
}

pub fn build_routing_preset(current: &RoutingSettings, preset_id: &str) -> RoutingSettings {
    let direct_domains = custom_domain_rules(&current.direct_domains);
    let proxy_domains = custom_domain_rules(&current.proxy_domains);
    let block_domains = custom_domain_rules(&current.block_domains);
    match preset_id {
        ROUTING_PRESET_GLOBAL => RoutingSettings {
            preset_id: preset_id.into(),
            mode: "global".into(),
            direct_domains,
            proxy_domains,
            block_domains,
            service_routes: Default::default(),
            tun_default_outbound: "proxy".into(),
            tun_default_outbound_user_selected: false,
            ..current.clone()
        },
        ROUTING_PRESET_BLOCKED | ROUTING_PRESET_BLOCKED_CN => RoutingSettings {
            preset_id: preset_id.into(),
            mode: "rule".into(),
            direct_domains,
            proxy_domains,
            block_domains,
            service_routes: proxy_default_services(),
            tun_default_outbound: "direct".into(),
            tun_default_outbound_user_selected: false,
            ..current.clone()
        },
        ROUTING_PRESET_EXCEPT_RU | ROUTING_PRESET_EXCEPT_CN | ROUTING_PRESET_EXCEPT_IR => {
            RoutingSettings {
                preset_id: preset_id.into(),
                mode: "global".into(),
                direct_domains,
                proxy_domains,
                block_domains,
                service_routes: Default::default(),
                tun_default_outbound: "proxy".into(),
                tun_default_outbound_user_selected: false,
                ..current.clone()
            }
        }
        _ => current.clone(),
    }
}
