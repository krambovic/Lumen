//! Master password, unlock/lock and the idle auto-lock timer
//! (`AppController.set_master_password` ... `_check_auto_lock`).

use std::time::Instant;

use lumen_core::security::{create_password_hash, verify_password};

use crate::app_controller::{AppController, AppEvent};

impl AppController {
    /// Python `set_master_password`.
    pub fn set_master_password(&mut self, password: &str) {
        let (password_hash, salt) = match create_password_hash(password, None) {
            Ok((hash, salt)) => (hash, salt),
            Err(error) => {
                self.emit_status("error", &format!("Не удалось задать пароль: {error}"));
                return;
            }
        };
        self.state.security.enabled = true;
        self.state.security.password_hash = password_hash;
        self.state.security.salt = salt;
        self.save();
    }

    /// Python `disable_master_password`.
    pub fn disable_master_password(&mut self) {
        self.state.security.enabled = false;
        self.state.security.password_hash = String::new();
        self.state.security.salt = String::new();
        self.locked = false;
        self.emit(AppEvent::LockStateChanged(false));
        self.save();
    }

    /// Python `unlock`.
    pub fn unlock(&mut self, password: &str) -> bool {
        if !self.state.security.enabled {
            self.locked = false;
            self.emit(AppEvent::LockStateChanged(false));
            return true;
        }
        let ok = verify_password(
            password,
            &self.state.security.password_hash,
            &self.state.security.salt,
        );
        if ok {
            self.locked = false;
            self.emit(AppEvent::LockStateChanged(false));
        }
        ok
    }

    /// Python `lock`.
    pub fn lock(&mut self) {
        if !self.state.security.enabled {
            return;
        }
        self.locked = true;
        self.emit(AppEvent::LockStateChanged(true));
        self.runtime.desired_connected = false;
        self.transitions.resume_reconnect_pending = false;
        self.request_transition("app locked");
    }

    /// Python `_check_auto_lock` (15 s timer started with deferred services).
    pub(crate) fn check_auto_lock(&mut self) {
        if !self.state.security.enabled {
            return;
        }
        if self.locked {
            return;
        }
        let minutes = self.state.security.auto_lock_minutes.max(1);
        if lumen_platform::idle::idle_seconds() >= (minutes as u32).saturating_mul(60) {
            self.lock();
        }
        let _ = Instant::now();
    }
}
