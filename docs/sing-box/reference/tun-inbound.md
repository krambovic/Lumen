# TUN Inbound -- справочник полей

Целевая версия: sing-box 1.14.x
Только поля, поддерживаемые на Windows.

## Структура

```json
{
  "type": "tun",
  "tag": "tun-in",
  "interface_name": "xftun0",
  "address": ["172.19.0.1/30"],
  "mtu": 9000,
  "auto_route": true,
  "strict_route": false,
  "stack": "mixed",
  "route_address": [],
  "route_exclude_address": [],
  "route_address_set": [],
  "route_exclude_address_set": [],
  "endpoint_independent_nat": false,
  "udp_timeout": "5m"
}
```

## Поля

### interface_name

| | |
|---|---|
| Тип | `string` |
| По умолчанию | автовыбор |

Имя виртуального сетевого адаптера. Если пустое, система выберет имя автоматически.
Lumen KVN генерирует имя с префиксом `xftun` и случайным суффиксом (например `xftun3a1b2c`).

### address

| | |
|---|---|
| Тип | `string` или `string[]` |
| С версии | 1.10.0 |

Список IPv4/IPv6 CIDR-адресов для TUN-интерфейса.
Пример: `["172.19.0.1/30", "fdfe:dcba:9876::1/126"]`.
Заменяет устаревшие `inet4_address` / `inet6_address`.

### mtu

| | |
|---|---|
| Тип | `number` |
| По умолчанию | `9000` |

Максимальный размер передаваемого блока данных (Maximum Transmission Unit).

### auto_route

| | |
|---|---|
| Тип | `bool` |
| По умолчанию | `false` |

Устанавливает маршрут по умолчанию через TUN-адаптер. Должно быть `true` для перехвата трафика.

**Важно:** чтобы избежать петли маршрутизации, установите `route.auto_detect_interface` или `route.default_interface`, либо используйте `outbound.bind_interface`.

### strict_route

| | |
|---|---|
| Тип | `bool` |
| По умолчанию | `false` |

Применяет строгие правила маршрутизации при включённом `auto_route`:
- Делает неподдерживаемые сети недоступными.
- Предотвращает утечку DNS через стандартное поведение Windows (multihomed DNS resolution).

**Предупреждение (Windows):** значение `true` может нарушить работу VirtualBox и вызвать петлю маршрутизации для direct-исходящих соединений на некоторых сборках Windows. Lumen KVN по умолчанию использует `false`.

### stack

| | |
|---|---|
| Тип | `string` |
| По умолчанию | `mixed` (если gVisor включён в сборку) |

TCP/IP стек для обработки трафика:

| Значение | Описание |
|----------|----------|
| `system` | L3->L4 через сетевой стек ОС |
| `gvisor` | L3->L4 через виртуальный стек gVisor |
| `mixed` | TCP через system, UDP через gvisor |

### route_address

| | |
|---|---|
| Тип | `string[]` |
| С версии | 1.10.0 |

Пользовательские CIDR-маршруты вместо маршрутов по умолчанию при включённом `auto_route`.
Пример: `["0.0.0.0/1", "128.0.0.0/1"]`.

### route_exclude_address

| | |
|---|---|
| Тип | `string[]` |
| С версии | 1.10.0 |

CIDR-адреса, исключённые из маршрутизации через TUN.
Пример: `["192.168.0.0/16"]`.

### route_address_set

| | |
|---|---|
| Тип | `string[]` |
| С версии | 1.11.0 |

Добавляет IP-адреса из указанных rule-set в маршруты TUN (эквивалент добавления в `route_address`). Трафик, не соответствующий правилам, обходит sing-box.

### route_exclude_address_set

| | |
|---|---|
| Тип | `string[]` |
| С версии | 1.11.0 |

Исключает IP-адреса из указанных rule-set из маршрутов TUN (эквивалент добавления в `route_exclude_address`).

### endpoint_independent_nat

| | |
|---|---|
| Тип | `bool` |
| По умолчанию | `false` |

Включает Endpoint-Independent NAT. Работает только со стеком `gvisor` (остальные стеки используют EI-NAT по умолчанию). Незначительно снижает производительность -- не рекомендуется без необходимости.

### udp_timeout

| | |
|---|---|
| Тип | `string` (duration) |
| По умолчанию | `"5m"` |

Время жизни UDP NAT-записи. Формат: golang duration (например `"5m"`, `"300s"`).

## Не поддерживается на Windows

| Поле | Платформа |
|------|-----------|
| `auto_redirect` | Linux (nftables) |
| `auto_redirect_input_mark` | Linux (nftables) |
| `auto_redirect_output_mark` | Linux (nftables) |
| `auto_redirect_reset_mark` | Linux (nftables, 1.13) |
| `auto_redirect_nfqueue` | Linux (nftables, 1.13) |
| `auto_redirect_iproute2_fallback_rule_index` | Linux (1.12) |
| `iproute2_table_index` | Linux |
| `iproute2_rule_index` | Linux |
| `include_interface` | Linux |
| `exclude_interface` | Linux |
| `include_uid`, `include_uid_range` | Linux |
| `exclude_uid`, `exclude_uid_range` | Linux |
| `include_android_user` | Android |
| `include_package`, `exclude_package` | Android |
| `include_mac_address` | Linux (1.14) |
| `exclude_mac_address` | Linux (1.14) |
| `exclude_mptcp` | Linux (1.13) |
| `loopback_address` | Linux (auto_redirect, 1.12) |

## Пример

Конфигурация TUN, генерируемая Lumen KVN по умолчанию:

```json
{
  "type": "tun",
  "tag": "tun-in",
  "interface_name": "xftun3a1b2c",
  "address": ["172.19.0.1/30"],
  "auto_route": true,
  "strict_route": false,
  "stack": "mixed"
}
```
