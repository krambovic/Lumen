//! Localized catalogs (`i18n.py`). Russian is the source language; `en.json`,
//! `fa.json` and `zh.json` provide translations. Supports `{placeholder}`
//! substitution and reverse-translation of rendered Russian messages against
//! catalog templates (backend workers emit finished Russian strings).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::OnceLock;

const SOURCE_LANGUAGE: &str = "ru";

static EN: &str = include_str!("../locales/en.json");
static FA: &str = include_str!("../locales/fa.json");
static ZH: &str = include_str!("../locales/zh.json");

pub struct Catalogs {
    /// language code -> key -> translated text (Russian keys).
    pub maps: BTreeMap<String, BTreeMap<String, String>>,
    pub names: BTreeMap<String, String>,
}

fn parse_catalog(raw: &'static str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(map) = parsed.as_object() {
            for (key, value) in map {
                if key.starts_with("__") {
                    continue;
                }
                if let Some(text) = value.as_str() {
                    out.insert(key.clone(), text.to_string());
                }
            }
        }
    }
    out
}

fn catalog_name(raw: &'static str) -> String {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v.get("__name__").and_then(|n| n.as_str()).map(|s| s.to_string()))
        .unwrap_or_default()
}

fn catalogs() -> &'static Catalogs {
    static CATALOGS: OnceLock<Catalogs> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        let mut maps = BTreeMap::new();
        let mut names = BTreeMap::new();
        names.insert(SOURCE_LANGUAGE.to_string(), "Русский".to_string());
        for (code, raw) in [("en", EN), ("fa", FA), ("zh", ZH)] {
            let map = parse_catalog(raw);
            if map.is_empty() {
                continue;
            }
            let name = catalog_name(raw);
            names.insert(
                code.to_string(),
                if name.is_empty() { code.to_uppercase() } else { name },
            );
            maps.insert(code.to_string(), map);
        }
        Catalogs { maps, names }
    })
}

static LANGUAGE: Mutex<Option<String>> = Mutex::new(None);

pub fn available_languages() -> Vec<String> {
    let catalogs = catalogs();
    let mut out = vec![SOURCE_LANGUAGE.to_string()];
    for code in catalogs.maps.keys() {
        if code != SOURCE_LANGUAGE {
            out.push(code.clone());
        }
    }
    out
}

pub fn language_name(code: &str) -> String {
    catalogs()
        .names
        .get(code)
        .cloned()
        .unwrap_or_else(|| code.to_uppercase())
}

pub fn set_language(lang: &str) -> String {
    let candidate = lang.to_lowercase();
    let resolved = if available_languages().contains(&candidate) {
        candidate
    } else if catalogs().maps.contains_key("en") {
        "en".to_string()
    } else {
        SOURCE_LANGUAGE.to_string()
    };
    *LANGUAGE.lock().unwrap() = Some(resolved.clone());
    resolved
}

pub fn get_language() -> String {
    LANGUAGE
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| "en".to_string())
}

fn catalog_for_language(language: &str) -> &'static BTreeMap<String, String> {
    static EMPTY: BTreeMap<String, String> = BTreeMap::new();
    if language == SOURCE_LANGUAGE {
        return &EMPTY;
    }
    catalogs().maps.get(language).unwrap_or(&EMPTY)
}

/// Borrow the loaded translations for the current language.
pub fn active_catalog() -> &'static BTreeMap<String, String> {
    let language = LANGUAGE.lock().unwrap();
    catalog_for_language(language.as_deref().unwrap_or("en"))
}

pub fn active_map() -> BTreeMap<String, String> {
    active_catalog().clone()
}

/// Translate a Russian source key with optional `{name}` parameters.
pub fn translate(key: &str, params: &[(&str, &str)]) -> String {
    let mut text = active_catalog()
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string());
    if !params.is_empty() {
        for (name, value) in params {
            let placeholder = format!("{{{name}}}");
            text = text.replace(&placeholder, value);
        }
    }
    text
}

pub fn tr(key: &str, params: &[(&str, &str)]) -> String {
    translate(key, params)
}

fn has_cyrillic(text: &str) -> bool {
    text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c))
}

fn count_letters(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphabetic()).count()
}

/// Split a template key into literal parts and field names: `{n} с` ->
/// ["", "n", " с"].
fn split_placeholders(source: &str) -> Vec<(bool, String)> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            let mut closed = false;
            for inner in chars.by_ref() {
                if inner == '}' {
                    closed = true;
                    break;
                }
                name.push(inner);
            }
            if closed && !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                parts.push((false, std::mem::take(&mut literal)));
                parts.push((true, name));
                continue;
            }
            // Not a placeholder; keep the raw text.
            literal.push('{');
            if !closed {
                literal.push_str(&name);
            } else {
                literal.push_str(&name);
                literal.push('}');
            }
        } else {
            literal.push(c);
        }
    }
    parts.push((false, literal));
    parts
}

struct Template {
    /// (literal, field_name) sequence in order.
    segments: Vec<Segment>,
    source: String,
}

enum Segment {
    Literal(String),
    Field(String),
}

fn compile_templates(map: &BTreeMap<String, String>) -> Vec<Template> {
    let mut templates: Vec<Template> = Vec::new();
    let mut keys: Vec<&String> = map.keys().filter(|k| k.contains('{')).collect();
    keys.sort_by(|a, b| b.len().cmp(&a.len()));
    for key in keys {
        let parts = split_placeholders(key);
        let literal_text: String = parts
            .iter()
            .filter(|(is_field, _)| !is_field)
            .map(|(_, text)| text.as_str())
            .collect();
        if count_letters(&literal_text) < 3 {
            continue;
        }
        let segments: Vec<Segment> = parts
            .into_iter()
            .map(|(is_field, text)| {
                if is_field {
                    Segment::Field(text)
                } else {
                    Segment::Literal(text)
                }
            })
            .collect();
        templates.push(Template {
            segments,
            source: key.clone(),
        });
    }
    templates
}

/// Translate a rendered message by matching catalog keys with placeholders.
pub fn translate_dynamic(message: &str) -> Option<String> {
    let map = active_catalog();
    if message.is_empty() || map.is_empty() {
        return None;
    }
    translate_dynamic_in(message, map, 0)
}

fn translate_dynamic_in(message: &str, map: &BTreeMap<String, String>, depth: usize) -> Option<String> {
    if let Some(exact) = map.get(message) {
        return Some(exact.clone());
    }
    if !has_cyrillic(message) {
        return None;
    }
    for template in compile_templates(map) {
        if let Some(params) = match_template(&template, message) {
            let mut params = params;
            for (name, value) in params.clone() {
                if depth < 2 && !value.is_empty() && has_cyrillic(&value) {
                    if let Some(localized) = translate_dynamic_in(&value, map, depth + 1) {
                        params.insert(name, localized);
                    }
                }
            }
            let translated = map.get(&template.source)?;
            let parts = split_placeholders(translated);
            let mut out = String::new();
            for (is_field, text) in &parts {
                if *is_field {
                    match params.get(text) {
                        Some(value) => out.push_str(value),
                        None => out.push_str(&format!("{{{text}}}")),
                    }
                } else {
                    out.push_str(text);
                }
            }
            return Some(out);
        }
    }
    None
}

fn match_template(template: &Template, message: &str) -> Option<BTreeMap<String, String>> {
    // Match literals in order; each field captures everything up to the next
    // literal (fullmatch semantics: trailing field consumes the remainder).
    let mut params = BTreeMap::new();
    let mut rest = message;
    let mut segments = template.segments.iter().peekable();
    while let Some(segment) = segments.next() {
        match segment {
            Segment::Literal(literal) => {
                if !rest.starts_with(literal.as_str()) {
                    return None;
                }
                rest = &rest[literal.len()..];
            }
            Segment::Field(name) => {
                let next_literal = match segments.peek() {
                    Some(Segment::Literal(l)) => l.as_str(),
                    _ => "",
                };
                let captured = if next_literal.is_empty() {
                    rest
                } else {
                    match rest.find(next_literal) {
                        Some(pos) => &rest[..pos],
                        None => return None,
                    }
                };
                params
                    .entry(name.clone())
                    .or_insert_with(|| captured.to_string());
                rest = &rest[captured.len()..];
            }
        }
    }
    if !rest.is_empty() {
        return None;
    }
    Some(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogs_reuse_the_loaded_translations() {
        let english = catalog_for_language("en");
        assert!(std::ptr::eq(english, &catalogs().maps["en"]));
        assert!(!english.is_empty());
        assert!(catalog_for_language(SOURCE_LANGUAGE).is_empty());
    }

    #[test]
    fn placeholder_substitution() {
        set_language("en");
        let text = translate("Nonexistent key", &[("n", "5")]);
        assert_eq!(text, "Nonexistent key");
        set_language("ru");
    }

    #[test]
    fn template_split() {
        let parts = split_placeholders("sing-box актуален ({version})");
        assert_eq!(parts.len(), 3);
    }
}
