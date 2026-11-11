//! Coordinates Xray update workers with the connection transition loop.

use lumen_core::models::AppSettings;
use lumen_engines::xray::core_updater::{
    XrayCoreUpdateEvent, XrayCoreUpdateOptions, XrayCoreUpdateResult, XrayCoreUpdateStatus,
    XrayCoreUpdateWorker,
};
use tokio::{sync::oneshot, task::JoinError};

#[derive(Clone, Copy, Default)]
pub struct XrayUpdateConnection {
    pub connected: bool,
    /// Any connection transition is pending, scheduled, or active.
    pub transition_in_progress: bool,
    pub shutting_down: bool,
}

#[derive(Clone, Copy, Default)]
pub struct XrayUpdateRequest {
    pub apply_update: bool,
    pub silent: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum XrayUpdateEvent {
    Progress(u32),
    Result(XrayCoreUpdateResult),
    Status {
        level: &'static str,
        message: String,
    },
    Log(String),
    TransitionRequested {
        desired_connected: bool,
        reason: &'static str,
    },
}

impl XrayUpdateEvent {
    fn status(level: &'static str, message: impl Into<String>) -> Self {
        Self::Status {
            level,
            message: message.into(),
        }
    }
}

#[derive(Default)]
pub struct XrayUpdateService {
    worker: Option<XrayCoreUpdateWorker>,
    pending_disconnect: Option<oneshot::Sender<bool>>,
    apply_after_check: bool,
    reconnect_after_install: bool,
    silent: bool,
    proxy_url: Option<String>,
}

impl XrayUpdateService {
    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    pub fn run_xray_core_update(
        &mut self,
        settings: &AppSettings,
        connection: XrayUpdateConnection,
        request: XrayUpdateRequest,
        effective_proxy_url: Option<String>,
        mut emit: impl FnMut(XrayUpdateEvent),
    ) -> bool {
        let Some(options) = self.prepare_start(
            settings,
            connection,
            request,
            effective_proxy_url,
            &mut emit,
        ) else {
            return false;
        };
        self.worker = Some(XrayCoreUpdateWorker::start(options));
        if !request.silent {
            let message = if request.apply_update {
                "Обновление Xray..."
            } else {
                "Проверка обновлений Xray..."
            };
            emit(XrayUpdateEvent::status("info", message));
        }
        true
    }

    /// The receive is cancellation-safe for use in the controller's event loop.
    /// Dispatch each returned event with the current connection state before polling again.
    pub async fn next_worker_event(&mut self) -> Option<XrayCoreUpdateEvent> {
        let event = self.worker.as_mut()?.events.recv().await;
        if event.is_none() {
            *self = Self::default();
        }
        event
    }

    pub fn handle_worker_event(
        &mut self,
        event: XrayCoreUpdateEvent,
        settings: &AppSettings,
        connection: XrayUpdateConnection,
        mut emit: impl FnMut(XrayUpdateEvent),
    ) {
        match event {
            XrayCoreUpdateEvent::Progress(percent) => emit(XrayUpdateEvent::Progress(percent)),
            XrayCoreUpdateEvent::RequestDisconnect(confirm) => {
                self.request_disconnect(confirm, connection, &mut emit)
            }
            XrayCoreUpdateEvent::Done(result) => {
                self.worker = None;
                self.pending_disconnect = None;
                if let Some(options) = self.finish_update(settings, connection, result, &mut emit) {
                    self.worker = Some(XrayCoreUpdateWorker::start(options));
                }
            }
        }
    }

    /// Call after the connection transition changes state or finishes.
    pub fn resolve_pending_disconnect(
        &mut self,
        connection: XrayUpdateConnection,
        mut emit: impl FnMut(XrayUpdateEvent),
    ) {
        if !connection.shutting_down && connection.connected && connection.transition_in_progress {
            return;
        }
        let Some(confirm) = self.pending_disconnect.take() else {
            return;
        };
        let stopped = !connection.shutting_down && !connection.connected;
        if stopped {
            emit(XrayUpdateEvent::Log(
                "[updater] Disconnected successfully.".into(),
            ));
        } else {
            self.reconnect_after_install = false;
            emit(XrayUpdateEvent::Log(
                "[updater] Failed to disconnect.".into(),
            ));
        }
        let _ = confirm.send(stopped);
    }

    pub async fn cancel(&mut self) -> Result<(), JoinError> {
        let worker = self.worker.take();
        *self = Self::default();
        if let Some(worker) = worker {
            worker.cancel();
            worker.join().await?;
        }
        Ok(())
    }

    fn prepare_start(
        &mut self,
        settings: &AppSettings,
        connection: XrayUpdateConnection,
        request: XrayUpdateRequest,
        effective_proxy_url: Option<String>,
        mut emit: impl FnMut(XrayUpdateEvent),
    ) -> Option<XrayCoreUpdateOptions> {
        if connection.shutting_down {
            return None;
        }
        if self.is_running() {
            if !request.silent {
                emit(XrayUpdateEvent::status(
                    "info",
                    "Обновление Xray уже выполняется",
                ));
            }
            return None;
        }
        self.apply_after_check = request.apply_update && connection.connected;
        self.reconnect_after_install = false;
        self.silent = request.silent;
        self.proxy_url = if connection.connected {
            effective_proxy_url
        } else {
            None
        };
        Some(self.worker_options(settings, request.apply_update && !self.apply_after_check))
    }

    fn worker_options(&self, settings: &AppSettings, apply_update: bool) -> XrayCoreUpdateOptions {
        XrayCoreUpdateOptions {
            xray_path: settings.xray_path.clone(),
            channel: settings.xray_release_channel.clone(),
            feed_url: settings.xray_update_feed_url.clone(),
            apply_update,
            proxy_url: self.proxy_url.clone(),
        }
    }

    fn request_disconnect(
        &mut self,
        confirm: oneshot::Sender<bool>,
        connection: XrayUpdateConnection,
        emit: &mut impl FnMut(XrayUpdateEvent),
    ) {
        if connection.shutting_down {
            self.reconnect_after_install = false;
            let _ = confirm.send(false);
            return;
        }
        if !connection.connected {
            let _ = confirm.send(true);
            return;
        }
        if confirm.is_closed() {
            return;
        }
        emit(XrayUpdateEvent::Log(
            "[updater] Connection active. Disconnecting before replacing files...".into(),
        ));
        emit(XrayUpdateEvent::status(
            "info",
            "Остановка ядра для установки обновлений...",
        ));
        self.reconnect_after_install = true;
        self.pending_disconnect = Some(confirm);
        emit(XrayUpdateEvent::TransitionRequested {
            desired_connected: false,
            reason: "updater disconnect",
        });
    }

    fn finish_update(
        &mut self,
        settings: &AppSettings,
        connection: XrayUpdateConnection,
        result: XrayCoreUpdateResult,
        mut emit: impl FnMut(XrayUpdateEvent),
    ) -> Option<XrayCoreUpdateOptions> {
        use XrayCoreUpdateStatus::{Available, Error, UpToDate, Updated};

        let status = result.status;
        let message = result.message.clone();
        emit(XrayUpdateEvent::Result(result));
        if connection.shutting_down {
            *self = Self::default();
            return None;
        }
        let first_pass = std::mem::take(&mut self.apply_after_check);
        if first_pass && status == Available {
            emit(XrayUpdateEvent::Progress(0));
            if !self.silent {
                emit(XrayUpdateEvent::status("info", "Обновление Xray..."));
            }
            self.reconnect_after_install = false;
            return Some(self.worker_options(settings, true));
        }
        let level = match (first_pass, status) {
            (_, Error) => "error",
            (true, _) | (false, UpToDate) => "info",
            (false, Available) => "warning",
            (false, Updated) => "success",
        };
        if !self.silent {
            emit(XrayUpdateEvent::status(level, &message));
        }
        if self.silent || (!first_pass && status == Updated) {
            let prefix = if status == Error {
                "[core-update] error: "
            } else {
                "[core-update] "
            };
            emit(XrayUpdateEvent::Log(format!("{prefix}{message}")));
        }
        if !first_pass && self.reconnect_after_install {
            emit(XrayUpdateEvent::TransitionRequested {
                desired_connected: true,
                reason: "core update reconnect",
            });
        }
        self.reconnect_after_install = false;
        self.silent = false;
        self.proxy_url = None;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONNECTED: XrayUpdateConnection = XrayUpdateConnection {
        connected: true,
        transition_in_progress: false,
        shutting_down: false,
    };

    fn result(status: XrayCoreUpdateStatus, message: &str) -> XrayCoreUpdateResult {
        XrayCoreUpdateResult {
            status,
            message: message.into(),
            channel: "beta".into(),
            current_version: "26.7.11".into(),
            latest_version: "26.7.12".into(),
            updated: status == XrayCoreUpdateStatus::Updated,
        }
    }

    #[test]
    fn connected_silent_apply_checks_first_and_reuses_live_proxy() {
        let mut service = XrayUpdateService::default();
        let settings = AppSettings::default();
        let mut events = Vec::new();
        let options = service
            .prepare_start(
                &settings,
                CONNECTED,
                XrayUpdateRequest {
                    apply_update: true,
                    silent: true,
                },
                Some("http://127.0.0.1:10809".into()),
                |event| events.push(event),
            )
            .unwrap();
        assert!(!options.apply_update);
        assert_eq!(options.proxy_url.as_deref(), Some("http://127.0.0.1:10809"));
        let available = result(XrayCoreUpdateStatus::Available, "available");
        let install = service
            .finish_update(&settings, CONNECTED, available.clone(), |event| {
                events.push(event)
            })
            .unwrap();
        assert!(install.apply_update);
        assert_eq!(install.proxy_url, options.proxy_url);
        assert_eq!(
            events,
            [XrayUpdateEvent::Result(available), XrayUpdateEvent::Progress(0)]
        );
    }

    #[test]
    fn install_waits_for_disconnect_and_reconnects_after_the_result() {
        let mut service = XrayUpdateService::default();
        let settings = AppSettings::default();
        let mut events = Vec::new();
        let (ack, mut stopped) = oneshot::channel();
        service.handle_worker_event(
            XrayCoreUpdateEvent::RequestDisconnect(ack),
            &settings,
            CONNECTED,
            |event| events.push(event),
        );
        let busy = XrayUpdateConnection {
            transition_in_progress: true,
            ..CONNECTED
        };
        service.resolve_pending_disconnect(busy, |event| events.push(event));
        assert!(matches!(
            stopped.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        let disconnected = XrayUpdateConnection::default();
        service.resolve_pending_disconnect(disconnected, |event| events.push(event));
        assert!(stopped.try_recv().unwrap());
        let updated = result(XrayCoreUpdateStatus::Updated, "installed");
        service.handle_worker_event(
            XrayCoreUpdateEvent::Done(updated.clone()),
            &settings,
            disconnected,
            |event| events.push(event),
        );
        assert_eq!(
            events,
            [
                XrayUpdateEvent::Log(
                    "[updater] Connection active. Disconnecting before replacing files...".into()
                ),
                XrayUpdateEvent::Status {
                    level: "info",
                    message: "Остановка ядра для установки обновлений...".into()
                },
                XrayUpdateEvent::TransitionRequested {
                    desired_connected: false,
                    reason: "updater disconnect"
                },
                XrayUpdateEvent::Log("[updater] Disconnected successfully.".into()),
                XrayUpdateEvent::Result(updated),
                XrayUpdateEvent::Status {
                    level: "success",
                    message: "installed".into()
                },
                XrayUpdateEvent::Log("[core-update] installed".into()),
                XrayUpdateEvent::TransitionRequested {
                    desired_connected: true,
                    reason: "core update reconnect"
                },
            ]
        );
    }

    #[test]
    fn failed_disconnect_rejects_install_and_does_not_request_reconnect() {
        let mut service = XrayUpdateService::default();
        let settings = AppSettings::default();
        let mut events = Vec::new();
        let (ack, mut stopped) = oneshot::channel();
        service.handle_worker_event(
            XrayCoreUpdateEvent::RequestDisconnect(ack),
            &settings,
            CONNECTED,
            |event| events.push(event),
        );
        service.resolve_pending_disconnect(CONNECTED, |event| events.push(event));
        assert!(!stopped.try_recv().unwrap());
        events.clear();
        let error = result(XrayCoreUpdateStatus::Error, "still connected");
        service.handle_worker_event(
            XrayCoreUpdateEvent::Done(error.clone()),
            &settings,
            CONNECTED,
            |event| events.push(event),
        );
        assert_eq!(
            events,
            [
                XrayUpdateEvent::Result(error),
                XrayUpdateEvent::status("error", "still connected")
            ]
        );
    }

    #[test]
    fn shutdown_rejects_disconnect_and_suppresses_completion_actions() {
        let mut service = XrayUpdateService::default();
        let settings = AppSettings::default();
        let shutting_down = XrayUpdateConnection {
            shutting_down: true,
            ..CONNECTED
        };
        let mut events = Vec::new();
        let (ack, mut stopped) = oneshot::channel();
        service.handle_worker_event(
            XrayCoreUpdateEvent::RequestDisconnect(ack),
            &settings,
            shutting_down,
            |event| events.push(event),
        );
        assert!(!stopped.try_recv().unwrap());
        let error = result(XrayCoreUpdateStatus::Error, "stopping");
        service.handle_worker_event(
            XrayCoreUpdateEvent::Done(error.clone()),
            &settings,
            shutting_down,
            |event| events.push(event),
        );
        assert_eq!(events, [XrayUpdateEvent::Result(error)]);
    }

    #[tokio::test]
    async fn runs_existing_worker_and_blocks_duplicate_updates() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let settings = AppSettings {
            xray_path: directory
                .path()
                .join("missing.exe")
                .to_string_lossy()
                .into_owned(),
            // The feed is rejected by the worker before a network request is made.
            xray_update_feed_url: "http://example.test/releases.json".into(),
            ..AppSettings::default()
        };
        let mut service = XrayUpdateService::default();
        let connection = XrayUpdateConnection::default();
        let mut events = Vec::new();
        for _ in 0..2 {
            service.run_xray_core_update(
                &settings,
                connection,
                XrayUpdateRequest::default(),
                None,
                |event| events.push(event),
            );
        }
        assert!(service.is_running());
        assert_eq!(
            events,
            [
                XrayUpdateEvent::status("info", "Проверка обновлений Xray..."),
                XrayUpdateEvent::status("info", "Обновление Xray уже выполняется"),
            ]
        );
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            service.next_worker_event(),
        )
        .await?
        .expect("terminal worker event");
        service.handle_worker_event(event, &settings, connection, |event| events.push(event));
        assert!(!service.is_running());
        assert!(matches!(
            events[2],
            XrayUpdateEvent::Result(XrayCoreUpdateResult {
                status: XrayCoreUpdateStatus::Error,
                ..
            })
        ));
        assert!(matches!(
            events[3],
            XrayUpdateEvent::Status { level: "error", .. }
        ));
        Ok(())
    }
}
