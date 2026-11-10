use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Map, Value, json};

use crate::win_process_list::try_list_running_executables;

use super::{LOOPBACK, remove_if_exists, write_backup};

const BEGIN: &str = "// Lumen system proxy begin";
const END: &str = "// Lumen system proxy end";
const RESET_BEGIN: &str = "// Lumen proxy restore begin";
const RESET_END: &str = "// Lumen proxy restore end";
const FILES: [&str; 2] = ["user.js", "prefs.js"];
const LEASE_KEYS: &[&str] = &[
    "network.proxy.type",
    "network.proxy.http",
    "network.proxy.http_port",
    "network.proxy.ssl",
    "network.proxy.ssl_port",
    "network.proxy.socks",
    "network.proxy.socks_port",
];
const BROWSERS: &[&str] = &["firefox.exe", "librewolf.exe", "waterfox.exe", "floorp.exe"];

type Backup = BTreeMap<PathBuf, Value>;

pub(super) struct FirefoxProxyManager {
    backup_path: PathBuf,
}

impl FirefoxProxyManager {
    pub(super) fn new(backup_path: PathBuf) -> Self {
        Self { backup_path }
    }

    pub(super) fn enable(&self, port: u16, bypass_lan: bool) -> Result<()> {
        self.enable_profiles(&find_profiles()?, port, bypass_lan)
    }

    fn enable_profiles(&self, profiles: &[PathBuf], port: u16, bypass_lan: bool) -> Result<()> {
        if profiles.is_empty() {
            return Ok(());
        }
        let mut backup = self.load_backup()?;
        let block = proxy_block(port, bypass_lan);
        for profile in profiles {
            // Each profile has its own durable snapshot before either file is changed.
            let _ = self.enable_profile(&mut backup, profile, &block);
        }
        Ok(())
    }

    fn enable_profile(&self, backup: &mut Backup, profile: &Path, block: &str) -> Result<()> {
        let previous = backup.get(profile).cloned();
        let mut original = match previous.as_ref() {
            Some(Value::Object(original)) => original.clone(),
            Some(_) => return Ok(()), // Unverified legacy browser backup.
            None => {
                let mut original = Map::new();
                for name in FILES {
                    original.insert(name.into(), json!(read_prefs(&profile.join(name))?));
                }
                original
            }
        };
        original.insert("__applied".into(), json!(block));
        backup.insert(profile.to_path_buf(), Value::Object(original));
        if let Err(error) = self.save_backup(backup) {
            if let Some(previous) = previous {
                backup.insert(profile.to_path_buf(), previous);
            } else {
                backup.remove(profile);
            }
            return Err(error);
        }

        for name in FILES {
            let path = profile.join(name);
            let current = read_prefs(&path)?.unwrap_or_default();
            let base = strip_block(&strip_block(&current, RESET_BEGIN, RESET_END), BEGIN, END);
            write_prefs(&path, &append_block(&base, block))?;
        }
        Ok(())
    }

    pub(super) fn disable(&self) -> Result<()> {
        if !self.backup_path.try_exists()? {
            return Ok(());
        }
        let browser_running = try_list_running_executables().map_or(true, |names| {
            names
                .iter()
                .any(|name| BROWSERS.contains(&name.to_ascii_lowercase().as_str()))
        });
        self.restore_profiles(browser_running)
    }

    fn restore_profiles(&self, browser_running: bool) -> Result<()> {
        let backup = self.load_backup()?;
        if backup.is_empty() {
            return Ok(());
        }
        let mut remaining = Backup::new();
        for (profile, original) in backup {
            let original = match original {
                Value::Object(files) => files,
                user_js => Map::from_iter([("user.js".into(), user_js)]),
            };
            let applied = original
                .get("__applied")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let pending = browser_running && !applied.is_empty();
            let mut failed = Map::new();
            for name in FILES {
                let Some(content) = original.get(name) else {
                    continue;
                };
                let restore = restore_file(
                    &profile.join(name),
                    name,
                    &original,
                    content,
                    applied,
                    pending,
                );
                if pending || restore.is_err() {
                    // When running, keep originals even if the immediate restore succeeds.
                    failed.insert(name.into(), content.clone());
                }
            }
            if !failed.is_empty() {
                if !applied.is_empty() {
                    failed.insert("__applied".into(), json!(applied));
                }
                remaining.insert(profile, Value::Object(failed));
            }
        }
        self.save_backup(&remaining)
    }

    fn load_backup(&self) -> Result<Backup> {
        let bytes = match fs::read(&self.backup_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Backup::new()),
            Err(error) => return Err(error.into()),
        };
        let mut backup: Backup = serde_json::from_slice(&bytes)?;
        backup.retain(|profile, original| {
            profile.is_absolute()
                && match original {
                    Value::Null | Value::String(_) => true,
                    Value::Object(files) => {
                        files.retain(|name, content| {
                            (FILES.contains(&name.as_str()) || name == "__applied")
                                && (content.is_null() || content.is_string())
                        });
                        true
                    }
                    _ => false,
                }
        });
        Ok(backup)
    }

    fn save_backup(&self, backup: &Backup) -> Result<()> {
        if backup.is_empty() {
            remove_if_exists(&self.backup_path)?;
            Ok(())
        } else {
            write_backup(&self.backup_path, ".firefox-", backup)
        }
    }
}

fn find_profiles() -> io::Result<Vec<PathBuf>> {
    let mut profiles = BTreeSet::new();
    for base in ["APPDATA", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .filter(|base| base.is_absolute())
    {
        for root in [
            "Mozilla/Firefox/Profiles",
            "librewolf/Profiles",
            "Waterfox/Profiles",
            "Floorp/Profiles",
        ] {
            let root = base.join(root);
            if !root.is_dir() {
                continue;
            }
            for entry in fs::read_dir(root)? {
                let child = entry?.path();
                let default = child
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| {
                        ext.eq_ignore_ascii_case("default")
                            || ext.eq_ignore_ascii_case("default-release")
                    });
                if child.is_dir() && (default || child.join("prefs.js").exists()) {
                    profiles.insert(child);
                }
            }
        }
    }
    Ok(profiles.into_iter().collect())
}

fn read_prefs(path: &Path) -> io::Result<Option<String>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(
            String::from_utf8_lossy(&bytes)
                .replace("\r\n", "\n")
                .replace('\r', "\n"),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn write_prefs(path: &Path, text: &str) -> io::Result<()> {
    let mut staged = tempfile::NamedTempFile::new_in(path.parent().expect("profile parent"))?;
    staged.write_all(text.replace('\n', "\r\n").as_bytes())?;
    staged.persist(path)?;
    Ok(())
}

fn append_block(text: &str, block: &str) -> String {
    let text = text.trim_end();
    if text.is_empty() {
        format!("{block}\n")
    } else {
        format!("{text}\n\n{block}\n")
    }
}

fn block_range(text: &str, begin: &str, end: &str) -> Option<std::ops::Range<usize>> {
    let start = text.find(begin)?;
    let stop = text.find(end)?;
    (stop >= start).then_some(start..stop + end.len())
}

fn strip_block(text: &str, begin: &str, end: &str) -> String {
    let Some(range) = block_range(text, begin, end) else {
        return text.into();
    };
    let (before, after) = (
        text[..range.start].trim_end(),
        text[range.end..].trim_start(),
    );
    if !before.trim().is_empty() && !after.trim().is_empty() {
        format!("{before}\n{after}")
    } else {
        format!("{before}{after}")
    }
}

fn proxy_block(port: u16, bypass_lan: bool) -> String {
    let mut no_proxies = String::from("localhost, 127.0.0.1");
    if bypass_lan {
        no_proxies.push_str(", 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, *.local, *.lan");
    }
    let prefs = [
        ("network.proxy.type", json!(1)),
        ("network.proxy.http", json!(LOOPBACK)),
        ("network.proxy.http_port", json!(port)),
        ("network.proxy.ssl", json!(LOOPBACK)),
        ("network.proxy.ssl_port", json!(port)),
        ("network.proxy.ftp", json!(LOOPBACK)),
        ("network.proxy.ftp_port", json!(port)),
        ("network.proxy.socks", json!(LOOPBACK)),
        ("network.proxy.socks_port", json!(port)),
        ("network.proxy.socks_version", json!(5)),
        ("network.proxy.socks_remote_dns", json!(true)),
        ("network.proxy.share_proxy_settings", json!(true)),
        ("network.proxy.no_proxies_on", json!(no_proxies)),
        ("network.trr.mode", json!(5)),
    ];
    [BEGIN.into()]
        .into_iter()
        .chain(
            prefs
                .into_iter()
                .map(|(key, value)| preference(key, &value)),
        )
        .chain([END.into()])
        .collect::<Vec<_>>()
        .join("\n")
}

fn preference(key: &str, value: &Value) -> String {
    format!("user_pref({}, {value});", json!(key))
}

fn parse_preference(line: &str) -> Option<(String, Value)> {
    let args = line.trim().strip_prefix("user_pref(")?.strip_suffix(");")?;
    let (key, value) = serde_json::from_str(&format!("[{args}]")).ok()?;
    Some((key, value))
}

#[derive(Default)]
struct Prefs<'a> {
    keys: Vec<String>,
    lines: HashMap<String, (Value, &'a str)>,
}

impl<'a> Prefs<'a> {
    fn parse(text: &'a str) -> Self {
        let mut prefs = Self::default();
        for line in text.lines() {
            if let Some((key, value)) = parse_preference(line) {
                if !prefs.lines.contains_key(&key) {
                    prefs.keys.push(key.clone());
                }
                prefs.lines.insert(key, (value, line));
            }
        }
        prefs
    }

    fn value(&self, key: &str) -> Option<&Value> {
        self.lines.get(key).map(|(value, _)| value)
    }

    fn line(&self, key: &str) -> Option<&'a str> {
        self.lines.get(key).map(|(_, line)| *line)
    }

    fn owns(&self, expected: &Prefs<'_>) -> bool {
        let lease: Vec<&str> = LEASE_KEYS
            .iter()
            .copied()
            .filter(|key| expected.value(key).is_some())
            .collect();
        let lease = if lease.is_empty() {
            expected.keys.iter().map(String::as_str).collect()
        } else {
            lease
        };
        lease.first().is_some_and(|key| self.value(key).is_some())
            && lease.into_iter().all(|key| {
                self.value(key)
                    .is_none_or(|live| Some(live) == expected.value(key))
            })
    }
}

fn reset_block(applied: &Prefs<'_>, original: &Map<String, Value>) -> String {
    let user = Prefs::parse(
        original
            .get("user.js")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let prefs = Prefs::parse(
        original
            .get("prefs.js")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let mut lines: Vec<String> = applied
        .keys
        .iter()
        .filter_map(|key| {
            if let Some(line) = user.line(key).or_else(|| prefs.line(key)) {
                return Some(line.trim().to_owned());
            }
            let default = match key.as_str() {
                "network.proxy.type" | "network.proxy.socks_version" => json!(5),
                "network.proxy.http"
                | "network.proxy.ssl"
                | "network.proxy.socks"
                | "network.proxy.no_proxies_on" => json!(""),
                "network.proxy.http_port"
                | "network.proxy.ssl_port"
                | "network.proxy.socks_port"
                | "network.trr.mode" => json!(0),
                "network.proxy.share_proxy_settings" | "network.proxy.socks_remote_dns" => {
                    json!(false)
                }
                _ => return None,
            };
            Some(preference(key, &default))
        })
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    lines.insert(0, RESET_BEGIN.into());
    lines.push(RESET_END.into());
    lines.join("\n")
}

fn restore_file(
    path: &Path,
    name: &str,
    original: &Map<String, Value>,
    content: &Value,
    applied: &str,
    pending: bool,
) -> Result<()> {
    let Some(current) = read_prefs(path)? else {
        return Ok(());
    };
    let base = strip_block(&current, RESET_BEGIN, RESET_END);
    let expected = if applied.is_empty() {
        let range = block_range(&base, BEGIN, END)
            .ok_or_else(|| anyhow::anyhow!("unproven legacy browser override"))?;
        &base[range]
    } else {
        applied
    };
    let live = Prefs::parse(&base);
    let expected = Prefs::parse(expected);
    let owned = live.owns(&expected);
    let mut restored = if owned {
        restore_preferences(
            &base,
            content.as_str().unwrap_or_default(),
            &live,
            &expected,
        )
    } else {
        base.clone()
    };
    if pending && name == "user.js" {
        let reset = reset_block(&expected, original);
        if !reset.is_empty() {
            let reset_values = Prefs::parse(&reset);
            let already_pinned = block_range(&current, RESET_BEGIN, RESET_END)
                .is_some_and(|range| current[range] == reset)
                && LEASE_KEYS.iter().all(|key| {
                    live.value(key)
                        .is_none_or(|value| Some(value) == reset_values.value(key))
                });
            if already_pinned {
                return Ok(());
            }
            if owned {
                restored = append_block(&restored, &reset);
            }
        }
    }
    if restored != current {
        if content.is_null() && restored.trim().is_empty() {
            remove_if_exists(path)?;
        } else {
            write_prefs(path, &restored)?;
        }
    }
    Ok(())
}

fn restore_preferences(base: &str, before: &str, live: &Prefs<'_>, expected: &Prefs<'_>) -> String {
    let owned = |key: &str| {
        expected.value(key).is_some()
            && live
                .value(key)
                .is_none_or(|value| Some(value) == expected.value(key))
    };
    let remaining: String = base
        .split_inclusive('\n')
        .filter(|line| {
            !matches!(line.trim(), BEGIN | END)
                && !parse_preference(line).is_some_and(|(key, _)| owned(&key))
        })
        .collect();
    let before = Prefs::parse(before);
    let mut restored = std::iter::once(remaining.trim_end())
        .chain(
            expected
                .keys
                .iter()
                .filter(|key| owned(key))
                .filter_map(|key| before.line(key)),
        )
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if !restored.is_empty() {
        restored.push('\n');
    }
    restored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_after_browser_rewrite_and_defers_cleanup_until_it_exits() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let manager = FirefoxProxyManager::new(dir.path().join("firefox_proxy_backup.json"));
        let profile = dir.path().join("profile.default");
        fs::create_dir(&profile)?;
        let prefs = profile.join("prefs.js");
        fs::write(&prefs, "user_pref(\"browser.theme\", \"old\");\n")?;
        manager.enable_profiles(std::slice::from_ref(&profile), 10808, true)?;

        manager.restore_profiles(true)?;
        let user = profile.join("user.js");
        let pinned = read_prefs(&user)?.unwrap();
        assert!(pinned.contains("user_pref(\"network.proxy.type\", 5);"));
        manager.restore_profiles(true)?;
        assert_eq!(read_prefs(&user)?.unwrap(), pinned);
        assert!(manager.backup_path.exists());

        fs::write(
            &prefs,
            concat!(
                "user_pref(\"browser.theme\", \"changed\");\n",
                "user_pref(\"network.proxy.type\", 1);\n",
                "user_pref(\"network.proxy.http\", \"127.0.0.1\");\n",
                "user_pref(\"network.proxy.http_port\", 10808);\n"
            ),
        )?;
        manager.restore_profiles(false)?;
        assert_eq!(
            read_prefs(&prefs)?.unwrap(),
            "user_pref(\"browser.theme\", \"changed\");\n"
        );
        assert!(!user.exists());
        assert!(!manager.backup_path.exists());
        Ok(())
    }

    #[test]
    fn leaves_an_external_browser_proxy_in_place() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let manager = FirefoxProxyManager::new(dir.path().join("firefox_proxy_backup.json"));
        let profile = dir.path().join("profile.default");
        fs::create_dir(&profile)?;
        manager.enable_profiles(std::slice::from_ref(&profile), 10808, false)?;
        let user = profile.join("user.js");
        let foreign = read_prefs(&user)?
            .unwrap()
            .replace("127.0.0.1", "other.proxy");
        fs::write(&user, &foreign)?;
        manager.restore_profiles(true)?;
        assert_eq!(read_prefs(&user)?.unwrap(), foreign);
        Ok(())
    }

    #[test]
    fn continues_with_other_profiles_when_one_cannot_be_read() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let manager = FirefoxProxyManager::new(dir.path().join("firefox_proxy_backup.json"));
        let locked = dir.path().join("locked.default");
        let writable = dir.path().join("writable.default");
        fs::create_dir_all(locked.join("user.js"))?;
        fs::create_dir(&writable)?;
        manager.enable_profiles(&[locked, writable.clone()], 10808, false)?;
        assert!(
            read_prefs(&writable.join("user.js"))?
                .unwrap()
                .contains("10808")
        );
        assert!(manager.load_backup()?.contains_key(&writable));
        Ok(())
    }
}
