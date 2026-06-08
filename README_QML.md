# Bebra VPN — QML-фронтенд

Интерфейс полностью переписан с QtWidgets + qfluentwidgets на **Qt Quick / QML**
с GPU-рендерингом через scene graph. Бэкенд (`AppController`, `XrayManager`,
`SingBoxManager`, `ZapretManager`, хранилище, маршрутизация) **не изменён** —
новый UI общается с ним через слой-мост (`xray_fluent/qml_app/bridge/`).

Это «ночная» (nightly) редакция приложения. Классическая (stable) сборка на
QtWidgets продолжает существовать параллельно — см. раздел «Сборка».

## Почему это решает проблемы производительности

- **Отрисовка на GPU.** Весь UI рендерит scene graph (OpenGL/D3D), а не CPU-отрисовка
  QWidget. Скроллинг и анимации идут на частоте монитора (60/120/144 Гц) без
  хаков вроде `_unlock_qfluent_smooth_scroll_fps`.
- **Нет попиксельных пересчётов.** Списки (`ListView`) переиспользуют делегаты,
  а не создают виджет на каждую строку.
- **Модели вместо ручных обновлений лейблов.** Метрики, логи и узлы приходят
  через property-binding и `QAbstractListModel`; обновляются только изменённые ячейки.
- **График трафика** рисуется на `Canvas` с ring-буфером, перерисовка по тику
  метрик (~1 Гц), а не на каждый кадр.

Требуется только `PyQt6` (Qt Quick/QML входят в колесо PyQt6). Отдельный
`PyQt6-Fluent-Widgets` новому фронтенду НЕ нужен.

## Запуск

```bash
python run_qml.py
```

Точка входа — `xray_fluent.qml_app.main_qml:main`: создаёт QGuiApplication +
QQmlApplicationEngine, поднимает трей и грузит `qml/Main.qml`.

## Сборка

Две редакции собираются и распространяются раздельно, но в одном GitHub-релизе:

| Редакция | Канал | Сборщик | Артефакты |
| --- | --- | --- | --- |
| Классическая (QtWidgets) | stable | `build.py` | `BebraVPN-portable-windows-x64.zip`, `BebraVPN-Setup-windows-x64.exe` |
| Новая (QML) | nightly | `build_qml.py` | `BebraVPN-nightly-portable-windows-x64.zip`, `BebraVPN-nightly-Setup-windows-x64.exe` |

QML-сборка:

```bash
python build_qml.py                # portable zip + установщик
python build_qml.py --no-installer # только portable zip
python build_qml.py --no-zip       # только установщик
python build_qml.py --clean        # очистить build/ и dist/qml перед сборкой
```

Под капотом `build_qml.py` вызывает `pyinstaller BebraVPN-qml.spec`, пакует
portable-zip из `dist/qml/BebraVPN` и собирает установщик Inno Setup
(`installer/BebraVPN.iss`) с отдельной identity «Bebra VPN Nightly» (свой
AppId), чтобы ставиться рядом со стабильной версией, не перезаписывая её.

PyInstaller можно дёрнуть и напрямую:

```bash
pyinstaller BebraVPN-qml.spec --noconfirm --clean
```

Спек `BebraVPN-qml.spec`: точка входа `run_qml.py`; копирует все `.qml`;
добавляет QML-плагины Qt Quick / Controls (Universal) и скрытые импорты;
исключает `qfluentwidgets`.

> `build.py` умеет собирать обе редакции сразу (флаги `--qml-only` / `--no-qml`).

## Обновления

Обе редакции тянут обновления из одного GitHub-релиза; канал решает, какой
ассет качать. Апдейтер (`app_updater.py`, `_asset_score`) для nightly выбирает
ассет с «nightly» в имени (старый суффикс «qml» тоже поддерживается для
совместимости), для stable — ассет без него.

## Архитектура

```
xray_fluent/qml_app/
  main_qml.py            # QGuiApplication + QQmlApplicationEngine + трей, грузит qml/Main.qml
  tray.py                # QmlTray: иконка в трее, сворачивание в фон
  bridge/
    app_bridge.py        # AppBridge(QObject): сигналы/слоты/свойства ↔ AppController (контекст «App»)
    node_list_model.py   # модель списка серверов
    log_model.py         # модель логов (кольцевой буфер)
    process_model.py     # модель потребления процессами
    configs_helpers.py   # хелперы редактора конфигов
    zapret_helpers.py    # хелперы Zapret
    history_helpers.py   # хелперы истории трафика
    node_edit_helpers.py # хелперы редактирования/импорта узлов
  qml/
    Main.qml             # окно + Mica + навигация + StackLayout + ToastHost + lock-overlay
    Theme.qml            # singleton: цвета/размеры/хелперы
    Card / AccentButton / NavButton / ToastHost / TrafficGraph
    FluentCombo / FluentScroll / FluentScrollBar          # Fluent-компоненты
    NodeEditDialog / BulkEditDialog / ColorPickerDialog    # диалоги
    DashboardPage / NodesPage / RoutingPage / ConfigsPage / ZapretPage /
    LogsPage / HistoryPage / UpdatesPage / SettingsPage / AboutPage
```

Мост доступен в QML как singleton `App` (напр. `App.toggleConnection()`,
`App.nodeModel`, `App.downBps`, `App.locked`). `Theme` — singleton, `import "."`.

## Разделы

- ✅ **Панель** — статус, connect, график трафика, латентность, режимы маршрутизации, TUN/proxy/Discord, процессы.
- ✅ **Серверы** — список с переиспользованием, мультивыбор, пинг/speedtest/импорт/удаление, редактирование и массовое редактирование.
- ✅ **Маршруты** — режимы global/rule/direct + пресеты.
- ✅ **Конфиги** — редактор sing-box / Xray.
- ✅ **Zapret**.
- ✅ **Логи** — живой поток, follow-tail, очистка.
- ✅ **История** — графики трафика.
- ✅ **Обновления** — обновление приложения и ядра Xray.
- ✅ **Настройки** — тема, акцент (палитра), компактный режим, пути к ядрам, автозапуск / старт свёрнутым, запуск от админа, трей/фон, мастер-пароль/блокировка.
- ✅ **О проекте**.


## Ограничения

- Подробная статистика по процессам доступна только в режиме sing-box TUN.
- Запуск GUI / сборку .exe нельзя проверить в этой среде — запускайте `python run_qml.py` на Windows; при ошибках импорта QML пришлите трейсбек.
