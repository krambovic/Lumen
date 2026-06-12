# Lumen KVN

<p align="center">
  <img src="assets/LumenKVN.png" alt="Lumen KVN" width="160">
</p>

Lumen KVN - самостоятельный Windows-клиент для VPN, системного прокси, TUN-режима и DPI bypass через zapret.

Проект сохраняет copyright и лицензионные уведомления исходных компонентов. Подробности смотрите в [LICENSE](LICENSE) и [NOTICE.md](NOTICE.md).

## Что Это

Lumen KVN объединяет в одном клиенте xray-core, sing-box-extended, zapret, маршрутизацию и удобное управление серверами без ручного редактирования JSON-конфигов.

Основные возможности:

- импорт VLESS, Trojan, Shadowsocks, VMess, WARP, WireGuard и AWG конфигов;
- системный proxy-режим через xray-core;
- TUN/VPN-режим через sing-box-extended;
- поддержка XHTTP, WARP и AWG 2.0 через sing-box-extended;
- встроенный zapret/DPI bypass;
- проверка ping и скорости серверов;
- маршрутизация по доменам, IP, сервисам и приложениям;
- пресеты маршрутизации: все, заблокированное, все кроме РФ;
- Discord voice proxy через droute без TUN;
- компактный и полный режим интерфейса;
- автообновление приложения и xray-core;
- QML-интерфейс с GPU-отрисовкой.

## Установка

Скачайте последнюю версию на странице [Releases](https://github.com/krambovic/lumen-kvn/releases).

- `LumenKVN-Setup-windows-x64.exe` - обычный установщик Windows. Приложение появится в списке установленных программ, меню Пуск и, при выборе опции, на рабочем столе.
- `LumenKVN-portable-windows-x64.zip` - portable-версия без установки.

Для TUN/VPN и zapret требуются права администратора.

## sing-box-extended

В Lumen KVN `sing-box-extended` используется как основное ядро TUN-режима.

Через него работают:

- WARP и WireGuard `.conf`;
- AWG 2.0 / AmneziaWG `.conf`;
- native XHTTP для поддерживаемых серверов;
- маршрутизация TUN по доменам, IP, сервисам и приложениям.

WARP/WireGuard/AWG-конфиги доступны только в TUN на sing-box-extended. В системном прокси они специально блокируются, потому что xray-core не может запускать такие конфиги напрямую.

## Импорт Конфигов

Обычные ссылки можно вставлять из буфера обмена.

Для WARP/WireGuard/AWG:

1. Откройте вкладку серверов.
2. Нажмите `Импорт .conf`.
3. Выберите файл с блоками `[Interface]` и `[Peer]`.

Также можно вставить в импорт сам текст `.conf`, обычный путь к файлу или `file:///...` ссылку.

AWG 2.0 параметры вроде `Jc`, `Jmin`, `Jmax`, `S1-S4`, `H1-H4` считываются из `[Interface]` автоматически.

## Сборка

```powershell
python build.py
```

Сборка создает:

- `dist/LumenKVN-Setup-windows-x64.exe`
- `dist/LumenKVN-portable-windows-x64.zip`

Перед релизной сборкой в каталоге `core/` должны лежать `xray.exe`, `sing-box.exe`, `tun2socks.exe`, `wintun.dll`, `geoip.dat` и `geosite.dat`.

## Сторонние Компоненты

В сборку входят сторонние компоненты со своими лицензиями:

- [Xray-core](https://github.com/XTLS/Xray-core)
- [sing-box-extended](https://github.com/shtorm-7/sing-box-extended)
- [tun2socks](https://github.com/xjasonlyu/tun2socks)
- zapret/WinDivert bundle в каталоге `zapret/`

Подробности смотрите в [NOTICE.md](NOTICE.md).

## Лицензия

GPL-3.0. См. [LICENSE](LICENSE).
