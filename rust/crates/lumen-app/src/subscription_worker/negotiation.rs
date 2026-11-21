//! Negotiate a usable subscription representation before preparing its nodes.

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{LazyLock, OnceLock};

use caseless::default_case_fold_str;
use lumen_core::link_parser::{
    MAX_IMPORT_BYTES, is_happ_auto_placeholder, normalize_node_outbound, parse_links_text,
    validate_node_outbound,
};
use lumen_core::models::Node;
use lumen_core::paths::get_install_id;
use lumen_core::pyjson;
use lumen_core::subscription::{
    fetcher::{SubscriptionFetcherError, SubscriptionHttpPayload, fetch_subscription_http},
    request::{
        ClientProfile, SubscriptionSource, client_profiles, decode_percent, happ_windows_profile,
        needs_magvpn_variants, provider_id, request_hwid, resolve_source,
    },
    response::{SubscriptionResponse, merge_subscription_info},
};
use regex::Regex;
use serde_json::{Value, json};

use super::SubscriptionJob;

const UNSUPPORTED_CLIENT: &[&str] = &[
    "client not supported",
    "client is not supported",
    "unsupported client",
    "app not supported",
    "unsupported app",
    "application is not supported",
    "update your app",
    "update your client",
    "use another client",
    "клиент не поддерживается",
    "неподдерживаемый клиент",
    "приложение не поддерживается",
    "обновите приложение",
    "обновите клиент",
    "используйте другой клиент",
];
const NO_SERVERS: &str = "нет подходящих серверов";
const FETCH_FAILED: &str = "Не удалось загрузить подписку";
const PROVIDER_SAYS: &str = "Провайдер сообщает: ";
const PROVIDER_PLACEHOLDER: &str = "Провайдер подписки вернул служебное сообщение";
static MACHINE_HWID: OnceLock<Option<String>> = OnceLock::new();
static NOTICE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"«([^»]*)»").unwrap());
static NOTICE_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[^\w@#]+").unwrap());

struct Candidate {
    response: SubscriptionResponse,
    nodes: Vec<Node>,
    errors: Vec<String>,
    validation_errors: Vec<String>,
    usable: bool,
}

impl Candidate {
    fn parse(payload: SubscriptionHttpPayload, profile: &str) -> Self {
        let response = SubscriptionResponse::from_http(payload, profile);
        let (mut nodes, errors) = if response.not_modified {
            (Vec::new(), Vec::new())
        } else {
            parse_links_text(&response.text, false)
                .unwrap_or_else(|error| (Vec::new(), vec![error.to_string()]))
        };
        let mut usable = false;
        let mut validation_errors = Vec::new();
        for node in &mut nodes {
            normalize_node_outbound(node);
            if let Some(error) = validate_node_outbound(node) {
                push_unique(&mut validation_errors, error);
            } else {
                usable = true;
            }
        }
        Self {
            response,
            nodes,
            errors,
            validation_errors,
            usable,
        }
    }

    fn accepted(&self) -> bool {
        self.usable && !self.is_client_stub()
    }

    fn is_client_stub(&self) -> bool {
        if self.nodes.len() > 2 {
            return false;
        }
        let raw = &self.response.text;
        let decoded = decode_percent(raw);
        let text = default_case_fold_str(&format!("{raw}\n{decoded}"));
        raw.trim().is_empty()
            || UNSUPPORTED_CLIENT
                .iter()
                .any(|marker| text.contains(marker))
    }

    fn rejection(&self) -> String {
        if self.usable && self.is_client_stub() {
            return "панель вернула заглушку «клиент не поддерживается»".into();
        }
        if !self.usable {
            let mut notices = Vec::new();
            for node in &self.nodes {
                if matches!(node.server.trim(), "0.0.0.0" | "::")
                    && node.port <= 1
                    && validate_node_outbound(node)
                        .is_some_and(|error| error.starts_with(PROVIDER_PLACEHOLDER))
                {
                    let label = NOTICE_PREFIX.replace_all(&node.name, "");
                    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !label.is_empty() {
                        push_unique(&mut notices, label);
                    }
                }
            }
            if !notices.is_empty() {
                return format_notices(&notices, 3);
            }
        }
        self.validation_detail()
    }

    fn validation_detail(&self) -> String {
        let errors = if self.validation_errors.is_empty() {
            &self.errors
        } else {
            &self.validation_errors
        };
        if errors.is_empty() {
            NO_SERVERS.into()
        } else {
            errors
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        }
    }

    fn auto_placeholders(&self) -> BTreeSet<&str> {
        self.nodes
            .iter()
            .filter(|node| is_happ_auto_placeholder(node))
            .map(|node| node.name.as_str())
            .collect()
    }

    fn finish(self) -> (SubscriptionResponse, Vec<String>) {
        (self.response, self.errors)
    }
}

type RequestFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SubscriptionHttpPayload, SubscriptionFetcherError>> + Send + 'a>>;

/// One HTTP request for a client profile (Python
/// `_fetch_subscription_with_headers`); tests substitute scripted responses.
pub(super) trait Requester: Sync {
    fn request<'a>(
        &'a self,
        job: &'a SubscriptionJob,
        url: &'a str,
        profile: &'a ClientProfile,
    ) -> RequestFuture<'a>;
}

struct HttpRequester;

impl Requester for HttpRequester {
    fn request<'a>(
        &'a self,
        job: &'a SubscriptionJob,
        url: &'a str,
        profile: &'a ClientProfile,
    ) -> RequestFuture<'a> {
        Box::pin(request(job, url, profile))
    }
}

pub(super) async fn fetch(
    job: &SubscriptionJob,
) -> Result<(SubscriptionResponse, Vec<String>), String> {
    fetch_with(job, &HttpRequester).await
}

/// Python `fetch_subscription_payload`: try each client profile until one
/// returns a usable subscription.
pub(super) async fn fetch_with(
    job: &SubscriptionJob,
    requester: &impl Requester,
) -> Result<(SubscriptionResponse, Vec<String>), String> {
    let url = job.url.clone();
    let converter = job.converter_url.clone();
    let source = tokio::task::spawn_blocking(move || resolve_source(&url, &converter))
        .await
        .map_err(|error| error.to_string())??;
    let url = match source {
        SubscriptionSource::Http(url) => url,
        SubscriptionSource::HappBody(body) => {
            let mut candidate = parse_candidate(
                SubscriptionHttpPayload {
                    body: body.into_bytes(),
                    ..Default::default()
                },
                "Happ",
            )
            .await?;
            if !candidate.usable {
                candidate.errors = vec![format!("Happ: {}", candidate.validation_detail())];
                candidate.response.text.clear();
            }
            return Ok(candidate.finish());
        }
    };
    let machine = if job.use_real_hwid {
        tokio::task::spawn_blocking(|| {
            MACHINE_HWID
                .get_or_init(|| lumen_platform::os_info::subscription_hwid(&get_install_id()))
                .as_deref()
        })
        .await
        .map_err(|error| error.to_string())?
    } else {
        None
    };
    let hwid = request_hwid(&job.hwid, machine)?;
    let magvpn = job.client_profile.is_empty() && needs_magvpn_variants(&url, &job.user_agent);
    let profiles = profiles(job, &hwid, magvpn);
    let mut attempts = Vec::new();
    let mut first_info = None;
    let mut last_response = SubscriptionResponse::default();
    let provider = provider_id(&url);
    let network = if job.use_proxy_tun {
        "proxy-tun"
    } else {
        "direct"
    };
    for profile in &profiles {
        let mut fetched = requester.request(job, url.as_str(), profile).await;
        let retried = fetched
            .as_ref()
            .is_err_and(|error| tls_eof(&error.to_string().to_lowercase()));
        if retried {
            fetched = requester.request(job, url.as_str(), profile).await;
        }
        let payload = match fetched {
            Ok(payload) => payload,
            Err(error) => {
                let message = friendly_error(&error, job.use_proxy_tun).unwrap_or_else(|| {
                    format!(
                        "{}{}: {error}",
                        profile.name,
                        if retried { " direct" } else { "" }
                    )
                });
                push_unique(&mut attempts, message);
                let permanent = matches!(http_status(&error), Some(404 | 410));
                if let SubscriptionFetcherError::Http {
                    status, headers, ..
                } = error
                {
                    last_response = SubscriptionResponse {
                        status,
                        headers,
                        ..Default::default()
                    };
                }
                if !retried && permanent {
                    break;
                }
                continue;
            }
        };
        let mut candidate = parse_candidate(payload, &profile.name).await?;
        if let Some(provider) = &provider {
            candidate
                .response
                .userinfo
                .entry("providerId")
                .or_insert_with(|| json!(provider));
        }
        candidate
            .response
            .userinfo
            .insert("networkPath".into(), json!(network));
        if candidate.response.not_modified {
            return Ok(candidate.finish());
        }
        if first_info.is_none() {
            first_info = Some(candidate.response.userinfo.clone());
        }
        if !candidate.auto_placeholders().is_empty() {
            match fetch_happ(requester, job, url.as_str(), &hwid).await {
                Ok(happ) => return Ok(resolve_auto(candidate, happ)),
                Err(error) => {
                    push_unique(
                        &mut attempts,
                        friendly_error(&error, job.use_proxy_tun)
                            .unwrap_or_else(|| format!("{}: {error}", profile.name)),
                    );
                    candidate.response.text.clear();
                    last_response = candidate.response;
                    if matches!(http_status(&error), Some(404 | 410)) {
                        break;
                    }
                    continue;
                }
            }
        }
        if candidate.accepted() {
            if magvpn {
                let merged = match fetch_happ(requester, job, url.as_str(), &hwid).await {
                    Ok(happ) => merge_magvpn(&candidate.response.text, happ),
                    Err(error) => Err(error.to_string()),
                };
                match merged {
                    Ok(text) => candidate.response.text = text,
                    Err(error) => {
                        candidate.response.text.clear();
                        candidate.errors = vec![format!("MagVPN AUTO: {error}")];
                    }
                }
            }
            return Ok(candidate.finish());
        }
        attempts.push(format!(
            "{}{}: {}",
            profile.name,
            if retried { " direct" } else { "" },
            candidate.rejection()
        ));
        candidate.response.text.clear();
        last_response = candidate.response;
    }
    last_response.userinfo = first_info.unwrap_or_default();
    Ok((last_response, collapse_attempts(attempts, &profiles)))
}

fn profiles(job: &SubscriptionJob, hwid: &str, magvpn: bool) -> Vec<ClientProfile> {
    let mut profiles = client_profiles(&job.user_agent, hwid);
    if !job.client_profile.is_empty() {
        let index = profiles
            .iter()
            .position(|profile| profile.name == job.client_profile)
            .unwrap_or(0);
        let mut profile = profiles.swap_remove(index);
        profile.name.clone_from(&job.client_profile);
        profiles = vec![profile];
    }
    if magvpn {
        profiles.truncate(1);
    }
    for profile in &mut profiles {
        profile.headers.extend(
            job.headers
                .iter()
                .map(|(key, value)| (key.to_ascii_lowercase(), value.clone())),
        );
        for (name, value) in [
            ("if-none-match", &job.etag),
            ("if-modified-since", &job.last_modified),
        ] {
            if magvpn {
                profile.headers.remove(name);
            } else if !value.trim().is_empty() {
                profile.headers.insert(name.into(), value.trim().into());
            }
        }
    }
    profiles
}

async fn request(
    job: &SubscriptionJob,
    url: &str,
    profile: &ClientProfile,
) -> Result<SubscriptionHttpPayload, SubscriptionFetcherError> {
    fetch_subscription_http(
        url,
        &profile.headers,
        20.0,
        MAX_IMPORT_BYTES,
        job.use_proxy_tun,
        &job.proxy_url,
        false,
        None,
    )
    .await
}

async fn fetch_happ(
    requester: &impl Requester,
    job: &SubscriptionJob,
    url: &str,
    hwid: &str,
) -> Result<Candidate, SubscriptionFetcherError> {
    let profile = happ_windows_profile(hwid);
    let payload = requester.request(job, url, &profile).await?;
    parse_candidate(payload, &profile.name)
        .await
        .map_err(SubscriptionFetcherError::Runtime)
}

async fn parse_candidate(
    payload: SubscriptionHttpPayload,
    profile: &str,
) -> Result<Candidate, String> {
    let profile = profile.to_owned();
    tokio::task::spawn_blocking(move || Candidate::parse(payload, &profile))
        .await
        .map_err(|error| error.to_string())
}

fn resolve_auto(mut original: Candidate, happ: Candidate) -> (SubscriptionResponse, Vec<String>) {
    let resolved = happ
        .nodes
        .iter()
        .filter(|node| node.scheme == "auto")
        .map(|node| node.name.as_str())
        .collect();
    if !original.auto_placeholders().is_subset(&resolved)
        || !happ.auto_placeholders().is_empty()
        || !happ.accepted()
    {
        original.response.text.clear();
        return (
            original.response,
            vec!["Провайдер не вернул полные конфигурации автовыбора для профиля Happ".into()],
        );
    }
    let mut userinfo = original.response.userinfo;
    merge_subscription_info(&mut userinfo, happ.response.userinfo);
    // The validators identify the URI representation, not the resolved graphs.
    (
        SubscriptionResponse {
            text: happ.response.text,
            userinfo,
            ..Default::default()
        },
        happ.errors,
    )
}

fn merge_magvpn(links: &str, happ: Candidate) -> Result<String, String> {
    let Value::Array(graphs) =
        serde_json::from_str(&happ.response.text).map_err(|error| error.to_string())?
    else {
        return Err("Happ-вариант не содержит конфигураций AUTO".into());
    };
    if graphs.is_empty() {
        return Err("Happ-вариант не содержит конфигураций AUTO".into());
    }
    if !happ.nodes.iter().any(|node| node.scheme == "auto") {
        return Err("Happ-вариант не содержит групп AUTO".into());
    }
    let merged = Value::Array(std::iter::once(json!(links)).chain(graphs).collect());
    let merged = pyjson::dumps(&merged, false, None);
    if merged.len() > MAX_IMPORT_BYTES {
        return Err("Объединённая подписка превышает 8 МиБ".into());
    }
    Ok(merged)
}

fn collapse_attempts(attempts: Vec<String>, profiles: &[ClientProfile]) -> Vec<String> {
    let mut reasons = Vec::new();
    for attempt in &attempts {
        let reason = profiles
            .iter()
            .find_map(|profile| {
                let rest = attempt.strip_prefix(&profile.name)?;
                rest.strip_prefix(" direct: ")
                    .or_else(|| rest.strip_prefix(": "))
            })
            .unwrap_or(attempt);
        push_unique(&mut reasons, reason.to_owned());
    }
    let merged = merge_subscription_reasons(&reasons);
    if merged.len() < reasons.len() {
        return merged;
    }
    if reasons.is_empty() {
        vec![FETCH_FAILED.into()]
    } else if reasons.len() < attempts.len() || reasons.len() == 1 {
        reasons
    } else {
        attempts
    }
}

pub(crate) fn merge_subscription_reasons(reasons: &[String]) -> Vec<String> {
    let mut notices = Vec::new();
    let mut others = Vec::new();
    for reason in reasons {
        let reason = reason.trim();
        if reason.is_empty() {
            continue;
        }
        if let Some(notice) = reason.strip_prefix(PROVIDER_SAYS) {
            for captures in NOTICE.captures_iter(notice) {
                let item = captures[1].trim();
                if !item.is_empty() {
                    push_unique(&mut notices, item.to_owned());
                }
            }
        } else {
            push_unique(&mut others, reason.to_owned());
        }
    }
    if !notices.is_empty() {
        others.retain(|reason| {
            !matches!(reason.as_str(), NO_SERVERS | FETCH_FAILED)
                && !reason.contains("служебное сообщение")
                && !reason.contains("заглушк")
        });
        others.insert(0, format_notices(&notices, 4));
    }
    others
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn format_notices(notices: &[String], limit: usize) -> String {
    let labels = notices
        .iter()
        .take(limit)
        .map(|label| format!("«{label}»"))
        .collect::<Vec<_>>();
    format!("{PROVIDER_SAYS}{}", labels.join(", "))
}

fn http_status(error: &SubscriptionFetcherError) -> Option<u16> {
    if let SubscriptionFetcherError::Http { status, .. } = error {
        Some(*status)
    } else {
        None
    }
}

fn tls_eof(message: &str) -> bool {
    includes(
        message,
        &[
            "tls/ssl connection has been closed",
            "_ssl.c:1010",
            "unexpected eof",
            "eof occurred in violation of protocol",
        ],
    )
}

fn includes(message: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| message.contains(marker))
}

fn friendly_error(error: &SubscriptionFetcherError, use_proxy_tun: bool) -> Option<String> {
    let low = default_case_fold_str(&error.to_string());
    let hint = if use_proxy_tun {
        "Загрузка через прокси/TUN уже включена — проверьте подключение Lumen или временно отключите эту настройку."
    } else {
        "Попробуйте включить «Загружать подписки через прокси/TUN» в Настройки → Подписки."
    };
    if includes(
        &low,
        &[
            "getaddrinfo failed",
            "errno 11001",
            "os error 11001",
            "name or service not known",
            "nodename nor servname provided",
            "no address associated with hostname",
            "temporary failure in name resolution",
            "eai_again",
            "eai_noname",
            "dns error",
        ],
    ) {
        return Some(format!(
            "Не удалось определить адрес сервера подписки: ошибка DNS. Проверьте системный DNS. {hint}"
        ));
    }
    if includes(
        &low,
        &[
            "certificate verify failed",
            "cert_verify_failed",
            "invalid peer certificate",
            "unknownissuer",
        ],
    ) {
        return Some("Не удалось проверить TLS-сертификат сервера подписки. Проверьте дату и время Windows, а также правильность ссылки подписки.".into());
    }
    match http_status(error) {
        Some(401) => return Some("Сервер подписки отклонил авторизацию (HTTP 401). Проверьте ссылку, HWID и срок подписки.".into()),
        Some(403) => return Some(format!("Сервер подписки запретил доступ (HTTP 403). Проверьте ссылку и HWID. {hint}")),
        Some(404) => return Some("Подписка не найдена (HTTP 404). Возможно, ссылка устарела или была удалена.".into()),
        Some(429) => return Some("Сервер подписки временно ограничил частоту запросов (HTTP 429). Повторите позже.".into()),
        Some(status @ 500..=599) => return Some(format!("Сервер подписки временно недоступен (HTTP {status}). Повторите попытку позже.")),
        _ => {}
    }
    let message = if matches!(error, SubscriptionFetcherError::Timeout(_))
        || includes(
            &low,
            &["timed out", "timeout", "winerror 10060", "os error 10060"],
        ) {
        "Сервер подписки не ответил вовремя: превышено время ожидания."
    } else if includes(
        &low,
        &[
            "winerror 10054",
            "os error 10054",
            "connection reset",
            "forcibly closed",
            "принудительно разорвал",
            "connection aborted",
        ],
    ) {
        "Соединение с сервером подписки было принудительно разорвано."
    } else if includes(
        &low,
        &["winerror 10061", "os error 10061", "connection refused"],
    ) {
        "Сервер подписки отклонил соединение."
    } else if includes(
        &low,
        &[
            "winerror 10051",
            "winerror 10065",
            "os error 10051",
            "os error 10065",
            "network is unreachable",
            "no route to host",
            "physical internet interface",
            "физический интернет-интерфейс",
            "direct network",
        ],
    ) {
        "Не удалось построить прямой маршрут к серверу подписки."
    } else if tls_eof(&low) || includes(&low, &["ssl eof", "tls handshake", "wrong version number"])
    {
        "Защищённое соединение с сервером подписки было прервано во время TLS-обмена."
    } else if includes(&low, &["tunnel connection failed", "proxy error"]) {
        "Прокси/TUN не смог подключиться к серверу подписки."
    } else {
        return None;
    };
    Some(format!("{message} {hint}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMMY: &str = "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTp0ZXN0@max.ru:1234#АВТОВЫБОР";
    const GRAPH: &str = r#"[{
        "remarks": "АВТОВЫБОР",
        "routing": {"balancers": [{"tag": "pool", "selector": ["member-"]}]},
        "outbounds": [{
            "tag": "member-1", "protocol": "vless",
            "settings": {"vnext": [{"address": "node.example", "port": 443,
                "users": [{"id": "00000000-0000-0000-0000-000000000001"}]}]}
        }]
    }]"#;
    const PHYSICAL: &str =
        "vless://00000000-0000-0000-0000-000000000001@node.example:443?encryption=none#node";

    fn candidate(body: &str, profile: &str) -> Candidate {
        Candidate::parse(
            SubscriptionHttpPayload {
                body: body.as_bytes().into(),
                headers: [("etag".into(), "\"variant\"".into())].into(),
                status: 200,
                ..Default::default()
            },
            profile,
        )
    }

    #[test]
    fn happ_replaces_a_dummy_pool_only_with_the_matching_graph() {
        let original = candidate(DUMMY, "Lumen");
        assert_eq!(original.auto_placeholders(), BTreeSet::from(["АВТОВЫБОР"]));
        let (response, errors) = resolve_auto(original, candidate(GRAPH, "Happ Windows"));
        assert!(errors.is_empty());
        assert_eq!(response.text, GRAPH);
        assert_eq!(response.userinfo["clientProfile"], "Happ Windows");
        assert!(!response.headers.contains_key("etag"));
        let (rejected, errors) = resolve_auto(
            candidate(DUMMY, "Lumen"),
            candidate(PHYSICAL, "Happ Windows"),
        );
        assert!(rejected.text.is_empty());
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn magvpn_requires_both_representations_and_drops_conditional_validators() {
        let mut job = SubscriptionJob::new(
            "https://www.magvpn.ru/tsub/123".into(),
            super::super::SubscriptionJobKind::Update,
        );
        job.etag = "\"cached\"".into();
        let requests = profiles(&job, "device", true);
        assert_eq!(requests.len(), 1);
        assert!(!requests[0].headers.contains_key("if-none-match"));
        let merged = merge_magvpn(PHYSICAL, candidate(GRAPH, "Happ Windows")).unwrap();
        let (nodes, errors) = parse_links_text(&merged, false).unwrap();
        assert!(errors.is_empty());
        assert_eq!(
            nodes
                .iter()
                .map(|node| node.scheme.as_str())
                .collect::<Vec<_>>(),
            ["vless", "auto"]
        );
        assert!(merge_magvpn(PHYSICAL, candidate("[]", "Happ Windows")).is_err());
    }

    // ── fetch flow with scripted responses (Python tests monkeypatch
    // `_fetch_subscription_with_headers`) ───────────────────────────────────

    const REAL: &str =
        "vless://00000000-0000-0000-0000-000000000001@one.example:443?encryption=none&type=tcp&security=none#one";

    type Reply = Result<String, SubscriptionFetcherError>;

    /// Records `(url, profile, headers)` per request and answers with
    /// `reply(profile, call_index)`.
    struct Scripted<F> {
        calls: std::sync::Mutex<Vec<(String, String, std::collections::BTreeMap<String, String>)>>,
        reply: F,
    }

    impl<F: Fn(&str, usize) -> Reply + Sync> Requester for Scripted<F> {
        fn request<'a>(
            &'a self,
            _job: &'a SubscriptionJob,
            url: &'a str,
            profile: &'a ClientProfile,
        ) -> RequestFuture<'a> {
            let index = {
                let mut calls = self.calls.lock().unwrap();
                calls.push((url.to_owned(), profile.name.clone(), profile.headers.clone()));
                calls.len() - 1
            };
            let reply = (self.reply)(&profile.name, index);
            Box::pin(async move {
                reply.map(|body| SubscriptionHttpPayload {
                    body: body.into_bytes(),
                    status: 200,
                    ..Default::default()
                })
            })
        }
    }

    fn scripted<F: Fn(&str, usize) -> Reply + Sync>(reply: F) -> Scripted<F> {
        Scripted { calls: Default::default(), reply }
    }

    fn profiles_called<F>(requester: &Scripted<F>) -> Vec<String> {
        requester.calls.lock().unwrap().iter().map(|call| call.1.clone()).collect()
    }

    fn job(url: &str) -> SubscriptionJob {
        let mut job = SubscriptionJob::new(url.into(), super::super::SubscriptionJobKind::Update);
        job.use_real_hwid = false;
        job
    }

    fn unavailable(message: &str) -> Reply {
        Err(SubscriptionFetcherError::Unavailable(message.into()))
    }

    #[tokio::test]
    async fn other_providers_do_not_receive_a_second_request() {
        let requester = scripted(|_, _| Ok(PHYSICAL.into()));
        let (response, errors) = fetch_with(&job("https://other.example/tsub/29332"), &requester).await.unwrap();
        assert!(!response.text.is_empty() && errors.is_empty());
        assert_eq!(profiles_called(&requester), ["Lumen"]);
    }

    #[tokio::test]
    async fn magvpn_secondary_failure_does_not_reconcile_partial_profile() {
        let requester = scripted(|profile, _| {
            if profile == "Happ Windows" { unavailable("HTTP 500") } else { Ok(PHYSICAL.into()) }
        });
        let (response, errors) = fetch_with(&job("https://www.magvpn.ru/tsub/29332"), &requester).await.unwrap();
        assert_eq!(response.text, "");
        assert!(errors[0].contains("MagVPN AUTO"), "{errors:?}");
    }

    #[tokio::test]
    async fn empty_body_advances_to_the_next_client_profile() {
        let requester = scripted(|profile, _| Ok(if profile == "Lumen" { String::new() } else { REAL.into() }));
        let (response, errors) = fetch_with(&job("https://sub.example/path"), &requester).await.unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(profiles_called(&requester), ["Lumen", "Happ Windows"]);
        assert_eq!(response.userinfo["clientProfile"], "Happ Windows");
    }

    #[tokio::test]
    async fn tls_eof_retries_the_same_profile() {
        let requester = scripted(|_, index| {
            if index == 0 {
                unavailable("<urlopen error TLS/SSL connection has been closed (EOF) (_ssl.c:1010)>")
            } else {
                Ok(REAL.into())
            }
        });
        let (response, errors) = fetch_with(&job("https://sub.example/path"), &requester).await.unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert!(response.text.contains("one.example"));
        assert_eq!(response.userinfo["clientProfile"], "Lumen");
        assert_eq!(response.userinfo["networkPath"], "direct");
        let calls = requester.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        for (_, profile, headers) in calls.iter() {
            assert_eq!(profile, "Lumen");
            assert_eq!(headers["user-agent"], format!("Lumen-Subscription/Windows-{}", lumen_core::constants::APP_VERSION));
            assert_eq!(headers["x-hwid"], lumen_core::models::DEFAULT_SUBSCRIPTION_HWID);
        }
    }

    #[tokio::test]
    async fn custom_user_agent_and_converter_come_first() {
        let requester = scripted(|_, _| Ok(REAL.into()));
        let mut job = job("https://sub.example/path?id=1");
        job.user_agent = "CustomClient/2.0".into();
        job.hwid = "custom-device-id".into();
        job.converter_url = "https://converter.example/sub?url={url}".into();
        let (response, errors) = fetch_with(&job, &requester).await.unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert!(response.text.contains("one.example"));
        assert_eq!(response.userinfo["clientProfile"], "Custom");
        let calls = requester.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "https://converter.example/sub?url=https%3A%2F%2Fsub.example%2Fpath%3Fid%3D1");
        assert_eq!(calls[0].1, "Custom");
        assert_eq!(calls[0].2["user-agent"], "CustomClient/2.0");
        assert_eq!(calls[0].2["x-hwid"], "custom-device-id");
    }

    #[tokio::test]
    async fn proxy_tun_mode_is_reported_as_the_network_path() {
        let requester = scripted(|_, _| Ok(REAL.into()));
        let mut job = job("https://sub.example/path");
        job.use_proxy_tun = true;
        job.proxy_url = "http://127.0.0.1:10809".into();
        let (response, errors) = fetch_with(&job, &requester).await.unwrap();
        assert!(errors.is_empty());
        assert_eq!(response.userinfo["networkPath"], "proxy-tun");
        assert_eq!(profiles_called(&requester), ["Lumen"]);
    }

    #[tokio::test]
    async fn dns_failure_is_explained_and_suggests_proxy_tun() {
        let requester = scripted(|_, _| unavailable("<urlopen error [Errno 11001] getaddrinfo failed>"));
        let (response, errors) = fetch_with(&job("https://blocked.example/sub"), &requester).await.unwrap();
        assert_eq!(response.text, "");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("ошибка DNS"));
        assert!(!errors[0].contains("getaddrinfo"));
        assert!(errors[0].contains("Загружать подписки через прокси/TUN"));
        assert!(errors[0].contains("Настройки → Подписки"));
    }

    #[tokio::test]
    async fn error_does_not_suggest_enabling_proxy_tun_twice() {
        let requester = scripted(|_, _| Err(SubscriptionFetcherError::Timeout("timed out".into())));
        let mut job = job("https://blocked.example/sub");
        job.use_proxy_tun = true;
        let (_, errors) = fetch_with(&job, &requester).await.unwrap();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("уже включена"));
        assert!(!errors[0].contains("Попробуйте включить"));
    }

    #[tokio::test]
    async fn hwid_with_newline_is_rejected_before_any_request() {
        let requester = scripted(|_, _| panic!("request must not start"));
        let mut job = job("https://sub.example/path");
        job.hwid = "device-id\r\nX-Injected: true".into();
        let Err(error) = fetch_with(&job, &requester).await else { panic!("expected an error") };
        assert_eq!(error, "HWID не должен содержать переносы строк");
    }

    #[tokio::test]
    async fn happ_crypt5_1_link_without_a_key_reports_a_friendly_error() {
        let requester = scripted(|_, _| panic!("a happ:// link is decrypted locally"));
        let link = "happ://crypt5/AABBCCDDEEFFGGHHIIJJKKLLMMNNOOPPQQRRSSTTUUVVWWXXYYZZ0011223344";
        let outcome = fetch_with(&job(link), &requester).await;
        let message = match outcome {
            Ok((response, errors)) => {
                assert_eq!(response.text, "");
                errors.first().cloned().unwrap_or_default()
            }
            Err(error) => error,
        };
        assert!(message.starts_with("Happ:"), "{message}");
        assert!(message.contains("crypt5.1"), "{message}");
    }

    #[test]
    fn network_errors_are_decoded_consistently() {
        let cases = [
            ("<urlopen error timed out>", "превышено время ожидания"),
            ("<urlopen error [WinError 10054] forcibly closed by remote host>", "принудительно разорвано"),
            ("<urlopen error [WinError 10061] connection refused>", "отклонил соединение"),
            ("<urlopen error [WinError 10051] network is unreachable>", "прямой маршрут"),
            ("<urlopen error TLS/SSL connection has been closed (EOF) (_ssl.c:1010)>", "TLS-обмена"),
        ];
        for (raw, expected) in cases {
            let message = friendly_error(&SubscriptionFetcherError::Unavailable(raw.into()), false).unwrap();
            assert!(message.contains(expected), "{raw}: {message}");
        }
        let not_found = SubscriptionFetcherError::Http {
            status: 404,
            reason: "Not Found".into(),
            headers: Default::default(),
        };
        assert!(friendly_error(&not_found, false).unwrap().contains("Подписка не найдена"));
    }

    #[test]
    fn client_stub_is_detected_in_english_and_russian() {
        let stub_names = [
            "Your%20client%20is%20not%20supported",
            "Please%20update%20your%20app",
            "%D0%92%D0%B0%D1%88%20%D0%BA%D0%BB%D0%B8%D0%B5%D0%BD%D1%82%20%D0%BD%D0%B5%20%D0%BF%D0%BE%D0%B4%D0%B4%D0%B5%D1%80%D0%B6%D0%B8%D0%B2%D0%B0%D0%B5%D1%82%D1%81%D1%8F",
            "%D0%9E%D0%B1%D0%BD%D0%BE%D0%B2%D0%B8%D1%82%D0%B5%20%D0%BF%D1%80%D0%B8%D0%BB%D0%BE%D0%B6%D0%B5%D0%BD%D0%B8%D0%B5",
        ];
        for name in stub_names {
            let body = format!(
                "vless://00000000-0000-0000-0000-000000000002@stub.example:443?encryption=none&type=tcp&security=none#{name}"
            );
            let stub = candidate(&body, "Lumen");
            assert!(stub.errors.is_empty());
            // The stub itself looks like a valid server; only its text gives it away.
            assert!(stub.usable);
            assert!(stub.is_client_stub(), "{name}");
            assert!(!stub.accepted());
        }
        let real = candidate(REAL, "Lumen");
        assert!(!real.is_client_stub());
        assert!(real.accepted());
    }
}
