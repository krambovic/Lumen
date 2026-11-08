//! Built-in service and process routing preset catalogs (`service_presets.py`,
//! `process_presets.py`).

#[derive(Debug, Clone, Copy)]
pub struct ServicePreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub domains: &'static [&'static str],
    pub default_action: &'static str,
}

pub const SERVICE_PRESETS: &[ServicePreset] = &[
    ServicePreset {
        id: "youtube",
        name: "YouTube",
        description: "YouTube, ytimg, ggpht, googleapis",
        default_action: "proxy",
        domains: &[
            "youtube.com", "youtubekids.com", "ggpht.com", "googlevideo.com", "ytimg.com",
            "yt.be", "youtu.be", "googleadservices.com", "lh3.googleusercontent.com",
            "yt3.googleusercontent.com", "gvt1.com", "youtube-nocookie.com",
            "youtubeembeddedplayer.googleapis.com", "youtube.googleapis.com",
            "youtubei.googleapis.com", "youtubeeducation.com", "youtubegaming.com",
            "youtube-ui.l.google.com", "jnn-pa.googleapis.com", "yt-video-upload.l.google.com",
            "wide-youtube.l.google.com", "redirector.googlevideo.com", "play.google.com",
            "accounts.google.com", "ytimg.l.google.com",
        ],
    },
    ServicePreset {
        id: "discord",
        name: "Discord",
        description: "Discord, discordapp, discordcdn",
        default_action: "proxy",
        domains: &[
            "dis.gd", "disboard.org", "discord.center", "discord.co", "discord.com",
            "discord.design", "discord.dev", "discord.gift", "discord.gifts", "discord.me",
            "discord.gg", "discordapp.com", "discordapp.net", "discordapp.io", "discord.new",
            "discord.st", "discord.store", "discord.tools", "discordactivities.com",
            "discord-activities.com", "discord-attachments-uploads-prd.storage.googleapis.com",
            "discordbee.com", "discordbotlist.com", "discordcdn.com", "discordcdn.net",
            "discordexpert.com", "discordhome.com", "discordhub.com", "discordinvites.net",
            "discordlist.me", "discordlist.space", "discordmedia.com", "discordmerch.com",
            "discordpartygames.com", "discord-rtc.com", "discords.com", "discordsays.com",
            "discordservers.com", "discordstatus.com", "discordtop.com", "disforge.com",
            "dyno.gg", "findadiscord.com", "mee6.xyz", "top.gg", "discord.media",
            "rtc.discord.com", "gateway.discord.gg", "gateway-us-east1-b.discord.gg",
            "gateway-us-east1-c.discord.gg", "gateway-us-east1-d.discord.gg",
            "gateway-us-east1.discord.gg", "cdn.discordapp.com", "media.discordapp.net",
            "dl.discordapp.net", "images-ext-1.discordapp.net", "images-ext-2.discordapp.net",
            "media.discordapp.com", "router.discordapp.net", "remote-auth-gateway.discord.gg",
            "status.discord.com", "cloudflare-ech.com",
        ],
    },
    ServicePreset {
        id: "instagram",
        name: "Instagram",
        description: "Instagram, CDN Instagram",
        default_action: "proxy",
        domains: &["instagram.com", "cdninstagram.com"],
    },
    ServicePreset {
        id: "facebook",
        name: "Facebook",
        description: "Facebook, fbcdn, fburl",
        default_action: "proxy",
        domains: &["facebook.com", "fbcdn.net", "fburl.com", "fbsbx.com"],
    },
    ServicePreset {
        id: "twitter",
        name: "Twitter / X",
        description: "Twitter, X, t.co, twimg",
        default_action: "proxy",
        domains: &["twimg.com", "twitter.com", "x.com", "t.co"],
    },
    ServicePreset {
        id: "telegram",
        name: "Telegram",
        description: "Telegram, t.me, telegram-cdn",
        default_action: "proxy",
        domains: &[
            "telegram.org", "t.me", "telegram.me", "telegram.dog", "tdesktop.com",
            "telegram.tips", "telegramusercontent.com", "telesco.pe", "telegram-cdn.org",
            "91.108.56.0/22", "91.108.4.0/22", "91.108.8.0/22", "91.108.16.0/22",
            "91.108.12.0/22", "91.108.20.0/22", "91.105.192.0/23", "149.154.160.0/20",
            "185.76.151.0/24", "2001:b28:f23d::/48", "2001:b28:f23f::/48", "2001:67c:4e8::/48",
            "2001:b28:f23c::/48", "2a0a:f280::/32",
        ],
    },
    ServicePreset {
        id: "twitch",
        name: "Twitch",
        description: "Twitch, twitchcdn, jtvnw",
        default_action: "proxy",
        domains: &[
            "twitch.tv", "twitch.com", "twitchcdn.net", "twitchcon.com", "twitchsvc.net",
            "jtvnw.net", "ttvnw.net", "twitch-ext.rootonline.de", "scorecardresearch.com",
            "ext-twitch.tv", "live-video.net", "twitchadvertising.tv",
        ],
    },
    ServicePreset {
        id: "soundcloud",
        name: "SoundCloud",
        description: "SoundCloud, sndcdn",
        default_action: "proxy",
        domains: &[
            "sndcdn.com", "soundcloud.cloud", "soundcloud.com", "soundcloud.app.goo.gl",
            "soundcloud.app", "soundcloud.org", "soundcloud.net", "soundcloud.co",
            "soundcloud.co.uk", "soundcloud.fr", "soundcloud.de", "soundcloud.me",
            "soundclouddesign.com", "soundcloudpress.com", "soundcloudstatus.com",
            "soundcloudforartists.com",
        ],
    },
    ServicePreset {
        id: "ntcparty",
        name: "NTC.party",
        description: "Форум ntc.party",
        default_action: "proxy",
        domains: &["ntc.party"],
    },
    ServicePreset {
        id: "rutracker",
        name: "RuTracker",
        description: "RuTracker и зеркала",
        default_action: "proxy",
        domains: &[
            "rutracker.org", "rutracker.net", "rutracker.cr", "rutracker.nl", "rutracker.ru",
            "rutracker.cc", "rutracker.cloud", "rutracker.in", "rutracker.me", "rutracker.is",
            "dostup-rutracker.org", "maintracker.net", "maintracker.org", "rutracker-net.ru",
        ],
    },
    ServicePreset {
        id: "rutor",
        name: "Rutor",
        description: "Rutor и зеркала",
        default_action: "proxy",
        domains: &["rutor.info", "rutor.is"],
    },
    ServicePreset {
        id: "amazon_cdn",
        name: "Amazon / Cloudflare",
        description: "AWS, Cloudflare, CloudFront",
        default_action: "proxy",
        domains: &[
            "cloudflare-ech.com", "cloudflare.com", "cloudflareportal.com", "cloudflareok.com",
            "cloudflareclient.com", "cloudflarecp.com", "cloudfront.net",
            "cloudflareinsights.com", "amazon.com", "amazonaws.com", "awsstatic.com",
            "awsglobalaccelerator.com",
        ],
    },
    ServicePreset {
        id: "openai",
        name: "OpenAI / ChatGPT",
        description: "ChatGPT, OpenAI, CDN и авторизация",
        default_action: "proxy",
        domains: &[
            "chatgpt.com", "chat.openai.com", "openai.com", "api.openai.com", "auth.openai.com",
            "cdn.openai.com", "oaistatic.com", "cdn.oaistatic.com", "oaiusercontent.com",
            "files.oaiusercontent.com", "chatgpt.livekit.cloud", "openaiapi-site.azureedge.net",
            "events.statsigapi.net", "featuregates.org", "challenges.cloudflare.com",
            "intercom.io", "intercomcdn.com", "sentry.io", "browser-intake-datadoghq.com",
        ],
    },
    ServicePreset {
        id: "claude",
        name: "Claude AI",
        description: "Claude AI, Anthropic",
        default_action: "proxy",
        domains: &["claude.ai", "claude.com", "anthropic.com"],
    },
    ServicePreset {
        id: "steam",
        name: "Steam",
        description: "Steam, Valve, SteamPowered",
        default_action: "direct",
        domains: &[
            "steamcommunity.com", "steampowered.com", "steam-chat.com", "steamgames.com",
            "steamusercontent.com", "steamcontent.com", "steamstatic.com", "akamaihd.net",
            "steam-api.com", "steamserver.net", "playartifact.com", "s.team", "steam.tv",
            "steamdeck.com", "underlords.com", "valvesoftware.com",
        ],
    },
    ServicePreset {
        id: "google",
        name: "Google",
        description: "Google, gstatic, googleapis",
        default_action: "direct",
        domains: &[
            "google.com", "gstatic.com", "googleapis.com", "googleusercontent.com",
            "doubleclick.net", "withgoogle.com",
        ],
    },
    ServicePreset {
        id: "github",
        name: "GitHub",
        description: "GitHub, githubassets, ghcr.io",
        default_action: "direct",
        domains: &[
            "github.com", "github.io", "githubassets.com", "githubusercontent.com", "ghcr.io",
            "githubstatus.com", "githubapp.com", "scoop.sh",
        ],
    },
    ServicePreset {
        id: "whatsapp",
        name: "WhatsApp",
        description: "WhatsApp, wa.me",
        default_action: "proxy",
        domains: &[
            "whatsapp.com", "whatsapp.net", "whatsapp.co", "wl.co", "wa.me", "whatsappbrand.com",
        ],
    },
    ServicePreset {
        id: "speedtest",
        name: "Speedtest",
        description: "Speedtest, Ookla",
        default_action: "proxy",
        domains: &["speedtest.net", "ooklaserver.net", "ziffstatic.com", "cdnst.net"],
    },
    ServicePreset {
        id: "roblox",
        name: "Roblox",
        description: "Roblox, rbxcdn и зеркала",
        default_action: "direct",
        domains: &[
            "roblox.com", "roblox.net", "roblox.org", "robloxapp.com", "robloxgames.com",
            "robloxlabs.com", "rbxcdn.com", "rbxcdn.net", "rbxcdn.org", "rbxcdn.io",
            "rbxinfra.net", "rbxtrk.com", "rbxlabs.com", "rblx.com", "rblx.co", "rblx.org",
        ],
    },
    ServicePreset {
        id: "epic_fortnite",
        name: "Epic Games / Fortnite",
        description: "Epic Games, EOS, Fortnite, EasyAntiCheat, BattlEye",
        default_action: "direct",
        domains: &[
            "epicgames.com", "epicgames.dev", "epicgames.net", "epicgamescdn.com",
            "unrealengine.com", "fortnite.com", "easy.ac", "easyanticheat.net", "battleye.com",
        ],
    },
];

pub fn service_preset_by_id(id: &str) -> Option<&'static ServicePreset> {
    SERVICE_PRESETS.iter().find(|p| p.id == id)
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub processes: &'static [&'static str],
    pub default_action: &'static str,
}

pub const PROCESS_PRESETS: &[ProcessPreset] = &[
    ProcessPreset {
        id: "telegram",
        name: "Telegram",
        description: "Telegram, AyuGram, Unigram, Kotatogram",
        processes: &["Telegram.exe", "AyuGram.exe", "Unigram.exe", "Kotatogram.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "discord",
        name: "Discord",
        description: "Discord, BetterDiscord, Vesktop",
        processes: &["Discord.exe", "Vesktop.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "chrome",
        name: "Google Chrome",
        description: "Chrome, Chromium",
        processes: &["chrome.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "firefox",
        name: "Firefox",
        description: "Firefox, Waterfox, LibreWolf",
        processes: &["firefox.exe", "waterfox.exe", "librewolf.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "edge",
        name: "Microsoft Edge",
        description: "Edge",
        processes: &["msedge.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "opera",
        name: "Opera / Brave / Vivaldi",
        description: "Opera, Brave, Vivaldi, Яндекс Браузер",
        processes: &["opera.exe", "brave.exe", "vivaldi.exe", "browser.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "spotify",
        name: "Spotify",
        description: "Spotify",
        processes: &["Spotify.exe"],
        default_action: "proxy",
    },
    ProcessPreset {
        id: "torrents",
        name: "Торренты",
        description: "qBittorrent, uTorrent, Transmission, Deluge, PicoTorrent",
        processes: &[
            "qbittorrent.exe", "uTorrent.exe", "utorrentie.exe", "BitTorrent.exe",
            "Transmission-qt.exe", "deluge.exe", "PicoTorrent.exe", "Vuze.exe", "Tixati.exe",
        ],
        default_action: "direct",
    },
    ProcessPreset {
        id: "windows_system",
        name: "Windows система",
        description: "svchost, explorer, dwm, службы, обновления",
        processes: &[
            "svchost.exe", "explorer.exe", "dwm.exe", "csrss.exe", "taskhostw.exe", "sihost.exe",
            "ctfmon.exe", "fontdrvhost.exe", "dllhost.exe", "conhost.exe", "audiodg.exe",
            "spoolsv.exe", "SearchApp.exe", "SearchHost.exe", "RuntimeBroker.exe",
            "LockApp.exe", "StartMenuExperienceHost.exe", "ShellExperienceHost.exe",
            "TextInputHost.exe", "backgroundTaskHost.exe", "ApplicationFrameHost.exe",
            "UserOOBEBroker.exe", "SystemSettings.exe", "WUDFHost.exe",
        ],
        default_action: "direct",
    },
    ProcessPreset {
        id: "windows_defender",
        name: "Windows Defender",
        description: "Антивирус, SmartScreen, обновления безопасности",
        processes: &[
            "MsMpEng.exe", "NisSrv.exe", "SecurityHealthService.exe",
            "SecurityHealthSystray.exe", "MpCmdRun.exe", "smartscreen.exe",
        ],
        default_action: "direct",
    },
    ProcessPreset {
        id: "windows_update",
        name: "Windows Update",
        description: "Обновления Windows, доставка, BITS",
        processes: &[
            "wuauclt.exe", "WaaSMedicAgent.exe", "UsoClient.exe", "musNotification.exe",
            "musNotificationUx.exe", "TiWorker.exe", "TrustedInstaller.exe",
        ],
        default_action: "direct",
    },
    ProcessPreset {
        id: "onedrive",
        name: "OneDrive",
        description: "OneDrive синхронизация",
        processes: &["OneDrive.exe", "OneDriveSetup.exe"],
        default_action: "direct",
    },
];

pub fn process_preset_by_id(id: &str) -> Option<&'static ProcessPreset> {
    PROCESS_PRESETS.iter().find(|p| p.id == id)
}
