<p align="center">
  <img src="assets/banner.png" alt="Lumen — кроссплатформенный клиент Xray / sing-box extended" width="100%">
</p>

<p align="center">
  <a href="https://github.com/krambovic/Lumen/releases"><img src="https://img.shields.io/github/v/release/krambovic/Lumen?style=for-the-badge&label=%D0%A0%D0%B5%D0%BB%D0%B8%D0%B7&labelColor=1C1C1C&color=8A2BE2" alt="Релиз"></a>
  <a href="https://github.com/krambovic/Lumen/releases"><img src="https://img.shields.io/github/downloads/krambovic/Lumen/total?style=for-the-badge&label=%D0%A1%D0%BA%D0%B0%D1%87%D0%B8%D0%B2%D0%B0%D0%BD%D0%B8%D1%8F&labelColor=1C1C1C&color=17A673" alt="Скачивания"></a>
  <img src="https://img.shields.io/badge/Windows%20%7C%20Android-29B6F6?style=for-the-badge&labelColor=1C1C1C&label=%D0%9F%D0%BB%D0%B0%D1%82%D1%84%D0%BE%D1%80%D0%BC%D0%B0" alt="Платформа">
  <img src="https://img.shields.io/badge/GPL--3.0-F5A623?style=for-the-badge&labelColor=1C1C1C&label=%D0%9B%D0%B8%D1%86%D0%B5%D0%BD%D0%B7%D0%B8%D1%8F" alt="Лицензия">
</p>

<p align="center">
  <a href="README.md">English</a> · <b>Русский</b>
</p>

---

**Lumen** — клиент для VPN и обхода блокировок под **Windows** и **Android**. Одно приложение на всю цепочку: импортировать сервер или подписку, пустить через него только то, что нужно, и обойти DPI — с нормальным интерфейсом на обеих платформах вместо редактирования конфигов.

На Windows одновременно работают **xray-core** и **sing-box-extended**, плюс обход DPI на уровне пакетов. Android — нативный клиент на Jetpack Compose поверх **sing-box-extended** и системного `VpnService`.

> [!IMPORTANT]
> На Windows для режима TUN/VPN и обхода DPI (zapret) нужны права администратора.

---

## Возможности

| | Что делает | Windows | Android |
| :--- | :--- | :---: | :---: |
| **TUN / VPN** | Полный туннель через sing-box-extended, включая AmneziaWG (AWG 2.0) и WireGuard | ✅ | ✅ |
| **Системный прокси** | Направляет всю систему через xray-core без туннеля | ✅ | — |
| **Обход DPI** | zapret / WinDivert — разблокирует YouTube, Discord и другие сервисы на уровне пакетов | ✅ | — |
| **Раздельное туннелирование** | Выбор приложений, которые идут через VPN | — | ✅ |
| **Редактор маршрутов** | Пресеты плюс свои домены, IP-правила и поведение отдельных сервисов | ✅ | ✅ |
| **Discord Voice** | Пускает голос и стримы Discord через прокси без полного TUN | ✅ | — |
| **AUTO-пулы серверов** | Группы `urltest` выбирают самый быстрый сервер и перепроверяют его по таймеру | ✅ | ✅ |
| **Диагностика** | Замер задержки и реальной скорости; пинг по TCP, ICMP, HTTP GET и через сам прокси | ✅ | ✅ |
| **Быстрый доступ** | Меню в трее на Windows; виджеты и плитка в шторке на Android | ✅ | ✅ |
| **Темы** | Набор встроенных тем, а на Android ещё AMOLED-чёрный и Material You | ✅ | ✅ |

---

## Поддерживаемые протоколы

| Группа | Протоколы |
| :--- | :--- |
| **Базовые** | VLESS · VMess · Trojan · Shadowsocks · SOCKS · HTTP |
| **Современные** | Hysteria · Hysteria2 · TUIC · MASQUE · Mieru · AnyTLS · NaïveProxy |
| **WireGuard** | WireGuard · AmneziaWG (AWG 1.5 и 2.0) · Cloudflare WARP |
| **Прочее** | OpenVPN, в том числе через мосты obfs2/obfs3 |
| **Транспорты** | TCP · WebSocket · gRPC · HTTP/2 · HTTPUpgrade · XHTTP · mKCP · REALITY · uTLS |

Сырые JSON-конфиги **Xray** и **sing-box** импортируются как есть, включая документы с несколькими профилями.

---

## Подписки

- Обычные URL подписок и зашифрованные ссылки Happ: от `happ://crypt` до `happ://crypt5`.
- Подписки с привязкой по HWID — можно отправлять идентификатор установки или свой собственный HWID.
- Метаданные Happ Premium, остаток трафика и дата окончания видны прямо в списке серверов.
- Автообновление по расписанию: сначала используется собственный User-Agent Lumen, а другие профили клиентов подставляются только для панелей, которые смотрят на имя клиента.
- Сайт может передать подписку прямо в приложение:
  ```
  lumen://add?url=<URL-подписки-в-percent-encoding>&name=<необязательное-имя>
  ```

---

## Скриншоты

<details>
<summary><b>Windows — панель управления, темы и обход DPI</b></summary>
<br>

<img src="windows/assets/screenshots/dashboard-dark.png" alt="Панель управления в тёмной теме" width="100%">
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

## Установка

Свежая сборка — на странице **[Releases](https://github.com/krambovic/Lumen/releases)**.

| Платформа | Файл | Примечание |
| :--- | :--- | :--- |
| Windows | `Lumen-Setup-windows-x64.exe` | Рекомендуемый установщик |
| Windows | `Lumen-portable-windows-x64.zip` | Работает без установки |
| Android | `Lumen-<версия>-arm64-v8a.apk` | Почти все современные телефоны |
| Android | `Lumen-<версия>-x86_64.apk` | Эмуляторы и устройства на x86 |
| Android | `Lumen-<версия>-universal.apk` | Обе архитектуры в одном файле, крупнее |

> [!NOTE]
> Сборок под `armeabi-v7a` и `x86` нет: нативное ядро sing-box-extended под эти ABI не собирается, поэтому такой пакет всё равно не смог бы подключиться.

---

## Сборка

<details>
<summary><b>Windows</b> — Python 3, PyQt6</summary>

```powershell
cd windows
pip install -r requirements.txt
python build_qml.py   # установщик и портативный архив в windows/dist/
python run_qml.py     # запуск из исходников
pytest                # тесты
```
</details>

<details>
<summary><b>Android</b> — JDK 17, Gradle</summary>

```powershell
cd android
./gradlew assembleRelease    # APK в app/build/outputs/apk/release/
./gradlew testDebugUnitTest  # юнит-тесты
```

Подпись релиза берётся из `keystore.properties` рядом с `settings.gradle.kts`; без него сборка остаётся неподписанной.
</details>

---

## Структура репозитория

```
windows/   Десктопный клиент на Python + PyQt6/QML, xray-core, sing-box-extended, zapret
android/   Клиент на Jetpack Compose
  app/            навигация, view-модели, виджеты
  ui/             экраны Compose, темы, дизайн-токены
  core/config     разбор ссылок, сборка конфигов, нормализация AmneziaWG
  core/engine     управление процессом ядра
  core/vpn        VpnService, раздельное туннелирование, плитка в шторке
  core/database   хранилище Room
```

---

## Лицензия

Lumen распространяется под **GPL-3.0**. Сторонние компоненты сохраняют свои лицензии — см. [LICENSE](LICENSE) и [NOTICE.md](NOTICE.md).
