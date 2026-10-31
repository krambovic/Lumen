# Droute (Proxy Support for Discord on Windows)

<p align="center">
  <img src="external/droute.svg" alt="Droute Logo" width="128">
</p>

<p align="center">
  <a href="https://github.com/snowluwu/droute/releases"><img src="https://img.shields.io/github/v/release/snowluwu/droute?style=for-the-badge&label=Release&labelColor=3A3A3A&color=5662f6" alt="Release"></a>
</p>

**Droute** adds SOCKS5 proxy support to the Discord Desktop Client on Windows for chat, media, voice calls and screen sharing.

Inspired by [force-proxy](https://github.com/runetfreedom/force-proxy). Uses [MinHook](https://github.com/tsudakageyu/minhook) to intercept network calls.

> [!NOTE]
> Requires a running SOCKS5 proxy. Voice calls and screen sharing also need UDP support. A local proxy from v2rayN, Clash Verge or NekoBox works too.

## Screenshots
<details>
<summary>Installer previews</summary>
<br>
<img width="451" height="415" alt="image" src="https://github.com/user-attachments/assets/11509cf4-1a11-4744-92e6-d340c49de6cd" />
<br>
<img width="581" height="449" alt="image" src="https://github.com/user-attachments/assets/9cc70f54-c621-4ebd-8931-15ec48f4d8ea" />
</details>

<details>
<summary>CLI Previews</summary>
<br>
<img width="1113" height="626" alt="image" src="https://github.com/user-attachments/assets/3fa1e04f-7364-4a81-9a38-c653502fac12" />
<br>
<img width="1113" height="721" alt="image" src="https://github.com/user-attachments/assets/ef23cfba-913a-4e82-85b0-7c15139aff55" />
</details>

---

## Features

- **TCP and UDP proxying:** Routes chat, media, voice calls and screen sharing through SOCKS5.
- **Automatic reconnection:** Restores UDP proxy connections after the proxy disconnects or restarts.
- **Update hook:** Installs Droute in new Discord versions before they launch.
- **Installer and CLI:** Configure the proxy and install or remove Droute through the installer or command line.
- **Discord builds:** Supports Stable, Canary and PTB on Windows x64.

---

## Installation

### Installer Mode

1. Start your SOCKS5 proxy. Check its address, port, and username/password if required.
2. Download `droute.zip` from the [latest release](https://github.com/snowluwu/droute/releases/latest) and extract it.
3. Open `droute.exe`.
4. Enter your SOCKS5 proxy settings and click **Apply**.
5. Select Stable, Canary, or PTB and click **Install**. With automatic restart enabled, the installer closes Discord first. Otherwise, close Discord yourself.
6. Close the installation log window. If the installer closed Discord, it starts it again. Otherwise, start Discord yourself.

To remove Droute, close Discord and use the installer's removal action for the selected build.

### CLI Mode

Close Discord before installing or removing Droute. The CLI does not close it automatically.

Install for Stable and save the proxy address:

```console
.\droute.exe --install --branch stable --host 127.0.0.1 --port 1080
```

Replace `127.0.0.1:1080` with your proxy address. Use `--branch canary` or `--branch ptb` for other builds. Add `--user` and `--password` if your proxy requires authentication.

Remove Droute from Stable:

```console
.\droute.exe --uninstall --branch stable
```

List available options:

```console
.\droute.exe --help
```

---

## How It Works

### Main library

The installer copies `version.dll` from the Windows `System32` folder to the selected Discord `app-*` folder. It adds `droute.dll` to the import table of this copy and places `droute.dll` in the same folder.

When Discord loads the local `version.dll`, Windows also loads `droute.dll`. This is the main Droute library: it uses MinHook to intercept Winsock calls and send connections through SOCKS5. The original system DLL and Discord executable are not changed.

### Applying after updates

The installer places `Droute.UpdaterHook.dll` and `Update.exe.config` in the same folder as Discord's `Update.exe`. The config file tells the updater to load the hook. Before Discord starts, the hook finds the latest `app-*` folder and installs `version.dll` and `droute.dll` if they are missing.

### Settings & Logs

- **Configuration:** Proxy settings are stored for the current Windows user at `HKCU\Software\droute`. Change them through the installer or `regedit`, then restart Discord.
- **Logs:** Located at `%LOCALAPPDATA%\droute\logs\<branch>\droute-YYYY-MM-DD.log` and `updater-YYYY-MM-DD.log` (`stable`, `canary`, or `ptb`). Each day has a separate file; Discord processes share the same daily log. Old logs are kept.
- **Log Level:** Controlled by the `LogLevel` (DWORD) value in `HKCU\Software\droute`:
  - `0` — TRACE (all events and detailed UDP operations)
  - `1` — DEBUG (association attempts, SOCKS negotiation)
  - `2` — INFO (default: lifecycle events and UDP summaries)
  - `3` — WARN
  - `4` — ERROR

**Reporting a Problem:**
Include your Droute version, Discord build, proxy software and what went wrong. Attach the logs for that day and say roughly when the problem happened.

- Logs include session IDs (`SESSION`), process PIDs, and connection generations (`generation`) to help trace traffic flows.
- Proxy usernames/passwords, packet contents and command-line arguments are **not logged**.
