# Lumen KVN — QML-фронтенд

QML-редакция Lumen KVN использует Qt Quick / QML и GPU-rendering через scene graph. Бэкенд остаётся тем же: `AppController`, менеджеры xray/sing-box/zapret, хранилище, маршрутизация и воркеры.

## Зачем Это Нужно

- Интерфейс отрисовывается на GPU, а не через тяжёлые QWidget-перерисовки.
- Списки используют `ListView` и модели, поэтому не создают отдельный виджет на каждую строку.
- Логи, серверы и процессы обновляют только изменённые элементы.
- График трафика рисуется через ring-buffer и не перегружает event loop.

## Запуск Из Исходников

```bash
python run_qml.py
```

## Сборка

Две редакции собираются раздельно, но публикуются в одном релизе:

| Редакция | Канал | Сборщик | Артефакты |
| --- | --- | --- | --- |
| Классическая QtWidgets | stable | `build.py` | `LumenKVN-portable-windows-x64.zip`, `LumenKVN-Setup-windows-x64.exe` |
| QML | nightly | `build_qml.py` | `LumenKVN-nightly-portable-windows-x64.zip`, `LumenKVN-nightly-Setup-windows-x64.exe` |

```bash
python build.py                 # обе редакции
python build.py --no-qml        # только stable
python build.py --qml-only      # только nightly
python build_qml.py             # nightly напрямую
```

Под капотом `build_qml.py` вызывает `pyinstaller LumenKVN-qml.spec`, пакует portable-zip из `dist/LumenKVN Nightly` и собирает установщик Inno Setup (`installer/LumenKVN.iss`) с отдельной identity «Lumen KVN Nightly».

## Обновления

Обе редакции проверяют релизы [krambovic/lumen-kvn](https://github.com/krambovic/lumen-kvn/releases). Stable выбирает asset без `nightly` в имени, nightly выбирает asset с `nightly`.

## Архитектура

```text
xray_fluent/qml_app/
  main_qml.py            # QGuiApplication + QQmlApplicationEngine
  tray.py                # tray icon и сворачивание
  bridge/                # QObject-мост между QML и AppController
  qml/                   # страницы, компоненты, тема, диалоги
```

QML получает доступ к приложению через singleton `App`, например `App.toggleConnection()`, `App.nodeModel`, `App.downBps`, `App.locked`.
