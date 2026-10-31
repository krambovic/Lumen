import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import App 1.0
import "."

// Servers tab.
Item {
    id: page
    enabled: App.profileLoaded

    Timer {
        id: searchFilterDebounce
        interval: 180
        repeat: false
        onTriggered: page.applyFilters()
    }

    // ── selection state ──────────────────────────
    property var sel: ({})        // nodeId -> true
    property int selCount: 0
    property int selRev: 0        // bumped on every selection change so delegates re-evaluate
    property int anchorRow: -1
    property int menuRow: -1
    property bool compact: App.compactMode
    property bool selectionDragging: false
    property real selectionPointerX: -1
    property real selectionPointerY: -1
    property int hoverRow: -1
    property real savedListContentY: 0
    property int inspectorTab: 0

    // First selected server, else the active one: shown on the inspector's «Сервер» tab.
    readonly property string inspectedNodeId: {
        var rev = page.selRev;
        var first = page.firstSelected();
        return first.length ? first : String(App.selectedNodeId || "");
    }
    property int detailsTick: 0
    readonly property var inspectedNode: {
        var tick = page.detailsTick;
        var id = page.inspectedNodeId;
        if (!id) return null;
        var d = App.nodeDetails(id);
        return d && d.id !== undefined ? d : null;
    }
    // Refresh the inspector when pings, speeds or the list change.
    Connections {
        target: App.nodeModel
        function onDataChanged() { page.detailsTick++ }
        function onModelReset() { page.detailsTick++ }
    }
    function pingKindLabel(kind) {
        if (kind === "proxy") return I18n.t("Через прокси");
        if (kind === "tcping_endpoint") return I18n.t("TCP до адреса");
        if (kind === "icmp_endpoint") return I18n.t("ICMP до адреса");
        if (kind === "unavailable") return I18n.t("Недоступно");
        return "";
    }
    function serverDetailRows(d) {
        var rows = [[I18n.t("Адрес"), d.server + (d.port > 0 ? ":" + d.port : "")],
                    [I18n.t("Протокол"), d.protocol],
                    [I18n.t("Транспорт"), d.transport],
                    [I18n.t("Безопасность"), d.security || "—"]];
        if (d.sni) rows.push(["SNI", d.sni]);
        if (d.fingerprint) rows.push([I18n.t("Отпечаток"), d.fingerprint]);
        if (d.country) rows.push([I18n.t("Страна"), d.country]);
        rows.push([I18n.t("Группа"), d.group]);
        if (d.subscription) rows.push([I18n.t("Подписка"), d.subscription]);
        if (page.pingKindLabel(d.pingKind)) rows.push([I18n.t("Тип пинга"), page.pingKindLabel(d.pingKind)]);
        if (d.lastUsed) rows.push([I18n.t("Последнее использование"), page.fmtTime(d.lastUsed)]);
        if (d.description) rows.push([I18n.t("Описание"), d.description]);
        return rows;
    }
    function visibleIds() {
        var ids = [];
        for (var i = 0; i < list.count; i++) {
            var id = App.nodeIdAt(i);
            if (id) ids.push(id);
        }
        return ids;
    }
    function timeMs(v) {
        if (v === undefined || v === null || v === "") return NaN;
        if (typeof v === "number") return v > 32000000000 ? v : v * 1000;
        var t = String(v).trim();
        if (/^\d{9,}$/.test(t)) return Number(t) > 32000000000 ? Number(t) : Number(t) * 1000;
        return new Date(t).getTime();
    }
    // Traffic, expiry and status of a subscription (inspector and source picker).
    function computeSubState(s) {
        var r = { active: true, status: "", title: "", hasTraffic: false, fraction: 0,
                  usedText: "", limitText: "", expires: "", daysLeft: "", soon: false,
                  limited: false, line: "", lineHtml: "", expired: false,
                  trafficColor: Theme.success, statusColor: Theme.success };
        if (!s) return r;
        var ui = (s.userinfo && typeof s.userinfo === "object") ? s.userinfo : {};
        r.title = String(ui.profileTitle || "");
        var hasUsed = ui.trafficUsedBytes !== undefined || ui.total !== undefined
            || ui.upload !== undefined || ui.download !== undefined;
        var used = ui.trafficUsedBytes !== undefined ? Number(ui.trafficUsedBytes)
            : Number(ui.upload || 0) + Number(ui.download || 0);
        var limit = ui.trafficLimitBytes !== undefined ? Number(ui.trafficLimitBytes)
            : (ui.total !== undefined ? Number(ui.total) : NaN);
        if (hasUsed || ui.trafficLimitBytes !== undefined) {
            r.hasTraffic = true;
            r.usedText = page.fmtBytes(isFinite(used) ? used : 0);
            if (isFinite(limit) && limit > 0) {
                r.limited = true;
                r.fraction = Math.max(0, Math.min(1, used / limit));
                r.limitText = I18n.t("из {total}", { total: page.fmtBytes(limit) });
            } else {
                r.limitText = I18n.t("Безлимитный трафик");
            }
        }
        var exp = ui.expiresAt || (Number(ui.expire) > 0 ? ui.expire : "");
        if (exp) {
            r.expires = page.fmtTime(exp);
            var ms = page.timeMs(exp);
            if (isFinite(ms)) {
                var days = Math.ceil((ms - Date.now()) / 86400000);
                r.daysLeft = days > 0 ? I18n.t("осталось {days} дн.", { days: days }) : I18n.t("Подписка истекла");
                r.soon = days <= 3;
                r.expired = days <= 0;
                if (days <= 0) r.active = false;
            }
        }
        if (ui.userStatus) {
            r.status = String(ui.userStatus);
            r.active = !/(expired|disabled|limited|inactive)/i.test(r.status);
        } else if (ui.isActive !== undefined) {
            r.active = !!ui.isActive && r.active;
            r.status = r.active ? I18n.t("Активна") : I18n.t("Неактивна");
        } else if (r.expires.length) {
            r.status = r.active ? I18n.t("Активна") : I18n.t("Неактивна");
        }
        if (r.expired) {
            r.hasTraffic = false;
            r.limited = false;
        }
        var parts = [];
        if (r.hasTraffic) parts.push(r.limited ? (r.usedText + " " + r.limitText) : I18n.t("Безлимитный трафик"));
        if (r.daysLeft.length) parts.push(r.daysLeft);
        r.line = parts.join(" · ");
        // Traffic colour follows what is left: green → yellow (75%) → red (90%).
        r.trafficColor = !r.limited ? Theme.success : (r.fraction >= 0.9 ? Theme.danger : (r.fraction >= 0.75 ? Theme.warning : Theme.success));
        r.statusColor = r.active ? Theme.success : Theme.danger;
        var html = [];
        if (r.hasTraffic)
            html.push('<font color="' + r.trafficColor + '">' + (r.limited ? r.usedText + " " + r.limitText : I18n.t("Безлимитный трафик")) + "</font>");
        if (r.daysLeft.length)
            html.push('<font color="' + (r.expired ? Theme.danger : (r.soon ? Theme.warning : Theme.textMuted)) + '">' + r.daysLeft + "</font>");
        r.lineHtml = html.join('<font color="' + Theme.textFaint + '"> · </font>');
        return r;
    }
    readonly property var subState: { var tick = page.providerClock; return page.computeSubState(page.activeSub); }

    // ── filtering / sorting state ───────────────────
    property string filterText: ""
    property string filterGroup: App.nodeFilterGroup   // "" = all groups
    property string sortKey: App.nodeSortKey
    property bool sortAsc: App.nodeSortAscending

    readonly property var groupModel: {
        var groups = (App.groupOptions || []).slice();
        var subs = App.subscriptions || [];
        for (var i = 0; i < subs.length; ++i) {
            var group = String(subs[i].group || subs[i].name || "Default");
            if (groups.indexOf(group) < 0) groups.push(group);
        }
        return [I18n.t("Все группы")].concat(groups);
    }
    readonly property var sortLabels: [I18n.t("Вручную"), I18n.t("Имя"), I18n.t("Группа"), I18n.t("Тип"), I18n.t("Транспорт"), I18n.t("Пинг"), I18n.t("Скорость"), I18n.t("Последнее использование"), I18n.t("Адрес"), I18n.t("Статус")]
    readonly property var sortKeys: ["manual", "name", "group", "scheme", "transport", "ping", "speed", "last", "address", "status"]
    readonly property bool hasSubscriptionMeta: page.subscriptionMeta(page.selectedSub()) !== null

    property bool viewStateRestored: false
    // ── sizing ───────────────────────────────
    // Original compact server-row sizing.
    readonly property int rowH: compact ? 30 : 34
    readonly property int cellFont: 13

    // Column widths: defaults, or what the user dragged.
    readonly property var savedTableLayout: App.nodeTableLayout || ({})
    property bool manualColumnWidths: savedTableLayout.manual === true
    property real manualColName: savedTableLayout.name !== undefined ? Number(savedTableLayout.name) : 200
    property real manualColType: savedTableLayout.type !== undefined ? Number(savedTableLayout.type) : 80
    property real manualColTransport: savedTableLayout.transport !== undefined ? Number(savedTableLayout.transport) : 92
    property real manualColAddr: savedTableLayout.address !== undefined ? Number(savedTableLayout.address) : 160
    property real manualColPort: savedTableLayout.port !== undefined ? Number(savedTableLayout.port) : 60
    property real manualColGroup: savedTableLayout.group !== undefined ? Number(savedTableLayout.group) : 120
    property real manualColPing: savedTableLayout.ping !== undefined ? Number(savedTableLayout.ping) : 76
    property real manualColSpeed: savedTableLayout.speed !== undefined ? Number(savedTableLayout.speed) : 100
    property real manualColStatus: savedTableLayout.status !== undefined ? Number(savedTableLayout.status) : 64
    property real manualColLast: savedTableLayout.last !== undefined ? Number(savedTableLayout.last) : 130
    readonly property int leftPad: 12
    readonly property int trailPad: 12

    // Until a divider is dragged the columns fit the card: «Сервер» takes the
    // spare space, and on a narrow card it gives way first, then the others
    // shrink toward readable minimums. After a drag the widths are exact.
    readonly property var colMins: [140, 56, 60, 110, 44, 70, 56, 76, 50, 70]
    readonly property var colWidths: {
        var want = [manualColName, manualColType, manualColTransport, manualColAddr, manualColPort,
                    manualColGroup, manualColPing, manualColSpeed, manualColStatus, manualColLast].map(Math.round);
        var avail = Math.floor(hflick.width) - leftPad - trailPad;
        var total = 0;
        for (var i = 0; i < want.length; i++) total += want[i];
        var out = want.slice();
        if (manualColumnWidths) {
            // Dragged widths are preferences: spare space goes to the last
            // column; on a narrow card the other columns give way down to their
            // minimums while the column being dragged keeps its width.
            if (avail >= total) {
                out[out.length - 1] = want[want.length - 1] + (avail - total);
                return out;
            }
            var over = total - avail;
            var room = 0;
            for (var m = 0; m < want.length; m++)
                if (m !== page.lastResized) room += Math.max(0, want[m] - colMins[m]);
            if (room > 0) {
                var f = Math.min(1, over / room);
                for (var n = 0; n < want.length; n++)
                    if (n !== page.lastResized)
                        out[n] = Math.round(want[n] - Math.max(0, want[n] - colMins[n]) * f);
            }
            return out;
        }
        if (avail >= total) {
            out[0] = want[0] + (avail - total);
            return out;
        }
        var deficit = total - avail;
        var nameCut = Math.min(deficit, Math.max(0, want[0] - colMins[0]));
        out[0] = want[0] - nameCut;
        deficit -= nameCut;
        var slack = 0;
        for (var j = 1; j < want.length; j++) slack += Math.max(0, want[j] - colMins[j]);
        if (deficit > 0 && slack > 0) {
            var k = Math.min(1, deficit / slack);
            for (var c = 1; c < want.length; c++)
                out[c] = Math.round(want[c] - Math.max(0, want[c] - colMins[c]) * k);
        }
        return out;
    }
    readonly property int colName: colWidths[0]
    readonly property int colType: colWidths[1]
    readonly property int colTransport: colWidths[2]
    readonly property int colAddr: colWidths[3]
    readonly property int colPort: colWidths[4]
    readonly property int colGroup: colWidths[5]
    readonly property int colPing: colWidths[6]
    readonly property int colSpeed: colWidths[7]
    readonly property int colStatus: colWidths[8]
    readonly property int colLast: colWidths[9]
    readonly property int tableWidth: Math.max(Math.floor(hflick.width), leftPad + trailPad + colName + colType + colTransport
        + colAddr + colPort + colGroup + colPing + colSpeed + colStatus + colLast)

    // First drag: keep what is on screen, then switch to exact widths.
    function freezeColumnWidths() {
        if (manualColumnWidths)
            return;
        var w = colWidths;
        manualColName = w[0];
        manualColType = w[1];
        manualColTransport = w[2];
        manualColAddr = w[3];
        manualColPort = w[4];
        manualColGroup = w[5];
        manualColPing = w[6];
        manualColSpeed = w[7];
        manualColStatus = w[8];
        manualColLast = w[9];
        manualColumnWidths = true;
    }

    property int lastResized: -1
    function resizeColumn(index, width) {
        freezeColumnWidths();
        page.lastResized = index;
        var value = Math.max(index === 0 || index === 3 ? 90 : 40, Math.round(width));
        if (index === 0) manualColName = value;
        else if (index === 1) manualColType = value;
        else if (index === 2) manualColTransport = value;
        else if (index === 3) manualColAddr = value;
        else if (index === 4) manualColPort = value;
        else if (index === 5) manualColGroup = value;
        else if (index === 6) manualColPing = value;
        else if (index === 7) manualColSpeed = value;
        else if (index === 8) manualColStatus = value;
        else if (index === 9) manualColLast = value;
        persistTableLayout();
    }

    function persistTableLayout() {
        App.setNodeTableLayout({
            "manual": manualColumnWidths,
            "name": manualColName,
            "type": manualColType,
            "transport": manualColTransport,
            "address": manualColAddr,
            "port": manualColPort,
            "group": manualColGroup,
            "ping": manualColPing,
            "speed": manualColSpeed,
            "status": manualColStatus,
            "last": manualColLast
        });
    }

    // ── selection helpers ─────────────────────
    function _commit(obj, count) {
        sel = obj;
        selCount = count;
        selRev++;
    }
    function isSelected(id) { return sel[id] === true; }
    function selectedIds() { return Object.keys(sel); }
    function firstSelected() { var k = Object.keys(sel); return k.length ? k[0] : ""; }
    function testTargets() { return page.selCount > 0 ? page.selectedIds() : []; }
    property int providerClock: 0
    Timer { interval: 60000; repeat: true; running: page.visible; onTriggered: page.providerClock++ }
    onVisibleChanged: providerClock++
    readonly property var activeSub: { var tick = providerClock; return selectedSub(); }
    readonly property var providerView: activeSub && activeSub.presentation ? activeSub.presentation : ({})

    // Keep subscription banners in a QML model so the view can update without
    // recreating the servers page or leaving stale banner rows behind.
    QtObject {
        id: providerBanners
        property var rows: []
        readonly property int count: rows.length
        function clear() { rows = []; }
        function append(row) { rows = rows.concat([row]); }
        function itemAt(index) { return rows[index] || null; }
    }
    function syncProviderBanners() {
        providerBanners.clear();
        var banners = page.providerView.banners || [];
        for (var i = 0; i < banners.length; ++i) {
            var banner = banners[i] || {};
            providerBanners.append({
                text: I18n.t(String(banner.text || banner.title || "")),
                visible: banner.visible !== false
            });
        }
    }
    onProviderViewChanged: syncProviderBanners()
    function selectedSub() {
        var wanted = String(App.selectedSubscriptionId || "");
        var subs = App.subscriptions || [];
        for (var i = 0; i < subs.length; ++i) {
            if (String(subs[i].id || "") === wanted && wanted.length) return subs[i];
        }
        return null;
    }
    function subscriptionSummary(sub) {
        if (!sub) return "";
        var ui = sub.userinfo || {};
        var parts = [];
        var used = ui.trafficUsedBytes !== undefined ? Number(ui.trafficUsedBytes) : Number(ui.upload || 0) + Number(ui.download || 0);
        var total = ui.trafficLimitBytes !== undefined ? Number(ui.trafficLimitBytes) : Number(ui.total);
        if (isFinite(total)) parts.push(I18n.t("Трафик") + ": " + page.fmtBytes(used) + " / " + (total <= 0 ? I18n.t("Безлимитный трафик") : page.fmtBytes(total)));
        if (ui.expiresAt || Number(ui.expire) > 0) parts.push(I18n.t("Истекает") + ": " + page.fmtTime(ui.expiresAt || ui.expire));
        return parts.join("  ·  ");
    }
    // Подписка, выбранная в диалоге свойств (может отличаться от subCombo).
    function infoSub() {
        var subs = App.subscriptions;
        if (!subs || subs.length === 0) return null;
        var i = infoCombo.currentIndex;
        if (i < 0 || i >= subs.length) return null;
        return subs[i];
    }
    // Человекочитаемый размер: байты → КБ/МБ/ГБ/ТБ.
    function fmtBytes(n) {
        var b = Number(n);
        if (!isFinite(b) || b < 0) return String(n);
        var u = [I18n.t("Б"), I18n.t("КБ"), I18n.t("МБ"), I18n.t("ГБ"), I18n.t("ТБ"), I18n.t("ПБ")];
        var i = 0;
        while (b >= 1024 && i < u.length - 1) { b /= 1024; i++; }
        return (i === 0 ? String(Math.round(b)) : b.toFixed(2)) + " " + u[i];
    }
    // Время → локальный часовой пояс компьютера (из unix-секунд или ISO/UTC).
    function fmtTime(v) {
        if (v === undefined || v === null || v === "") return "";
        var d;
        if (typeof v === "number") d = new Date(v > 32000000000 ? v : v * 1000);
        else {
            var s = String(v).trim();
            if (/^\d{9,}$/.test(s)) d = new Date(Number(s) > 32000000000 ? Number(s) : Number(s) * 1000);
            else d = new Date(s);
        }
        if (isNaN(d.getTime())) return String(v);
        return Qt.formatDateTime(d, "dd.MM.yyyy HH:mm");
    }
    // Строит список [метка, значение] из данных подписки.
    function infoRows(sub) {
        var rows = [];
        if (!sub) return rows;
        rows.push([I18n.t("Группа"), (sub.name && sub.name.length ? sub.name : "—")]);
        if (sub.url) rows.push(sub.presentation && sub.presentation.hideUrl ? [I18n.t("Ссылка подписки"), I18n.t("Скрыта провайдером")] : ["URL", sub.url]);
        if (sub.updated_at) rows.push([I18n.t("Обновлено"), fmtTime(sub.updated_at)]);
        rows.push([I18n.t("Серверов"), String(sub.node_count || 0)]);
        var ui = sub.userinfo;
        if (ui && typeof ui === "object") {
            if (ui.username) rows.push([I18n.t("Пользователь"), String(ui.username)]);
            if (ui.userStatus) rows.push([I18n.t("Статус"), String(ui.userStatus)]);
            else if (ui.isActive !== undefined) rows.push([I18n.t("Статус"), ui.isActive ? I18n.t("Активна") : I18n.t("Неактивна")]);
            if (ui.daysLeft !== undefined) rows.push([I18n.t("Осталось дней"), String(ui.daysLeft)]);
            var usedB = (ui.trafficUsedBytes !== undefined) ? Number(ui.trafficUsedBytes)
                      : ((ui.total !== undefined) ? (Number(ui.upload || 0) + Number(ui.download || 0)) : undefined);
            var limitB = (ui.trafficLimitBytes !== undefined) ? Number(ui.trafficLimitBytes)
                       : ((ui.total !== undefined) ? Number(ui.total) : undefined);
            if (usedB !== undefined || limitB !== undefined) {
                if (limitB !== undefined && isFinite(limitB) && limitB <= 0) {
                    rows.push([I18n.t("Трафик"), I18n.t("Безлимитный трафик")]);
                } else {
                    var uStr = (usedB !== undefined) ? fmtBytes(usedB)
                             : (ui.trafficUsed !== undefined ? String(ui.trafficUsed) : "?");
                    var lStr = (limitB !== undefined) ? fmtBytes(limitB)
                             : (ui.trafficLimit !== undefined ? String(ui.trafficLimit) : "∞");
                    rows.push([I18n.t("Трафик"), uStr + " / " + lStr]);
                }
            } else if (ui.trafficUsed !== undefined || ui.trafficLimit !== undefined) {
                var used = (ui.trafficUsed !== undefined ? String(ui.trafficUsed) : "?");
                var limit = (ui.trafficLimit !== undefined ? String(ui.trafficLimit) : "∞");
                rows.push([I18n.t("Трафик"), used + " / " + limit]);
            }
            if (ui.expiresAt) rows.push([I18n.t("Истекает"), fmtTime(ui.expiresAt)]);
            else if (ui.expire) rows.push([I18n.t("Истекает"), fmtTime(ui.expire)]);
            if (ui.trafficLimitStrategy) rows.push([I18n.t("Сброс трафика"), String(ui.trafficLimitStrategy)]);
            if (ui.profileTitle) rows.push([I18n.t("Название профиля"), String(ui.profileTitle)]);
            if (ui.providerId) rows.push(["Provider ID", String(ui.providerId)]);
            if (ui.announcement) rows.push([I18n.t("Объявление"), String(ui.announcement)]);
            if (ui.profileUpdateInterval) rows.push([I18n.t("Интервал обновления"), String(ui.profileUpdateInterval)]);
            var extra = [["Описание", "profileDescription"], ["Баннер", "bannerText"], ["Кнопка баннера", "bannerButtonText"], ["Ссылка баннера", "bannerButtonUrl"], ["Цвет баннера", "bannerBgColor"], ["Premium", "premiumUrl"], ["Email", "supportEmail"], ["Поддержка", "supportUrl"], ["Сайт", "profileUrl"]];
            for (var e = 0; e < extra.length; ++e) {
                if (ui[extra[e][1]] !== undefined && ui[extra[e][1]] !== "") rows.push([I18n.t(extra[e][0]), String(ui[extra[e][1]])]);
            }
        }
        var features = sub.presentation ? sub.presentation.featureRows || [] : [];
        for (var p = 0; p < features.length; ++p) {
            rows.push([page.premiumFeatureLabel(features[p].key), String(features[p].value) + " · " + I18n.t(features[p].status)]);
        }
        return rows;
    }
    function premiumFeatureLabel(key) {
        var labels = {
            "new-url": I18n.t("Новый URL подписки"),
            "new-domain": I18n.t("Новый домен подписки"),
            "subscription-always-hwid-enable": I18n.t("Обязательный HWID"),
            "notification-subs-expire": I18n.t("Уведомлять об окончании"),
            "hide-settings": I18n.t("Скрыть настройки серверов"),
            "server-address-resolve-enable": I18n.t("Резолвинг адресов серверов"),
            "server-address-resolve-dns-domain": I18n.t("Домен DNS для резолвинга"),
            "server-address-resolve-dns-ip": I18n.t("IP DNS для резолвинга"),
            "subscription-autoconnect": I18n.t("Автоподключение"),
            "subscription-autoconnect-type": I18n.t("Режим автоподключения"),
            "subscription-ping-onopen-enabled": I18n.t("Автопинг при открытии"),
            "subscription-auto-update-enable": I18n.t("Автообновление подписок"),
            "fragmentation-enable": I18n.t("Фрагментация"),
            "fragmentation-packets": I18n.t("Пакеты фрагментации"),
            "fragmentation-length": I18n.t("Размер фрагментации"),
            "fragmentation-interval": I18n.t("Интервал фрагментации"),
            "ping-type": I18n.t("Тип пинга"),
            "check-url-via-proxy": I18n.t("URL проверки через прокси"),
            "change-user-agent": "User-Agent",
            "app-auto-start": I18n.t("Автозапуск приложения"),
            "subscription-auto-update-open-enable": I18n.t("Обновлять подписки при запуске"),
            "per-app-proxy-mode": I18n.t("Режим прокси приложений"),
            "per-app-proxy-list": I18n.t("Список приложений"),
            "sniffing-enable": "Sniffing",
            "subscriptions-collapse": I18n.t("Сворачивание подписки"),
            "ping-result": I18n.t("Отображение пинга"),
            "mux-enable": "Mux",
            "mux-tcp-connections": "Mux TCP",
            "mux-xudp-connections": "Mux XUDP",
            "mux-quic": "Mux QUIC",
            "exclude-routes": I18n.t("Исключённые маршруты")
        };
        return labels[key] || key;
    }
    function subscriptionMeta(sub) {
        if (!sub || !sub.userinfo || typeof sub.userinfo !== "object") return null;
        var ui = sub.userinfo;
        var premium = ui.premiumFeatures && typeof ui.premiumFeatures === "object"
            && Object.keys(ui.premiumFeatures).length > 0;
        if (!ui.profileTitle && !ui.supportUrl && !ui.profileUrl && !ui.telegramUrl
                && !ui.announcement && !ui.announcementUrl && !ui.providerId && !premium) return null;
        return ui;
    }
    function subscriptionActionUrl(key) {
        var meta = page.subscriptionMeta(page.selectedSub());
        if (!meta) return "";
        var wanted = String(meta[key] || "").trim();
        var links = page.providerView.links || [];
        for (var i = 0; i < links.length; ++i) {
            if (String(links[i].url) === wanted) return wanted;
        }
        return "";
    }
    function menuNodeId() { return App.nodeIdAt(page.menuRow); }

    function selectOnly(rowIndex) {
        var id = App.nodeIdAt(rowIndex);
        if (!id) return;
        var o = {}; o[id] = true;
        anchorRow = rowIndex;
        _commit(o, 1);
    }
    function toggle(rowIndex) {
        var id = App.nodeIdAt(rowIndex);
        if (!id) return;
        var o = Object.assign({}, sel);
        if (o[id]) delete o[id]; else o[id] = true;
        anchorRow = rowIndex;
        _commit(o, Object.keys(o).length);
    }
    function selectRange(a, b) {
        if (a < 0) a = b;
        var lo = Math.min(a, b), hi = Math.max(a, b);
        var o = {};
        for (var r = lo; r <= hi; r++) {
            var id = App.nodeIdAt(r);
            if (id) o[id] = true;
        }
        _commit(o, Object.keys(o).length);
    }
    function clearSel() { anchorRow = -1; _commit({}, 0); }
    function rowAtSelectionPoint(x, y) {
        if (!listSelectionArea || !list || list.count <= 0) return -1;
        var px = Math.max(0, Math.min(list.width - 1, x));
        var py = Math.max(0, Math.min(list.height - 1, y));
        var pt = listSelectionArea.mapToItem(list.contentItem, px, py);
        return list.indexAt(pt.x, pt.y);
    }
    function updateDragSelection() {
        if (!selectionDragging) return;
        var r = rowAtSelectionPoint(selectionPointerX, selectionPointerY);
        if (r >= 0) selectRange(anchorRow, r);
    }
    function scrollDragSelection(frameScale) {
        if (!selectionDragging || !listVbarTrack.scrollable) return;
        var k = frameScale > 0 ? Math.min(frameScale, 4) : 1;
        var edge = Math.max(28, rowH);
        var delta = 0;
        if (selectionPointerY < edge) {
            delta = -((edge - selectionPointerY) / edge) * rowH * k;
        } else if (selectionPointerY > list.height - edge) {
            delta = ((selectionPointerY - (list.height - edge)) / edge) * rowH * k;
        }
        if (Math.abs(delta) < 0.01) return;
        listScrollAnim.stop();
        list.contentY = Math.max(listVbarTrack.minY, Math.min(listVbarTrack.maxY, list.contentY + delta));
        listVbarTrack.flash();
        updateDragSelection();
    }
    function stopSelectionDrag() {
        selectionDragging = false;
        hoverRow = -1;
    }
    function selectAll() {
        var o = {};
        for (var i = 0; i < list.count; i++) {
            var id = App.nodeIdAt(i);
            if (id) o[id] = true;
        }
        anchorRow = 0;
        _commit(o, Object.keys(o).length);
    }

    function applyFilters() {
        App.setNodeFilter(page.filterGroup, page.filterText);
    }

    function revealImportedNode(nodeId) {
        if (!nodeId) return;
        page.filterText = "";
        searchInput.text = "";
        page.applyFilters();
        Qt.callLater(function() {
            var row = App.nodeIndexById(nodeId);
            if (row < 0) {
                page.filterGroup = "";
                groupCombo.currentIndex = 0;
                page.applyFilters();
                row = App.nodeIndexById(nodeId);
            }
            if (row < 0) return;
            page.selectOnly(row);
            list.positionViewAtIndex(row, ListView.Center);
        });
    }
    function selectGroupByName(groupName) {
        Qt.callLater(function() {
            var idx = page.groupModel.indexOf(groupName);
            if (idx <= 0) return;
            groupCombo.currentIndex = idx;
            page.filterGroup = groupName;
            page.applyFilters();
        });
    }
    function selectSubscriptionByGroup(groupName) {
        Qt.callLater(function() {
            var subs = App.subscriptions || [];
            for (var i = 0; i < subs.length; i++) {
                var group = String(subs[i].group || subs[i].name || "Default");
                if (group === groupName) {
                    subCombo.currentIndex = i + 1;
                    App.setSelectedSubscriptionId(String(subs[i].id || ""));
                    return;
                }
            }
            subCombo.currentIndex = 0;
            App.setSelectedSubscriptionId("");
        });
    }

    // ── source picker (subscriptions + groups) ─────
    function sourceIsAll() { return page.selectedSub() === null && page.filterGroup.length === 0; }
    function sourceTitle() {
        var sub = page.selectedSub();
        if (sub) return sub.name || I18n.t("Подписка");
        if (page.filterGroup.length) return page.filterGroup;
        return I18n.t("Все серверы");
    }
    function sourceSubtitleHtml() {
        if (page.selectedSub() !== null && page.subState.lineHtml.length) return page.subState.lineHtml;
        return I18n.t("Серверов: {count}", { count: list.count });
    }
    function sourceSubtitle() {
        var count = I18n.t("Серверов: {count}", { count: list.count });
        if (page.selectedSub() !== null && page.subState.line.length) return page.subState.line;
        return count;
    }
    function sourceEntries() {
        var rows = [{ kind: "all", title: I18n.t("Все серверы"), subtitle: "", active: page.sourceIsAll() }];
        var subs = App.subscriptions || [];
        var subGroups = {};
        if (subs.length) rows.push({ kind: "header", title: I18n.t("Подписки") });
        for (var i = 0; i < subs.length; i++) {
            var sub = subs[i];
            var st = page.computeSubState(sub);
            subGroups[String(sub.group || sub.name || "Default")] = true;
            rows.push({ kind: "sub", index: i, title: sub.name || I18n.t("Подписка"), subtitle: st.line,
                        subtitleHtml: st.lineHtml, barColor: st.trafficColor,
                        count: sub.node_count || 0, fraction: st.fraction, hasBar: st.limited,
                        warn: st.soon || st.fraction >= 0.9 || !st.active,
                        active: String(App.selectedSubscriptionId || "") === String(sub.id || "") });
        }
        var groups = page.groupModel.slice(1).filter(function(g) { return !subGroups[g]; });
        if (groups.length) rows.push({ kind: "header", title: I18n.t("Группы") });
        for (var j = 0; j < groups.length; j++) {
            rows.push({ kind: "group", name: groups[j], title: groups[j], subtitle: "",
                        active: page.selectedSub() === null && page.filterGroup === groups[j],
                        deletable: groups[j].toLowerCase() !== "default" });
        }
        return rows;
    }
    // Closes the picker first; the choice is applied after the popup's rows are
    // gone (the list is rebuilt by the choice), from the page's own scope.
    function chooseSource(kind, index, name) {
        sourcePopup.close();
        Qt.callLater(function() {
            if (kind === "all") page.chooseAll();
            else if (kind === "sub") page.chooseSubscription(index);
            else if (kind === "group") page.chooseGroup(name);
        });
    }
    function requestDeleteGroup(name) {
        // Closing the popup empties its Repeater and destroys this row, so
        // the dialog must be opened from page scope.
        sourcePopup.close();
        Qt.callLater(function() {
            deleteGroupDialog.groupName = name;
            deleteGroupDialog.open();
        });
    }
    function chooseAll() {
        App.setSelectedSubscriptionId("");
        subCombo.currentIndex = 0;
        page.filterGroup = "";
        groupCombo.currentIndex = 0;
        page.applyFilters();
    }
    function chooseSubscription(i) {
        var subs = App.subscriptions || [];
        var subscription = i >= 0 && i < subs.length ? subs[i] : null;
        subCombo.currentIndex = subscription ? i + 1 : 0;
        App.setSelectedSubscriptionId(subscription ? String(subscription.id || "") : "");
        page.filterGroup = subscription ? String(subscription.group || subscription.name || "Default") : "";
        groupCombo.currentIndex = Math.max(0, page.groupModel.indexOf(page.filterGroup));
        page.applyFilters();
    }
    function chooseGroup(name) {
        page.filterGroup = name || "";
        groupCombo.currentIndex = Math.max(0, page.groupModel.indexOf(page.filterGroup));
        page.applyFilters();
        page.selectSubscriptionByGroup(page.filterGroup);
    }
    // Provider description for the inspector, never a repeat of the subscription name.
    function inspectorDescription() {
        var sub = page.activeSub;
        var text = String(page.providerView.description || "").trim();
        if (!sub || !text.length) return "";
        return text.toLowerCase() === String(sub.name || "").trim().toLowerCase() ? "" : text;
    }

    function restoreSubscriptionSelection() {
        if (!App.profileLoaded) return;
        var wanted = String(App.selectedSubscriptionId || "");
        var subs = App.subscriptions || [];
        for (var i = 0; i < subs.length; i++) {
            if (String(subs[i].id || "") === wanted) {
                subCombo.currentIndex = i + 1;
                return;
            }
        }
        subCombo.currentIndex = 0;
        if (wanted.length)
            App.setSelectedSubscriptionId("");
    }

    function restoreServerViewState() {
        if (!App.profileLoaded) return;
        page.viewStateRestored = true;
        var restoredSub = page.selectedSub();
        if (restoredSub)
            page.filterGroup = String(restoredSub.group || restoredSub.name || "Default");
        var groupIndex = page.groupModel.indexOf(page.filterGroup);
        if (groupIndex < 0) {
            page.filterGroup = "";
            groupIndex = 0;
        }
        groupCombo.currentIndex = groupIndex;
        var sortIndex = page.sortKeys.indexOf(page.sortKey);
        if (sortIndex < 0) {
            page.sortKey = "manual";
            sortIndex = 0;
        }
        sortCombo.currentIndex = sortIndex;
        page.applyFilters();
        App.setNodeSort(page.sortKey, page.sortAsc);
        page.restoreSubscriptionSelection();
    }

    Component.onCompleted: {
        syncProviderBanners();
        Qt.callLater(page.restoreServerViewState);
    }

    function applySort(key) {
        if (!key) return;
        if (page.sortKey === key) {
            if (page.sortAsc) {
                page.sortAsc = false;          // asc -> desc
            } else {
                page.sortKey = "manual";       // desc -> off (manual order)
                page.sortAsc = true;
            }
        } else {
            page.sortKey = key;
            page.sortAsc = true;
        }
        var idx = page.sortKeys.indexOf(page.sortKey);
        if (idx >= 0)
            sortCombo.currentIndex = idx;
        App.setNodeSort(page.sortKey, page.sortAsc);
    }

    // ── reusable cell / header text ───────────────────
    component CellText: Text {
        textFormat: Text.PlainText
        property int w: 80
        width: w
        height: parent ? parent.height : 0
        leftPadding: 4
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        color: Theme.textMuted
        font.family: Theme.fontFamily
        font.pixelSize: page.cellFont
    }
    component HeaderLabel: Text {
        id: hdr
        property int w: 80
        property int columnIndex: -1
        property string sortKey: ""
        readonly property bool sortable: sortKey !== ""
        readonly property bool activeSort: sortable && page.sortKey === sortKey
        width: w
        height: parent ? parent.height : 0
        leftPadding: 4
        rightPadding: activeSort ? 16 : 0
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        color: activeSort ? Theme.text : Theme.textFaint
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.bold: true

        // Sort-direction chevron, shown only on the active sort column.
        Text {
            visible: hdr.activeSort
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            text: page.sortAsc ? "\uE74A" : "\uE74B"
            font.family: "Segoe Fluent Icons"
            font.pixelSize: 10
            color: Theme.textMuted
        }

        // Click a sortable header to sort by it; click again to flip direction.
        MouseArea {
            anchors.fill: parent
            anchors.rightMargin: 5
            enabled: hdr.sortable
            cursorShape: Qt.PointingHandCursor
            onClicked: page.applySort(hdr.sortKey)
        }

        MouseArea {
            id: columnResizeHandle
            z: 2
            width: 10
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.right: parent.right
            acceptedButtons: Qt.LeftButton
            cursorShape: Qt.SplitHCursor
            property real pressedX: 0
            property real pressedWidth: 0
            onPressed: (mouse) => {
                var point = mapToItem(page, mouse.x, mouse.y);
                pressedX = point.x;
                pressedWidth = hdr.width;
                mouse.accepted = true;
            }
            onPositionChanged: (mouse) => {
                if (!pressed)
                    return;
                var point = mapToItem(page, mouse.x, mouse.y);
                page.resizeColumn(hdr.columnIndex, pressedWidth + point.x - pressedX);
            }
        }
    }
    component FilterCombo: FluentCombo {
        Layout.preferredHeight: Theme.controlHeight
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.spacing
        spacing: Theme.spacing

        // ── top bar: title, source picker, global actions ─────
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.controlHeight
            spacing: 10

            Text {
                textFormat: Text.PlainText
                text: I18n.t("Серверы")
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontTitle
                font.bold: true
                Layout.rightMargin: 4
            }

            // Logic holders: the picker drives these so filters, persistence and
            // restore keep working exactly as before.
            FilterCombo {
                id: subCombo
                visible: false
                model: [I18n.t("Нет подписки")].concat(
                    App.subscriptions.map(function(s) { return (s.name && s.name.length ? s.name : I18n.t("Подписка")) + " (" + (s.node_count || 0) + ")"; })
                )
                onActivated: page.chooseSubscription(currentIndex - 1)
            }
            FilterCombo {
                id: groupCombo
                visible: false
                model: page.groupModel
                onActivated: page.chooseGroup(currentIndex <= 0 ? "" : currentText)
            }

            Text {
                textFormat: Text.PlainText
                visible: page.selCount > 0
                text: I18n.t("Выбрано: {count}", { count: page.selCount })
                color: Theme.textMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                Layout.leftMargin: 4
            }
            Item { Layout.fillWidth: true }
            // Source picker: one control for subscriptions and groups.
            Rectangle {
                id: sourceButton
                objectName: "sourcePicker"
                Layout.preferredWidth: Math.min(340, Math.max(240, page.width * 0.24))
                Layout.preferredHeight: Theme.controlHeight
                radius: Theme.radius
                color: sourceHover.hovered || sourcePopup.opened ? Theme.controlFillHover : Theme.controlFill
                border.width: 1
                border.color: sourcePopup.opened ? Theme.accent : Theme.borderSolid
                RowLayout {
                    id: sourceRow
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 10
                    spacing: 10
                    Text {
                        text: page.sourceTitle()
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontNormal
                        font.weight: Font.DemiBold
                        Layout.maximumWidth: sourceButton.width * 0.45
                    }
                    Text {
                        Layout.fillWidth: true
                        text: page.sourceSubtitleHtml()
                        textFormat: Text.StyledText
                        elide: Text.ElideRight
                        color: Theme.textMuted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                    }
                    Text {
                        text: ""
                        color: Theme.textMuted
                        font.family: "Segoe Fluent Icons"
                        font.pixelSize: 11
                    }
                }
                HoverHandler { id: sourceHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: sourcePopup.opened ? sourcePopup.close() : sourcePopup.open() }

                Popup {
                    id: sourcePopup
                    objectName: "sourcePopup"
                    y: sourceButton.height + 6
                    x: sourceButton.width - width
                    width: 360
                    padding: 6
                    focus: true
                    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
                    background: Rectangle {
                        color: Theme.flyout
                        radius: 8
                        border.width: 1
                        border.color: Theme.flyoutBorder
                    }
                    contentItem: ColumnLayout {
                        spacing: 4
                        FluentScroll {
                            Layout.fillWidth: true
                            Layout.preferredHeight: Math.min(440, sourceList.implicitHeight)
                            roundedClip: false
                            ColumnLayout {
                                id: sourceList
                                width: parent.width
                                spacing: 2
                                Repeater {
                                    model: sourcePopup.opened ? page.sourceEntries() : []
                                    delegate: Item {
                                        id: srcRow
                                        required property var modelData
                                        readonly property bool header: modelData.kind === "header"
                                        Layout.fillWidth: true
                                        implicitHeight: header ? 28 : (modelData.kind === "sub" && modelData.subtitle.length ? 54 : 40)
                                        Text {
                                            visible: srcRow.header
                                            x: 10
                                            anchors.bottom: parent.bottom
                                            anchors.bottomMargin: 4
                                            text: srcRow.modelData.title
                                            color: Theme.textFaint
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                        }
                                        Rectangle {
                                            visible: !srcRow.header
                                            anchors.fill: parent
                                            radius: 6
                                            color: srcRow.modelData.active ? Theme.accentSoft : (srcHover.hovered ? Theme.cardHover : "transparent")
                                            Rectangle {
                                                visible: srcRow.modelData.active === true
                                                x: 0
                                                anchors.verticalCenter: parent.verticalCenter
                                                width: 3
                                                height: 16
                                                radius: 1.5
                                                color: Theme.accent
                                            }
                                            RowLayout {
                                                anchors.fill: parent
                                                anchors.leftMargin: 10
                                                anchors.rightMargin: 8
                                                spacing: 10
                                                ColumnLayout {
                                                    Layout.fillWidth: true
                                                    spacing: 3
                                                    Text {
                                                        Layout.fillWidth: true
                                                        text: srcRow.modelData.title
                                                        textFormat: Text.PlainText
                                                        elide: Text.ElideRight
                                                        color: Theme.text
                                                        font.family: Theme.fontFamily
                                                        font.pixelSize: Theme.fontNormal
                                                    }
                                                    Rectangle {
                                                        visible: srcRow.modelData.hasBar === true
                                                        Layout.fillWidth: true
                                                        Layout.preferredHeight: 4
                                                        radius: 2
                                                        color: Theme.dark ? Qt.rgba(0, 0, 0, 0.28) : Qt.rgba(0, 0, 0, 0.08)
                                                        Rectangle {
                                                            height: parent.height
                                                            radius: 2
                                                            width: parent.width * Math.min(1, Number(srcRow.modelData.fraction || 0))
                                                            color: srcRow.modelData.barColor || Theme.accent
                                                        }
                                                    }
                                                    Text {
                                                        visible: String(srcRow.modelData.subtitle || "").length > 0
                                                        Layout.fillWidth: true
                                                        text: String(srcRow.modelData.subtitleHtml || srcRow.modelData.subtitle || "")
                                                        textFormat: Text.StyledText
                                                        elide: Text.ElideRight
                                                        color: Theme.textMuted
                                                        font.family: Theme.fontFamily
                                                        font.pixelSize: Theme.fontSmall
                                                    }
                                                }
                                                Text {
                                                    visible: srcRow.modelData.count !== undefined
                                                    text: String(srcRow.modelData.count)
                                                    color: Theme.textFaint
                                                    font.family: Theme.fontFamily
                                                    font.pixelSize: Theme.fontSmall
                                                }
                                                AccentButton {
                                                    visible: srcRow.modelData.deletable === true && srcHover.hovered
                                                    kind: "ghost"
                                                    iconOnly: true
                                                    glyph: ""
                                                    text: I18n.t("Удалить выбранную группу")
                                                    Layout.preferredWidth: 28
                                                    Layout.preferredHeight: 28
                                                    onClicked: page.requestDeleteGroup(srcRow.modelData.name)
                                                }
                                            }
                                            HoverHandler { id: srcHover; cursorShape: Qt.PointingHandCursor }
                                            TapHandler {
                                                onTapped: {
                                                    var kind = srcRow.modelData.kind;
                                                    var index = srcRow.modelData.index;
                                                    var name = srcRow.modelData.name;
                                                    page.chooseSource(kind, index, name);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Rectangle { Layout.fillWidth: true; Layout.topMargin: 2; height: 1; color: Theme.divider }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 4
                            AccentButton { kind: "ghost"; glyph: ""; text: I18n.t("Импорт подписки"); onClicked: { sourcePopup.close(); subDialog.openNew() } }
                            AccentButton { kind: "ghost"; glyph: ""; text: I18n.t("Новая группа"); onClicked: { sourcePopup.close(); groupDialog.openNew() } }
                        }
                    }
                }
            }

            AccentButton {
                id: addButton
                kind: "accent"
                glyph: ""
                text: I18n.t("Добавить")
                onClicked: addMenu.popup(addButton, 0, addButton.height + 4)
            }
        }

        // ── tools: search, sort, list-wide tests ─────
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.controlHeight
            spacing: 8

            Rectangle {
                id: searchBox
                Layout.fillWidth: true
                Layout.minimumWidth: 160
                Layout.preferredHeight: Theme.controlHeight
                radius: Theme.radiusSmall
                color: Theme.controlFill
                border.width: 1
                border.color: searchInput.activeFocus ? Theme.accent : Theme.borderSolid

                Text {
                    textFormat: Text.PlainText
                    id: searchGlyph
                    anchors.left: parent.left
                    anchors.leftMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    text: ""
                    font.family: "Segoe Fluent Icons"
                    font.pixelSize: 14
                    color: Theme.textFaint
                }
                FluentTextField {
                    id: searchInput
                    anchors.left: searchGlyph.right
                    anchors.leftMargin: 8
                    anchors.right: parent.right
                    anchors.rightMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    background: null
                    verticalAlignment: TextInput.AlignVCenter
                    placeholderText: I18n.t("Поиск серверов…")
                    placeholderTextColor: Theme.textFaint
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: page.cellFont
                    onTextChanged: { page.filterText = text; searchFilterDebounce.restart() }
                    onEditingFinished: { searchFilterDebounce.stop(); page.applyFilters() }
                }
            }
            FilterCombo {
                id: sortCombo
                Layout.preferredWidth: 190
                model: page.sortLabels
                currentIndex: 0
                onActivated: {
                    page.sortKey = page.sortKeys[currentIndex];
                    App.setNodeSort(page.sortKey, page.sortAsc);
                }
            }
            AccentButton {
                kind: "ghost"
                iconOnly: true
                glyph: page.sortAsc ? "" : ""
                tip: page.sortAsc ? I18n.t("По возрастанию") : I18n.t("По убыванию")
                onClicked: {
                    page.sortAsc = !page.sortAsc;
                    App.setNodeSort(page.sortKey, page.sortAsc);
                }
            }
            AccentButton {
                kind: "ghost"
                glyph: ""
                text: I18n.t("Пинг")
                tip: page.selCount > 0 ? I18n.t("Пинг выбранных") : I18n.t("Пинг всех в списке")
                onClicked: App.pingNodes(page.selCount > 0 ? page.selectedIds() : page.visibleIds())
            }
            AccentButton {
                id: moreButton
                kind: "ghost"
                iconOnly: true
                glyph: ""
                text: I18n.t("Ещё")
                onClicked: moreMenu.popup(moreButton, 0, moreButton.height + 4)
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Theme.spacing

            // ── table ────────────────────────────
            Card {
                objectName: "serverTableCard"
                Layout.fillWidth: true
                Layout.fillHeight: true

                Flickable {
                    id: hflick
                    anchors.fill: parent
                    anchors.rightMargin: 14
                    contentWidth: page.tableWidth
                    contentHeight: height
                    flickableDirection: Flickable.HorizontalFlick
                    interactive: false
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.horizontal: FluentScrollBar {
                        id: hbar
                        parent: hflick
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        interactive: true
                        policy: page.tableWidth > hflick.width ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
                    }

                    Column {
                        width: page.tableWidth
                        height: hflick.height

                        // header
                        Rectangle {
                            id: headerRow
                            width: page.tableWidth
                            height: 30
                            color: Theme.micaBase
                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: page.leftPad
                                HeaderLabel { columnIndex: 0; w: page.colName; text: I18n.t("Сервер"); sortKey: "name" }
                                HeaderLabel { columnIndex: 1; w: page.colType; text: I18n.t("Тип"); sortKey: "scheme" }
                                HeaderLabel { columnIndex: 2; w: page.colTransport; text: I18n.t("Транспорт"); sortKey: "transport" }
                                HeaderLabel { columnIndex: 3; w: page.colAddr; text: I18n.t("Адрес"); sortKey: "address" }
                                HeaderLabel { columnIndex: 4; w: page.colPort; text: I18n.t("Порт") }
                                HeaderLabel { columnIndex: 5; w: page.colGroup; text: I18n.t("Группа"); sortKey: "group" }
                                HeaderLabel { columnIndex: 6; w: page.colPing; text: I18n.t("Пинг"); sortKey: "ping" }
                                HeaderLabel { columnIndex: 7; w: page.colSpeed; text: I18n.t("Скорость"); sortKey: "speed" }
                                HeaderLabel { columnIndex: 8; w: page.colStatus; text: I18n.t("Статус"); sortKey: "status" }
                                HeaderLabel { columnIndex: 9; w: page.colLast; text: I18n.t("Последнее"); sortKey: "last" }
                            }
                            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.divider }
                        }

                        ListView {
                            id: list
                            width: page.tableWidth
                            height: Math.max(0, parent.height - headerRow.height - (hbar.visible ? 12 : 0))
                            objectName: "serverList"
                            reuseItems: true
                            model: App.nodeModel
                            onCountChanged: {
                                if (page.selCount === 0)
                                    return;
                                var o = {};
                                var n = 0;
                                for (var i = 0; i < count; i++) {
                                    var id = App.nodeIdAt(i);
                                    if (id && page.sel[id] === true) { o[id] = true; n++; }
                                }
                                if (n !== page.selCount) {
                                    page.anchorRow = -1;
                                    page._commit(o, n);
                                }
                            }
                            clip: true
                            interactive: false  // wheel/scrollbar only; no drag-to-scroll list movement
                            boundsBehavior: Flickable.StopAtBounds
                            onContentYChanged: page.updateDragSelection()

                            Connections {
                                target: App.nodeModel
                                function onModelAboutToBeReset() {
                                    listScrollAnim.stop();
                                    page.savedListContentY = list.contentY - list.originY;
                                }
                                function onModelReset() {
                                    Qt.callLater(function() {
                                        list.forceLayout();
                                        var origin = list.originY;
                                        var range = Math.max(0, list.contentHeight - list.height);
                                        list.contentY = origin + Math.max(0, Math.min(range, page.savedListContentY));
                                    });
                                }
                            }

                            NumberAnimation {
                                id: listScrollAnim
                                target: list
                                property: "contentY"
                                duration: 420
                                easing.type: Easing.OutQuint
                            }
                            WheelHandler {
                                acceptedDevices: PointerDevice.Mouse
                                onWheel: (ev) => {
                                    var minY = list.originY
                                    var maxY = minY + Math.max(0, list.contentHeight - list.height)
                                    if (maxY <= minY) { ev.accepted = true; return }
                                    var step = Math.max(60, page.rowH * 3)
                                    var base = listScrollAnim.running ? listScrollAnim.to : list.contentY
                                    var target = Math.max(minY, Math.min(maxY, base - (ev.angleDelta.y / 120) * step))
                                    listScrollAnim.to = target
                                    listScrollAnim.restart()
                                    listVbarTrack.flash()
                                    ev.accepted = true
                                }
                            }

                            delegate: Rectangle {
                                id: nodeRow
                                objectName: "serverRow_" + nodeId
                                required property int index
                                required property string nodeId
                                required property string name
                                required property string scheme
                                required property string description
                                required property string transport
                                required property string server
                                required property int port
                                required property string group
                                required property int ping
                                required property bool pinging
                                required property real speed
                                required property bool isAlive
                                required property string pingKind
                                required property bool tested
                                required property bool selected
                                required property bool runtimeSupported
                                required property int speedProgress
                                required property string flagSource
                                required property string flagEmoji
                                required property string flagOrient
                                required property var flagColors
                                required property string lastUsed

                                readonly property bool picked: (page.selRev >= 0) && (page.sel[nodeId] === true)

                                width: page.tableWidth
                                height: page.rowH
                                opacity: runtimeSupported ? 1.0 : 0.45
                                color: picked ? Theme.accentSoft : (page.hoverRow === nodeRow.index && runtimeSupported ? Theme.cardHover : "transparent")

                                // active-node accent bar
                                Rectangle {
                                    visible: nodeRow.selected
                                    width: 3
                                    height: parent.height
                                    color: Theme.accent
                                }
                                // bottom divider
                                Rectangle {
                                    anchors.bottom: parent.bottom
                                    width: parent.width
                                    height: 1
                                    color: Theme.divider
                                    opacity: 0.5
                                }

                                Row {
                                    anchors.fill: parent
                                    anchors.leftMargin: page.leftPad

                                    // name + flag
                                    Item {
                                        width: page.colName
                                        height: parent.height
                                        Row {
                                            anchors.left: parent.left
                                            anchors.leftMargin: 4
                                            anchors.verticalCenter: parent.verticalCenter
                                            height: parent.height
                                            spacing: 8
                                            Item {
                                                id: flagBox
                                                readonly property bool imageReady: !!nodeRow.flagSource && flagImg.status === Image.Ready
                                                readonly property bool hasShapeFallback: nodeRow.flagColors && nodeRow.flagColors.length > 0
                                                    && !!nodeRow.flagOrient
                                                readonly property bool showEmojiFallback: !hasShapeFallback && !!nodeRow.flagEmoji
                                                width: (nodeRow.flagSource || nodeRow.flagEmoji || (nodeRow.flagColors && nodeRow.flagColors.length > 0)) ? 24 : 0
                                                height: 18
                                                anchors.verticalCenter: parent.verticalCenter
                                                visible: width > 0
                                                clip: true
                                                Image {
                                                    id: flagImg
                                                    objectName: "serverFlagImage"
                                                    anchors.fill: parent
                                                    z: 1
                                                    visible: flagBox.imageReady
                                                    source: nodeRow.flagSource
                                                    fillMode: Image.PreserveAspectFit
                                                    sourceSize.width: Math.round(flagBox.width * Screen.devicePixelRatio)
                                                    sourceSize.height: Math.round(flagBox.height * Screen.devicePixelRatio)
                                                    smooth: true
                                                    mipmap: true
                                                    asynchronous: true
                                                    cache: true
                                                }
                                                Text {
                                                    anchors.centerIn: parent
                                                    visible: flagBox.showEmojiFallback
                                                    text: nodeRow.flagEmoji
                                                    font.pixelSize: 18
                                                    font.family: "Segoe UI Emoji"
                                                    renderType: Text.NativeRendering
                                                }
                                                Column {
                                                    anchors.fill: parent
                                                    visible: flagBox.hasShapeFallback && nodeRow.flagOrient === "h"
                                                    Repeater {
                                                        model: (flagBox.hasShapeFallback && nodeRow.flagOrient === "h") ? nodeRow.flagColors : []
                                                        delegate: Rectangle {
                                                            required property string modelData
                                                            width: flagBox.width
                                                            height: flagBox.height / Math.max(1, nodeRow.flagColors.length)
                                                            color: modelData
                                                        }
                                                    }
                                                }
                                                Row {
                                                    anchors.fill: parent
                                                    visible: flagBox.hasShapeFallback && nodeRow.flagOrient === "v"
                                                    Repeater {
                                                        model: (flagBox.hasShapeFallback && nodeRow.flagOrient === "v") ? nodeRow.flagColors : []
                                                        delegate: Rectangle {
                                                            required property string modelData
                                                            width: flagBox.width / Math.max(1, nodeRow.flagColors.length)
                                                            height: flagBox.height
                                                            color: modelData
                                                        }
                                                    }
                                                }
                                                Item {
                                                    anchors.fill: parent
                                                    visible: flagBox.hasShapeFallback && nodeRow.flagOrient === "nordic"
                                                    Rectangle { anchors.fill: parent; color: (nodeRow.flagColors && nodeRow.flagColors.length > 0) ? nodeRow.flagColors[0] : "transparent" }
                                                    Rectangle {
                                                        x: 0; y: Math.round(flagBox.height * 0.36)
                                                        width: flagBox.width; height: Math.max(2, Math.round(flagBox.height * 0.28))
                                                        color: (nodeRow.flagColors && nodeRow.flagColors.length > 1) ? nodeRow.flagColors[1] : "transparent"
                                                    }
                                                    Rectangle {
                                                        x: Math.round(flagBox.width * 0.30); y: 0
                                                        width: Math.max(3, Math.round(flagBox.width * 0.18)); height: flagBox.height
                                                        color: (nodeRow.flagColors && nodeRow.flagColors.length > 1) ? nodeRow.flagColors[1] : "transparent"
                                                    }
                                                    Rectangle {
                                                        visible: nodeRow.flagColors && nodeRow.flagColors.length > 2
                                                        x: 0; y: Math.round(flagBox.height * 0.43)
                                                        width: flagBox.width; height: Math.max(1, Math.round(flagBox.height * 0.14))
                                                        color: (nodeRow.flagColors && nodeRow.flagColors.length > 2) ? nodeRow.flagColors[2] : "transparent"
                                                    }
                                                    Rectangle {
                                                        visible: nodeRow.flagColors && nodeRow.flagColors.length > 2
                                                        x: Math.round(flagBox.width * 0.36); y: 0
                                                        width: Math.max(1, Math.round(flagBox.width * 0.09)); height: flagBox.height
                                                        color: (nodeRow.flagColors && nodeRow.flagColors.length > 2) ? nodeRow.flagColors[2] : "transparent"
                                                    }
                                                }
                                                Item {
                                                    anchors.fill: parent
                                                    visible: flagBox.hasShapeFallback && nodeRow.flagOrient === "cross"
                                                    Rectangle { anchors.fill: parent; color: (nodeRow.flagColors && nodeRow.flagColors.length > 0) ? nodeRow.flagColors[0] : "transparent" }
                                                    Rectangle {
                                                        anchors.verticalCenter: parent.verticalCenter
                                                        width: flagBox.width; height: Math.max(3, Math.round(flagBox.height * 0.30))
                                                        color: (nodeRow.flagColors && nodeRow.flagColors.length > 1) ? nodeRow.flagColors[1] : "transparent"
                                                    }
                                                    Rectangle {
                                                        anchors.horizontalCenter: parent.horizontalCenter
                                                        width: Math.max(3, Math.round(flagBox.width * 0.20)); height: flagBox.height
                                                        color: (nodeRow.flagColors && nodeRow.flagColors.length > 1) ? nodeRow.flagColors[1] : "transparent"
                                                    }
                                                }
                                                Rectangle {
                                                    anchors.fill: parent
                                                    visible: flagBox.width > 0
                                                    color: "transparent"
                                                    radius: 3
                                                    antialiasing: true
                                                    border.width: 1
                                                    border.color: Qt.rgba(0, 0, 0, 0.2)
                                                }
                                            }
                                            Column {
                                                width: page.colName - 24 - flagBox.width
                                                anchors.verticalCenter: parent.verticalCenter
                                                spacing: 0
                                                Text {
                                                    width: parent.width
                                                    text: nodeRow.name
                                                    textFormat: Text.PlainText
                                                    color: Theme.text
                                                    font.family: Theme.fontFamily
                                                    font.pixelSize: page.cellFont
                                                    elide: Text.ElideRight
                                                }
                                                Text {
                                                    visible: nodeRow.description.length > 0
                                                    width: parent.width
                                                    text: nodeRow.description
                                                    textFormat: Text.PlainText
                                                    color: Theme.textMuted
                                                    font.family: Theme.fontFamily
                                                    font.pixelSize: Math.max(9, page.cellFont - 3)
                                                    elide: Text.ElideRight
                                                }
                                            }
                                        }
                                    }

                                    CellText { w: page.colType; text: nodeRow.scheme }
                                    CellText { w: page.colTransport; text: nodeRow.transport }
                                    CellText { w: page.colAddr; text: nodeRow.server; color: Theme.text }
                                    CellText { w: page.colPort; text: nodeRow.port > 0 ? ("" + nodeRow.port) : "—" }
                                    CellText { w: page.colGroup; text: nodeRow.group || "—" }

                                    // ping
                                    Item {
                                        width: page.colPing
                                        height: parent.height
                                        Text {
                                            visible: !nodeRow.pinging
                                            anchors.verticalCenter: parent.verticalCenter
                                            leftPadding: 4
                                            text: nodeRow.ping < 0 ? "—" : (nodeRow.ping + " ms")
                                            color: nodeRow.ping < 0 ? Theme.textFaint : Theme.pingColor(nodeRow.ping)
                                            font.family: Theme.fontFamily
                                            font.pixelSize: page.cellFont
                                        }
                                        Text {
                                            id: pingSpinner
                                            visible: nodeRow.pinging
                                            anchors.left: parent.left
                                            anchors.leftMargin: 5
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: "\uE895"
                                            color: Theme.accent
                                            font.family: "Segoe Fluent Icons"
                                            font.pixelSize: 15
                                            transformOrigin: Item.Center
                                            RotationAnimator on rotation {
                                                from: 0
                                                to: 360
                                                duration: 850
                                                loops: Animation.Infinite
                                                running: nodeRow.pinging
                                            }
                                        }
                                    }
                                    // speed
                                    Item {
                                        width: page.colSpeed
                                        height: parent.height
                                        Text {
                                            anchors.verticalCenter: parent.verticalCenter
                                            leftPadding: 4
                                            text: nodeRow.speedProgress >= 0
                                                ? (nodeRow.speedProgress + "%")
                                                : (nodeRow.speed < 0 ? "—" : (nodeRow.speed.toFixed(1) + " MB/s"))
                                            color: nodeRow.speedProgress >= 0
                                                ? Theme.accent
                                                : (nodeRow.speed < 0 ? Theme.textFaint : Theme.text)
                                            font.family: Theme.fontFamily
                                            font.pixelSize: page.cellFont
                                        }
                                    }
                                    // status
                                    Item {
                                        width: page.colStatus
                                        height: parent.height
                                        Text {
                                            anchors.verticalCenter: parent.verticalCenter
                                            leftPadding: 4
                                            text: nodeRow.isAlive ? "OK" : (nodeRow.tested ? "✕" : "—")
                                            color: nodeRow.isAlive ? Theme.success : (nodeRow.tested ? Theme.danger : Theme.textFaint)
                                            font.family: Theme.fontFamily
                                            font.pixelSize: page.cellFont
                                            font.bold: true
                                        }
                                    }
                                    CellText { w: page.colLast; text: nodeRow.lastUsed }
                                }
                            }

                            MouseArea {
                                id: listSelectionArea
                                objectName: "serverListSelectionArea"
                                property var hoveredNode: page.hoverRow >= 0 ? App.nodeRowAt(page.hoverRow) : null
                                FluentToolTip {
                                    objectName: "serverPingToolTip"
                                    parent: listSelectionArea
                                    width: Math.min(300, listSelectionArea.width - 16)
                                    x: Math.max(8, Math.min(listSelectionArea.mouseX + 12, listSelectionArea.width - width - 8))
                                    y: listSelectionArea.mouseY + 20
                                    visible: listSelectionArea.containsMouse && !page.selectionDragging
                                        && listSelectionArea.mouseX >= page.leftPad + page.colName + page.colType + page.colTransport + page.colAddr + page.colPort + page.colGroup
                                        && listSelectionArea.mouseX < page.leftPad + page.colName + page.colType + page.colTransport + page.colAddr + page.colPort + page.colGroup + page.colPing
                                        && listSelectionArea.hoveredNode
                                        && listSelectionArea.hoveredNode.pingKind !== ""
                                        && listSelectionArea.hoveredNode.pingKind !== "proxy"
                                    delay: 650
                                    text: listSelectionArea.hoveredNode && listSelectionArea.hoveredNode.pingKind === "unavailable"
                                        ? I18n.t("Проверка не выполнена: проверьте ядро и прямой маршрут")
                                        : ((listSelectionArea.hoveredNode && listSelectionArea.hoveredNode.pingKind === "icmp_endpoint" ? "ICMP. " : "TCP. ") + I18n.t("Проверена доступность адреса, а не профиль VPN"))
                                }
                                anchors.fill: parent
                                z: 100
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                hoverEnabled: true
                                preventStealing: true
                                propagateComposedEvents: false

                                onPositionChanged: (mouse) => {
                                    page.selectionPointerX = mouse.x;
                                    page.selectionPointerY = mouse.y;
                                    page.hoverRow = page.rowAtSelectionPoint(mouse.x, mouse.y);
                                    page.updateDragSelection();
                                }
                                onExited: {
                                    if (!page.selectionDragging)
                                        page.hoverRow = -1;
                                }
                                onPressed: (mouse) => {
                                    list.forceActiveFocus();
                                    page.selectionPointerX = mouse.x;
                                    page.selectionPointerY = mouse.y;
                                    var row = page.rowAtSelectionPoint(mouse.x, mouse.y);
                                    if (row < 0)
                                        return;
                                    page.hoverRow = row;
                                    if (mouse.button === Qt.RightButton) {
                                        if (!page.isSelected(App.nodeIdAt(row)))
                                            page.selectOnly(row);
                                        page.menuRow = row;
                                        ctxMenu.popup();
                                        return;
                                    }
                                    if (mouse.modifiers & Qt.ControlModifier) {
                                        page.toggle(row);
                                        return;
                                    }
                                    if (mouse.modifiers & Qt.ShiftModifier) {
                                        page.selectRange(page.anchorRow < 0 ? row : page.anchorRow, row);
                                        return;
                                    }
                                    page.selectOnly(row);
                                    page.selectionDragging = true;
                                    selectionDragScroll.restart();
                                }
                                onDoubleClicked: (mouse) => {
                                    var row = page.rowAtSelectionPoint(mouse.x, mouse.y);
                                    var id = App.nodeIdAt(row);
                                    if (id)
                                        App.selectNode(id);
                                }
                                onReleased: page.stopSelectionDrag()
                                onCanceled: page.stopSelectionDrag()
                            }

                            // Ticks once per rendered frame (the monitor's refresh
                            // rate); the step is scaled to keep the old 30 ms speed.
                            FrameAnimation {
                                id: selectionDragScroll
                                running: page.selectionDragging
                                onTriggered: page.scrollDragSelection(frameTime / 0.03)
                            }
                        }

                        Item {
                            id: listVbarTrack
                            parent: hflick.parent
                            z: 50
                            width: 12
                            x: hflick.x + hflick.width + 1
                            y: headerRow.height
                            height: Math.max(0, hflick.height - headerRow.height - (hbar.visible ? 10 : 0))
                            property bool forcedVisible: false
                            readonly property real minY: list.originY
                            readonly property real rangeY: Math.max(0, list.contentHeight - list.height)
                            readonly property real maxY: minY + rangeY
                            readonly property bool scrollable: rangeY > 1
                            readonly property bool shown: scrollable

                            function flash() {
                                if (!scrollable)
                                    return
                                forcedVisible = true
                                listBarHide.restart()
                            }

                            Timer {
                                id: listBarHide
                                interval: 720
                                repeat: false
                                onTriggered: listVbarTrack.forcedVisible = false
                            }

                            visible: scrollable
                            opacity: shown ? 1 : 0
                            Behavior on opacity { NumberAnimation { duration: Theme.animations ? 160 : 0; easing.type: Theme.easeStandard } }
                            HoverHandler { id: listBarHover }

                            Rectangle {
                                id: listThumb
                                readonly property real ratio: list.contentHeight <= 0 ? 1 : Math.min(1, list.height / list.contentHeight)
                                width: listThumbMouse.drag.active || listBarHover.hovered ? 8 : 4
                                height: Math.max(36, listVbarTrack.height * ratio)
                                x: Math.round((listVbarTrack.width - width) / 2)
                                y: listVbarTrack.rangeY <= 0 ? 0 : (listVbarTrack.height - height) * Math.max(0, Math.min(1, (list.contentY - listVbarTrack.minY) / listVbarTrack.rangeY))
                                radius: width / 2
                                color: listThumbMouse.drag.active ? Theme.textMuted : Theme.textFaint
                                opacity: listThumbMouse.drag.active ? 0.9 : (listBarHover.hovered ? 0.68 : 0.38)
                                Behavior on width { NumberAnimation { duration: Theme.animations ? 120 : 0; easing.type: Easing.OutCubic } }

                                MouseArea {
                                    id: listThumbMouse
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    drag.target: parent
                                    drag.axis: Drag.YAxis
                                    drag.minimumY: 0
                                    drag.maximumY: Math.max(0, listVbarTrack.height - listThumb.height)
                                    preventStealing: true
                                    onPressed: {
                                        listScrollAnim.stop()
                                        listVbarTrack.flash()
                                    }
                                    onPositionChanged: {
                                        if (!drag.active)
                                            return
                                        var denom = Math.max(1, listVbarTrack.height - listThumb.height)
                                        list.contentY = listVbarTrack.minY + Math.max(0, Math.min(listVbarTrack.rangeY, (listThumb.y / denom) * listVbarTrack.rangeY))
                                        listVbarTrack.flash()
                                    }
                                    onReleased: listVbarTrack.flash()
                                    onCanceled: listVbarTrack.flash()
                                }
                            }
                        }
                    }
                }

                // A single empty/loading state; no overlapping placeholder texts.
                ColumnLayout {
                    anchors.centerIn: parent
                    width: Math.max(0, Math.min(parent.width - 24, 390))
                    visible: list.count === 0
                    spacing: 8
                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: "\uEC05"
                        font.family: "Segoe Fluent Icons"
                        font.pixelSize: 28
                        color: Theme.textFaint
                    }
                    Text {
                        Layout.fillWidth: true
                        text: !App.profileLoaded ? I18n.t("Загрузка профиля и серверов…")
                            : (page.filterGroup.length || page.filterText.length
                               ? I18n.t("Нет серверов по выбранным фильтрам")
                               : I18n.t("Добавьте подписку или импортируйте конфигурации"))
                        textFormat: Text.PlainText
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                        color: Theme.textMuted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontNormal
                    }
                }

            }

            // ── inspector: subscription details / selected server ─────
            Card {
                id: inspector
                objectName: "serverInspector"
                Layout.preferredWidth: 300
                Layout.fillHeight: true
                visible: page.width >= 1100
                padding: 0
                hoverable: false
                // Without a subscription only the server view exists.
                readonly property int tab: page.activeSub !== null ? page.inspectorTab : 1

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 0

                    Row {
                        Layout.fillWidth: true
                        Layout.leftMargin: 10
                        Layout.topMargin: 4
                        spacing: 4
                        visible: page.activeSub !== null
                        Repeater {
                            model: [I18n.t("Подписка"), I18n.t("Сервер")]
                            delegate: Item {
                                id: inspTab
                                required property int index
                                required property string modelData
                                readonly property bool active: inspector.tab === index
                                width: inspTabText.implicitWidth + 20
                                height: 36
                                Text {
                                    id: inspTabText
                                    anchors.centerIn: parent
                                    text: inspTab.modelData
                                    color: inspTab.active || inspTabHover.hovered ? Theme.text : Theme.textMuted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontNormal
                                    font.weight: inspTab.active ? Font.DemiBold : Font.Normal
                                }
                                Rectangle {
                                    anchors.bottom: parent.bottom
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    width: inspTab.active ? Math.max(16, inspTabText.implicitWidth - 4) : 0
                                    height: 3
                                    radius: 1.5
                                    color: Theme.accent
                                }
                                HoverHandler { id: inspTabHover; cursorShape: Qt.PointingHandCursor }
                                TapHandler { onTapped: page.inspectorTab = inspTab.index }
                            }
                        }
                    }
                    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.divider; visible: page.activeSub !== null }

                    FluentScroll {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        roundedClip: false

                        ColumnLayout {
                            width: parent.width
                            spacing: 0

                            // ===== Подписка =====
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.margins: 14
                                visible: inspector.tab === 0 && page.activeSub !== null
                                spacing: 12

                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 8
                                    Text {
                                        Layout.fillWidth: true
                                        text: page.activeSub ? (page.activeSub.name || I18n.t("Подписка")) : ""
                                        textFormat: Text.PlainText
                                        elide: Text.ElideRight
                                        color: Theme.text
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontStrong + 2
                                        font.weight: Font.DemiBold
                                    }
                                    Rectangle {
                                        visible: page.subState.status.length > 0
                                        implicitWidth: subStatusText.implicitWidth + 16
                                        implicitHeight: 22
                                        radius: 11
                                        color: Qt.rgba(page.subState.statusColor.r, page.subState.statusColor.g, page.subState.statusColor.b, 0.16)
                                        Text {
                                            id: subStatusText
                                            anchors.centerIn: parent
                                            text: "● " + page.subState.status
                                            color: page.subState.statusColor
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                        }
                                    }
                                }
                                Text {
                                    Layout.fillWidth: true
                                    visible: text.length > 0
                                    text: page.inspectorDescription()
                                    textFormat: Text.PlainText
                                    color: page.subState.expired ? Theme.danger : Theme.textMuted
                                    wrapMode: Text.WordWrap
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSmall
                                    Layout.topMargin: -6
                                }
                                Text {
                                    Layout.fillWidth: true
                                    visible: text.length > 0
                                    text: I18n.t(String(page.providerView.notice || ""))
                                    textFormat: Text.PlainText
                                    color: Theme.warning
                                    wrapMode: Text.WordWrap
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSmall
                                }

                                // Traffic ring + expiry.
                                RowLayout {
                                    Layout.fillWidth: true
                                    visible: page.subState.hasTraffic || page.subState.expires.length > 0
                                    spacing: 14
                                    Item {
                                        visible: page.subState.hasTraffic
                                        Layout.preferredWidth: 104
                                        Layout.preferredHeight: 104
                                        Canvas {
                                            anchors.fill: parent
                                            readonly property real fraction: page.subState.limited ? page.subState.fraction : 1
                                            readonly property color ink: page.subState.trafficColor
                                            onFractionChanged: requestPaint()
                                            onInkChanged: requestPaint()
                                            onPaint: {
                                                var ctx = getContext("2d");
                                                ctx.reset();
                                                var r = width / 2 - 6;
                                                ctx.lineWidth = 8;
                                                ctx.lineCap = "round";
                                                ctx.strokeStyle = Theme.dark ? "rgba(0,0,0,0.28)" : "rgba(0,0,0,0.08)";
                                                ctx.beginPath(); ctx.arc(width / 2, height / 2, r, 0, Math.PI * 2); ctx.stroke();
                                                if (fraction > 0) {
                                                    ctx.strokeStyle = ink;
                                                    ctx.beginPath();
                                                    ctx.arc(width / 2, height / 2, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * Math.min(1, fraction));
                                                    ctx.stroke();
                                                }
                                            }
                                        }
                                        Column {
                                            anchors.centerIn: parent
                                            width: 84
                                            Text {
                                                anchors.horizontalCenter: parent.horizontalCenter
                                                text: page.subState.usedText
                                                color: Theme.text
                                                font.family: Theme.fontFamily
                                                font.pixelSize: Theme.fontNormal + 2
                                                font.weight: Font.DemiBold
                                            }
                                            Text {
                                                width: parent.width
                                                horizontalAlignment: Text.AlignHCenter
                                                wrapMode: Text.WordWrap
                                                text: page.subState.limited ? page.subState.limitText : I18n.t("Безлимитный трафик")
                                                color: page.subState.limited ? Theme.textMuted : Theme.success
                                                font.family: Theme.fontFamily
                                                font.pixelSize: Theme.fontSmall
                                            }
                                        }
                                    }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        visible: page.subState.expires.length > 0
                                        spacing: 2
                                        Text { text: I18n.t("Истекает"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                                        Text { text: page.subState.expires; color: page.subState.expired ? Theme.danger : Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                                        Text {
                                            visible: page.subState.daysLeft !== ""
                                            text: page.subState.daysLeft
                                            color: page.subState.expired ? Theme.danger : (page.subState.soon ? Theme.warning : Theme.success)
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                        }
                                    }
                                }

                                Repeater {
                                    model: page.providerView.banners || []
                                    delegate: Rectangle {
                                        required property var modelData
                                        Layout.fillWidth: true
                                        implicitHeight: bannerCol.implicitHeight + 16
                                        radius: Theme.radiusSmall
                                        color: modelData.background || Theme.accent
                                        ColumnLayout {
                                            id: bannerCol
                                            anchors.fill: parent
                                            anchors.margins: 8
                                            spacing: 6
                                            Text {
                                                Layout.fillWidth: true
                                                text: I18n.t(String(modelData.text || ""))
                                                textFormat: Text.PlainText
                                                wrapMode: Text.WordWrap
                                                color: modelData.foreground || "#FFFFFF"
                                                font.family: Theme.fontFamily
                                                font.pixelSize: Theme.fontSmall
                                            }
                                            Rectangle {
                                                visible: String(modelData.button || "").length > 0 && String(modelData.url || "").length > 0
                                                implicitWidth: bannerBtn.implicitWidth + 20
                                                implicitHeight: 26
                                                radius: 4
                                                color: modelData.buttonBackground || Theme.accent
                                                Text { id: bannerBtn; anchors.centerIn: parent; text: String(modelData.button || ""); color: modelData.buttonForeground || "#FFFFFF"; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                                                HoverHandler { cursorShape: Qt.PointingHandCursor }
                                                TapHandler { onTapped: App.openUrl(String(modelData.url)) }
                                            }
                                        }
                                    }
                                }

                                Rectangle {
                                    Layout.fillWidth: true
                                    visible: String(page.providerView.announcement || "").length > 0
                                    implicitHeight: annText.implicitHeight + 16
                                    radius: Theme.radiusSmall
                                    color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.14)
                                    Text {
                                        id: annText
                                        anchors.fill: parent
                                        anchors.margins: 8
                                        text: String(page.providerView.announcement || "")
                                        textFormat: Text.PlainText
                                        wrapMode: Text.WordWrap
                                        color: Theme.text
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontSmall
                                    }
                                }

                                Flow {
                                    Layout.fillWidth: true
                                    visible: (page.providerView.links || []).length > 0
                                    spacing: 14
                                    Repeater {
                                        model: page.providerView.links || []
                                        delegate: Text {
                                            required property var modelData
                                            text: I18n.t(String(modelData.label || ""))
                                            color: linkHover.hovered ? Theme.text : Theme.accent
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall + 1
                                            font.underline: linkHover.hovered
                                            HoverHandler { id: linkHover; cursorShape: Qt.PointingHandCursor }
                                            TapHandler { onTapped: App.openUrl(String(modelData.url)) }
                                        }
                                    }
                                }
                            }

                            // ===== Сервер =====
                            ColumnLayout {
                                id: serverInfo
                                Layout.fillWidth: true
                                Layout.margins: 14
                                visible: inspector.tab === 1
                                spacing: 12
                                readonly property var d: page.inspectedNode
                                readonly property bool has: d !== null && d.id !== undefined

                                Text {
                                    Layout.fillWidth: true
                                    visible: !serverInfo.has
                                    text: I18n.t("Выберите сервер в списке.")
                                    color: Theme.textMuted
                                    wrapMode: Text.WordWrap
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSmall
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    visible: serverInfo.has
                                    spacing: 2
                                    Text {
                                        Layout.fillWidth: true
                                        text: serverInfo.has ? serverInfo.d.name : ""
                                        textFormat: Text.PlainText
                                        wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                                        color: Theme.text
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontStrong + 2
                                        font.weight: Font.DemiBold
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: !serverInfo.has ? "" : (serverInfo.d.security && String(serverInfo.d.protocol).indexOf(serverInfo.d.security) < 0
                                            ? serverInfo.d.protocol + " · " + serverInfo.d.security : serverInfo.d.protocol)
                                        textFormat: Text.PlainText
                                        elide: Text.ElideRight
                                        color: Theme.textMuted
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontSmall
                                    }
                                }

                                // Ping / speed / status tiles.
                                GridLayout {
                                    Layout.fillWidth: true
                                    visible: serverInfo.has
                                    columns: 3
                                    columnSpacing: 8
                                    Repeater {
                                        model: serverInfo.has ? [
                                            { label: I18n.t("Пинг"), value: serverInfo.d.ping > 0 ? serverInfo.d.ping + " ms" : "—",
                                              color: serverInfo.d.ping > 0 ? Theme.pingColor(serverInfo.d.ping) : Theme.textFaint },
                                            { label: I18n.t("Скорость"), value: serverInfo.d.speed >= 0 ? serverInfo.d.speed.toFixed(1) + " MB/s" : "—",
                                              color: serverInfo.d.speed >= 0 ? Theme.text : Theme.textFaint },
                                            { label: I18n.t("Статус"), value: serverInfo.d.alive === "ok" ? "OK" : (serverInfo.d.alive === "fail" ? "✕" : "—"),
                                              color: serverInfo.d.alive === "ok" ? Theme.success : (serverInfo.d.alive === "fail" ? Theme.danger : Theme.textFaint) }
                                        ] : []
                                        delegate: Rectangle {
                                            required property var modelData
                                            Layout.fillWidth: true
                                            Layout.preferredWidth: 1
                                            implicitHeight: 54
                                            radius: Theme.radiusSmall
                                            color: Theme.dark ? Qt.rgba(0, 0, 0, 0.16) : Qt.rgba(0, 0, 0, 0.04)
                                            Column {
                                                anchors.left: parent.left
                                                anchors.leftMargin: 10
                                                anchors.verticalCenter: parent.verticalCenter
                                                spacing: 2
                                                Text { text: modelData.label; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                                                Text { text: modelData.value; color: modelData.color; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal + 1; font.weight: Font.DemiBold }
                                            }
                                        }
                                    }
                                }

                                // All connection details.
                                Repeater {
                                    model: serverInfo.has ? page.serverDetailRows(serverInfo.d) : []
                                    delegate: RowLayout {
                                        required property var modelData
                                        Layout.fillWidth: true
                                        spacing: 10
                                        Text {
                                            text: modelData[0]
                                            color: Theme.textMuted
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                            wrapMode: Text.WordWrap
                                            Layout.preferredWidth: 104
                                            Layout.maximumWidth: 104
                                            Layout.alignment: Qt.AlignTop
                                        }
                                        Text {
                                            text: modelData[1]
                                            textFormat: Text.PlainText
                                            color: Theme.text
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                                            maximumLineCount: 3
                                            elide: Text.ElideRight
                                            Layout.fillWidth: true
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ── actions pinned to the bottom ─────
                    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.divider; visible: subActions.visible || serverActions.visible }
                    RowLayout {
                        id: subActions
                        Layout.fillWidth: true
                        Layout.margins: 12
                        visible: inspector.tab === 0 && page.activeSub !== null
                        spacing: 6
                        AccentButton {
                            kind: "ghost"
                            glyph: ""
                            text: I18n.t("Обновить")
                            Layout.fillWidth: true
                            onClicked: { var s = page.selectedSub(); if (s) App.updateSubscription(s.url) }
                        }
                        AccentButton { kind: "ghost"; iconOnly: true; glyph: ""; text: I18n.t("Свойства подписки"); onClicked: infoDialog.openInfo() }
                        AccentButton {
                            kind: "ghost"
                            iconOnly: true
                            glyph: ""
                            text: I18n.t("Копировать ссылку")
                            visible: !(page.providerView.hideUrl === true)
                            onClicked: { var s = page.selectedSub(); if (s) App.copyText(String(s.url || "")) }
                        }
                        AccentButton { kind: "danger"; iconOnly: true; glyph: ""; text: I18n.t("Удалить подписку (с серверами)"); onClicked: { var s = page.selectedSub(); if (s) App.removeSubscription(s.url, true) } }
                    }
                    ColumnLayout {
                        id: serverActions
                        Layout.fillWidth: true
                        Layout.margins: 12
                        visible: inspector.tab === 1 && serverInfo.has
                        spacing: 6
                        AccentButton { Layout.fillWidth: true; kind: "accent"; glyph: ""; text: I18n.t("Установить как активный"); onClicked: App.selectNode(page.inspectedNodeId) }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 6
                            AccentButton { Layout.fillWidth: true; kind: "ghost"; glyph: ""; text: I18n.t("Редактировать"); onClicked: editDialog.openFor(page.inspectedNodeId) }
                            AccentButton { kind: "ghost"; iconOnly: true; glyph: ""; text: I18n.t("Пинг"); onClicked: App.pingNodes([page.inspectedNodeId]) }
                            AccentButton { kind: "ghost"; iconOnly: true; glyph: ""; text: I18n.t("Копировать ссылку"); onClicked: App.copyNodeLink(page.inspectedNodeId) }
                            AccentButton { kind: "ghost"; iconOnly: true; glyph: ""; text: I18n.t("Экспорт в QR-код"); onClicked: App.showNodeQr(page.inspectedNodeId) }
                        }
                    }
                }
            }
        }
    }

    // ── toolbar menus ────
    FluentMenu {
        id: addMenu
        width: 280
        showIcons: true
        FluentMenuItem { glyph: ""; text: I18n.t("Импорт подписки"); onTriggered: subDialog.openNew() }
        FluentMenuItem { glyph: ""; text: I18n.t("Импорт из буфера"); hint: "Ctrl+V"; onTriggered: App.importClipboard() }
        FluentMenuItem { glyph: ""; text: I18n.t("Импорт файла конфигурации"); onTriggered: App.importNodeFile() }
        FluentMenuItem { glyph: ""; text: I18n.t("Добавить сервер вручную"); onTriggered: editDialog.openNew(page.filterGroup || "Default") }
        FluentMenuSeparator {}
        FluentMenuItem { glyph: ""; text: I18n.t("Создать группу"); onTriggered: groupDialog.openNew() }
    }
    FluentMenu {
        id: moreMenu
        width: 290
        showIcons: true
        FluentMenuItem { glyph: ""; text: I18n.t("Выбрать все"); hint: "Ctrl+A"; enabled: list.count > 0; onTriggered: page.selectAll() }
        FluentMenuItem { glyph: ""; text: I18n.t("Тест скорости всех в списке"); enabled: list.count > 0; onTriggered: App.speedTestNodes(page.visibleIds()) }
        FluentMenuItem { glyph: ""; text: I18n.t("Остановить тест"); enabled: App.testsRunning; onTriggered: App.cancelTests() }
        FluentMenuItem { glyph: ""; text: I18n.t("Экспорт всех (ссылки)"); onTriggered: App.exportNodeLinks([]) }
        FluentMenuSeparator {}
        FluentMenuItem { glyph: ""; text: I18n.t("Свойства подписки"); enabled: page.selectedSub() !== null; onTriggered: infoDialog.openInfo() }
        FluentMenuItem { glyph: ""; text: I18n.t("Обновить все подписки"); enabled: App.subscriptions.length > 0; onTriggered: App.updateAllSubscriptions() }
        FluentMenuItem { glyph: ""; danger: true; text: I18n.t("Удалить подписку (с серверами)"); enabled: page.selectedSub() !== null; onTriggered: { var s = page.selectedSub(); if (s) App.removeSubscription(s.url, true) } }
    }

    // ── context menu: every server action lives here ────
    FluentMenu {
        id: ctxMenu
        width: 290
        showIcons: true
        readonly property bool single: page.selCount === 1
        readonly property var targets: page.selectedIds()

        FluentMenuItem { glyph: ""; text: I18n.t("Установить как активный"); enabled: ctxMenu.single; onTriggered: App.selectNode(page.menuNodeId()) }
        FluentMenuSeparator {}
        FluentMenu {
            title: page.selCount > 1 ? I18n.t("Пинг — способ из настроек ({count})", { count: page.selCount }) : I18n.t("Пинг")
            glyph: ""
            width: 260
            showIcons: true
            // Click = ping with the method from settings; hover opens the other methods.
            clickable: true
            onActivated: App.pingNodes(ctxMenu.targets)
            FluentMenuItem { glyph: ""; text: I18n.t("Способ из настроек"); onTriggered: App.pingNodes(ctxMenu.targets) }
            FluentMenuSeparator {}
            FluentMenuItem { glyph: ""; text: I18n.t("Тест TCP ping"); onTriggered: App.tcpingNodes(ctxMenu.targets) }
            FluentMenuItem { glyph: ""; text: I18n.t("Тест HTTP GET"); onTriggered: App.httpGetNodes(ctxMenu.targets) }
            FluentMenuItem { glyph: "\uE968"; text: I18n.t("Тест ICMP"); onTriggered: App.icmpNodes(ctxMenu.targets) }
        }
        FluentMenuItem { glyph: ""; text: I18n.t("Тест скорости загрузки ({count})", { count: page.selCount }); onTriggered: App.downloadSpeedNodes(ctxMenu.targets) }
        FluentMenuItem { glyph: "\uE71A"; text: I18n.t("Остановить тест"); enabled: App.testsRunning; onTriggered: App.cancelTests() }
        FluentMenuSeparator {}
        FluentMenuItem { glyph: ""; text: I18n.t("Редактировать"); enabled: ctxMenu.single; onTriggered: editDialog.openFor(page.firstSelected()) }
        FluentMenuItem { glyph: ""; text: page.selCount > 1 ? I18n.t("Переместить ({count})", { count: page.selCount }) : I18n.t("Переместить"); enabled: page.selCount > 0; onTriggered: bulkDialog.openFor(ctxMenu.targets) }
        FluentMenuItem { glyph: ""; text: I18n.t("Копировать ссылку"); hint: "Ctrl+C"; enabled: ctxMenu.single; onTriggered: App.copyNodeLink(page.firstSelected()) }
        FluentMenu {
            title: I18n.t("Экспорт")
            glyph: ""
            width: 270
            showIcons: true
            FluentMenuItem { glyph: ""; text: I18n.t("Экспорт выбранных — ссылки ({count})", { count: page.selCount }); onTriggered: App.exportNodeLinks(ctxMenu.targets) }
            FluentMenuItem { glyph: ""; text: I18n.t("Экспорт в QR-код"); enabled: ctxMenu.single; onTriggered: App.showNodeQr(page.firstSelected()) }
            FluentMenuSeparator {}
            FluentMenuItem { glyph: ""; text: I18n.t("Экспорт outbound JSON"); enabled: ctxMenu.single; onTriggered: App.saveOutboundJson(page.firstSelected()) }
            FluentMenuItem { glyph: ""; text: I18n.t("Экспорт runtime JSON"); enabled: ctxMenu.single; onTriggered: App.saveRuntimeJson(page.firstSelected()) }
        }
        FluentMenuSeparator {}
        FluentMenuItem { glyph: ""; text: I18n.t("В начало списка"); enabled: ctxMenu.single; onTriggered: App.reorderNode(page.menuNodeId(), "top") }
        FluentMenuItem { glyph: ""; text: I18n.t("В конец списка"); enabled: ctxMenu.single; onTriggered: App.reorderNode(page.menuNodeId(), "bottom") }
        FluentMenuItem { glyph: ""; text: I18n.t("Выбрать все"); hint: "Ctrl+A"; onTriggered: page.selectAll() }
        FluentMenuSeparator {}
        FluentMenuItem { glyph: ""; danger: true; text: I18n.t("Удалить ({count})", { count: page.selCount }); hint: "Del"; enabled: page.selCount > 0; onTriggered: App.deleteNodes(ctxMenu.targets) }
    }

    // ── keyboard shortcuts ───────────────────────
    readonly property bool _kbReady: page.visible && page.enabled
        && !groupCombo.popup.opened && !sortCombo.popup.opened && !subCombo.popup.opened
        && !subDialog.opened && !infoDialog.opened && !deleteGroupDialog.opened && !qrDialog.opened && !importProgressDialog.opened
        && !searchInput.activeFocus
        && !editDialog.opened
        && !bulkDialog.opened
        && !groupDialog.opened
        && !ctxMenu.opened
        && !addMenu.opened && !moreMenu.opened && !sourcePopup.opened

    Shortcut {                              // Ctrl+V → импорт из буфера
        sequences: [ StandardKey.Paste ]
        enabled: page._kbReady
        autoRepeat: false
        onActivated: App.importClipboard()
    }
    Shortcut {                              // Ctrl+A → выбрать все
        sequences: [ StandardKey.SelectAll ]
        enabled: page._kbReady && list.count > 0
        autoRepeat: false
        onActivated: page.selectAll()
    }
    Shortcut {                              // Ctrl+C → копировать ссылку
        sequences: [ StandardKey.Copy ]
        enabled: page._kbReady && page.selCount === 1
        autoRepeat: false
        onActivated: App.copyNodeLink(page.firstSelected())
    }
    Shortcut {                              // Delete → удалить выбранные
        sequences: [ StandardKey.Delete ]
        enabled: page._kbReady && page.selCount > 0
        autoRepeat: false
        onActivated: App.deleteNodes(page.selectedIds())
    }
    Shortcut {                              // Esc → снять выбор
        sequences: [ StandardKey.Cancel ]
        enabled: page._kbReady && page.selCount > 0
        autoRepeat: false
        onActivated: page.clearSel()
    }

    // Диалоги редактирования (одиночного и массового) — порт ui/node_edit_dialog.py и ui/bulk_edit_dialog.py.
    NodeEditDialog {
        id: editDialog
        onNodeCreated: function(nodeId) { page.revealImportedNode(nodeId) }
    }
    BulkEditDialog { id: bulkDialog }

    // ── диалог создания ручной группы без подписки ──────────────
    Dialog {
        id: groupDialog
        modal: true
        dim: true
        parent: Overlay.overlay
        anchors.centerIn: parent
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.45) }
        width: 420
        padding: 18
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        function openNew() {
            groupNameField.text = "";
            groupDialog.open();
            groupNameField.forceActiveFocus();
        }

        background: Rectangle {
            color: Theme.flyout
            radius: 10
            border.width: 1
            border.color: Theme.flyoutBorder
        }

        contentItem: ColumnLayout {
            spacing: 12

            Text {
                text: I18n.t("Новая группа")
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontStrong
                font.bold: true
            }
            Text {
                text: I18n.t("Группа будет создана отдельно от подписок. В неё можно переносить серверы через редактирование или массовое редактирование.")
                color: Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Text { text: I18n.t("Имя группы"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            FluentTextField {
                id: groupNameField
                Layout.fillWidth: true
                implicitHeight: Theme.controlHeight
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                selectByMouse: true
                leftPadding: 10; rightPadding: 10
                placeholderText: I18n.t("Например: Мои серверы")
                placeholderTextColor: Theme.textFaint
                background: Rectangle { radius: Theme.radiusSmall; color: Theme.card; border.width: 1; border.color: groupNameField.activeFocus ? Theme.accent : Theme.borderSolid }
                Keys.onReturnPressed: createGroupButton.clicked()
                Keys.onEnterPressed: createGroupButton.clicked()
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 6
                Item { Layout.fillWidth: true }
                AccentButton { kind: "ghost"; text: I18n.t("Отмена"); onClicked: groupDialog.close() }
                AccentButton {
                    id: createGroupButton
                    kind: "accent"; text: I18n.t("Создать")
                    enabled: groupNameField.text.trim().length > 0
                    onClicked: {
                        var name = groupNameField.text.trim();
                        if (App.createManualGroup(name)) {
                            groupDialog.close();
                            page.selectGroupByName(name);
                        }
                    }
                }
            }
        }
    }

    // ── диалог импорта подписки (URL + имя группы) ──────────────
    Dialog {
        id: subDialog
        modal: true
        dim: true
        parent: Overlay.overlay
        anchors.centerIn: parent
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.45) }
        width: 480
        padding: 18
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        function openNew() {
            subUrlField.text = "";
            subNameField.text = "";
            subDialog.open();
            subUrlField.forceActiveFocus();
        }

        background: Rectangle {
            color: Theme.flyout
            radius: 10
            border.width: 1
            border.color: Theme.flyoutBorder
        }

        contentItem: ColumnLayout {
            spacing: 12

            Text {
                text: I18n.t("Импорт подписки")
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontStrong
                font.bold: true
            }
            Text {
                text: I18n.t("Вставьте ссылку на подписку. Серверы автоматически попадут в новую группу.")
                color: Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Text { text: I18n.t("URL подписки"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            FluentTextField {
                id: subUrlField
                Layout.fillWidth: true
                implicitHeight: Theme.controlHeight
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                selectByMouse: true
                leftPadding: 10; rightPadding: 10
                placeholderText: "https://…"
                placeholderTextColor: Theme.textFaint
                background: Rectangle { radius: Theme.radiusSmall; color: Theme.card; border.width: 1; border.color: subUrlField.activeFocus ? Theme.accent : Theme.borderSolid }
            }

            Text { text: I18n.t("Имя группы (необязательно)"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            FluentTextField {
                id: subNameField
                Layout.fillWidth: true
                implicitHeight: Theme.controlHeight
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                selectByMouse: true
                leftPadding: 10; rightPadding: 10
                placeholderText: I18n.t("Пусто — имя будет создано автоматически")
                placeholderTextColor: Theme.textFaint
                background: Rectangle { radius: Theme.radiusSmall; color: Theme.card; border.width: 1; border.color: subNameField.activeFocus ? Theme.accent : Theme.borderSolid }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 6
                Item { Layout.fillWidth: true }
                AccentButton { kind: "ghost"; text: I18n.t("Отмена"); onClicked: subDialog.close() }
                AccentButton {
                    kind: "accent"; text: I18n.t("Импортировать")
                    enabled: subUrlField.text.trim().length > 0
                    onClicked: {
                        App.importSubscription(subUrlField.text.trim(), subNameField.text.trim());
                        subDialog.close();
                    }
                }
            }
        }
    }

    // ── диалог свойств подписки (инфо из ссылки, если есть) ──────
    Dialog {
        id: infoDialog
        modal: true
        dim: true
        parent: Overlay.overlay
        anchors.centerIn: parent
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.45) }
        width: 520
        padding: 18
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        function openInfo() {
            if (page.selectedSub() === null) return;
            var i = subCombo.currentIndex - 1;
            if (i < 0 || i >= App.subscriptions.length) i = 0;
            infoCombo.currentIndex = i;
            infoDialog.open();
        }

        background: Rectangle {
            color: Theme.flyout
            radius: 10
            border.width: 1
            border.color: Theme.flyoutBorder
        }

        contentItem: ColumnLayout {
            spacing: 12

            Text {
                text: I18n.t("Свойства подписки")
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontStrong
                font.bold: true
            }

            // Переключатель подписки
            FilterCombo {
                id: infoCombo
                Layout.fillWidth: true
                enabled: App.subscriptions.length > 0
                model: App.subscriptions.length > 0
                    ? App.subscriptions.map(function(s) { return (s.name && s.name.length ? s.name : s.url) + " (" + (s.node_count || 0) + ")"; })
                    : [I18n.t("Нет подписок")]
            }

            // Premium-подписка может передать несколько десятков параметров;
            // свойства остаются доступны в компактном прокручиваемом блоке.
            FluentScroll {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(420, Math.max(90, infoProperties.implicitHeight))
                roundedClip: false

            Column {
                id: infoProperties
                width: parent ? Math.max(0, parent.width - 4) : 0
                spacing: 6
                Repeater {
                    model: page.infoRows(page.infoSub())
                    delegate: RowLayout {
                        id: infoRow
                        width: infoProperties.width
                        spacing: 12
                        readonly property bool isUrlRow: String(modelData[0]).toLowerCase() === "url"
                        Text {
                            text: modelData[0]
                            textFormat: Text.PlainText
                            color: Theme.textMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            Layout.preferredWidth: 140
                            Layout.alignment: Qt.AlignTop
                        }
                        Text {
                            visible: !infoRow.isUrlRow
                            text: modelData[1]
                            textFormat: Text.PlainText
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            Layout.preferredWidth: 1
                            Layout.alignment: Qt.AlignTop
                        }
                        FluentTextEdit {
                            visible: infoRow.isUrlRow
                            text: modelData[1]
                            color: Theme.text
                            selectedTextColor: Theme.text
                            selectionColor: Theme.accentSoft
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            wrapMode: TextEdit.WrapAnywhere
                            readOnly: true
                            selectByMouse: true
                            textFormat: TextEdit.PlainText
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            Layout.preferredWidth: 1
                        }
                        AccentButton {
                            visible: infoRow.isUrlRow
                            kind: "ghost"
                            iconOnly: true
                            glyph: "\uE8C8"
                            text: I18n.t("Копировать ссылку")
                            Layout.alignment: Qt.AlignTop
                            onClicked: App.copyText(modelData[1])
                        }
                        AccentButton {
                            visible: infoRow.isUrlRow
                            kind: "ghost"
                            iconOnly: true
                            glyph: "\uE8A7"
                            text: I18n.t("Открыть")
                            Layout.alignment: Qt.AlignTop
                            onClicked: App.openUrl(modelData[1])
                        }
                    }
                }
            }
            }

            Text {
                visible: { var s = page.infoSub(); return s ? !(s.userinfo && typeof s.userinfo === "object" && Object.keys(s.userinfo).length > 0) : false; }
                text: I18n.t("В ссылке этой подписки нет доп. информации о пользователе.")
                color: Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 6
                Layout.rightMargin: 4
                Layout.bottomMargin: 4
                Item { Layout.fillWidth: true }
                AccentButton { kind: "accent"; text: I18n.t("Закрыть"); onClicked: infoDialog.close() }
            }
        }
    }

    FluentDialog {
        id: deleteGroupDialog
        property string groupName: ""
        title: I18n.t("Удалить группу")
        okText: I18n.t("Удалить")
        cancelText: I18n.t("Отмена")
        width: 440
        onAccepted: {
            var removed = App.deleteGroup(groupName);
            if (removed)
                page.selectGroupByName("Default");
        }
        contentItem: Text {
            text: I18n.t("Группа «{name}», все её серверы и связанная подписка будут удалены.").replace("{name}", deleteGroupDialog.groupName)
            color: Theme.textMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontNormal
            wrapMode: Text.WordWrap
        }
    }

    // ---- QR dialog ---------------------------------------------------
    FluentDialog {
        id: qrDialog
        property string qrSource: ""
        property string qrName: ""
        title: qrName.length ? I18n.t("QR-код: ") + qrName : I18n.t("QR-код сервера")
        okText: I18n.t("Закрыть")
        showCancelButton: false
        width: 360
        contentItem: ColumnLayout {
            spacing: 12
            Image {
                source: qrDialog.qrSource
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: 280
                Layout.preferredHeight: 280
                fillMode: Image.PreserveAspectFit
                smooth: false
            }
            Text {
                text: I18n.t("Отсканируйте код в клиенте, чтобы импортировать сервер.")
                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter; Layout.fillWidth: true
            }
        }
    }

    // ── диалог прогресса импорта подписки ──────────────────────────
    Dialog {
        id: importProgressDialog
        modal: true
        dim: true
        parent: Overlay.overlay
        anchors.centerIn: parent
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.45) }
        width: 320
        padding: 20
        closePolicy: Popup.NoAutoClose
        visible: App.subscriptionImporting

        background: Rectangle {
            color: Theme.flyout
            radius: 10
            border.width: 1
            border.color: Theme.flyoutBorder
        }

        contentItem: ColumnLayout {
            spacing: 16
            Text {
                text: I18n.t("Импорт подписки")
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontStrong
                font.bold: true
                Layout.alignment: Qt.AlignHCenter
            }
            ProgressBar {
                indeterminate: true
                Layout.fillWidth: true
            }
            Text {
                text: App.subscriptionImportStatus || I18n.t("Загрузка подписки...")
                color: Theme.textMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                Layout.alignment: Qt.AlignHCenter
                wrapMode: Text.WordWrap
                horizontalAlignment: Text.AlignHCenter
                Layout.fillWidth: true
            }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                AccentButton {
                    kind: "ghost"
                    text: I18n.t("Отмена")
                    onClicked: App.cancelSubscriptionImport()
                }
                Item { Layout.fillWidth: true }
            }
        }
    }

    Connections {
        target: App
        function onNodeImported(nodeId) {
            page.revealImportedNode(nodeId);
        }
        function onSubscriptionImported(groupName) {
            page.selectGroupByName(groupName);
            page.selectSubscriptionByGroup(groupName);
        }
        function onSettingsChanged() {
            if (!page.viewStateRestored && App.profileLoaded)
                Qt.callLater(page.restoreServerViewState);
        }
        function onSubscriptionsChanged() {
            if (!page.viewStateRestored && App.profileLoaded)
                Qt.callLater(page.restoreServerViewState);
            else
                Qt.callLater(page.restoreSubscriptionSelection);
        }
        function onNodeQrReady(dataUri, name) {
            qrDialog.qrSource = dataUri;
            qrDialog.qrName = name;
            qrDialog.open();
        }
    }
}
