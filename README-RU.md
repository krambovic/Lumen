# Lumen KVN

<p align="center">
  <img src="assets/LumenKVN.png" alt="Lumen KVN Logo" width="140">
</p>

<p align="center">
  <a href="https://github.com/krambovic/Lumen-KVN/releases">
    <img src="https://img.shields.io/github/v/release/krambovic/Lumen-KVN?color=8000FF&style=for-the-badge" alt="Версия">
  </a>
  <a href="https://github.com/krambovic/Lumen-KVN/releases">
    <img src="https://img.shields.io/github/downloads/krambovic/Lumen-KVN/total?color=8000FF&style=for-the-badge" alt="Скачивания">
  </a>
  <img src="https://img.shields.io/badge/Platform-Windows%2011%20%7C%2010-0078d4?style=for-the-badge&logo=windows" alt="Платформа">
</p>

<p align="center">
  <b>Язык:</b> <a href="README.md">English</a> | <b>Русский</b>
</p>

---

Lumen KVN - самостоятельный Windows-клиент для VPN/TUN, системного прокси, маршрутизации, управления серверами и обхода DPI (DPI bypass) через zapret. Проект предлагает графический интерфейс на базе QML с эффектами Mica/Acrylic и аппаратным ускорением рендеринга.

> [!IMPORTANT]
> Для работы TUN/VPN режимов и запуска средств обхода DPI (zapret) требуются права администратора.

---

## Скриншоты

<table>
  <tr>
    <td width="33%">
      <img src="assets/screenshots/dashboard-dark.png" alt="Панель управления в темной теме">
      <sub><b>Панель управления</b> · темная тема</sub>
    </td>
    <td width="33%">
      <img src="assets/screenshots/dashboard-light.png" alt="Панель управления в светлой теме">
      <sub><b>Панель управления</b> · светлая тема</sub>
    </td>
    <td width="33%">
      <img src="assets/screenshots/zapret-dark.png" alt="Экран обхода DPI через zapret">
      <sub><b>zapret</b> · пресеты обхода DPI</sub>
    </td>
  </tr>
</table>

---

## Возможности программы

| Раздел | Используемые компоненты | Описание |
| :--- | :--- | :--- |
| **Обход DPI** | zapret / WinDivert | Обход замедлений и блокировок YouTube, Discord и других сервисов на уровне пакетов. |
| **TUN / VPN** | sing-box-extended | Полноценный TUN-режим с поддержкой AmneziaWG (AWG 2.0), WireGuard и Necko/XHTTP. |
| **Прокси** | xray-core | Системный прокси (VLESS, Trojan, Shadowsocks, VMess). |
| **Диагностика** | встроенные тесты | Проверка ping и реальной скорости скачивания серверов. |
| **Интерфейс** | PyQt6 / QML | Динамические акцентные цвета, темы оформления (включая Codex) и поддержка фоновых обоев. |

---

## Установка и запуск

Перейдите на страницу **[Releases](https://github.com/krambovic/Lumen-KVN/releases)** и скачайте актуальную версию:

* **Установщик (`LumenKVN-Setup-windows-x64.exe`):** Рекомендуется для большинства пользователей.
* **Портативная версия (`LumenKVN-portable-windows-x64.zip`):** Работает без установки.

---

## Быстрый старт

1. Запустите Lumen KVN от имени администратора.
2. Импортируйте ссылку сервера или поддерживаемый `.conf` файл.
3. Выберите режим подключения: системный прокси, VPN/TUN или обход DPI через zapret.
4. Выберите пресет маршрутизации и подключитесь.

WARP, WireGuard и AmneziaWG конфиги работают через TUN на `sing-box-extended`; обычные VLESS, Trojan, Shadowsocks и VMess ссылки можно использовать через режим системного прокси.

---

## Сборка проекта (для разработчиков)

<details>
<summary><b>Показать инструкции по сборке</b></summary>

1. Установите зависимости проекта:
   ```powershell
   pip install -r requirements.txt
   ```
2. Поместите исполняемые файлы ядер (`xray.exe`, `sing-box.exe`, `wintun.dll`, файлы базы GeoIP) в каталог `core/`.
3. Запустите скрипт компиляции и сборки:
   ```powershell
   python build_qml.py
   ```
Сборка создает установщик и портативный архив в директории `dist/`.
</details>

---

## История звезд

<a href="https://www.star-history.com/?repos=krambovic%2FLumen-KVN&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=krambovic/Lumen-KVN&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=krambovic/Lumen-KVN&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=krambovic/Lumen-KVN&type=date&legend=top-left" />
 </picture>
</a>

---

## Участники проекта

[![Участники](https://contrib.rocks/image?repo=krambovic/Lumen-KVN)](https://github.com/krambovic/Lumen-KVN/graphs/contributors)

---

## Лицензия

Проект Lumen KVN поставляется под лицензией GPL-3.0. Сторонние бинарные файлы и библиотеки сохраняют свои оригинальные лицензии. Подробнее: [LICENSE](LICENSE) и [NOTICE.md](NOTICE.md).
