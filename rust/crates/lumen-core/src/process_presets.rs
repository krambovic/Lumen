//! Ready-made process presets for per-application routing (winws/TUN splits).

// NOTE: the Python module carries presentation metadata only (`icon: FIF`,
// never read by the QML frontend); here the icon is its plain name string.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub description: &'static str,
    pub processes: &'static [&'static str],
    /// "proxy" | "direct"
    pub default_action: &'static str,
}

pub const PROCESS_PRESETS: &[ProcessPreset] = &[
    // ── Popular apps (default: proxy — route through VPN) ──
    ProcessPreset {
        id: "telegram",
        name: "Telegram",
        icon: "CHAT",
        description: "Telegram, AyuGram, Unigram, Kotatogram",
        processes: &["Telegram.exe", "AyuGram.exe", "Unigram.exe", "Kotatogram.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "discord",
        name: "Discord",
        icon: "MICROPHONE",
        description: "Discord, BetterDiscord, Vesktop",
        processes: &["Discord.exe", "Vesktop.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "chrome",
        name: "Google Chrome",
        icon: "GLOBE",
        description: "Chrome, Chromium",
        processes: &["chrome.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "firefox",
        name: "Firefox",
        icon: "GLOBE",
        description: "Firefox, Waterfox, LibreWolf",
        processes: &["firefox.exe", "waterfox.exe", "librewolf.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "edge",
        name: "Microsoft Edge",
        icon: "GLOBE",
        description: "Edge",
        processes: &["msedge.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "opera",
        name: "Opera / Brave / Vivaldi",
        icon: "GLOBE",
        description: "Opera, Brave, Vivaldi, Яндекс Браузер",
        processes: &["opera.exe", "brave.exe", "vivaldi.exe", "browser.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "spotify",
        name: "Spotify",
        icon: "MUSIC",
        description: "Spotify",
        processes: &["Spotify.exe"],
        default_action: "proxy",
    },
    // ── Torrents (default: direct — P2P traffic goes direct) ──
    ProcessPreset {
        id: "torrents",
        name: "Торренты",
        icon: "CLOUD_DOWNLOAD",
        description: "qBittorrent, uTorrent, Transmission, Deluge, PicoTorrent",
        processes: &[
            "qbittorrent.exe",
            "uTorrent.exe",
            "utorrentie.exe",
            "BitTorrent.exe",
            "Transmission-qt.exe",
            "deluge.exe",
            "PicoTorrent.exe",
            "Vuze.exe",
            "Tixati.exe",
        ],
        default_action: "direct",
    },
    // ── Windows system (default: direct — bypass VPN to reduce server load) ──
    ProcessPreset {
        id: "windows_system",
        name: "Windows система",
        icon: "SETTING",
        description: "svchost, explorer, dwm, службы, обновления",
        processes: &[
            "svchost.exe",
            "explorer.exe",
            "dwm.exe",
            "csrss.exe",
            "taskhostw.exe",
            "sihost.exe",
            "ctfmon.exe",
            "fontdrvhost.exe",
            "dllhost.exe",
            "conhost.exe",
            "audiodg.exe",
            "spoolsv.exe",
            "SearchApp.exe",
            "SearchHost.exe",
            "RuntimeBroker.exe",
            "LockApp.exe",
            "StartMenuExperienceHost.exe",
            "ShellExperienceHost.exe",
            "TextInputHost.exe",
            "backgroundTaskHost.exe",
            "ApplicationFrameHost.exe",
            "UserOOBEBroker.exe",
            "SystemSettings.exe",
            "WUDFHost.exe",
        ],
        default_action: "direct",
    },
    ProcessPreset {
        id: "windows_defender",
        name: "Windows Defender",
        icon: "CERTIFICATE",
        description: "Антивирус, SmartScreen, обновления безопасности",
        processes: &[
            "MsMpEng.exe",
            "NisSrv.exe",
            "SecurityHealthService.exe",
            "SecurityHealthSystray.exe",
            "MpCmdRun.exe",
            "smartscreen.exe",
        ],
        default_action: "direct",
    },
    ProcessPreset {
        id: "windows_update",
        name: "Windows Update",
        icon: "UPDATE",
        description: "Обновления Windows, доставка, BITS",
        processes: &[
            "wuauclt.exe",
            "WaaSMedicAgent.exe",
            "UsoClient.exe",
            "musNotification.exe",
            "musNotificationUx.exe",
            "TiWorker.exe",
            "TrustedInstaller.exe",
        ],
        default_action: "direct",
    },
    ProcessPreset {
        id: "onedrive",
        name: "OneDrive",
        icon: "CLOUD",
        description: "OneDrive синхронизация",
        processes: &["OneDrive.exe", "OneDriveSetup.exe"],
        default_action: "direct",
    },
];

pub fn process_preset_by_id(id: &str) -> Option<&'static ProcessPreset> {
    PROCESS_PRESETS.iter().find(|preset| preset.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_ids_are_unique_and_actions_known() {
        let mut ids: Vec<&str> = PROCESS_PRESETS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PROCESS_PRESETS.len());
        assert!(PROCESS_PRESETS.iter().all(|p| p.default_action == "proxy" || p.default_action == "direct"));
        assert_eq!(process_preset_by_id("telegram").unwrap().processes.len(), 4);
        assert!(process_preset_by_id("missing").is_none());
    }
}
