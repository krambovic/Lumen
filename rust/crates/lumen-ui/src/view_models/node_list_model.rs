//! GPU-friendly list model for VPN server nodes
//! (port of `qml_app/bridge/node_list_model.py`).
//!
//! The QAbstractListModel roles become plain accessor methods the egui shell
//! reads each frame; the row-change bookkeeping (`_pending_roles` +
//! `flush_pending_changes`) is kept so bulk updates keep coalescing into the
//! same contiguous ranges the QML view consumed.

use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};

use parking_lot::Mutex;

use lumen_app::node_runtime_service::{is_native_singbox_only_node, node_supported_by_core};
use lumen_core::country_flags::{STRIPES, detect_country, get_flag_emoji};
use lumen_core::i18n::tr;
use lumen_core::models::Node;
use lumen_core::node_transport::{node_protocol_label, node_transport};

/// Python `_server_display_name_without_country_prefix` (defined in
/// `app_bridge.py`, mirrored here for the model's NameRole).
pub fn server_display_name_without_country_prefix(name: &str, country: &str) -> String {
    let original = name.trim().to_string();
    let mut cleaned = original.clone();
    let emoji = get_flag_emoji(country);
    if !emoji.is_empty() && cleaned.starts_with(&emoji) {
        cleaned = cleaned[emoji.len()..].trim().to_string();
    }
    let country = country.trim().to_uppercase();
    if country.chars().count() == 2 {
        // Python: ^\s*[\[\(\{]?CODE[\]\)\}]?(?:\s*[-_.|#:]\s*|\s+)  (count=1)
        cleaned = strip_country_prefix(&cleaned, &country);
    }
    if cleaned.is_empty() {
        original
    } else {
        cleaned
    }
}

fn strip_country_prefix(cleaned: &str, country: &str) -> String {
    let trimmed = cleaned.trim_start();
    // `[\[\(\{]?` — the opening bracket is optional in the Python pattern.
    let after_bracket = trimmed
        .strip_prefix('[')
        .or_else(|| trimmed.strip_prefix('('))
        .or_else(|| trimmed.strip_prefix('{'))
        .unwrap_or(trimmed);
    if !after_bracket
        .get(..country.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(country))
    {
        return cleaned.to_string();
    }
    let after_code = &after_bracket[country.len()..];
    // `[\]\)\}]?` — the closing bracket is optional too.
    let after_bracket2 = after_code
        .strip_prefix(']')
        .or_else(|| after_code.strip_prefix(')'))
        .or_else(|| after_code.strip_prefix('}'))
        .unwrap_or(after_code);
    // (?:\s*[-_.|#:]\s*|\s+)
    let separator_chars = ['-', '_', '.', '|', '#', ':'];
    let mut rest = after_bracket2;
    let mut consumed = false;
    if rest.starts_with(char::is_whitespace) {
        rest = rest.trim_start();
        consumed = true;
    }
    if !rest.is_empty() && separator_chars.contains(&rest.chars().next().unwrap()) {
        rest = rest[1..].trim_start();
        consumed = true;
    }
    if consumed {
        rest.trim().to_string()
    } else {
        cleaned.to_string()
    }
}

/// Python `roleNames` — exposed as a role bitmask for the change tracking.
pub mod roles {
    pub const ID: u64 = 1 << 0;
    pub const NAME: u64 = 1 << 1;
    pub const SCHEME: u64 = 1 << 2;
    pub const SERVER: u64 = 1 << 3;
    pub const PORT: u64 = 1 << 4;
    pub const GROUP: u64 = 1 << 5;
    pub const PING: u64 = 1 << 6;
    pub const SPEED: u64 = 1 << 7;
    pub const ALIVE: u64 = 1 << 8;
    pub const COUNTRY: u64 = 1 << 9;
    pub const SELECTED: u64 = 1 << 10;
    pub const SPEED_PROGRESS: u64 = 1 << 11;
    pub const LAST_USED: u64 = 1 << 12;
    pub const RUNTIME_SUPPORTED: u64 = 1 << 13;
    pub const FLAG_EMOJI: u64 = 1 << 14;
    pub const FLAG_SOURCE: u64 = 1 << 15;
    pub const TRANSPORT: u64 = 1 << 16;
    pub const TESTED: u64 = 1 << 17;
    pub const DESCRIPTION: u64 = 1 << 18;
    pub const PINGING: u64 = 1 << 19;
    pub const PING_KIND: u64 = 1 << 20;
    pub const PROTOCOL: u64 = 1 << 21;
    pub const ALL: u64 = (1 << 22) - 1;
}

/// Python `_FLAG_STRIPES[code]` → `(orientation, colors)` with Python's
/// `"h"`/`"v"` orientation markers.
pub fn flag_stripes(code: &str) -> Option<(&'static str, Vec<&'static str>)> {
    let upper = code.trim().to_uppercase();
    STRIPES
        .iter()
        .find(|entry| entry.code == upper)
        .map(|entry| {
            (
                match entry.orientation {
                    lumen_core::country_flags::Orientation::Horizontal => "h",
                    lumen_core::country_flags::Orientation::Vertical => "v",
                    lumen_core::country_flags::Orientation::Nordic => "nordic",
                    lumen_core::country_flags::Orientation::Cross => "cross",
                },
                entry.colors.to_vec(),
            )
        })
}

/// Filterable fields of one row (Python `node_row_at`, used by Ctrl+A so it
/// can select only rows passing the current filter).
#[derive(Debug, Clone, PartialEq)]
pub struct NodeRow {
    pub id: String,
    pub name: String,
    pub server: String,
    pub group: String,
    pub transport: String,
    pub ping_kind: String,
}

/// Display data and live measurements. The controller owns connection
/// configs, import links and measurement histories.
struct ServerRow {
    id: String,
    name: String,
    scheme: String,
    server: String,
    port: i64,
    group: String,
    description: String,
    auto_description: bool,
    transport: String,
    protocol: String,
    ping_ms: Option<i64>,
    ping_kind: String,
    speed_mbps: Option<f64>,
    is_alive: Option<bool>,
    country_code: String,
    last_used_at: Option<String>,
    singbox_only: bool,
    singbox_supported: bool,
}

impl From<&Node> for ServerRow {
    fn from(node: &Node) -> Self {
        let protocol = node.outbound["protocol"].as_str().unwrap_or_default().trim();
        Self {
            id: node.id.clone(),
            name: node.name.clone(),
            scheme: node.scheme.to_uppercase(),
            server: node.server.clone(),
            port: node.port,
            group: node.group.clone(),
            description: node.description.trim().to_owned(),
            auto_description: node.scheme.trim().eq_ignore_ascii_case("auto")
                || protocol.eq_ignore_ascii_case("xray_config"),
            transport: node_transport(node),
            protocol: node_protocol_label(node),
            ping_ms: node.ping_ms,
            ping_kind: node.ping_kind.clone(),
            speed_mbps: node.speed_mbps,
            is_alive: node.is_alive,
            country_code: node.country_code.clone(),
            last_used_at: node.last_used_at.clone(),
            singbox_only: is_native_singbox_only_node(Some(node)),
            singbox_supported: node_supported_by_core(Some(node), "singbox"),
        }
    }
}

/// Python `NodeListModel` (QAbstractListModel) as a plain struct.
#[derive(Default)]
pub struct NodeListModel {
    nodes: Vec<ServerRow>,
    selected_id: Option<String>,
    index_by_id: HashMap<String, usize>,
    speed_progress: HashMap<String, i64>,
    pinging_ids: HashSet<String>,
    country_cache: Mutex<HashMap<String, ((String, String), String)>>,
    allow_native_singbox_only: bool,
    core_preference: String,
    /// Bulk ping/speed runs touch thousands of rows: per-row change roles are
    /// coalesced and flushed as contiguous ranges.
    pending_roles: HashMap<String, u64>,
}

impl NodeListModel {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            selected_id: None,
            index_by_id: HashMap::new(),
            speed_progress: HashMap::new(),
            pinging_ids: HashSet::new(),
            country_cache: Mutex::new(HashMap::new()),
            allow_native_singbox_only: false,
            core_preference: "auto".into(),
            pending_roles: HashMap::new(),
        }
    }

    pub fn row_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn node_ids(&self) -> impl Iterator<Item = &str> {
        self.nodes.iter().map(|node| node.id.as_str())
    }

    fn node_at(&self, row: usize) -> Option<&ServerRow> {
        self.nodes.get(row)
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.selected_id.as_deref()
    }

    pub fn core_preference(&self) -> &str {
        &self.core_preference
    }

    // ── Role accessors (Python `data()` per-role branches) ─────

    pub fn name_at(&self, row: usize) -> String {
        let Some(node) = self.node_at(row) else {
            return String::new();
        };
        let raw_name = if node.name.is_empty() {
            if node.server.is_empty() {
                tr("(без имени)", &[])
            } else {
                node.server.clone()
            }
        } else {
            node.name.clone()
        };
        let country = self.country_for(node);
        server_display_name_without_country_prefix(&raw_name, &country)
    }

    pub fn scheme_at(&self, row: usize) -> String {
        self.node_at(row)
            .map(|node| node.scheme.clone())
            .unwrap_or_default()
    }

    pub fn description_at(&self, row: usize) -> String {
        let Some(node) = self.node_at(row) else {
            return String::new();
        };
        if !node.description.is_empty() {
            return node.description.clone();
        }
        if node.auto_description {
            return tr("Автовыбор лучших серверов", &[]);
        }
        String::new()
    }

    pub fn transport_at(&self, row: usize) -> String {
        self.node_at(row)
            .map(|node| node.transport.clone())
            .unwrap_or_default()
    }

    pub fn server_at(&self, row: usize) -> String {
        self.node_at(row)
            .map(|node| node.server.clone())
            .unwrap_or_default()
    }

    pub fn port_at(&self, row: usize) -> i64 {
        self.node_at(row).map(|node| node.port).unwrap_or(0)
    }

    pub fn group_at(&self, row: usize) -> String {
        self.node_at(row)
            .map(|node| node.group.clone())
            .unwrap_or_default()
    }

    pub fn ping_at(&self, row: usize) -> i64 {
        match self.node_at(row).and_then(|node| node.ping_ms) {
            Some(ping) => ping,
            None => -1,
        }
    }

    pub fn protocol_at(&self, row: usize) -> String {
        self.node_at(row)
            .map(|node| node.protocol.clone())
            .unwrap_or_default()
    }

    pub fn ping_kind_at(&self, row: usize) -> String {
        self.node_at(row)
            .map(|node| node.ping_kind.clone())
            .unwrap_or_default()
    }

    pub fn pinging_at(&self, row: usize) -> bool {
        self.node_at(row)
            .map(|node| self.pinging_ids.contains(&node.id))
            .unwrap_or(false)
    }

    pub fn speed_at(&self, row: usize) -> f64 {
        match self.node_at(row).and_then(|node| node.speed_mbps) {
            Some(speed) => speed,
            None => -1.0,
        }
    }

    pub fn alive_at(&self, row: usize) -> bool {
        self.node_at(row)
            .map(|node| node.is_alive.unwrap_or(false))
            .unwrap_or(false)
    }

    pub fn tested_at(&self, row: usize) -> bool {
        self.node_at(row)
            .map(|node| node.is_alive.is_some())
            .unwrap_or(false)
    }

    pub fn country_at(&self, row: usize) -> String {
        self.node_at(row)
            .map(|node| self.country_for(node).to_lowercase())
            .unwrap_or_default()
    }

    pub fn flag_emoji_at(&self, row: usize) -> String {
        self.node_at(row)
            .map(|node| get_flag_emoji(&self.country_for(node)))
            .unwrap_or_default()
    }

    pub fn flag_source_at(&self, row: usize) -> String {
        self.node_at(row)
            .map(|node| crate::view_models::app_bridge::flag_svg_data_uri(&self.country_for(node)))
            .unwrap_or_default()
    }

    pub fn flag_orient_at(&self, row: usize) -> String {
        self.node_at(row)
            .and_then(|node| flag_stripes(&self.country_for(node)))
            .map(|(orient, _)| orient.to_string())
            .unwrap_or_default()
    }

    pub fn flag_colors_at(&self, row: usize) -> Vec<String> {
        self.node_at(row)
            .and_then(|node| flag_stripes(&self.country_for(node)))
            .map(|(_, colors)| colors.iter().map(|color| color.to_string()).collect())
            .unwrap_or_default()
    }

    pub fn last_used_at(&self, row: usize) -> String {
        self.node_at(row)
            .map(|node| Self::format_last_used(node.last_used_at.as_deref()))
            .unwrap_or_else(|| Self::format_last_used(None))
    }

    pub fn runtime_supported_at(&self, row: usize) -> bool {
        self.node_at(row)
            .map(|node| self.runtime_supported(node))
            .unwrap_or(false)
    }

    pub fn selected_at(&self, row: usize) -> bool {
        self.node_at(row)
            .map(|node| Some(node.id.as_str()) == self.selected_id.as_deref())
            .unwrap_or(false)
    }

    pub fn speed_progress_at(&self, row: usize) -> i64 {
        self.node_at(row)
            .map(|node| self.speed_progress.get(&node.id).copied().unwrap_or(-1))
            .unwrap_or(-1)
    }

    // ── Country detection (cached per node id) ─────────────────

    fn country_for(&self, node: &ServerRow) -> String {
        let code = node.country_code.trim().to_uppercase();
        if !code.is_empty() {
            return code;
        }
        // Nodes are mutable: identity alone cannot invalidate a cached miss.
        let signature = (node.name.clone(), node.server.clone());
        let mut cache = self.country_cache.lock();
        match cache.get(&node.id) {
            Some(cached) if cached.0 == signature => cached.1.clone(),
            _ => {
                let detected = detect_country(&node.name, &node.server);
                cache.insert(node.id.clone(), (signature, detected.clone()));
                detected
            }
        }
    }

    /// Python `_format_last_used`.
    pub fn format_last_used(value: Option<&str>) -> String {
        let Some(value) = value else {
            return "\u{2014}".to_string();
        };
        if value.is_empty() {
            return "\u{2014}".to_string();
        }
        let normalized = value.replace('Z', "+00:00");
        match chrono::DateTime::parse_from_rfc3339(&normalized) {
            Ok(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
            Err(_) => {
                match chrono::NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%dT%H:%M:%S%.f") {
                    Ok(naive) => naive.format("%Y-%m-%d %H:%M").to_string(),
                    Err(_) => value.to_string(),
                }
            }
        }
    }

    // ── Bulk + incremental updates ─────────────────────────────

    /// Python `set_nodes`.
    pub fn set_nodes(
        &mut self,
        nodes: impl IntoIterator<Item = impl Borrow<Node>>,
        selected_id: Option<&str>,
    ) {
        self.nodes = nodes
            .into_iter()
            .map(|node| ServerRow::from(node.borrow()))
            .collect();
        self.selected_id = selected_id.map(str::to_string);
        self.index_by_id = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.clone(), index))
            .collect();
        let live_ids: HashSet<&str> = self.nodes.iter().map(|node| node.id.as_str()).collect();
        self.speed_progress
            .retain(|key, _| live_ids.contains(key.as_str()));
        self.pinging_ids
            .retain(|key| live_ids.contains(key.as_str()));
        self.country_cache
            .lock()
            .retain(|key, _| live_ids.contains(key.as_str()));
    }

    /// Python `set_selected`.
    pub fn set_selected(&mut self, selected_id: Option<&str>) {
        if selected_id.map(str::to_string) == self.selected_id {
            return;
        }
        let previous = self.selected_id.take();
        self.selected_id = selected_id.map(str::to_string);
        for node_id in [previous, self.selected_id.clone()] {
            self.emit_row_changed(node_id.as_deref(), roles::SELECTED);
        }
    }

    fn runtime_supported(&self, node: &ServerRow) -> bool {
        if !self.allow_native_singbox_only && node.singbox_only {
            return false;
        }
        match self.core_preference.as_str() {
            "auto" => true,
            "xray" => !node.singbox_only,
            _ => node.singbox_supported,
        }
    }

    /// Python `set_core_preference` — dim servers the chosen main core cannot
    /// run.
    pub fn set_core_preference(&mut self, preference: &str) {
        let value = if preference.is_empty() {
            "auto"
        } else {
            preference
        };
        if value == self.core_preference {
            return;
        }
        self.core_preference = value.to_string();
    }

    /// Python `set_runtime_support`.
    pub fn set_runtime_support(&mut self, allow_native_singbox_only: bool) {
        self.allow_native_singbox_only = allow_native_singbox_only;
    }

    /// Python `update_ping`.
    pub fn update_ping(&mut self, node_id: &str, ping_ms: Option<i64>) {
        let Some(row) = self.index_by_id.get(node_id).copied() else {
            return;
        };
        self.nodes[row].ping_ms = ping_ms;
        self.pinging_ids.remove(node_id);
        self.queue_row_changed(
            node_id,
            roles::PING | roles::ALIVE | roles::TESTED | roles::PINGING | roles::PING_KIND,
        );
    }

    /// Python `set_pinging_ids`.
    pub fn set_pinging_ids(&mut self, node_ids: impl IntoIterator<Item = String>) {
        let next_ids: HashSet<String> = node_ids.into_iter().filter(|id| !id.is_empty()).collect();
        let changed: HashSet<String> = self
            .pinging_ids
            .symmetric_difference(&next_ids)
            .cloned()
            .collect();
        self.pinging_ids = next_ids;
        for node_id in changed {
            self.queue_row_changed(&node_id, roles::PINGING);
        }
    }

    /// Python `clear_pinging`.
    pub fn clear_pinging(&mut self) {
        let empty: Vec<String> = Vec::new();
        self.set_pinging_ids(empty);
    }

    /// Python `update_speed`.
    pub fn update_speed(&mut self, node_id: &str, speed_mbps: Option<f64>) {
        let Some(row) = self.index_by_id.get(node_id).copied() else {
            return;
        };
        self.nodes[row].speed_mbps = speed_mbps;
        self.speed_progress.remove(node_id);
        self.queue_row_changed(
            node_id,
            roles::SPEED | roles::SPEED_PROGRESS | roles::TESTED,
        );
    }

    /// Python `update_alive`. Python relied on the worker mutating the shared
    /// `Node` object; the Rust model owns its copies, so the health result is
    /// applied here.
    pub fn update_alive(&mut self, node_id: &str, is_alive: bool) {
        let Some(row) = self.index_by_id.get(node_id).copied() else {
            return;
        };
        self.nodes[row].is_alive = Some(is_alive);
        self.queue_row_changed(node_id, roles::ALIVE | roles::TESTED);
    }

    /// Python `update_speed_progress`.
    pub fn update_speed_progress(&mut self, node_id: &str, percent: i64) {
        if !self.index_by_id.contains_key(node_id) {
            return;
        }
        self.speed_progress.insert(node_id.to_string(), percent);
        self.queue_row_changed(node_id, roles::SPEED_PROGRESS);
    }

    /// Python `node_id_at`.
    pub fn node_id_at(&self, row: usize) -> Option<&str> {
        self.nodes.get(row).map(|node| node.id.as_str())
    }

    /// Python `index_of_id`.
    pub fn index_of_id(&self, node_id: &str) -> i64 {
        self.index_by_id
            .get(node_id)
            .copied()
            .map(|index| index as i64)
            .unwrap_or(-1)
    }

    /// Python `node_row_at`.
    pub fn node_row_at(&self, row: usize) -> Option<NodeRow> {
        let node = self.nodes.get(row)?;
        Some(NodeRow {
            id: node.id.clone(),
            name: if node.name.is_empty() {
                if node.server.is_empty() {
                    tr("(без имени)", &[])
                } else {
                    node.server.clone()
                }
            } else {
                node.name.clone()
            },
            server: node.server.clone(),
            group: node.group.clone(),
            transport: node.transport.clone(),
            ping_kind: node.ping_kind.clone(),
        })
    }

    /// Python `best_ping_node_id` — the visible, runnable node with the lowest
    /// measured ping.
    pub fn best_ping_node_id(&self) -> String {
        let mut best_id = String::new();
        let mut best_ping: Option<i64> = None;
        for node in &self.nodes {
            let Some(ping) = node.ping_ms else {
                continue;
            };
            if ping <= 0 || node.is_alive == Some(false) {
                continue;
            }
            if !self.runtime_supported(node) {
                continue;
            }
            if best_ping.is_none_or(|current| ping < current) {
                best_id = node.id.clone();
                best_ping = Some(ping);
            }
        }
        best_id
    }

    // ── Change bookkeeping ─────────────────────────────────────

    /// Python `_queue_row_changed`.
    fn queue_row_changed(&mut self, node_id: &str, role_bits: u64) {
        if !self.index_by_id.contains_key(node_id) {
            return;
        }
        *self.pending_roles.entry(node_id.to_string()).or_insert(0) |= role_bits;
    }

    /// Python `_emit_row_changed` (immediate, un-coalesced).
    fn emit_row_changed(&mut self, node_id: Option<&str>, role_bits: u64) {
        let Some(node_id) = node_id else {
            return;
        };
        if !self.index_by_id.contains_key(node_id) {
            return;
        }
        // egui redraws everything per frame; the immediate path only keeps the
        // role accounting faithful.
        let _ = role_bits;
    }

    /// Python `flush_pending_changes` — emit queued row changes as a few
    /// contiguous ranges. Returns `[(start_row, end_row, roles)]`.
    pub fn flush_pending_changes(&mut self) -> Vec<(usize, usize, u64)> {
        let pending = std::mem::take(&mut self.pending_roles);
        let mut rows: Vec<(usize, u64)> = pending
            .into_iter()
            .filter_map(|(node_id, role_bits)| {
                self.index_by_id.get(&node_id).map(|row| (*row, role_bits))
            })
            .collect();
        rows.sort_by_key(|(row, _)| *row);
        let mut ranges: Vec<(usize, usize, u64)> = Vec::new();
        let mut start: i64 = -1;
        let mut end: i64 = -1;
        let mut merged: u64 = 0;
        for (row, role_bits) in rows {
            if row as i64 == end + 1 && start >= 0 {
                end = row as i64;
                merged |= role_bits;
                continue;
            }
            if start >= 0 {
                ranges.push((start as usize, end as usize, merged));
            }
            start = row as i64;
            end = row as i64;
            merged = role_bits;
        }
        if start >= 0 {
            ranges.push((start as usize, end as usize, merged));
        }
        ranges
    }
}

/// Python `DashboardNodeProxy` (QSortFilterProxyModel) — dashboard view of the
/// server list. Follows the Серверы page order when a sort is chosen there;
/// with manual order it auto-sorts by ping (unknown pings last). Also owns the
/// dashboard search so Ctrl+A can select exactly the rows that are shown.
#[derive(Debug, Default)]
pub struct DashboardNodeProxy {
    sort_key: String,
    text: String,
    ping_sorted: Option<bool>,
}

impl DashboardNodeProxy {
    pub const UNKNOWN_PING: i64 = 1 << 30;

    pub fn new(sort_key: &str) -> Self {
        let mut proxy = Self {
            sort_key: sort_key.to_string(),
            text: String::new(),
            ping_sorted: None,
        };
        proxy.refresh_sort();
        proxy
    }

    /// Python `refresh_sort` — called when the node sort key changes.
    pub fn set_sort_key(&mut self, key: &str) {
        self.sort_key = key.to_owned();
        self.refresh_sort();
    }

    pub fn refresh_sort(&mut self) {
        let ping_sorted = self.sort_key.is_empty() || self.sort_key == "manual";
        if self.ping_sorted == Some(ping_sorted) {
            return;
        }
        self.ping_sorted = Some(ping_sorted);
    }

    /// Python `set_filter_text`.
    pub fn set_filter_text(&mut self, text: &str) {
        let value = text.trim().to_casefold();
        if value != self.text {
            self.text = value;
        }
    }

    /// Python `node_ids` — the visible rows as node ids.
    pub fn node_ids(&self, source: &NodeListModel) -> Vec<String> {
        self.visible_rows(source)
            .into_iter()
            .map(|row| source.node_id_at(row).unwrap_or("").to_string())
            .collect()
    }

    /// The filtered (and, under manual order, ping-sorted) source rows.
    pub fn visible_rows(&self, source: &NodeListModel) -> Vec<usize> {
        let mut rows: Vec<usize> = (0..source.row_count())
            .filter(|row| self.filter_accepts_row(source, *row))
            .collect();
        if self.ping_sorted.unwrap_or(false) {
            let nodes = &source.nodes;
            rows.sort_by(|a, b| {
                let ping = |row: usize| -> i64 {
                    match nodes[row].ping_ms {
                        Some(ping) if ping > 0 => ping,
                        _ => Self::UNKNOWN_PING,
                    }
                };
                ping(*a).cmp(&ping(*b)).then_with(|| a.cmp(b))
            });
        }
        rows
    }

    /// Python `filterAcceptsRow`.
    pub fn filter_accepts_row(&self, source: &NodeListModel, source_row: usize) -> bool {
        if self.text.is_empty() {
            return true;
        }
        let haystack = [
            source.name_at(source_row),
            source.protocol_at(source_row),
            source.group_at(source_row),
            source.country_at(source_row),
            source.server_at(source_row),
        ]
        .join(" ")
        .to_casefold();
        haystack.contains(&self.text)
    }
}

/// Python `str.casefold()` for the filter inputs.
pub trait Casefold {
    fn to_casefold(&self) -> String;
}

impl Casefold for str {
    fn to_casefold(&self) -> String {
        // Rust lacks full Unicode case folding; handle the common sharp-s
        // difference from `lower()` and lowercase the rest.
        self.replace('ß', "ss").replace('ẛ', "ṡ").to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn row_snapshot_keeps_labels_and_core_support_from_a_borrowed_node() {
        let mut node = node("id", "Example", "host.test");
        node.scheme = "naive".into();
        node.outbound = json!({"singbox": {"type": "naive", "quic": true}});
        node.ping_history = vec![("old".into(), Some(25)); 50];
        let mut model = NodeListModel::new();
        model.set_nodes([&node], Some("id"));
        assert_eq!(model.transport_at(0), "QUIC");
        assert_eq!(model.protocol_at(0), "Naive");
        assert!(model.selected_at(0));
        assert!(!model.runtime_supported_at(0));
        model.set_runtime_support(true);
        assert!(model.runtime_supported_at(0));
        model.set_core_preference("xray");
        assert!(!model.runtime_supported_at(0));
    }

    fn node(id: &str, name: &str, server: &str) -> Node {
        let mut node = Node::new();
        node.id = id.to_string();
        node.name = name.to_string();
        node.server = server.to_string();
        node.port = 443;
        node
    }

    #[test]
    fn test_endpoint_ping_does_not_claim_profile_was_tested() {
        let node = node("dead", "dead", "203.0.113.1");
        let mut model = NodeListModel::new();
        model.set_nodes(vec![node.clone()], None);
        assert!(!model.tested_at(0));
        assert!(!model.alive_at(0));

        model.update_ping(&node.id, None);

        assert!(node.is_alive.is_none());
        assert!(!model.tested_at(0));
        assert!(!model.alive_at(0));
        assert_eq!(model.ping_at(0), -1);
    }

    #[test]
    fn test_failed_speed_can_override_previous_alive_status() {
        let mut node = node("slow", "slow", "203.0.113.2");
        node.is_alive = Some(true);
        node.ping_kind = "proxy".into();
        let mut model = NodeListModel::new();
        model.set_nodes(vec![node.clone()], None);

        // The profile worker owns the authoritative health result.
        model.update_speed(&node.id, None);
        model.update_alive(&node.id, false);

        assert!(model.tested_at(0));
        assert!(!model.alive_at(0));
        assert_eq!(model.speed_at(0), -1.0);
    }

    #[test]
    fn test_pinging_role_is_visible_until_the_node_result_arrives() {
        let first = node("first", "first", "203.0.113.10");
        let second = node("second", "second", "203.0.113.11");
        let mut model = NodeListModel::new();
        model.set_nodes(vec![first.clone(), second.clone()], None);

        model.set_pinging_ids([first.id.clone(), second.id.clone()]);
        assert!(model.pinging_at(0));
        assert!(model.pinging_at(1));

        model.update_ping(&first.id, Some(42));
        assert!(!model.pinging_at(0));
        assert!(model.pinging_at(1));

        model.clear_pinging();
        assert!(!model.pinging_at(1));
    }

    #[test]
    fn test_auto_node_without_description_gets_default_description() {
        let mut auto = node("auto", "auto", "");
        auto.scheme = "auto".into();
        let mut xray_auto = node("xray-auto", "xray-auto", "");
        xray_auto.outbound = json!({"protocol": "xray_config"});
        let mut custom = node("custom", "custom", "");
        custom.scheme = "auto".into();
        custom.description = "Провайдер выберет сервер".into();
        let mut model = NodeListModel::new();
        model.set_nodes(vec![auto, xray_auto, custom], None);

        assert_eq!(
            model.description_at(0),
            tr("Автовыбор лучших серверов", &[])
        );
        assert_eq!(
            model.description_at(1),
            tr("Автовыбор лучших серверов", &[])
        );
        assert_eq!(model.description_at(2), "Провайдер выберет сервер");
    }

    #[test]
    fn test_bulk_ping_updates_are_coalesced_into_ranges() {
        let nodes: Vec<Node> = (0..200)
            .map(|index| {
                let mut node = node(
                    &format!("n{index}"),
                    &format!("N{index}"),
                    &format!("h{index}.example"),
                );
                node.ping_ms = Some(10);
                node
            })
            .collect();
        let ids: Vec<String> = nodes.iter().map(|node| node.id.clone()).collect();
        let mut model = NodeListModel::new();
        model.set_nodes(nodes, None);

        for id in &ids {
            model.update_ping(id, None);
        }
        assert!(model.pending_roles.len() > 0);

        let ranges = model.flush_pending_changes();
        assert_eq!(
            ranges,
            vec![(
                0,
                199,
                roles::PING | roles::ALIVE | roles::TESTED | roles::PINGING | roles::PING_KIND
            )]
        );
    }

    #[test]
    fn test_dashboard_proxy_sorts_unknown_pings_last() {
        let mut fast = node("fast", "fast", "1.1.1.1");
        fast.ping_ms = Some(10);
        let mut slow = node("slow", "slow", "1.1.1.2");
        slow.ping_ms = Some(500);
        let mut unknown = node("unknown", "unknown", "1.1.1.3");
        unknown.ping_ms = None;
        let mut model = NodeListModel::new();
        model.set_nodes(vec![unknown, slow, fast], None);

        let mut proxy = DashboardNodeProxy::new("manual");
        assert_eq!(proxy.visible_rows(&model), vec![2, 1, 0]);
        proxy.set_filter_text("SLOW");
        assert_eq!(proxy.visible_rows(&model), vec![1]);
        proxy.set_filter_text("");
        proxy.refresh_sort();
        let proxy2 = DashboardNodeProxy::new("name");
        assert_eq!(proxy2.visible_rows(&model), vec![0, 1, 2]);
    }

    #[test]
    fn test_display_name_strips_country_prefix() {
        assert_eq!(
            server_display_name_without_country_prefix("🇩🇪 DE - Berlin", "DE"),
            "Berlin"
        );
        assert_eq!(
            server_display_name_without_country_prefix("Ru | Москва", "RU"),
            "Москва"
        );
        assert_eq!(
            server_display_name_without_country_prefix("[NL] Amsterdam", "NL"),
            "Amsterdam"
        );
        assert_eq!(
            server_display_name_without_country_prefix("Plain", "RU"),
            "Plain"
        );
        assert_eq!(server_display_name_without_country_prefix("DE", "DE"), "DE");
    }

    #[test]
    fn test_node_row_at_and_index_of_id() {
        let mut model = NodeListModel::new();
        model.set_nodes(
            vec![node("a", "A", "a.example"), node("b", "B", "b.example")],
            Some("b"),
        );
        assert_eq!(model.index_of_id("b"), 1);
        assert_eq!(model.index_of_id("missing"), -1);
        assert_eq!(model.node_id_at(0), Some("a"));
        let row = model.node_row_at(1).unwrap();
        assert_eq!(row.id, "b");
        assert_eq!(row.name, "B");
        assert!(model.selected_at(1));
        assert!(!model.selected_at(0));
    }

    #[test]
    fn test_best_ping_node_id_skips_unsupported_and_dead() {
        let mut alive = node("alive", "alive", "1.1.1.4");
        alive.ping_ms = Some(80);
        alive.is_alive = Some(true);
        let mut dead = node("dead", "dead", "1.1.1.5");
        dead.ping_ms = Some(5);
        dead.is_alive = Some(false);
        let mut unreachable = node("none", "none", "1.1.1.6");
        unreachable.ping_ms = None;
        let mut model = NodeListModel::new();
        model.set_nodes(vec![alive, dead, unreachable], None);
        assert_eq!(model.best_ping_node_id(), "alive");
    }
}
