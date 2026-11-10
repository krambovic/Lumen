//! winws2 strategies dedicated to the traffic of a VPN server.
//!
//! A server strategy is only the desync part of one winws2 profile
//! (`--lua-desync` / `--payload` / `--out-range` / `--in-range`); the target
//! (server IP, transport, port) is added by [`crate::zapret_server_scope`].
//! Strategies come from the winws2 catalogs ported from ZapretKVN
//! (`zapret/strategy_catalogs/winws2/<transport>.txt` plus local
//! `<transport>.local.txt` overrides). Each bound server picks one from the
//! catalog of its kind (TCP servers, QUIC servers, WireGuard); without a
//! choice the default strategy of the kind is used.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use regex::Regex;
use serde_json::Value;
use unicode_general_category::{get_general_category, GeneralCategory};

use crate::constants::base_dir;

pub fn zapret_dir() -> PathBuf {
    base_dir().join("zapret")
}

pub fn catalog_dir() -> PathBuf {
    zapret_dir().join("strategy_catalogs").join("winws2")
}

fn bin_dir() -> PathBuf {
    zapret_dir().join("bin")
}

pub const SERVER_KINDS: [&str; 3] = ["tcp", "quic", "wireguard"];

pub fn kind_transport(kind: &str) -> &'static str {
    match kind {
        "quic" | "wireguard" => "udp",
        _ => "tcp",
    }
}

pub fn kind_title(kind: &str) -> &'static str {
    match kind {
        "quic" => "QUIC",
        "wireguard" => "WireGuard",
        _ => "TCP",
    }
}

pub fn default_strategy(kind: &str) -> &'static str {
    match kind {
        "tcp" => "alt9",
        "quic" | "wireguard" => "general_bf_32",
        _ => "",
    }
}

/// Catalog labels, from most to least battle-tested.
pub const STRATEGY_LABELS: [&str; 6] =
    ["recommended", "stable", "stock", "game", "experimental", "caution"];

const WIREGUARD_PROTOCOLS: [&str; 5] = ["wireguard", "wg", "warp", "awg", "amneziawg"];
const QUIC_PROTOCOLS: [&str; 4] = ["hysteria", "hysteria2", "hy2", "tuic"];
const UDP_NETWORKS: [&str; 3] = ["kcp", "mkcp", "quic"];

/// Arguments a strategy may hold; everything else belongs to the app.
pub const STRATEGY_ARGUMENTS: [&str; 4] =
    ["--lua-desync=", "--payload=", "--out-range=", "--in-range="];
pub const CORE_LUA_INITS: [&str; 2] =
    ["--lua-init=@lua/zapret-lib.lua", "--lua-init=@lua/zapret-antidpi.lua"];

/// Blob name -> `--blob=` source (`@bin/<file>` or an inline hex literal).
pub const BLOB_REGISTRY: [(&str, &str); 74] = [
    ("discord_active", "@bin/ACTIVE_DISCORD_UDP.bin"),
    ("dtls_w3", "@bin/dtls_clienthello_w3_org.bin"),
    ("fake_dbankcloud", "@bin/quic_initial_dbankcloud_ru.bin"),
    ("fake_http_max", "@bin/tls_clienthello_max_ru.bin"),
    ("fake_quic", "@bin/fake_quic.bin"),
    ("fake_quic_1", "@bin/fake_quic_1.bin"),
    ("fake_quic_2", "@bin/fake_quic_2.bin"),
    ("fake_quic_3", "@bin/fake_quic_3.bin"),
    ("fake_stun", "@bin/stun.bin"),
    ("fake_stun_as_tls", "@bin/stun.bin"),
    ("fake_tls", "@bin/fake_tls_1.bin"),
    ("fake_tls_1", "@bin/fake_tls_1.bin"),
    ("fake_tls_2", "@bin/fake_tls_2.bin"),
    ("fake_tls_3", "@bin/fake_tls_3.bin"),
    ("fake_tls_4", "@bin/fake_tls_4.bin"),
    ("fake_tls_5", "@bin/fake_tls_5.bin"),
    ("fake_tls_6", "@bin/fake_tls_6.bin"),
    ("fake_tls_7", "@bin/fake_tls_7.bin"),
    ("fake_tls_8", "@bin/fake_tls_8.bin"),
    ("game_active", "@bin/ACTIVE_GAME_UDP.bin"),
    ("http_max", "@bin/tls_clienthello_max_ru.bin"),
    ("http_req", "@bin/http_iana_org.bin"),
    ("quic1", "@bin/quic_1.bin"),
    ("quic2", "@bin/quic_2.bin"),
    ("quic3", "@bin/quic_3.bin"),
    ("quic4", "@bin/quic_4.bin"),
    ("quic5", "@bin/quic_5.bin"),
    ("quic6", "@bin/quic_6.bin"),
    ("quic7", "@bin/quic_7.bin"),
    ("quic_4pda", "@bin/quic_initial_4pda.to.bin"),
    ("quic_google", "@bin/quic_initial_www_google_com.bin"),
    ("quic_test", "@bin/quic_test_00.bin"),
    ("quic_vk", "@bin/quic_initial_vk_com.bin"),
    ("stun2", "@bin/stun2.bin"),
    ("stun_pat", "@bin/stun.bin"),
    ("syn_packet", "@bin/syn_packet.bin"),
    ("syndata3", "@bin/tls_clienthello_3.bin"),
    ("tls1", "@bin/tls_clienthello_1.bin"),
    ("tls10", "@bin/tls_clienthello_10.bin"),
    ("tls11", "@bin/tls_clienthello_11.bin"),
    ("tls12", "@bin/tls_clienthello_12.bin"),
    ("tls13", "@bin/tls_clienthello_13.bin"),
    ("tls14", "@bin/tls_clienthello_14.bin"),
    ("tls17", "@bin/tls_clienthello_17.bin"),
    ("tls18", "@bin/tls_clienthello_18.bin"),
    ("tls2", "@bin/tls_clienthello_2.bin"),
    ("tls2n", "@bin/tls_clienthello_2n.bin"),
    ("tls3", "@bin/tls_clienthello_3.bin"),
    ("tls4", "@bin/tls_clienthello_4.bin"),
    ("tls5", "@bin/tls_clienthello_5.bin"),
    ("tls6", "@bin/tls_clienthello_6.bin"),
    ("tls7", "@bin/tls_clienthello_7.bin"),
    ("tls8", "@bin/tls_clienthello_8.bin"),
    ("tls9", "@bin/tls_clienthello_9.bin"),
    ("tls_4pda", "@bin/tls_clienthello_4pda_to.bin"),
    ("tls_deepseek", "@bin/tls_clienthello_chat_deepseek_com.bin"),
    ("tls_google", "@bin/tls_clienthello_www_google_com.bin"),
    ("tls_gosuslugi", "@bin/tls_clienthello_gosuslugi_ru.bin"),
    ("tls_iana", "@bin/tls_clienthello_iana_org.bin"),
    ("tls_max", "@bin/tls_clienthello_max_ru.bin"),
    ("tls_sber", "@bin/tls_clienthello_sberbank_ru.bin"),
    ("tls_sber_v2", "@bin/tls_clienthello_sberbank_ru_v2.bin"),
    ("tls_sochi", "@bin/tls_clienthello_sochi_park.bin"),
    ("tls_stun", "@bin/stun.bin"),
    ("tls_vk", "@bin/tls_clienthello_vk_com.bin"),
    ("tls_vk_kyber", "@bin/tls_clienthello_vk_com_kyber.bin"),
    ("zero256", "@bin/zero_256.bin"),
    ("fake_default_udp", "0x00000000000000000000000000000000"),
    ("hex_00", "0x00"),
    ("hex_0e0e0f0e", "0x0E0E0F0E"),
    ("hex_0f0e0e0f", "0x0F0E0E0F"),
    ("hex_0f0f0f0f", "0x0F0F0F0F"),
    ("tls_zero4", "0x00000000"),
    // zero64 = "0x" + "00" * 64
    (
        "zero64",
        concat!("0x",
            "00000000000000000000000000000000",
            "00000000000000000000000000000000",
            "00000000000000000000000000000000",
            "00000000000000000000000000000000",
        ),
    ),
];

/// Provided by winws2 itself; declaring them breaks the start.
pub const BUILTIN_BLOBS: [&str; 5] =
    ["fake_default_tls", "fake_default_quic", "fake_default_http", "fake_unknown_256", "fake_zero64"];

/// desync function -> lua file defining it (functions outside the core lua).
pub const LUA_EXTENSION_FUNCS: [(&str, &str); 18] = [
    ("discord_ecn_exploit", "custom_funcs.lua"),
    ("discord_router_alert", "custom_funcs.lua"),
    ("discord_timestamp_travel", "custom_funcs.lua"),
    ("discord_ultimate_combo", "custom_funcs.lua"),
    ("discord_urgent_sni", "custom_funcs.lua"),
    ("discord_window_collapse", "custom_funcs.lua"),
    ("fakemultidisorder", "fakemultidisorder.lua"),
    ("fakemultisplit", "fakemultisplit.lua"),
    ("hostfakesplit_multi", "zapret-multishake.lua"),
    ("multisplit_tls", "custom_funcs.lua"),
    ("tls_aggressive", "custom_funcs.lua"),
    ("tls_disorder_gentle", "custom_funcs.lua"),
    ("tls_fake_disorder_gentle", "custom_funcs.lua"),
    ("tls_fake_flood", "custom_funcs.lua"),
    ("tls_fake_simple", "custom_funcs.lua"),
    ("tls_fake_split", "custom_funcs.lua"),
    ("tls_multisplit_sni", "custom_funcs.lua"),
    ("tls_split_gentle", "custom_funcs.lua"),
];

pub fn blob_registry_source(name: &str) -> Option<&'static str> {
    BLOB_REGISTRY
        .iter()
        .find(|(blob, _)| *blob == name)
        .map(|(_, source)| *source)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Strategy {
    pub strategy_id: String,
    pub transport: String,
    pub name: String,
    pub args: Vec<String>,
    pub description: String,
    pub author: String,
    pub label: String,
}

impl Strategy {
    pub fn is_pass(&self) -> bool {
        self.args
            .iter()
            .filter(|arg| arg.starts_with("--lua-desync="))
            .all(|arg| arg.split(':').next().unwrap_or("") == "--lua-desync=pass")
    }
}

fn blob_reference_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("(?:blob|pattern)=([A-Za-z0-9_]+)").unwrap())
}

fn desync_func_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("(?i)--lua-desync=([a-z0-9_]+)").unwrap())
}

/// Blob names referenced by the arguments (builtins and hex literals excluded).
pub fn blob_names_in_args(args: &[String]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for argument in args {
        for capture in blob_reference_re().captures_iter(argument) {
            let Some(name) = capture.get(1) else { continue };
            let name = name.as_str().to_string();
            if name.to_lowercase().starts_with("0x") || BUILTIN_BLOBS.contains(&name.as_str()) {
                continue;
            }
            if !found.contains(&name) {
                found.push(name);
            }
        }
    }
    found
}

pub fn unresolved_blob_names(args: &[String]) -> Vec<String> {
    blob_names_in_args(args)
        .into_iter()
        .filter(|name| blob_registry_source(name).is_none())
        .collect()
}

/// Extension `--lua-init` options needed by the desync functions in `args`.
pub fn lua_init_arguments(args: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for argument in args {
        for func in desync_func_re().find_iter(argument) {
            let func = func.as_str().to_lowercase();
            let tail = func.strip_prefix("--lua-desync=").unwrap_or(&func);
            if let Some((_, lua_file)) = LUA_EXTENSION_FUNCS.iter().find(|(name, _)| *name == tail) {
                let init = format!("--lua-init=@lua/{lua_file}");
                if !seen.contains(&init) {
                    seen.push(init);
                }
            }
        }
    }
    seen
}

/// Process-wide options a strategy needs: core + extension lua, blob definitions.
pub fn strategy_global_args(args: &[String]) -> Vec<String> {
    let mut result: Vec<String> = CORE_LUA_INITS.iter().map(|s| s.to_string()).collect();
    for init in lua_init_arguments(args) {
        if !result.contains(&init) {
            result.push(init);
        }
    }
    for name in blob_names_in_args(args) {
        if let Some(source) = blob_registry_source(&name) {
            result.push(format!("--blob={name}:{source}"));
        }
    }
    result
}

/// Validated strategy lines; `Err` for anything outside a profile body.
pub fn parse_strategy_lines(text: &str) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let prefix = STRATEGY_ARGUMENTS
            .iter()
            .find(|item| line.starts_with(**item))
            .copied()
            .unwrap_or("");
        let value = if prefix.is_empty() { "" } else { &line[prefix.len()..] };
        if prefix.is_empty() || value.trim().is_empty() || line.chars().any(|c| c.is_whitespace()) {
            return Err("only --lua-desync=, --payload=, --out-range=, --in-range= lines are allowed".to_string());
        }
        args.push(line.to_string());
    }
    if !args.iter().any(|arg| arg.starts_with("--lua-desync=")) {
        return Err("no --lua-desync line".to_string());
    }
    Ok(args)
}

/// Catalog title without emoji and pictographs.
pub fn clean_strategy_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| {
            !matches!(
                get_general_category(*c),
                GeneralCategory::OtherSymbol
                    | GeneralCategory::ModifierSymbol
                    | GeneralCategory::Surrogate
                    | GeneralCategory::PrivateUse
                    | GeneralCategory::Format
            ) && !(0xFE00..=0xFE0F).contains(&(*c as u32))
        })
        .collect();
    // re.sub(r"\s+", " ", kept).strip(" -\u2014\u00b7")
    let mut collapsed = String::new();
    let mut pending_space = false;
    for c in kept.chars() {
        if c.is_whitespace() {
            pending_space = !collapsed.is_empty();
        } else {
            if pending_space {
                collapsed.push(' ');
                pending_space = false;
            }
            collapsed.push(c);
        }
    }
    collapsed
        .trim_matches(|c| c == ' ' || c == '-' || c == '\u{2014}' || c == '\u{b7}')
        .to_string()
}

/// `[id]` sections of `key = value` metadata followed by winws2 lines.
fn parse_catalog(path: &Path, transport: &str) -> Vec<(String, Strategy)> {
    let text = match std::fs::read_to_string(path) {
        // Python reads with errors="replace"; lossy matches that behavior.
        Ok(text) => text,
        Err(_) => String::from_utf8_lossy(&std::fs::read(path).unwrap_or_default()).into_owned(),
    };
    let mut sections: Vec<(String, HashMap<String, String>, Vec<String>)> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            sections.push((line[1..line.len() - 1].trim().to_string(), HashMap::new(), Vec::new()));
        } else if sections.is_empty() {
            continue;
        } else if line.starts_with("--") {
            sections.last_mut().unwrap().2.push(line.to_string());
        } else if let Some((key, value)) = line.split_once('=') {
            sections
                .last_mut()
                .unwrap()
                .1
                .insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }
    let mut result: Vec<(String, Strategy)> = Vec::new();
    let mut rejected = 0usize;
    for (strategy_id, meta, lines) in sections {
        if strategy_id.is_empty() || lines.is_empty() {
            continue;
        }
        let args = match parse_strategy_lines(&lines.join("\n")) {
            Ok(args) => args,
            Err(_) => {
                rejected += 1;
                continue;
            }
        };
        if !unresolved_blob_names(&args).is_empty() {
            rejected += 1;
            continue;
        }
        let meta_get = |key: &str| meta.get(key).cloned().unwrap_or_default();
        result.push((
            strategy_id.clone(),
            Strategy {
                strategy_id: strategy_id.clone(),
                transport: transport.to_string(),
                name: {
                    let name = clean_strategy_name(&meta_get("name"));
                    if name.is_empty() { strategy_id.clone() } else { name }
                },
                args,
                description: meta_get("description"),
                author: meta_get("author"),
                label: meta_get("label").trim().to_lowercase(),
            },
        ));
    }
    if rejected > 0 {
        crate::logging::Logger::named("xray_fluent.zapret_strategies").warning(&format!(
            "zapret catalog {}: skipped {rejected} entries",
            path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        ));
    }
    result
}

/// Upstream catalog merged with local overrides; insertion order preserved.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: Vec<(String, Strategy)>,
}

impl Catalog {
    pub fn get(&self, id: &str) -> Option<&Strategy> {
        self.entries.iter().find(|(key, _)| key == id).map(|(_, strategy)| strategy)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn ids(&self) -> Vec<String> {
        self.entries.iter().map(|(key, _)| key.clone()).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Strategy)> {
        self.entries.iter().map(|(key, strategy)| (key, strategy))
    }
}

struct CachedCatalog {
    signature: Vec<(String, u128, u64)>,
    catalog: Arc<Catalog>,
}

static CATALOG_CACHE: OnceLock<parking_lot::Mutex<HashMap<String, CachedCatalog>>> = OnceLock::new();

fn cache() -> &'static parking_lot::Mutex<HashMap<String, CachedCatalog>> {
    CATALOG_CACHE.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

/// Upstream catalog merged with local overrides; cached on file mtime/size.
pub fn load_catalog(transport: &str, catalog_dir: Option<&Path>) -> Arc<Catalog> {
    let root: PathBuf = catalog_dir.map(Path::to_path_buf).unwrap_or_else(crate::zapret_strategies::catalog_dir);
    let paths = [
        root.join(format!("{transport}.txt")),
        root.join(format!("{transport}.local.txt")),
    ];
    let mut signature: Vec<(String, u128, u64)> = vec![(root.to_string_lossy().into_owned(), 0, 0)];
    for path in &paths {
        match std::fs::metadata(path) {
            Ok(meta) => {
                let nanos = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                signature.push((path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), nanos, meta.len()));
            }
            Err(_) => continue,
        }
    }
    let key = format!("{}|{transport}", root.to_string_lossy());
    {
        let cache = cache().lock();
        if let Some(cached) = cache.get(&key) {
            if cached.signature == signature {
                return Arc::clone(&cached.catalog);
            }
        }
    }
    let mut merged: Vec<(String, Strategy)> = Vec::new();
    for path in &paths {
        for (id, strategy) in parse_catalog(path, transport) {
            // Python dict.update keeps the original position of existing keys.
            match merged.iter_mut().find(|(existing, _)| *existing == id) {
                Some((_, existing_strategy)) => *existing_strategy = strategy,
                None => merged.push((id, strategy)),
            }
        }
    }
    let mut name_counts: HashMap<String, usize> = HashMap::new();
    for (_, strategy) in &merged {
        let key = strategy.name.to_lowercase();
        *name_counts.entry(key).or_insert(0) += 1;
    }
    for (id, strategy) in merged.iter_mut() {
        if name_counts.get(&strategy.name.to_lowercase()).copied().unwrap_or(0) > 1 {
            strategy.name = format!("{} ({})", strategy.name, id);
        }
    }
    let catalog = Arc::new(Catalog { entries: merged });
    cache().lock().insert(
        key,
        CachedCatalog { signature, catalog: Arc::clone(&catalog) },
    );
    catalog
}

pub fn kind_catalog(kind: &str, catalog_dir: Option<&Path>) -> Arc<Catalog> {
    load_catalog(kind_transport(kind), catalog_dir)
}

/// Binding value of a server that uses a catalog strategy: `@server` (the
/// default strategy of its kind) or `@server:<strategy id>`.
pub const SERVER_STRATEGY_PRESET: &str = "@server";

pub fn is_server_strategy(preset: &str) -> bool {
    let value = preset.trim();
    value == SERVER_STRATEGY_PRESET || value.starts_with(&format!("{SERVER_STRATEGY_PRESET}:"))
}

/// Strategy id stored in a `@server:<id>` binding ("" for the default).
pub fn server_strategy_id(preset: &str) -> String {
    let value = preset.trim();
    match value.strip_prefix(&format!("{SERVER_STRATEGY_PRESET}:")) {
        Some(id) => id.trim().to_string(),
        None => String::new(),
    }
}

pub fn server_strategy_preset(strategy_id: &str) -> String {
    let sid = strategy_id.trim();
    if sid.is_empty() {
        SERVER_STRATEGY_PRESET.to_string()
    } else {
        format!("{SERVER_STRATEGY_PRESET}:{sid}")
    }
}

/// Chosen strategy of a server kind; the default one when none (or a vanished
/// one) is chosen.
pub fn strategy_for(kind: &str, strategy_id: &str, catalog_dir: Option<&Path>) -> Option<Strategy> {
    let catalog = kind_catalog(kind, catalog_dir);
    let chosen = catalog.get(strategy_id.trim()).map(|s| s.clone());
    chosen.or_else(|| catalog.get(default_strategy(kind)).map(|s| s.clone()))
}

pub fn strategy_sort_key(strategy: &Strategy) -> (usize, String) {
    let rank = STRATEGY_LABELS
        .iter()
        .position(|label| *label == strategy.label)
        .unwrap_or(STRATEGY_LABELS.len());
    (rank, strategy.name.to_lowercase())
}

/// `tcp` / `quic` / `wireguard`: which strategy family fits the node.
pub fn node_server_kind(node: &crate::models::Node) -> String {
    let outbound = &node.outbound;
    let mut protocol = outbound
        .get("protocol")
        .and_then(Value::as_str)
        .or_else(|| outbound.get("type").and_then(Value::as_str))
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let network = outbound
        .get("streamSettings")
        .and_then(|stream| stream.get("network"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_lowercase();
    if protocol.is_empty() {
        protocol = node.scheme.trim().to_lowercase();
    }
    if WIREGUARD_PROTOCOLS.contains(&protocol.as_str()) {
        return "wireguard".to_string();
    }
    if QUIC_PROTOCOLS.contains(&protocol.as_str()) || UDP_NETWORKS.contains(&network.as_str()) {
        return "quic".to_string();
    }
    "tcp".to_string()
}

/// Registry entries whose `@bin` file is not shipped.
pub fn missing_blob_files() -> Vec<String> {
    missing_blob_files_in(&bin_dir())
}

fn missing_blob_files_in(bin_root: &Path) -> Vec<String> {
    let mut missing: Vec<String> = Vec::new();
    let mut names: Vec<&str> = BLOB_REGISTRY.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    for name in names {
        if let Some(source) = blob_registry_source(name) {
            if let Some(file) = source.strip_prefix("@bin/") {
                if !bin_root.join(file).is_file() {
                    missing.push(name.to_string());
                }
            }
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Repo checkout of the shipped catalogs (tests run from target/debug,
    /// where the installed `zapret/` tree is absent).
    fn shipped_catalog_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../windows/zapret/strategy_catalogs/winws2")
    }

    fn shipped_bin_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../windows/zapret/bin")
    }

    fn write_catalog(dir: &Path, transport: &str, text: &str, local: &str) {
        fs::write(dir.join(format!("{transport}.txt")), text).unwrap();
        if !local.is_empty() {
            fs::write(dir.join(format!("{transport}.local.txt")), local).unwrap();
        }
    }

    #[test]
    fn catalog_sections_and_local_override() {
        let dir = std::env::temp_dir().join(format!("lumen-cat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        write_catalog(
            &dir,
            "tcp",
            "# header\n[one]\nname = One\nlabel = Stable\n--lua-desync=multisplit:pos=1\n[two]\n--payload=tls_client_hello\n--lua-desync=fake:blob=tls_google\n",
            "[one]\nname = One local\n--lua-desync=split\n",
        );
        let catalog = load_catalog("tcp", Some(&dir));
        assert_eq!(catalog.ids(), ["one", "two"]);
        assert_eq!(catalog.get("one").unwrap().name, "One local");
        assert_eq!(catalog.get("one").unwrap().args, ["--lua-desync=split"]);
        assert_eq!(catalog.get("two").unwrap().name, "two");
        assert_eq!(catalog.get("two").unwrap().transport, "tcp");
        assert_eq!(
            catalog.get("two").unwrap().args,
            ["--payload=tls_client_hello", "--lua-desync=fake:blob=tls_google"]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn catalog_rejects_unsafe_and_unresolved_entries() {
        let dir = std::env::temp_dir().join(format!("lumen-cat-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        write_catalog(
            &dir,
            "udp",
            "[filter]\n--filter-udp=443\n--lua-desync=fake\n[no_desync]\n--payload=quic_initial\n[unknown_blob]\n--lua-desync=fake:blob=nope_blob\n[ok]\nlabel = recommended\n--lua-desync=fake:blob=quic_google:repeats=6\n",
            "",
        );
        let catalog = load_catalog("udp", Some(&dir));
        assert_eq!(catalog.ids(), ["ok"]);
        assert_eq!(catalog.get("ok").unwrap().label, "recommended");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_without_emoji_and_unique() {
        let dir = std::env::temp_dir().join(format!("lumen-cat-names-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        write_catalog(
            &dir,
            "tcp",
            "[a]\nname = \u{1f3ad}\u{2702}\u{fe0f} TLS Fake + Split\n--lua-desync=split\n[b]\nname = Same\n--lua-desync=split\n[c]\nname = same\n--lua-desync=split\n[d]\nname = \u{1f525}\n--lua-desync=split\n",
            "",
        );
        let catalog = load_catalog("tcp", Some(&dir));
        assert_eq!(catalog.get("a").unwrap().name, "TLS Fake + Split");
        let b = catalog.get("b").unwrap().name.clone();
        let c = catalog.get("c").unwrap().name.clone();
        assert!(b == "Same (b)" && c == "same (c)", "got {b}, {c}");
        assert_eq!(catalog.get("d").unwrap().name, "d");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn shipped_names_are_clean() {
        let dir = shipped_catalog_dir();
        for kind in SERVER_KINDS {
            let catalog = kind_catalog(kind, Some(&dir));
            let names: Vec<&str> = catalog.iter().map(|(_, s)| s.name.as_str()).collect();
            let mut folded: Vec<&str> = names.clone();
            folded.sort_by_key(|name| name.to_lowercase());
            folded.dedup_by_key(|name| name.to_lowercase());
            assert_eq!(folded.len(), names.len());
            for name in names {
                let clean = clean_strategy_name(name);
                assert_eq!(clean, name, "unclean shipped name");
                assert!(!clean.is_empty());
            }
        }
    }

    #[test]
    fn parse_strategy_lines_validation() {
        assert_eq!(
            parse_strategy_lines("# c\n--payload=all\n--lua-desync=pass\n").unwrap(),
            ["--payload=all", "--lua-desync=pass"]
        );
        for bad in ["--lua-desync=fake --new", "--wf-tcp-out=443\n--lua-desync=fake", "--payload=all"] {
            assert!(parse_strategy_lines(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn global_args_declare_lua_and_blobs() {
        let args: Vec<String> = [
            "--lua-desync=fakemultisplit:fake_blob=fake_default_tls:pos=1",
            "--lua-desync=fake:blob=tls_google:repeats=2",
            "--lua-desync=fake:blob=0x00",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(blob_names_in_args(&args), ["tls_google"]);
        let mut expected: Vec<String> = CORE_LUA_INITS.iter().map(|s| s.to_string()).collect();
        expected.push("--lua-init=@lua/fakemultisplit.lua".to_string());
        expected.push("--blob=tls_google:@bin/tls_clienthello_www_google_com.bin".to_string());
        assert_eq!(strategy_global_args(&args), expected);
    }

    #[test]
    fn defaults_exist_in_shipped_catalogs() {
        let dir = shipped_catalog_dir();
        for kind in SERVER_KINDS {
            let strategy = strategy_for(kind, "", Some(&dir)).expect(kind);
            assert_eq!(strategy.strategy_id, default_strategy(kind));
            assert_eq!(strategy.transport, kind_transport(kind));
        }
        assert_eq!(missing_blob_files_in(&shipped_bin_dir()), Vec::<String>::new());
    }

    #[test]
    fn strategy_for_falls_back_to_default() {
        let dir = shipped_catalog_dir();
        assert_eq!(
            strategy_for("tcp", "no-such-id", Some(&dir)).unwrap().strategy_id,
            default_strategy("tcp")
        );
        assert_eq!(strategy_for("tcp", "pass", Some(&dir)).unwrap().strategy_id, "pass");
    }

    #[test]
    fn server_strategy_binding_values() {
        assert_eq!(server_strategy_preset(""), "@server");
        assert_eq!(server_strategy_preset("alt9"), "@server:alt9");
        assert!(is_server_strategy("@server") && is_server_strategy("@server:x"));
        assert!(!is_server_strategy("general.txt") && !is_server_strategy(""));
        assert_eq!(server_strategy_id("@server:alt9"), "alt9");
        assert_eq!(server_strategy_id("@server"), "");
    }

    #[test]
    fn sort_key_orders_by_label() {
        fn item(name: &str, label: &str) -> Strategy {
            Strategy {
                strategy_id: name.to_string(),
                transport: "tcp".to_string(),
                name: name.to_string(),
                args: vec!["--lua-desync=pass".to_string()],
                description: String::new(),
                author: String::new(),
                label: label.to_string(),
            }
        }
        let mut strategies = [
            item("b", ""),
            item("c", "caution"),
            item("a", "stable"),
            item("z", "recommended"),
        ];
        strategies.sort_by_key(strategy_sort_key);
        let names: Vec<&str> = strategies.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["z", "a", "c", "b"]);
    }

    #[test]
    fn node_kinds_map_to_strategy_families() {
        let make = |scheme: &str, outbound: Value| crate::models::Node {
            scheme: scheme.to_string(),
            outbound,
            ..Default::default()
        };
        assert_eq!(
            node_server_kind(&make("vless", serde_json::json!({"protocol": "vless", "streamSettings": {"network": "ws"}}))),
            "tcp"
        );
        assert_eq!(node_server_kind(&make("trojan", serde_json::json!({"type": "trojan"}))), "tcp");
        assert_eq!(node_server_kind(&make("hysteria2", serde_json::json!({"type": "hysteria2"}))), "quic");
        assert_eq!(node_server_kind(&make("tuic", Value::Null)), "quic");
        assert_eq!(
            node_server_kind(&make("vmess", serde_json::json!({"protocol": "vmess", "streamSettings": {"network": "kcp"}}))),
            "quic"
        );
        assert_eq!(node_server_kind(&make("wireguard", serde_json::json!({"type": "wireguard"}))), "wireguard");
        assert_eq!(node_server_kind(&make("awg", Value::Null)), "wireguard");
    }

    #[test]
    fn zero64_blob_has_128_hex_digits() {
        let source = blob_registry_source("zero64").unwrap();
        assert!(source.starts_with("0x"));
        assert_eq!(source[2..].len(), 128);
        assert!(source[2..].chars().all(|c| c == '0'));
    }
}
