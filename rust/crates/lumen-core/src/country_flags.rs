//! Country detection from node name/server and flag rendering
//! (`country_flags.py`). Flags are painted as small stripe textures.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::OnceLock;

use image::{ImageBuffer, Rgba};
use serde_json::{json, Value};

pub const FLAG_WIDTH: u32 = 20;
pub const FLAG_HEIGHT: u32 = 14;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Orientation {
    Horizontal,
    Vertical,
    Nordic,
    Cross,
}

#[derive(Debug, Clone, Copy)]
pub struct FlagStripes {
    pub code: &'static str,
    pub orientation: Orientation,
    pub colors: &'static [&'static str],
}

pub static STRIPES: &[FlagStripes] = &[
    FlagStripes { code: "RU", orientation: Orientation::Horizontal, colors: &["#FFFFFF", "#0039A6", "#D52B1E"] },
    FlagStripes { code: "DE", orientation: Orientation::Horizontal, colors: &["#000000", "#DD0000", "#FFCC00"] },
    FlagStripes { code: "NL", orientation: Orientation::Horizontal, colors: &["#AE1C28", "#FFFFFF", "#21468B"] },
    FlagStripes { code: "LU", orientation: Orientation::Horizontal, colors: &["#ED2939", "#FFFFFF", "#00A1DE"] },
    FlagStripes { code: "AT", orientation: Orientation::Horizontal, colors: &["#ED2939", "#FFFFFF", "#ED2939"] },
    FlagStripes { code: "HU", orientation: Orientation::Horizontal, colors: &["#CE2939", "#FFFFFF", "#477050"] },
    FlagStripes { code: "BG", orientation: Orientation::Horizontal, colors: &["#FFFFFF", "#00966E", "#D62612"] },
    FlagStripes { code: "LT", orientation: Orientation::Horizontal, colors: &["#FDB913", "#006A44", "#C1272D"] },
    FlagStripes { code: "EE", orientation: Orientation::Horizontal, colors: &["#0072CE", "#000000", "#FFFFFF"] },
    FlagStripes { code: "LV", orientation: Orientation::Horizontal, colors: &["#9E3039", "#FFFFFF", "#9E3039"] },
    FlagStripes { code: "HR", orientation: Orientation::Horizontal, colors: &["#FF0000", "#FFFFFF", "#171796"] },
    FlagStripes { code: "RS", orientation: Orientation::Horizontal, colors: &["#C7363C", "#0C4076", "#FFFFFF"] },
    FlagStripes { code: "SI", orientation: Orientation::Horizontal, colors: &["#FFFFFF", "#003DA5", "#ED1C24"] },
    FlagStripes { code: "AM", orientation: Orientation::Horizontal, colors: &["#D90012", "#0033A0", "#F2A800"] },
    FlagStripes { code: "AZ", orientation: Orientation::Horizontal, colors: &["#00B5E2", "#DD0000", "#00B532"] },
    FlagStripes { code: "CO", orientation: Orientation::Horizontal, colors: &["#FCD116", "#003893", "#CE1126"] },
    FlagStripes { code: "AR", orientation: Orientation::Horizontal, colors: &["#74ACDF", "#FFFFFF", "#74ACDF"] },
    FlagStripes { code: "IN", orientation: Orientation::Horizontal, colors: &["#FF9933", "#FFFFFF", "#138808"] },
    FlagStripes { code: "EG", orientation: Orientation::Horizontal, colors: &["#CE1126", "#FFFFFF", "#000000"] },
    FlagStripes { code: "IR", orientation: Orientation::Horizontal, colors: &["#239F40", "#FFFFFF", "#DA0000"] },
    FlagStripes { code: "BO", orientation: Orientation::Horizontal, colors: &["#D52B1E", "#F9E300", "#007934"] },
    FlagStripes { code: "ET", orientation: Orientation::Horizontal, colors: &["#009A44", "#FCDD09", "#DA121A"] },
    FlagStripes { code: "YE", orientation: Orientation::Horizontal, colors: &["#CE1126", "#FFFFFF", "#000000"] },
    FlagStripes { code: "IQ", orientation: Orientation::Horizontal, colors: &["#CE1126", "#FFFFFF", "#000000"] },
    FlagStripes { code: "TJ", orientation: Orientation::Horizontal, colors: &["#CC0000", "#FFFFFF", "#006600"] },
    FlagStripes { code: "GH", orientation: Orientation::Horizontal, colors: &["#CF0921", "#FCD116", "#006B3F"] },
    FlagStripes { code: "UA", orientation: Orientation::Horizontal, colors: &["#005BBB", "#FFD500"] },
    FlagStripes { code: "PL", orientation: Orientation::Horizontal, colors: &["#FFFFFF", "#DC143C"] },
    FlagStripes { code: "ID", orientation: Orientation::Horizontal, colors: &["#FF0000", "#FFFFFF"] },
    FlagStripes { code: "MC", orientation: Orientation::Horizontal, colors: &["#CE1126", "#FFFFFF"] },
    FlagStripes { code: "SG", orientation: Orientation::Horizontal, colors: &["#EF3340", "#FFFFFF"] },
    FlagStripes { code: "FR", orientation: Orientation::Vertical, colors: &["#002395", "#FFFFFF", "#ED2939"] },
    FlagStripes { code: "IT", orientation: Orientation::Vertical, colors: &["#009246", "#FFFFFF", "#CE2B37"] },
    FlagStripes { code: "IE", orientation: Orientation::Vertical, colors: &["#009A49", "#FFFFFF", "#FF7900"] },
    FlagStripes { code: "BE", orientation: Orientation::Vertical, colors: &["#000000", "#FAE042", "#ED2939"] },
    FlagStripes { code: "RO", orientation: Orientation::Vertical, colors: &["#002B7F", "#FCD116", "#CE1126"] },
    FlagStripes { code: "MD", orientation: Orientation::Vertical, colors: &["#003DA5", "#FCD116", "#CC0000"] },
    FlagStripes { code: "MX", orientation: Orientation::Vertical, colors: &["#006847", "#FFFFFF", "#CE1126"] },
    FlagStripes { code: "NG", orientation: Orientation::Vertical, colors: &["#008751", "#FFFFFF", "#008751"] },
    FlagStripes { code: "CI", orientation: Orientation::Vertical, colors: &["#F77F00", "#FFFFFF", "#009E60"] },
    FlagStripes { code: "PE", orientation: Orientation::Vertical, colors: &["#D91023", "#FFFFFF", "#D91023"] },
    FlagStripes { code: "CA", orientation: Orientation::Vertical, colors: &["#FF0000", "#FFFFFF", "#FF0000"] },
    FlagStripes { code: "PT", orientation: Orientation::Vertical, colors: &["#006600", "#FF0000", "#FF0000"] },
    FlagStripes { code: "SE", orientation: Orientation::Nordic, colors: &["#006AA7", "#FECC02"] },
    FlagStripes { code: "NO", orientation: Orientation::Nordic, colors: &["#EF2B2D", "#FFFFFF", "#002868"] },
    FlagStripes { code: "FI", orientation: Orientation::Nordic, colors: &["#FFFFFF", "#003580"] },
    FlagStripes { code: "DK", orientation: Orientation::Nordic, colors: &["#C60C30", "#FFFFFF"] },
    FlagStripes { code: "IS", orientation: Orientation::Nordic, colors: &["#003897", "#FFFFFF", "#D72828"] },
    FlagStripes { code: "CH", orientation: Orientation::Cross, colors: &["#FF0000", "#FFFFFF"] },
    FlagStripes { code: "GR", orientation: Orientation::Horizontal, colors: &["#0D5EAF", "#FFFFFF", "#0D5EAF"] },
    FlagStripes { code: "GE", orientation: Orientation::Cross, colors: &["#FFFFFF", "#FF0000"] },
    FlagStripes { code: "US", orientation: Orientation::Horizontal, colors: &["#3C3B6E", "#FFFFFF", "#B22234"] },
    FlagStripes { code: "GB", orientation: Orientation::Horizontal, colors: &["#012169", "#FFFFFF", "#C8102E"] },
    FlagStripes { code: "JP", orientation: Orientation::Horizontal, colors: &["#FFFFFF", "#BC002D", "#FFFFFF"] },
    FlagStripes { code: "CN", orientation: Orientation::Horizontal, colors: &["#DE2910", "#DE2910", "#FFDE00"] },
    FlagStripes { code: "TW", orientation: Orientation::Horizontal, colors: &["#000095", "#FE0000", "#FE0000"] },
    FlagStripes { code: "HK", orientation: Orientation::Horizontal, colors: &["#DE2910", "#FFFFFF", "#DE2910"] },
    FlagStripes { code: "KR", orientation: Orientation::Horizontal, colors: &["#FFFFFF", "#CD2E3A", "#003478"] },
    FlagStripes { code: "TR", orientation: Orientation::Horizontal, colors: &["#E30A17", "#FFFFFF", "#E30A17"] },
    FlagStripes { code: "IL", orientation: Orientation::Horizontal, colors: &["#FFFFFF", "#0038B8", "#FFFFFF"] },
    FlagStripes { code: "BR", orientation: Orientation::Horizontal, colors: &["#009C3B", "#FFDF00", "#009C3B"] },
    FlagStripes { code: "AU", orientation: Orientation::Horizontal, colors: &["#012169", "#FFFFFF", "#012169"] },
    FlagStripes { code: "NZ", orientation: Orientation::Horizontal, colors: &["#00247D", "#CC142B", "#00247D"] },
    FlagStripes { code: "ZA", orientation: Orientation::Horizontal, colors: &["#007749", "#FFB81C", "#DE3831"] },
    FlagStripes { code: "KE", orientation: Orientation::Horizontal, colors: &["#000000", "#BB0000", "#006600"] },
    FlagStripes { code: "KZ", orientation: Orientation::Horizontal, colors: &["#00AFCA", "#FFD700", "#00AFCA"] },
    FlagStripes { code: "UZ", orientation: Orientation::Horizontal, colors: &["#1EB53A", "#FFFFFF", "#0099B5"] },
    FlagStripes { code: "VN", orientation: Orientation::Horizontal, colors: &["#DA251D", "#FFCD00", "#DA251D"] },
    FlagStripes { code: "SA", orientation: Orientation::Horizontal, colors: &["#006C35", "#FFFFFF", "#006C35"] },
    FlagStripes { code: "AE", orientation: Orientation::Horizontal, colors: &["#00732F", "#FFFFFF", "#000000"] },
    FlagStripes { code: "QA", orientation: Orientation::Horizontal, colors: &["#8A1538", "#FFFFFF", "#8A1538"] },
    FlagStripes { code: "MY", orientation: Orientation::Horizontal, colors: &["#010066", "#CC0001", "#FFFFFF"] },
    FlagStripes { code: "TH", orientation: Orientation::Horizontal, colors: &["#A51931", "#F4F5F8", "#2D2A4A"] },
    FlagStripes { code: "PH", orientation: Orientation::Horizontal, colors: &["#0038A8", "#FFFFFF", "#CE1126"] },
    FlagStripes { code: "MM", orientation: Orientation::Horizontal, colors: &["#FECB00", "#34B233", "#EA2839"] },
    FlagStripes { code: "BD", orientation: Orientation::Horizontal, colors: &["#006A4E", "#F42A41", "#006A4E"] },
    FlagStripes { code: "PK", orientation: Orientation::Horizontal, colors: &["#01411C", "#FFFFFF", "#01411C"] },
    FlagStripes { code: "CL", orientation: Orientation::Horizontal, colors: &["#FFFFFF", "#0039A6", "#D52B1E"] },
    FlagStripes { code: "PA", orientation: Orientation::Horizontal, colors: &["#FFFFFF", "#DA121A", "#003DA5"] },
    FlagStripes { code: "CU", orientation: Orientation::Horizontal, colors: &["#002A8F", "#FFFFFF", "#CF142B"] },
    FlagStripes { code: "ES", orientation: Orientation::Horizontal, colors: &["#AA151B", "#F1BF00", "#AA151B"] },
    FlagStripes { code: "CZ", orientation: Orientation::Horizontal, colors: &["#FFFFFF", "#11457E", "#D7141A"] },
    FlagStripes { code: "SK", orientation: Orientation::Horizontal, colors: &["#FFFFFF", "#0B4EA2", "#EE1C25"] },
    FlagStripes { code: "BA", orientation: Orientation::Horizontal, colors: &["#002395", "#FECB00", "#002395"] },
    FlagStripes { code: "AL", orientation: Orientation::Horizontal, colors: &["#E41E20", "#000000", "#E41E20"] },
    FlagStripes { code: "MK", orientation: Orientation::Horizontal, colors: &["#D20000", "#FFE600", "#D20000"] },
    FlagStripes { code: "ME", orientation: Orientation::Horizontal, colors: &["#C40308", "#D4AF37", "#C40308"] },
    FlagStripes { code: "MA", orientation: Orientation::Horizontal, colors: &["#C1272D", "#006233", "#C1272D"] },
    FlagStripes { code: "TN", orientation: Orientation::Horizontal, colors: &["#E70013", "#FFFFFF", "#E70013"] },
    FlagStripes { code: "DZ", orientation: Orientation::Horizontal, colors: &["#006233", "#FFFFFF", "#D21034"] },
];

fn valid_codes() -> &'static Vec<&'static str> {
    static CODES: OnceLock<Vec<&'static str>> = OnceLock::new();
    CODES.get_or_init(|| STRIPES.iter().map(|f| f.code).collect())
}

fn parse_hex(color: &str) -> Rgba<u8> {
    let text = color.trim_start_matches('#');
    let value = u32::from_str_radix(text, 16).unwrap_or(0);
    Rgba([
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
        255,
    ])
}

/// Paint the simplified flag bitmap (rounded corners approximated by a plain
/// rect; the UI applies its own rounding/border).
pub fn draw_flag(code: &str) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let code = code.to_uppercase();
    let data = STRIPES.iter().find(|f| f.code == code)?;
    let mut img = ImageBuffer::new(FLAG_WIDTH, FLAG_HEIGHT);
    let fill = |img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, x0: u32, y0: u32, w: u32, h: u32, color: Rgba<u8>| {
        for y in y0..(y0 + h).min(FLAG_HEIGHT) {
            for x in x0..(x0 + w).min(FLAG_WIDTH) {
                img.put_pixel(x, y, color);
            }
        }
    };
    match data.orientation {
        Orientation::Horizontal => {
            let count = data.colors.len();
            let sh = FLAG_HEIGHT as f32 / count as f32;
            for (i, color) in data.colors.iter().enumerate() {
                let y0 = (i as f32 * sh).round() as u32;
                let h = (sh.ceil() as u32).max(1);
                fill(&mut img, 0, y0, FLAG_WIDTH, h, parse_hex(color));
            }
        }
        Orientation::Vertical => {
            let count = data.colors.len();
            let sw = FLAG_WIDTH as f32 / count as f32;
            for (i, color) in data.colors.iter().enumerate() {
                let x0 = (i as f32 * sw).round() as u32;
                let w = (sw.ceil() as u32).max(1);
                fill(&mut img, x0, 0, w, FLAG_HEIGHT, parse_hex(color));
            }
        }
        Orientation::Nordic => {
            fill(&mut img, 0, 0, FLAG_WIDTH, FLAG_HEIGHT, parse_hex(data.colors[0]));
            let outer = parse_hex(data.colors[1]);
            let inner = data.colors.get(2).map(|c| parse_hex(c)).unwrap_or(outer);
            fill(&mut img, 0, 5, FLAG_WIDTH, 4, outer);
            fill(&mut img, 6, 0, 4, FLAG_HEIGHT, outer);
            if data.colors.len() > 2 {
                fill(&mut img, 0, 6, FLAG_WIDTH, 2, inner);
                fill(&mut img, 7, 0, 2, FLAG_HEIGHT, inner);
            }
        }
        Orientation::Cross => {
            fill(&mut img, 0, 0, FLAG_WIDTH, FLAG_HEIGHT, parse_hex(data.colors[0]));
            let cross = parse_hex(data.colors[1]);
            fill(&mut img, 0, 5, FLAG_WIDTH, 4, cross);
            fill(&mut img, 8, 0, 4, FLAG_HEIGHT, cross);
        }
    }
    // Neutral fill for known codes without stripes handled by caller fallback.
    Some(img)
}

/// Fallback gray flag for unknown codes.
pub fn draw_unknown_flag() -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let mut img = ImageBuffer::new(FLAG_WIDTH, FLAG_HEIGHT);
    let gray = parse_hex("#B0BEC5");
    for y in 0..FLAG_HEIGHT {
        for x in 0..FLAG_WIDTH {
            img.put_pixel(x, y, gray);
        }
    }
    img
}

/// Regional-indicator flag emoji for an ISO 3166-1 alpha-2 code.
pub fn get_flag_emoji(code: &str) -> String {
    let code = code.trim().to_uppercase();
    if code.len() != 2 || !code.chars().all(|c| c.is_ascii_alphabetic()) {
        return String::new();
    }
    code.chars()
        .map(|c| char::from_u32(0x1F1E6 + (c as u32) - ('A' as u32)).unwrap_or(' '))
        .collect()
}

// ── Country detection ──

pub fn detect_country(name: &str, server: &str) -> String {
    detect_emoji(name)
        .or_else(|| detect_name(name))
        .or_else(|| detect_code(name))
        .or_else(|| detect_server(server))
        .unwrap_or_default()
}

fn detect_emoji(text: &str) -> Option<String> {
    let codepoints: Vec<u32> = text.chars().map(|c| c as u32).collect();
    for pair in codepoints.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if (0x1F1E6..=0x1F1FF).contains(&a) && (0x1F1E6..=0x1F1FF).contains(&b) {
            let a_char = char::from_u32(a - 0x1F1E6 + 'A' as u32)?;
            let b_char = char::from_u32(b - 0x1F1E6 + 'A' as u32)?;
            return Some(format!("{a_char}{b_char}"));
        }
    }
    None
}

/// (pattern, code) sorted longest-first so multi-word names match first.
fn names() -> &'static Vec<(&'static str, &'static str)> {
    static NAMES: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut list: Vec<(&'static str, &'static str)> = vec![
            ("united states", "US"), ("usa", "US"), ("america", "US"),
            ("russia", "RU"), ("germany", "DE"), ("france", "FR"),
            ("united kingdom", "GB"), ("great britain", "GB"), ("england", "GB"),
            ("japan", "JP"), ("china", "CN"), ("south korea", "KR"), ("korea", "KR"),
            ("taiwan", "TW"), ("hong kong", "HK"), ("singapore", "SG"),
            ("netherlands", "NL"), ("holland", "NL"),
            ("italy", "IT"), ("spain", "ES"), ("portugal", "PT"),
            ("ireland", "IE"), ("belgium", "BE"), ("austria", "AT"),
            ("switzerland", "CH"), ("poland", "PL"), ("czech", "CZ"), ("czechia", "CZ"),
            ("slovakia", "SK"), ("hungary", "HU"), ("romania", "RO"), ("bulgaria", "BG"),
            ("croatia", "HR"), ("serbia", "RS"), ("greece", "GR"), ("turkey", "TR"),
            ("turkiye", "TR"), ("ukraine", "UA"), ("moldova", "MD"), ("georgia", "GE"),
            ("armenia", "AM"), ("azerbaijan", "AZ"), ("kazakhstan", "KZ"),
            ("uzbekistan", "UZ"), ("sweden", "SE"), ("norway", "NO"),
            ("finland", "FI"), ("denmark", "DK"), ("iceland", "IS"),
            ("estonia", "EE"), ("latvia", "LV"), ("lithuania", "LT"),
            ("luxembourg", "LU"), ("albania", "AL"), ("north macedonia", "MK"),
            ("bosnia", "BA"), ("montenegro", "ME"), ("slovenia", "SI"),
            ("israel", "IL"), ("india", "IN"), ("thailand", "TH"),
            ("vietnam", "VN"), ("indonesia", "ID"), ("malaysia", "MY"),
            ("philippines", "PH"), ("brazil", "BR"), ("argentina", "AR"),
            ("chile", "CL"), ("colombia", "CO"), ("mexico", "MX"),
            ("canada", "CA"), ("australia", "AU"), ("new zealand", "NZ"),
            ("south africa", "ZA"), ("nigeria", "NG"), ("kenya", "KE"),
            ("egypt", "EG"), ("uae", "AE"), ("emirates", "AE"),
            ("saudi arabia", "SA"), ("panama", "PA"), ("iran", "IR"),
            ("iraq", "IQ"), ("pakistan", "PK"), ("bangladesh", "BD"),
            ("cambodia", "KH"), ("myanmar", "MM"), ("mongolia", "MN"),
            ("россия", "RU"), ("германия", "DE"), ("франция", "FR"),
            ("великобритания", "GB"), ("англия", "GB"), ("япония", "JP"),
            ("китай", "CN"), ("корея", "KR"), ("тайвань", "TW"),
            ("гонконг", "HK"), ("сингапур", "SG"), ("нидерланды", "NL"),
            ("голландия", "NL"), ("италия", "IT"), ("испания", "ES"),
            ("португалия", "PT"), ("ирландия", "IE"), ("бельгия", "BE"),
            ("австрия", "AT"), ("швейцария", "CH"), ("польша", "PL"),
            ("чехия", "CZ"), ("словакия", "SK"), ("венгрия", "HU"),
            ("румыния", "RO"), ("болгария", "BG"), ("хорватия", "HR"),
            ("сербия", "RS"), ("греция", "GR"), ("турция", "TR"),
            ("украина", "UA"), ("молдова", "MD"), ("молдавия", "MD"),
            ("грузия", "GE"), ("армения", "AM"), ("азербайджан", "AZ"),
            ("казахстан", "KZ"), ("узбекистан", "UZ"), ("швеция", "SE"),
            ("норвегия", "NO"), ("финляндия", "FI"), ("дания", "DK"),
            ("исландия", "IS"), ("эстония", "EE"), ("латвия", "LV"),
            ("литва", "LT"), ("люксембург", "LU"), ("албания", "AL"),
            ("македония", "MK"), ("босния", "BA"), ("черногория", "ME"),
            ("словения", "SI"), ("израиль", "IL"), ("индия", "IN"),
            ("таиланд", "TH"), ("вьетнам", "VN"), ("индонезия", "ID"),
            ("малайзия", "MY"), ("филиппины", "PH"), ("бразилия", "BR"),
            ("аргентина", "AR"), ("чили", "CL"), ("колумбия", "CO"),
            ("мексика", "MX"), ("канада", "CA"), ("австралия", "AU"),
            ("новая зеландия", "NZ"), ("юар", "ZA"), ("нигерия", "NG"),
            ("кения", "KE"), ("египет", "EG"), ("оаэ", "AE"), ("эмираты", "AE"),
            ("саудовская аравия", "SA"), ("панама", "PA"), ("иран", "IR"),
            ("ирак", "IQ"), ("пакистан", "PK"), ("монголия", "MN"),
            ("moscow", "RU"), ("saint petersburg", "RU"), ("novosibirsk", "RU"),
            ("москва", "RU"), ("питер", "RU"), ("спб", "RU"), ("петербург", "RU"),
            ("new york", "US"), ("los angeles", "US"), ("chicago", "US"),
            ("san francisco", "US"), ("seattle", "US"), ("dallas", "US"),
            ("miami", "US"), ("atlanta", "US"), ("washington", "US"),
            ("silicon valley", "US"), ("ashburn", "US"), ("phoenix", "US"),
            ("las vegas", "US"), ("denver", "US"), ("boston", "US"),
            ("berlin", "DE"), ("frankfurt", "DE"), ("munich", "DE"), ("hamburg", "DE"),
            ("dusseldorf", "DE"), ("nuremberg", "DE"),
            ("paris", "FR"), ("marseille", "FR"), ("lyon", "FR"),
            ("london", "GB"), ("manchester", "GB"), ("edinburgh", "GB"),
            ("tokyo", "JP"), ("osaka", "JP"),
            ("beijing", "CN"), ("shanghai", "CN"), ("guangzhou", "CN"), ("shenzhen", "CN"),
            ("seoul", "KR"), ("busan", "KR"),
            ("taipei", "TW"),
            ("amsterdam", "NL"), ("rotterdam", "NL"),
            ("rome", "IT"), ("milan", "IT"),
            ("madrid", "ES"), ("barcelona", "ES"),
            ("lisbon", "PT"), ("dublin", "IE"), ("brussels", "BE"),
            ("vienna", "AT"), ("wien", "AT"),
            ("zurich", "CH"), ("geneva", "CH"), ("bern", "CH"),
            ("warsaw", "PL"), ("krakow", "PL"),
            ("prague", "CZ"), ("budapest", "HU"), ("bucharest", "RO"),
            ("sofia", "BG"), ("zagreb", "HR"), ("belgrade", "RS"),
            ("athens", "GR"),
            ("kyiv", "UA"), ("kiev", "UA"), ("odessa", "UA"),
            ("киев", "UA"), ("одесса", "UA"),
            ("tbilisi", "GE"), ("yerevan", "AM"), ("baku", "AZ"),
            ("astana", "KZ"), ("almaty", "KZ"), ("tashkent", "UZ"),
            ("stockholm", "SE"), ("oslo", "NO"), ("helsinki", "FI"),
            ("copenhagen", "DK"), ("reykjavik", "IS"),
            ("tallinn", "EE"), ("riga", "LV"), ("vilnius", "LT"),
            ("istanbul", "TR"), ("ankara", "TR"),
            ("tel aviv", "IL"), ("jerusalem", "IL"),
            ("dubai", "AE"), ("abu dhabi", "AE"),
            ("riyadh", "SA"), ("jeddah", "SA"),
            ("cairo", "EG"), ("johannesburg", "ZA"), ("cape town", "ZA"),
            ("lagos", "NG"), ("nairobi", "KE"),
            ("sao paulo", "BR"), ("rio de janeiro", "BR"),
            ("buenos aires", "AR"), ("santiago", "CL"), ("bogota", "CO"),
            ("mexico city", "MX"),
            ("toronto", "CA"), ("montreal", "CA"), ("vancouver", "CA"),
            ("sydney", "AU"), ("melbourne", "AU"), ("perth", "AU"),
            ("auckland", "NZ"),
            ("mumbai", "IN"), ("delhi", "IN"), ("bangalore", "IN"),
            ("bangkok", "TH"),
            ("ho chi minh", "VN"), ("hanoi", "VN"),
            ("jakarta", "ID"), ("manila", "PH"), ("kuala lumpur", "MY"),
        ];
        list.sort_by_key(|(pattern, _)| std::cmp::Reverse(pattern.len()));
        list
    })
}

fn word_boundary_match(lower: &str, pattern: &str) -> bool {
    // \b pattern \b equivalent for word characters (letters/digits/_).
    let mut search_from = 0;
    while let Some(pos) = lower[search_from..].find(pattern) {
        let start = search_from + pos;
        let end = start + pattern.len();
        let before_ok = start == 0
            || !lower[..start]
                .chars()
                .last()
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);
        let after_ok = end >= lower.len()
            || !lower[end..]
                .chars()
                .next()
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);
        if before_ok && after_ok {
            return true;
        }
        search_from = start + 1;
    }
    false
}

fn detect_name(name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    for (pattern, code) in names() {
        if word_boundary_match(&lower, pattern) {
            return Some(code.to_string());
        }
    }
    None
}

fn detect_code(name: &str) -> Option<String> {
    // ^([A-Za-z]{2})(?:\s*[-_.|#\s]|\d)
    let mut chars = name.chars();
    let c0 = chars.next()?;
    let c1 = chars.next()?;
    if !c0.is_ascii_alphabetic() || !c1.is_ascii_alphabetic() {
        return None;
    }
    let cc: String = format!("{c0}{c1}").to_uppercase();
    let known = || valid_codes().contains(&cc.as_str()).then(|| cc.clone());
    match chars.next() {
        None => None,
        Some(d) if d.is_ascii_digit() => known(),
        Some(d) => {
            // optional whitespace run, then one of -_.|# (or whitespace)
            let mut sep = d;
            if sep.is_whitespace() {
                for next in chars.by_ref() {
                    if next.is_whitespace() {
                        continue;
                    }
                    sep = next;
                    break;
                }
            }
            if "-_.|#".contains(sep) || sep.is_whitespace() {
                known()
            } else {
                None
            }
        }
    }
}

const COUNTRY_TLDS: [&str; 62] = [
    "ru", "de", "fr", "jp", "kr", "cn", "ua", "pl", "cz", "nl", "se", "no", "fi", "dk", "at",
    "ch", "it", "es", "pt", "ie", "be", "hu", "ro", "bg", "hr", "rs", "gr", "tr", "il", "br",
    "ar", "mx", "ca", "au", "nz", "in", "sg", "my", "th", "vn", "id", "ph", "eg", "za", "ng",
    "ke", "kz", "uz", "ge", "am", "az", "ee", "lv", "lt", "lu", "al", "mk", "ba", "sk", "md",
    "tw", "hk",
];

fn remap_tld(tld: &str) -> String {
    match tld {
        "uk" => "GB".into(),
        "us" => "US".into(),
        other => other.to_uppercase(),
    }
}

fn detect_server(server: &str) -> Option<String> {
    if server.is_empty() {
        return None;
    }
    let server = server.to_lowercase();
    let server = server.trim_end_matches('.');
    // TLD check
    if let Some((_, tld)) = server.rsplit_once('.') {
        if tld.len() == 2 && COUNTRY_TLDS.contains(&tld) {
            return Some(remap_tld(tld));
        }
    }
    // First label: "de1.server.com" → "de"
    let first = server.split('.').next().unwrap_or("");
    let chars: Vec<char> = first.chars().collect();
    if chars.len() >= 3
        && chars[0].is_ascii_alphabetic()
        && chars[1].is_ascii_alphabetic()
        && chars[2].is_ascii_digit()
    {
        let cc: String = chars[0..2].iter().collect::<String>();
        if COUNTRY_TLDS.contains(&cc.as_str()) {
            return Some(remap_tld(&cc));
        }
    }
    None
}

// ── Async IP-based country resolution (ip-api.com batch) ──

static CACHE: OnceLock<Mutex<BTreeMap<String, String>>> = OnceLock::new();

pub fn cached_country(host: &str) -> Option<String> {
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap()
        .get(host)
        .cloned()
}

pub fn cache_country(host: &str, country: &str) {
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap()
        .insert(host.to_string(), country.to_string());
}

/// Resolve countries for hosts in batches of 100 via ip-api.com/batch.
pub async fn resolve_countries(hosts: &[String]) -> BTreeMap<String, String> {
    let mut results = BTreeMap::new();
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return results,
    };
    for batch in hosts.chunks(100) {
        if batch.is_empty() {
            continue;
        }
        let payload: Vec<Value> = batch
            .iter()
            .map(|host| json!({"query": host, "fields": "status,countryCode,query"}))
            .collect();
        let response = match client
            .post("http://ip-api.com/batch")
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => return results,
        };
        let data: Value = match response.json().await {
            Ok(v) => v,
            Err(_) => return results,
        };
        let Some(items) = data.as_array() else {
            return results;
        };
        for (host, item) in batch.iter().zip(items) {
            if item.get("status").and_then(|s| s.as_str()) == Some("fail") {
                continue;
            }
            if let Some(cc) = item.get("countryCode").and_then(|c| c.as_str()) {
                if !cc.is_empty() {
                    results.insert(host.clone(), cc.to_string());
                    cache_country(host, cc);
                }
            }
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_roundtrip() {
        assert_eq!(get_flag_emoji("RU"), "🇷🇺");
        assert_eq!(get_flag_emoji("DE"), "🇩🇪");
        assert_eq!(get_flag_emoji(""), "");
        assert_eq!(get_flag_emoji("XYZ"), "");
    }

    #[test]
    fn detection() {
        assert_eq!(detect_country("Москва 1", ""), "RU");
        assert_eq!(detect_country("Frankfurt #3", ""), "DE");
        assert_eq!(detect_country("", "de1.example.com"), "DE");
        assert_eq!(detect_country("\u{1F1FA}\u{1F1F8} fast", ""), "US");
        assert_eq!(detect_country("", "co.uk.example"), "");
    }

    #[test]
    fn flag_bitmap() {
        let img = draw_flag("RU").unwrap();
        assert_eq!(img.width(), FLAG_WIDTH);
        assert_eq!(img.get_pixel(5, 1).0[0], 255); // white top stripe
        assert!(draw_unknown_flag().width() == FLAG_WIDTH);
    }
}
