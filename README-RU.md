# Lumen KVN

<p align="center">
  <img src="assets/LumenKVN.png" alt="Lumen KVN" width="160">
</p>

**Язык:** [English](README.md) | Русский

Lumen KVN - самостоятельный Windows-клиент для VPN/TUN, системного прокси, маршрутизации, управления серверами и DPI bypass через zapret.

Проект сохраняет copyright и лицензионные уведомления исходных материалов и сторонних компонентов. Подробнее: [LICENSE](LICENSE) и [NOTICE.md](NOTICE.md).

## Что Это

Lumen KVN объединяет xray-core, sing-box-extended, zapret, пресеты маршрутизации и QML-интерфейс без необходимости вручную редактировать JSON-конфиги для обычных задач.

Основные возможности:

- импорт VLESS, Trojan, Shadowsocks, VMess, WARP, WireGuard и AWG/AmneziaWG конфигов;
- системный прокси через xray-core;
- VPN/TUN режим через sing-box-extended;
- поддержка XHTTP, WARP, WireGuard и AWG 2.0 через sing-box-extended;
- встроенный zapret/DPI bypass;
- проверка ping и скорости серверов;
- маршрутизация по доменам, IP, сервисам и приложениям;
- пресеты маршрутизации: всё, заблокированное и всё кроме РФ;
- Discord voice proxy через droute без TUN;
- компактный и полный режим интерфейса;
- обновление приложения, xray-core, sing-box-extended, geoip.dat и geosite.dat;
- QML-интерфейс с GPU-отрисовкой.

## Установка

Скачайте последнюю версию на странице [Releases](https://github.com/krambovic/lumen-kvn/releases).

- `LumenKVN-Setup-windows-x64.exe` - обычный установщик Windows.
- `LumenKVN-portable-windows-x64.zip` - portable-версия без установки.

Для TUN/VPN и zapret требуются права администратора.

## sing-box-extended

В Lumen KVN `sing-box-extended` используется как основное ядро TUN-режима.

Через него работают:

- WARP и WireGuard `.conf`;
- AWG 2.0 / AmneziaWG `.conf`;
- native XHTTP для поддерживаемых серверов;
- маршрутизация TUN по доменам, IP, сервисам и приложениям.

WARP/WireGuard/AWG-конфиги доступны только в TUN на sing-box-extended. В системном прокси они специально недоступны, потому что xray-core не может запускать такие конфиги напрямую.

## Импорт Конфигов

Обычные proxy-ссылки можно вставлять из буфера обмена.

Для WARP/WireGuard/AWG:

1. Откройте вкладку серверов.
2. Нажмите `Import .conf`.
3. Выберите файл с блоками `[Interface]` и `[Peer]`.

Также можно вставить сам текст `.conf`, обычный путь к файлу или `file:///...` ссылку.

AWG 2.0 параметры вроде `Jc`, `Jmin`, `Jmax`, `S1-S4`, `H1-H4` считываются из `[Interface]` автоматически.

## Сборка

```powershell
python build_qml.py
```

Сборка создает:

- `dist/LumenKVN-Setup-windows-x64.exe`
- `dist/LumenKVN-portable-windows-x64.zip`

Перед релизной сборкой в каталоге `core/` должны лежать `xray.exe`, `sing-box.exe`, `wintun.dll`, `geoip.dat` и `geosite.dat`.

## Сторонние Компоненты

Сторонние компоненты сохраняют свои лицензии и notices:

- [Xray-core](https://github.com/XTLS/Xray-core)
- [sing-box-extended](https://github.com/shtorm-7/sing-box-extended)
- zapret/WinDivert bundle в каталоге `zapret/`
- droute helper для Discord voice proxy, скачивается/используется отдельно

Подробнее: [NOTICE.md](NOTICE.md).

## Лицензия

GPL-3.0. См. [LICENSE](LICENSE).
