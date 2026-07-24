# Lumen

<p align="center">
  <img src="windows/assets/Lumen.png" alt="Lumen Logo" width="140">
</p>

<p align="center">
  <a href="https://github.com/krambovic/Lumen/releases"><img src="https://img.shields.io/github/v/release/krambovic/Lumen?style=for-the-badge&label=Release&labelColor=3A3A3A&color=8A2BE2" alt="Релиз"></a>
  <a href="https://github.com/krambovic/Lumen/releases"><img src="https://img.shields.io/github/downloads/krambovic/Lumen/total?style=for-the-badge&label=Downloads&labelColor=3A3A3A&color=17A673" alt="Скачивания"></a>
  <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20Android-29B6F6?style=for-the-badge&labelColor=3A3A3A" alt="Платформа">
</p>

<p align="center">
  <b>Язык:</b> <a href="README.md">English</a> | <b>Русский</b>
</p>

---

Lumen — кроссплатформенный клиент для VPN/TUN, системного прокси, маршрутизации, управления серверами и обхода DPI (DPI bypass). Проект поддерживает **Windows Desktop** (PyQt6 QML с эффектами Mica/Acrylic) и **Android** (Jetpack Compose, VpnService и встроенный Go-ядро sing-box extended).

> [!IMPORTANT]
> Для работы TUN/VPN режимов и запуска средств обхода DPI (zapret) на Windows требуются права администратора.

---

## Скриншоты

<details>
<summary>Панель управления и темы оформления (Windows)</summary>
<br>

<img src="windows/assets/screenshots/dashboard-dark.png" alt="Панель управления в темной теме" width="100%">
<br><br>
<img src="windows/assets/screenshots/dashboard-red.png" alt="Панель управления с красным акцентом" width="100%">
<br><br>
<img src="windows/assets/screenshots/settings-rose-pine.png" alt="Настройки в теме Rose Pine" width="100%">
<br><br>
<img src="windows/assets/screenshots/dashboard-light.png" alt="Панель управления в светлой теме" width="100%">
<br><br>
<img src="windows/assets/screenshots/zapret-dark.png" alt="Экран обхода DPI через zapret" width="100%">

</details>

---

## Возможности программы

| Раздел | Используемые компоненты | Описание |
| :--- | :--- | :--- |
| **Обход DPI (Windows)** | zapret / WinDivert | Обход замедлений и блокировок YouTube, Discord и других сервисов на уровне пакетов. |
| **TUN / VPN** | sing-box-extended | Полноценный TUN-режим с поддержкой AmneziaWG (AWG 2.0), WireGuard и XHTTP. |
| **Прокси** | xray-core | Системный прокси (VLESS, Trojan, Shadowsocks, VMess). |
| **Маршрутизация** | GUI-пресеты | Удобная настройка маршрутов через интерфейс: пресеты, пользовательские домены, IP-правила и поведение отдельных сервисов. |
| **Discord Voice** | droute / SOCKS5 | Направляет голосовые каналы и стримы Discord через прокси без включения полного TUN-режима. |
| **Диагностика** | встроенные тесты | Проверка ping и реальной скорости скачивания серверов. |
| **Кроссплатформенность** | Windows & Android | Графический интерфейс на QML для Windows и Jetpack Compose для Android. |

---

## Поддерживаемые протоколы

Lumen поддерживает импорт и запуск таких типов серверов:

- **Xray / системный прокси:** VLESS, VMess, Trojan, Shadowsocks, SOCKS, HTTP.
- **sing-box / TUN:** Hysteria, Hysteria2, TUIC, Mieru, MASQUE, WireGuard, AmneziaWG (AWG), WARP.
- **Кастомные конфиги:** raw Xray и sing-box JSON-конфиги, включая импорт полных sing-box конфигов.

## Поддержка подписок

- Обычные URL подписок и зашифрованные ссылки Happ: `happ://crypt`, `happ://crypt2`, `happ://crypt3`, `happ://crypt4` и `happ://crypt5`.
- Подписки с привязкой по HWID: Lumen может отправлять настоящий HWID устройства Windows (включено по умолчанию) или указанный пользователем HWID.
- Метаданные и поддерживаемые функции подписок Happ Premium отображаются прямо в списке серверов и свойствах подписки.
- Сайты могут открыть Lumen и импортировать подписку через диплинк `lumen://`. Для кнопки «Добавить VPN» используйте `lumen://add?url=<URL-подписки-в-percent-encoding>&name=<необязательное-имя>`.

---

## Структура репозитория

- **`windows/`**: Клиент для Windows (Python, PyQt6, QML, zapret).
- **`android/`**: Нативный клиент для Android (`:app`, `:ui`, `:core:config`, `:core:database`, `:core:engine`, `:core:vpn`).

---

## Установка и запуск

Перейдите на страницу **[Releases](https://github.com/krambovic/Lumen/releases)** и скачайте актуальную версию:

* **Windows Установщик (`Lumen-Setup-windows-x64.exe`):** Рекомендуется для Windows.
* **Windows Портативная версия (`Lumen-portable-windows-x64.zip`):** Работает без установки.
* **Android APK (`app-universal-debug.apk`):** Приложение для смартфонов и эмуляторов Android.

---

## Сборка проекта (для разработчиков)

<details>
<summary><b>Сборка для Windows</b></summary>

```powershell
cd windows
pip install -r requirements.txt
python build_qml.py
```
Сборка создает установщик и портативный архив в директории `windows/dist/`.
</details>

<details>
<summary><b>Сборка для Android</b></summary>

```powershell
cd android
./gradlew assembleDebug
```
Сборка создает APK в директории `android/app/build/outputs/apk/debug/`.
</details>

---

## Лицензия

Проект Lumen поставляется под лицензией GPL-3.0. Сторонние бинарные файлы и библиотеки сохраняют свои оригинальные лицензии. Подробнее: [LICENSE](LICENSE) и [NOTICE.md](NOTICE.md).
